//! Find the ALID/ALED/ALLS/FNAM that fail, and print the record's shape.
use fnv_esm::decode::Coverage;
use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
fn main() {
    let schema = Schema::load_game("schemas/base", ["schemas/fnv", "schemas/skyrimse"]).unwrap();
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"QUST" { return; }
        let mut cov = Coverage::default();
        cov.add(Coverage::measure(r, &schema, None));
        let unk: Vec<String> = cov.by_sub.iter().filter(|(_, c)| c.unknown > 0)
            .map(|(k, c)| format!("{}.{}", k.0, k.1)).collect();
        if unk.is_empty() { return; }
        n += 1;
        if n > 2 { return; }
        let Ok(subs) = r.subrecords() else { return };
        let shape: Vec<String> = subs.iter().map(|x| x.sig_str().to_string()).collect();
        println!("QUST {n} unknown {unk:?}\n   {shape:?}");
    });
}
