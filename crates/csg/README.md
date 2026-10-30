# csg

Reader for Bethesda CSG (`.csg` / `.psg`) precombined shared geometry archives shipped with Fallout 4.

A CSG file (such as `Fallout4 - Geometry.csg` or `DLCCoast - Geometry.csg`) externalizes vertex and triangle buffers for precombined cell architecture (`meshes\precombined\*_oc.nif`).

## Format Overview

```text
offset 0    char[4]        magic = "bcsg"
offset 4    u32            num_objects
offset 8    u32            num_chunks
offset 12   ChunkEntry[num_chunks]       8 bytes each (u32 compressed_size, u32 file_offset)
...         ObjectEntry[num_objects]     20 bytes each
...         zlib stream[num_chunks]      compressed payload chunks
```

Every chunk except the final one decompresses to exactly 65,536 bytes. The concatenation of all decompressed chunks forms the contiguous PSG byte stream addressed by `BSPackedGeomObject.data_offset`.
The companion `filename_hash` is computed using Bethesda's `BSCRC32` over `"<plugin stem> - geometry"`.
