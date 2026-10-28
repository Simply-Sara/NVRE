# The stages

One section per stage. Each says what the stage decides, in engine terms; what
it consumes; what oracle judges it; and where in the code it lives. The
per-stage steps, one line each, are in [`steps/`](steps/).

The statuses below are the **measured** ones — what the code does today, not an
aspiration. Where the code is wrong, the wrongness is named in the step and
carried into [`matrix.json`](matrix.json).

---

## S0 — Archives, load order and file resolution

**Decides:** which file wins when a name appears in a BSA and as a loose file,
and which plugin's version of a record is the one that renders.

**Consumes:** the game's `SArchiveList`, the load order, the plugin headers'
`MAST`.

**Oracle:** O1 (re-encode), O4 (invariants), plus the load order the game
itself computes (`nvre-editor/src/plugin_order.rs`).

**Lives:** `crates/nvre-vfs`, `crates/nvre-editor/src/plugin_order.rs`.

**Status:** working for the Fallout family; BA2 reads Fallout 4's archives.

---

## S1 — NIF header and block decode

**Decides:** every field value the rest of the pipeline reads. A wrong layout
here silently shifts every later field, which is why this is the first rung.

**Consumes:** the file's header (version, `bs_version`), the string table, the
schema chain `base → family → version`.

**Oracle:** O1 — re-encode. This is the stage where O1 is strongest, because
the encoder is already written (`crates/nvre-nif/src/encode.rs`) and the
records prove it works.

**Lives:** `crates/nvre-nif/src/{codec,schema,file,encode}.rs`.

**Status:** the records are at 100% re-encode. **Meshes have never been
round-tripped.** That is S1-1, and it is the first thing to run.

---

## S2 — Scene graph

**Decides:** which node is the parent of which, and the local transform of
each. A parent assigned wrongly displaces a subtree; a transform read wrongly
scales or rotates it.

**Consumes:** `children` refs, each node's `Transform` (rotation, translation,
scale), the root.

**Oracle:** O4 (exactly one root per connected component; every shape reachable
from exactly one node) and O1 for the transform fields themselves.

**Lives:** `crates/nvre-nif/src/scene.rs:218` (`scene_objects`).

**Known shape of the problem:** parent = "the lowest-index node listing it as
a child", which is right for a well-formed file and wrong for a node listed by
two parents — the second listing is dropped silently.

---

## S3 — Geometry

**Decides:** the vertices, the triangles, and which streams are present. This
is where the packed-vertex formats live, and where a game's difference is
largest: Skyrim SE packs `BSVertexDataSSE`, Fallout 4 packs `BSVertexData`
with half-precision positions.

**Consumes:** the geometry-data block, or the shape itself for `BSTriShape`;
`bs_version` and the vertex description's word offsets and attribute flags.

**Oracle:** O4 (every index in range; a strip's triangle count; the vertex
count the shape declares against the bytes the data holds) and O3 (NVForever's
unpacker, read for the reasoning).

**Lives:** `crates/nvre-nif/src/scene.rs:498` (`mesh_data`, `unpack_sse`),
`crates/nvre-assets/src/shape.rs:416` (`build_mesh`).

---

## S4 — Material decode

**Decides:** what the shape is drawn *with* — texture, normal map, glow map,
blend mode, double-sidedness, unlit-ness, falloff, alpha cut, decal, vertex
alpha. The block-type dispatch lives here, and so does the class of bug this
project has hit most: a field the props struct carries that nothing assigns.

**Consumes:** the property blocks by **type name**, the shader flags, the
texture set's slots, `NiAlphaProperty`'s bitfield.

**Oracle:** O2 (fopdoc/nif.xml for the bit layouts) and O4 (the
consumed-field census: every field the schema declares, marked read or
ignored).

**Lives:** `crates/nvre-assets/src/shape.rs:139` (`shape_props`).

**Status:** the dispatch is by type name and covers the Fallout and Skyrim
families. `BSLightingShaderProperty` — every lit Skyrim shape — has no entry
in `shape_props`'s `match` at all: its texture set is read by the generic
`texture_set` path above the match, but its own flags and fields are not. That
is S4-2.

---

## S5 — Textures

**Decides:** how a DDS becomes a GPU image — format, sRGB or not, mip levels,
sampler.

**Consumes:** the DDS header, the device's compressed-format support.

**Oracle:** O2 (the DDS spec) and O4 (the level sizes the header claims against
the bytes present; the sRGB decision per slot).

