//! Skyrim: does the third word count the trailing words?
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut bad = 0;
    let mut good = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        let Ok(subs) = r.subrecords() else { return };
        for s in subs.iter() {
            if s.sig != *b"MODT" { continue; }
            let d = &s.data;
            if d.len() < 12 || d[0..4] != [2, 0, 0, 0] { continue; }
            let count = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
            let extra = u32::from_le_bytes([d[8], d[9], d[10], d[11]]) as usize;
            if d.len() == 12 + count * 12 + extra * 4 { good += 1; } else {
                bad += 1;
                if bad < 4 { println!("MISMATCH len={} count={count} extra={extra}", d.len()); }
            }
        }
    });
    println!("good {good}, bad {bad}");
}
