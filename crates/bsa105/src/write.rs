//! Write a minimal BSA archive, for tests that need real archive contents.
//!
//! The reader is only half of the precedence question: which archive a file
//! comes from is decided by the *order* the archives are opened in, and the
//! index's `insert` overwrites whatever was there before — so the archive
//! opened last wins. That rule is invisible when every archive in a test is
//! an empty file, which is what the VFS's own tests use. This writes a real
//! v104 archive so a test can put a different payload in two archives and
//! assert which one is read.
//!
//! The layout is the one the reader parses, in its order:
//!
//! 1. a 36-byte header;
//! 2. the folder records, one per folder, sorted by name hash;
//! 3. for each folder in turn, its name (a bstring: length byte, bytes, NUL)
//!    followed by that folder's own file records;
//! 4. the file-name block: every file's name in the same order, NUL-terminated;
//! 5. the data blocks, in the same order as the file records.
//!
//! Everything is uncompressed, because the payload is the thing under test and
//! compression is [`crate`]'s business.

/// One file in an archive being written: its folder, its name and its bytes.
pub struct File {
    /// The folder the file sits in (no leading or trailing separator).
    pub folder: String,
    /// The file's name inside the folder.
    pub name: String,
    /// The file's bytes.
    pub data: Vec<u8>,
}

/// Write a v104 BSA holding `files` to `path`.
///
/// Uncompressed, with directory and file names embedded — the flags Bethesda's
/// own archives carry, so the reader takes the same path it takes for a real
/// one. Folders are written sorted by name hash, which is the order the reader
/// expects and checks.
pub fn write(path: impl AsRef<std::path::Path>, files: &[File]) -> std::io::Result<()> {
    // Folders in first-appearance order, then sorted by name hash.
    let mut folders: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        match folders.iter_mut().find(|(n, _)| *n == f.folder) {
            Some((_, v)) => v.push(i),
            None => folders.push((f.folder.clone(), vec![i])),
        }
    }
    folders.sort_by_key(|(name, _)| crate::name_hash(name.as_bytes(), false));

    // The header's total-folder-name-length counts the directory names only:
    // each is a length byte, its bytes, and a NUL.
    // The header's total-folder-name-length counts the directory names'
    // bytes and their NULs, but not the length byte in front of each.
    let folder_name_len: u32 = folders.iter().map(|(f, _)| f.len() as u32 + 1).sum();
    // A folder's name as a bstring: the length byte counts the NUL.
    let dir_name = |name: &str| {
        std::iter::once(name.len() as u8 + 1)
            .chain(name.bytes())
            .chain(std::iter::once(0))
            .collect::<Vec<u8>>()
    };
    // The file-name block: every file's name in folder order, NUL-terminated.
    let name_block: Vec<u8> = folders
        .iter()
        .flat_map(|(_, m)| m.iter())
        .flat_map(|&i| files[i].name.bytes().chain(std::iter::once(0)))
        .collect();
    // The reader's own `dir_len` is
    //   folder_count*16 + (total_folder_name_length + folder_count)
    //   + file_count*16 + total_file_name_length,
    // and its `total_folder_name_length` counts each folder name's bytes and
    // its NUL but not the length byte in front of it. So the directory is
    // `folder_name_len + folder_count` bytes of names, and the data blocks
    // start after them.
    let data_start = crate::HEADER_SIZE
        + folders.len() as u32 * 16
        + (folder_name_len + folders.len() as u32)
        + files.len() as u32 * 16
        + name_block.len() as u32;

    let flags: u32 = crate::flags::DIR_NAMES
        | crate::flags::FILE_NAMES
        | crate::flags::RETAIN_DIR_NAMES
        | crate::flags::RETAIN_FILE_NAMES;
    // The header, the folder records, the per-folder names and file records,
    // and the file-name block; the data blocks follow.
    // The data blocks follow the whole directory: the header, the folder
    // records, the per-folder names and their file records, and the file-name
    // block.
    let mut out = Vec::with_capacity(data_start as usize);
    out.extend_from_slice(&crate::MAGIC);
    out.extend_from_slice(&crate::VERSION_104.to_le_bytes());
    out.extend_from_slice(&crate::HEADER_SIZE.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&(folders.len() as u32).to_le_bytes());
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    out.extend_from_slice(&folder_name_len.to_le_bytes());
    out.extend_from_slice(&(name_block.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // content type
    out.extend_from_slice(&0u16.to_le_bytes());

    // Folder records: the name hash, the file count, and the offset of the
    // folder's own name (its position in the directory block) plus the name
    // block's length — the convention v104 uses and v105 keeps, and which the
    // reader checks against its own cursor.
    let mut name_pos = crate::HEADER_SIZE + folders.len() as u32 * 16;
    for (folder, members) in &folders {
        out.extend_from_slice(&crate::name_hash(folder.as_bytes(), false).to_le_bytes());
        out.extend_from_slice(&(members.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_pos + name_block.len() as u32).to_le_bytes());
        // This folder's name, then its file records.
        name_pos += folder.len() as u32 + 2 + members.len() as u32 * 16;
    }

    // Each folder's name, then its file records.
    let mut data_offset = data_start;
    for (folder, members) in &folders {
        out.extend_from_slice(&dir_name(folder));
        for &i in members {
            out.extend_from_slice(&crate::name_hash(files[i].name.as_bytes(), true).to_le_bytes());
            out.extend_from_slice(&(files[i].data.len() as u32).to_le_bytes());
            out.extend_from_slice(&data_offset.to_le_bytes());
            data_offset += files[i].data.len() as u32;
        }
    }
    out.extend_from_slice(&name_block);

    // The data blocks, in the same order as the file records: the reader maps
    // a record to its block by the record's own offset, so the two orders must
    // agree.
    for (_, members) in &folders {
        for &i in members {
            out.extend_from_slice(&files[i].data);
        }
    }

    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A written archive reads back exactly what was put in it, with no
    /// warnings: an archive the reader complains about is not a canonical one,
    /// and the precedence tests need the reader's own path taken cleanly.
    #[test]
    fn a_written_archive_reads_back() {
        let dir = std::env::temp_dir().join(format!("bsa105-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Test.bsa");
        write(
            &path,
            &[
                File {
                    folder: "Meshes".into(),
                    name: "a.nif".into(),
                    data: b"first".to_vec(),
                },
                File {
                    folder: "Meshes".into(),
                    name: "b.nif".into(),
                    data: b"second".to_vec(),
                },
                File {
                    folder: "Textures\\Clutter".into(),
                    name: "c.dds".into(),
                    data: b"third".to_vec(),
                },
            ],
        )
        .unwrap();

        let mut a = crate::Archive::open(&path).unwrap();
        assert_eq!(a.header.folder_count, 2);
        assert_eq!(a.header.file_count, 3);
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        for (want_path, want) in [
            ("Meshes\\a.nif", &b"first"[..]),
            ("Meshes\\b.nif", &b"second"[..]),
            ("Textures\\Clutter\\c.dds", &b"third"[..]),
        ] {
            let got = a.extract_by_path(want_path).unwrap();
            assert_eq!(got, want, "{want_path}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
