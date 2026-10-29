# S1 findings

What the S1 checks have measured, and what they found. This is the working
record: each entry says what was measured, over how much, and what it means.

## S1-3 — schema coverage (exhaustive, all five games)

`niftypes` reads only a NIF header's type-name list and decodes nothing, so it
answers "does the schema know this block type?" cheaply and completely. Over
**306,674 files**:

| Game | Files | Distinct types | Undefined |
|---|---|---|---|
| fnv | 20,541 | 150 | **0** |
| fo3 | 17,162 | 145 | 4 |
| skyrimle | 21,476 | 145 | **0** |
| skyrimse | 22,761 | 143 | 2 |
| fo4 | 224,734 | 116 | **37** |

Fallout 4's 37 is the largest single gap in the plan. Its biggest is
`BSLightingShaderProperty` at **1,170,607 uses** — every lit shape in the game —
followed by `BSPackedCombinedSharedGeomDataExtra` (1,057,980) and
`BSMeshLODTriShape` (533,507). FO4's materials are therefore *unreadable*, not
misread, which makes S4 and S6 unachievable for that game until the layer
exists.

The smaller gaps: Fallout 3's `bhkConvexListShape` (71), `bhkAabbPhantom` (8),
`Lighting30ShaderProperty` (4), `NiFogProperty` (1); Skyrim SE's
`bhkCylinderShape` (70) and `bhkPlaneShape` (1).

## S1-1 / S1-4 — re-encode (sampled)

`nifsample` takes a stratified sample — one share of each data subtree, so a
bug confined to `meshes\weapons` is not missed by chance — and checks every
block of every sampled file.

| Game | Sampled files | Blocks | Unknown | Short | **Mismatch** |
|---|---|---|---|---|---|
| fnv | 537 | 16,982 | 0 | 0 | **0** |
| fo3 | 451 | 12,079 | 3 | 0 | **0** |
| skyrimle | 556 | 17,941 | 0 | 130 | **130** |
| skyrimse | 589 | — | — | — | running |
| fo4 | — | — | — | — | not run |

Fallout: New Vegas and Fallout 3 are clean: every block re-encodes
byte-identically. Fallout 3's three unknowns are the `bhkConvexListShape`
blocks from S1-3.

## The Skyrim `BSLightingShaderProperty` defect

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

Fixed, Skyrim LE's sampled mismatches went **1,670 → 130**.

### The remaining 130, read by hand

One block was dumped and decoded field by field. Its 120 bytes read cleanly as:

```
 0  name (string index)      11
 4  extra_data               -1
 8  controller               0
12  shader_flags             0xffffffff
16  shader_flags_2           0x82400301   <- nif.xml's documented default
20  shader_type              16810017
24  uv_offset                (0, 0)
32  uv_scale                 (1, 1)
40  texture_set              30
44  emissive_color           (0, 0, 0)
56  emissive_multiple        1.5
60  texture_clamp_mode       3
64  alpha                    1.0
68  refraction_strength      0.0
72  glossiness               80.0
76  specular_color           (0.157, 0.157, 0.157)
88  specular_strength        1.0
92  lighting_effect_1        0.3
96  lighting_effect_2        2.0
100 environment_map_scale    50.0
104 skin_tint_color          (0.5, 0.95, 0.95)
116 ...                      1.0          <- 4 bytes, block ends at 120
```

The fixed part is 100 bytes and the block is 120, so 20 bytes of conditional
fields are present. `environment_map_scale` at 100 is there **although
`shader_type` is 16810017, not 1** — so its condition is not what nif.xml says,
or `shader_type` is read from the wrong offset. The trailing 20 bytes are
`50.0, 0.5, 0.95, 0.95, 1.0`: that reads as `environment_map_scale` (50.0),
then `skin_tint_color` (0.5, 0.95, 0.95) — a full Color3 — and then 4 bytes
that cannot be `hair_tint_color`, which needs 12.

So either `hair_tint_color` is absent for this shape (making its condition
something other than "always"), or the group's order differs from nif.xml's.
The two candidates are worth one more measurement each, and the byte counts say
which: if `hair_tint_color` is absent, 120 bytes is exactly right and only the
condition is wrong; if it is present, the block is 8 bytes short of what this
schema would write, which is the 8-byte excess already measured.

**This is the open question, stated precisely rather than guessed at.**
