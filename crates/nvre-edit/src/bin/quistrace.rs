//! Decode one MODT with the schema and see what happens.
use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
fn main() {
    let schema = Schema::load_game("schemas/base", ["schemas/fnv", "schemas/skyrimse"]).unwrap();
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"ARTO" || n >= 3 {
            return;
        }
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"MODT" {
                continue;
            }
            let w = s
                .data
                .get(0..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .unwrap_or(0);
            if w == 2 {
                continue;
            }
            n += 1;
            let def = schema.lookup(b"ARTO", b"MODT").unwrap();
            println!("ARTO.MODT def type: {:?}", def.ty);
            println!(
                "ARTO.MODT name: {:?}",
                schema.lookup(b"ARTO", b"MODT").map(|d| d.name.clone())
            );
        }
    });
}
