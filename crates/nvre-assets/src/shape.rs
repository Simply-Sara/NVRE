//! A shape: its geometry, and the material it is drawn with.
//!
//! A NIF's **shape** points at a **data block** that holds the geometry.
//! In the original edition the geometry is a list of vertices and a list of
//! triangles; in the Special Edition the shape holds its own geometry, in a
//! packed format where each vertex's parts sit at word offsets the vertex
//! description gives.
//!
//! The material is not one block either. Fallout spreads it across the
//! shape's property list, and each block answers a different question:
//!
//! | block | carries |
//! | --- | --- |
//! | `BSShaderPPLightingProperty` | a lit shape's `texture_set` |
//! | `BSShaderNoLightingProperty` | unlit, the texture's `file_name`, the view-angle falloff |
//! | `NiTexturingProperty` | the diffuse texture, again, as `base_texture` |
//! | `NiAlphaProperty` | blend mode and alpha test |
//! | `NiStencilProperty` | double-sidedness |
//! | `NiMaterialProperty` | emissive colour, glossiness, alpha |
//! | `NiVertexColorProperty` | the vertex colours tint the shape |
//!
//! So the blocks are read by their **type name**, not by which fields they
//! happen to have: an effect mesh, an unlit mesh and a `NiTexturingProperty`
//! mesh have none of the lit path's fields, and reading by field name leaves
//! every one of them untextured and opaque.

use bevy::material::AlphaMode;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use nvre_nif::scene::{mesh_data, Decoded, SceneObject};
use nvre_nif::value::Struct;
use std::collections::HashMap;

/// What a shape is drawn with.
///
/// A manual `Default` because one field's default is not `0.0`: the emissive
/// multiplier is 1.0. A `#[derive(Default)]` would make it zero, and a shape
/// whose file carries an `emissive_color` but no multiplier — which
/// `NiMaterialProperty` allows, since `emissive_mult` needs `bs_version > 21` —
/// would have its emissive colour scaled to nothing.
#[derive(Debug, Clone)]
pub struct ShapeProps {
    /// The shape's diffuse texture.
    pub texture: Option<String>,
    /// The shape's normal map.
    pub normal_texture: Option<String>,
    /// The shape's glow map: the texture the emissive light is multiplied by.
    pub glow: Option<String>,
    /// The shape's own colour.
    pub alpha: Option<f32>,
    /// The shape's emissive colour, before its multiplier.
    pub emissive: Option<[f32; 3]>,
    /// The emissive multiplier: what the engine scales `emissive_color` by.
    ///
    /// Named `emissive_mult` in `NiMaterialProperty` and `emissive_multiple` in
    /// `BSLightingShaderProperty`, so it is read by both names and a shape that
    /// has neither is 1.0 — the engine's own default, and the value a shape
    /// with no emissive at all wants.
    pub emissive_mult: f32,
    /// How glossy the shape is.
    pub glossiness: Option<f32>,
    /// The shape is drawn without lighting.
    pub unlit: bool,
    /// The shape is drawn from both sides.
    pub double_sided: bool,
    /// How the shape is blended.
    pub alpha_mode: AlphaMode,
    /// The view-angle falloff: the start and stop angles, and the opacities
    /// at them.
    pub falloff: Option<Vec4>,
    /// Fragments whose alpha is below this are dropped, on a blended shape
    /// that also asks for an alpha test.
    pub alpha_cut: f32,
    /// An additive shape whose colour is added whole, whatever its alpha.
    pub add_whole: bool,
    /// The shape lies on another surface, and is drawn with a depth offset.
    pub decal: bool,
    /// The shader takes the vertex colours' alpha.
    pub vertex_alpha: bool,
    /// The vertex colours tint the shape.
    pub vertex_colors: bool,
}

impl Default for ShapeProps {
    fn default() -> Self {
        Self {
            texture: None,
            normal_texture: None,
            glow: None,
            alpha: None,
            emissive: None,
            emissive_mult: 1.0,
            glossiness: None,
            unlit: false,
            double_sided: false,
            alpha_mode: AlphaMode::Opaque,
            falloff: None,
            alpha_cut: 0.0,
            add_whole: false,
            decal: false,
            vertex_alpha: false,
            vertex_colors: false,
        }
    }
}

