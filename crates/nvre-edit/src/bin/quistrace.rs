//! MISC: the KWDA and its siblings.
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"MISC" || n >= 3 { return; }
        let Ok(subs) = r.subrecords() else { return };
        let has = subs.iter().any(|s| s.sig == *b"KWDA");
        if !has { return; }
        n += 1;
        let ctx: Vec<String> = subs.iter().map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
        println!("MISC: {}", ctx.join(" "));
    });
}
