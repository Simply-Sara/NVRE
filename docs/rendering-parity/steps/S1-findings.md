# S1 findings

What the S1 checks have measured, and what they found. This is the working
record: each entry says what was measured, over how much, and what it means.

## S1-1 / S1-3 / S1-4 — the whole corpus, all five games

`nifreencode` reads every block of every NIF in a game and writes it back,
and the file's own block table says how many bytes each block holds, so the
two are compared. A block that decodes but does not re-encode is a block the
schema read with the wrong layout; a block the schema does not know is one
it cannot read at all; a block that reads short is one it read with the
wrong number of bytes.

Over **all 306,674 files** of all five games:

| Game | Files | Blocks | Unknown | Short | Error | **Re-encode** |
|---|---|---|---|---|---|---|
| fnv | 20,542 | 654,685 | 0 | 0 | 0 | **0** |
| fo3 | 17,163 | 522,939 | 0 | 0 | 0 | **0** |
| skyrimle | 21,476 | 831,923 | 0 | 0 | 0 | **0** |
| skyrimse | 22,761 | 786,986 | 0 | 0 | 0 | **0** |
| fo4 | 224,734 | 7,002,022 | 0 | 0 | 0 | **0** |

Every block of every mesh in all five games now decodes cleanly and writes
back to the bytes it was read from, and every file parses.

`nifsize` answers the same question more cheaply, by comparing each block's
byte count against the file's own table without decoding anything else; it
reports the same zero.

## How the Fallout 4 and Skyrim layers were built

The Fallout 4 and Skyrim layers were hand-written, and wrong in about
thirty block types, which is what the numbers above were measuring. They
are now generated from **Bethesda's own `nif.xml`** — the format
definition the NIF tooling is generated from — by
`tools/nifxml_to_schema.py`, which turns nif.xml into NVRE's schema
language.

The conversion is a translation, not a copy. nif.xml names its fields
"Num Data" and its versions "#FO4#", writes its conditions in its own
expression language (`#BITAND#`, `#RSH#`, `#FO4_2#`), and stores an enum as
whatever width its `storage` attribute says — a byte for the Havok material
types, a word for the shader flags, a word for the animation types. NVRE's
schema writes fields in snake case, conditions as expressions over the
file's own versions, and an enum as a named type. The converter does that
translation and reports the fields it cannot express rather than guessing
at them; three fields are skipped, all in types no shipped block uses.

`tools/build_fo4_layer.py` then merges the generated schema with the
hand-written notes, so a reader of the layer still knows why a type is the
way it is.

## The three faults the measurements found

The schema could not fix these; the reader and the writer had to.

**A half float's NaN lost its payload.** A signalling NaN is quieted when
it is widened from the half to the 32-bit float, and the quieting takes the
bit the payload's top bit would have been. Fallout 4's vertex data holds
NaNs, so a file that held one came back as a *different* NaN and no longer
re-encoded to the same bytes — 8 blocks in Fallout 4 and 3 in Skyrim SE.
The payload is now kept by hand, in the wider format's mantissa where a
narrowing will not lose it, and the reader and the writer agree on where
it is.

**A Normbyte is an unsigned byte, not a signed one.** nif.xml says "a float
in the range -1.0:1.0, stored as a byte", and nifgen reads it as
`b / 127.5 - 1` — unsigned, scaled by 127.5 so that the byte's whole range
maps onto the float's. The reader used a signed byte scaled by 127, while
the renderer used the unsigned form, so the two disagreed about what a
normal was and a byte that was read was not the byte that was written.
Both now use the unsigned form.

**An enum-typed field with a length was read as one value.** Skyrim's
compressed mesh shapes store their welding information as an array of
`bhkWeldInfo`, an enum of sixteen bits, and reading one value instead of
the whole array left every shape short.

**A 32-bit float's NaN lost its payload too.** The same fault as the half's,
one size up: Skyrim's compressed mesh shapes hold float geometry, and a NaN
among them came back changed.

**A string before the string table is not nul-terminated.** From 20.1.0.3 a
NIF's string is an index into the file's table of strings. Before that it is
the text itself, written with a four-byte length in front of it. The reader
read it as a nul-terminated string, so the length was taken for the first
four characters of the text, and every block after the string was read at
the wrong place. One file in Fallout 3 and one in Fallout: New Vegas — the
same `meshes\triggers\collisionboxstatic.nif` — were the only ones in
either corpus old enough to be affected, and neither parsed at all.

## The Skyrim `BSLightingShaderProperty` defect

The defect that started the Skyrim work, kept here because it is the
clearest example of what a `bs_version` condition written for another game
does.

Skyrim LE reported 1,636 `BSLightingShaderProperty` blocks reading **96 of
100–128 bytes**. Measured against nif.xml, three things were wrong:

1. **`specular_color` (Color3) and `specular_strength` (float) were missing**
   between `glossiness` and `lighting_effect_1` — 16 bytes, the size of the
   shortfall.
2. **`shader_type` was not a field of the block at all.** nif.xml's conditions
   on this very block read "Shader Type", but `BSShaderProperty`'s own copy is
   `#NI_BS_LTE_FO3#` — Fallout 3 and earlier — so a Skyrim shape never had it
   and *every* `shader_type == N` condition was false. The whole conditional
   group was being skipped: the environment map scale, the parallax fields, the
   sparkle and eye parameters, the skin and hair tints, and max passes and
   scale.
3. `environment_map_scale_2` was a duplicate of `environment_map_scale` under a
   condition; nif.xml has one field with `cond="Shader Type == 1"`.

Fixed, Skyrim LE's sampled mismatches went **1,670 → 130**, and the last
130 were the payload and Normbyte faults above.
