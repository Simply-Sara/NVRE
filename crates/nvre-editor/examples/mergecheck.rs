fn main() {
    let root = std::path::Path::new(".");
    let s = fnv_esm::Schema::load_game(&root.join("schemas/base"), &[root.join("schemas/fo4")]).unwrap();
    let rd = s.records.get(b"RACE").unwrap();
    let seq = rd.sequence.as_ref().unwrap();
    for b in &seq.blocks {
        if b.subrecords.contains_key(b"SAKD") {
            println!("block {} order {:?}", b.name, b.order);
        }
    }
}
