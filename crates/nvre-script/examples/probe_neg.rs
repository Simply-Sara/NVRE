//! Probe: how does the game encode a negative literal in an expression?
//! `-5` after `==`: the token stream around it, corpus-wide, plus the
//! source lines that spell negative literals.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_neg
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
    let mut shapes: std::collections::BTreeMap<String, usize> = Default::default();
    let mut examples = Vec::new();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let compiled = subs
            .iter()
            .find(|x| x.sig == *b"SCDA")
            .map(|x| x.data.clone())
            .unwrap_or_default();
        let source = subs
            .iter()
            .find(|x| x.sig == *b"SCTX")
            .map(|x| String::from_utf8_lossy(&x.data).into_owned())
            .unwrap_or_default();
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if compiled.is_empty() {
            return;
        }
        // source lines with a negative literal in a condition or set
        let neg_lines: Vec<String> = source
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.contains("== -")
                    || t.contains("!= -")
                    || t.contains("< -")
                    || t.contains("> -")
                    || t.contains("<= -")
                    || t.contains(">= -")
                    || t.contains("to -")
            })
            .map(|l| l.trim().to_string())
            .collect();
        if neg_lines.is_empty() {
            return;
        }
        // find 20 <digits> 20 7E patterns and 20 2D patterns
        let mut has_7e = false;
        let mut has_2d = false;
        for w in compiled.windows(6) {
            if w[0] == 0x20 && w[2] == 0x20 && w[3] == 0x7E {
                has_7e = true;
            }
            if w[0] == 0x20 && w[1] == 0x2D {
                has_2d = true;
            }
        }
        if has_7e || has_2d {
            let key = format!("7E:{} 2D:{}", has_7e, has_2d);
            *shapes.entry(key.clone()).or_insert(0) += 1;
            if examples.len() < 10 {
                examples.push(format!("{edid}: {key}  lines: {neg_lines:?}"));
            }
        }
    });
    for (k, v) in &shapes {
        println!("{k}: {v} units");
    }
    for e in &examples {
        println!("  {e}");
    }
}
