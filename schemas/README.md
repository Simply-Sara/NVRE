# schemas — a shared base, one directory per family, one per game

```text
schemas/base/    the record types every supported game uses, with a layout
                 each of them decodes correctly. Empty of anything else.
schemas/fnv/     the Fallout family: Fallout: New Vegas's own types, and the
                 records where its layout differs from the base's. Fallout 3
                 loads this as its family layer.
schemas/fo3/     Fallout 3's own types and differences.
schemas/skyrim/  the Skyrim family: the layouts the original edition and the
                 Special Edition share.
schemas/skyrimle/the original edition's own types and differences.
schemas/skyrimse/the Special Edition's own types and differences.
schemas/fo4/     Fallout 4's own types and differences.
```

The chain is `base -> family -> version`. A family is a set of versions of the
same game. Fallout 3 and Fallout: New Vegas are one family, and so are Skyrim
and Skyrim Special Edition: the two Skyrim versions differ in six records and
share the rest, so the shared layouts live in `schemas/skyrim` and each
version's directory holds only its own differences. Fallout 4 is its own
family and has no shared layer yet.

Oblivion is not in this set. Its container revision predates the `TES4` layout
the reader parses, so `Oblivion.esm` does not open at all; nothing measured for
it can be called shared yet.

The format is `fnv.schema/1`, documented at the top of
`crates/fnv-esm/src/schema.rs`. The name is historical — the format is
game-neutral, and the container, the GRUP tree and the FormID numbering model
are shared across the Gamebryo and Creation engines. Only the record layouts
differ, which is why one schema language serves every game.

## The rule

A record belongs in `base` only when **every** supported game uses it **and**
the base layout decodes it correctly in all of them. Both halves are required.
"Present in every game" is not enough: `NPC_` is in all five, and FNV's layout
reads Fallout 3's copy fine while reading Skyrim's as an error, because Skyrim's
`DATA` is empty where FNV's is 11 bytes. A type in `base` is a claim about every
engine that will read it.

A record that only one *family* has belongs to that family's directory, not the
base. `CREA` is Fallout's and only Fallout's; Skyrim must not inherit a layout
for a record it does not have.

## How it was decided

Measured over every game installed on this machine, base master only:

| Game | Master | Types | Coverage of FNV's schema | Coverage of the game's own |
|---|---|---|---|---|
| Fallout: New Vegas | `FalloutNV.esm` | 107 | 100.00% (2,321,456) | 100.00% (2,321,456) |
| Fallout 3 | `Fallout3.esm` | 90 | 100.00% (2,634,084) | 100.00% (2,634,084) |
| Skyrim SE | `Skyrim.esm` | 118 | 67.65% | 100.00% (4,131,332) |

The last column is what the game's own directory achieves over the base. The
Fallout games are at 100% either way, because Fallout 3's records are Fallout
New Vegas's. Skyrim's own directory is what the difference between the columns
is made of: the sequence-aware records (`SCEN`, `PACK`, `QUST`), the
game-specific layouts (`NPC_`, `RACE`, `WEAP`, `ARMO`, `CELL`, `REFR`) and
the fields Skyrim added or changed (the 32-byte `CTDA`, the face tint fields,
`MODT`).

Every Skyrim SE master is at 100.00% with nothing but the base and Skyrim's own
files — Fallout's directory is not in the chain at all:

Fallout 4 is not measured yet: it needs a BA2 archive reader and a record
overlay, and its strings are UTF-16 rather than the other games' Windows-1252.
Its coverage against FNV's schema alone is 60.14%.

The intersection of all five is 77 types. Of those, 63 decode with zero
unknowns and zero errors in every game and moved to `base`:

```
ACHR ACTI ADDN ALCH AMMO ANIO ARMA ASPC AVIF BOOK BPTD CAMS CLAS CLMT CONT
CPTH CSTY DEBR DOBJ DOOR ECZN EFSH ENCH EXPL FACT FLST FURN GLOB GMST GRAS
HDPT IDLE IDLM IMAD IMGS INGR IPCT KEYM LAND LGTM LIGH LSCR LTEX LVLI LVLN
MESG MGEF MISC MSTT MUSC NAVI PERK PGRE PROJ SOUN SPEL TACT TREE TXST VTYP
WATR WRLD WTHR
```

The other 14 are present everywhere but their layout is not shared, so they stay
per-game:

