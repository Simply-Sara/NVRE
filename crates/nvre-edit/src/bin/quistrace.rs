//! CSTY: the CSGD bytes.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut seen: BTreeMap<usize, String> = BTreeMap::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"CSTY" { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"CSGD" { continue; }
            seen.entry(s.data.len()).or_insert_with(|| s.data.iter().map(|b| format!("{b:02x}")).collect());
        }
    });
    for (n, h) in seen { println!("CSGD {n} bytes: {h}"); }
}
