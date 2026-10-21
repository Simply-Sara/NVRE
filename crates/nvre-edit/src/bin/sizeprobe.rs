//! The size of a named subrecord inside a named record type.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let rec = std::env::args().nth(2).unwrap();
    let mut sizes: BTreeMap<(String, usize), u64> = Default::default();
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != rec.as_bytes() {
            return;
        }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs {
            *sizes.entry((s.sig_str(), s.data.len())).or_default() += 1;
        }
    });
    let mut by: BTreeMap<String, BTreeMap<usize, u64>> = Default::default();
    for ((sig, len), c) in sizes {
        by.entry(sig).or_default().insert(len, c);
    }
    for (sig, m) in &by {
        let h: Vec<String> = m.iter().map(|(l, c)| format!("{l}B x{c}")).collect();
        println!("  {sig:5} {}", h.join(", "));
    }
}
