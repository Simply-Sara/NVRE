//! What does a block's re-encoding look like, next to the bytes it was read
//! from?
//!
//! The probe counts the mismatches; this shows one.
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let data_dir = std::env::args().nth(1).unwrap();
    let schema_dir = std::env::args().nth(2).unwrap();
    let want = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "NiTransformData".into());
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).unwrap()
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).unwrap()
    };
    let mut dirs2 = schema_dir.split(':');
    let b2 = dirs2.next().unwrap();
    let l2: Vec<String> = dirs2.map(str::to_string).collect();
    let _ = b2;
    let _ = l2;
    let vfs = nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).unwrap();
    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    for p in paths.iter().take(2000) {
        let Some(Ok(data)) = vfs.read(p) else {
            continue;
        };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            continue;
        };
        for (i, b) in f.blocks.iter().enumerate() {
            if b.type_name != want {
                continue;
            }
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
                &schema, &f.header, &lineage, &fields, &st, None, &mut out,
            )
            .is_err()
            {
                continue;
            }
            let orig = &data[b.range.clone()];
            if out == orig {
                continue;
            }
            println!("the file: {p}, block {i}: {}", b.type_name);
            println!("the schema's fields:");
            for fld in &fields {
                println!(
                    "  {} {:?} cond={:?} len={:?}",
                    fld.name, fld.ty, fld.cond, fld.length
                );
            }
            println!("the template chain:");
            println!("  the block's template: None");
            println!("  KeyGroup's template: from the field (xyz_rotations -> f32)");
            println!("the values:");
            for (n, v) in &st.fields {
                let s = format!("{v:?}");
                println!("  {n} = {}", &s[..s.len().min(400)]);
            }
            let n = out.len().min(orig.len()).min(64);
            println!("re-encoded: {:02x?}", &out[..n]);
            println!("as read:    {:02x?}", &orig[..n]);
            return;
        }
    }
    println!("no mismatching {want} found");
}
