//! The NIF container: the header, the block table, the string table and the
//! groups.
//!
//! A NIF begins with a **text line** naming the format and its version,
//! then the binary header:
//!
//! - the file version, the endian byte and the user version;
//! - Bethesda's export information, in some versions;
//! - the **block types**: the names of the block types the file uses;
//! - the **block type index**: which type each block is;
//! - the **block sizes**, from 20.2.0.5 on;
//! - the **string table**, from 20.1.0.1 on;
//! - the **groups**.
//!
//! Every byte read is kept, so a writer can reproduce the header exactly.

use nvre_util::{cp1252, Cursor, OutOfBounds};

/// A packed file version, such as `0x14020007` for 20.2.0.7.
pub type Version = u32;

/// The version 20.0.0.4.
pub const V20_0_0_4: Version = 0x1400_0004;
/// The version 20.1.0.1.
pub const V20_1_0_1: Version = 0x1401_0001;
/// The version 20.1.0.3.
pub const V20_1_0_3: Version = 0x1401_0003;
/// The version 20.2.0.5.
pub const V20_2_0_5: Version = 0x1402_0005;
/// The version 20.2.0.7.
pub const V20_2_0_7: Version = 0x1402_0007;

/// Parse a dotted version such as `"20.2.0.7"` into its packed form.
pub fn parse_version(s: &str) -> Option<Version> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut v: u32 = 0;
    for p in parts {
        let n: u8 = p.parse().ok()?;
        v = (v << 8) | n as u32;
    }
    Some(v)
}

/// A packed version as its dotted form.
pub fn version_string(v: Version) -> String {
    format!(
        "{}.{}.{}.{}",
        v >> 24,
        (v >> 16) & 0xFF,
        (v >> 8) & 0xFF,
        v & 0xFF
    )
}

/// A fault in a NIF.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NifError {
    /// The file does not begin with a NIF header line.
    #[error("not a NIF: {0}")]
    NotNif(String),
    /// The file's version is one this reader does not know.
    #[error("unsupported NIF version {0}")]
    Unsupported(String),
    /// The file ended before the data did.
    #[error("truncated: {0}")]
    Truncated(#[from] OutOfBounds),
    /// The file says something that cannot be true.
    #[error("{0}")]
    Invalid(String),
}

/// A length-prefixed string, kept as the bytes it was.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawString(pub Vec<u8>);

impl RawString {
    /// The string as text.
    pub fn text(&self) -> String {
        cp1252::decode(&self.0)
    }
}

/// Bethesda's export information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsHeader {
    /// The game version, which the schema's conditions are written against.
    pub bs_version: u32,
    /// The export strings, raw: a length byte and then the bytes.
    pub author: Vec<u8>,
    /// Present in some versions.
    pub unknown_int: Option<u32>,
    /// Present in some versions.
    pub process_script: Option<Vec<u8>>,
    /// The export script.
    pub export_script: Vec<u8>,
    /// Present in some versions.
    pub max_filepath: Option<Vec<u8>>,
}

impl BsHeader {
    /// The author string as text.
    pub fn author_text(&self) -> String {
        export_text(&self.author)
    }
}

/// The text of an export string: the length byte is skipped, and the text
/// stops at the NUL.
pub fn export_text(raw: &[u8]) -> String {
    cp1252::decode_z(raw.get(1..).unwrap_or_default())
}

/// The file's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The header line, as the bytes it was.
    pub header_string: Vec<u8>,
    /// The file version.
    pub version: Version,
    /// The endian byte.
    pub endian: u8,
    /// The user version, which the schema's conditions may also use.
    pub user_version: u32,
    /// How many blocks the file holds.
    pub num_blocks: u32,
    /// Bethesda's export information, when the file has it.
    pub bs: Option<BsHeader>,
    /// The names of the block types the file uses.
    pub block_types: Vec<RawString>,
    /// Which type each block is.
    pub block_type_index: Vec<u16>,
    /// The size of each block, when the file records them.
    pub block_sizes: Option<Vec<u32>>,
    /// The length of the longest string, when the file has a string table.
    pub max_string_length: Option<u32>,
    /// The string table, when the file has one.
    pub strings: Option<Vec<RawString>>,
    /// The groups: how many blocks each group holds.
    pub groups: Vec<u32>,
    /// Where the header ends.
    pub header_len: usize,
}

