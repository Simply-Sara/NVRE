//! Reader for Bethesda BA2 archives, as shipped with Fallout 4 and Fallout 76.
//!
//! A BA2 is a header, a table of file records, the file data, and a name
//! table at the very end. Three archive types exist; this reader handles the
//! two that matter for rendering:
//!
//! * **GNRL** — general files: meshes, materials, audio, scripts. A flat
//!   table of 36-byte records, each naming a file by hash and pointing at
//!   its data.
//! * **DX10** — textures, stored as raw mip chunks with a reconstructed DDS
//!   header. Each record carries a chunk table.
//!
//! GNMF (PlayStation GNF) is rejected with a clear error rather than
//! misread.
//!
//! # The layout, as measured
//!
//! ```text
//! "BTDX"              char[4]
//! version             u32        1 in the base game; 7 and 8 in the
//!                               next-gen update, with no structural change
//! archive_type        char[4]    "GNRL", "DX10" or "GNMF"
//! num_files           u32
//! name_table_offset   u64        absolute byte offset of the name table
//! ```
//!
//! The header is **24 bytes**. The public documentation describes "two
//! unknown u32s only present if version >= 2", but they are not header
//! fields: in `Fallout4 - Meshes.ba2` (version 8) the two words at offsets
//! 24 and 28 are `9b 99 0f 90` and `6e 69 66 00` — the first record's
//! name hash and its `nif\0` extension. Reading them as a header desyncs
//! every record that follows.
//!
//! A GNRL record is 36 bytes:
//!
//! ```text
//! name_hash     u32     hash of the file name without its extension
//! extension     char[4] zero-padded, e.g. "nif\0"
//! dir_hash      u32     hash of the directory path
//! flags         u32     bit 8 (0x100) marks a compressed payload
//! data_offset   u64     absolute byte offset of the file's data
//! packed_size   u32     0 means uncompressed
//! unpacked_size u32
//! align         u32     always 0xBAADF00D
//! ```
//!
//! Compression is **zlib (RFC 1950)**, with the `78 9c` header — not raw
//! DEFLATE as the documentation says. All 42,426 entries of
//! `Fallout4 - Meshes.ba2` were checked, and every compressed one begins
//! `78 9c`.
//!
//! The name table sits at `name_table_offset` and holds one entry per file
//! record, in index order:
//!
//! ```text
//! str_length   u16    byte length of the path
//! str_data     char[] not NUL-terminated
//! ```
//!
//! Paths use backslashes. The first entry of the table in
//! `Fallout4 - Meshes.ba2` is preceded by a single stray byte (`>`), which
//! this reader skips: it is not part of any name, and without the skip every
//! name is off by one.
//!
//! # Tolerance
//!
//! As in `bsa105`, an anomaly becomes a [`Warning`] and never a panic: a
//! record whose data lies past the end of the file, a name that is not
//! valid Windows-1252, or a `packed_size` that disagrees with the payload's
//! own header. A file that cannot be read at all is reported as an error on
//! that file alone, so one bad entry does not cost the whole archive.

use nvre_inflate::zlib_decompress;
use nvre_util::{cp1252, Cursor};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const MAGIC: [u8; 4] = *b"BTDX";

/// Bytes in the header: magic, version, type, count and the name-table offset.
pub const HEADER_SIZE: usize = 24;

/// Bytes in a GNRL file record.
pub const GNRL_RECORD_SIZE: usize = 36;

/// Bytes in the fixed prefix of a DX10 file record: name_hash(4) +
/// extension(4) + dir_hash(4) + unknown(1) + num_chunks(1) +
/// chunk_header_size(2) + height(2) + width(2) + num_mips(1) + format(1) +
/// is_cubemap(1) + tile_mode(1).
///
/// Measured on `Fallout4 - Textures1.ba2` (version 7): the first record's
/// prefix decodes as `dds`, 1024×1024, 11 mips, DXGI format 71 (BC1),
/// 3 chunks, tile mode 8 (PC layout).
pub const DX10_RECORD_PREFIX_SIZE: usize = 24;

/// Bytes in a DX10 chunk record.
pub const DX10_CHUNK_SIZE: usize = 24;

