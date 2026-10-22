# fnv-dds

NVRE's DDS texture reader.

The crate keeps the name `fnv-dds` and its library the name `fnv_dds`, so
that the Bevy asset layer, which imports it as `fnv_dds`, links against this
one through the workspace patch table without a change to its source.

DDS is a DirectDraw Surface file: a four-byte `DDS ` magic, a 128-byte
header, then the pixel data.

## What is here

- **`DdsHeader`** — the size, the pixel format, the mip count and the
  cube-map flags.
- **`Dds`** — the whole texture, with `levels[face][mip]` slices into the
  payload, so a reader can find a level without decoding it.
- **`to_rgba8`** — the three uncompressed layouts (`A8R8G8B8`, `R8G8B8`,
  `A4R4G4B4`) converted to RGBA8. `None` for the block-compressed formats,
  which are not converted.
- **`decode_blocks`** — DXT1, DXT3 and DXT5 decoded on the CPU, for the few
  places that need the pixels themselves rather than the compressed blocks.

The block-compressed formats are meant to reach the GPU as they are; this
crate does not re-encode them.

## The rules

- **An unknown format is described, not guessed.** `PixelFormat::format`
  returns a description of the flags, the four-character code and the masks,
  so an unrecognised texture says what it is instead of failing opaquely.
- **The payload is checked against the layout.** `Dds::parse` refuses a file
  whose payload is not exactly the size the header's mip and face counts
  call for, because a texture that does not match its own header is a
  different format or a damaged file.
- **The surface is clipped, not padded.** `decode_blocks` writes only the
  texels inside the surface, so a 4×4 block on a 5×3 texture does not spill
  into the next row.

## Why it is its own crate

The texture layer is a format concern, not a game's — Skyrim and Fallout
ship the same DDS files. It was written for NVRE from the format's
description: the header layout, the RGB565 and RGBA layouts, and the DXT
block encodings.
