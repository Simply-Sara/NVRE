fn main() {
    let root = std::path::Path::new(".");
    let s = nvre_esm::Schema::load_game(
        &root.join("schemas/base"),
        &[root.join("schemas/fo4").to_string_lossy().into_owned()],
    )
    .unwrap();
    let rd = s.records.get("RACE").unwrap();
    let seq = rd.sequence.as_ref().unwrap();
    for (bname, b) in &seq.blocks {
        if b.subrecords.contains_key("SAKD") {
            println!("block {bname} order {:?}", b.order);
        }
    }
}