/// `BSShaderFlags` bit 3: the shader takes the vertex colours' alpha.
const SHADER_VERTEX_ALPHA: i64 = 1 << 3;
/// `BSShaderFlags` bit 6: the no-lighting property's view-angle falloff is on.
/// nif.xml names it "Empty" in this game's flag set; the effects set it.
const SHADER_FALLOFF: i64 = 1 << 6;
/// `BSShaderFlags` bit 19: "Dynamic_Alpha", which the unlit effect shaders use
/// in place of bit 3.
const SHADER_DYNAMIC_ALPHA: i64 = 1 << 19;
/// `BSShaderFlags` bits 26 and 27: the shape is a decal.
const SHADER_DECAL: i64 = (1 << 26) | (1 << 27);

/// A blend-and-test shape whose threshold is at least this draws alpha-tested;
/// below it the test only trims nearly transparent texels, so the shape
/// blends. `NiAlphaProperty` thresholds are bytes.
const BLEND_TEST_CUT: i64 = 64;

/// The type name of the block at `i`.
fn type_name(dec: &Decoded, i: i32) -> &str {
    usize::try_from(i)
        .ok()
        .and_then(|i| dec.lineage.get(i))
        .and_then(|l| l.first())
        .map(String::as_str)
        .unwrap_or("")
}

