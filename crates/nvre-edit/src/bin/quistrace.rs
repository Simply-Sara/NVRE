//! WTHR: LNAM and PNAM together.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"WTHR" { return; }
        let Ok(subs) = r.subrecords() else { return };
        let lnam = subs.iter().find(|s| s.sig == *b"LNAM").map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]));
        let pnam = subs.iter().find(|s| s.sig == *b"PNAM").map(|s| s.data.len());
        let (Some(l), Some(pn)) = (lnam, pnam) else { return };
        if n < 6 { println!("LNAM={l} PNAM={pn}"); }
        n += 1;
    });
}
