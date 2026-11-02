//! Probe: do `&&` and `||` commute in the game's RPN? For every unit whose
//! condition contains both, decode the game's operator tokens (separator-
//! aligned, so no byte inside a literal or call is misread) and compare
//! with the source's infix order.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_prec
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

/// The logical operators of an expression, in RPN order, read
/// separator-aligned.
fn logical_ops(expr: &[u8]) -> Vec<&'static str> {
    let mut ops = Vec::new();
    let mut p = 0usize;
    while p < expr.len() {
        if expr[p] != 0x20 {
            p += 1;
            continue;
        }
        // A token runs to the next separator. Its shape decides its length:
        // a tagged value is tag + payload, a literal/operator is ASCII to
        // the next separator. Read the token, then classify.
        let start = p + 1;
        let mut end = start;
        while end < expr.len() && expr[end] != 0x20 {
            end += 1;
        }
        let tok = &expr[start..end];
        if tok == b"&&" {
            ops.push("&&");
        } else if tok == b"||" {
            ops.push("||");
        }
        p = if end > start { end } else { p + 1 };
    }
    ops
}

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
    let mut both = 0usize;
    let mut or_first = 0usize;
    let mut and_first = 0usize;
    let mut or_first_examples: Vec<String> = Vec::new();
    let mut and_first_examples: Vec<String> = Vec::new();
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
        let ins = nvre_script::emit::read_instructions(&compiled);
        for x in &ins {
            if x.opcode != 0x16 && x.opcode != 0x18 {
                continue;
            }
            if x.args.len() < 4 {
                continue;
            }
            let expr_len = u16::from_le_bytes([x.args[2], x.args[3]]) as usize;
            let expr = &x.args[4..(4 + expr_len).min(x.args.len())];
            let ops = logical_ops(expr);
            let has_and = ops.contains(&"&&");
            let has_or = ops.contains(&"||");
            if has_and && has_or {
                both += 1;
                if ops[0] == "||" {
                    or_first += 1;
                    if or_first_examples.len() < 4 {
                        or_first_examples.push(format!("{edid}: RPN {ops:?}"));
                    }
                } else {
                    and_first += 1;
                    if and_first_examples.len() < 4 {
                        and_first_examples.push(format!("{edid}: RPN {ops:?}"));
                    }
                }
            }
        }
    });
    println!("conditions with both && and ||: {both}");
    println!("RPN's first logical operator is ||: {or_first}");
    println!("RPN's first logical operator is &&: {and_first}");
    println!("||-first examples (the game compiled || before &&):");
    for e in &or_first_examples {
        println!("  {e}");
    }
    println!("&&-first examples:");
    for e in &and_first_examples {
        println!("  {e}");
    }
}
