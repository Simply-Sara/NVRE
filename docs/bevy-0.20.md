# Moving to Bevy 0.20

The editor is on Bevy 0.19 and is going to 0.20. The asset layer is being
written fresh at the same time, so the two are done together — writing a
material against 0.19's shader internals and then porting it would be
throwing the work away.

## What 0.20 changes, and what it costs here

### The shaders are WESL now

Bevy's own shaders are written in [WESL](https://wesl-lang.dev), and the
`naga_oil` preprocessor is gone. A custom shader in the `naga_oil` dialect
has to be translated to WESL and renamed from `.wgsl` to `.wesl`; a plain
WGSL file with no preprocessor directives keeps working.

The old material's shader is not plain WGSL — it uses `#import`,
`#ifdef` and `#{MATERIAL_BIND_GROUP}` — so it has to be translated.

```wgsl
// 0.19
#import bevy_pbr::forward_io::VertexOutput
#import "shaders/util.wgsl"::hsv_to_rgb

#ifdef VERTEX_COLORS
var<private> tint: vec4<f32>;
#endif

@group(2) @binding(#{MATERIAL_BIND_GROUP}) var<uniform> color: vec4<f32>;

// 0.20
import bevy_pbr::render::forward_io::VertexOutput;
import super::util::hsv_to_rgb;

@if(VERTEX_COLORS)
var<private> tint: vec4<f32>;
@endif

@group(2) @binding(constants::MATERIAL_BIND_GROUP) var<uniform> color: vec4<f32>;
```

The rules that matter:

- imports come first in the file, before any declaration or `enable`;
- imports end with a semicolon;
- module names match the shader's path in its crate: `bevy_pbr::forward_io`
  is `bevy_pbr::render::forward_io`, and `bevy_pbr::mesh_view_bindings` is
  `bevy_pbr::render::mesh_view_bindings`.

### Order-independent transparency

OIT can be opted out of per material, through the new `enable_oit` method on
`Material` and `MaterialExtension`, with the shader define
`MATERIAL_OIT_ENABLED`. The old `OIT_ENABLED` is now per camera and per
view, so a custom shader gates its `oit_draw` behind `MATERIAL_OIT_ENABLED`
instead.

`oit_draw` now expects premultiplied colour, so it supports the
`Premultiplied` and `Add` alpha modes as well as `Blend`.

### Smaller things that touch the editor

- `MeshAabb::compute_aabb` is now `get_aabb`.
- `bevy_shape` split out of `bevy_math`; the primitives and the
  `Primitive3d` and `BoundingVolume` traits moved with it. The prelude still
  has them.
- `Tonemapping` and `DebandDither` moved from `bevy_core_pipeline` to
  `bevy_render::view`. The Rust imports still work through re-exports.
- `ScreenSpaceTransmission` is no longer required by `Camera3d`, so it is
  off by default. A camera that wants it adds the component.
- `WgpuWrapper` is gone.
- `UnpreparedBindGroup` is now `BindGroupBuilder`.
- `ShaderBuffer` stores typed data in an `AlignedVec` with an explicit size.

## What the editor touches

The editor's own Bevy use is small: the prelude, `Camera3d`,
`DirectionalLight`, `GlobalAmbientLight`, `Mesh3d`, `MeshMaterial3d`,
`Transform`, `Visibility`, `Name`, `Image`, `StandardMaterial`, `Handle`,
`Color`, `PrimitiveTopology`, `Mesh::ATTRIBUTE_*`, and `bevy_egui`.

`bevy_egui` is the one with a version lock:

| bevy | bevy_egui |
|------|-----------|
| 0.20 | 0.43 |
| 0.19 | 0.40–0.42 |

The editor is on 0.42, which is the last version that takes Bevy 0.19. Going
to 0.20 means going to `bevy_egui` 0.43 as well.

## The order

1. **The new asset layer, on Bevy 0.20.** The material and its shaders
   first, because the shader is the part that has to be written twice if the
   order is wrong. The mesh builder and the model cache after.
2. **The editor on Bevy 0.20.** The prelude changes, the egui version, the
   camera's transmission component, and whatever the compile turns up.
3. **The swap.** The editor's `nvre_assets` dependency moves from upstream's
   to the new crate, and the new crate is measured against the old one on
   the same meshes: the FNV control (67,172 meshes, 38.5 million vertices)
   and the Skyrim LE control (80,623 meshes, 35.6 million vertices).

## What is not affected

The record and NIF readers do not touch Bevy at all. `nvre-esm`,
`nvre-nif`, `nvre-util`, `nvre-inflate` and `fnv-dds` are independent of
this, and their measurements stand: every game's master at 98–99% explained,
every game's meshes parsed with no unknown blocks and no errors.
