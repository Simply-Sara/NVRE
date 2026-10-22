# Fallout 4's plugin format

What was measured on the user's Fallout 4 install
(`/home/sara/Games/Fallout4/Data`, the base game plus its six DLC masters and
its Creation Club plugins), and what follows from it.

The container is the same TES4 format the other four games use: a `TES4`
header record, then groups of records, each with a 24-byte header and a
payload of subrecords. `Fallout4.esm` holds 1,549,277 records in 138 record
types, and the existing reader opens it unchanged.

[observed] With `schemas/base` and `schemas/fo4` over it, all 6,742,917
subrecords of `Fallout4.esm` decode: 100.00%, with nothing unknown, nothing
left over and no errors. Every one of them re-encodes to the same bytes.

Everything below that differs from the other games is listed here with the
evidence for it.

## Text is Windows-1252, not UTF-16

`schemas/README.md` said, before this was measured, that Fallout 4's strings
are UTF-16. They are not.

[observed] A sweep of every subrecord in `Fallout4.esm` that holds
text-shaped bytes found no UTF-16LE field. Every text field decodes as
Windows-1252: `EDID` is `MQ101SoldierListCheckerREF\0`, `MODL` is a path like
`Meshes\Actors\Character\...\Character.nif\0`, and the non-ASCII bytes that
do appear decode as CP1252 (`ú`, `º`, `Œ`, `ÿ` and so on). The one field
whose bytes look like text but are not is `NOTE`'s `SNAM`, which holds a
4-byte string ID like any other localized field.

So the schema language needs no new string primitive for the encoding. The
other games' `zstring` reads Fallout 4's text correctly.

## Localized strings are 4-byte string IDs

A *localized* file keeps its translatable strings in a string table beside
the plugin and refers to them by index, so the fields that would hold a name
hold a `u32` instead.

[documented] xEdit's `wbDefinitionsFO4.pas` declares `FULL`, `DESC` and their
kin as `wbLStringKC` — a localized string — and `wbLStringDef.GetSize` returns
`Min(4, ...)` for one when the file is localized.

[observed] `FULL` is exactly 4 bytes in every one of the 43 record types of
`Fallout4.esm` that carry it, with no exceptions. `STAT`'s `FULL` holds
`0x00035072` and `0x0003502A` — string-table indices, not names.

### The switch is the file's, not the record's

[documented] xEdit's `TwbLStringDef` asks `aElement._File.IsLocalized`, and
`wbRecord(TES4, 'Main File Header', ...)` names the TES4 header's bit 7
"Localized".

[observed] All nine shipped Fallout 4 masters and Creation Club plugins set
that bit (`0x81` for an ESM, `0x281` for an ESL). No record in the master
carries the flag itself for this purpose.

So the switch belongs to the file, and the schema expresses it as a type
rather than as a per-record flag: the `localized` primitive in
`fnv.schema/1` reads a `u32` when the file is localized and a NUL-terminated
Windows-1252 string when it is not. One schema therefore describes both
Fallout 4's shipped masters and an ordinary plugin.

`crates/fnv-esm/src/raw.rs` names the bit `record_flags::LOCALIZED`, and
`PluginHeader::is_localized` reads it.

## The record header's flags

[documented] xEdit's `TwbMainRecordStructFlags`:

| bit | mask | meaning |
|---|---|---|
| 0 | 0x00000001 | ESM (master) |
| 2 | 0x00000004 | Non-playable (per record type) |
| 5 | 0x00000020 | Deleted |
| 6 | 0x00000040 | Visible when distant |
| 7 | 0x00000080 | Localized (TES4 only; per record type elsewhere) |
| 9 | 0x00000200 | ESL (light master) |
| 10 | 0x00000400 | Persistent (per record type) |
| 11 | 0x00000800 | Initially disabled |
| 18 | 0x00040000 | Compressed |