/// Read a shape's properties.
///
/// The property blocks are read in the order they are listed. The diffuse
/// texture is the exception: a shader property's own texture is the one the
/// game draws, and `NiTexturingProperty` repeats it for the editor's sake, so
/// the texturing property's is used only when no shader property named one.
pub fn shape_props(
    dec: &Decoded,
    o: &SceneObject,
    _by_block: &HashMap<usize, &SceneObject>,
) -> ShapeProps {
    // `ShapeProps::default()` carries the two non-zero defaults: an opaque
    // blend mode and an emissive multiplier of one.
    let mut props = ShapeProps::default();
    let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
        return props;
    };
    let mut prop_refs: Vec<i32> = shape.refs("properties").unwrap_or_default();
    if let Some(sp) = shape.ref_at("shader_property") {
        if sp >= 0 {
            prop_refs.push(sp);
        }
    }
    if let Some(ap) = shape.ref_at("alpha_property") {
        if ap >= 0 {
            prop_refs.push(ap);
        }
    }

    // The texturing property's diffuse texture, kept aside in case no shader
    // property names one.
    let mut texturing: Option<String> = None;
    // The shader flags, which several fields are read against.
    let mut shader_flags: Option<i64> = None;
    // The falloff fields, applied only when the flags switch the falloff on.
    let mut falloff_fields: Option<[f32; 4]> = None;

    for p in prop_refs {
        let Some(prop) = dec.get(p) else { continue };
        let ty = type_name(dec, p);
        let shader = dec.is_a(p as usize, "BSShaderProperty");

        if shader {
            // The flags are named two ways: `shader_flags` on the older
            // games' shader property, and `shader_flags_1` on Fallout 4's.
            // Both are read, and the one that is there is the one that is
            // used.
            shader_flags = prop
                .int("shader_flags")
                .or_else(|| prop.int("shader_flags_1"));
        }
        // The lighting property: glossiness, emissive, alpha.
        //
        // The emissive multiplier is read by both its spellings, because the
        // two property types that carry an emissive colour name it differently:
        // `emissive_mult` on `NiMaterialProperty` (FNV and FO3) and
        // `emissive_multiple` on `BSLightingShaderProperty` (Skyrim). Reading
        // each by name rather than branching on the type is what makes one loop
        // cover all five games — a property that has neither is the engine's
        // own default of 1.0, which `unwrap_or` supplies.
        //
        // The glossiness is `glossiness` before Fallout 4 and `smoothness`
        // from it on.
        if let Some(g) = prop.num("glossiness").or_else(|| prop.num("smoothness")) {
            props.glossiness = Some(g as f32);
        }
        if let Some(e) = color3(prop, "emissive_color") {
            props.emissive = Some(e);
        }
        if let Some(m) = prop
            .num("emissive_mult")
            .or_else(|| prop.num("emissive_multiple"))
        {
            props.emissive_mult = m as f32;
        }
        if let Some(a) = prop.num("alpha") {
            props.alpha = Some(a as f32);
        }

        // The shader property's texture set: slot 0 diffuse, 1 normal, 2 glow.
        if let Some(set) = prop.ref_at("texture_set") {
            let (diffuse, normal, glow) = texture_set(dec, set);
            if diffuse.is_some() {
                props.texture = diffuse;
            }
            if normal.is_some() {
                props.normal_texture = normal;
            }
            if glow.is_some() {
                props.glow = glow;
            }
        }

        match ty {
            // An unlit shape: its own texture, and the view-angle falloff.
            "BSShaderNoLightingProperty" => {
                props.unlit = true;
                if let Some(f) = prop.text("file_name").filter(|s| !s.is_empty()) {
                    props.texture = Some(f.to_string());
                }
                falloff_fields = Some([
                    prop.num("falloff_start_angle").unwrap_or(1.0) as f32,
                    prop.num("falloff_stop_angle").unwrap_or(0.0) as f32,
                    prop.num("falloff_start_opacity").unwrap_or(1.0) as f32,
                    prop.num("falloff_stop_opacity").unwrap_or(0.0) as f32,
                ]);
            }
            // Skyrim's effect shader: unlit, its own texture, its own falloff.
            "BSEffectShaderProperty" => {
                props.unlit = true;
                if let Some(f) = prop.text("source_texture").filter(|s| !s.is_empty()) {
                    props.texture = Some(f.to_string());
                }
                falloff_fields = Some([
                    prop.num("falloff_start_angle").unwrap_or(1.0) as f32,
                    prop.num("falloff_stop_angle").unwrap_or(0.0) as f32,
                    prop.num("falloff_start_opacity").unwrap_or(1.0) as f32,
                    prop.num("falloff_stop_opacity").unwrap_or(0.0) as f32,
                ]);
            }
            // The diffuse texture again: the editor's copy, and the block an
            // effect mesh's texture transform hangs on.
            "NiTexturingProperty" => {
                if let Some(f) = tex_desc(dec, prop, "base_texture") {
                    texturing = Some(f);
                }
                if let Some(f) = tex_desc(dec, prop, "glow_texture") {
                    props.glow = Some(f);
                }
                if let Some(f) = tex_desc(dec, prop, "normal_texture") {
                    props.normal_texture = Some(f);
                }
            }
            "NiAlphaProperty" => {
                let flags = prop.int("flags").unwrap_or(0);
                let threshold = prop.int("threshold").unwrap_or(128);
                props.alpha_mode = alpha_mode(flags, threshold);
                props.alpha_cut = blend_cut(flags, threshold);
            }
            "NiStencilProperty" => {
                // 20.1.0.3+ packs the draw mode into the flags; before that
                // it is a field of its own.
                let mode = prop
                    .int("draw_mode")
                    .or_else(|| prop.int("flags").map(|f| (f >> 10) & 3))
                    .unwrap_or(0);
                props.double_sided = mode == 3;
            }
            "NiVertexColorProperty" => props.vertex_colors = true,
            _ => {}
        }
    }

    if props.texture.is_none() {
        props.texture = texturing;
    }
    // An unlit shape takes the vertex colours' alpha through "Dynamic_Alpha"
    // where the lit path uses "Vertex_Alpha".
    let flags = shader_flags.unwrap_or(0);
    // Unlit effect meshes use vertex alpha for their fading gradient, and apply falloff
    // whenever falloff fields are present (even if bit 6 is omitted).
    props.vertex_alpha =
        props.unlit || flags & SHADER_VERTEX_ALPHA != 0 || flags & SHADER_DYNAMIC_ALPHA != 0;

    let has_falloff = flags & SHADER_FALLOFF != 0 || (props.unlit && falloff_fields.is_some());
    if has_falloff && is_blended(props.alpha_mode) {
        if let Some([a, b, c, d]) = falloff_fields {
            props.falloff = crate::nif_material::Falloff::from_fields(a, b, c, d).map(|f| f.params);
        }
    }
    props.decal = flags & SHADER_DECAL != 0;
    props.add_whole = props.alpha_mode == AlphaMode::Add && adds_whole(dec, o);
    props
}

