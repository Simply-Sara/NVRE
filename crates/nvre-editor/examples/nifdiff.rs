//! Where does a block's re-encoding differ from the bytes it was read from?
//!
//! The probe counts the mismatches; this finds them. It decodes a block,
//! writes it out again, and reports the first byte that differs and the
//! field it falls in.
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let data_dir = std::env::args().nth(1).unwrap();
    let schema_dir = std::env::args().nth(2).unwrap();
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).unwrap()
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).unwrap()
    };
    let vfs = nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).unwrap();
    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    // How many blocks of each type mismatch, and where the first difference
    // is.
    let mut by_type: std::collections::BTreeMap<String, (u64, usize)> =
        std::collections::BTreeMap::new();
    for p in paths.iter().take(limit) {
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            continue;
        };
        for i in 0..f.blocks.len() {
            let b = &f.blocks[i];
            let Some(fields) = schema.block_fields(&b.type_name) else {
                continue;
            };
            let Ok(st) = f.decode_block(i, &schema, &data) else {
                continue;
            };
            let mut lineage: Vec<String> = vec![b.type_name.clone()];
            lineage.extend(
                schema
                    .block_chain(&b.type_name)
                    .into_iter()
                    .map(str::to_string),
            );
            let mut out = Vec::new();
            if nvre_nif::encode::encode_fields(
                &schema,
                &f.header,
                &lineage,
                &fields,
                &st,
                None,
                &mut out,
            )
            .is_err()
            {
                continue;
            }
            let orig = &data[b.range.clone()];
            if out == orig {
                continue;
            }
            let at = out
                .iter()
                .zip(orig.iter())
                .position(|(a, c)| a != c)
                .unwrap_or(out.len().min(orig.len()));
            let e = by_type.entry(b.type_name.clone()).or_insert((0, at));
            e.0 += 1;
            if e.1 == usize::MAX {
                e.1 = at;
            }
        }
    }
    for (k, (n, at)) in by_type {
        println!("{k}: {n} mismatched, first difference at byte {at}");
    }
}
