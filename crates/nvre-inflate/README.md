# nvre-inflate

NVRE's zlib reader.

zlib is RFC 1950: a two-byte header, a DEFLATE body (RFC 1951), and a
four-byte Adler-32 trailer.

## What is here

- **`zlib_decompress`** — decode a zlib stream, verifying the trailer.
- **`zlib_decompress_ex`** — the same, but trailer problems come back in the
  result rather than as an error.
- **`inflate_raw`** — a raw DEFLATE stream, returning the bytes consumed.
- **`zlib_store`** — encode as *stored* (uncompressed) deflate blocks.
- **`adler32`** — the checksum.

## The two rules that are not ordinary

Shipped game data contains streams that break the ordinary rules, and a
reader that rejects them makes those records unreadable.

- **A missing trailer is reported, not rejected.** Some shipped streams end
  right after the final deflate block, with no Adler-32 at all.
- **A wrong trailer is reported, not rejected — but only when the size is
  known.** A stream whose Adler-32 is wrong but whose deflate data is
  complete and whose output is exactly the expected size is an anomaly to
  report, not damage. If the size is *not* known, the wrong Adler-32 is the
  only evidence of damage there is, so it is an error.

A partial trailer and a size mismatch are always errors.

The trailer status comes back as a [`Trailer`], so the caller can log the
anomaly and carry on, or refuse the record.

## Why the encoder only emits stored blocks

A stored block is one the encoder does not have to compress, and a stream
of them is valid zlib that every inflater accepts. It exists so a modified
record can be written back compressed.

**Unmodified data should be written from its original bytes**, to stay
byte-identical — a stored re-encoding is valid but not the same bytes, and
the round-trip check exists to catch exactly that.
