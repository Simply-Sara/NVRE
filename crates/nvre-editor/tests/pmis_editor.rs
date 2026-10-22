//! The Placed Missile (PMIS) record's coverage.
//!
//! The shipped masters hold no `PMIS` records, so this suite builds one
//! from the placed grenade's layout, which the missile's spec points at,
//! and proves the page reads and writes every field it names.

mod common;

use common::{rawsub, write_synthetic_plugin, zsub};
use nvre_editor::object_editor::ObjectEditorState;

/// A `PMIS` record with every field the placed grenade's page names.
fn build_pmis(path: &std::path::Path) -> u32 {
    let form_id = 0x0F000002;

    // `DATA` — the position, the rotation, the bounding radius, the flags
    // and the six unnamed bytes.
    let mut data = Vec::new();
    for v in [1.0f32, 2.0, 3.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0.5f32, 0.25, 0.125] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&12.5f32.to_le_bytes());
    data.extend_from_slice(&0x20u32.to_le_bytes());
    data.extend_from_slice(&[9, 8, 7, 6, 5, 4]);

    let subs = vec![
        zsub(b"EDID", "TestMissile"),
        rawsub(b"NAME", 0x0F000100u32.to_le_bytes().to_vec()),
        rawsub(b"XEZN", 0x0F000101u32.to_le_bytes().to_vec()),
        rawsub(b"XRGD", vec![1, 2, 3, 4]),
        rawsub(b"XRGB", vec![5, 6, 7, 8]),
        rawsub(b"XPRD", 4.5f32.to_le_bytes().to_vec()),
        rawsub(b"INAM", 0x0F000102u32.to_le_bytes().to_vec()),
        rawsub(b"TNAM", 0x0F000103u32.to_le_bytes().to_vec()),
        rawsub(b"XOWN", 0x0F000104u32.to_le_bytes().to_vec()),
        rawsub(b"XRNK", 3i32.to_le_bytes().to_vec()),
        rawsub(b"XCNT", 7i32.to_le_bytes().to_vec()),
        rawsub(b"XRDS", 6.5f32.to_le_bytes().to_vec()),
        rawsub(b"XHLP", 100.0f32.to_le_bytes().to_vec()),
        rawsub(b"XLKR", 0x0F000105u32.to_le_bytes().to_vec()),
        rawsub(b"XAPD", vec![0x03]),
        zsub(b"XATO", "Fire the missile"),
        rawsub(b"XEMI", 0x0F000106u32.to_le_bytes().to_vec()),
        rawsub(b"XMBR", 0x0F000107u32.to_le_bytes().to_vec()),
        rawsub(b"XIBS", Vec::new()),
        rawsub(b"XSCL", 1.25f32.to_le_bytes().to_vec()),
        rawsub(b"DATA", data),
    ];

    write_synthetic_plugin(path, *b"PMIS", form_id, subs);
    form_id
}

fn open(path: &std::path::Path) -> nvre_editor::session::Session {
    nvre_editor::session::Session::open(
        path,
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec![],
    )
    .expect("open the synthetic plugin")
}

