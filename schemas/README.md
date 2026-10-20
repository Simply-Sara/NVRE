# Record schemas

NVRE's own copy of the Fallout: New Vegas record layouts, in the `fnv.schema/1`
format (documented at the top of `fnv-esm`'s `crates/fnv-esm/src/schema.rs`).

`fnv/*.json` started as a byte-for-byte copy of NVForever's
`NVForever/schemas/fnv/`, which covers 86 of the 107 record types in the
shipped masters. This directory adds the rest, so the editor can bind a
property grid to every field it can read.

## One directory per game

`schemas/<game>/` is the record layout for one game. The editor picks the
directory from the game the launcher chose (`crates/nvre-editor/src/game.rs`),
so a session over Fallout 3 reads `schemas/fo3/` and never FNV's. A directory
that does not exist yet is reported as `no schema files in ...` — the editor
refuses to guess another game's layout out of FNV's.

The format itself is game-neutral despite the `fnv` in its name: the container,
the GRUP tree and the FormID numbering model are shared across Gamebryo and
Creation engines, and only the record layouts differ. That is why one schema
language serves every game.

| Directory | Game | State |
|---|---|---|
| `fnv/` | Fallout: New Vegas | measured against all six shipped masters |
| `fo3/` | Fallout 3 | not started |
| `skyrim/` | Skyrim (Legendary) | not started |
| `skyrimse/` | Skyrim Special Edition | not started |
| `fo4/` | Fallout 4 | not started |

A new game is measured the same way FNV's was: point `coverage` and `rtcheck`
at its master, read the undecoded subrecords off the report, and write each
entry with a `confidence` and a `note` naming its source. Never transcribe a
layout from FNV's and assume the field means the same thing — `NPC_`'s `DATA`
differs between the two engines.

| File | Records |
|---|---|
| `common.json` | shared types and subrecords identical everywhere |
| `world.json` | CELL, WRLD, LAND, LTEX, REFR, ACHR, ACRE |
| `objects.json` | base objects (STAT, WEAP, ARMO, CONT, …) |
| `combat.json` | PROJ, EXPL, BPTD, AMEF, ENCH, SPEL |
| `gameplay.json` | GLOB, SCPT, QUST, DIAL, INFO, NPC_, CREA, RACE, … |
| `ai.json` | PACK, IDLM |
| `audio.json` | MUSC, MSET, ALOC, ASPC |
| `effects.json` | EFSH |
| `placed.json` | MSTT, GRAS, PWAT, ADDN, ANIO, IPCT, IPDS, TACT |
| `sky.json` | CLMT, WTHR, WATR, IMGS |
| `navmesh.json` | NAVM, NAVI — **NVRE addition** |
| `gaps.json` | records upstream does not define, transcribed from xEdit — **NVRE addition** |

`navmesh.json` and `gaps.json` are the two files NVRE has edited. Everything else
is still a byte-for-byte copy of upstream — check with `diff -ru
NVForever/schemas/fnv schemas/fnv`.

## Provenance

Every entry carries `confidence` and a `note` naming its source. Upstream's
entries are `documented`: layouts read from the xEdit project's public
definitions (`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`,
`Core/wbDefinitionsFO3.pas`, branch `dev-4.1.6`, Mozilla Public License 2.0,
https://github.com/TES5Edit/TES5Edit) and re-expressed in this format; no xEdit
code is included. NVRE's additions follow the same rule, and their `note` names
the Pascal file and line they came from.

## Rules for editing

- **Never change a layout to make the coverage number go up.** A layout change
  needs a cited source or an observation written up in a spec note.
- Unknown fields are named `unknown_N` / `unused`, never guessed.
- A record type may be defined in **exactly one file**. `Schema::merge_str`
  rejects a duplicate with `record X defined twice`, so a record's subrecords
  cannot be split across files — add to the file that already owns it.
- Coverage is measured, not assumed. Check after every edit:

```bash
cargo run --bin coverage -- schemas/fnv "$NVRE_DATA"/FalloutNV.esm
```

`fnvtool esm coverage --schema schemas/fnv` prints the same numbers, but
`fnvtool` lives inside the NVForever submodule and links upstream's `fnv-esm`,
so it cannot see NVRE's patched schema language (`len_string`, `count_prefix`,
`count_field`). Use NVRE's own `coverage` binary for anything that uses them.

`ok_percent` should rise and `trailing` and `error` should stay at zero.

Measured 2026-10-17 across all six shipped masters, after the navmesh, gaps and
schema-language additions: FalloutNV 99.81%, DeadMoney 99.79%, HonestHearts
99.80%, OldWorldBlues 99.73%, LonesomeRoad 99.72%, GunRunnersArsenal 98.66%.
`trailing` is 0 in all six; `error` is 0 in five and the two long-standing
`QUST DATA` errors (`0001E5CA: unused: [0]: read of 1 bytes at offset 2 exceeds
length 2`) in FalloutNV. The reasoning behind every layout, and the list of
what is still unexpressible and why, is in
`docs/spec-notes/schema-coverage.md`.

## Re-importing from upstream

This is a fork, so upstream fixes do not arrive on their own. After pulling
NVForever:

```bash
# 1. What moved upstream?
diff -ru NVForever/schemas/fnv schemas/fnv

# 2. Take upstream's version of any file you have not changed, then re-apply
#    your own additions on top.
cp NVForever/schemas/fnv/world.json schemas/fnv/world.json
# ... re-apply your entries to navmesh.json / gaps.json ...

# 3. Prove it still decodes.
cargo run --bin coverage -- schemas/fnv "$NVRE_DATA"/FalloutNV.esm
```

Files you have edited (`navmesh.json`, `gaps.json`, and any upstream file you
have added subrecords to) need the diff reviewed by hand. Everything else is a
straight copy.

The check that keeps this honest: `ok_percent` must never drop, and `trailing`
and `error` must stay at zero. If a re-import regresses either, the upstream
change and NVRE's addition disagree about a layout and one of them is wrong.