```
ARMO CELL DIAL INFO IPDS NAVM NPC_ PACK QUST RACE REFR REGN STAT WEAP
```

The 63 is conservative in two ways. It uses each game's base master rather than
every DLC, so a type only a DLC introduces does not shrink `base`. And it
compares the *layout*, not just the signature, so a type that differs anywhere
stays out.

Skyrim loading `base + skyrimse` therefore never sees `CREA`, `HAIR`, `LVLC`,
`NOTE`, `PWAT`, `RADS`, `RGDL`, `SCPT` or `TERM` — the types it does not have and
would otherwise inherit a wrong layout for.

## Loading

```rust
// The base first, then the family's layer, then the game's own.
let schema = Schema::load_game("schemas/base", ["schemas/fnv", "schemas/fo3"])?;
```

`load_game` applies the directories in order, so a later one wins. A game
directory that does not exist is not an error — it means that game shares every
layout the base holds. Only a missing *base* is an error, because without it
there is nothing to overlay onto.

A game directory may carry a symlink to `../base`, which `load_game` detects by
comparing canonical paths and skips, so the base's files never merge twice. That
symlink is why `schemas/fo3` can also be handed to a tool that reads one
directory and still see both halves.

## Measuring a game

```bash
cargo run --bin coverage -- --overlay schemas/fnv --overlay schemas/fo3 \
    schemas/base "$FO3"/Fallout3.esm
cargo run --bin rtcheck   -- --overlay schemas/fnv --overlay schemas/fo3 \
    schemas/base "$FO3"/Fallout3.esm

# Skyrim needs only its own directory over the base.
cargo run --bin coverage -- --overlay schemas/skyrimse \
    schemas/base "$SKYRIM"/Skyrim.esm
```

`--overlay` may be repeated and the directories after the last one are the base.
`rtcheck` re-encodes every subrecord it decodes, which is the round-trip half of
the check: `coverage` says a layout reads the bytes, `rtcheck` says it writes
them back identically.

Current numbers through the full chain:

| Game | coverage | rtcheck |
|---|---|---|
| Fallout: New Vegas | 100.00% (2,321,456) | byte-identical |
| Fallout 3 | 100.00% (2,634,084) | byte-identical |
| Skyrim LE | 100.00% (4,130,941) | byte-identical |
| Skyrim SE | 100.00% (4,131,332) | byte-identical (3,674,003) |
| Fallout 4 | 60.14% | — |

The Skyrim figures are for `base + skyrim + skyrim<version>` alone:

| Master | Subrecords | Coverage |
|---|---|---|
| `Skyrim.esm` | 4,131,332 | 100.00% |
| `Update.esm` | 220,368 | 100.00% |
| `Dawnguard.esm` | 513,461 | 100.00% |
| `Dragonborn.esm` | 812,296 | 100.00% |
| `HearthFires.esm` | 114,323 | 100.00% |

And the original edition's, over `base + skyrim + skyrimle`:

| Master | Subrecords | Coverage |
|---|---|---|
| `Skyrim.esm` | 4,130,941 | 100.00% |
| `Update.esm` | 86,370 | 100.00% |
| `Dawnguard.esm` | 509,751 | 100.00% |
| `Dragonborn.esm` | 811,372 | 100.00% |
| `HearthFires.esm` | 101,056 | 100.00% |

The two versions differ in six records — `AMMO`, `ARMO`, `RACE`, `WATR`,
`WEAP` and `WRLD` — and the differences are the ones UESP names: the
original edition's `AMMO.DATA` is 16 bytes and the Special Edition's is 20,
its `RACE.DATA` is 128 and the Special Edition's is 164, its `WEAP.CRDT` is
16 and the Special Edition's is 24, and its `WRLD.MNAM` is 16 and the
Special Edition's is 28. The body template moved from `RACE.BODT` to
`RACE.BOD2`, and `ARMO` grew a `BOD2` of its own.

## Adding a record to `base`

1. Run `coverage` against every game's master with the base and that game's
   directory.
2. If the record comes back `ok` in all of them with no `unknown`, `trailing` or
   `error`, move it from `schemas/<game>/` into `schemas/base/shared.json`,
   along with every `types` entry it transitively references.
3. Re-run `coverage` for every game. All numbers must be unchanged.
4. Say in the record's `note` which games confirmed it.

## Adding a record to a game directory

