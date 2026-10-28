# The order of work

The steps in [`steps/`](steps/) are a catalogue. This is the sequence — what to
do first, and why that thing and not another. The whole plan is 103 steps × 5
games; nobody works through that in file order.

## The principle

**Work by oracle strength, not by stage number.** S1 before S4 is not a
preference — S4's answers are meaningless until S1 proves the bytes were read
right. And within a stage, the step whose oracle is strongest goes first: O1
(re-encode) before O4 (invariant) before O5 (capture).

## Tier 1 — Prove the reader (no external reference needed)

Nothing downstream can be trusted until these are green for all five games.
Every one of these is a measurement that already has its tool half-written.

| # | Step | Why first |
|---|---|---|
| 1 | **S1-1** — every NIF block re-encodes byte-identically, per game | The single strongest oracle, the tool exists (`nifprobe`), and it has never been run over meshes. Whatever number comes back is the real starting point for everything else |
| 2 | **S1-3 / S1-4** — no `Unknown` and no `Short` blocks | Same run as #1. A `Short` block is a layout written for another game, and it silently shifts every field after it |
| 3 | **S1-2** — the version/`bs_version` histograms per game | Cheap, and it tells you which games the rest of the plan can even be verified for |
| 4 | **S0-2 / S0-3** — file precedence and load order | A wrong file is a false positive in every later measurement |
| 5 | **S4-1** — the consumed-field census | The check that generates the rest of S4. Expected to find something on its first run |

**Expected outcome:** a number of re-encode mismatches, a list of unknown
block types, and a census of unread fields. Those three lists *are* the bug
backlog, ranked by how many shapes each one touches.

## Tier 2 — The discrete mappings

These are the decisions with a finite, enumerable domain. Each becomes a table
test, and each is cheap once Tier 1 has cleared the way.

| # | Step | Why here |
|---|---|---|
| 6 | **S4-3 / S4-4** — `alpha_mode` over all 8192 flag combinations, per family | 13 bits, exhaustively checkable, and the blend mode is the single most visible property of a shape |
| 7 | **S4-2** — `BSLightingShaderProperty` dispatched on | Every lit Skyrim shape. A missing match arm is a class-wide defect, not a per-mesh one |
| 8 | **S4-10 / S4-11 / S4-12** — `shader_flags_2`, `NiVertexColorProperty`, `TexDesc` transforms | All three are "the field exists and nothing reads it". The census names them; this is the fixing |
| 9 | **S5-1 / S5-2** — BC4/BC5/BC7 and the DX10 header | Fallout 4 is one of the five target games and its textures are the least verified of the five |
| 10 | **S3-3 / S3-4** — the packed-vertex formats, per game | Two formats, one flag that is not reliable on its own; the difference is a whole game's geometry |
| 11 | **S6-1 / S6-2** — assert the extension's default and its population | Four lines of test that close the bug class which produced the last two rendering bugs |
| 12 | **S6-9 / S6-10 / S5-5 / S5-6 / S16-2** — dead code: `shade_light`, `hard_edges`, `set_anisotropy`, `get_clamped`, `decal_bias` | Wire or delete. Each is a field the pipeline reads and nothing writes |

## Tier 3 — The geometry and placement invariants

| # | Step | Why here |
|---|---|---|
| 13 | **S3-1** — count the dropped triangles instead of dropping them silently | A silent drop is how a layout error hides |
| 14 | **S2-1 … S2-4** — the scene-graph invariants | Cheap, and they catch the "wrong size / wrong place / mirrored" class |
| 15 | **S7-1 / S7-2** — re-measure the static-collection oracle and assert the convention | The strongest oracle in the plan that needs no external reference, and it is measured once for one game |
| 16 | **S7-5 / S7-6** — references resolve, and the grid draws exactly what it claims | Turns "the view is truncated" into a number |
| 17 | **S8-1 … S8-5** — terrain decode, and settle S8-3's stored-vs-computed normals | One open question that changes the picture, not a number |

## Tier 4 — Lighting, camera, and the residue

| # | Step | Why last |
|---|---|---|
| 18 | **S9-1 … S9-5** — the light mappings, with units | The record layouts are fopdoc-checkable; the units are not |
| 19 | **S9-6 / S9-7** — the sun convention and the fog power | The only two steps whose answer needs an authority or a capture. Both are single parameters |
| 20 | **S10-1 … S10-6** — camera, FOV, MSAA, clear colour | MSAA is a real gap and cheap to close |
| 21 | **The residue table in [README.md](README.md)** — one targeted capture each | By this point every discrete stage is proven, so a capture mismatch has exactly one candidate cause |

## Tier 5 — The missing stages

S11–S16, in the order the editor notices their absence: **S16** (decals, because
the plumbing exists), **S15** (LOD, because a wide view without fade is wrong
in an obvious way), **S12** (skinning, because actors are everywhere), **S14**
(water), **S13** (particles), **S11** (animation, the largest).

## What to do right now

Step 1. One command, one number:

```sh
cargo run -p nvre-editor --bin nifprobe -- \
  "/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data" schemas/nif
```

Raise `NIF_LIMIT` past the 300 default and let it walk the corpus. The output
is the mismatch count, the unknown types and the `bs_version` histogram — and
those three numbers set the size of everything that follows.
