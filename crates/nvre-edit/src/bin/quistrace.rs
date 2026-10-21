//! STAT and WTHR: the fields that leave bytes over.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        let sig = r.header.sig;
        if (sig != *b"STAT" && sig != *b"WTHR") || n >= 3 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if (sig == *b"STAT" && s.sig == *b"DNAM") || (sig == *b"WTHR" && s.sig == *b"HNAM") {
                n += 1;
                println!("{} {} bytes: {}", sig_str(&sig), s.data.len(),
                         s.data.iter().take(20).map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
    });
}
fn sig_str(sig: &[u8; 4]) -> String { String::from_utf8_lossy(sig).to_string() }
