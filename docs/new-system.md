# The new system

NVRE does not own the upstream repository, and does not want to depend on it
in future. So the upstream crates are not being patched and forked into
place — they are being **replaced by new crates that stand beside them**,
and removed when the new ones are good enough.

## The rule

**Nothing is swapped until the new thing is proved.** The old crate stays in
the graph, doing its job, until the new crate passes the same measurements.
Then the swap is one line in a manifest, and the old crate is deleted.

That is what "in pieces, so nothing gets worse" means here:

- The old and the new live at the same time.
- The new one is measured against real game data before it is used.
- The swap is a separate commit from the new code, so the diff that changes
  behaviour is empty.

## Where the pieces stand

| New crate | Replaces | Size | Status |
|---|---|---|---|
| `crates/nvre-util` | `fnv-util` | ~200 | **done** — the cursor, Windows-1252, SHA-256 |
| `crates/nvre-inflate` | `fnv-inflate` | ~250 | **done** — zlib with Bethesda's trailer anomalies |
| `crates/fnv-dds` | `fnv-dds` | ~460 | **done** — the DDS header, mip layout and RGBA8 |
| `crates/nvre-nif` | `fnv-nif` | ~3,600 | to do |
| `crates/nvre-esm` | `fnv-esm` | ~3,600 | to do |
| `crates/nvre-world` | `fnv-world` | ~4,800 | to do |
| `crates/nvf-assets` | `nvf-assets` | ~2,800 | to do |

The first three are done and the graph no longer reaches upstream for them.

## The order, and why

The order is by *what the editor actually uses*, smallest surface first, and
each piece is measured before the next begins.

### 1. `nvre-nif` — the NIF reader

The editor's own mesh path is the measure: `crates/nvre-editor/examples/nifload.rs`
reads 300 meshes per game through the editor's real asset path, and
`nifmesh.rs` counts the meshes that build with geometry.

This is also where the Special Edition's packed vertices have to land. Its
geometry is in the packed format, where the original edition keeps a vertex
list, so the current reader finds no positions and no mesh builds.

### 2. `nvre-esm` — the record reader

The measure is the coverage probe and the round trip: every game's masters
at 100.00%, and every decoded subrecord re-encoding to the same bytes.

This is the crate that is furthest from upstream already — 3,595 of its
3,618 lines are NVRE's own, because the schema language had to grow to
express what several games need.

### 3. `nvre-world` — the world formats

The measure is the editor opening a game headless: its records, its cells,
its references, its worldspaces.

This is the one that is furthest from being NVRE's own — 4,462 of its 4,810
lines are still upstream's, because the world formats (land, sky, water,
grass, placed objects) have not needed changing for Skyrim yet.

### 4. `nvf-assets` — the Bevy layer

The measure is the FNV control and the Skyrim LE control: the meshes that
build, their vertices and their triangles.

This is a **rewrite, not a fork**. The editor wants a model's geometry; the
game wants its animations, its door clips and its particle systems. A fork
would carry the game-side code forward; a rewrite leaves it behind.

## What "good enough" means

Each new crate must do at least what the old one does, for every game, and
measure the same. The measurements are in `tools/checkall.sh`, and they are
the definition of "good enough":

- Record coverage: every game's masters at 100.00%, zero unknown, zero
  trailing, zero error.
- Round trip: every decoded subrecord re-encodes byte-identically.
- NIF schemas: zero unknown blocks, zero re-encode mismatches, and the
  short and error counts no worse than the old reader's.
- The editor's own asset path: every game's meshes read, parsed, and their
  scene objects counted.
- Meshes that build: the FNV and Skyrim LE counts no lower than now.

## What stays until the end

`fnv-esm`, `fnv-nif`, `fnv-vfs`, `fnv-world` and `nvf-assets` all stay in the
graph, doing their jobs, until their replacement is in place and measured.
The editor keeps working the whole time.
