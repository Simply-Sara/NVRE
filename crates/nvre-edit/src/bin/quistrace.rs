//! PERK: the EPFT and EPFD size of every record that has both.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut seen: BTreeMap<(u8, usize), usize> = BTreeMap::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"PERK" { return; }
        let Ok(subs) = r.subrecords() else { return };
        let e = subs.iter().find(|s| s.sig == *b"EPFT").map(|s| s.data[0]);
        let d = subs.iter().find(|s| s.sig == *b"EPFD").map(|s| s.data.len());
        if let (Some(e), Some(d)) = (e, d) { *seen.entry((e, d)).or_insert(0) += 1; }
    });
    for ((e, d), c) in seen { println!("EPFT={e} EPFD={d} bytes x{c}"); }
}