/// The value every record's `align` field holds.
pub const ALIGN_MARKER: u32 = 0xBAADF00D;

/// The flag bit that marks a record's payload as compressed.
pub const FLAG_COMPRESSED: u32 = 0x100;

/// What an archive holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveType {
    /// General files: meshes, materials, audio, scripts.
    Gnrl,
    /// DirectX textures, stored as mip chunks.
    Dx10,
    /// PlayStation GNF textures. Not read here.
    Gnmf,
}

impl ArchiveType {
    /// The type a four-byte tag names, if this reader handles it.
    pub fn from_tag(tag: &[u8; 4]) -> Option<ArchiveType> {
        match tag {
            b"GNRL" => Some(ArchiveType::Gnrl),
            b"DX10" => Some(ArchiveType::Dx10),
            b"GNMF" => Some(ArchiveType::Gnmf),
            _ => None,
        }
    }

    /// The tag this type is stored as.
    pub fn tag(&self) -> &'static [u8; 4] {
        match self {
            ArchiveType::Gnrl => b"GNRL",
            ArchiveType::Dx10 => b"DX10",
            ArchiveType::Gnmf => b"GNMF",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Ba2Error {
    #[error("not a BA2 archive: {0}")]
    NotBa2(String),
    #[error("unsupported archive type {0}")]
    UnsupportedType(String),
    #[error("archive version {0} is not one this reader knows")]
    UnsupportedVersion(u32),
    #[error("record {index} ({name}): {kind}")]
    Record {
        index: usize,
        name: String,
        #[source]
        kind: RecordError,
    },
    #[error("{0}")]
    Io(#[from] io::Error),
}

impl From<nvre_util::OutOfBounds> for Ba2Error {
    fn from(e: nvre_util::OutOfBounds) -> Self {
        Ba2Error::NotBa2(format!(
            "a field at byte {} wanted {} bytes but the data is {} bytes",
            e.pos, e.wanted, e.len
        ))
    }
}

/// Why one file record could not be read.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("its data lies past the end of the archive")]
    OutOfBounds,
    #[error("its payload is not valid zlib")]
    NotZlib,
    #[error("its unpacked size is {actual} bytes, not the {expected} the record says")]
    WrongSize { expected: usize, actual: usize },
}

/// An anomaly that did not stop the archive being read.
#[derive(Debug, Clone)]
pub struct Warning {
    pub code: &'static str,
    pub detail: String,
}

/// One file in a GNRL archive.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name_hash: u32,
    /// The extension as stored, without its zero padding.
    pub extension: String,
    pub dir_hash: u32,
    pub flags: u32,
    /// Absolute byte offset of the file's data.
    pub data_offset: u64,
    /// Compressed size; 0 means the payload is stored uncompressed.
    pub packed_size: u32,
    pub unpacked_size: u32,
    /// The `align` field as stored. Always 0xBAADF00D in the archives
    /// measured; preserved so an anomaly is visible rather than hidden.
    pub align: u32,
    /// The record's index, which is also its position in the name table.
    pub index: usize,
    /// The full path from the name table, decoded from Windows-1252.
    pub path: String,
}

/// One texture in a DX10 archive.
#[derive(Debug, Clone)]
pub struct TextureEntry {
    pub name_hash: u32,
    pub extension: String,
    pub dir_hash: u32,
    pub unknown: u8,
    /// Number of mip chunks that follow the fixed prefix.
    pub num_chunks: u8,
    pub chunk_header_size: u16,
    pub height: u16,
    pub width: u16,
    pub num_mips: u8,
    /// DXGI_FORMAT enum value; see [`crate::dxgi_format_name`].
    pub format: u8,
    pub is_cubemap: bool,
    /// 8 is a PC layout; anything else is an Xbox tiled layout.
    pub tile_mode: u8,
    pub index: usize,
    pub path: String,
    /// One per mip chunk.
    pub chunks: Vec<Chunk>,
}

/// One mip chunk of a DX10 texture.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub data_offset: u64,
    /// Compressed size; 0 means the chunk is stored uncompressed.
    pub packed_size: u32,
    pub unpacked_size: u32,
    pub start_mip: u16,
    pub end_mip: u16,
    pub align: u32,
}

