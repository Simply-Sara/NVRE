//! Do a game's NIFs decode, and does the schema explain them?
//!
//! `decode_block` says how well the schema explains a block, and
//! `reencode_block` says the schema's reading re-encodes to the same bytes.
//! A block that decodes but does not re-encode is a block the schema read
//! with the wrong layout — which is what a `bs_version` condition written
//! for another game does.
use fnv_nif::Schema;
use std::path::PathBuf;
fn main() {
    let data_dir = std::env::args().nth(1).unwrap();
    let schema_dir = std::env::args().nth(2).unwrap();
    let limit: usize = std::env::var("NIF_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    // The schema directory may be a base plus layers: the first path is the
    // base, the rest are layers over it.
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).unwrap()
    } else {
        Schema::load_game(base, &layers).unwrap()
    };
    // The archive list comes from the command line, so the same probe works
    // for a game whose archives are not FNV's.
    let archives: Vec<String> = std::env::args().skip(3).collect();
    let vfs = if archives.is_empty() {
        nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).unwrap()
    } else {
        nvre_vfs::Vfs::open(PathBuf::from(&data_dir).as_path(), &archives).unwrap()
    };
    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    println!("the archive holds {} nif files", paths.len());
    let mut files = 0u64;
    let mut parse_fail = 0u64;
    let mut blocks = 0u64;
    let mut unknown = 0u64;
    let mut short = 0u64;
    let mut err = 0u64;
    let mut mismatch = 0u64;
    let mut bs = std::collections::BTreeMap::new();
    let mut ver: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut fails: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut undef: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut types: std::collections::BTreeMap<i64, u64> = std::collections::BTreeMap::new();
    let mut sizes: std::collections::BTreeMap<usize, u64> = std::collections::BTreeMap::new();
    for p in paths.iter().take(limit) {
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        let f = match fnv_nif::NifFile::parse(&data, Some(&schema)) {
            Ok(f) => f,
            Err(_) => { parse_fail += 1; continue; }
        };
        files += 1;
        *bs.entry(f.header.bs_version()).or_insert(0u64) += 1;
        *ver.entry(format!("{:?}", f.header.version)).or_insert(0u64) += 1;
        for (i, _b) in f.blocks.iter().enumerate() {
            blocks += 1;
            match f.decode_block(&schema, &data, i) {
                Err(fnv_nif::BlockStatus::Unknown) => {
                    unknown += 1;
                    *fails.entry(format!("{} unknown", f.blocks[i].type_name)).or_insert(0) += 1;
                    *undef.entry(f.blocks[i].type_name.clone()).or_insert(0) += 1;
                }
                Err(fnv_nif::BlockStatus::Short { consumed, size }) => {
                    short += 1;
                    *fails.entry(format!("{} short: consumed {consumed} of {size}", f.blocks[i].type_name)).or_insert(0) += 1;
                    if f.blocks[i].type_name == "BSLightingShaderProperty" {
                        let (st, _, _) = f.decode_block_partial(&schema, &data, i);
                        let t = st.int("shader_type").unwrap_or(-99);
                        *types.entry(t).or_insert(0) += 1;
                    }
                }
                Err(fnv_nif::BlockStatus::Error(e)) => {
                    err += 1;
                    *fails.entry(format!("{} error: {e}", f.blocks[i].type_name)).or_insert(0) += 1;
                    if f.blocks[i].type_name.contains("RigidBody") {
                        *sizes.entry(f.blocks[i].range.len()).or_insert(0) += 1;
                    }
                }
                Err(_) => {}
                Ok(_) => {
                    *fails.entry(format!("{} ok", f.blocks[i].type_name)).or_insert(0) += 1;
                }
            }
            if f.reencode_block(&schema, &data, i) == Ok(false) {
                mismatch += 1;
            }
        }
    }
    println!("files {files}, parse failures {parse_fail}");
    println!("bs_versions: {bs:?}");
    println!("versions: {ver:?}");
    println!("blocks {blocks}, unknown {unknown}, short {short}, error {err}, re-encode mismatches {mismatch}");
    println!("undefined block types:");
    for (k, v) in &undef { println!("    {k}: {v}"); }
    println!("lighting shader types that failed: {types:?}");
    println!("rigid body sizes that failed: {sizes:?}");
    println!("by block:");
    for (k, v) in &fails { println!("    {k}: {v}"); }
}
