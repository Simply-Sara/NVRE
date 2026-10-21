//! Skyrim: the fourth-to-eighth bytes of every 12-byte texture entry.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
use std::collections::BTreeMap;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"MODT" { continue; }
            let d = &s.data;
            // The bare form: the entries fill the field.
            let start = if d.len() >= 12 && d[0..4] == [2,0,0,0] { 12 } else { 0 };
            let mut i = start;
            while i + 12 <= d.len() {
                let e = d[i+4..i+8].iter().map(|b| format!("{b:02x}")).collect::<String>();
                *seen.entry(e).or_insert(0) += 1;
                i += 12;
            }
        }
    });
    let mut v: Vec<_> = seen.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (e, c) in v.into_iter().take(12) { println!("ext {e} x{c}"); }
}
