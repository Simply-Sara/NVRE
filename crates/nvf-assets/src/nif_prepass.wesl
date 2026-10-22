// The depth-only prepass fragment of a NIF shape: the dither of a crossfade,
// then the alpha test on the alpha the main pass tests.
//
// Bevy's own prepass fragment tests the material's alpha and the texture's
// alpha and leaves the vertex alpha out, which the main pass multiplies in.
// A shape whose vertex alpha falls below its cutoff then held depth where
// the main pass drew nothing, and the clear colour showed through whatever
// lay behind it.

import bevy_pbr::prepass::io::VertexOutput;
import bevy_pbr::render::pbr_functions::visibility_range_dither;
import bevy_pbr::render::pbr_bindings;
import bevy_pbr::render::pbr_types;
import bevy_pbr::render::mesh_view_bindings::view;

struct NifSurface {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop
    // opacity; (1, 0, 1, 1) when the shape has no falloff.
    falloff: vec4<f32>,
    // Fragments with a lower alpha are not drawn: the alpha test of a shape
    // that is drawn blended. Zero on every other shape.
    alpha_cut: f32,
    // One for an additive shape whose source factor is ONE; zero otherwise.
    add_whole: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> surface: NifSurface;

// Bevy's cutoff for a blended shape in a depth-only pass.
const BLENDED_CUTOFF: f32 = 0.05;

@fragment
fn fragment(in: VertexOutput) {
@if(VISIBILITY_RANGE_DITHER) {
    visibility_range_dither(in.position, in.visibility_range_dither);
}
@if(MAY_DISCARD) {
    let flags = pbr_bindings::material.flags;
    var alpha = pbr_bindings::material.base_color.a;

@if(VERTEX_COLORS) {
    alpha *= in.color.a;
}

@if(VERTEX_UVS) {
    if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        let uv = (pbr_bindings::material.uv_transform * vec3<f32>(in.uv, 1.0)).xy;
        alpha *= textureSampleBias(
            pbr_bindings::base_color_texture,
            pbr_bindings::base_color_sampler,
            uv,
            view.mip_bias,
        ).a;
    }
}

    // The cut of a blended shape that is also alpha-tested; zero on every
    // other shape. The view-angle falloff of the main pass is left out,
    // because a shadow has no view angle.
    if alpha < surface.alpha_cut {
        discard;
    }

    let alpha_mode = flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK {
        if alpha < pbr_bindings::material.alpha_cutoff {
            discard;
        }
    } else if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND
        || alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD
        || alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ALPHA_TO_COVERAGE
    {
        if alpha < BLENDED_CUTOFF {
            discard;
        }
    } else {
        // A premultiplied or a multiply shape: as Bevy tests them.
        bevy_pbr::render::pbr_prepass_functions::prepass_alpha_discard(
            flags,
            pbr_bindings::material.alpha_cutoff,
            vec4<f32>(alpha),
        );
    }
}
}
