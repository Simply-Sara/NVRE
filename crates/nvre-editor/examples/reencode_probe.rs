//! Why a subrecord does not re-encode: decode it, put it back, and say which
//! union variant failed and how.
//!
//! The matrix reports "does not re-encode" and a first failing field. That is
//! enough to know something is wrong and not enough to know what, because the
//! failure is usually two levels down inside a nested union whose error message
//! is the *first* variant's, not the reason every variant failed.
//!
//! This walks the type by hand, printing each union variant's own result.
//!
//! Usage:
//!   reencode_probe --game skyrimse --sig RACE --sub DATA [--limit 1]

use nvre_editor::game::GameProfile;
use nvre_esm::schema::Prim;
use nvre_esm::TypeDef;

fn main() {
    let mut game = String::new();
    let mut want_sig = String::new();
    let mut want_sub = String::new();
    let mut want_fid: Option<u32> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().unwrap_or_default(),
            "--sig" => want_sig = args.next().unwrap_or_default(),
            "--sub" => want_sub = args.next().unwrap_or_default(),
            "--fid" => {
                want_fid = args
                    .next()
                    .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok())
            }
            other => {
                eprintln!("unknown {other:?}");
                std::process::exit(2);
            }
        }
    }
    let profile = GameProfile::by_id(&game).expect("a game");
    let schema = profile.load_schema().expect("a schema");
    let data = profile.data_dir().expect("a data folder");
    let session = nvre_editor::session::Session::open(
        &data.join(profile.master),
        &profile.schema_dirs().0,
        vec![profile.master.to_string()],
    )
    .expect("a session");
    let lo = session.load_order().expect("a load order");
    let sig = sig4(&want_sig);
    let mut shown = 0;
    for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            if rec.header.sig != sig || shown >= 400 {
                continue;
            }
            if let Some(want) = want_fid {
                if rec.header.form_id != want {
                    continue;
                }
            }
            let Ok(subs) = rec.subrecords() else { continue };
            let matching: Vec<_> = subs.iter().filter(|s| s.sig_str() == want_sub).collect();
            if matching.is_empty() {
                continue;
            }
            shown += 1;
            for (oi, sub) in matching.iter().enumerate() {
                println!("  occurrence {oi} of {}", matching.len());
                println!(
                    "\n{} {} {} len={}",
                    rec.header.form_id,
                    want_sig,
                    want_sub,
                    sub.data.len()
                );
                let Some(def) = schema.lookup(&sig, &sub.sig) else {
                    println!("  no layout");
                    continue;
                };
                let ty = schema.resolve_def(&def.ty);
                match nvre_edit::encode::decoded(&schema, &sig, sub) {
                    Some(v) => {
                        let keys: Vec<&String> = v
                            .as_object()
                            .map(|o| o.keys().collect())
                            .unwrap_or_default();
                        println!(
                            "  decoded {} keys: {:?}",
                            keys.len(),
                            &keys[..keys.len().min(8)]
                        );
                        walk(&schema, &ty, &v, sub.data.len(), 0);
                        // Exactly what the matrix does: every number field, +1 then
                        // -1, re-encoding with the original length.
                        let block = nvre_esm::decode::block_of(&sig, &subs, &schema, sub.sig);
                        for row in
                            nvre_edit::encode::rows(&schema, &sig, &sub.sig, block.as_deref(), &v)
                        {
                            let Some(cur) = nvre_edit::encode::value_at(&v, &row.path) else {
                                continue;
                            };
                            let Some(n) = cur.as_i64() else { continue };
                            if row.derived {
                                continue;
                            }
                            let mut done = false;
                            for cand in [n + 1, n - 1] {
                                let mut edited = v.clone();
                                if nvre_editor::session::set_path(
                                    &mut edited,
                                    &row.path,
                                    serde_json::json!(cand),
                                )
                                .is_err()
                                {
                                    continue;
                                }
                                match nvre_edit::encode::encode_sub_len_in_block(
                                    &schema,
                                    &sig,
                                    &sub.sig,
                                    &edited,
                                    Some(sub.data.len()),
                                    block.as_deref(),
                                ) {
                                    Ok(_) => {
                                        done = true;
                                        break;
                                    }
                                    Err(e) => {
                                        if cand == n + 1 {
                                            println!("  FAIL {} = {cand} (was {n}): {e}", row.path);
                                        }
                                    }
                                }
                            }
                            let _ = done;
                        }
                    }
                    None => println!("  does not decode"),
                }
            }
        }
    }
}

fn walk(schema: &nvre_esm::Schema, t: &TypeDef, v: &serde_json::Value, len: usize, depth: usize) {
    let pad = "  ".repeat(depth + 1);
    match t {
        TypeDef::Union(variants) => {
            println!(
                "{pad}union of {} variants, value has {} keys",
                variants.len(),
                v.as_object().map(|o| o.len()).unwrap_or(0)
            );
            for (size, vt) in variants {
                let mut out = Vec::new();
                let r = nvre_edit::encode::encode_type(schema, vt, v, &mut out, "");
                println!(
                    "{pad}  variant size={size:?} -> {}",
                    match &r {
                        Ok(()) => format!("ok, {} bytes", out.len()),
                        Err(e) => format!("err: {e}"),
                    }
                );
                if r.is_ok() && depth < 3 {
                    walk(schema, vt, v, len, depth + 1);
                }
            }
        }
        TypeDef::Struct(fields) => {
            if let Some(o) = v.as_object() {
                let declared: std::collections::BTreeSet<&str> =
                    fields.iter().map(|f| f.name.as_str()).collect();
                let extra: Vec<&String> = o
                    .keys()
                    .filter(|k| !declared.contains(k.as_str()))
                    .collect();
                let missing: Vec<&str> = fields
                    .iter()
                    .filter(|f| !f.optional && !o.contains_key(&f.name))
                    .map(|f| f.name.as_str())
                    .collect();
                if !extra.is_empty() || !missing.is_empty() {
                    println!("{pad}struct: extra={extra:?} missing={missing:?}");
                }
            }
        }
        TypeDef::Prim(Prim::FormId) | TypeDef::FormIdRef { .. } => {}
        _ => {}
    }
}

fn sig4(s: &str) -> [u8; 4] {
    let b = s.as_bytes();
    [b[0], b[1], b[2], b[3]]
}
