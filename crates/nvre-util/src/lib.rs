//! NVRE's shared helpers.
//!
//! Three things, and nothing else:
//!
//! - [`cp1252`] — Windows-1252, the 8-bit text encoding Bethesda's formats use.
//! - [`sha256`] — SHA-256, for fingerprints of file contents.
//! - [`Cursor`] — a bounds-checked little-endian reader over a byte slice.
//!
//! Every one of these is a *format* concern rather than a game's, which is
//! why they live in their own crate: the record readers, the NIF reader and
//! the archive reader all speak them, and none of them should have to know
//! which game they are reading.

pub mod cp1252 {
    use encoding_rs::WINDOWS_1252;

    /// Decode Windows-1252 bytes into text.
    ///
    /// The five undefined slots (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to the
    /// C1 control of the same value, as the WHATWG encoding standard has
    /// them, so a decode never loses a byte.
    pub fn decode(bytes: &[u8]) -> String {
        WINDOWS_1252
            .decode_without_bom_handling(bytes)
            .0
            .into_owned()
    }

    /// Encode text as Windows-1252 bytes.
    ///
    /// `None` when a character has no Windows-1252 representation, so a
    /// writer can refuse rather than lose it.
    pub fn encode(s: &str) -> Option<Vec<u8>> {
        let (out, _, unmappable) = WINDOWS_1252.encode(s);
        (!unmappable).then(|| out.into_owned())
    }

    /// Decode a NUL-terminated string: stops at the first NUL.
    pub fn decode_z(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        decode(&bytes[..end])
    }
}

pub mod sha256 {
    use sha2::{Digest, Sha256};

    /// The SHA-256 digest of `data`.
    pub fn digest(data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    /// The SHA-256 digest of `data`, as lower-case hex.
    pub fn hex(data: &[u8]) -> String {
        to_hex(&digest(data))
    }

    /// Bytes as lower-case hex.
    pub fn to_hex(b: &[u8]) -> String {
        hex::encode(b)
    }
}

/// A read past the end of the slice.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("read of {wanted} bytes at offset {pos} exceeds length {len}")]
pub struct OutOfBounds {
    /// Where the read started.
    pub pos: usize,
    /// How many bytes were wanted.
    pub wanted: usize,
    /// How many bytes there are.
    pub len: usize,
}

/// A bounds-checked little-endian reader over a byte slice.
///
/// The position is public because a reader often needs to look at where it
/// is, to rewind past a field it has already read, or to report where a
/// value came from.
#[derive(Clone)]
pub struct Cursor<'a> {
    /// The bytes being read.
    pub data: &'a [u8],
    /// How far into them the reader is.
    pub pos: usize,
}

macro_rules! read_le {
    ($name:ident, $ty:ty, $size:expr) => {
        /// Read a little-endian value.
        pub fn $name(&mut self) -> Result<$ty, OutOfBounds> {
            let b = self.bytes($size)?;
            Ok(<$ty>::from_le_bytes(b.try_into().unwrap()))
        }
    };
}

impl<'a> Cursor<'a> {
    /// A reader over `data`, positioned at its start.
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    /// How many bytes are left.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Whether the reader is at the end.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Read `n` bytes, advancing past them.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], OutOfBounds> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len());
        let Some(end) = end else {
            return Err(OutOfBounds {
                pos: self.pos,
                wanted: n,
                len: self.data.len(),
            });
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// Read four bytes as a little-endian number, without moving past them.
    ///
    /// A union whose variant is chosen by the field's first word reads the
    /// word this way: the word belongs to the field either way.
    pub fn peek_u32(&self) -> Result<u32, OutOfBounds> {
        let end = self.pos.checked_add(4).filter(|e| *e <= self.data.len());
        let Some(end) = end else {
            return Err(OutOfBounds {
                pos: self.pos,
                wanted: 4,
                len: self.data.len(),
            });
        };
        Ok(u32::from_le_bytes(
            self.data[self.pos..end].try_into().unwrap(),
        ))
    }

    /// Read a four-byte signature.
    pub fn tag(&mut self) -> Result<[u8; 4], OutOfBounds> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }

    read_le!(u8, u8, 1);
    read_le!(i8, i8, 1);
    read_le!(u16, u16, 2);
    read_le!(i16, i16, 2);
    read_le!(u32, u32, 4);
    read_le!(i32, i32, 4);
    read_le!(u64, u64, 8);
    read_le!(i64, i64, 8);
    read_le!(f32, f32, 4);
    read_le!(f64, f64, 8);
}

/// Printable form of a four-byte signature: the printable bytes as they are,
/// and the rest as `\xNN`.
pub fn tag_str(t: &[u8; 4]) -> String {
    let mut s = String::with_capacity(4);
    for &b in t {
        if (0x20..0x7F).contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn the_cursor_reads_little_endian() {
        let mut c = Cursor::new(&[1, 0, 0, 0, 0xFF, 0xFF]);
        assert_eq!(c.u32().unwrap(), 1);
        assert_eq!(c.u16().unwrap(), 0xFFFF);
        assert!(c.is_empty());
    }

    #[test]
    fn a_read_past_the_end_is_an_error() {
        let mut c = Cursor::new(&[1, 2]);
        let e = c.u32().unwrap_err();
        assert_eq!(e.pos, 0);
        assert_eq!(e.wanted, 4);
        assert_eq!(e.len, 2);
        // And the cursor did not move.
        assert_eq!(c.pos, 0);
    }

    #[test]
    fn windows_1252_round_trips() {
        for s in ["Hello", "café", "naïve", "Ünicode", "FNV"] {
            let bytes = cp1252::encode(s).unwrap();
            assert_eq!(cp1252::decode(&bytes), s);
        }
        // A character with no Windows-1252 form is refused, not mangled.
        assert!(cp1252::encode("日本").is_none());
    }

    #[test]
    fn a_nul_terminated_string_stops_at_the_nul() {
        assert_eq!(cp1252::decode_z(b"abc\0def"), "abc");
        assert_eq!(cp1252::decode_z(b"abc"), "abc");
    }

    #[test]
    fn a_signature_prints_readably() {
        assert_eq!(tag_str(b"CREA"), "CREA");
        assert_eq!(tag_str(&[0x01, 0x02, b'A', b'B']), "\\x01\\x02AB");
    }

    proptest! {
        /// Whatever a cursor reads, it reads the same bytes.
        #[test]
        fn a_cursor_reads_what_the_slice_holds(
            data in proptest::collection::vec(any::<u8>(), 0..64),
            at in 0usize..64,
        ) {
            let mut c = Cursor::new(&data);
            if at < data.len() {
                c.pos = at;
                let rest = data.len() - at;
                assert_eq!(c.remaining(), rest);
                assert_eq!(c.bytes(rest).unwrap(), &data[at..]);
            }
        }
    }
}
