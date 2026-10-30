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
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 | Format::Bc4 | Format::Bc5 | Format::Bc7 => {
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
                (Format::Dxt5, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (Format::Dxt5, false) => TextureFormat::Bc3RgbaUnorm,
                (Format::Bc4, _) => TextureFormat::Bc4RUnorm,
                (Format::Bc5, _) => TextureFormat::Bc5RgUnorm,
                (Format::Bc7, true) => TextureFormat::Bc7RgbaUnormSrgb,
                (Format::Bc7, false) => TextureFormat::Bc7RgbaUnorm,
                _ => unreachable!(),
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
    let mut img = Image::new_uninit(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s5_4_all_mip_levels_present_and_count_matches() {
        // Build a synthetic 3-level 16x16 uncompressed RGBA8 DDS file
        // Level 0: 16x16 = 256 pixels = 1024 bytes
        // Level 1: 8x8 = 64 pixels = 256 bytes
        // Level 2: 4x4 = 16 pixels = 64 bytes
        // Total data = 1344 bytes
        let mut dds_bytes = vec![0u8; 128 + 1344];
        dds_bytes[0..4].copy_from_slice(b"DDS ");
        // Header size 124
        dds_bytes[4..8].copy_from_slice(&124u32.to_le_bytes());
        // Flags: CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT
        dds_bytes[8..12].copy_from_slice(&(0x1 | 0x2 | 0x4 | 0x1000 | 0x20000u32).to_le_bytes());
        // Height 16
        dds_bytes[12..16].copy_from_slice(&16u32.to_le_bytes());
        // Width 16
        dds_bytes[16..20].copy_from_slice(&16u32.to_le_bytes());
        // Pitch 64 (16 * 4)
        dds_bytes[20..24].copy_from_slice(&64u32.to_le_bytes());
        // Mip count 3
        dds_bytes[28..32].copy_from_slice(&3u32.to_le_bytes());
        // Pixel format size 32 at offset 76 (4 + 72)
        dds_bytes[76..80].copy_from_slice(&32u32.to_le_bytes());
        // DDPF_RGB | DDPF_ALPHAPIXELS = 0x40 | 0x1 = 0x41
        dds_bytes[80..84].copy_from_slice(&0x41u32.to_le_bytes());
        // RGB bit count 32
        dds_bytes[88..92].copy_from_slice(&32u32.to_le_bytes());
        // RGBA masks: DirectDraw A8R8G8B8 format has R in high bits, B in low bits
        dds_bytes[92..96].copy_from_slice(&0x00FF0000u32.to_le_bytes()); // R
        dds_bytes[96..100].copy_from_slice(&0x0000FF00u32.to_le_bytes()); // G
        dds_bytes[100..104].copy_from_slice(&0x000000FFu32.to_le_bytes()); // B
        dds_bytes[104..108].copy_from_slice(&0xFF000000u32.to_le_bytes()); // A

        let img = dds_image(&dds_bytes, true, true).expect("valid multi-mip DDS");
        assert_eq!(img.texture_descriptor.mip_level_count, 3);
        assert_eq!(img.data.as_ref().unwrap().len(), 1344);
    }

    #[test]
    fn s5_5_set_anisotropy_clamps_range() {
        set_anisotropy(16);
        assert_eq!(ANISOTROPY.load(Ordering::Relaxed), 16);
        set_anisotropy(32); // clamps to 16
        assert_eq!(ANISOTROPY.load(Ordering::Relaxed), 16);
        set_anisotropy(0); // clamps to 1
        assert_eq!(ANISOTROPY.load(Ordering::Relaxed), 1);
        set_anisotropy(8); // restore default
        assert_eq!(ANISOTROPY.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn s5_7_texture_load_failures_are_counted_and_reported() {
        let mut cache = TextureCache::default();
        assert_eq!(cache.failures().len(), 0);

        // Missing texture should record a failure entry rather than panic
        let dummy_data = GameData {
            vfs: std::sync::Arc::new(
                nvre_vfs::Vfs::open_default(std::path::Path::new(".")).unwrap_or_else(|_| {
                    nvre_vfs::Vfs::open(std::path::Path::new("."), &[]).unwrap()
                }),
            ),
            schema: std::sync::Arc::new(nvre_nif::Schema::default()),
        };
        let mut images = Assets::<Image>::default();
        let res = cache.get(
            "textures/nonexistent_dummy_tex.dds",
            true,
            &dummy_data,
            &mut images,
            None,
        );
        assert!(res.is_none());
        assert_eq!(cache.failures().len(), 1);
        assert!(cache.failures()[0].contains("nonexistent_dummy_tex.dds"));
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
