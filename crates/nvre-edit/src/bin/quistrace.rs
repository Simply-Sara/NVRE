//! WEAP: the CRDT bytes.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"WEAP" || n >= 2 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"CRDT" { continue; }
            n += 1;
            println!("CRDT {} bytes: {}", s.data.len(),
                     s.data.iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
    });
}
