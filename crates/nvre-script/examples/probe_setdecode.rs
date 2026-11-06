//! Probe: decode one Set instruction's arguments structurally — destination,
//! value length, and the value's expression tokens — from the game's bytes.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_setdecode -- <EDID> <set-dest-name>
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "f".into()) else {
        return;
    };
    let wanted = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "VEuclidQuestSCRIPT".into());
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if !edid.eq_ignore_ascii_case(&wanted) {
            return;
        }
        let compiled = subs
            .iter()
            .find(|x| x.sig == *b"SCDA")
            .map(|x| x.data.clone())
            .unwrap_or_default();
        let ins = nvre_script::emit::read_instructions(&compiled);
        for (n, x) in ins.iter().enumerate() {
            if x.opcode != 0x0015 {
                continue;
            }
            let a = &x.args;
            if a.len() < 5 {
                continue;
            }
            // dest: tag + payload
            let dest = match a[0] {
                0x73 | 0x66 => format!(
                    "var tag {:02X} idx {}",
                    a[0],
                    u16::from_le_bytes([a[1], a[2]])
                ),
                0x72 => format!(
                    "ref compound {} idx {} tag {:02X} id {}",
                    "r",
                    u16::from_le_bytes([a[1], a[2]]),
                    a[3],
                    u16::from_le_bytes([a[4], a[5]])
                ),
                0x47 => format!("global idx {}", u16::from_le_bytes([a[1], a[2]])),
                t => format!("tag {t:02X}"),
            };
            // value: after dest (3 or 6 bytes), len:u16, then 20-prefixed tokens
            let voff = if a[0] == 0x72 && a.len() > 5 && (a[3] == 0x73 || a[3] == 0x66) {
                6
            } else {
                3
            };
            if a.len() < voff + 2 {
                continue;
            }
            let vlen = u16::from_le_bytes([a[voff], a[voff + 1]]) as usize;
            let value = &a[voff + 2..(voff + 2 + vlen).min(a.len())];
            // tokens: 20 sep, then tag+payload or ASCII to next sep
            let mut toks: Vec<String> = Vec::new();
            let mut p = 0;
            while p < value.len() {
                if value[p] != 0x20 {
                    p += 1;
                    continue;
                }
                let start = p + 1;
                let mut end = start;
                while end < value.len() && value[end] != 0x20 {
                    end += 1;
                }
                let t = &value[start..end];
                let desc = if t.len() >= 3
                    && (t[0] == 0x73
                        || t[0] == 0x66
                        || t[0] == 0x72
                        || t[0] == 0x5A
                        || t[0] == 0x47)
                {
                    format!("{:02X} idx {}", t[0], u16::from_le_bytes([t[1], t[2]]))
                } else if t.len() >= 5 && t[0] == 0x58 {
                    format!("X op {:02X?}", &t[1..5])
                } else if t.len() == 9 && t[0] == 0x7A {
                    format!(
                        "z f64 {}",
                        f64::from_le_bytes(t[1..9].try_into().unwrap_or([0; 8]))
                    )
                } else if t.len() == 5 && t[0] == 0x6E {
                    format!(
                        "n i32 {}",
                        i32::from_le_bytes(t[1..5].try_into().unwrap_or([0; 4]))
                    )
                } else if t.len() == 2 && (t[0] as char).is_ascii_alphanumeric() {
                    format!("op '{}'", String::from_utf8_lossy(t))
                } else {
                    format!("lit {}", String::from_utf8_lossy(t))
                };
                toks.push(desc);
                p = end.max(start);
            }
            println!("ins[{n}] Set dest: {dest}  valueLen {vlen}  tokens: {toks:?}");
            if vlen > 20 {
                println!("    raw value: {:02X?}", value);
            }
        }
    });
}
