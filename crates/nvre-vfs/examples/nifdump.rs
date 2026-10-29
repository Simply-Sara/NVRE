//! Dump one NIF's blocks: the type, the size and the raw bytes.
//!
//! Usage: `nifdump <schema dir[:layers]> <nif path> [type]`
//!
//! The blocks are split by NVRE's own parser, so the offsets are the ones
//! the reader uses. With a type, only the blocks of that type are shown.

use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_dir = args.next().expect("a schema directory");
    let want = args.next().expect("a nif path");
    let only = args.next();

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let data = std::fs::read(&want).expect("the nif file");
    let f = nvre_nif::NifFile::parse(&data, Some(&schema)).expect("the nif parses");
    println!(
        "{}: {} blocks, {} bytes, bs_version {}",
        want,
        f.blocks.len(),
        data.len(),
        f.header.bs_version()
    );

    for (i, b) in f.blocks.iter().enumerate() {
        if let Some(t) = only.as_deref() {
            if b.type_name != t {
                continue;
            }
        }
        let bytes = &data[b.range.clone()];
        println!("\n--- block {i}: {} size={}", b.type_name, bytes.len());
        for (o, chunk) in bytes.chunks(16).enumerate() {
            let hexs: Vec<String> = chunk.iter().map(|c| format!("{c:02x}")).collect();
            let txt: String = chunk
                .iter()
                .map(|&c| {
                    if (32..127).contains(&c) {
                        c as char
                    } else {
                        '.'
                    }
                })
                .collect();
            println!("{:5}: {:<47} {}", o * 16, hexs.join(" "), txt);
        }
    }
}
