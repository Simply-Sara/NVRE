//! SCEN's tail: what follows the blocks?
use fnv_esm::raw::visit_records;
use fnv_esm::Plugin;
fn main() {
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if n >= 3 { return; }
        let Ok(subs) = r.subrecords() else { return };
        // Find the last block opener, then show the tail.
        let last_action = subs.iter().rposition(|s| s.sig == *b"ANAM");
        let Some(la) = last_action else { return };
        n += 1;
        let tail: Vec<String> = subs[la..].iter().map(|x| format!("{}:{}", x.sig_str(), x.data.len())).collect();
        println!("tail from {la}: {}", tail.join(" "));
    });
}