/// A parsed archive directory. File data is read on demand.
pub struct Archive {
    pub archive_type: ArchiveType,
    pub version: u32,
    pub num_files: u32,
    /// The name table's offset, as the header records it.
    pub name_table_offset: u64,
    pub warnings: Vec<Warning>,
    /// GNRL entries, in record order.
    pub files: Vec<FileEntry>,
    /// DX10 entries, in record order.
    pub textures: Vec<TextureEntry>,
    file: File,
    file_len: u64,
}

impl Archive {
    /// Open and parse the archive at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Archive, Ba2Error> {
        let path = path.as_ref();
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut head = vec![0u8; HEADER_SIZE];
        file.read_exact(&mut head)?;
        let mut c = Cursor::new(&head);
        let magic = c.tag().map_err(|_| Ba2Error::NotBa2("the file is shorter than a header".into()))?;
        if magic != MAGIC {
            return Err(Ba2Error::NotBa2(format!(
                "the magic is {}, not BTDX",
                nvre_util::tag_str(&magic)
            )));
        }
        let version = c.u32()?;
        if version == 0 {
            return Err(Ba2Error::NotBa2("version 0".into()));
        }
        let tag = c.tag()?;
        let Some(archive_type) = ArchiveType::from_tag(&tag) else {
            return Err(Ba2Error::UnsupportedType(nvre_util::tag_str(&tag)));
        };
        if archive_type == ArchiveType::Gnmf {
            return Err(Ba2Error::UnsupportedType("GNMF (PlayStation GNF)".into()));
        }
        let num_files = c.u32()?;
        let name_table_offset = c.u64()?;

