//! The material of a NIF shape.
//!
//! A NIF shape is drawn with Bevy's `StandardMaterial` and one change: the
//! emissive light is multiplied by the surface colour, the view-angle falloff
//! scales the alpha, a multiply-blended shape is a factor on what lies behind
//! it, and an additive shape adds about what it would add to display values.
//!
//! All four are in the fragment shader (`nif_material.wesl`), and what the
//! shader needs to know is in [`NifSurface`], the material's extension.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
    MaterialPlugin, MeshPipelineKey,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, DepthBiasState, DepthStencilState, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError,
};
use bevy::shader::{ShaderDefVal, ShaderRef};

/// `nif_prepass.wesl`.
///
/// The prepass fragment of a NIF shape: it dithers a crossfading mesh as the
/// main pass does, and it tests the vertex alpha as the main pass does.
///
/// A mesh that fades has its fragments discarded in a dither pattern, and
/// Bevy's depth-only prepass does not dither — so the pixels the main pass
/// discards would still hold depth, and hide what is behind them.
///
/// The main pass tests the material's alpha, the texture's alpha and the
/// vertex's alpha together. Bevy's depth-only prepass leaves the vertex alpha
/// out, so a shape that is cut out by its vertex alpha still holds depth in
/// the prepass, and hides what is behind it.
const NIF_PREPASS_SHADER: Handle<Shader> = uuid_handle!("b1d6a2f4-7c35-4e0a-8f29-3a5d9c0e6b17");

/// Anti-alias the cut-out edges of an alpha-tested material.
///
/// Where the view is multisampled, the edges of an alpha-tested shape are
/// anti-aliased. A material that opts out says its edges stay hard.
pub fn soft_edges(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::MAY_DISCARD) || descriptor.multisample.count <= 1 {
        return;
    }
    if descriptor
        .vertex
        .shader_defs
        .contains(&ShaderDefVal::from("PREPASS_PIPELINE"))
    {
        return;
    }
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push("SOFT_EDGES".into());
    }
}

/// What the shader needs to know about the shape it is drawing.
///
/// The default is the shape with no falloff, no alpha cut, no decal and no
/// additive trick: the fragment shader multiplies the alpha by
/// `falloff`, so a `Vec4::ZERO` here would hide every shape drawn with a
/// default extension. `Falloff::NONE` is (1, 0, 1, 1) — opaque at every
/// angle — and is the only sane default.
#[derive(Asset, AsBindGroup, Clone, Copy, Debug, Reflect)]
#[bind_group_data(NifSurfaceKey)]
#[uniform(100, NifSurfaceUniform)]
pub struct NifSurface {
    /// The start angle, the stop angle, the start opacity and the stop
    /// opacity. A shape with no falloff has an opacity of one at every
    /// angle.
    pub falloff: Vec4,
    /// Fragments whose alpha is below this are not drawn: the alpha test of a
    /// shape that is drawn blended. Zero on every other shape, which drops
    /// nothing.
    pub alpha_cut: f32,
    /// One for an additive shape whose source factor is `ONE`: its colour is
    /// added whole, whatever its alpha. Zero for every other shape, which
    /// adds its colour times its alpha.
    pub add_whole: f32,
    /// The shape lies on another surface, so it is drawn with a depth offset
    /// towards the camera.
    pub decal: bool,
    /// The shape's edges stay hard, rather than being anti-aliased.
    pub hard_edges: bool,
    /// The shape takes its sun shadows from another light's shadow map.
    pub shade_light: bool,
}

impl Default for NifSurface {
    fn default() -> NifSurface {
        NifSurface {
            falloff: Falloff::NONE.params,
            alpha_cut: 0.0,
            add_whole: 0.0,
            decal: false,
            hard_edges: false,
            shade_light: false,
        }
    }
}

/// The uniform the shader reads: what it needs to know about the shape it is
/// drawing.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct NifSurfaceUniform {
    /// The start angle, the stop angle, the start opacity and the stop
    /// opacity.
    pub falloff: Vec4,
    /// The alpha below which a fragment is dropped.
    pub alpha_cut: f32,
    /// Whether an additive shape's colour is added whole.
    pub add_whole: f32,
}

impl From<&NifSurface> for NifSurfaceUniform {
    fn from(surface: &NifSurface) -> NifSurfaceUniform {
        NifSurfaceUniform {
            falloff: surface.falloff,
            alpha_cut: surface.alpha_cut,
            add_whole: surface.add_whole,
        }
    }
}

/// The parts of a surface that decide which pipeline a shape is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NifSurfaceKey {
    /// The shape is a decal.
    pub decal: bool,
    /// The shape's edges are hard.
    pub hard_edges: bool,
    /// The shape takes its shadows from another light.
    pub shade_light: bool,
}

