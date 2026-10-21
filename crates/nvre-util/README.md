# nvre-util

NVRE's own shared helpers. Three things, and nothing else:

- **`cp1252`** — Windows-1252, the 8-bit text encoding Bethesda's formats
  use. `decode`, `encode` (which refuses a character that has no
  Windows-1252 form rather than losing it) and `decode_z` for NUL-terminated
  strings.
- **`sha256`** — `digest`, `hex` and `to_hex`, for fingerprints of file
  contents and for printing raw bytes.
- **`Cursor`** — a bounds-checked little-endian reader over a byte slice,
  with `u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`u64`/`i64`/`f32`/`f64`, `bytes`,
  `tag` and `remaining`. Its `pos` is public because a reader often needs to
  look at where it is, rewind past a field it has already read, or report
  where a value came from.

Plus `tag_str`, which prints a four-byte signature readably.

## Why it is its own crate

These are *format* concerns rather than any one game's, and every crate in
the graph speaks them: the record reader, the NIF reader and the archive
reader all read Windows-1252 text, hash file contents and walk bytes with
the same cursor. None of them should have to know which game they are
reading.

This crate is NVRE's own code — not a copy of another project's. It was
written from the format requirements: Windows-1252 is the WHATWG encoding,
SHA-256 is FIPS 180-4, and the cursor is the obvious shape for a
little-endian reader.

## The rules

- **A read past the end is an error, not a panic.** Every read returns
  `Result<_, OutOfBounds>`, and a failed read leaves the cursor where it
  was. The record and NIF decoders rely on this: they read optimistically
  and rewind when the read turns out to be wrong.
- **The error says where it happened.** `OutOfBounds` carries the offset,
  the size wanted and the size available, so a decoder can report "read of
  4 bytes at offset 2,875,196 exceeds length 2,875,194" without guessing.
- **No game-specific knowledge.** Nothing here knows what a record or a
  block is.
