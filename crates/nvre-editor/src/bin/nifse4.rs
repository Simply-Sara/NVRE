//! One block's bytes and the fields the schema read, side by side.
use bsa105::Archive;
use fnv_nif::Schema;
use std::path::PathBuf;
fn main() {
    let data_dir = std::env::args().nth(1).unwrap();
    let schema_dir = std::env::args().nth(2).unwrap();
    let want = std::env::args().nth(3).unwrap();
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = Schema::load_game(base, &layers).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&data_dir).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bsa"))
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    for name in &names {
        if !name.contains("Mesh") { continue; }
        let Ok(mut a) = Archive::open(PathBuf::from(&data_dir).join(name)) else { continue };
        let paths: Vec<String> = a.entries().map(|e| e.path()).collect();
        for p in paths {
            if !p.to_lowercase().ends_with(".nif") { continue; }
            let Ok(data) = a.extract_by_path(&p) else { continue };
            let Ok(f) = fnv_nif::NifFile::parse(&data, Some(&schema)) else { continue };
            for (i, blk) in f.blocks.iter().enumerate() {
                if blk.type_name != want { continue; }
                let (st, consumed, size) = f.decode_block_partial(&schema, &data, i);
                println!("{} block {i} {size} bytes, consumed {consumed}", p);
                for (n, v) in &st.fields {
                    let s = format!("{v:?}");
                    println!("  {n:16} {}", &s[..s.len().min(300)]);
                }
                let bytes = &data[blk.range.clone()];
                for (row, chunk) in bytes.chunks(16).take(8).enumerate() {
                    let hex: Vec<String> = chunk.iter().map(|x| format!("{x:02x}")).collect();
                    println!("  {row:3} {:48} {}", row * 16, hex.join(" "));
                }
                return;
            }
        }
    }
}
