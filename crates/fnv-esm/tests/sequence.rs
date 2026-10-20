//! The sequence walk: a record whose subrecords are an ordered list of blocks,
//! where the same signature means different things in different blocks.
//!
//! `SCEN` is the real case this exists for. Its `ALID` opens the actor list
//! *and* is a field inside an action, so the signature alone cannot say which
//! definition applies — the block the subrecord is in decides.

use fnv_esm::decode::{decode_record, Coverage};
use fnv_esm::raw::{visit_records, write_subrecords, Subrecord};
use fnv_esm::{Plugin, Schema};

/// A schema with one sequence record, `TEST`, whose `ALID` means two things.
const SCHEMA: &str = r#"{
  "format": "fnv.schema/1",
  "records": {
    "TEST": {
      "name": "Test",
      "confidence": "documented",
      "sequence": {
        "start": ["EDID"],
        "blocks": {
          "actor": {
            "opener": "ALID",
            "order": ["LNAM", "DNAM"],
            "subrecords": {
              "ALID": { "name": "Actor Alias", "confidence": "documented", "type": "u32" },
              "LNAM": { "name": "Actor Flags", "confidence": "documented", "type": "u32" },
              "DNAM": { "name": "Actor Behaviors", "confidence": "documented", "type": "u32" }
            }
          },
          "action": {
            "opener": "ANAM",
            "order": ["ALID", "INAM"],
            "subrecords": {
              "ANAM": { "name": "Action Type", "confidence": "documented", "type": "u16" },
              "ALID": { "name": "Action Actor", "confidence": "documented", "type": "u32" },
              "INAM": { "name": "Action Number", "confidence": "documented", "type": "u32" }
            }
          }
        }
      }
    }
  }
}"#;

fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
    Subrecord {
        sig: *sig,
        data: data.to_vec(),
        extended: false,
    }
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(sig);
    v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&fid.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&15u16.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn group(label: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"GRUP");
    v.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
    v.extend_from_slice(label);
    v.extend_from_slice(&0i32.to_le_bytes());
    v.extend_from_slice(&body.len().to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// A plugin with a valid header and one `TEST` record.
fn plugin(subs: &[Subrecord]) -> Plugin {
    let body = group(b"TEST", &record(b"TEST", 0, 0x100, &write_subrecords(subs)));
    let mut header = Vec::new();
    header.extend_from_slice(&1.34f32.to_le_bytes());
    header.extend_from_slice(&1i32.to_le_bytes());
    header.extend_from_slice(&0x800u32.to_le_bytes());
    let mut data = record(b"TES4", 1, 0, &write_subrecords(&[sub(b"HEDR", &header)]));
    data.extend_from_slice(&body);
    Plugin::parse(&data, "Test.esm".into()).expect("the synthetic plugin parses")
}

/// The names the decoder gave each subrecord, in order.
fn decoded_names(p: &Plugin, schema: &Schema) -> Vec<String> {
    let mut out = Vec::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        let v = decode_record(r, schema);
        let subs = v
            .get("subrecords")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        for s in subs {
            out.push(
                s.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
        }
    });
    out
}

/// The schema under test, built from the JSON above.
fn schema() -> Schema {
    let mut s = Schema::default();
    s.merge_overlay_str(SCHEMA, "test")
        .expect("the test schema parses");
    s
}

#[test]
fn a_signature_means_different_things_in_different_blocks() {
    // The actor list, then an action whose ALID is the actor performing it.
    let subs = vec![
        sub(b"ALID", &1u32.to_le_bytes()),
        sub(b"LNAM", &2u32.to_le_bytes()),
        sub(b"DNAM", &3u32.to_le_bytes()),
        sub(b"ANAM", &0u16.to_le_bytes()),
        sub(b"ALID", &4u32.to_le_bytes()),
        sub(b"INAM", &5u32.to_le_bytes()),
    ];
    let p = plugin(&subs);
    let names = decoded_names(&p, &schema());

    assert_eq!(
        names,
        vec![
            "Actor Alias", // the ALID that opens the actor list
            "Actor Flags",
            "Actor Behaviors",
            "Action Type",  // the ANAM that opens an action
            "Action Actor", // the ALID *inside* the action — a different field
            "Action Number",
        ]
    );
}

