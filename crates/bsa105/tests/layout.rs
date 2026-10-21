//! Round-trip and layout tests for the v105 reader.
//!
//! The important tests are the ones that would catch a misread of the folder
//! record, because that is the one structural difference from v104 and it does
//! not fail loudly when wrong — it desyncs every name and hash after the first
//! folder. A wrong record size therefore shows up as hash mismatches, not as an
//! error, so the hash check is the real assertion.

use bsa105::*;

/// The archive Bethesda ships with Skyrim Special Edition, if present. Tests
/// that need it are skipped when the game is not installed.
fn se_data() -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(
        "/home/sara/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
    );
    p.is_dir().then(|| p.to_path_buf())
}

fn find_bsa(dir: &std::path::Path, stem: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(stem) && n.ends_with(".bsa"))
        })
}

#[test]
fn folder_record_is_24_bytes() {
    // The whole point of the crate: v105's folder record is 8 + 4 + 4 + 4 + 4.
    // If this ever changes, every name after the first folder desyncs.
    assert_eq!(FOLDER_RECORD_SIZE, 24);
    assert_eq!(HEADER_SIZE, 36);
    assert_eq!(VERSION, 105);
}

#[test]
fn name_hash_matches_stored_hashes_in_every_se_archive() {
    // The load-bearing test. Reading 24-byte folder records is what makes the
    // names line up; if that assumption is wrong, every hash below mismatches.
    let Some(dir) = se_data() else {
        eprintln!("Skyrim SE not installed; skipping");
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bsa")))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no .bsa in {dir:?}");
    let mut archives = 0;
    let mut hashes = 0u64;
    for p in &paths {
        let a = Archive::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(a.header.version, 105);
        archives += 1;
        for f in &a.folders {
            if !f.name.is_empty() {
                assert_eq!(
                    name_hash(&f.name, false),
                    f.hash,
                    "{}: folder hash mismatch for {:?} — the folder record size is wrong",
                    p.display(),
                    String::from_utf8_lossy(&f.name)
                );
            }
            for r in &f.files {
                assert_eq!(
                    name_hash(&r.name, true),
                    r.hash,
                    "{}: file hash mismatch for {:?}",
                    p.display(),
                    String::from_utf8_lossy(&r.name)
                );
                hashes += 1;
            }
        }
    }
    assert!(archives >= 20, "expected the full SE set, got {archives}");
    assert!(hashes > 100_000, "expected >100k files, hashed {hashes}");
}

#[test]
fn every_se_archive_extracts_its_first_files() {
    let Some(dir) = se_data() else { return };
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bsa")))
        .collect();
    paths.sort();
    for p in &paths {
        let mut a = Archive::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let sample: Vec<(String, u32, u32)> = a
            .entries()
            .take(3)
            .map(|e| (e.path(), e.record.stored_size(), e.record.offset))
            .collect();
        for (path, _sz, _off) in sample {
            let Some((fi, ri)) = a.find(&path) else {
                panic!("{}: lookup of {path} failed", p.display());
            };
            let rec = a.folders[fi].files[ri].clone();
            let out = a
                .extract(&rec)
                .unwrap_or_else(|e| panic!("{}: extracting {path}: {e}", p.display()));
            assert!(!out.is_empty(), "{path} extracted empty");
        }
    }
}

#[test]
fn uncompressed_archives_are_recognised() {
    // `Skyrim - Misc.bsa` has flags 0x13: no COMPRESSED_DEFAULT, so its blocks
    // are raw. A reader that compressed everything would fail here.
    let Some(dir) = se_data() else { return };
    let Some(p) = find_bsa(&dir, "Skyrim - Misc") else {
        return;
    };
    let mut a = Archive::open(&p).unwrap();
    assert_eq!(a.header.archive_flags & flags::COMPRESSED_DEFAULT, 0);
    let (fi, ri) = a.find("seq\\skyrim.seq").expect("skyrim.seq");
    let rec = a.folders[fi].files[ri].clone();
    let out = a.extract(&rec).unwrap();
    assert_eq!(out.len() as u32, rec.stored_size());
    // A .seq is not text and has no count header: it is a flat array of 5-byte
    // records, a u32 FormID plus a tag-type byte. Every .seq in the shipped
    // archives divides evenly by 5 (skyrim 1320 = 264, dawnguard 204, fish 40).
    assert_eq!(
        out.len() % 5,
        0,
        "{} bytes is not whole 5-byte records",
        out.len()
    );
    assert!(out.len() >= 5, "shorter than one record");
}

