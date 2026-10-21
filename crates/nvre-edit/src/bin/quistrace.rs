//! WTHR: the record with the odd fields.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"WTHR" { return; }
        let Ok(subs) = r.subrecords() else { return };
        let has = subs.iter().any(|s| s.sig == *b"ANAM" || s.sig == *b"BNAM");
        if !has || n >= 1 { return; }
        n += 1;
        let shape: Vec<String> = subs.iter().map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
        println!("{shape:?}");
    });
}
