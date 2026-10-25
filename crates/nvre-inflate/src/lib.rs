//! NVRE's zlib reader.
//!
//! zlib is RFC 1950: a two-byte header, a DEFLATE body (RFC 1951), and a
//! four-byte Adler-32 trailer.
//!
//! Two things here are not what a general-purpose inflater does, and both
//! come from how Bethesda's shipped data actually behaves:
//!
//! - **A missing or wrong trailer is reported, not rejected.** Shipped game
//!   data contains streams that end right after the final deflate block with
//!   no Adler-32 at all, and streams whose Adler-32 is wrong even though the
//!   deflate data is complete and the output is exactly the expected size.
//!   Rejecting them would make those records unreadable, so the reader
//!   reports what it found in [`Trailer`] and lets the caller decide.
//! - **A wrong checksum is only tolerated when the size is known.** If the
//!   caller says how many bytes to expect and the output matches exactly, a
//!   wrong Adler-32 is an anomaly to report. If the size is not known, a
//!   wrong Adler-32 is the only evidence of damage there is, so it is an
//!   error.
//!
//! The DEFLATE body is inflated by `flate2` in raw mode with the zlib-rs
//! backend, because miniz_oxide in streaming mode does not reject
//! back-references that reach before the start of the output.

use flate2::{Decompress, FlushDecompress, Status};

/// A fault in a compressed stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InflateError {
    /// The stream ended before the data did.
    #[error("compressed stream is truncated")]
    Truncated,
    /// The two-byte zlib header was not valid.
    #[error("invalid zlib header")]
    BadZlibHeader,
    /// The stream asked for a preset dictionary, which is not supported.
    #[error("zlib preset dictionaries are not supported")]
    PresetDictionary,
    /// The deflate data itself was damaged.
    #[error("invalid deflate data: {0}")]
    Corrupt(String),
    /// The Adler-32 did not match, and there was no other evidence of damage.
    #[error("adler32 mismatch: stored {expected:08x}, computed {actual:08x}")]
    ChecksumMismatch {
        /// The Adler-32 the stream carried.
        expected: u32,
        /// The Adler-32 of the data that came out.
        actual: u32,
    },
    /// The output was not the size the caller expected.
    #[error("decompressed size {actual} != expected {expected}")]
    SizeMismatch {
        /// The size the caller expected.
        expected: usize,
        /// The size that came out.
        actual: usize,
    },
    /// The output was larger than the caller allowed.
    #[error("decompressed output exceeds limit")]
    OutputLimit,
}

/// The Adler-32 checksum of `data` (RFC 1950, section 8.2).
pub fn adler32(data: &[u8]) -> u32 {
    adler2::adler32_slice(data)
}

fn corrupt(e: flate2::DecompressError) -> InflateError {
    // zlib-rs names the specific fault ("invalid distance too far back", ...)
    // only when it is hit at the start of a call; mid-call it reports its
    // internal state instead.
    let msg = match e.message() {
        Some(m) if !m.contains("bad state") => m.to_string(),
        _ => "corrupt deflate stream".to_string(),
    };
    InflateError::Corrupt(msg)
}

/// Inflate a raw DEFLATE stream.
///
/// Returns the output and the number of input bytes consumed. Bytes after the
/// final block — a zlib trailer, say — are not consumed.
pub fn inflate_raw(
    input: &[u8],
    size_hint: usize,
    limit: usize,
) -> Result<(Vec<u8>, usize), InflateError> {
    // Never trust a declared size for preallocation: cap it by what the input
    // could plausibly expand to. DEFLATE's maximum ratio is about 1032:1, so
    // 64x covers typical data and the Vec grows beyond that if needed.
    let cap = size_hint
        .min(limit)
        .min(input.len().saturating_mul(64).max(4096));
    // One byte of headroom past `limit` distinguishes "exactly limit" from
    // "more than limit".
    let hard = limit.saturating_add(1);
    let mut out: Vec<u8> = Vec::with_capacity(cap.min(hard));
    let mut d = Decompress::new(false);
    loop {
        if out.len() == out.capacity() {
            let grow = out.capacity().max(4096).min(hard - out.len());
            out.reserve_exact(grow);
        }
        let (in_before, out_before) = (d.total_in(), d.total_out());
        let status = d
            .decompress_vec(
                &input[d.total_in() as usize..],
                &mut out,
                FlushDecompress::None,
            )
            .map_err(corrupt)?;
        if out.len() > limit {
            return Err(InflateError::OutputLimit);
        }
        match status {
            Status::StreamEnd => break,
            Status::Ok | Status::BufError => {
                let stalled = d.total_in() == in_before && d.total_out() == out_before;
                if stalled && out.len() < out.capacity() {
                    return Err(InflateError::Truncated);
                }
            }
        }
    }
    Ok((out, d.total_in() as usize))
}

