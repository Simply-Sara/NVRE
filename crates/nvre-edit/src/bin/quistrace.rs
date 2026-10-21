//! LGTM: the DALC bytes.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"LGTM" || n >= 2 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"DALC" { continue; }
            n += 1;
            println!("DALC {} bytes: {}", s.data.len(),
                     s.data.iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
    });
}
