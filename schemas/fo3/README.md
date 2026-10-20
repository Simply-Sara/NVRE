# schemas/fo3 — Fallout 3 record layouts

Fallout 3's record layouts, as an **overlay** on `schemas/fnv`.

Measured with NVRE's own `coverage` binary against all six shipped Fallout 3
masters (2026-10-20):

```
$ cargo run --bin coverage -- --overlay schemas/fo3 schemas/fnv <Fallout3>/Data/*.esm
Fallout3.esm             total 2634084 ok 2634084 (100.00%) unknown 0 trailing 0 error 0
Anchorage.esm            total  177986 ok  177986 (100.00%) unknown 0 trailing 0 error 0
ThePitt.esm              total  136807 ok  136807 (100.00%) unknown 0 trailing 0 error 0
BrokenSteel.esm          total  174525 ok  174525 (100.00%) unknown 0 trailing 0 error 0
PointLookout.esm         total  235206 ok  235206 (100.00%) unknown 0 trailing 0 error 0
Zeta.esm                 total  125879 ok  125879 (100.00%) unknown 0 trailing 0 error 0
```

Round-trip (`rtcheck --overlay schemas/fo3 schemas/fnv ...`): every one of the
3,484,461 decoded subrecords re-encodes byte-identically.

## Why an overlay and not a copy

The first measurement was **FNV's schema reading FO3's files**: 100.00% on
2,634,080 of 2,634,084 subrecords, with 3 unknown and 1 error. The container,
the group tree, the FormID numbering model and almost every record layout are
shared between the two engines, so a Fallout 3 schema is four records, not 107.

`Schema::load_dir` rejects a record defined twice, which is right for one
game's files but wrong here: restating all 107 records to change four would
drift the moment `schemas/fnv` moves. So `fnv-esm` gained `load_overlay`, which
loads `schemas/fnv` and then lets `schemas/fo3`'s files **replace** the records
they name. A replacement is whole, not a merge — an overlay file carries a
complete record, because a partial one would silently drop the subrecords it
did not repeat.

`schemas/fo3/fnv` is a symlink to `schemas/fnv`. `load_overlay` skips the files
it reaches through that symlink (comparing canonical paths, which resolve it),
so `schemas/fo3` can be handed to any tool that reads a directory and still see
both halves.

## The four records that differ

| Record | Change | Source |
|---|---|---|
| `CONT` | `MODS` — the alternate-texture list the other nine `wbTexturedModel` types carry | xEdit; 5 instances in PointLookout.esm |
| `REGN` | `RDMD` (music type enum), `RDGS` (grasses), `ICON`, `MICO` | fopdoc Fallout3/REGN |
| `PACK` | `IDLB` — 4 unused bytes after `IDLA` | fopdoc Fallout3/PACK |
| `DOBJ` | `DATA` is 21 formids (84 bytes), not New Vegas's 34 (136) | Fallout3.esm's single instance |

Three of the four are also gaps in FNV's own schema rather than FO3
differences: FNV defines `MODS` on nine types but not `CONT`, and omits `IDLB`
entirely. FNV's shipped masters happen not to use either, so its coverage never
noticed. `RDMD` is genuinely Fallout-3-only.

### `DOBJ.DATA`: the record wins over the spec

fopdoc's Fallout 3 `DOBJ` page lists **23** fields, which would be 92 bytes. The
shipped record is **84 bytes = 21 formids**. The record wins: the game writes 21
of the 23 it documents, and the two it omits cannot be told apart from the bytes
alone. The 21 map field-for-field against fopdoc's order, every value a
plausible FormID or a legitimate `0`.

Because the subrecord's own length is the field count, a save writes 84 bytes
rather than growing the record to New Vegas's 136 — the same rule the FNV schema
follows for its variable-length subrecords.

## Rules for editing

- **A record named here is defined in full.** The overlay replaces rather than
  merges, so a partial definition silently deletes the subrecords it did not
  repeat.
- **Never change a layout to make the coverage number go up.** A layout change
  needs a cited source or an observation written up in a spec note, and the
  measurement goes in the entry's `note` — the same rule `schemas/README.md`
  sets for FNV.
- Unknown fields are named `unknown_N` / `unused`, never guessed.
- A record type may be defined in exactly one overlay file.
- Check after every edit:

```bash
cargo run --bin coverage -- --overlay schemas/fo3 schemas/fnv "$FO3"/Fallout3.esm
cargo run --bin rtcheck   -- --overlay schemas/fo3 schemas/fnv "$FO3"/Fallout3.esm
```

`ok_percent` must stay at 100.00 and `trailing` and `error` must stay at zero.

## What Fallout 3 has that New Vegas does not

The probe that produced the four records also lists the record types FO3's
masters never use — 39 of FNV's 107, from `AMEF` and `CCRD` to `MSET`, `SLPD`
and `TREE`. None of them need a FO3-specific layout: FNV's definitions decode
them wherever they appear, and FO3 simply has no instances. The list is here so
a later reader does not mistake "absent from the shipped masters" for "absent
from the format".