/// Inflate a raw DEFLATE stream, stopping once `want` bytes have come out.
///
/// For a caller that needs the *start* of a stream and not the whole thing: a
/// record's payload can decompress to tens of kilobytes, and reading only its
/// first subrecord should not cost the rest. [`inflate_raw`] cannot serve this —
/// it treats `limit` as a ceiling to refuse past, discarding what it produced —
/// so this returns the prefix instead.
///
/// The result may be shorter than `want` when the stream ends first, which is not
/// an error: a short stream is a short stream. A stream that is corrupt still
/// fails, because a caller reading a prefix is reading a real record, not
/// guessing.
pub fn inflate_prefix(input: &[u8], want: usize) -> Result<Vec<u8>, InflateError> {
    let mut out: Vec<u8> = Vec::with_capacity(want.min(input.len().saturating_mul(8)).max(256));
    let mut d = Decompress::new(false);
    loop {
        if out.len() >= want {
            break;
        }
        if out.len() == out.capacity() {
            out.reserve_exact(want - out.len());
        }
        let (in_before, out_before) = (d.total_in(), d.total_out());
        let status = d
            .decompress_vec(
                &input[d.total_in() as usize..],
                &mut out,
                FlushDecompress::None,
            )
            .map_err(corrupt)?;
        if matches!(status, Status::StreamEnd) {
            break;
        }
        // A stalled inflate with room to spare is a truncated stream.
        if d.total_in() == in_before && d.total_out() == out_before && out.len() < out.capacity() {
            return Err(InflateError::Truncated);
        }
        // Input exhausted without the stream ending: the prefix is what there is.
        if d.total_in() as usize >= input.len() {
            break;
        }
    }
    out.truncate(want);
    Ok(out)
}

/// What was found after the deflate data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trailer {
    /// An Adler-32 was present and correct.
    Verified,
    /// The stream ended right after the final deflate block, with no
    /// Adler-32 at all.
    Missing,
    /// An Adler-32 was present but wrong, while the deflate stream itself
    /// ended cleanly and produced exactly the expected size.
    Mismatch {
        /// The Adler-32 the stream carried.
        stored: u32,
        /// The Adler-32 of the data that came out.
        computed: u32,
    },
}

/// Decompress a zlib stream, verifying the Adler-32 trailer.
///
/// If `expected_len` is given, the output size must match it exactly.
pub fn zlib_decompress(input: &[u8], expected_len: Option<usize>) -> Result<Vec<u8>, InflateError> {
    match zlib_decompress_ex(input, expected_len)? {
        (out, Trailer::Verified) => Ok(out),
        (_, Trailer::Missing) => Err(InflateError::Truncated),
        (_, Trailer::Mismatch { stored, computed }) => Err(InflateError::ChecksumMismatch {
            expected: stored,
            actual: computed,
        }),
    }
}

