//! Is a FormID in a master? A quick check against the real data.
use nvre_esm::Plugin;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap();
    let raw = args.next().unwrap();
    let want = u32::from_str_radix(raw.trim_start_matches("0x"), 16).unwrap();
    let p = Plugin::open(std::path::Path::new(&path)).unwrap();
    let mut found = 0u64;
    nvre_esm::raw::visit_nodes(&p.nodes, &mut |r, _| {
        if r.header.form_id == want {
            found += 1;
            if found <= 3 {
                println!("  {} {:08X}", r.sig_str(), r.header.form_id);
            }
        }
    });
    println!("the records with {:08X}: {found}", want);
}
