# FO4 precombined geometry: the CSG format, reverse-engineered

FO4 stores the geometry of its pre-combined world (`meshes\precombined\*_oc.nif`)
outside the NIF. This is what the file actually contains and how to read it.

## What the NIF holds

A precombined NIF's `BSTriShape` blocks are **stubs**: `data_size` 0, a
`num_vertices` that is declared but not carried, and a pointer at the shared
data instead. The geometry is in a `BSPackedCombinedSharedGeomDataExtra` — an
`NiExtraData` that carries:

| Field | Meaning |
|---|---|
| `vertex_desc` | the packed-vertex layout (`BSVertexDesc`), shared by the file's shapes |
| `num_vertices` / `num_triangles` | totals for this extra's shapes |
| `num_data` | how many entries follow |
| `object[]` | `{filename_hash, data_offset}` — where the geometry lives |
| `object_data[]` | `BSPackedSharedGeomData`: per-object `num_verts`, LOD triangle counts/offsets, `num_combined` transforms, and its own `vertex_desc` |

nif.xml says it outright: *"Geometry is NOT baked into the file. It is instead a
reference to the geometry via a CSG filename hash and data offset."*

## The CSG container

`Fallout4 - Geometry.csg` (240 MB) and its five DLC siblings (~480 MB total):

```
offset 0    "bcsg"
offset 4    u32 version            0x7e72
offset 8    u32 chunk_count        6841 for the base game
offset 12   chunk_count x { u32 compressed_len, u32 compressed_offset }
            ...then the zlib streams at those offsets
```

Measured on `Fallout4 - Geometry.csg`:

- The chunks are **contiguous**: chunk *i*'s data ends exactly where chunk
  *i+1* begins (verified: 702140 + 53818 = 755958, chunk 1's offset).
- Every chunk is a zlib stream (`78 9c`) that inflates to **exactly 65,536
  bytes**, except the **last**, which inflates to 46,894. That is the invariant
  the FO4 audit issue `FO4-D1-01` describes, and it holds on real data.
- Total uncompressed: 6840 x 65536 + 46894 = **448,313,134 bytes**.

The concatenation of the inflated chunks is the **PSG stream**, and
`data_offset` addresses into it.

## Reading a shape's geometry

1. `filename_hash` (a BSCRC32 of the CSG filename, without the extension)
   selects the CSG file. `data_offset` is a PSG offset.
2. PSG offset -> `(chunk, local)` by `idx = off / 65536`, `local = off % 65536`.
   Inflate that chunk (cache it — one chunk serves many shapes).
3. At `local` sits a `BSVertexData` record per the `vertex_desc` the NIF names.
   The existing `unpack_structs` reader already handles this layout; what was
   missing was the bytes to feed it.

Worked example, `meshes\precombined\00000fc9_01831dbf_oc.nif`:

```
object[0] = { filename_hash: 3723598439, data_offset: 58753758 }
object_data[0].vertex_desc = 1037939064898054   (attrs 59, stride 24)
object_data[0].num_verts = 9, tri_count_lod0 = 8
```

PSG 58753758 -> chunk 896, local 33502. The bytes there:

```
00 00 00 d8  00 00 00 3c  00 38 00 3c  7f 7f ff 7f  7f 00 7f 7f  ff ff ff ff 00
```

A half-float position (`d800` = -6.5, `0000` = 0, `3c00` = 1.0), half UVs
(`0038`, `003c`), then Normbyte normal / bitangent / tangent / bitangent — the
`BSVertexData` layout the desc describes, stride 24, exactly.

## What this means for the editor

The 540,965 FO4 shapes `geomsurvey` reported as having no UV set are **correctly
read**: the NIF carries no vertex data for them. The gap is that the editor does
not open the CSG, so FO4's pre-combined world — the bulk of its exteriors —
draws nothing. That is the single largest remaining gap in FO4's rendering, and
it is a reader to write rather than a bug to fix:

1. A CSG reader: parse the header, index the chunk table, inflate and cache
   chunks, expose `read_psg(offset, len)`.
2. Wire it into the VFS the way BSA/BA2 already are, so `GameData` can resolve
   a `filename_hash` to a CSG in the load order — noting the audit's warning
   that a DLC cell's objects may reference the **base** CSG, so the hash must
   be resolved across all six files, not just the cell's own.
3. In `ModelCtx::prepare`, when a shape has no geometry of its own, look for a
   `BSPackedCombinedSharedGeomDataExtra` on its parent node and build the mesh
   from the CSG bytes it names.
