# New Vegas Rust Editor

A GECK-like editor for Fallout: New Vegas, written in Rust, built on
[NVForever](https://github.com/LootJunkieAzza/NVForever)'s format crates.

The editor is **standalone**: it lives in its own cargo workspace and depends on
NVForever by git. Nothing is patched into NVForever, and the game's files are
only ever read — saving writes a new `.esp` wherever you ask.

NVForever's crates are pulled by git (no pinned `rev`; `cargo update` moves
them), so no submodule is needed:

```
git clone <url>
cargo build
```

Two of those crates are forked into `crates/fnv-esm` and `crates/fnv-world` and
substituted with `[patch.crates-io]`-style `[patch]` entries. `fnv.schema/1`
could not express count-prefixed arrays, length-prefixed strings, mid-struct
NUL-terminated strings or value-decided unions, so the layouts in `schemas/`
could not reach 100% coverage through the upstream schema language. Each fork's
diff against upstream is small and documented in its `Cargo.toml` header.

## What is built

### `nvre-edit` — the write path (headless, fully tested)

NVForever's `fnv-esm` reads plugins losslessly and writes them back byte for
byte, but it cannot *change* one. This crate adds that.

| Module | What it does |
|---|---|
| `encode.rs` | JSON values → subrecord payloads, the exact inverse of `fnv_esm::decode`. Refuses anything it cannot round-trip. |
| `document.rs` | A mutable plugin: edit fields, insert and delete references, save. Untouched records keep their original bytes. |
| `formid.rs` | Rewriting the FormIDs a record carries when it moves into another plugin — schema-driven, so nothing is guessed. |
| `save.rs` | Writing the result as a **new plugin** with its own master list, which is what makes it loadable as a mod. |

**Guarantees, all enforced by tests:**

- An untouched document re-serialises **byte-identically** to the input.
- Every subrecord the decoder reads as `Ok` re-encodes to the same bytes, or the
  field is marked read-only. Measured over `FalloutNV.esm` + 5 DLC masters:
  **2,321,456 subrecords, 100.00% decoded, 0 unknown, 0 trailing, 0 errors**,
  and every decoded subrecord re-encodes byte-identically.
- Integers are width-checked. A `300` written into a `u8` field is an error, not
  a silent truncation.
