//! Compare NVRE's NIF reader against the one it replaces, on the same block.
//!
//! This is a measurement tool for the migration: it decodes one block with
//! both readers and prints the fields side by side, so the first difference
//! is where the new reader and the old one part company.
fn main() {
    let want = std::env::args().nth(1).expect("a block type");
    let data_dir = std::env::args().nth(2).expect("a data folder");
    let schema_dir = std::env::args().nth(3).expect("a schema directory");

    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();

    let new_schema = if layers.is_empty() {
        nvre_nif::Schema::load_dir(std::path::Path::new(base)).expect("the new schema")
    } else {
        nvre_nif::Schema::load_game(std::path::Path::new(base), &layers).expect("the new schema")
    };
    let old_schema = if layers.is_empty() {
        fnv_nif::Schema::load_dir(std::path::Path::new(base)).expect("the old schema")
    } else {
        fnv_nif::Schema::load_game(std::path::Path::new(base), &layers).expect("the old schema")
    };

    let vfs = nvre_vfs::Vfs::open_default(std::path::PathBuf::from(&data_dir).as_path())
        .expect("the archives");

    for p in vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        let Some(r) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&new_schema)) else {
            continue;
        };
        for (i, b) in f.blocks.iter().enumerate() {
            if b.type_name != want {
                continue;
            }
            let (ns, _) = f.decode_block_status(i, &new_schema, &data);
            let new = f.decode_block(i, &new_schema, &data).ok();

            // The old reader, on the same block.
            let old_file = fnv_nif::NifFile::parse(&data, Some(&old_schema)).ok();
            let old = old_file.as_ref().and_then(|of| {
                of.blocks
                    .iter()
                    .position(|ob| ob.type_name == want)
                    .and_then(|j| of.decode_block(&old_schema, &data, j).ok())
            });

            println!("{want}: new status {ns:?}");
            println!("  the block is {} bytes", b.range.end - b.range.start);
            let nf = new.as_ref().map(|s| s.fields.clone()).unwrap_or_default();
            let of = old
                .as_ref()
                .map(|s| s.fields.clone())
                .unwrap_or_else(|| Vec::new());
            let names: Vec<String> = nf
                .iter()
                .map(|(n, _)| n.clone())
                .chain(of.iter().map(|(n, _)| n.to_string()))
                .collect();
            let mut seen = Vec::new();
            for n in names {
                if seen.contains(&n) {
                    continue;
                }
                seen.push(n.clone());
                let n: &str = &n;
                let nv = nf
                    .iter()
                    .find(|(k, _)| &**k == n)
                    .map(|(_, v)| format!("{v:?}"))
                    .unwrap_or_else(|| "(absent)".into());
                let ov = of
                    .iter()
                    .find(|(k, _)| &**k == n)
                    .map(|(_, v)| format!("{v:?}"))
                    .unwrap_or_else(|| "(absent)".into());
                let mark = if nv == ov { "  " } else { "!!" };
                println!("{mark} {n:28} new {nv:40} old {ov}");
            }
            return;
        }
    }
    println!("no {want} found");
}
