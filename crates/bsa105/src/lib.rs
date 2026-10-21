//! Reader for Bethesda archives as shipped with Skyrim Special Edition
//! (archive version 105).
//!
//! Version 105 is version 104 with two differences, both documented on UESP's
//! *Skyrim Mod:Archive File Format* page and both confirmed against the user's
//! own install before any code was written:
//!
//! 1. **A folder record is 24 bytes, not 16.** v104's is
//!    `hash u64, count u32, offset u32`; v105 inserts a `u32` after `count` and
//!    another after `offset`. The header, the file record, the name block and
//!    the data blocks are unchanged — which is why reading a v105 archive with
//!    a v104 reader does not fail loudly, it silently desyncs on the first
//!    folder name.
//! 2. **A compressed file block's payload is an LZ4 frame**, where v104's is
//!    zlib. Measured: `Skyrim - Meshes0.bsa`'s first file carries
//!    `04 22 4d 18` after its `u32` original size (the LZ4 frame magic), while
//!    `Fallout - Meshes.bsa`'s carries `78 9c` (zlib). So the version selects
//!    the codec, and an archive whose `COMPRESSED_DEFAULT` flag is clear holds
//!    raw blocks either way.
//!
//! Everything else follows v104, because that is what the format does: the
//! archive flag bits, the name hash, the size field's compression-toggle bit,
//! the embedded-name prefix. Those are restated here rather than imported
//! because this crate deliberately does not depend on NVForever's `fnv-bsa` —
//! it is a sibling, not a fork, and the workspace's `[patch]` table decides
//! which one a consumer resolves.
//!
//! Tolerance follows the same rule `fnv-bsa` uses: an anomaly becomes a
//! [`Warning`], never a panic, and an unknown flag bit is preserved and
//! reported rather than dropped.

use fnv_util::{cp1252, Cursor};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const MAGIC: [u8; 4] = *b"BSA\0";
pub const HEADER_SIZE: u32 = 36;

/// The archive version this crate reads.
pub const VERSION: u32 = 105;

/// Archive flag bits. Names follow public community documentation; the two
/// marked "unconfirmed" have no observed layout effect.
pub mod flags {
    pub const DIR_NAMES: u32 = 0x1;
    pub const FILE_NAMES: u32 = 0x2;
    pub const COMPRESSED_DEFAULT: u32 = 0x4;
    pub const RETAIN_DIR_NAMES: u32 = 0x8;
    pub const RETAIN_FILE_NAMES: u32 = 0x10;
    pub const RETAIN_FILE_NAME_OFFSETS: u32 = 0x20;
    pub const XBOX360: u32 = 0x40;
    pub const RETAIN_STRINGS: u32 = 0x80; // "Retain Strings During Startup"
    pub const EMBED_FILE_NAMES: u32 = 0x100;
    pub const XMEM_CODEC: u32 = 0x200;
    pub const KNOWN: u32 = 0x3FF;
}

/// Size field bit 30: inverts the archive's default compression for this file.
pub const SIZE_COMPRESSION_TOGGLE: u32 = 0x4000_0000;
/// Size field bit 31: meaning undocumented; preserved.
pub const SIZE_UNKNOWN_BIT31: u32 = 0x8000_0000;
pub const SIZE_MASK: u32 = 0x3FFF_FFFF;

/// Bytes in a v105 folder record: hash(8) + count(4) + pad(4) + offset(4) + pad(4).
pub const FOLDER_RECORD_SIZE: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum Bsa105Error {
    #[error("I/O error: {0}")]
    Io(io::Error),
    #[error("not a BSA archive (bad magic)")]
    NotBsa,
    #[error("unsupported BSA version {0} (this reader handles 105)")]
    UnsupportedVersion(u32),
    #[error("Xbox 360 (big-endian) archives are not supported")]
    BigEndian,
    #[error("archive truncated: {0}")]
    Truncated(String),
    #[error("cannot decompress {path}: {err}")]
    Decompress { path: String, err: String },
    #[error("layout error: {0}")]
    Layout(String),
    #[error("no such file in the archive: {0}")]
    Missing(String),
}

