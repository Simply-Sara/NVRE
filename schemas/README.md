# schemas — a shared base plus one directory per game

```text
schemas/base/    the 63 record types every supported game uses, with a layout
                 each of them decodes correctly. Empty of anything else.
schemas/fnv/     Fallout: New Vegas's own types, and the records where its
                 layout differs from the base's.
schemas/fo3/     Fallout 3's own types and differences.
schemas/skyrim/  Skyrim LE's own types and differences.
schemas/skyrimse/Skyrim Special Edition's own types and differences.
schemas/fo4/     Fallout 4's own types and differences.
```

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

## How it was decided

Measured over every game installed on this machine, base master only:

| Game | Master | Types | Coverage of FNV's schema |
|---|---|---|---|
| Fallout: New Vegas | `FalloutNV.esm` | 107 | 100.00% (2,321,456) |
| Fallout 3 | `Fallout3.esm` | 90 | 100.00% (2,634,084) |
| Skyrim LE | `Skyrim.esm` | 118 | 66.75% |
| Skyrim SE | `Skyrim.esm` | 118 | 67.65% |
| Fallout 4 | `Fallout4.esm` | 137 | 60.14% |

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
// The base first, then each game's differences over it.
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
```

`--overlay` may be repeated and the directories after the last one are the base.
`rtcheck` re-encodes every subrecord it decodes, which is the round-trip half of
the check: `coverage` says a layout reads the bytes, `rtcheck` says it writes
them back identically.

Current numbers through the full chain:

| Game | coverage | rtcheck |
|---|---|---|
| Fallout: New Vegas | 100.00% (2,321,456) | byte-identical |
| Fallout 3 | 100.00% (2,634,084) | byte-identical (3,484,461) |
| Skyrim LE | 66.75% | — |
| Skyrim SE | 67.65% | — |
| Fallout 4 | 60.14% | — |

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
subrecords it did not repeat.

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

## Provenance

Every entry carries `confidence` and a `note` naming its source. The original
layouts came from the xEdit project's public definitions
(`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`,
`Core/wbDefinitionsFO3.pas`, branch `dev-4.1.6`, Mozilla Public License 2.0,
https://github.com/TES5Edit/TES5Edit) and were re-expressed in this format; no
xEdit code is included. NVRE's additions follow the same rule, and their `note`
names the Pascal file and line they came from.

`fnvtool esm coverage` prints the same numbers, but it lives inside the
NVForever submodule and links upstream's `fnv-esm`, so it cannot see NVRE's
patched schema language (`len_string`, `count_prefix`, `count_field`,
`sequence`). Use NVRE's own `coverage` binary for anything that uses them.