#[test]
fn lz4_compressed_blocks_decode_to_their_original_size() {
    // `Skyrim - Meshes0.bsa` is flags 0x87, so it is LZ4. The compressed block
    // carries its original size in the leading u32; the decoder must hit it.
    let Some(dir) = se_data() else { return };
    let Some(p) = find_bsa(&dir, "Skyrim - Meshes0") else {
        return;
    };
    let mut a = Archive::open(&p).unwrap();
    assert_ne!(a.header.archive_flags & flags::COMPRESSED_DEFAULT, 0);
    let (fi, ri) = a
        .find("meshes\\dungeons\\nordic\\levers\\pullchainanim\\norpullchainanim01.nif")
        .expect("pullchain nif");
    let rec = a.folders[fi].files[ri].clone();
    assert!(a.is_compressed(&rec));
    let raw = a.read_raw(&rec).unwrap();
    assert!(raw.starts_with(&[0x04, 0x22, 0x4D, 0x18]) || raw[4..8] == [0x04, 0x22, 0x4D, 0x18]);
    let original = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
    let out = a.extract(&rec).unwrap();
    assert_eq!(
        out.len(),
        original,
        "decoded size must match the block's own header"
    );
    assert!(
        out.starts_with(b"Gamebryo"),
        "not a NIF: {:02x?}",
        &out[..8.min(out.len())]
    );
}

#[test]
fn find_accepts_both_separators_and_is_case_insensitive() {
    let Some(dir) = se_data() else { return };
    let Some(p) = find_bsa(&dir, "Skyrim - Meshes0") else {
        return;
    };
    let a = Archive::open(&p).unwrap();
    let fwd = a.find("meshes/dungeons/nordic/levers/pullchainanim/norpullchainanim01.nif");
    let back = a.find("meshes\\dungeons\\nordic\\levers\\pullchainanim\\norpullchainanim01.nif");
    assert!(fwd.is_some() && back.is_some());
    assert_eq!(fwd, back);
    assert!(a.find("meshes\\nope\\missing.nif").is_none());
}

#[test]
fn non_bsa_and_wrong_version_are_rejected() {
    // A Fallout 4 BA2 begins "BTDX", and a file that is not a BSA at all must
    // be refused rather than misread.
    let dir = std::env::temp_dir().join("bsa105-reject");
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("not-a-bsa.bsa");
    let mut not_bsa = b"NOPE".to_vec();
    not_bsa.extend_from_slice(&[0u8; 32]);
    std::fs::write(&bad, not_bsa).unwrap();
    assert!(matches!(Archive::open(&bad), Err(Bsa105Error::NotBsa)));

    // A version this reader does not know is refused: the folder record's
    // size and the payload's codec are both read from the version, so an
    // unknown one cannot be read safely.
    let unknown = dir.join("unknown-version.bsa");
    let mut hdr = [0u8; 36];
    hdr[..4].copy_from_slice(&MAGIC);
    hdr[4..8].copy_from_slice(&107u32.to_le_bytes());
    std::fs::write(&unknown, hdr).unwrap();
    assert!(matches!(
        Archive::open(&unknown),
        Err(Bsa105Error::UnsupportedVersion(107))
    ));
}

#[test]
fn both_archive_versions_are_accepted() {
    // Version 104 (the original edition) and version 105 (the Special
    // Edition) differ in exactly two places: the folder record is 16 bytes
    // rather than 24, and the payload may be an LZ4 frame rather than zlib.
    // One reader handles both.
    let dir = std::env::temp_dir().join("bsa105-versions");
    std::fs::create_dir_all(&dir).unwrap();
    for version in [104u32, 105u32] {
        let path = dir.join(format!("v{version}.bsa"));
        let mut hdr = [0u8; 36];
        hdr[..4].copy_from_slice(&MAGIC);
        hdr[4..8].copy_from_slice(&version.to_le_bytes());
        std::fs::write(&path, hdr).unwrap();
        // The header is read; there is no folder block in this stub, so the
        // archive opens with no entries rather than being refused.
        let a = Archive::open(&path).expect("a version this reader knows");
        assert_eq!(a.entries().count(), 0, "the stub has no entries");
    }
}

#[test]
fn name_hash_uses_the_documented_extension_markers() {
    // Same file stem, different extension bits.
    let a = name_hash(b"foo\\bar\\baz.nif", true);
    let b = name_hash(b"foo\\bar\\baz.nif", true);
    assert_eq!(a, b, "hash must be deterministic");
    let c = name_hash(b"foo/bar/baz.nif", true);
    assert_eq!(a, c, "forward slash normalises to backslash");
    let d = name_hash(b"FOO\\BAR\\BAZ.NIF", true);
    assert_eq!(a, d, "hash is case-insensitive");
    let e = name_hash(b"foo\\bar\\baz.dds", true);
    assert_ne!(
        a, e,
        "different extension changes the low word's marker bits"
    );
}

#[test]
fn size_field_bits_decode_as_documented() {
    let r = FileRecord {
        hash: 0,
        size_field: 0x4000_1234,
        offset: 0,
        name: b"x".to_vec(),
    };
    assert_eq!(r.stored_size(), 0x1234);
    assert!(r.toggles_compression());
    let r2 = FileRecord {
        hash: 0,
        size_field: 0x1234,
        offset: 0,
        name: b"x".to_vec(),
    };
    assert_eq!(r2.stored_size(), 0x1234);
    assert!(!r2.toggles_compression());
}
