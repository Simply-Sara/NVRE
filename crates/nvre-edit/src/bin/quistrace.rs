//! MATO: the DATA sizes across the masters.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"MATO" { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig == *b"DATA" {
                println!("{} bytes: {}", s.data.len(),
                         s.data.iter().take(24).map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
    });
}
