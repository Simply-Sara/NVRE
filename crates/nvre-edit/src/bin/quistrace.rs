//! PACK: what is the ObjectList activity's own structure?
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"PACK" { return; }
        let Ok(subs) = r.subrecords() else { return };
        for (i, s) in subs.iter().enumerate() {
            if s.sig != *b"ANAM" { continue; }
            let v = String::from_utf8_lossy(&s.data[..s.data.len().saturating_sub(1)]).to_string();
            if v != "ObjectList" { continue; }
            n += 1;
            if n > 1 { return; }
            let lo = i.saturating_sub(2); let hi = (i+12).min(subs.len());
            let ctx: Vec<String> = subs[lo..hi].iter().map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
            println!("ObjectList: {ctx:?}");
        }
    });
}