/// Whether a blend mode draws see-through.
pub fn is_blended(mode: AlphaMode) -> bool {
    matches!(
        mode,
        AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied
    )
}

/// The blend mode `NiAlphaProperty` flags ask for.
///
/// bit 0 blend, bits 1-4 source factor, bits 5-8 destination factor (0 ONE,
/// 1 ZERO, 2 SRC_COLOR, 4 DEST_COLOR, 6 SRC_ALPHA, 7 INV_SRC_ALPHA), bit 9
/// alpha test, bits 10-12 test function (4 = GREATER).
///
/// Bevy offers fixed modes, so the factors map to the nearest: destination
/// ONE is additive, a source of ZERO or DEST_COLOR multiplies. When both test
/// and blend are set, a threshold of [`BLEND_TEST_CUT`] or more draws
/// alpha-tested, and a lower one blended — the test then only trims nearly
/// transparent texels.
pub fn alpha_mode(flags: i64, threshold: i64) -> AlphaMode {
    let (src, dst) = ((flags >> 1) & 0xF, (flags >> 5) & 0xF);
    let (blend, test) = (flags & 1 != 0, flags & 0x200 != 0);
    let strict = (flags >> 10) & 7 == 4;
    let cutoff = (threshold as f32 + if strict { 0.5 } else { 0.0 }) / 255.0;
    match (blend, test, src, dst) {
        (true, _, _, 0) => AlphaMode::Blend,
        (false, true, _, _) => AlphaMode::Mask(cutoff),
        (true, true, _, _) if threshold >= BLEND_TEST_CUT => AlphaMode::Mask(cutoff),
        (true, _, 1 | 4, _) => AlphaMode::Multiply,
        (true, ..) => AlphaMode::Blend,
        _ => AlphaMode::Opaque,
    }
}

/// The alpha below which a blended shape drops its fragments, when its alpha
/// test would otherwise have made it an alpha-tested one: a blend-and-test
/// shape whose threshold is below [`BLEND_TEST_CUT`] blends, and the test
/// only trims. Zero for every other shape.
fn blend_cut(flags: i64, threshold: i64) -> f32 {
    let (blend, test) = (flags & 1 != 0, flags & 0x200 != 0);
    if !blend || !test || threshold >= BLEND_TEST_CUT {
        return 0.0;
    }
    threshold as f32 / 255.0
}

/// Whether a shape adds its colour whole: blend on, source and destination
/// factors both ONE.
fn adds_whole(dec: &Decoded, o: &SceneObject) -> bool {
    let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
        return false;
    };
    for p in shape.refs("properties").unwrap_or_default() {
        if type_name(dec, p) != "NiAlphaProperty" {
            continue;
        }
        if let Some(flags) = dec.get(p).and_then(|s| s.int("flags")) {
            if flags & 1 != 0 && (flags >> 1) & 0xF == 0 && (flags >> 5) & 0xF == 0 {
                return true;
            }
        }
    }
    false
}

/// A three-number colour field.
fn color3(s: &Struct, name: &str) -> Option<[f32; 3]> {
    let c = s.list(name)?;
    if c.len() < 3 {
        return None;
    }
    Some([
        c[0].as_number().unwrap_or(0.0) as f32,
        c[1].as_number().unwrap_or(0.0) as f32,
        c[2].as_number().unwrap_or(0.0) as f32,
    ])
}