impl From<NifSurface> for NifSurfaceKey {
    fn from(surface: NifSurface) -> Self {
        NifSurfaceKey {
            decal: surface.decal,
            hard_edges: surface.hard_edges,
            shade_light: surface.shade_light,
        }
    }
}

impl From<&NifSurface> for NifSurfaceKey {
    fn from(surface: &NifSurface) -> Self {
        NifSurfaceKey::from(*surface)
    }
}

impl MaterialExtension for NifSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvre_assets/nif_material.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://nvre_assets/nif_material.wesl".into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        NIF_PREPASS_SHADER.into()
    }

    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<NifSurface>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let mesh_key = key.mesh_key;
        let key = key.bind_group_data;
        if key.decal {
            // A decal is drawn with a depth offset towards the camera, so it
            // does not fight the surface it lies on.
            // The bias lives inside the depth-stencil state in Bevy 0.20.
            descriptor.depth_stencil = Some(
                descriptor
                    .depth_stencil
                    .clone()
                    .map(|d| DepthStencilState {
                        bias: DepthBiasState {
                            constant: d.bias.constant + DECAL_BIAS,
                            slope_scale: d.bias.slope_scale + DECAL_SLOPE_BIAS,
                            ..d.bias
                        },
                        ..d
                    })
                    .unwrap_or(DepthStencilState {
                        bias: DepthBiasState {
                            constant: DECAL_BIAS,
                            slope_scale: DECAL_SLOPE_BIAS,
                            ..default()
                        },
                        format: bevy::render::render_resource::TextureFormat::Depth32Float,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(
                            bevy::render::render_resource::CompareFunction::GreaterEqual,
                        ),
                        stencil: Default::default(),
                    }),
            );
        }
        if !key.hard_edges {
            soft_edges(descriptor, mesh_key);
        }
        if key.shade_light
            && !descriptor
                .vertex
                .shader_defs
                .contains(&ShaderDefVal::from("PREPASS_PIPELINE"))
        {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("SHADE_LIGHT".into());
            }
        }
        Ok(())
    }
}

/// The bias a decal is drawn with, in depth-buffer units. It is small
/// enough not to disturb the geometry near it, and large enough to win
/// against the surface it lies on.
pub const DECAL_BIAS: i32 = 64;
/// The slope part of the decal's bias.
pub const DECAL_SLOPE_BIAS: f32 = 1.0;

/// The depth bias a decal is drawn with.
pub fn decal_bias(bias: DepthBiasState) -> DepthBiasState {
    DepthBiasState {
        constant: bias.constant + DECAL_BIAS,
        slope_scale: bias.slope_scale + DECAL_SLOPE_BIAS,
        ..bias
    }
}

/// A NIF material: Bevy's standard material, with the NIF's own changes.
pub type NifMaterial = ExtendedMaterial<StandardMaterial, NifSurface>;

/// A NIF material from its standard part.
pub fn nif_material(base: StandardMaterial) -> NifMaterial {
    NifMaterial {
        base,
        extension: NifSurface::default(),
    }
}

/// The view-angle falloff of a shape, as it is stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Falloff {
    /// The start angle, the stop angle, the start opacity and the stop
    /// opacity.
    pub params: Vec4,
}

impl Falloff {
    /// No falloff: the shape is as opaque at every angle.
    pub const NONE: Falloff = Falloff {
        params: Vec4::new(1.0, 0.0, 1.0, 1.0),
    };

    /// The falloff to draw for the stored fields, or `None` when it changes
    /// nothing or is degenerate.
    ///
    /// A start angle equal to the stop angle gives no range to interpolate
    /// over, and 166 shapes store all four fields as zero — which, read
    /// literally, would hide them entirely. A constant opacity of one is no
    /// falloff.
    pub fn from_fields(
        start_angle: f32,
        stop_angle: f32,
        start_opacity: f32,
        stop_opacity: f32,
    ) -> Option<Falloff> {
        let p = Vec4::new(start_angle, stop_angle, start_opacity, stop_opacity);
        if !p.is_finite()
            || (start_angle - stop_angle).abs() < 1e-4
            || (start_opacity == 1.0 && stop_opacity == 1.0)
        {
            return None;
        }
        Some(Falloff { params: p })
    }

    /// How opaque the shape is, seen at an angle.
    ///
    /// The angle is the cosine of the angle between the surface and the
    /// view, so one is straight on.
    pub fn opacity(&self, cos_angle: f32) -> f32 {
        let [start, stop, s_op, e_op] =
            [self.params.x, self.params.y, self.params.z, self.params.w];
        if (stop - start).abs() < 1e-6 {
            return s_op;
        }
        // The start angle's opacity at the start angle, the stop angle's at
        // the stop angle: `t` is the share of the way from start to stop.
        let t = ((cos_angle - start) / (stop - start)).clamp(0.0, 1.0);
        s_op + (e_op - s_op) * t
    }
}

