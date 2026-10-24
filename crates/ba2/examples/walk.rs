//! What does the BA2 reader make of a real archive?
//!
//! Reads an archive, prints what it found, and extracts a few files so the
//! caller can compare them against the game's own data.
use ba2::{Archive, ArchiveType};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("an archive path"));
    let want_ext = args.next().unwrap_or_default();
    let limit: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let out_dir = args.next().map(PathBuf::from);

    let mut a = Archive::open(&path).expect("the archive");
    println!(
        "{}: {:?} v{}, {} files, name table at {}, {} warnings",
        path.display(),
        a.archive_type,
        a.version,
        a.num_files,
        a.name_table_offset,
        a.warnings.len()
    );
    for w in &a.warnings {
        println!("  warning {}: {}", w.code, w.detail);
    }
    match a.archive_type {
        ArchiveType::Gnrl => {
            println!("  {} GNRL entries", a.files.len());
            let by_ext = |e: &str| {
                a.files
                    .iter()
                    .filter(|f| f.extension.eq_ignore_ascii_case(e))
                    .count()
            };
            for e in ["nif", "dds", "bgsm", "wav"] {
                let n = by_ext(e);
                if n > 0 {
                    println!("    .{e}: {n}");
                }
            }
            println!("  first 3 paths:");
            for f in a.files.iter().take(3) {
                println!(
                    "    {} (offset {}, packed {}, unpacked {}, flags {:#x})",
                    f.path, f.data_offset, f.packed_size, f.unpacked_size, f.flags
                );
            }
            // Extract a few of the requested extension.
            let mut done = 0;
            let picks: Vec<_> = a
                .files
                .iter()
                .filter(|f| want_ext.is_empty() || f.extension.eq_ignore_ascii_case(&want_ext))
                .take(limit)
                .cloned()
                .collect();
            for f in picks {
                match a.extract(&f) {
                    Ok(data) => {
                        println!("    extracted {} ({} bytes)", f.path, data.len());
                        if let Some(dir) = &out_dir {
                            std::fs::create_dir_all(dir).ok();
                            let name = f.path.replace('\\', "_");
                            std::fs::write(dir.join(name), &data).ok();
                        }
                        done += 1;
                    }
                    Err(e) => println!("    FAILED {}: {e}", f.path),
                }
            }
            println!("  extracted {done} of {limit}");
        }
        ArchiveType::Dx10 => {
            println!("  {} DX10 textures", a.textures.len());
            for t in a.textures.iter().take(3) {
                println!(
                    "    {} {}x{} mips {} format {} ({}) {} chunks, cubemap {}, tile {}",
                    t.path,
                    t.width,
                    t.height,
                    t.num_mips,
                    t.format,
                    ba2::dxgi_format_name(t.format),
                    t.num_chunks,
                    t.is_cubemap,
                    t.tile_mode
                );
            }
            let mut done = 0;
            let picks: Vec<_> = a.textures.iter().take(limit).cloned().collect();
            for t in picks {
                match a.extract_texture(&t) {
                    Ok(mips) => {
                        let dds = ba2::dds_from_texture(&t, &mips);
                        println!(
                            "    extracted {}: {} mip bytes -> {} byte DDS",
                            t.path,
                            mips.len(),
                            dds.len()
                        );
                        if let Some(dir) = &out_dir {
                            std::fs::create_dir_all(dir).ok();
                            let name = t.path.replace('\\', "_");
                            std::fs::write(dir.join(name), &dds).ok();
                        }
                        done += 1;
                    }
                    Err(e) => println!("    FAILED {}: {e}", t.path),
                }
            }
            println!("  extracted {done} of {limit}");
        }
        ArchiveType::Gnmf => println!("  GNMF is not read"),
    }
}