Write it in that game's directory with a `note` naming the source and the
measured instance count. A record in a game directory replaces the base's whole
definition, so it must be complete — a partial one silently drops the
subrecords it did not repeat. A record that says `"merge": true` adds to the
record it shadows instead, which is what a game that changes two fields of a
hundred wants.

## Rules for editing

- **Never change a layout to make the coverage number go up.** A layout change
  needs a cited source or an observation written up in a spec note.
- Unknown fields are named `unknown_N` / `unused`, never guessed.
- A record type may be defined in **exactly one file per directory**.
  `Schema::merge_str` rejects a duplicate with `record X defined twice`, so a
  record's subrecords cannot be split across files in one directory — add to the
  file that already owns it. A *later directory* in the chain may replace it.
- A record in `base` is a claim about every engine that reads it. The evidence
  for that claim is the coverage run against each game's master, and it goes in
  the record's `note`.
- `ok_percent` should rise and `trailing` and `error` should stay at zero.

## A subrecord's count can come from a sibling

Some records carry a count in one subrecord and the elements in another:
Skyrim's `KSIZ` holds the number of keywords and `KWDA` holds them all in one
subrecord. The count is in a different subrecord from the elements, so it
cannot be a struct's `count_field`, and there was no way to express it.

A subrecord can name a `count_from` sibling. The count has to come *first*,
since it must have been read by the time the elements are:

```json
"KWDA": { "name": "Keyword", "type": { "array": "formid" }, "count_from": "KSIZ" }
```

The subrecord's value is remembered under its own signature as each subrecord
is decoded, so a later one can take its count from it. A subrecord decoded on
its own — which is what the encoder does when it checks that a field
re-encodes to the bytes it came from — recovers the count from its own size,
which divides evenly by the element size.

## Sequences, and the two kinds of block

Some records are not a bag of subrecords but a list of blocks, where the same
signature means different things depending on which block it is in. `SCEN` is
the case that needs it: its `ALID` opens the actor list *and* is a field
inside an action, so the signature alone cannot say which definition applies.

A record with a `sequence` gives the blocks it has:

```json
"sequence": {
  "start": ["EDID", "VMAD", "FNAM"],
  "blocks": {
    "actor": {
      "opener": "ALID",
      "order": ["LNAM", "DNAM"],
      "subrecords": { "ALID": { "name": "Alias ID", "type": "u32" } }
    }
  },
  "end": ["PNAM", "INAM", "VNAM"]
}
```

- `opener` is the signature that starts the block.
- `order` lists the signatures that follow the opener, in the order they
  appear. A signature in the order **continues** the current block; one that is
  not in the order, and is a known opener, **starts** a new block. That is what
  tells `SCEN`'s `ALID` apart: inside an action it is where the action's order
  says it should be, so it continues that block; at the head of the actor list
  it is not expected, so it opens one.
- `subrecords` is the block's own definitions. A signature defined in a block
  means that block's version of it, and the block's definition wins over the
  record's own map.
- `start` and `end` are the subrecords outside any block.

A block may also choose its layout by the opener's **value**, not just by its
signature. Skyrim's `PACK` opens a public activity with an `ANAM` whose value
names the activity's type — "Bool", "Float", "Location", "Topic" — and the
field that follows is decoded against that type:

```json
"variants": {
  "Bool":     { "subrecords": { "CNAM": { "name": "Bool Value", "type": "u8" } } },
  "Location": { "subrecords": { "PLDT": { "name": "Location Value", "type": "PLDT" } } }
}
```

A value that is not listed falls back to the block's own `subrecords`. The
choice holds for the whole block, not just the opener, so the field *after*
the opener is decoded against the variant the opener named.

The decoder walks the sequence and records the block each subrecord is in
(`DecodeCtx::block`), so `Schema::lookup_in_block` can pick the right
definition. `Coverage` uses the same walk, so a sequence record is measured
against its blocks.

## Overlays replace, they do not merge

A later directory in the chain **replaces** a record, a `common` subrecord or a
named type that an earlier one defined. It does not merge them. So a record
that is not the same in every game cannot live in `base` — the base holds only
what is the same everywhere, and a game that differs supplies its own copy.

`Condition` is the worked example. Fallout's `CTDA` is 20, 24 or 28 bytes
(Run On and Reference are optional); Skyrim's is always 32 bytes (Run On,
Reference and a trailing index are all present). So `Condition` and the
`CTDA` common subrecord live in `schemas/fnv/conditions.json` and
`schemas/fo3/conditions.json` — not in `base`.