- Repeated struct field names (CSTY/CSTD's six `unused` fields) are
  disambiguated at parse time (`unused`, `unused_2`, …) so the JSON holds every
  field and the record still round-trips.
- Deleting a record sets the Deleted flag — the way the game itself removes a
  reference — rather than dropping it from the file.
- A record that was compressed stays compressed: the subrecords are decoded from
  the decompressed form, so writing them back plain would leave the COMPRESSED
  flag set on uncompressed bytes and make the record unreadable.

### `schemas/fnv` — the record layouts

106 record types plus the shared subrecord table, measured against the shipped
masters rather than transcribed from documentation. Every entry carries a
`confidence` and a `note` naming its source; see
[docs/spec-notes/schema-coverage.md](docs/spec-notes/schema-coverage.md) for what
was measured, and [schemas/README.md](schemas/README.md) for the rules a new
entry has to follow.

### `nvre-editor` — the GUI

A Bevy window with bevy_egui panels, built on NVRE's `nvre-assets` layer so
the viewport renders the game's own meshes.

| Module | What it does |
|---|---|
| `session.rs` | The open editing session: which cell is open, what is selected, and the queued edits. Plain data, no Bevy, so it can be driven from a test. |
| `viewport.rs` | Orbit camera, cell rendering through `nvre-assets`, click-to-select. |
| `panels.rs` | Cell browser, reference table, property grid, object palette, status bar. |

```
NVRE_DATA=<Data> cargo run -p nvre-editor -- --cell Goodsprings
```

`--check` loads the session, reports what it sees, and exits without a window, so
the GUI's data path can be verified where no display is available:

```
$ NVRE_DATA=<Data> cargo run -p nvre-editor -- --check --cell Goodsprings
plugin        FalloutNV.esm
records       465017
cells         30497
references    314269
worldspaces   14
open cell     000DAEBB with 159 refs
```

The session is also drivable from an example, which places, moves, deletes and
saves without a window — the same code the panels call:

```
NVRE_DATA=<Data> cargo run -p nvre-editor --example roundtrip_gui -- out.esp
```

## Verified against the real game data

With `NVRE_DATA` pointing at a `Data` folder, an example builds a real mod
and NVForever's own `fnvtool` checks it:

```
NVRE_DATA=<Data> cargo run -p nvre-editor --example make_mod -- MyMod.esp
<NVForever>/target/release/fnvtool esm verify MyMod.esp
<NVForever>/target/release/fnvtool esm loadorder <Data> FalloutNV.esm DeadMoney.esm MyMod.esp
```

`make_mod` inserts two placed objects into WastelandNV's persistent cell and
saves them as a plugin depending on `FalloutNV.esm` + `DeadMoney.esm`. The
result, **1.8 MB with 2 records**:

```json
{"roundtrip_identical": true, "records": 3, "compressed_records": 0,
 "decompress_errors": [], "subrecord_errors": [], "payload_roundtrip_mismatches": []}
```

and the load-order report:

```
load_index 2   records 2   new_forms 2   overrides 0   unmappable []
```

`new_forms: 2, overrides: 0` is the point: a mod that adds two records and
overrides nothing it did not touch. The inserted references decode correctly
through NVForever's own reader, with their base objects remapped from the
source's numbering (`0x00157C89` → `0x02157C89`) and fresh non-colliding FormIDs
(`0x02000000`, `0x02000001`).

### What that verification caught

Four bugs were found this way and fixed:

1. **Compressed records were being flattened.** The remapper decodes a record's
   subrecords, which decompresses them, and was writing the result back plain —
   leaving the COMPRESSED flag set on uncompressed bytes. 9,339 records failed
   to decompress and the file grew by 25 MB. Fixed by recompressing when the
   record was compressed.
2. **The saved mod overrode everything.** Copying the master into the new file
   made the load order report `overrides: 465016` with no ONAM entries to
   declare them. Fixed by defaulting to `Scope::OwnRecords`, so a mod carries
   only its own records; `Scope::WholePlugin` remains for round-tripping.
3. **ONAM was written one FormID per subrecord.** The shipped masters store it
   as **one packed subrecord** (DeadMoney.esm: 179 FormIDs in 716 bytes). One
   subrecord per FormID left the reader seeing a single-entry list and dropping
   the rest: `onam_entries: 1` for two overrides. Fixed by packing.
4. **Edits to existing records were silently dropped.** An override needs the
   record copied into the mod under its original FormID; without that, editing a
   master's field produced a mod with nothing in it.

### Overriding, not just adding

Editing or deleting a record that belongs to another plugin means *overriding*
it: the mod carries a copy of that record with the change applied, keeps its
original FormID (top byte = the overridden plugin's index in the new master
list), and declares it in the header's ONAM array. Deletion is an override with
the Deleted flag set, which is how the game itself removes a reference.

A newly placed record is staged with FormID 0 and the save assigns its real one
from the new plugin's own range (`masters.len() << 24`), so the session tracks it
through `selected_saved_id()` until then. Edits queued on a staged record follow
it to the saved FormID.

```
NVRE_DATA=<Data> cargo run -p nvre-editor --example make_override -- MyOverride.esp
<NVForever>/target/release/fnvtool esm loadorder <Data> FalloutNV.esm MyOverride.esp
```

A mod that moves one reference and deletes another:

```
records 2   new_forms 0   overrides 2   onam_entries 2   onam_that_are_overrides 2
overrides_by_type {'REFR': 2}
```

with `esm verify` reporting `roundtrip_identical: true`, no decompress errors,
no subrecord errors and no payload mismatches — and the overridden records
decoding correctly, the moved one at its new position with every other field
preserved, the deleted one carrying `flags 0x420` (the `0x20` Deleted bit on top
of its original `0x400`).

Overriding a plugin that is not in the master list is an error rather than a
dangling reference.

### Placement comes from the world index

Where an override lands matters as much as what it contains: it has to sit in the
same cell-children group as the record it replaces, or the load order will not
associate the two and the edit silently does nothing in-game while every
structural check still passes. The cell comes from `fnv_world::WorldIndex`
(`ix.refs[fid].cell`) — the same index the renderer and the simulation use — not
from the FormID's low bits, which have no relation to a cell's FormID.

Newly placed references are written with `flags 0`, which is what 278,274 of the
game's own references in a temporary-children group carry. `0x400` (PERSISTENT)
is only for a worldspace's persistent cell, where 10,374 carry it, and is set
explicitly through `make_refr_flags`.

## The ESM layout, as measured

Read out of the shipped master with a size-validating walker, not from
documentation. A node is a **group** when its signature is `GRUP`, otherwise a
**record**, and the two have different 24-byte header layouts:

| Field | GRUP | record |
|---|---|---|
| +4 | `size` (total, incl. header) | `data_size` (payload only) |
| +8 | label (4 bytes) | flags (u32) |
| +12 | group_type (i32) | form_id (u32) |

Reading one as the other desyncs the walk — the single most common way to get
this format wrong.

```
GRUP type 0 "WRLD"                    (101 top-level type-0 groups, one per record sig)
└─ WRLD record                        (14 worldspaces)
   └─ GRUP type 1 WORLD_CHILDREN       (labelled with the WRLD FormID)
      ├─ GRUP type 4 CELL_BLOCK        (label = two i16 grid coords)
      │  └─ GRUP type 5 CELL_SUBBLOCK  (label = two i16 grid coords)
      │     ├─ CELL record(s)
      │     └─ GRUP type 6 CELL_CHILDREN      (labelled with the CELL FormID)
      │        ├─ GRUP type 8 PERSISTENT_CHILDREN  ← existing references
      │        └─ GRUP type 9 TEMPORARY_CHILDREN   ← new placements go here
      └─ CELL + GRUP type 6 (the worldspace's own persistent cell, e.g. 0x31e2a)
```

`raw::write_nodes` recomputes every group's size from its children, so inserting
a node needs no manual size patching.

The smallest valid `REFR` is `NAME` (base FormID) + `DATA` (24 bytes: position
xyz then rotation xyz as f32 radians); `EDID`, `XSCL`, `XTEL` and the rest are
optional. 186 of the 187 persistent references in WastelandNV's persistent cell
carry header flags `0x400`.

## Building and testing

```
cargo build                       # the whole workspace
cargo test -p fnv-esm -p nvre-edit --lib
cargo clippy -p fnv-esm -p nvre-edit --all-targets
```

Set `NVRE_DATA` to the game's `Data` folder to run the tests that check against
real masters (they skip otherwise, so CI needs no game data).

Two headless binaries measure the schema against real plugins, and are the
fastest way to see where coverage stands:

```
cargo run --bin coverage -- schemas/fnv "$NVRE_DATA"/FalloutNV.esm
cargo run --bin rtcheck   -- schemas/fnv "$NVRE_DATA"/FalloutNV.esm
```

`coverage` reports decoded / unknown / trailing / error per plugin;
`rtcheck` decodes every subrecord and re-encodes it, reporting any that do not
come back byte-identical. Both currently report 100% and a clean round-trip
across all six shipped masters.

**Coverage is measured against the six shipped masters, which is not the same as
"every possible ESM/ESP".** A modded plugin can carry record types and
subrecord signatures those masters never use, and those are only discovered by
pointing these binaries at real mods.

## Licence

GPL-3.0-or-later, matching NVForever — see [LICENSE](LICENSE) and
[LEGAL.md](LEGAL.md).
