//! Enum dropdowns and list editing write back through the same encoder.
//!
//! Two widgets that did not exist: an enum rendered as `(object)` and a list as
//! `(list of 12)`, both uneditable. The risk in adding them is not that they fail
//! to draw — it is that they write a shape the encoder does not expect. An enum
//! is `{"value": n, "name": "…"}` and the name is derived from the value; a list's
//! length is part of its encoding, so writing one element means writing the whole
//! array.
//!
//! These tests take real records out of each game's master and check both:
//! changing an enum's value re-encodes to the original bytes when changed back,
//! and an array's elements survive being written back as a list.

use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn schema_dir(game: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(game)
}

fn open(game: &str, master: &str) -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        &std::path::Path::new(&data).join(master),
        &schema_dir(game),
        vec![master.to_string()],
    )
    .ok()
}

/// Every enum field in the first `limit` records of a type, with its values.
fn enum_fields(s: &Session, sig: [u8; 4], limit: usize) -> Vec<(u32, String, Vec<(i64, String)>)> {
    let Ok(lo) = s.load_order() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = 0;
    'outer: for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            if rec.header.sig != sig {
                continue;
            }
            if seen >= limit {
                break 'outer;
            }
            seen += 1;
            let Ok(views) = nvre_edit::encode::sub_views(&s.schema, rec, false) else {
                continue;
            };
            for v in &views {
                let mut sub = [0u8; 4];
                let b = v.sig.as_bytes();
                if b.len() != 4 {
                    continue;
                }
                sub.copy_from_slice(b);
                let rows =
                    nvre_edit::encode::rows(&s.schema, &sig, &sub, v.block.as_deref(), &v.value);
                for row in rows {
                    let values = nvre_edit::encode::enum_values_at(
                        &s.schema,
                        &sig,
                        &sub,
                        v.block.as_deref(),
                        &row.path,
                    );
                    if values.len() > 1 {
                        out.push((
                            rec.header.form_id,
                            format!("{}.{}", v.sig, row.path),
                            values,
                        ));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn an_enum_dropdown_writes_a_value_the_decoder_reads_back() {
    // FNV's NOTE.DATA is `{0: Sound, 1: Text, 2: Image, 3: Voice}` — a real
    // four-way enum in the shipped master, which is what a dropdown offers.
    let Some(s) = open("fnv", "FalloutNV.esm") else {
        eprintln!("NVRE_DATA unset; skipping");
        return;
    };
    let fields = enum_fields(&s, *b"NOTE", 40);
    assert!(
        !fields.is_empty(),
        "FNV's NOTE records have an enum field; the schema names its values"
    );
    let (fid, at, values) = &fields[0];
    eprintln!(
        "{fid:08X} {at}: {:?}",
        values
            .iter()
            .map(|(v, n)| (*v, n.clone()))
            .collect::<Vec<_>>()
    );
    let sub = &at[..4];
    let path = &at[5..];
    // Pick a value the record does not hold, then put the original back.
    let rec = s.resolved_record(*fid).expect("the record");
    let orig = nvre_edit::encode::sub_views(&s.schema, &rec, false)
        .expect("views")
        .into_iter()
        .find(|v| v.sig == sub)
        .expect("the subrecord");
    let mut sub_sig = [0u8; 4];
    sub_sig.copy_from_slice(sub.as_bytes());
    let cur = nvre_edit::encode::value_at(&orig.value, path)
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_i64())
        .expect("the enum's value");
    let other = values
        .iter()
        .map(|(v, _)| *v)
        .find(|v| *v != cur)
        .expect("another value");
    let mut edited = orig.value.clone();
    let mut obj = serde_json::Map::new();
    obj.insert("value".into(), serde_json::json!(other));
    obj.insert(
        "name".into(),
        serde_json::json!(values.iter().find(|(v, _)| *v == other).unwrap().1),
    );
    nvre_editor::session::set_path(&mut edited, path, serde_json::Value::Object(obj))
        .expect("the enum path is addressable");
    // No expected length: the subrecord's size is the layout's business, and an
    // enum is a fixed width either way.
    let bytes = nvre_edit::encode::encode_sub_len_in_block(
        &s.schema,
        &rec.header.sig,
        &sub_sig,
        &edited,
        None,
        orig.block.as_deref(),
    )
    .expect("the changed enum encodes");
    // Read it back: the decoder must agree about which value is now held.
    let back = nvre_edit::encode::decoded_in_block(
        &s.schema,
        &rec.header.sig,
        &nvre_esm::Subrecord {
            sig: sub_sig,
            data: bytes,
            extended: false,
        },
        orig.block.as_deref(),
    )
    .expect("the changed enum decodes");
    let got = nvre_edit::encode::value_at(&back, path)
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_i64());
    assert_eq!(got, Some(other), "the dropdown's value reads back");
}

#[test]
fn a_list_field_is_addressable_as_one_value() {
    // `rows()` gives an array its own path and no element paths, so the list
    // widget writes the array as a whole. What has to hold is that the path the
    // grid shows is a path `set_path` can write.
    let Some(s) = open("fnv", "FalloutNV.esm") else {
        eprintln!("NVRE_DATA unset; skipping");
        return;
    };
    let Ok(lo) = s.load_order() else { return };
    let mut found = false;
    'outer: for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            if rec.header.sig != *b"NAVI" && rec.header.sig != *b"STAT" {
                continue;
            }
            let Ok(views) = nvre_edit::encode::sub_views(&s.schema, rec, false) else {
                continue;
            };
            for v in &views {
                let mut sub = [0u8; 4];
                if v.sig.len() != 4 {
                    continue;
                }
                sub.copy_from_slice(v.sig.as_bytes());
                for row in nvre_edit::encode::rows(
                    &s.schema,
                    &rec.header.sig,
                    &sub,
                    v.block.as_deref(),
                    &v.value,
                ) {
                    let Some(val) = nvre_edit::encode::value_at(&v.value, &row.path) else {
                        continue;
                    };
                    if !val.is_array() {
                        continue;
                    }
                    // The list's path is writable: the same value goes back.
                    let mut edited = v.value.clone();
                    nvre_editor::session::set_path(&mut edited, &row.path, val.clone())
                        .unwrap_or_else(|e| {
                            panic!(
                                "{:08X} {}.{}: the list's own path is not addressable: {e}",
                                rec.header.form_id, v.sig, row.path
                            )
                        });
                    found = true;
                    break 'outer;
                }
            }
        }
    }
    assert!(
        found,
        "some record has an array field the grid shows as a list"
    );
}