[observed] The flags measured on the master match: `ACHR` carries 0x200 on
1,062 records, 0x400 on 613, 0x800 on 769, and so on.

## The layout source

xEdit's `wbDefinitionsFO4.pas` (Mozilla Public License 1.1) is the
authoritative layout: it names every record type, every subrecord and every
field, and gives the header-flag bits per record type.

The Fallout, Fallout 3 and Skyrim schemas in this repo were built from the
same kind of source (`wbDefinitionsFNV.pas`, `wbDefinitionsFO3.pas`,
`wbDefinitionsTES5.pas`), so Fallout 4's is read the same way.

`tools/fo4_pascal_to_schema.py` reads the .pas and writes the schema. It
handles the constructs xEdit uses for Fallout 4:

- `wbRecord` and `wbRefRecord` — a record and a placed object.
- `wbRStruct` / `wbRArray` and their variants — a group: a run of subrecords
  that repeats, or a fixed run of several subrecords in order. A group of
  one repeating subrecord is a plain list of that subrecord; a fixed run
  becomes a `sequence` block, because it is positional.
- `wbStruct`, `wbArray`, `wbUnion` — each in both the with-signature and the
  without-signature form xEdit uses.
- The unit's own `var` block, whose names are the shared definitions.
- `ReferenceRecord`, a local procedure that builds the eight placed-projectile
  record types from one layout, expanded once per call.

The eight record types whose groups the generator cannot express are written
by hand: `SCEN`, `PACK`, `QUST`, `DIAL`, `INFO`, `RACE`, `NPC_`, `PERK` and
`MGEF`.

## Compressed payloads

Some record types hold a payload that is compressed in place, and the
compression is not announced by the record's type.

[observed] `LAND`, `CELL`, `NAVM` and `NPC_` are the candidates, but the
compression is decided by the bytes: a plain `CELL` begins with the ASCII
`DATA`, and only 8,471 of the 40,165 `CELL` records in `Fallout4.esm` are
compressed. So the reader looks at the bytes rather than at the type.

[observed] The framing is a little-endian `u32` giving the uncompressed
length, then a zlib stream that begins at offset 4 of the payload. A zlib
header is recognised by its compression method (8), its check bits
(`(CMF * 256 + FLG) % 31 == 0`) and its preset-dictionary flag being clear.

## The navmesh

[observed] All 7,894 `NAVM` records in `Fallout4.esm` carry an `NVNM` of
version 15, and all of them use 21-byte triangles and 11-byte edge links,
whatever their form version. The grid's size is the pathing cell's size
squared, which the schema says with `count_pow`.

## The weather's colours

[observed] A `WTHR`'s `NAM0` is 272 bytes in 4 records, 544 in 2 and 608 in
65. The 272-byte form is 68 FormIDs; the other two are 17 and 19 sets of
32-byte weather colours, each set being 8 colours of 4 bytes.

## The cell's combined references

[observed] All 4,600 `CELL` records carry an `XCRI`. Its size is 8 bytes plus
4 bytes for each mesh and 4 bytes for each reference, and a reference is the
FormID alone — the combined mesh xEdit shows is computed, not stored beside
it.

## A quest's description is a string ID

[documented] xEdit declares `QUST`'s `NNAM` a plain `wbString`.

[observed] The game holds it as a string ID: all 1,198 `NNAM` fields in
`Fallout4.esm` are 4 bytes. The schema therefore says `localized` for it.
Skyrim's quest objective text is the same shape — all 1,452 `NNAM` fields
in each of its two `Skyrim.esm` are 4 bytes — and its schema says so too.

## What is not modelled yet

- **BA2 archives.** Fallout 4's assets are in BA2, which no reader here
  opens. Fallout 4 opens with loose files only.
- **ESL plugins.** The light-master format (header flag 0x200) and the
  0xFE000800 FormID range are not modelled.
- **The string table.** A localized file's strings live in a `.strings`
  file inside a BA2; the editor shows the string *ID*, not the text.
