//! SCEN: what is around NEXT and QNAM?
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if n >= 3 { return; }
        let Ok(subs) = r.subrecords() else { return };
        for (i, s) in subs.iter().enumerate() {
            if s.sig != *b"NEXT" { continue; }
            n += 1;
            let lo = i.saturating_sub(4); let hi = (i+4).min(subs.len());
            let ctx: Vec<String> = subs[lo..hi].iter().map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
            println!("NEXT at {i}: {ctx:?}");
        }
    });
}
