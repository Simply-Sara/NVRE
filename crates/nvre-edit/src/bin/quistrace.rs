//! RACE: the DATA bytes of one record, in both editions.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"RACE" || n >= 1 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"DATA" { continue; }
            n += 1;
            println!("DATA {} bytes:", s.data.len());
            for (i, c) in s.data.chunks(4).enumerate() {
                println!("  {:3}: {}", i*4, c.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
    });
}