impl From<io::Error> for Bsa105Error {
    fn from(e: io::Error) -> Self {
        Bsa105Error::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub folder_records_offset: u32,
    pub archive_flags: u32,
    pub folder_count: u32,
    pub file_count: u32,
    pub total_folder_name_length: u32,
    pub total_file_name_length: u32,
    /// Content-type bits (meshes/textures/menus/sounds/...), low 16 bits.
    pub file_flags: u16,
    /// High 16 bits of the file-flags dword. Not zero in every archive; preserved.
    pub file_flags_padding: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub hash: u64,
    /// Raw size field including its flag bits.
    pub size_field: u32,
    /// Absolute offset of the file's data block from the start of the archive.
    pub offset: u32,
    /// Raw (Windows-1252) file name without its directory, from the name block.
    pub name: Vec<u8>,
}

impl FileRecord {
    pub fn stored_size(&self) -> u32 {
        self.size_field & SIZE_MASK
    }
    pub fn toggles_compression(&self) -> bool {
        self.size_field & SIZE_COMPRESSION_TOGGLE != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRecord {
    pub hash: u64,
    pub count: u32,
    /// The offset field as stored. In v105 as in v104 it points at the folder's
    /// file-record block, biased by `total_file_name_length`.
    pub offset: u32,
    /// The u32 v105 inserts after `count`. Documented as padding, but it is
    /// nonzero in a minority of folders with values unrelated to anything
    /// structural — preserved, never used, reported once per archive.
    pub pad_after_count: u32,
    /// The u32 v105 inserts after `offset`. Always 0 in the archives measured.
    pub pad_after_offset: u32,
    /// Raw folder path bytes (Windows-1252, backslash separated), without the NUL.
    pub name: Vec<u8>,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone)]
pub struct Warning {
    pub code: &'static str,
    pub detail: String,
}

/// A parsed archive directory. File data is read on demand.
pub struct Archive {
    pub header: Header,
    pub folders: Vec<FolderRecord>,
    pub warnings: Vec<Warning>,
    /// Bytes of the file-name block after the last name (counted in
    /// `total_file_name_length`). Empty in canonical archives.
    pub name_block_tail: Vec<u8>,
    file: File,
    file_len: u64,
}

/// A flattened view of one file in the archive.
#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub folder: &'a FolderRecord,
    pub record: &'a FileRecord,
    pub folder_index: usize,
    pub file_index: usize,
}

impl<'a> Entry<'a> {
    /// Full path as stored (`folder\name`), decoded from Windows-1252.
    pub fn path(&self) -> String {
        if self.folder.name.is_empty() {
            cp1252::decode(&self.record.name)
        } else {
            format!(
                "{}\\{}",
                cp1252::decode(&self.folder.name),
                cp1252::decode(&self.record.name)
            )
        }
    }
}

/// Compute the archive name hash of a folder path or a file name.
///
/// Unchanged from v104: input is lower-cased (ASCII), `/` becomes `\`, and a
/// file's extension is hashed separately with four known extensions setting
/// marker bits. Verified against every name in the user's v105 archives.
pub fn name_hash(name: &[u8], is_file: bool) -> u64 {
    let lowered: Vec<u8> = name
        .iter()
        .map(|&b| {
            if b == b'/' {
                b'\\'
            } else {
                b.to_ascii_lowercase()
            }
        })
        .collect();
    let (stem, ext): (&[u8], &[u8]) = if is_file {
        match lowered.iter().rposition(|&b| b == b'.') {
            Some(i) => (&lowered[..i], &lowered[i..]),
            None => (&lowered[..], &[][..]),
        }
    } else {
        (&lowered[..], &[][..])
    };
    let n = stem.len();
    let mut lo: u32 = 0;
    if n > 0 {
        lo = stem[n - 1] as u32
            | if n > 2 { (stem[n - 2] as u32) << 8 } else { 0 }
            | (n as u32) << 16
            | (stem[0] as u32) << 24;
    }
    match ext {
        b".kf" => lo |= 0x80,
        b".nif" => lo |= 0x8000,
        b".dds" => lo |= 0x8080,
        b".wav" => lo |= 0x8000_0000,
        _ => {}
    }
    let mut h2: u32 = 0;
    if n > 2 {
        for &c in &stem[1..n - 2] {
            h2 = h2.wrapping_mul(0x1003F).wrapping_add(c as u32);
        }
    }
    let mut h3: u32 = 0;
    for &c in ext {
        h3 = h3.wrapping_mul(0x1003F).wrapping_add(c as u32);
    }
    ((h2.wrapping_add(h3) as u64) << 32) | lo as u64
}

/// Read a bzstring: a length byte (counting the NUL) then the bytes.
fn read_bstring_z(c: &mut Cursor) -> Result<Vec<u8>, Bsa105Error> {
    let len = c.u8().map_err(|e| Bsa105Error::Truncated(e.to_string()))? as usize;
    let b = c
        .bytes(len)
        .map_err(|e| Bsa105Error::Truncated(e.to_string()))?;
    Ok(b.strip_suffix(&[0]).unwrap_or(b).to_vec())
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Archive, Bsa105Error> {
        let mut file = File::open(path.as_ref())?;
        let file_len = file.metadata()?.len();
        let mut hb = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut hb).map_err(|_| Bsa105Error::NotBsa)?;
        let mut c = Cursor::new(&hb);
        if c.tag().unwrap() != MAGIC {
            return Err(Bsa105Error::NotBsa);
        }
        let header = Header {
            version: c.u32().unwrap(),
            folder_records_offset: c.u32().unwrap(),
            archive_flags: c.u32().unwrap(),
            folder_count: c.u32().unwrap(),
            file_count: c.u32().unwrap(),
            total_folder_name_length: c.u32().unwrap(),
            total_file_name_length: c.u32().unwrap(),
            file_flags: c.u16().unwrap(),
            file_flags_padding: c.u16().unwrap(),
        };
        if header.version != VERSION {
            return Err(Bsa105Error::UnsupportedVersion(header.version));
        }
        if header.archive_flags & flags::XBOX360 != 0 {
            return Err(Bsa105Error::BigEndian);
        }
        let mut warnings = Vec::new();
        if header.archive_flags & !flags::KNOWN != 0 {
            warnings.push(Warning {
                code: "unknown_archive_flags",
                detail: format!("0x{:x}", header.archive_flags & !flags::KNOWN),
            });
        }
        if header.archive_flags & flags::XMEM_CODEC != 0 {
            warnings.push(Warning {
                code: "xmem_codec",
                detail: "XMem codec bit set: an Xbox 360 compression, not readable here".into(),
            });
        }
        if header.folder_records_offset != HEADER_SIZE {
            warnings.push(Warning {
                code: "unusual_header_offset",
                detail: header.folder_records_offset.to_string(),
            });
        }

        // The directory is folder records, then for each folder its name and its
        // file records, then the file name block. v105's folder record is 24
        // bytes; that single difference is what this crate exists for.
        let has_dir_names = header.archive_flags & flags::DIR_NAMES != 0;
        let has_file_names = header.archive_flags & flags::FILE_NAMES != 0;
        let dir_len = header.folder_count as u64 * FOLDER_RECORD_SIZE as u64
            + if has_dir_names {
                header.total_folder_name_length as u64 + header.folder_count as u64
            } else {
                0
            }
            + header.file_count as u64 * 16
            + if has_file_names {
                header.total_file_name_length as u64
            } else {
                0
            };
        let start = header.folder_records_offset as u64;
        if start + dir_len > file_len {
            return Err(Bsa105Error::Truncated(format!(
                "directory needs {dir_len} bytes at {start}, file is {file_len}"
            )));
        }
        let mut dir = vec![0u8; dir_len as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut dir)?;
        let mut c = Cursor::new(&dir);

        let mut folders = Vec::with_capacity(header.folder_count as usize);
        for _ in 0..header.folder_count {
            folders.push(FolderRecord {
                hash: c.u64().map_err(|e| Bsa105Error::Truncated(e.to_string()))?,
                count: c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?,
                pad_after_count: c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?,
                offset: c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?,
                pad_after_offset: c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?,
                name: Vec::new(),
                files: Vec::new(),
            });
        }

        // The u32 between `count` and `offset` is not documented anywhere that
        // this reader's sources agree on. It is 0 in most folders and holds a
        // value in a minority, with no relationship to the count, the offset or
        // the name (measured: `Skyrim - Animations.bsa` has 7 of 1,691 folders
        // with a nonzero value; `Skyrim - Interface.bsa` has 1 of 13). It is
        // reported once per archive as unresolved rather than guessed at, and it
        // is never used.
        let unresolved_words: Vec<(usize, u32)> = folders
            .iter()
            .enumerate()
            .filter(|(_, f)| f.pad_after_count != 0)
            .map(|(i, f)| (i, f.pad_after_count))
            .collect();
        if !unresolved_words.is_empty() {
            warnings.push(Warning {
                code: "unresolved_folder_word",
                detail: format!(
                    "{} of {} folder records carry a nonzero u32 between count and offset;                      first is folder #{} with {}. UESP documents this word as padding; the                      values are not padding in practice, so it is preserved and unused",
                    unresolved_words.len(),
                    folders.len(),
                    unresolved_words[0].0,
                    unresolved_words[0].1,
                ),
            });
        }

        // Each folder's name and file records, in folder order.
        let mut total_files = 0u64;
        for (fi, f) in folders.iter_mut().enumerate() {
            let block_pos = start as usize + c.pos;
            // v104's offset field is the block's position plus the name block's
            // length; v105 keeps that convention.
            let expect = block_pos as u64 + header.total_file_name_length as u64;
            if f.offset as u64 != expect {
                warnings.push(Warning {
                    code: "folder_offset_mismatch",
                    detail: format!(
                        "folder #{fi}: offset field {} != computed {}",
                        f.offset, expect
                    ),
                });
            }
            if f.pad_after_offset != 0 {
                warnings.push(Warning {
                    code: "nonzero_pad_after_offset",
                    detail: format!(
                        "folder #{fi}: {} (v105's second padding word; ignored)",
                        f.pad_after_offset
                    ),
                });
            }
            if has_dir_names {
                let len_byte = dir.get(c.pos).copied().unwrap_or(0);
                f.name = read_bstring_z(&mut c)?;
                if len_byte as usize != f.name.len() + 1 {
                    warnings.push(Warning {
                        code: "noncanonical_folder_name",
                        detail: format!(
                            "folder #{fi}: length byte {len_byte}, name {:?}",
                            cp1252::decode(&f.name)
                        ),
                    });
                }
            }
            for _ in 0..f.count {
                let hash = c.u64().map_err(|e| Bsa105Error::Truncated(e.to_string()))?;
                let size_field = c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?;
                let offset = c.u32().map_err(|e| Bsa105Error::Truncated(e.to_string()))?;
                f.files.push(FileRecord {
                    hash,
                    size_field,
                    offset,
                    name: Vec::new(),
                });
            }
            total_files += f.count as u64;
        }
        if total_files != header.file_count as u64 {
            warnings.push(Warning {
                code: "file_count_mismatch",
                detail: format!(
                    "header says {}, folder records sum to {total_files}",
                    header.file_count
                ),
            });
        }

        let mut name_block_tail = Vec::new();
        if has_file_names {
            let names = c
                .bytes(header.total_file_name_length as usize)
                .map_err(|e| Bsa105Error::Truncated(e.to_string()))?;
            let mut it = names.split(|&b| b == 0);
            let mut used = 0usize;
            for f in folders.iter_mut() {
                for r in f.files.iter_mut() {
                    match it.next() {
                        Some(n) => {
                            r.name = n.to_vec();
                            used += n.len() + 1;
                        }
                        None => {
                            return Err(Bsa105Error::Layout(
                                "file name block has fewer names than files".into(),
                            ))
                        }
                    }
                }
            }
            if used < names.len() {
                name_block_tail = names[used..].to_vec();
                warnings.push(Warning {
                    code: "name_block_slack",
                    detail: format!("{} byte(s) after the last file name", name_block_tail.len()),
                });
            }
        }

        // Consistency checks. None are fatal: an archive that fails one of these
        // is still readable, and the warning is the work queue.
        for f in &folders {
            if has_dir_names && name_hash(&f.name, false) != f.hash {
                warnings.push(Warning {
                    code: "folder_hash_mismatch",
                    detail: format!("{} stored {:016x}", cp1252::decode(&f.name), f.hash),
                });
            }
            for r in &f.files {
                if has_file_names && name_hash(&r.name, true) != r.hash {
                    warnings.push(Warning {
                        code: "file_hash_mismatch",
                        detail: format!("{} stored {:016x}", cp1252::decode(&r.name), r.hash),
                    });
                }
                if r.offset as u64 + r.stored_size() as u64 > file_len {
                    warnings.push(Warning {
                        code: "data_out_of_bounds",
                        detail: format!(
                            "{} offset {} size {}",
                            cp1252::decode(&r.name),
                            r.offset,
                            r.stored_size()
                        ),
                    });
                }
                if r.size_field & SIZE_UNKNOWN_BIT31 != 0 {
                    warnings.push(Warning {
                        code: "size_bit31_set",
                        detail: cp1252::decode(&r.name),
                    });
                }
            }
        }

        let mut by_offset: Vec<&FileRecord> = folders.iter().flat_map(|f| f.files.iter()).collect();
        let in_record_order = by_offset.windows(2).all(|w| w[0].offset <= w[1].offset);
        if !in_record_order {
            let moved = by_offset
                .windows(2)
                .filter(|w| w[0].offset > w[1].offset)
                .count();
            warnings.push(Warning {
                code: "data_not_in_record_order",
                detail: format!("{moved} place(s) where a block precedes the previous record's"),
            });
            by_offset.sort_by_key(|r| r.offset);
        }
        let mut expected = start + dir_len;
        let mut gaps = 0u64;
        let mut first_gap = None;
        for r in by_offset {
            if r.offset as u64 != expected {
                gaps += 1;
                if first_gap.is_none() {
                    first_gap = Some(format!(
                        "{}: offset {} expected {}",
                        cp1252::decode(&r.name),
                        r.offset,
                        expected
                    ));
                }
            }
            expected = expected.max(r.offset as u64 + r.stored_size() as u64);
        }
        if let Some(g) = first_gap {
            warnings.push(Warning {
                code: "data_not_contiguous",
                detail: format!("{gaps} block(s); first {g}"),
            });
        }
        if expected < file_len {
            warnings.push(Warning {
                code: "trailing_bytes",
                detail: format!("{} byte(s) after the last data block", file_len - expected),
            });
        }
        for w in folders.windows(2) {
            if w[0].hash >= w[1].hash {
                warnings.push(Warning {
                    code: "folders_not_sorted",
                    detail: format!("{:016x}", w[1].hash),
                });
                break;
            }
        }

        tracing::debug!(
            folders = header.folder_count,
            files = header.file_count,
            flags = header.archive_flags,
            "opened BSA v105"
        );
        for w in &warnings {
            tracing::debug!(code = w.code, detail = %w.detail, "BSA layout anomaly");
        }
        Ok(Archive {
            header,
            folders,
            warnings,
            name_block_tail,
            file,
            file_len,
        })
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.folders.iter().enumerate().flat_map(|(fi, f)| {
            f.files.iter().enumerate().map(move |(ri, r)| Entry {
                folder: f,
                record: r,
                folder_index: fi,
                file_index: ri,
            })
        })
    }