/// The gamma the multiply blend works in.
pub const MULTIPLY_GAMMA: f32 = 2.2;

/// The factor a multiply-blended shape puts on what is behind it.
///
/// The shape's lit colour and its emissive light are summed as display
/// values and held to one, without fog or tonemapping, and the result is
/// raised back to the working space.
pub fn multiply_factor(lit: Vec3, emissive: Vec3) -> Vec3 {
    let display = lit.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA)
        + emissive.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA);
    display.clamp(Vec3::ZERO, Vec3::ONE).powf(MULTIPLY_GAMMA)
}

/// The emissive value a multiply-blended shape adds.
pub fn multiply_emissive(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    value.powf(MULTIPLY_GAMMA)
}

/// The power an additive shape's alpha is raised to.
pub const ADD_ALPHA_POWER: f32 = 1.5;

/// The alpha an additive shape adds with.
///
/// An additive shape adds about what it would add to display values, so its
/// alpha is raised to a power before it is used.
pub fn add_alpha(a: f32) -> f32 {
    a.max(0.0).powf(ADD_ALPHA_POWER)
}

/// The plugin that installs the material and its shaders.
pub struct NifMaterialPlugin;

impl Plugin for NifMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "nif_material.wesl");
        load_internal_asset!(
            app,
            NIF_PREPASS_SHADER,
            "nif_prepass.wesl",
            Shader::from_wesl
        );
        app.add_plugins(MaterialPlugin::<NifMaterial>::default())
            .add_systems(Update, see_through_casts_no_shadow);
    }
}

/// Whether a material is see-through: blended, and not cut out.
pub fn see_through(m: &NifMaterial) -> bool {
    matches!(
        m.base.alpha_mode,
        AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied
    ) && m.extension.alpha_cut <= 0.0
}

/// Marks a mesh the see-through rule took the shadow from.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SeeThroughNoShadow;

/// The meshes whose material changed, and whether they are see-through.
///
/// A see-through mesh carries `NotShadowCaster` and this crate's own marker,
/// so the system can find the ones that need the marker removed.
type SeeThroughMeshes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static MeshMaterial3d<NifMaterial>,
        Has<bevy::light::NotShadowCaster>,
        Has<SeeThroughNoShadow>,
    ),
    Changed<MeshMaterial3d<NifMaterial>>,
>;

/// Take the shadow from the meshes that are see-through.
fn see_through_casts_no_shadow(
    mut commands: Commands,
    materials: Res<Assets<NifMaterial>>,
    changed: SeeThroughMeshes,
) {
    for (e, m, no_shadow, ours) in &changed {
        let Some(m) = materials.get(&m.0) else {
            continue;
        };
        match (see_through(m), no_shadow, ours) {
            (true, false, _) => {
                commands
                    .entity(e)
                    .try_insert((bevy::light::NotShadowCaster, SeeThroughNoShadow));
            }
            (false, true, true) => {
                commands
                    .entity(e)
                    .try_remove::<(bevy::light::NotShadowCaster, SeeThroughNoShadow)>();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constant_opacity_is_no_falloff() {
        assert_eq!(
            Falloff::from_fields(0.0, 1.0, 1.0, 1.0),
            None,
            "a shape that is equally opaque at every angle has no falloff"
        );
    }

    #[test]
    fn an_empty_range_is_no_falloff() {
        assert_eq!(Falloff::from_fields(1.0, 1.0, 1.0, 0.0), None);
    }

    #[test]
    fn a_falloff_interpolates() {
        let f = Falloff::from_fields(1.0, 0.0, 1.0, 0.0).unwrap();
        // Straight on: fully opaque. Edge on: transparent.
        assert!((f.opacity(1.0) - 1.0).abs() < 1e-6);
        assert!((f.opacity(0.0) - 0.0).abs() < 1e-6);
        // Halfway: half.
        assert!((f.opacity(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_multiply_blend_can_only_darken() {
        assert_eq!(multiply_factor(Vec3::ONE, Vec3::ZERO), Vec3::ONE);
        // A half-lit surface stays a factor of a half. The comparison is
        // approximate because the factor goes through a pow round trip
        // (display value, then back), which is not exact in f32.
        let half = multiply_factor(Vec3::splat(0.5), Vec3::ZERO);
        assert!(
            (half - Vec3::splat(0.5)).abs().max_element() < 1e-5,
            "a half-lit multiply is a factor of a half, not {half:?}"
        );
        // Even a bright emissive cannot take it above one.
        let lit = multiply_factor(Vec3::splat(0.25), Vec3::splat(4.0));
        assert!(lit.max_element() <= 1.0);
    }

    #[test]
    fn an_additive_alpha_is_raised_to_a_power() {
        assert!((add_alpha(0.25) - 0.125).abs() < 1e-6);
        assert_eq!(add_alpha(1.0), 1.0);
        assert_eq!(add_alpha(0.0), 0.0);
    }
}
