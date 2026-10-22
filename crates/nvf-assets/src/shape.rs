//! A shape: its geometry, and the material it is drawn with.
//!
//! A NIF's **shape** points at a **data block** that holds the geometry.
//! In the original edition the geometry is a list of vertices and a list of
//! triangles; in the Special Edition the shape holds its own geometry, in a
//! packed format where each vertex's parts sit at word offsets the vertex
//! description gives.

use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::material::AlphaMode;
use bevy::prelude::*;
use nvre_nif::scene::{mesh_data, Decoded, SceneObject};
use nvre_nif::value::Struct;
use std::collections::HashMap;

/// What a shape is drawn with.
#[derive(Debug, Clone, Default)]
pub struct ShapeProps {
    /// The shape's diffuse texture.
    pub texture: Option<String>,
    /// The shape's normal map.
    pub normal_texture: Option<String>,
    /// The shape's glow map.
    pub glow: Option<String>,
    /// The shape's own colour.
    pub alpha: Option<f32>,
    /// The shape's emissive light.
    pub emissive: Option<[f32; 3]>,
    /// How glossy the shape is.
    pub glossiness: Option<f32>,
    /// The shape is drawn without lighting.
    pub unlit: bool,
    /// The shape is drawn from both sides.
    pub double_sided: bool,
    /// How the shape is blended.
    pub alpha_mode: AlphaMode,
}

/// Read a shape's properties.
///
/// The lighting property holds the glossiness and the emissive light; the
/// shader property holds the texture set, which names the files.
pub fn shape_props(
    dec: &Decoded,
    o: &SceneObject,
    _by_block: &HashMap<usize, &SceneObject>,
) -> ShapeProps {
    let mut props = ShapeProps::default();
    let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
        return props;
    };
    for p in shape.refs("properties").unwrap_or_default() {
        let Some(prop) = dec.get(p) else { continue };
        // The lighting property is the one with a glossiness; the shader
        // property is the one with a texture set.
        if prop.get("glossiness").is_some() {
            props.glossiness = prop.num("glossiness").map(|g| g as f32);
            props.emissive = prop
                .list("emissive_color")
                .filter(|c| c.len() >= 3)
                .map(|c| {
                    [
                        c[0].as_number().unwrap_or(0.0) as f32,
                        c[1].as_number().unwrap_or(0.0) as f32,
                        c[2].as_number().unwrap_or(0.0) as f32,
                    ]
                });
            props.alpha = prop.num("alpha").map(|a| a as f32);
        } else if prop.get("texture_set").is_some() {
            if let Some(set) = prop.ref_at("texture_set") {
                let (diffuse, normal, glow) = texture_set(dec, set);
                props.texture = diffuse;
                props.normal_texture = normal;
                props.glow = glow;
            }
        }
    }
    props
}

/// The textures a texture set names.
///
/// The set holds a list of file names: the diffuse texture is the first,
/// the normal map the second, and the glow map the third. A name that is
/// empty is a slot that is not used.
fn texture_set(dec: &Decoded, set: i32) -> (Option<String>, Option<String>, Option<String>) {
    let Some(set) = dec.get(set) else {
        return (None, None, None);
    };
    let Some(textures) = set.list("textures") else {
        return (None, None, None);
    };
    let name = |i: usize| {
        textures
            .get(i)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    (name(0), name(1), name(2))
}

/// Build a mesh from a shape's data block.
///
/// `None` when the data holds no geometry.
pub fn build_mesh(data: &Struct) -> Option<(Mesh, usize)> {
    let md = mesh_data(data);
    if md.positions.is_empty() || md.triangles.is_empty() {
        return None;
    }
    let n = md.positions.len();
    let tris = md.triangles.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions);
    let has_normals = md.normals.len() == n;
    if has_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals);
    }
    if let Some(uv) = md.uv_sets.first().filter(|u| u.len() == n) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    }
    if md.colors.len() == n {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, md.colors);
    }
    // A triangle that names a vertex that is not there would draw a stray
    // triangle, so it is dropped.
    let max = n as u32;
    let idx: Vec<u32> = md
        .triangles
        .iter()
        .filter(|t| t.iter().all(|&i| i < max))
        .flatten()
        .copied()
        .collect();
    mesh.insert_indices(Indices::U32(idx));
    if !has_normals {
        mesh.compute_normals();
    }
    Some((mesh, tris))
}
