//! Is the WTHR wildcard parsed?
use fnv_esm::Schema;
fn main() {
    let s = Schema::load_game("schemas/base", ["schemas/fnv", "schemas/skyrimse"]).unwrap();
    let r = s.records.get(b"WTHR").unwrap();
    println!("wildcards: {:?}", r.wildcards.iter().map(|(k, d)| (String::from_utf8_lossy(k).to_string(), d.name.clone())).collect::<Vec<_>>());
    println!("lookup 00TX: {:?}", s.lookup(b"WTHR", b"00TX").map(|d| d.name.clone()));
    println!("lookup 10TX: {:?}", s.lookup(b"WTHR", b"10TX").map(|d| d.name.clone()));
}