## Generating a game's schemas from UESP

`tools/uesp_to_schema.py` reads the field table on a UESP record page and
writes the records and subrecords, with the type taken from a small mapping
and every entry marked as a `hypothesis` so it is clear what still needs
checking against the shipped data. It covers the record types whose pages are
a plain field table; the ones with a structure of their own — `SCEN`, `PACK`,
`QUST`, `DIAL`, `INFO`, `RACE`, `NPC_` — are written by hand.

The generated file is `schemas/skyrimse/generated.json`. Running the script
rewrites it, so the hand-written files are the ones that hold the layouts the
tables do not give.

## Provenance

Every entry carries `confidence` and a `note` naming its source. The original
layouts came from the xEdit project's public definitions
(`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`,
`Core/wbDefinitionsFO3.pas`, branch `dev-4.1.6`, Mozilla Public License 2.0,
https://github.com/TES5Edit/TES5Edit) and were re-expressed in this format; no
xEdit code is included. NVRE's additions follow the same rule, and their `note`
names the Pascal file and line they came from.

## Sequences: records whose subrecords are an ordered list of blocks

Some records are not a bag of subrecords but a list of blocks, where the same
signature means different things depending on which block it is in. `SCEN` is
the case this exists for: its `ALID` opens the actor list *and* is a field
inside an action, so the signature alone cannot say which definition applies.

A record with a `sequence` gives the blocks it has:

```json
"sequence": {
  "start": ["EDID", "VMAD", "FNAM"],
  "blocks": {
    "actor": {
      "opener": "ALID",
      "order": ["LNAM", "DNAM"],
      "subrecords": { "ALID": { "name": "Alias ID", "type": "u32" } }
    }
  },
  "end": ["PNAM", "INAM", "VNAM"]
}
```

- `opener` is the signature that starts the block.
- `order` lists the signatures that follow the opener, in the order they
  appear. A signature in the order **continues** the current block; one that is
  not in the order, and is a known opener, **starts** a new block. That is what
  tells `SCEN`'s `ALID` apart: inside an action it is where the action's order
  says it should be, so it continues that block; at the head of the actor list
  it is not expected, so it opens one.
- `subrecords` is the block's own definitions. A signature defined in a block
  means that block's version of it, and the block's definition wins over the
  record's own map.
- `start` and `end` are the subrecords outside any block.

The decoder walks the sequence and records the block each subrecord is in
(`DecodeCtx::block`), so `Schema::lookup_in_block` can pick the right
definition. `Coverage` uses the same walk, so a sequence record is measured
against its blocks.

A block may also choose its layout by the opener's **value**, not just by its
signature. Skyrim's `PACK` opens a public activity with an `ANAM` whose value
names the activity's type — "Bool", "Float", "Location", "Topic" — and the
field that follows is a `CNAM` for the first and a `PLDT` or `PDTO` for the
rest:

```json
"variants": {
  "Bool":     { "subrecords": { "CNAM": { "name": "Bool Value", "type": "u8" } } },
  "Location": { "subrecords": { "PLDT": { "name": "Location Value", "type": "PLDT" } } }
}
```

A value that is not listed falls back to the block's own `subrecords`. The
choice holds for the whole block, not just the opener, so the field *after*
the opener is decoded against the variant the opener named.

## Overlays replace, they do not merge

A later directory in the chain **replaces** a record, a `common` subrecord or a
named type that an earlier one defined. It does not merge them. So a record
that is not the same in every game cannot live in `base` — the base holds only
what is the same everywhere, and a game that differs supplies its own copy.

`Condition` is the worked example. Fallout's `CTDA` is 20, 24 or 28 bytes
(Run On and Reference are optional); Skyrim's is always 32 bytes (Run On,
Reference and a trailing index are all present). So `Condition` and the
`CTDA` common subrecord live in `schemas/fnv/conditions.json` and
`schemas/fo3/conditions.json` — not in `base`.

`fnvtool esm coverage` prints the same numbers, but it lives inside the
NVForever submodule and links upstream's `fnv-esm`, so it cannot see NVRE's
patched schema language (`len_string`, `count_prefix`, `count_field`,
`sequence`). Use NVRE's own `coverage` binary for anything that uses them.
