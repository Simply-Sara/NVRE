//! Which block of one NIF is slow, and why?
//!
//! `nifslow` names the file; this names the block inside it and prints what the
//! schema asked for, so the cause is a field and a length rather than a guess.
//!
//! Usage: `nifone <schema dir[:layers]> <nif path>`

use nvre_nif::Schema;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_dir = args.next().expect("a schema directory");
    let want = args.next().expect("a nif path");

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    // The file is read straight off disk, so this does not need the archive.
    let data = std::fs::read(&want).expect("the nif file");
    let f = nvre_nif::NifFile::parse(&data, Some(&schema)).expect("the nif parses");
    println!("{}: {} blocks, {} bytes", want, f.blocks.len(), data.len());

    for (i, b) in f.blocks.iter().enumerate() {
        let t = Instant::now();
        let (status, decoded) = f.decode_block_status(i, &schema, &data);
        let decode = t.elapsed();
        let reencoded = decoded
            .as_ref()
            .map(|st| f.reencode_decoded(&schema, &data, i, st).ok())
            .flatten();
        let total = t.elapsed();
        if total.as_millis() > 50 {
            println!(
                "block {i} {} ({} bytes): decode {:?}, total {:?}, status {:?}, re-encodes {reencoded:?}",
                b.type_name,
                b.range.len(),
                decode,
                total,
                status
            );
            // What the schema would read, so a length expression that asks for
            // something enormous is visible.
            if let Some(fields) = schema.block_fields(&b.type_name) {
                let n: usize = fields
                    .iter()
                    .filter(|f| {
                        f.length.is_some()
                            || f.width.is_some()
                            || f.name.contains("vertex")
                            || f.name.contains("data")
                    })
                    .count();
                println!(
                    "    {n} of {} fields carry a length, a width, or are data",
                    fields.len()
                );
                for fl in fields
                    .iter()
                    .filter(|f| f.length.is_some() || f.width.is_some())
                {
                    println!(
                        "      {}: length {:?}, width {:?}",
                        fl.name, fl.length, fl.width
                    );
                }
            }
        }
    }
}
