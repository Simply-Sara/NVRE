# 1:1 rendering, stage by stage

The goal: **for each of the five games the editor supports — FNV, FO3, FO4,
Skyrim LE, Skyrim SE — the editor's frame is the game's frame**, for the whole
pipeline from an archive byte to a lit pixel.

This directory is the plan. It is not a status report; it is the ladder, and
each rung is small enough to finish in one sitting and prove.

## The method

Three ideas hold the whole thing up.

**1. The pipeline is a chain of decisions, and each decision has a finite
domain.** A `NiAlphaProperty` flags word has 13 meaningful bits. A DDS header
names one of about fifteen formats. A `bs_version` picks one of a handful of
vertex layouts. A property block's type name picks one decode path. Every one
of those is *discrete*: its answer set can be enumerated exhaustively, so it
can be proved rather than sampled.

**2. Discrete decisions are provable without pixels. Continuous ones are not.**
The specular response, the fog falloff curve, the sun's angle convention, the
blend precision — those need a reference frame. The plan drives every stage to
discrete-exactness first, and keeps an explicit **residue list** (§Residue) of
the continuous parameters that only a capture can settle. That is deliberate:
nearly every "it renders wrong" bug so far — the unlit flag never assigned,
the material extension left at `Default`, the falloff `Vec4::ZERO` — turned out
to be a discrete bug wearing a continuous symptom. Prove the discrete half and
the residue gets small and cheap.

**3. Nothing is verified for one game.** A stage is done when all five columns
of [`matrix.json`](matrix.json) are filled. The FNV-only trap is the failure
mode this project has already paid for once in the schema work.

## The oracles, strongest first

| # | Oracle | What it proves | Cost |
|---|---|---|---|
| O1 | **Re-encode identity** — decode a block, write it back, compare bytes | the schema read the right layout, so every field S2–S6 consumes is right | free, no external reference |
| O2 | **nif.xml / fopdoc** — the published block layouts and bitfields | the mapping from bits to behaviour | free, citable |
| O3 | **An independent implementation** — NVForever's `nif_spawn.rs`, or a small Python reader over nif.xml as a differential oracle | two decoders agreeing over the corpus | medium |
| O4 | **Invariant conservation** — indices in range, one parent per node, a UV set with one distinct UV cannot texture | whole bug classes, no oracle needed | free |
| O5 | **In-game capture** — `coc`/`tm`/`tcl` with matched FOV, sun and fog, then SSIM | the continuous residue | expensive |

O1 and O4 are the workhorses. O1 already exists for records — every master
re-encodes byte-identically at 100% — and has **never been run over meshes**.
That single gap is the highest-value measurement in the plan.

## How a step is done

A step in [`steps/`](steps/) is one line in a table, and it is done when all
four are true:

1. **The check exists as code** — an example under `crates/*/examples/` or a
   bin under `crates/*/src/bin/`, in the pattern `nifprobe`/`nifsurvey`/
   `texturesurvey`/`emissive_check`/`schema_coverage` already use. It prints.
2. **It runs over the corpus, not one file.** A full-VFS walk needs a path
   filter env var — 20k meshes does not finish inside a tool timeout.
3. **Its output lands in `matrix.json`**, which is generated and committed, so
   a regression is a git diff.
4. **A unit test pins the rule** where the rule is discrete and small (the
   `alpha_mode` tests are the model).

A step that finds a defect is *not* failed — it is the step doing its job. The
defect gets fixed in its own commit, and the step's numbers go in the commit
message.

## The corpus

Verification is over a corpus, not over whatever file was open. The corpus is
a JSON manifest per game:

```json
{ "game": "fnv", "class": "effect", "path": "meshes\\fx\\lightrays.nif" }
```

Classes: `prop`, `clutter`, `architecture`, `effect`, `weapon`, `actor`,
`terrain`, `light`, `marker`, `water`. A class exists because a bug is almost
never one mesh — it is every effect mesh, or every unlit mesh, or every LOD
mesh. Counting by class is what turns "this one looks wrong" into "140 shapes,
all with the same cause".

## Residue

The continuous parameters, deferred until the discrete stages are clean. Each
is a single named number or curve, not a vague area:

| Parameter | Where | Why deferred |
|---|---|---|
| `fog_power` → fog curve | `viewport.rs:282` | Bevy's `FogFalloff` has no exponent; needs the engine's curve |
| Sun heading/elevation convention | `viewport.rs:319` | marked `[hypothesis]`; needs GECK/xEdit authority |
| `gloss_roughness` mapping | `model.rs:399` | the game's specular is not Bevy's roughness; error is bounded but nonzero |
| `DEFAULT_SUN_ILLUMINANCE` | `viewport.rs:344` | the game's sun is a display colour, not lux |
| `multiply_factor` gamma 2.2 | `nif_material.rs:305` | the blend equation's working space |
| `ADD_ALPHA_POWER` 1.5 | `nif_material.rs:327` | same |
| `DECAL_BIAS` / `DECAL_SLOPE_BIAS` | `nif_material.rs:226` | depth units, not game units |

Each residue entry gets a stage and a cell of its own in the matrix, so it
cannot be forgotten, and each is settled by one targeted capture rather than a
sweep.

## Index

| Stage | What it owns | Steps |
|---|---|---|
| S0 | Archives, load order, file resolution | [steps/S0.md](steps/S0.md) |
| S1 | NIF header and block decode | [steps/S1.md](steps/S1.md) |
| S2 | Scene graph | [steps/S2.md](steps/S2.md) |
| S3 | Geometry | [steps/S3.md](steps/S3.md) |
| S4 | Material decode | [steps/S4.md](steps/S4.md) |
| S5 | Textures | [steps/S5.md](steps/S5.md) |
| S6 | Material build and shader | [steps/S6.md](steps/S6.md) |
| S7 | Reference placement and world assembly | [steps/S7.md](steps/S7.md) |
| S8 | Terrain | [steps/S8.md](steps/S8.md) |
| S9 | Lighting and atmosphere | [steps/S9.md](steps/S9.md) |
| S10 | Camera and post | [steps/S10.md](steps/S10.md) |
| S11–S16 | The unimplemented stages | [steps/S11.md](steps/S11.md) |

[stages.md](stages.md) holds each stage's contract, the fields it consumes and
the oracle that judges it. [matrix.json](matrix.json) holds the five-game
status and is the only file a checker writes. [order.md](order.md) is the
sequence to work through — the catalogue is not the order.

## Running the checks

Every checker takes a data folder and a schema chain, so the same binary works
for every game. The schema chain argument differs per game — FNV and FO3 pass
`schemas/nif` alone, the others pass `base:<version dir>` — see
[bethesda-nif-appearance](../..) for the exact strings.

```sh
# Which NIF versions does this game actually ship?
cargo run -p nvre-editor --bin nifprobe -- <Data> schemas/nif

# Do the shapes get textures, and if not, why?
cargo run -p nvre-editor --example texturesurvey -- <Data> schemas/nif

# Do the emissive fields reach ShapeProps?
cargo run -p nvre-editor --example emissive_check -- fnv <Data>
```

`cargo test` is not run in this plan: parallel test targets exhaust RAM on this
machine. Verification is `cargo check` plus a single example run.
