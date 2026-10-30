//! Reader for Bethesda CSG (Compressed Shared Geometry) archives (Fallout 4).
//!
//! A `.csg` companion file (such as `Fallout4 - Geometry.csg` or `DLCCoast - Geometry.csg`)
//! contains the geometry for precombined world meshes (`meshes\precombined\*_oc.nif`).
//!
//! # The layout
//!
//! ```text
//! offset 0    char[4]        magic = "bcsg"
//! offset 4    u32            num_objects   (metadata object count)
//! offset 8    u32            num_chunks    (number of 64 KiB chunks)
//! offset 12   ChunkEntry[num_chunks]       8 bytes each (compressed_size, file_offset)
//! ...         ObjectEntry[num_objects]     20 bytes each (CK-side index)
//! ...         zlib stream[num_chunks]      compressed payload chunks
//! ```
//!
//! Every chunk except the last inflates to exactly 65,536 bytes. The concatenation
//! of all uncompressed chunks forms the contiguous PSG (Previs Shared Geometry) byte stream.
//!
//! A NIF's `BSPackedGeomObject` addresses into this stream via `(filename_hash, data_offset)`.
//! The `filename_hash` is `bscrc32("<plugin stem> - geometry")`.

use nvre_inflate::zlib_decompress;
use nvre_util::Cursor;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MAGIC: [u8; 4] = *b"bcsg";

/// Standard uncompressed size of every non-terminal chunk.
pub const CHUNK_UNCOMPRESSED_SIZE: usize = 65_536;

/// CRC-32 table for Bethesda's reflected CRC-32 (BSCRC32).
/// Polynomial 0xEDB88320 (IEEE 802.3), initial value 0, no final XOR.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            if (c & 1) != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// Bethesda's `BSCRC32`: reflected CRC-32 over ASCII-lowercased bytes with
/// init = 0 and no final XOR.
pub fn bscrc32(s: &str) -> u32 {
    let mut crc = 0u32;
    for &b in s.as_bytes() {
        let lower = b.to_ascii_lowercase();
        crc = (crc >> 8) ^ CRC_TABLE[((crc ^ (lower as u32)) & 0xFF) as usize];
    }
    crc
}

/// Compute the `filename_hash` for a geometry CSG archive given a plugin stem or name
/// (e.g. `"Fallout4"` or `"Fallout4.esm"` -> `bscrc32("fallout4 - geometry")`).
pub fn csg_name_hash(plugin_stem_or_filename: &str) -> u32 {
    let stem = plugin_stem_or_filename
        .strip_suffix(".esm")
        .or_else(|| plugin_stem_or_filename.strip_suffix(".ESM"))
        .or_else(|| plugin_stem_or_filename.strip_suffix(".esp"))
        .or_else(|| plugin_stem_or_filename.strip_suffix(".ESP"))
        .unwrap_or(plugin_stem_or_filename);
    let s = format!("{stem} - geometry");
    bscrc32(&s)
}

