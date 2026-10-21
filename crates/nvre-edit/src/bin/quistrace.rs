//! ACTI: the 76-byte DMDT.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"ACTI" || n >= 2 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"DMDT" || s.data.len() != 76 { continue; }
            n += 1;
            for (i, c) in s.data.chunks(12).enumerate() {
                println!("   {i:2}: {}", c.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
    });
}
