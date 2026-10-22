// The NIF surface material: Bevy's standard PBR fragment, with the changes a
// NIF's shapes need.
//
// - the alpha is scaled by the view-angle falloff;
// - a blended shape that is also alpha-tested drops the fragments below its
//   cut;
// - the emissive light is modulated by the surface colour, as NIF emissive
//   light is;
// - ambient light is diffuse only: the specular part Bevy adds for its
//   ambient light is taken out again, since the game's ambient term is a
//   flat colour and the highlights come from the lights;
// - a multiply-blended shape's colour is a factor on what lies behind it:
//   summed in display values, held to one, and neither fogged nor
//   tonemapped;
// - an additive shape adds about what it would add to display values, so
//   its alpha is raised to a power.
//
// A mesh in a crossfade is dithered first, as Bevy's own PBR fragment does.
//
// SHADE_LIGHT is the shape whose sun shadows come from another light's
// shadow map.
//
// SOFT_EDGES is the alpha-tested shape in the main pass of a multisampled
// view: instead of the alpha test, the fragment's alpha is the share of the
// pixel's samples it covers.

import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::alpha_discard;
import bevy_pbr::render::pbr_functions::apply_pbr_lighting;
import bevy_pbr::render::pbr_functions::calculate_F0;
import bevy_pbr::render::pbr_functions::visibility_range_dither;
import bevy_pbr::render::pbr_lighting::{EnvBRDFApprox, F_AB};
import bevy_pbr::render::mesh_view_bindings::{lights, view};
import bevy_pbr::render::pbr_types::{
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS,
    STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
};
@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{FragmentOutput, VertexOutput};
import bevy_pbr::deferred::functions::deferred_output;
@else
import bevy_pbr::render::forward_io::{FragmentOutput, VertexOutput};

struct NifSurface {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop
    // opacity; (1, 0, 1, 1) when the shape has no falloff.
    falloff: vec4<f32>,
    // Fragments with a lower alpha are not drawn: the alpha test of a shape
    // that is drawn blended. Zero on every other shape.
    alpha_cut: f32,
    // One for an additive shape whose source factor is ONE, which adds its
    // colour whole; zero otherwise.
    add_whole: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> surface: NifSurface;

// The view-angle falloff: linear in the cosine, from the start opacity at
// the start angle to the stop opacity at the stop angle, held beyond either
// end.
fn falloff_opacity(cos_angle: f32, p: vec4<f32>) -> f32 {
    let start = p.x;
    let stop = p.y;
    let start_opacity = p.z;
    let stop_opacity = p.w;
    if abs(start - stop) < 1e-6 {
        return start_opacity;
    }
    let t = clamp((cos_angle - stop) / (start - stop), 0.0, 1.0);
    return mix(start_opacity, stop_opacity, t);
}

// The factor a multiply-blended shape puts on what is behind it: its lit
// colour and its emissive light, summed as display values and held to one.
fn multiply_factor(lit: vec3<f32>, emissive: vec3<f32>) -> vec3<f32> {
    let gamma = 2.2;
    let display = pow(max(lit, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma))
        + pow(max(emissive, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma));
    return pow(clamp(display, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(gamma));
}

// The alpha an additive shape adds with.
fn add_alpha(a: f32) -> f32 {
    return pow(max(a, 0.0), 1.5);
}

// The colour a shape is blended with.
//
// A multiply-blended shape's colour is a factor, not light: no fog and no
// tonemapping. Bevy's multiply blend is destination × colour + destination ×
// (1 − alpha), so the colour is premultiplied.
//
// An additive shape adds about what it would add to display values, so its
// alpha is raised to a power.
fn blend_out(color: vec4<f32>, multiply_emissive: vec3<f32>) -> vec4<f32> {
@if(BLEND_MULTIPLY) {
    let factor = multiply_factor(color.rgb, multiply_emissive);
    return vec4<f32>(factor * color.a, color.a);
}
@elif(BLEND_ADD) {
    return vec4<f32>(color.rgb, add_alpha(color.a));
}
@else {
    return color;
}
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
@if(VISIBILITY_RANGE_DITHER) {
    visibility_range_dither(in.position, in.visibility_range_dither);
}
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // The normal is already flipped towards the viewer on the back faces of
    // a double-sided shape, and the absolute value covers a single-sided
    // one seen edge on.
    let cos_angle = abs(dot(normalize(pbr_input.world_normal), pbr_input.V));
    let falloff = falloff_opacity(cos_angle, surface.falloff);
    pbr_input.material.base_color.a *= falloff;
    if pbr_input.material.base_color.a < surface.alpha_cut {
        discard;
    }

@if(SOFT_EDGES) {
    // The share of the pixel's samples the shape covers: one at the cutoff
    // and above, falling to zero within a pixel outside it.
    let edge_alpha = pbr_input.material.base_color.a;
    let edge = saturate((edge_alpha - pbr_input.material.alpha_cutoff) / max(fwidth(edge_alpha), 0.0001) + 1.0);
    if edge <= 0.0 {
        discard;
    }
    pbr_input.material.base_color.a = 1.0;
}
@else {
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
}

    // The base colour already holds the texture, the vertex colour and the
    // material's own colour.
    let e = pbr_input.material.emissive;
    pbr_input.material.emissive = vec4<f32>(e.rgb * pbr_input.material.base_color.rgb, e.a);

    // The emissive light of a multiply-blended shape, kept out of the
    // lighting sum: multiply_factor adds it in display values.
    let multiply_emissive = pbr_input.material.emissive.rgb;
@if(BLEND_MULTIPLY) {
    pbr_input.material.emissive = vec4<f32>(vec3<f32>(0.0), e.a);
}

@if(PREPASS_PIPELINE) {
    let out = deferred_output(in, pbr_input);
}
@else {
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        // Bevy's ambient specular, with the same inputs as in
        // apply_pbr_lighting: the specular part of its ambient light, taken
        // out again.
        let m = pbr_input.material;
        let F0 = calculate_F0(m.base_color.rgb, m.metallic, m.reflectance);
        let NdotV = max(dot(pbr_input.N, pbr_input.V), 0.0001);
        let spec = EnvBRDFApprox(F0, F_AB(m.perceptual_roughness, NdotV));
        let spec_occlusion = saturate(dot(F0, vec3<f32>(50.0 * 0.33)));
        let ambient_spec = spec * spec_occlusion * lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
        out.color = vec4<f32>(
            max(out.color.rgb - view.exposure * ambient_spec, vec3<f32>(0.0)),
            out.color.a,
        );
    } else {
        out.color = pbr_input.material.base_color;
    }

    // A multiply-blended shape is a factor on what lies behind it, and an
    // additive one adds about what it would add to display values. Both are
    // written in their own functions, so that the two conditionals do not
    // have to be nested.
    out.color = blend_out(out.color, multiply_emissive);
}

    return out;
}