#[test]
fn a_repeated_opener_starts_a_new_block() {
    // Two actors in a row: the second ALID opens a second actor block.
    let subs = vec![
        sub(b"ALID", &1u32.to_le_bytes()),
        sub(b"LNAM", &2u32.to_le_bytes()),
        sub(b"DNAM", &3u32.to_le_bytes()),
        sub(b"ALID", &4u32.to_le_bytes()),
        sub(b"LNAM", &5u32.to_le_bytes()),
        sub(b"DNAM", &6u32.to_le_bytes()),
    ];
    let p = plugin(&subs);
    let names = decoded_names(&p, &schema());

    assert_eq!(
        names,
        vec![
            "Actor Alias",
            "Actor Flags",
            "Actor Behaviors",
            "Actor Alias",
            "Actor Flags",
            "Actor Behaviors",
        ]
    );
}

#[test]
fn coverage_counts_a_sequence_record_by_its_blocks() {
    let subs = vec![
        sub(b"ALID", &1u32.to_le_bytes()),
        sub(b"LNAM", &2u32.to_le_bytes()),
        sub(b"ANAM", &0u16.to_le_bytes()),
        sub(b"ALID", &3u32.to_le_bytes()),
    ];
    let p = plugin(&subs);
    let schema = schema();

    let mut cov = Coverage::default();
    // The record lives inside the group, so collect it by walking the tree.
    let mut recs: Vec<fnv_esm::Record> = Vec::new();
    visit_records(&p.nodes[1..], &mut |r, _| recs.push(r.clone()));
    let refs: Vec<&fnv_esm::Record> = recs.iter().collect();
    cov.add_records_par(&refs, &schema);

    // Every subrecord is known: none of them falls back to the record's own
    // (empty) map and none is left unknown.
    let total: u64 = cov.by_sub.values().map(|c| c.total).sum();
    let unknown: u64 = cov.by_sub.values().map(|c| c.unknown).sum();
    let errors: u64 = cov.by_sub.values().map(|c| c.error).sum();
    assert_eq!(unknown, 0, "no subrecord should be unknown");
    assert_eq!(errors, 0, "no subrecord should fail to decode");
    assert_eq!(total, 4, "all four subrecords are counted");
    assert!(
        cov.record_errors.is_empty(),
        "no record should fail: {:?}",
        cov.record_errors
    );
}

#[test]
fn a_block_definition_wins_over_the_record_map() {
    // The record's own map says ALID is a plain u32; the actor block calls it
    // "Actor Alias". The block must win.
    let schema_json = r#"{
      "format": "fnv.schema/1",
      "records": {
        "TEST": {
          "name": "Test",
          "confidence": "documented",
          "subrecords": { "ALID": { "name": "Record Level ALID", "confidence": "documented", "type": "u32" } },
          "sequence": {
            "start": [],
            "blocks": {
              "actor": {
                "opener": "ALID",
                "order": ["LNAM"],
                "subrecords": {
                  "ALID": { "name": "Actor Alias", "confidence": "documented", "type": "u32" },
                  "LNAM": { "name": "Actor Flags", "confidence": "documented", "type": "u32" }
                }
              }
            }
          }
        }
      }
    }"#;
    let mut s = Schema::default();
    s.merge_overlay_str(schema_json, "test").unwrap();

    let subs = vec![
        sub(b"ALID", &1u32.to_le_bytes()),
        sub(b"LNAM", &2u32.to_le_bytes()),
    ];
    let p = plugin(&subs);
    let names = decoded_names(&p, &s);
    assert_eq!(names, vec!["Actor Alias", "Actor Flags"]);
}