**Lives:** `crates/nvre-dds`, `crates/nvre-assets/src/texture.rs:59`.

**Status:** DXT1/3/5 pass through as BC1/2/3; the three uncompressed layouts
convert to RGBA8. **BC4, BC5 and BC7 are not read at all** — the reader's
format table stops at DXT5, so every Fallout 4 normal map (BC5) and most of
its colour textures (BC7) fail. That is S5-1 and S5-2, and it is the reason
FO4's textures are the least verified of the five.

---

## S6 — Material build and shader

**Decides:** how the decoded props become a `StandardMaterial` plus the
`NifSurface` extension, and what the fragment shader does with them.

**Consumes:** `ShapeProps`; the texture handles; whether the mesh has UVs and
tangents.

**Oracle:** O2 (the engine's shader behaviour, from nif.xml's notes and
NVForever's mapping) and O4 (the extension's `Default` must be the neutral
surface, not `Vec4::ZERO`).

**Lives:** `crates/nvre-assets/src/model.rs:251` (`material_for`),
`crates/nvre-assets/src/nif_material.rs`, `nif_material.wesl`.

**Status:** the four behaviours the shader implements — falloff, alpha cut,
multiply, additive — are all gated on the extension being populated, and
`material_for` populates all four. `unlit` is deliberately `false` on the base
material, with the unlit branch in the shader, because `StandardMaterial`
bypasses extension shaders when `unlit` is set.

---

## S7 — Reference placement and world assembly

**Decides:** where a placed reference sits, and which references are in the
cell being drawn.

**Consumes:** the reference's `DATA` position, its Euler angles, `XSCL`, the
worldspace's grid, the streaming radius.

**Oracle:** O4 — and this stage has one of the strongest available: **static
collections ship both their combined mesh and the placement of each part**, so
the two must agree. `crates/nvre-world/src/transform.rs` already uses it and
reports 286 of 308 parts agreeing under `M = Rx(-x) Ry(-y) Rz(-z)`.

**Lives:** `crates/nvre-world/src/{index,placed,transform}.rs`,
`crates/nvre-editor/src/viewport.rs:721`.

---

## S8 — Terrain

**Decides:** the exterior ground — heights, normals, vertex colours, texture
layers and their alpha.

**Consumes:** `LAND`'s `VHGT` steps, `VNML`, `VCLR`, `BTXT`/`ATXT`/`VTXT`, the
`LTEX`→`TXST` texture set.

**Oracle:** O4 (the integrated heights against the stored step range; the
layer alpha's 17×17 positions) and O2 (fopdoc's `LAND` layout).

**Lives:** `crates/nvre-world/src/land.rs`,
`crates/nvre-editor/src/viewport.rs:1509`.

---

## S9 — Lighting and atmosphere

**Decides:** the ambient colour, the sun, the fog, and the point and spot
lights a cell's references carry.

**Consumes:** `CELL`'s `XCLL`, `LIGH`'s `DATA`, the light's flags.

**Oracle:** O2 (fopdoc for the subrecord layouts) and — for the *continuous*
half — O5. The sun's angle convention is the named residue here.

**Lives:** `crates/nvre-editor/src/viewport.rs:219` and `:1038`.

---

## S10 — Camera and post

**Decides:** the projection, the tone curve, the fog the camera blends to, and
the clear colour.

**Consumes:** the window resolution, the FOV, the far plane.

**Oracle:** O5 for the FOV and exposure; O4 for the far plane against the
largest world coordinate in the load.

**Lives:** `crates/nvre-editor/src/windows.rs:231`.

**Status:** `Tonemapping::None` is deliberate and documented — every game
writes display values straight to the framebuffer.

---

## S11–S16 — The stages that do not exist yet

These are named so they are not rediscovered later. Each gets its own step
file with a "not started" row in every game's column.

| Stage | What it owns | State |
|---|---|---|
| S11 | Animation — `NiTimeController`, keyframe data | `crates/nvre-nif/src/anim.rs` is a one-line doc comment |
| S12 | Skinning — `NiSkinInstance`, bone transforms | in the schema, read nowhere |
| S13 | Particles — `NiPSys*` emitters and modifiers | in the schema, read nowhere |
| S14 | Water — `WATR`'s 196-byte visual struct | the record loads and round-trips; nothing draws it |
| S15 | LOD and distance fade — `BSFadeNode`, `NiLODNode` | no fade is applied |
| S16 | Scene sort and decal ordering | `decal_bias` exists and has no caller |
