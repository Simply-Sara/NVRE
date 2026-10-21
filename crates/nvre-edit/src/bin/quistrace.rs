//! PACK: which CNAMs are unknown, and what block are they in?
use fnv_esm::decode::Coverage;
use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
fn main() {
    let schema = Schema::load_game("schemas/base", ["schemas/fnv", "schemas/skyrimse"]).unwrap();
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"PACK" { return; }
        let mut cov = Coverage::default();
        cov.add(Coverage::measure(r, &schema, None));
        let unk: Vec<String> = cov.by_sub.iter().filter(|(_, c)| c.unknown > 0)
            .map(|(k, c)| format!("{}.{} x{} sizes {:?}", k.0, k.1, c.unknown, c.sizes.keys().collect::<Vec<_>>())).collect();
        if unk.is_empty() { return; }
        n += 1;
        if n > 2 { return; }
        let Ok(subs) = r.subrecords() else { return };
        let shape: Vec<String> = subs.iter().take(16).map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
        println!("PACK {n} {unk:?}\n   {shape:?}");
    });
}