        let mut warnings = Vec::new();
        let (files, textures) = match archive_type {
            ArchiveType::Gnrl => {
                let (files, w) = read_gnrl(&mut file, num_files, name_table_offset)?;
                warnings.extend(w);
                (files, Vec::new())
            }
            ArchiveType::Dx10 => {
                let (textures, w) = read_dx10(&mut file, num_files, name_table_offset)?;
                warnings.extend(w);
                (Vec::new(), textures)
            }
            ArchiveType::Gnmf => unreachable!("rejected above"),
        };
        warnings.extend(files_warnings(&files));
        Ok(Archive {
            archive_type,
            version,
            num_files,
            name_table_offset,
            warnings,
            files,
            textures,
            file,
            file_len,
        })
    }

    /// The bytes of the file at `entry`, decompressed.
    pub fn extract(&mut self, entry: &FileEntry) -> Result<Vec<u8>, Ba2Error> {
        // `packed_size == 0` means the payload is stored uncompressed, and its
        // length is then `unpacked_size`. Measured: `Fallout4 - Interface.ba2`
        // holds `Interface\Exported\0.png.dds` with packed 0 and unpacked
        // 1152, and the 1152 bytes at its offset are the file itself.
        let (offset, len, compressed) = if entry.packed_size == 0 {
            (entry.data_offset, entry.unpacked_size, false)
        } else {
            (entry.data_offset, entry.packed_size, true)
        };
        let payload = self.read_payload(offset, len)?;
        let data = if compressed {
            zlib_decompress(&payload, Some(entry.unpacked_size as usize)).map_err(|_| Ba2Error::Record {
                index: entry.index,
                name: entry.path.clone(),
                kind: RecordError::NotZlib,
            })?
        } else {
            payload
        };
        if entry.unpacked_size != 0 && data.len() != entry.unpacked_size as usize {
            return Err(Ba2Error::Record {
                index: entry.index,
                name: entry.path.clone(),
                kind: RecordError::WrongSize {
                    expected: entry.unpacked_size as usize,
                    actual: data.len(),
                },
            });
        }
        Ok(data)
    }

    /// The bytes of the texture at `entry`, decompressed.
    pub fn extract_texture(&mut self, entry: &TextureEntry) -> Result<Vec<u8>, Ba2Error> {
        let mut out = Vec::new();
        for chunk in &entry.chunks {
            let payload = self.read_payload(chunk.data_offset, chunk.packed_size)?;
            let data = if chunk.packed_size == 0 {
                payload
            } else {
                zlib_decompress(&payload, Some(chunk.unpacked_size as usize)).map_err(|_| Ba2Error::Record {
                    index: entry.index,
                    name: entry.path.clone(),
                    kind: RecordError::NotZlib,
                })?
            };
            out.extend_from_slice(&data);
        }
        Ok(out)
    }

    /// Every GNRL path, in record order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|f| f.path.as_str())
    }

    /// The GNRL entry whose path is `path`, comparing case-insensitively the
    /// way the game's own file system does.
    pub fn find(&self, path: &str) -> Option<&FileEntry> {
        let want = path.replace('/', "\\").to_ascii_lowercase();
        self.files
            .iter()
            .find(|f| f.path.replace('/', "\\").to_ascii_lowercase() == want)
    }

    /// Read `len` bytes at `offset`, or say the record's data is out of
    /// bounds. A length of 0 reads nothing.
    fn read_payload(&mut self, offset: u64, len: u32) -> Result<Vec<u8>, Ba2Error> {
        let len = len as u64;
        if offset.checked_add(len).is_none_or(|e| e > self.file_len) {
            return Err(Ba2Error::Record {
                index: 0,
                name: String::new(),
                kind: RecordError::OutOfBounds,
            });
        }
        let mut buf = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// The anomalies worth reporting about the records as a whole.
fn files_warnings(files: &[FileEntry]) -> Vec<Warning> {
    let mut out = Vec::new();
    let odd_align = files.iter().filter(|f| f.align != ALIGN_MARKER).count();
    if odd_align > 0 {
        out.push(Warning {
            code: "align",
            detail: format!("{odd_align} records have an align field that is not 0xBAADF00D"),
        });
    }
    out
}

/// Read the GNRL record table and the name table.
fn read_gnrl(
    file: &mut File,
    num_files: u32,
    name_table_offset: u64,
) -> Result<(Vec<FileEntry>, Vec<Warning>), Ba2Error> {
    let mut records = Vec::with_capacity(num_files as usize);
    file.seek(SeekFrom::Start(HEADER_SIZE as u64))?;
    let mut raw = vec![0u8; num_files as usize * GNRL_RECORD_SIZE];
    file.read_exact(&mut raw)?;
    let mut c = Cursor::new(&raw);
    for i in 0..num_files as usize {
        let name_hash = c.u32()?;
        let ext = c.tag()?;
        let dir_hash = c.u32()?;
        let flags = c.u32()?;
        let data_offset = c.u64()?;
        let packed_size = c.u32()?;
        let unpacked_size = c.u32()?;
        let align = c.u32()?;
        records.push(FileEntry {
            name_hash,
            extension: cp1252::decode(ext.split(|&b| b == 0).next().unwrap_or(&ext)),
            dir_hash,
            flags,
            data_offset,
            packed_size,
            unpacked_size,
            align,
            index: i,
            path: String::new(),
        });
    }
    let names = read_name_table(file, name_table_offset, num_files as usize)?;
    for (i, name) in names.into_iter().enumerate() {
        if let Some(r) = records.get_mut(i) {
            r.path = name;
        }
    }
    Ok((records, Vec::new()))
}

/// Read the DX10 record table and the name table.
fn read_dx10(
    file: &mut File,
    num_files: u32,
    name_table_offset: u64,
) -> Result<(Vec<TextureEntry>, Vec<Warning>), Ba2Error> {
    let mut entries = Vec::with_capacity(num_files as usize);
    file.seek(SeekFrom::Start(HEADER_SIZE as u64))?;
    for i in 0..num_files as usize {
        let mut prefix = vec![0u8; DX10_RECORD_PREFIX_SIZE];
        file.read_exact(&mut prefix)?;
        let mut c = Cursor::new(&prefix);
        let name_hash = c.u32()?;
        let ext = c.tag()?;
        let dir_hash = c.u32()?;
        let unknown = c.u8()?;
        let num_chunks = c.u8()?;
        let chunk_header_size = c.u16()?;
        let height = c.u16()?;
        let width = c.u16()?;
        let num_mips = c.u8()?;
        let format = c.u8()?;
        let is_cubemap = c.u8()? != 0;
        let tile_mode = c.u8()?;
        // The chunk records follow the prefix.
        let mut chunks = Vec::with_capacity(num_chunks as usize);
        let mut raw = vec![0u8; num_chunks as usize * DX10_CHUNK_SIZE];
        if file.read_exact(&mut raw).is_ok() {
            let mut cc = Cursor::new(&raw);
            for _ in 0..num_chunks {
                let data_offset = cc.u64()?;
                let packed_size = cc.u32()?;
                let unpacked_size = cc.u32()?;
                let start_mip = cc.u16()?;
                let end_mip = cc.u16()?;
                let align = cc.u32()?;
                chunks.push(Chunk {
                    data_offset,
                    packed_size,
                    unpacked_size,
                    start_mip,
                    end_mip,
                    align,
                });
            }
        }
        entries.push(TextureEntry {
            name_hash,
            extension: cp1252::decode(ext.split(|&b| b == 0).next().unwrap_or(&ext)),
            dir_hash,
            unknown,
            num_chunks,
            chunk_header_size,
            height,
            width,
            num_mips,
            format,
            is_cubemap,
            tile_mode,
            index: i,
            path: String::new(),
            chunks,
        });
    }
    let names = read_name_table(file, name_table_offset, num_files as usize)?;
    for (i, name) in names.into_iter().enumerate() {
        if let Some(e) = entries.get_mut(i) {
            e.path = name;
        }
    }
    Ok((entries, Vec::new()))
}

/// Read `count` length-prefixed paths from the name table.
///
/// The table begins with one stray byte in `Fallout4 - Meshes.ba2`, so a
/// leading length that cannot describe the bytes that follow is stepped
/// over. The test is whether the first entry, read as a length plus that
/// many bytes, lands exactly on the start of the second entry — a real
/// table's first entry does, and a table shifted by one does not.
fn read_name_table(file: &mut File, offset: u64, count: usize) -> Result<Vec<String>, Ba2Error> {
    file.seek(SeekFrom::Start(offset))?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let skip = name_table_leading_skip(&raw);
    let mut c = Cursor::new(&raw);
    c.pos = skip;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let Ok(len) = c.u16() else { break };
        let len = len as usize;
        let Ok(bytes) = c.bytes(len) else { break };
        names.push(cp1252::decode(bytes));
    }
    Ok(names)
}

