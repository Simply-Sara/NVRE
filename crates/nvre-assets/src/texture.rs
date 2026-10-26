//! DDS files as GPU textures.
//!
//! A DDS file holds one or more mip levels of a surface. The
//! block-compressed formats are uploaded as they are — the graphics card
//! samples them itself — and the three uncompressed layouts are converted to
//! RGBA8.

use crate::data::GameData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, CompressedImageFormats, ImageAddressMode, ImageFilterMode,
    ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use nvre_dds::{to_rgba8, Dds, Format};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};

/// The loaded textures, by their path and whether they are sRGB. A `None`
/// remembers a failure, so a missing texture is not looked for twice.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: HashMap<(String, bool), Option<Handle<Image>>>,
    /// The copies sampled with clamped edges, by the same key.
    clamped: HashMap<(String, bool), Option<Handle<Image>>>,
    /// The textures that could not be loaded.
    pub failures: Vec<String>,
}

/// The anisotropic filtering of the game's textures, unless the renderer's
/// graphics settings say otherwise.
static ANISOTROPY: AtomicU16 = AtomicU16::new(8);

/// Set the anisotropic filtering — 1, 2, 4, 8 or 16 — of the textures made
/// from now on.
pub fn set_anisotropy(clamp: u16) {
    ANISOTROPY.store(clamp.clamp(1, 16), Ordering::Relaxed);
}

/// The sampler the game's textures are read with: repeated, linearly
/// filtered, and anisotropically filtered.
pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY.load(Ordering::Relaxed),
        ..ImageSamplerDescriptor::linear()
    })
}

/// Build a GPU image from DDS bytes.
///
/// `None` for a cube map, an unsupported format, or a block-compressed
/// texture the device cannot sample.
pub fn dds_image(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map".into());
    }
    let (w, h) = (dds.header.width, dds.header.height);
    let levels = &dds.levels[0];
    let (format, data) = match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            if !bc_supported {
                return Err("the device cannot sample block-compressed textures".into());
            }
            if w % 4 != 0 || h % 4 != 0 {
                return Err(format!(
                    "a {w}x{h} block-compressed texture is not a multiple of 4"
                ));
            }
            let f = match (dds.format, srgb) {
                (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
                (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
                (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
                (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
                (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (_, false) => TextureFormat::Bc3RgbaUnorm,
            };
            (f, levels.concat())
        }
        other => {
            // An uncompressed layout: every level, converted to RGBA8.
            let mut out = Vec::new();
            for l in levels {
                out.extend(to_rgba8(other, l).ok_or("an unsupported layout")?);
            }
            (
                if srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                },
                out,
            )
        }
    };
    let mut img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels.len() as u32;
    img.sampler = sampler();
    Ok(img)
}

impl TextureCache {
    /// The texture at `path`, loaded if it is not loaded yet.
    ///
    /// `srgb` decides whether the texture's colours are read as sRGB, and
    /// `clamp` whether its edges are clamped rather than repeated.
    pub fn get(
        &mut self,
        path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.get_inner(path, srgb, false, data, images, formats)
    }

    /// The texture at `path`, with its edges clamped.
    pub fn get_clamped(
        &mut self,
        path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.get_inner(path, srgb, true, data, images, formats)
    }

    /// The texture at `path`, from the right cache.
    fn get_inner(
        &mut self,
        path: &str,
        srgb: bool,
        clamp: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        let key = (path.to_string(), srgb);
        let cached = if clamp {
            self.clamped.get(&key)
        } else {
            self.map.get(&key)
        };
        if let Some(h) = cached {
            return h.clone();
        }
        // The device can sample block-compressed textures unless it says it
        // cannot.
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let handle = self.load(path, srgb, data, images, bc);
        if clamp {
            self.clamped.insert(key, handle.clone());
        } else {
            self.map.insert(key, handle.clone());
        }
        handle
    }

    /// Read a texture's file and turn it into an image.
    fn load(
        &mut self,
        path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        bc: bool,
    ) -> Option<Handle<Image>> {
        // The game's textures live under `textures\`, and the path may or
        // may not say so.
        let rel = nvre_vfs::texture_path(path);
        let Some(bytes) = data.read(&rel) else {
            self.failures.push(format!("{path}: not in any archive"));
            return None;
        };
        match dds_image(&bytes, srgb, bc) {
            Ok(img) => Some(images.add(img)),
            Err(e) => {
                self.failures.push(format!("{path}: {e}"));
                None
            }
        }
    }

    /// The textures that could not be loaded.
    pub fn failures(&self) -> &[String] {
        &self.failures
    }
}

/// Extract texture paths from a Fallout 4 .bgsm binary.
pub fn extract_bgsm_textures(bytes: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    if bytes.len() < 4 || &bytes[..4] != b"BGSM" {
        return (None, None, None);
    }
    let mut diffuse = None;
    let mut normal = None;
    let mut glow = None;

    let text = String::from_utf8_lossy(bytes);
    let lower_text = text.to_lowercase();
    let mut start = 0;
    while let Some(idx) = lower_text[start..].find(".dds") {
        let abs_idx = start + idx + 4;
        let mut p_start = start + idx;
        while p_start > start && bytes[p_start - 1] > 32 && bytes[p_start - 1] < 127 {
            p_start -= 1;
        }
        let path = &text[p_start..abs_idx];
        let path_lower = path.to_lowercase();
        if path_lower.contains("_n.dds") {
            normal = Some(path.replace('/', "\\"));
        } else if path_lower.contains("_g.dds") || path_lower.contains("_m.dds") {
            glow = Some(path.replace('/', "\\"));
        } else if diffuse.is_none() {
            diffuse = Some(path.replace('/', "\\"));
        }
        start = abs_idx;
    }
    (diffuse, normal, glow)
}