    /// Whether this file's block is compressed: the archive's default, inverted
    /// by the file's own toggle bit.
    pub fn is_compressed(&self, r: &FileRecord) -> bool {
        (self.header.archive_flags & flags::COMPRESSED_DEFAULT != 0) ^ r.toggles_compression()
    }

    pub fn embeds_names(&self) -> bool {
        self.header.archive_flags & flags::EMBED_FILE_NAMES != 0
    }

    /// The exact stored bytes of a file's data block: still compressed, with any
    /// embedded name prefix.
    pub fn read_raw(&mut self, r: &FileRecord) -> Result<Vec<u8>, Bsa105Error> {
        let size = r.stored_size() as usize;
        if r.offset as u64 + size as u64 > self.file_len {
            return Err(Bsa105Error::Truncated(format!(
                "data for {} beyond end",
                cp1252::decode(&r.name)
            )));
        }
        let mut buf = vec![0u8; size];
        self.file.seek(SeekFrom::Start(r.offset as u64))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Decoded file contents: embedded name stripped, decompressed if needed.
    pub fn extract(&mut self, r: &FileRecord) -> Result<Vec<u8>, Bsa105Error> {
        let raw = self.read_raw(r)?;
        let name = cp1252::decode(&r.name);
        let mut body = &raw[..];
        if self.embeds_names() {
            // A bstring full path precedes the data.
            let Some((&len, rest)) = body.split_first() else {
                return Err(Bsa105Error::Truncated(format!("{name}: empty block")));
            };
            if rest.len() < len as usize {
                return Err(Bsa105Error::Truncated(format!(
                    "{name}: embedded name length {len} exceeds block"
                )));
            }
            body = &rest[len as usize..];
        }
        if !self.is_compressed(r) {
            return Ok(body.to_vec());
        }
        // A compressed block is a u32 original size followed by the codec's
        // payload: zlib in v104, LZ4 frame in v105. The version is what decides,
        // and this reader only accepts 105 — but the payload is sniffed anyway,
        // because an archive that claims one thing and stores another should be
        // reported rather than mis-decoded.
        if body.len() < 4 {
            return Err(Bsa105Error::Truncated(format!(
                "{name}: compressed block under 4 bytes"
            )));
        }
        let original = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
        let payload = &body[4..];
        if payload.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
            let mut out = Vec::with_capacity(original);
            lz4_flex::frame::FrameDecoder::new(payload)
                .read_to_end(&mut out)
                .map_err(|e| Bsa105Error::Decompress {
                    path: name.clone(),
                    err: format!("LZ4 frame: {e}"),
                })?;
            if out.len() != original {
                return Err(Bsa105Error::Decompress {
                    path: name,
                    err: format!("LZ4 produced {} bytes, header says {original}", out.len()),
                });
            }
            return Ok(out);
        }
        // zlib: a v104 payload in a v105 archive, which the format does not
        // define but a hand-built archive might carry.
        let d = flate2_zlib(payload).map_err(|e| Bsa105Error::Decompress {
            path: name.clone(),
            err: e,
        })?;
        if d.len() != original {
            return Err(Bsa105Error::Decompress {
                path: name,
                err: format!("zlib produced {} bytes, header says {original}", d.len()),
            });
        }
        Ok(d)
    }

