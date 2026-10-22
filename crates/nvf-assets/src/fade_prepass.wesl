// The depth-only prepass fragment for a mesh in a crossfade: the dither of
// the main pass, then the alpha-mask discard Bevy's own depth-only prepass
// fragment does.

import bevy_pbr::prepass::io::VertexOutput;
import bevy_pbr::render::pbr_functions::visibility_range_dither;

@fragment
fn fragment(in: VertexOutput) {
@if(VISIBILITY_RANGE_DITHER) {
    visibility_range_dither(in.position, in.visibility_range_dither);
}
@if(MAY_DISCARD) {
    var output_color = vec4<f32>(1.0);
    bevy_pbr::render::pbr_prepass_functions::prepass_sample_color_and_alpha_discard(in);
}
}
