# Schema coverage notes

Observations made while raising `fnvtool esm coverage` against the shipped
masters. Every layout change needs a note here or in the schema's own `note`
field, naming the record, the subrecord, the observed sizes and what was
concluded.

## 2026-10-17 — FalloutNV.esm, all six shipped masters

Measured with `cargo run --bin coverage -- schemas/fnv "$NVRE_DATA"/<plugin>`.
The same numbers via `fnvtool esm coverage --schema schemas/fnv` require
upstream's `fnv-esm`, which cannot parse NVRE's schema-language additions.

| plugin | total | ok | unknown | trailing | error |
|---|---|---|---|---|---|
| FalloutNV.esm | 2,321,456 | **2,321,456 (100.000%)** | 0 | 0 | 0 |
| DeadMoney.esm | 160,880 | **160,880 (100.00%)** | 0 | 0 | 0 |
| HonestHearts.esm | 179,706 | **179,706 (100.00%)** | 0 | 0 | 0 |
| OldWorldBlues.esm | 225,669 | **225,669 (100.00%)** | 0 | 0 | 0 |
| LonesomeRoad.esm | 161,312 | **161,312 (100.00%)** | 0 | 0 | 0 |
| GunRunnersArsenal.esm | 6,853 | **6,853 (100.00%)** | 0 | 0 | 0 |

Every subrecord signature in all six masters has a schema entry (`unknown` is 0
everywhere), every declared subrecord decodes (`error` and `trailing` are 0
everywhere), and FalloutNV.esm's 2,321,456 subrecords all decode.

### The last two errors: QUST DATA's trailing run

xEdit declares `wbStruct(DATA, 'General', [Flags u8, Priority u8, wbUnused(2),
wbFloat('Quest Delay')]).SetOptionalFrom(3)`, marking only `Quest Delay`
optional. Two records — `0001E5CA` and `0002242A` — are 2 bytes: flags and
priority only, with the `wbUnused(2)` padding absent as well. The other 635
instances across the six masters are the full 8 bytes. Making the trailing
`unused` field optional covers both forms; the schema note records the
measurement and the deliberate widening of xEdit's `SetOptionalFrom(3)`.

### Records added (`schemas/fnv/navmesh.json`, `schemas/fnv/gaps.json`)

`NAVM`, `NAVI`, `RCPE`, `CDCK`, `CPTH`, `CAMS`, `ARMA`, `VTYP`, `AVIF`,
`ECZN`, `DEBR`, `RGDL` — see `schemas/README.md` for the file map.

### CAMS DATA is 36 bytes in 4 of 276 instances

The 40-byte layout is `Action/Location/Target` (3×u32), `Flags` (u32), three
time multipliers (3×f32), then `Max Time`, `Min Time`, `Target % Between
Actors` (3×f32). Four instances stop after `Min Time`. Expressed as a
size-decided union (`CameraShotData` → 40-byte full, else 36-byte short)
rather than an optional field, because the missing bytes are at the end of the
subrecord, not the end of a struct.

### ARMA DNAM is 4 bytes in 115 of 131 instances

`SetOptionalFrom(2)` in xEdit's ARMA `wbStruct(DNAM)` makes everything from
index 2 optional: `DR` (i16) and the `Modulates Voice` flag (u16) are required,
`DT` (f32) and 4 unused bytes are not. A 4-byte DNAM therefore carries only
`DR` and the flag; a 12-byte one carries all four. Confirmed by direct byte
inspection of ARMA `0x001649DF` (`LegateArmorRightHand`), whose DNAM is 12
zero bytes.

### DEBR DATA needed a mid-struct string primitive

The layout is `[u8 percentage][NUL-terminated model file name][u8 has
collision]`. All 26 instances in FalloutNV.esm follow it exactly — e.g.
`0f 47 6f 72 65 5c 52 6f 62 6f 42 69 74 30 31 2e 4e 49 46 00 01`. The string
sits in the middle of the struct, so neither `zstring` (rest-of-field) nor
`string` (unterminated) could express it. The fork gained a `cstring`
primitive — xEdit's plain `wbString` inside a struct: text up to the first NUL,
with the cursor left just past that NUL so the following field still decodes.
A missing NUL is an error, never a silent truncation.

### PERK DATA and EPFD are decided by sibling subrecords, not by size

`wbPerkDATADecider` picks the effect-data variant from the sibling `PRKE`'s
`Type`, and `wbEPFDDecider` from `EPFT` plus the effect's `Function`. Neither
is a size decision, so `fnv.schema/1`'s size-decided union cannot express
them. Measured sizes for PERK `DATA` are 3 (247), 4 (69), 5 (201) and 8 (112)
— the 3/5 split is the perk header's `SetOptionalFrom(4)`, the 4/8 split is the
effect union. Both stay undefined.