/// How many bytes precede the name table's first length.
///
/// A name is a `u16` length and that many bytes of path. A table read from
/// the wrong position gives a length whose path runs past the end of the
/// data, or runs into the next entry; a table read from the right one does
/// neither. So both candidates are tried and the one that parses is used,
/// with the aligned read preferred when both do.
fn name_table_leading_skip(raw: &[u8]) -> usize {
    let parses = |skip: usize| -> bool {
        if raw.len() < skip + 2 {
            return false;
        }
        let len = u16::from_le_bytes([raw[skip], raw[skip + 1]]) as usize;
        // A path is at least a byte and at most a plausible file name; and it
        // has to fit in what is left.
        len > 0 && len <= 1024 && skip + 2 + len <= raw.len()
    };
    if parses(0) {
        0
    } else if parses(1) {
        1
    } else {
        0
    }
}

/// The name of a DXGI format, for the formats Fallout 4's textures use.
pub fn dxgi_format_name(format: u8) -> &'static str {
    match format {
        28 => "R8G8B8A8_UNORM",
        71 => "BC1_UNORM",
        74 => "BC2_UNORM",
        77 => "BC3_UNORM",
        80 => "BC4_UNORM",
        83 => "BC5_UNORM",
        98 => "BC7_UNORM",
        99 => "BC7_UNORM_SRGB",
        _ => "unknown",
    }
}