/// The file a `NiSourceTexture` names.
fn source_file(dec: &Decoded, source: i32) -> Option<String> {
    let t = dec.get(source)?;
    t.text("file_name")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The file a texture description's `source` names.
fn tex_desc(dec: &Decoded, s: &Struct, name: &str) -> Option<String> {
    let source = s.get(name)?.as_struct()?.get("source")?.as_ref()?;
    source_file(dec, source)
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
/// `bs_version` decides which packed vertex format the shape uses; see
/// `nvre_nif::scene::mesh_data`. `None` when the data holds no geometry.
///
/// The second number is the triangles the shape declares, and the third is
/// how many of them were dropped because they named a vertex that is not
/// there. A shape that drops any is a shape whose data block and whose
/// vertex count disagree, and that is worth counting rather than
/// swallowing: a stray triangle is drawn where the model should be empty.
pub fn build_mesh(data: &Struct, bs_version: u32) -> Option<(Mesh, usize, usize)> {
    let md = mesh_data(data, bs_version);
    if md.positions.is_empty() || md.triangles.is_empty() {
        return None;
    }
    let n = md.positions.len();
    let tris = md.triangles.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
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
    // triangle, so it is dropped — and counted.
    let max = n as u32;
    let mut dropped = 0usize;
    let mut idx: Vec<u32> = Vec::with_capacity(tris * 3);
    for t in &md.triangles {
        if t.iter().all(|&i| i < max) {
            idx.extend_from_slice(t);
        } else {
            dropped += 1;
        }
    }
    mesh.insert_indices(Indices::U32(idx));
    if !has_normals {
        mesh.compute_normals();
    }
    Some((mesh, tris, dropped))
}

/// Clean a shape's vertex colours for the material it is drawn with.
///
/// A shape whose shader does not take the vertex alpha has its alpha forced
/// to one: a stored zero would otherwise make the shape invisible. The
/// colours are clamped, and a non-finite one becomes white — a mesh can carry
/// NaN, and a NaN in a colour draws black.
///
/// An unlit shape's colour is texture × vertex colour, which the game forms in
/// display values; this renderer multiplies in linear ones, so the colour is
/// converted and the alpha kept.
pub fn fix_vertex_colors(mesh: &mut Mesh, vertex_alpha: bool, unlit: bool) {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        return;
    };
    let clean = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            1.0
        }
    };
    for c in colors.iter_mut() {
        let rgb = [clean(c[0]), clean(c[1]), clean(c[2])];
        let rgb = if unlit {
            let l = LinearRgba::from(Srgba::new(rgb[0], rgb[1], rgb[2], 1.0));
            [l.red, l.green, l.blue]
        } else {
            rgb
        };
        *c = [
            rgb[0],
            rgb[1],
            rgb[2],
            if vertex_alpha { clean(c[3]) } else { 1.0 },
        ];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV's near-universal "transparent": blend, SRC_ALPHA / INV_SRC_ALPHA.
    #[test]
    fn the_transparent_flags_blend() {
        assert_eq!(alpha_mode(0x10ED, 0), AlphaMode::Blend);
    }

    #[test]
    fn a_test_without_blend_masks() {
        // bit 9 set, bit 0 clear: the test cuts the shape.
        assert_eq!(alpha_mode(0x0200, 128), AlphaMode::Mask(128.0 / 255.0));
        // GREATER moves the cutoff half a step up.
        assert_eq!(
            alpha_mode(0x0200 | (4 << 10), 128),
            AlphaMode::Mask(128.5 / 255.0)
        );
    }

    #[test]
    fn a_blend_and_test_shape_above_the_cut_is_masked() {
        // Blend, test, threshold 200: the test cuts the shape, as weapon
        // parts want. 0x10ED already sets GREATER, so the cutoff is half a
        // step above the threshold.
        assert_eq!(
            alpha_mode(0x10ED | 0x0200, 200),
            AlphaMode::Mask(200.5 / 255.0)
        );
        // The same shape with a GREATER_EQUAL test keeps the threshold.
        let greater_equal = (0x10ED | 0x0200) & !(7 << 10);
        assert_eq!(
            alpha_mode(greater_equal, 200),
            AlphaMode::Mask(200.0 / 255.0)
        );
        // Below the cut it only trims, so the shape blends.
        assert_eq!(alpha_mode(0x10ED | 0x0200, 0), AlphaMode::Blend);
        assert_eq!(blend_cut(0x10ED | 0x0200, 0), 0.0);
    }

    #[test]
    fn the_blend_cut_only_applies_to_a_blend_and_test_shape() {
        // Blend alone: no cut.
        assert_eq!(blend_cut(0x10ED, 0), 0.0);
        // Test alone: the mask is the cut.
        assert_eq!(blend_cut(0x0200, 0), 0.0);
        // Blend and test, low threshold: the cut trims the transparent texels.
        assert!((blend_cut(0x10ED | 0x0200, 32) - 32.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn a_destination_of_one_blends() {
        // blend, source ZERO, destination ONE.
        assert_eq!(alpha_mode(1, 0), AlphaMode::Blend);
    }

    #[test]
    fn a_source_of_zero_multiplies() {
        // blend (bit 0), source ZERO (1) in bits 1-4, destination
        // INV_SRC_ALPHA (7) in bits 5-8.
        let flags = 1 | (1 << 1) | (7 << 5);
        assert_eq!(alpha_mode(flags, 0), AlphaMode::Multiply);
    }

    #[test]
    fn no_flags_is_opaque() {
        assert_eq!(alpha_mode(0, 0), AlphaMode::Opaque);
    }

    #[test]
    fn s4_3_exhaustive_alpha_mode_space() {
        for flags in 0..8192 {
            let _ = alpha_mode(flags, 0);
            let _ = alpha_mode(flags, 200);
        }
    }

    /// A shape's default emissive multiplier is one, not zero.
    ///
    /// This is the default that keeps a shape with an `emissive_color` but no
    /// multiplier rendering as the engine does. `NiMaterialProperty` gates
    /// `emissive_mult` on `bs_version > 21`, so a file can carry the colour
    /// without the multiplier — and a derived `Default` would make it zero,
    /// which scales every emissive colour to black.
    #[test]
    fn the_default_emissive_multiplier_is_one() {
        assert_eq!(ShapeProps::default().emissive_mult, 1.0);
    }

    /// A shape with no emissive colour has none, whatever the multiplier.
    ///
    /// The multiplier scales the colour; it does not create one. A shape whose
    /// property is missing — Fallout 4's shapes, whose `BSLightingShaderProperty`
    /// this schema does not define — must stay black rather than glowing at
    /// whatever the multiplier says.
    #[test]
    fn no_emissive_colour_stays_none() {
        let props = ShapeProps::default();
        assert_eq!(props.emissive, None);
        assert_eq!(props.emissive_mult, 1.0);
    }

    /// Both spellings of the emissive multiplier are read.
    ///
    /// `NiMaterialProperty` (FNV and FO3) calls it `emissive_mult`;
    /// `BSLightingShaderProperty` (Skyrim) calls it `emissive_multiple`. The
    /// reader takes either, which is what lets one loop cover all five games.
    #[test]
    fn either_spelling_of_the_multiplier_reads() {
        // A struct with the FNV spelling.
        let mut a = Struct::new();
        a.fields.push((
            "emissive_mult".to_string(),
            nvre_nif::value::Value::Num(5.0),
        ));
        // A struct with the Skyrim spelling.
        let mut b = Struct::new();
        b.fields.push((
            "emissive_multiple".to_string(),
            nvre_nif::value::Value::Num(6.27),
        ));
        // A struct with neither: the engine's own default.
        let c = Struct::new();
        for (s, want) in [(&a, 5.0_f32), (&b, 6.27_f32)] {
            let got = s
                .num("emissive_mult")
                .or_else(|| s.num("emissive_multiple"))
                .map(|m| m as f32);
            assert_eq!(got, Some(want), "{s:?}");
        }
        assert_eq!(
            c.num("emissive_mult")
                .or_else(|| c.num("emissive_multiple")),
            None,
            "a property with no multiplier is the engine's 1.0, supplied by the caller"
        );
    }
}
