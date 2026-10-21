# Decoupling from upstream

NVRE does not own the upstream repository, and does not want to depend on it
in future. This is the plan for getting to zero git dependencies, in pieces,
so that nothing gets worse on the way.

## Where the graph stands

The editor's dependency graph reaches upstream for four crates. Everything
else — `fnv-esm`, `fnv-nif`, `fnv-vfs`, `fnv-world` — already resolves to
NVRE's own copies through the workspace's `[patch]` table.

| Crate | Lines | Status |
|---|---|---|
| `fnv-util` | 163 | **done** — `crates/nvre-util` |
| `fnv-inflate` | 247 | **done** — `crates/nvre-inflate` |
| `fnv-dds` | 463 | **done** — `crates/nvre-dds` |
| `nvf-assets` | 6,789 | **to do** — `crates/nvf-assets` |

## The rules

- **One piece per commit.** Each commit changes one crate's ownership, and
  each is independently green.
- **A new crate alongside the old one, never a swap in place.** The new
  crate is added and proved; only then is the dependency switched, in a
  separate commit whose behavioural diff is empty.
- **A number, not a claim.** `tools/checkall.sh` measures the record
  schemas, the NIF schemas, the round trips and the editor's own asset path
  against real game data. Every number in it has to stay where it is.

## Piece 4 — the Bevy asset layer

`nvf-assets` is upstream's Bevy integration. The editor imports five of its
thirteen files:

| File | Lines | The editor's use |
|---|---|---|
| `model.rs` | 995 | `ModelCtx`, `ModelCache`, `PreparedModel` |
| `nif_material.rs` | 549 | `NifMaterial`, its plugin, the material builder |
| `texture.rs` | 195 | `TextureCache` |
| `data.rs` | 49 | `GameData` |
| `lib.rs` | 24 | the re-exports |

The other eight — `actor.rs`, `anim_blend.rs`, `anim_play.rs`,
`material_anim.rs`, `model_light.rs`, `nif_spawn.rs`, `particles.rs`,
`release.rs` — are 4,977 lines for the *game*, not the editor: playing
animations, door clips, particle systems, releasing idle assets.

### Why this one is a rewrite, not a fork

`model.rs` reaches into all five of those game-side modules. Its
`prepare()` builds the model's idle animation, its door clips, its material
and transform tracks, its particles and its light. The editor wants none of
that — it wants the geometry.

What the editor actually reads from a prepared model:

- `bounds` — for the reference's bounding box.
- `shapes` — each with a `transform`, a `mesh` and a `material`.
- the material's textures, for the texture cache.

### The order

1. **A faithful copy of the five files the editor uses**, minus the eight it
   does not, with the animation, particle and rig code removed. Behaviour
   for FNV and Skyrim LE must be identical — the FNV control (67,172 meshes,
   38.5 million vertices) and the Skyrim LE control (80,623 meshes, 35.6
   million vertices) are the measures.
2. **The Special Edition's packed vertices.** Its geometry is in the packed
   format and the current mesh builder reads a vertex list, so it builds
   nothing. This lands in the new layer, where it belongs.
3. **The game-specific bits**: the material and the texture lookup are
   Fallout's. Skyrim's own material conventions are different.

## What is left after that

Fallout 4 still needs its own archive reader (BA2), its own string
encoding (UTF-16), ESL support with flag `0x200`, record compression, and
a record overlay of its own.