impl Header {
    /// Read the header.
    pub fn parse(data: &[u8]) -> Result<Header, NifError> {
        let mut c = Cursor::new(data);
        // The header line: up to the first newline, within 128 bytes.
        let rest = &c.data[c.pos..];
        let nl = rest
            .iter()
            .take(128)
            .position(|&b| b == 0x0A)
            .ok_or_else(|| NifError::NotNif("no header line within 128 bytes".into()))?;
        let header_string = rest[..nl].to_vec();
        let line = String::from_utf8_lossy(&header_string).into_owned();
        if !line.starts_with("Gamebryo File Format, Version ")
            && !line.starts_with("NetImmerse File Format")
        {
            return Err(NifError::NotNif(format!("header line {line:?}")));
        }
        c.pos += nl + 1;
        let version = c.u32()?;
        if !(V20_0_0_4..=V20_2_0_7).contains(&version) {
            return Err(NifError::Unsupported(version_string(version)));
        }
        let endian = c.u8()?;
        if endian != 1 {
            return Err(NifError::Unsupported(format!(
                "big-endian file (endian byte {endian})"
            )));
        }
        let user_version = c.u32()?;
        let num_blocks = c.u32()?;

        // Bethesda's export information, in some versions.
        let has_bs = user_version >= 3
            && (version == V20_2_0_7
                || version == 0x1400_0005
                || (version == V20_0_0_4 && user_version <= 11));
        let bs = if has_bs {
            let bs_version = c.u32()?;
            let author = export_string(&mut c)?;
            let unknown_int = if bs_version > 130 {
                Some(c.u32()?)
            } else {
                None
            };
            let process_script = if bs_version < 131 {
                Some(export_string(&mut c)?)
            } else {
                None
            };
            let export_script = export_string(&mut c)?;
            let max_filepath = if bs_version >= 103 {
                Some(export_string(&mut c)?)
            } else {
                None
            };
            Some(BsHeader {
                bs_version,
                author,
                unknown_int,
                process_script,
                export_script,
                max_filepath,
            })
        } else {
            None
        };

        // The block types: the names the file's blocks are drawn from.
        let num_types = c.u16()?;
        let mut block_types = Vec::with_capacity(num_types as usize);
        for _ in 0..num_types {
            block_types.push(sized_string(&mut c)?);
        }

        // Which type each block is.
        let n = num_blocks as usize;
        if n > c.remaining() / 2 {
            return Err(NifError::Invalid(format!(
                "{num_blocks} blocks cannot fit in the file"
            )));
        }
        let mut block_type_index = Vec::with_capacity(n);
        for i in 0..n {
            let t = c.u16()?;
            if (t & 0x7FFF) as usize >= block_types.len() {
                return Err(NifError::Invalid(format!(
                    "block {i} has type index {t} of {num_types}"
                )));
            }
            block_type_index.push(t);
        }

        // The block sizes, from 20.2.0.5 on.
        let block_sizes = if version >= V20_2_0_5 {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(c.u32()?);
            }
            Some(v)
        } else {
            None
        };

        // The string table, from 20.1.0.1 on.
        let (max_string_length, strings) = if version >= V20_1_0_1 {
            let count = c.u32()?;
            let max = c.u32()?;
            if count as usize > c.remaining() / 4 {
                return Err(NifError::Invalid(format!(
                    "{count} strings cannot fit in the file"
                )));
            }
            let mut v = Vec::with_capacity(count as usize);
            for _ in 0..count {
                v.push(sized_string(&mut c)?);
            }
            (Some(max), Some(v))
        } else {
            (None, None)
        };

        // The groups.
        let num_groups = c.u32()?;
        if num_groups as usize > c.remaining() / 4 {
            return Err(NifError::Invalid(format!(
                "{num_groups} groups cannot fit in the file"
            )));
        }
        let mut groups = Vec::with_capacity(num_groups as usize);
        for _ in 0..num_groups {
            groups.push(c.u32()?);
        }
        let header_len = c.pos;
        Ok(Header {
            header_string,
            version,
            endian,
            user_version,
            num_blocks,
            bs,
            block_types,
            block_type_index,
            block_sizes,
            max_string_length,
            strings,
            groups,
            header_len,
        })
    }

    /// The game version the schema's conditions are written against.
    pub fn bs_version(&self) -> u32 {
        self.bs.as_ref().map(|b| b.bs_version).unwrap_or(0)
    }

    /// The name of the type of block `i`.
    ///
    /// The high bit of the index means the block is the last one, and is not
    /// part of the index itself.
    pub fn block_type_name(&self, i: usize) -> Option<String> {
        let idx = (*self.block_type_index.get(i)? as usize) & 0x7FFF;
        self.block_types
            .get(idx)
            .map(|s| s.text().trim_end_matches('\0').to_string())
    }
}

/// A string of a four-byte length and that many bytes.
fn sized_string(c: &mut Cursor<'_>) -> Result<RawString, OutOfBounds> {
    let len = c.u32()? as usize;
    Ok(RawString(c.bytes(len)?.to_vec()))
}

/// An export string: a length byte and that many bytes.
fn export_string(c: &mut Cursor<'_>) -> Result<Vec<u8>, OutOfBounds> {
    let len = c.u8()? as usize;
    Ok(c.bytes(len)?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_round_trips() {
        assert_eq!(parse_version("20.2.0.7"), Some(V20_2_0_7));
        assert_eq!(version_string(V20_2_0_7), "20.2.0.7");
        assert_eq!(parse_version("20.2.0"), None);
    }

    #[test]
    fn a_file_that_is_not_a_nif_is_refused() {
        let e = Header::parse(b"NOTHING HERE").unwrap_err();
        assert!(matches!(e, NifError::NotNif(_)));
    }

    #[test]
    fn an_export_string_stops_at_the_nul() {
        assert_eq!(export_text(b"\x05abc\0"), "abc");
    }
}
