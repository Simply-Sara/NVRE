//! Which record types does a plugin actually contain?
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        *counts.entry(String::from_utf8_lossy(&r.header.sig).to_string()).or_insert(0) += 1;
    });
    for (k, v) in counts { println!("{k} {v}"); }
}