/// Decompress a zlib stream, reporting trailer problems instead of failing
/// on them.
///
/// A missing Adler-32, or a wrong one when the deflate stream is complete
/// and the output is the expected size, comes back in the [`Trailer`]. A
/// partial trailer and a size mismatch are still errors.
pub fn zlib_decompress_ex(
    input: &[u8],
    expected_len: Option<usize>,
) -> Result<(Vec<u8>, Trailer), InflateError> {
    if input.len() < 3 {
        return Err(InflateError::Truncated);
    }
    let cmf = input[0];
    let flg = input[1];
    // The low four bits of CMF are the compression method, which is 8 for
    // deflate. The high four bits are the window size, which cannot exceed
    // 7 (a 32 KiB window) in the encoding RFC 1950 defines. And the two
    // header bytes together, read big-endian, must be a multiple of 31.
    if cmf & 0x0F != 8 || (cmf >> 4) > 7 || !((cmf as u16) << 8 | flg as u16).is_multiple_of(31) {
        return Err(InflateError::BadZlibHeader);
    }
    // FDICT means a preset dictionary follows the header.
    if flg & 0x20 != 0 {
        return Err(InflateError::PresetDictionary);
    }
    let limit = expected_len.unwrap_or(1 << 31);
    let (out, used) = inflate_raw(&input[2..], expected_len.unwrap_or(input.len() * 4), limit)?;
    if let Some(e) = expected_len {
        if e != out.len() {
            return Err(InflateError::SizeMismatch {
                expected: e,
                actual: out.len(),
            });
        }
    }
    // The trailer sits immediately after the deflate data.
    let tpos = 2 + used;
    if tpos == input.len() {
        return Ok((out, Trailer::Missing));
    }
    if tpos + 4 > input.len() {
        return Err(InflateError::Truncated);
    }
    let stored = u32::from_be_bytes([
        input[tpos],
        input[tpos + 1],
        input[tpos + 2],
        input[tpos + 3],
    ]);
    let actual = adler32(&out);
    if stored != actual {
        if expected_len.is_some() {
            return Ok((
                out,
                Trailer::Mismatch {
                    stored,
                    computed: actual,
                },
            ));
        }
        return Err(InflateError::ChecksumMismatch {
            expected: stored,
            actual,
        });
    }
    Ok((out, Trailer::Verified))
}

/// Encode `data` as a zlib stream of *stored* (uncompressed) deflate blocks.
///
/// The result is a valid zlib stream that every inflater accepts. It exists
/// so that a modified record can be written back compressed. Unmodified data
/// should be written from the original bytes, to stay byte-identical.
pub fn zlib_store(data: &[u8]) -> Vec<u8> {
    // The zlib header: deflate, a 32 KiB window, no preset dictionary, and
    // the two bytes a multiple of 31.
    let mut out = vec![0x78, 0x01];
    if data.is_empty() {
        // One empty stored block, and then the final block.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        // Stored blocks hold at most 65535 bytes each.
        for chunk in data.chunks(0xFFFF) {
            let last = chunk.len() < 0xFFFF;
            out.push(if last { 1 } else { 0 });
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn a_stored_stream_round_trips() {
        for data in [
            b"".as_slice(),
            b"a",
            b"hello, world",
            &[0u8; 1000],
            &[0xABu8; 70_000],
        ] {
            let z = zlib_store(data);
            let (out, trailer) = zlib_decompress_ex(&z, Some(data.len())).unwrap();
            assert_eq!(out, data);
            assert_eq!(trailer, Trailer::Verified);
        }
    }

    #[test]
    fn a_missing_trailer_is_reported() {
        let mut z = zlib_store(b"hello");
        z.truncate(z.len() - 4);
        let (out, trailer) = zlib_decompress_ex(&z, Some(5)).unwrap();
        assert_eq!(out, b"hello");
        assert_eq!(trailer, Trailer::Missing);
    }

    #[test]
    fn a_wrong_trailer_is_reported_when_the_size_is_known() {
        let mut z = zlib_store(b"hello");
        let n = z.len();
        z[n - 1] ^= 0xFF;
        let (out, trailer) = zlib_decompress_ex(&z, Some(5)).unwrap();
        assert_eq!(out, b"hello");
        assert!(matches!(trailer, Trailer::Mismatch { .. }));
        // Without a known size, the same stream is an error.
        assert!(matches!(
            zlib_decompress_ex(&z, None),
            Err(InflateError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_bad_header_is_refused() {
        assert_eq!(
            zlib_decompress_ex(&[0x00, 0x00, 0x00], None),
            Err(InflateError::BadZlibHeader)
        );
    }

    #[test]
    fn a_size_mismatch_is_an_error() {
        let z = zlib_store(b"hello");
        assert!(matches!(
            zlib_decompress_ex(&z, Some(6)),
            Err(InflateError::SizeMismatch { .. })
        ));
    }

    proptest! {
        /// Whatever a stored stream holds comes back out of the reader.
        #[test]
        fn a_stored_stream_survives_the_round_trip(
            data in proptest::collection::vec(any::<u8>(), 0..5000),
        ) {
            let z = zlib_store(&data);
            let (out, trailer) = zlib_decompress_ex(&z, Some(data.len())).unwrap();
            prop_assert_eq!(out, data);
            prop_assert_eq!(trailer, Trailer::Verified);
        }
    }
}
