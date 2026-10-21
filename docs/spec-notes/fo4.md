# Fallout 4's plugin format

What was measured on the user's Fallout 4 install
(`/home/sara/Games/Fallout4/Data`, the base game plus its six DLC masters and
its Creation Club plugins), and what follows from it.

The container is the same TES4 format the other four games use: a `TES4`
header record, then groups of records, each with a 24-byte header and a
payload of subrecords. `Fallout4.esm` holds 1,549,277 records in 138 record
types, and the existing reader opens it unchanged.

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

## What is not modelled yet

- **BA2 archives.** Fallout 4's assets are in BA2, which no reader here
  opens. Fallout 4 opens with loose files only.
- **ESL plugins.** The light-master format (header flag 0x200) and the
  0xFE000800 FormID range are not modelled.
- **The string table.** A localized file's strings live in a `.strings`
  file inside a BA2; the editor shows the string *ID*, not the text.
