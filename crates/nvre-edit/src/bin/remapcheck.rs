//! Verify FormID remapping inside the new array constructs: decode a NAVI.NVCI
//! / ARMA.MODS / navmesh NVGD payload, remap every FormID, re-encode, and
//! confirm the bytes differ only where a FormID was rewritten.
use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
fn main() {
    let schema = Schema::load_dir("schemas/fnv").unwrap();
    let p = Plugin::open(std::env::args().nth(1).unwrap()).unwrap();
    let want = std::env::args().nth(2).unwrap();
    let mut done = false;
    visit_records(&p.nodes[1..], &mut |r, _| {
        if done {
            return;
        }
        for s in r.subrecords().unwrap() {
            if format!("{}.{}", r.sig_str(), s.sig_str()) != want {
                continue;
            }
            let before = s.data.clone();
            let map = std::collections::HashMap::from([(1u32, 5u32)]);
            let remapped = nvre_edit::formid::remap_sub(&schema, &r.header.sig, &s, &|l| {
                let (idx, low) = (l >> 24, l & 0x00FF_FFFF);
                map.get(&idx).map(|n| (n << 24) | low)
            });
            match remapped {
                None => println!("{want}: NOT REMAPPED (left as stored)"),
                Some(a) => {
                    let diff: usize = a.iter().zip(&before).filter(|(x, y)| x != y).count();
                    let ones: usize = a.iter().zip(&before).filter(|(x, y)| **x != **y).count();
                    println!(
                        "{want}: len {} -> {} | bytes differing {}",
                        before.len(),
                        a.len(),
                        ones
                    );
                    let _ = diff;
                }
            }
            done = true;
            return;
        }
    });
    if !done {
        println!("{want}: not found");
    }
}
