//! LCTN: the LCPR, LCUN, LCSR, LCEC and LCEP headers.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut seen: BTreeMap<String, (usize, String)> = BTreeMap::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"LCTN" { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if !matches!(s.sig_str().as_str(), "LCPR"|"LCUN"|"LCSR"|"LCEC"|"LCEP") { continue; }
            let k = format!("{} len={}", s.sig_str(), s.data.len());
            let e = seen.entry(k).or_insert((0, s.data.iter().take(8).map(|b| format!("{b:02x}")).collect()));
            e.0 += 1;
        }
    });
    for (k, (c, h)) in seen { println!("{k} x{c} head={h}"); }
}