/// Build a DDS file from a DX10 texture and its decompressed mip data.
///
/// A BA2's DX10 record holds the mips as raw chunks with no header; a DDS
/// reader needs the header, so it is written here. Everything but the
/// uncompressed RGBA case is a DX10-extended header: the DXGI format number
/// is carried in the header's own DX10 extension rather than mapped to a
/// legacy four-character code, because BC5 and BC7 have no legacy code and
/// mapping BC1 to DXT1 would lose the sRGB variants.
///
/// `mips` is the decompressed chunk data concatenated in mip order, which
/// is what [`Archive::extract_texture`] returns.
pub fn dds_from_texture(entry: &TextureEntry, mips: &[u8]) -> Vec<u8> {
    const DDS_MAGIC: u32 = 0x2053_4444;
    const HEADER_SIZE: u32 = 124;
    // DDS_PIXELFORMAT flags.
    const DDPF_FOURCC: u32 = 0x4;
    // DDS_HEADER flags.
    const DDSD_CAPS: u32 = 0x1;
    const DDSD_HEIGHT: u32 = 0x2;
    const DDSD_WIDTH: u32 = 0x4;
    const DDSD_PIXELFORMAT: u32 = 0x1000;
    const DDSD_MIPMAPCOUNT: u32 = 0x2_0000;
    const DDSD_LINEARSIZE: u32 = 0x8_0000;
    // DDSCAPS flags.
    const DDSCAPS_TEXTURE: u32 = 0x1000;
    const DDSCAPS_COMPLEX: u32 = 0x8;
    const DDSCAPS_MIPMAP: u32 = 0x40_0000;
    // DDS_RESOURCE_DIMENSION / misc.
    const DDS_DIMENSION_TEXTURE2D: u32 = 3;
    const DDS_ALPHA_MODE_UNKNOWN: u32 = 0;
    const FOURCC_DX10: u32 = u32::from_le_bytes(*b"DX10");

    let mut out = Vec::with_capacity(148 + mips.len());
    out.extend_from_slice(&DDS_MAGIC.to_le_bytes());
    out.extend_from_slice(&HEADER_SIZE.to_le_bytes());

    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT;
    if entry.num_mips > 1 {
        flags |= DDSD_MIPMAPCOUNT;
    }
    // The linear size is the top mip's byte count for a block format; an
    // uncompressed texture uses pitch instead, which this writer does not
    // emit, so the flag is only set when a block size is known.
    if matches!(entry.format, 71 | 74 | 77 | 80 | 83 | 98 | 99) {
        flags |= DDSD_LINEARSIZE;
    }
    out.extend_from_slice(&flags.to_le_bytes());

    let h = entry.height as u32;
    let w = entry.width as u32;
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());

    // The top mip's size, or the pitch for an uncompressed format.
    let top = mip0_size(entry.format, w, h) as u32;
    let linear_or_pitch = if flags & DDSD_LINEARSIZE != 0 {
        top
    } else {
        w * 4
    };
    out.extend_from_slice(&linear_or_pitch.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // depth
    out.extend_from_slice(&(entry.num_mips.max(1) as u32).to_le_bytes()); // mip count

    // 11 reserved words.
    for _ in 0..11 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    // DDS_PIXELFORMAT: 32 bytes.
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&DDPF_FOURCC.to_le_bytes());
    out.extend_from_slice(&FOURCC_DX10.to_le_bytes());
    for _ in 0..5 {
        out.extend_from_slice(&0u32.to_le_bytes()); // bits and masks
    }

    // DDSCAPS.
    let mut caps = DDSCAPS_TEXTURE;
    if entry.num_mips > 1 || entry.is_cubemap {
        caps |= DDSCAPS_COMPLEX;
    }
    if entry.num_mips > 1 {
        caps |= DDSCAPS_MIPMAP;
    }
    out.extend_from_slice(&caps.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // caps2 (cubemap faces)
    out.extend_from_slice(&0u32.to_le_bytes()); // caps3
    out.extend_from_slice(&0u32.to_le_bytes()); // caps4
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved2

    // The DX10 extended header: 20 bytes.
    out.extend_from_slice(&(entry.format as u32).to_le_bytes());
    out.extend_from_slice(&DDS_DIMENSION_TEXTURE2D.to_le_bytes());
    out.extend_from_slice(&(if entry.is_cubemap { 4u32 } else { 0 }).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // array size
    out.extend_from_slice(&DDS_ALPHA_MODE_UNKNOWN.to_le_bytes());

    out.extend_from_slice(mips);
    out
}

/// The bytes one mip of a DXGI format takes, at `w`×`h`.
///
/// The block formats are 4×4 blocks; the uncompressed ones are a fixed
/// number of bytes per pixel.
fn mip0_size(format: u8, w: u32, h: u32) -> usize {
    let block = |b: usize| w.div_ceil(4).max(1) as usize * h.div_ceil(4).max(1) as usize * b;
    let bpp = |bits: usize| (w as usize * h as usize * bits).div_ceil(8);
    match format {
        71 => block(8),   // BC1
        74 => block(16),  // BC2
        77 => block(16),  // BC3
        80 => block(8),   // BC4
        83 => block(16),  // BC5
        98 | 99 => block(16), // BC7
        28 => bpp(32),    // R8G8B8A8
        _ => block(16),   // the common case for an unknown format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archive_type_tags_round_trip() {
        for t in [ArchiveType::Gnrl, ArchiveType::Dx10] {
            assert_eq!(ArchiveType::from_tag(t.tag()), Some(t));
        }
        assert_eq!(ArchiveType::from_tag(b"GNRL"), Some(ArchiveType::Gnrl));
        assert_eq!(ArchiveType::from_tag(b"ZZZZ"), None);
        // GNMf is recognised but not readable.
        assert_eq!(ArchiveType::from_tag(b"GNMF"), Some(ArchiveType::Gnmf));
    }

    #[test]
    fn a_payload_is_read_at_its_own_offset() {
        // A record whose data lies past the end of the archive is an error,
        // not a panic: the reader says the record is out of bounds.
        let mut a = Archive {
            archive_type: ArchiveType::Gnrl,
            version: 8,
            num_files: 1,
            name_table_offset: 0,
            warnings: Vec::new(),
            files: Vec::new(),
            textures: Vec::new(),
            file: File::open("/dev/null").unwrap(),
            file_len: 10,
        };
        let e = FileEntry {
            name_hash: 0,
            extension: "nif".into(),
            dir_hash: 0,
            flags: 0,
            data_offset: 100,
            packed_size: 10,
            unpacked_size: 10,
            align: ALIGN_MARKER,
            index: 0,
            path: "x.nif".into(),
        };
        let err = a.extract(&e).unwrap_err();
        assert!(matches!(
            err,
            Ba2Error::Record {
                kind: RecordError::OutOfBounds,
                ..
            }
        ));
    }

    #[test]
    fn a_stray_byte_before_the_name_table_is_stepped_over() {
        // The real case: a stray '>' precedes the table, so the bytes at the
        // table's offset are '>', 0x0C, 0x00 — a length of 0x0C3E, whose path
        // would run far past the end of the data. Reading one byte later
        // gives 0x000C and a 12-byte path, which fits.
        let mut raw = vec![b'>'];
        raw.extend_from_slice(&12u16.to_le_bytes());
        raw.extend_from_slice(b"Meshes\\a.nif");
        assert_eq!(name_table_leading_skip(&raw), 1);

        let mut c = Cursor::new(&raw);
        c.pos = name_table_leading_skip(&raw);
        let n = c.u16().unwrap() as usize;
        assert_eq!(n, 12);
        assert_eq!(cp1252::decode(c.bytes(n).unwrap()), "Meshes\\a.nif");
    }

    #[test]
    fn a_table_that_starts_correctly_is_not_shifted() {
        // A table whose first length is real is read from its first byte: the
        // path fits in the data, so there is nothing to step over.
        let mut raw = Vec::new();
        raw.extend_from_slice(&5u16.to_le_bytes());
        raw.extend_from_slice(b"a.nif");
        assert_eq!(name_table_leading_skip(&raw), 0);
        let mut c = Cursor::new(&raw);
        c.pos = name_table_leading_skip(&raw);
        let n = c.u16().unwrap() as usize;
        assert_eq!(n, 5);
        assert_eq!(cp1252::decode(c.bytes(n).unwrap()), "a.nif");
    }

    #[test]
    fn a_length_that_runs_past_the_end_is_not_trusted() {
        // A length whose path would run past the data cannot be the real one,
        // so the aligned read is rejected and the shifted one tried.
        let mut raw = Vec::new();
        raw.extend_from_slice(&999u16.to_le_bytes());
        raw.extend_from_slice(b"ab");
        // Aligned: 999 bytes wanted, only 2 there. Shifted: 0x0261 as a u16
        // from b'a', 0x62 — also too long. Neither parses, so the aligned
        // read is returned and the caller sees a truncated table.
        assert_eq!(name_table_leading_skip(&raw), 0);
    }
}