    /// Look a file up by its full path (`folder\name`, either separator).
    pub fn find(&self, path: &str) -> Option<(usize, usize)> {
        let p = path.replace('/', "\\");
        let (dir, name) = match p.rfind('\\') {
            Some(i) => (&p[..i], &p[i + 1..]),
            None => ("", &p[..]),
        };
        let dir_b = cp1252::encode(dir)?;
        let name_b = cp1252::encode(name)?;
        let dh = name_hash(&dir_b, false);
        let fh = name_hash(&name_b, true);
        let dir_lower: Vec<u8> = dir_b.iter().map(|b| b.to_ascii_lowercase()).collect();
        let name_lower: Vec<u8> = name_b.iter().map(|b| b.to_ascii_lowercase()).collect();
        for (fi, f) in self.folders.iter().enumerate() {
            if f.hash != dh {
                continue;
            }
            if !f.name.is_empty() && f.name.to_ascii_lowercase() != dir_lower {
                continue;
            }
            for (ri, r) in f.files.iter().enumerate() {
                if r.hash == fh && (r.name.is_empty() || r.name.to_ascii_lowercase() == name_lower)
                {
                    return Some((fi, ri));
                }
            }
        }
        None
    }

    /// The bytes of one file, by its full path.
    ///
    /// The convenience form of [`Archive::find`] and [`Archive::extract`]:
    /// a caller that knows a path gets the file's bytes without holding a
    /// borrow on the archive across the extraction.
    pub fn extract_by_path(&mut self, path: &str) -> Result<Vec<u8>, Bsa105Error> {
        let (fi, ri) = self
            .find(path)
            .ok_or_else(|| Bsa105Error::Missing(path.to_string()))?;
        let record = self.folders[fi].files[ri].clone();
        self.extract(&record)
    }
}

/// Inflate a zlib stream without depending on `fnv-inflate` (which is
/// NVForever's and carries FNV-specific trailer diagnostics this crate does not
/// need). `flate2` is already in the graph for the v104 path.
fn flate2_zlib(payload: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::ZlibDecoder;
    let mut d = ZlibDecoder::new(payload);
    let mut out = Vec::new();
    d.read_to_end(&mut out).map_err(|e| format!("zlib: {e}"))?;
    Ok(out)
}