### MODS / MO2S / MO3S / MO4S need a length-prefixed string

`wbAlternateTexture` is `(u32 count, u32 length-prefixed string, TXST FormID,
i32 3D index)`, repeated. Confirmed byte-for-byte, e.g. STAT MODS:
`01000000 13000000 "BillboardHeliosNV:1" 85a81600 01000000`. `fnv.schema/1` has
no length-prefixed string and no count-prefixed array, so these stay undefined.
This is the largest remaining expressible gap: ~2,300 instances across STAT,
ARMO, TERM, HDPT, ACTI, DOOR and ARMA.

*Resolved.* NVRE's `fnv-esm` fork (`crates/fnv-esm`) adds the two constructs:
`len_string` (xEdit's `wbLenString`: `[u32 len][len bytes]`, no terminator) and
`{"array": T, "count_prefix": "u32"}` (xEdit's `wbArrayS`/`wbArray` with
`Count = -1`). ARMA MODS/MO2S/MO3S/MO4S are now declared in `gaps.json` as
`AlternateTexture` arrays; 532 of 533 instances in FalloutNV.esm parse exactly,
the outlier being a byte-scan false positive.

### The schema-language fork (`crates/fnv-esm`)

Three constructs were added to `fnv.schema/1`, each with decode *and* encode
support, and each strict rather than permissive:

- **`len_string`** — `[u32 byte count][that many bytes]`, no terminator. A
  declared length running past the field is an error, never a truncation.
  Re-encoding writes the count back, so a round-trip is exact.
- **`count_prefix`** — `{"array": T, "count_prefix": "u8"|"u16"|"u32"}`. The
  count is read from the data before the elements. A count larger than the
  field can hold is an error, never a short array.
- **`count_field`** — `{"array": T, "count_field": "n"}`. The count is an
  earlier integer field of the *same* struct (xEdit's `SetCountPath`). This is
  what a pair of count fields followed by two different-length arrays needs:
  the counts are stored once and shared, not repeated before each array.
  Validated at load time: the name must be an earlier field of the same struct.

The decoder keeps the count as a plain JSON number on the named field, so the
array can look it up; the encoder checks the array length matches before
writing, because the count bytes come from the field, not the array.

### NAVMI's island branch: a flag decider expressed as a size union

`wbNAVINVMIDecider` reads bit 5 of `Flags` to choose the island variant, which
`fnv.schema/1` cannot express as a value-decided union. Measured instead: a
non-island Navmesh Info is exactly 32 bytes and every island one is larger, so
it is expressed as a size-decided union (`{"size": 32}` first, fallback island).

The island struct itself is 28 bytes of header (`Flags`, navmesh FormID,
location FormID, Grid Y/X as i16, Approx Location as Vec3), then
`Navmesh Bounds` (24 bytes: two Vec3), then **two u16 counts**
(`Vertex Count`, `Triangle Count`), then `Vertex Count` Vec3 vertices, then
`Triangle Count` triangles of three u16, then `Preferred %` (f32). All 1,687
islands in FalloutNV.esm fit exactly; 4,771 NVMI parse with zero bytes left.

The two counts sitting together before either array is the detail that
`count_prefix` cannot express and `count_field` can: each array's count is the
stored field, not a prefix in the data.

### NVGD and NVCI

`NVGD` = `[u32 divisor][f32 max X][f32 max Y][Vec3 min][Vec3 max]` (36 bytes)
then `divisor²` cells, each `[u16 count][u16 × count]` — the cell array is
fill-to-end and each cell's triangle list is `count_prefix: "u16"`. Verified
4,771/4,771 exact.

`NVCI` = `[FormID navmesh]` then three `count_prefix: "u32"` FormID arrays
(Standard, Preferred, Door Links). Verified 4,771/4,771 exact.

### What is left, and why

| subrecord | instances | blocker |
|---|---|---|
| *(nothing)* | **0** | — |

Every subrecord signature in all six shipped masters now has a schema entry, and
all 2,321,456 of FalloutNV.esm's subrecords (100.000%) decode, with `unknown`,
`trailing` and `error` all zero.

Nothing is left blocked. The four previously-blocked signatures are resolved:

| subrecord | how |
|---|---|
| CREA NIFT (1,578) | a doubly-nested count-prefixed array (`count_prefix` on the outer array, an inner `u8`-prefixed one) |
| REFR TNAM (1,532) | a size-decided union: 4 bytes = Patrol Data's Topic FormID, 2 bytes = Map Marker's `[u8 type][u8 unused]`. Both verified by their sibling signatures (all 2-byte cases follow `XMRK/FNAM/FULL`, all 4-byte ones follow `SCHR/SCRO/SCTX`) |
| PERK DATA / EPFD (525) | the *value*-decided union (`ValueUnion`): `wbPerkDATADecider` reads the `Type` of a different subrecord (PRKE), and `wbEPFDDecider` reads the sibling EPFT. PERK DATA is one ValueUnion whose `default` covers the record-level layout, since a missing switch means PRKE has not appeared yet |
| DEBR DATA (26) | a new `cstring` primitive — xEdit's plain `wbString` inside a struct, i.e. text up to the first NUL with the cursor left past it so the following `Has Collision` byte still decodes |

Also filled in along the way, all verified against real bytes before entry:
REFR TNAM's two meanings, REGN's `RDMP`/`RDSD`/`RDOT`, IMAD's `RDSD`/`RDSI`,
`RADS`/`DEHY`/`HUNG`/`SLPD` DATA, `CDCK`/`RCCT`/`DOBJ`/`INGR`/`LSCT` DATA and
`INGR ENIT`, plus the earlier MODS-family and ARMA BMDT.

## 2026-10-21 — Fallout4.esm, the base master

Measured with `cargo run --release --bin coverage -- --overlay schemas/fo4
schemas/base <Data>/Fallout4.esm`.

| plugin | total | ok | unknown | trailing | error |
|---|---|---|---|---|---|
| Fallout4.esm | 6,742,917 | **6,742,917 (100.00%)** | 0 | 0 | 0 |

The schema is `schemas/fo4`, most of it generated from xEdit's
`wbDefinitionsFO4.pas` by `tools/fo4_pascal_to_schema.py` and the rest written
by hand from measured bytes. `docs/spec-notes/fo4.md` has the measurements;
this is the list of what the schema language had to learn to read it.

### The switches a union may use

A union's variant is not always decided by the bytes left over:

| the switch reads | example | how |
|---|---|---|
| a whole value | `PERK`'s `EPFD` on the sibling `EPFT` | `cases` |
| a bit of a value | a book's `Teaches` on the "add perk" bit of `Flags` | `bits` |
| a sibling field | a script property's value on the property's `Type` | `switch_on` |
| a sibling *subrecord*'s field | `PERK`'s `DATA` on `PRKE.Type` | `switch_on` + `expose` |
| a case table over an expression | a script property's value on `Container.ElementNativeValues['Type']` | `cases` |
| the record's form version | `EXPL`'s `DATA` on `wbDeciderFormVersion99` | `min_version` |

### The counts a field may take

A count is not always in the data: a navmesh's grid is the pathing cell's
size squared (`count_pow`), a script's fragments are counted by the flag bits
that are set (`count_bits`), and a miscellaneous object's components are as
many as fit (`repeat`).

### The strings

[observed] Fallout 4's text is Windows-1252, not UTF-16 as `schemas/README.md`
long said. A localized file holds its translatable strings as 4-byte string
IDs, and the switch is the *file's* — the TES4 header's bit 7 — not the
record's.

### The hand-written layouts

Some layouts the generator cannot express, written from measured bytes:
`CELL`'s combined references, `NAVM`'s navmesh, `WTHR`'s weather colours, and
`QUST`'s description, which xEdit declares a plain string and the game holds
as a string ID.

### Round-trip (decode → encode)

Checked with `cargo run --bin rtcheck -- schemas/fnv "$NVRE_DATA"/<plugin>`:
**all 2,321,436 decoded subrecords re-encode byte for byte, in all six masters.**

The three previously-reported gaps were all fixable and are now closed:
- `CSTY/CSTD` — the struct declares the same padding name six times
  (`wbUnused(2)` at four offsets plus two more), and a struct's JSON form is an
  object, so the sixth silently overwrote the first and the value could not be
  re-encoded. `parse_type` now disambiguates repeated field names in declaration
  order (`unused`, `unused_2`, … `unused_6`). The byte layout is untouched; only
  the JSON keys differ.
- `TERM/ITXT` and `TERM/RNAM` — a 4-byte all-NUL zstring decodes to `""` and
  re-encoded as a single NUL, losing the padding. Both are now a struct of
  `cstring` plus an optional rest-of-field `bytes` field, so the padding is
  carried through the JSON and written back verbatim. (895 TERM menu-item RNAMs
  end in a NUL; the failing cases were the 4-byte all-NUL ones.)

Getting here needed one encoder fix beyond the schema constructs: a struct
encoder previously ignored value fields its layout did not declare, which let a
union pick the wrong variant (an island `NavMeshInfo` encoded as the 32-byte
plain one, silently dropping the geometry). The encoder now rejects a value
carrying undeclared fields, so a union's variants must be distinguishable by
the fields they have.