#[test]
fn the_placed_missile_reads_every_named_field() {
    let path = std::env::temp_dir().join("nvre_pmis_read.esp");
    let form_id = build_pmis(&path);
    let s = open(&path);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, form_id, *b"PMIS");

    assert_eq!(state.pmis_base, Some(0x0F000100), "the base");
    assert_eq!(
        state.pmis_encounter_zone,
        Some(0x0F000101),
        "the encounter zone"
    );
    assert_eq!(state.pmis_idle, Some(0x0F000102), "the idle");
    assert_eq!(state.pmis_topic, Some(0x0F000103), "the topic");
    assert_eq!(state.pmis_owner, Some(0x0F000104), "the owner");
    assert_eq!(state.pmis_faction_rank, 3, "the faction rank");
    assert_eq!(state.pmis_count, 7, "the count");
    assert_eq!(state.pmis_radius, 6.5, "the radius");
    assert_eq!(state.pmis_health, 100.0, "the health");
    assert_eq!(
        state.pmis_linked_reference,
        Some(0x0F000105),
        "the linked reference"
    );
    assert_eq!(state.pmis_activate_flags, 3, "the activate parents flags");
    assert_eq!(
        state.pmis_activation_prompt, "Fire the missile",
        "the activation prompt"
    );
    assert_eq!(state.pmis_emittance, Some(0x0F000106), "the emittance");
    assert_eq!(
        state.pmis_multibound_reference,
        Some(0x0F000107),
        "the multi-bound reference"
    );
    assert!(state.pmis_ignored_by_sandbox, "the ignored by sandbox flag");
    assert_eq!(state.pmis_scale, 1.25, "the scale");
    assert_eq!(state.pmis_position, [1.0, 2.0, 3.0], "the position");
    assert_eq!(state.pmis_rotation, [0.5, 0.25, 0.125], "the rotation");
    assert_eq!(state.pmis_bounding_radius, 12.5, "the bounding radius");
    assert_eq!(state.pmis_flags, 0x20, "the flags");
    assert_eq!(state.pmis_ragdoll, vec![1, 2, 3, 4], "the ragdoll data");
    assert_eq!(
        state.pmis_ragdoll_biped,
        vec![5, 6, 7, 8],
        "the ragdoll biped data"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_placed_missile_writes_every_named_field() {
    let path = std::env::temp_dir().join("nvre_pmis_write.esp");
    let form_id = build_pmis(&path);
    let mut s = open(&path);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, form_id, *b"PMIS");

    state.pmis_faction_rank = -2;
    state.pmis_count = 11;
    state.pmis_radius = 8.5;
    state.pmis_health = 55.0;
    state.pmis_linked_reference = Some(0x0F000200);
    state.pmis_activate_flags = 0x07;
    state.pmis_activation_prompt = "Launch".to_string();
    state.pmis_emittance = Some(0x0F000201);
    state.pmis_multibound_reference = Some(0x0F000202);
    state.pmis_ignored_by_sandbox = false;
    state.save_to_session(&mut s);

    let out = std::env::temp_dir().join("nvre_pmis_write_out.esp");
    s.save(&out).expect("save");
    let bytes = std::fs::read(&out).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "out.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&out);

    let mut found = None;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.form_id == form_id {
            found = Some(r.clone());
        }
    });
    let record = found.expect("the record survives the save");
    let subs = record.subrecords().expect("subrecords parse");

    let xrnk = common::payload(&subs, b"XRNK");
    assert_eq!(
        i32::from_le_bytes([xrnk[0], xrnk[1], xrnk[2], xrnk[3]]),
        -2,
        "the faction rank is written"
    );
    let xcnt = common::payload(&subs, b"XCNT");
    assert_eq!(
        i32::from_le_bytes([xcnt[0], xcnt[1], xcnt[2], xcnt[3]]),
        11,
        "the count is written"
    );
    let xrds = common::payload(&subs, b"XRDS");
    assert_eq!(
        f32::from_le_bytes([xrds[0], xrds[1], xrds[2], xrds[3]]),
        8.5,
        "the radius is written"
    );
    let xhlp = common::payload(&subs, b"XHLP");
    assert_eq!(
        f32::from_le_bytes([xhlp[0], xhlp[1], xhlp[2], xhlp[3]]),
        55.0,
        "the health is written"
    );
    let xlkr = common::payload(&subs, b"XLKR");
    assert_eq!(
        u32::from_le_bytes([xlkr[0], xlkr[1], xlkr[2], xlkr[3]]),
        0x0F000200,
        "the linked reference is written"
    );
    assert_eq!(
        common::payload(&subs, b"XAPD")[0],
        0x07,
        "the activate parents flags are written"
    );
    assert_eq!(
        String::from_utf8_lossy(&common::payload(&subs, b"XATO"))
            .trim_end_matches('\0')
            .to_string(),
        "Launch",
        "the activation prompt is written"
    );
    let xemi = common::payload(&subs, b"XEMI");
    assert_eq!(
        u32::from_le_bytes([xemi[0], xemi[1], xemi[2], xemi[3]]),
        0x0F000201,
        "the emittance is written"
    );
    let xmbr = common::payload(&subs, b"XMBR");
    assert_eq!(
        u32::from_le_bytes([xmbr[0], xmbr[1], xmbr[2], xmbr[3]]),
        0x0F000202,
        "the multi-bound reference is written"
    );
    assert!(
        common::payloads(&subs, b"XIBS").is_empty(),
        "the ignored by sandbox flag is removed"
    );

    let _ = std::fs::remove_file(&path);
}