#[derive(Debug, thiserror::Error)]
pub enum CsgError {
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("{0}: invalid magic {1:?}, expected b\"bcsg\"")]
    BadMagic(PathBuf, [u8; 4]),
    #[error("{path:?}: header math mismatch: expected first chunk at {expected}, got {actual}")]
    HeaderMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    #[error("{path:?}: chunk {chunk} inflate failed: {source}")]
    Inflate {
        path: PathBuf,
        chunk: usize,
        #[source]
        source: nvre_inflate::InflateError,
    },
    #[error("{path:?}: read at PSG offset {offset} with length {len} exceeds total size {total}")]
    OutOfBounds {
        path: PathBuf,
        offset: u64,
        len: usize,
        total: u64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkEntry {
    pub compressed_size: u32,
    pub file_offset: u32,
}

/// A reader for a single `.csg` archive.
pub struct CsgArchive {
    path: PathBuf,
    file: Mutex<File>,
    num_objects: u32,
    chunks: Vec<ChunkEntry>,
    /// Cache of recently decompressed 64 KiB chunks.
    chunk_cache: Mutex<HashMap<usize, Vec<u8>>>,
    /// Total uncompressed PSG size in bytes.
    total_uncompressed_size: u64,
}

impl CsgArchive {
    /// Open a `.csg` archive from disk.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CsgError> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path).map_err(|e| CsgError::Io(path.clone(), e))?;

        let mut header = [0u8; 12];
        f.read_exact(&mut header)
            .map_err(|e| CsgError::Io(path.clone(), e))?;

        let mut cur = Cursor::new(&header);
        let magic = cur.bytes(4).unwrap();
        if magic != MAGIC {
            return Err(CsgError::BadMagic(path.clone(), magic.try_into().unwrap()));
        }
        let num_objects = cur.u32().unwrap();
        let num_chunks = cur.u32().unwrap() as usize;

        // Read chunk table (num_chunks * 8 bytes)
        let mut table_bytes = vec![0u8; num_chunks * 8];
        f.read_exact(&mut table_bytes)
            .map_err(|e| CsgError::Io(path.clone(), e))?;

        let mut chunks = Vec::with_capacity(num_chunks);
        let mut cur_table = Cursor::new(&table_bytes);
        for _ in 0..num_chunks {
            let compressed_size = cur_table.u32().unwrap();
            let file_offset = cur_table.u32().unwrap();
            chunks.push(ChunkEntry {
                compressed_size,
                file_offset,
            });
        }

        // Validate header consistency if there are chunks
        if let Some(first) = chunks.first() {
            let expected_first_offset = 12 + (num_chunks as u64) * 8 + (num_objects as u64) * 20;
            if (first.file_offset as u64) != expected_first_offset {
                return Err(CsgError::HeaderMismatch {
                    path: path.clone(),
                    expected: expected_first_offset,
                    actual: first.file_offset as u64,
                });
            }
        }

        // Compute total uncompressed size: (num_chunks - 1) * 65536 + last_chunk_size
        let total_uncompressed_size = if num_chunks == 0 {
            0
        } else {
            // Decompress the last chunk to find its exact uncompressed length
            let last_idx = num_chunks - 1;
            let last_entry = chunks[last_idx];
            f.seek(SeekFrom::Start(last_entry.file_offset as u64))
                .map_err(|e| CsgError::Io(path.clone(), e))?;
            let mut comp = vec![0u8; last_entry.compressed_size as usize];
            f.read_exact(&mut comp)
                .map_err(|e| CsgError::Io(path.clone(), e))?;

            let decomp = zlib_decompress(&comp, None).map_err(|e| CsgError::Inflate {
                path: path.clone(),
                chunk: last_idx,
                source: e,
            })?;
            let last_len = decomp.len() as u64;

            ((num_chunks - 1) as u64) * (CHUNK_UNCOMPRESSED_SIZE as u64) + last_len
        };

        Ok(Self {
            path,
            file: Mutex::new(f),
            num_objects,
            chunks,
            chunk_cache: Mutex::new(HashMap::new()),
            total_uncompressed_size,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn num_objects(&self) -> u32 {
        self.num_objects
    }

    pub fn num_chunks(&self) -> usize {
        self.chunks.len()
    }

    pub fn total_uncompressed_size(&self) -> u64 {
        self.total_uncompressed_size
    }

    /// Decompress a single 64 KiB chunk by index (or retrieve from cache).
    pub fn get_chunk(&self, chunk_idx: usize) -> Result<Vec<u8>, CsgError> {
        if chunk_idx >= self.chunks.len() {
            return Err(CsgError::OutOfBounds {
                path: self.path.clone(),
                offset: (chunk_idx * CHUNK_UNCOMPRESSED_SIZE) as u64,
                len: 0,
                total: self.total_uncompressed_size,
            });
        }

        // Check cache first
        {
            let cache = self.chunk_cache.lock().unwrap();
            if let Some(data) = cache.get(&chunk_idx) {
                return Ok(data.clone());
            }
        }

        // Read and decompress
        let entry = self.chunks[chunk_idx];
        let mut comp = vec![0u8; entry.compressed_size as usize];
        {
            let mut f = self.file.lock().unwrap();
            f.seek(SeekFrom::Start(entry.file_offset as u64))
                .map_err(|e| CsgError::Io(self.path.clone(), e))?;
            f.read_exact(&mut comp)
                .map_err(|e| CsgError::Io(self.path.clone(), e))?;
        }

        let decomp = zlib_decompress(&comp, None).map_err(|e| CsgError::Inflate {
            path: self.path.clone(),
            chunk: chunk_idx,
            source: e,
        })?;

        // Cache (evict if cache gets very large: e.g. > 128 chunks = 8 MB)
        {
            let mut cache = self.chunk_cache.lock().unwrap();
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(chunk_idx, decomp.clone());
        }

        Ok(decomp)
    }

    /// Read an arbitrary span of bytes from the uncompressed PSG stream.
    ///
    /// Handles reads that cross 64 KiB chunk boundaries seamlessly.
    pub fn read_psg(&self, offset: u64, len: usize) -> Result<Vec<u8>, CsgError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        if offset + (len as u64) > self.total_uncompressed_size {
            return Err(CsgError::OutOfBounds {
                path: self.path.clone(),
                offset,
                len,
                total: self.total_uncompressed_size,
            });
        }

        let mut out = Vec::with_capacity(len);
        let mut cur_offset = offset;
        let mut remaining = len;

        while remaining > 0 {
            let chunk_idx = (cur_offset / (CHUNK_UNCOMPRESSED_SIZE as u64)) as usize;
            let local_offset = (cur_offset % (CHUNK_UNCOMPRESSED_SIZE as u64)) as usize;

            let chunk_data = self.get_chunk(chunk_idx)?;
            let available_in_chunk = chunk_data.len().saturating_sub(local_offset);
            let to_read = remaining.min(available_in_chunk);

            if to_read == 0 {
                break;
            }

            out.extend_from_slice(&chunk_data[local_offset..local_offset + to_read]);
            cur_offset += to_read as u64;
            remaining -= to_read;
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bscrc32_matches_all_shipped_geometry_csgs() {
        let cases = [
            ("Fallout4 - Geometry", 0xddf19a67),
            ("DLCCoast - Geometry", 0x2088054d),
            ("DLCNukaWorld - Geometry", 0xe81b308e),
            ("DLCRobot - Geometry", 0x3a1b90b8),
            ("DLCworkshop01 - Geometry", 0x8e566007),
            ("DLCworkshop03 - Geometry", 0x626dfe98),
        ];

        for (name, expected) in cases {
            assert_eq!(
                bscrc32(name),
                expected,
                "BSCRC32 mismatch for '{name}': expected 0x{expected:08x}, got 0x{:08x}",
                bscrc32(name)
            );
        }
    }

    #[test]
    fn csg_name_hash_resolves_plugin_names() {
        assert_eq!(csg_name_hash("Fallout4.esm"), 0xddf19a67);
        assert_eq!(csg_name_hash("Fallout4"), 0xddf19a67);
        assert_eq!(csg_name_hash("DLCCoast.esm"), 0x2088054d);
        assert_eq!(csg_name_hash("DLCNukaWorld.esm"), 0xe81b308e);
        assert_eq!(csg_name_hash("DLCRobot.esm"), 0x3a1b90b8);
        assert_eq!(csg_name_hash("DLCworkshop01.esm"), 0x8e566007);
        assert_eq!(csg_name_hash("DLCworkshop03.esm"), 0x626dfe98);
    }

    #[test]
    fn open_and_read_real_fallout4_geometry_csg() {
        let path =
            Path::new("/home/sara/Games/BethesdaGames/Fallout4/Data/Fallout4 - Geometry.csg");
        if !path.exists() {
            return;
        }

        let archive = CsgArchive::open(path).expect("failed to open Fallout4 - Geometry.csg");
        assert_eq!(archive.num_objects(), 32370);
        assert_eq!(archive.num_chunks(), 6841);
        assert_eq!(archive.total_uncompressed_size(), 448313134);

        // Read a known span from chunk 896 local 33502 (PSG offset 58753758)
        let bytes = archive
            .read_psg(58753758, 20)
            .expect("read_psg failed on known offset");
        assert_eq!(bytes.len(), 20);

        // Check the half-float position and attributes
        // Expected prefix: 00 00 00 d8 00 00 00 3c 00 38 00 3c
        let expected_prefix = [
            0x00, 0x00, 0x00, 0xd8, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x38, 0x00, 0x3c,
        ];
        assert_eq!(&bytes[..12], &expected_prefix);

        // Test boundary crossing read across 64 KiB boundary
        let cross_offset = 65536 - 10;
        let cross_bytes = archive
            .read_psg(cross_offset, 20)
            .expect("boundary crossing read failed");
        assert_eq!(cross_bytes.len(), 20);
    }
}
