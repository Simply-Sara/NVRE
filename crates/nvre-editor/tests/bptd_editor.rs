//! The Body Part Data (BPTD) editor reads and writes every field of every
//! subrecord in fopdoc's BPTD spec, and keeps the record's part collection
//! intact through a save.
//!
//! A BPTD is a list of parts, each an ordered run of eight subrecords
//! `(BPTN?) BPNN BPNT BPNI BPND NAM1 NAM4 NAM5`, so the editor has to walk the
//! subrecords in order to load them and write them back as one sequence. What
//! is checked here is that the values are the game's own values, and that a save
//! reproduces the record byte-for-byte when nothing was changed.

use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// The record's subrecords, in file order.
fn subs(s: &Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The record's subrecords from the saved file, in file order.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    saved(s, fid, name).subrecords().expect("subrecords parse")
}

/// Every part's `BPNN` payload, in order — the identity of each part.
fn nodes(s: &Session, fid: u32) -> Vec<Vec<u8>> {
    subs(s, fid)
        .into_iter()
        .filter(|sub| sub.sig == *b"BPNN")
        .map(|sub| sub.data)
        .collect()
}

/// The same, but read from the file the editor actually wrote.
fn saved_nodes(s: &Session, fid: u32, name: &str) -> Vec<Vec<u8>> {
    saved_subs(s, fid, name)
        .into_iter()
        .filter(|sub| sub.sig == *b"BPNN")
        .map(|sub| sub.data)
        .collect()
}

/// A scratch file for a saved plugin, unique per test.
fn out(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("nvre_bptd_{name}.esp"))
}

/// Write the session out and read the record back through a fresh parse.
fn saved(s: &Session, fid: u32, name: &str) -> fnv_esm::Record {
    let path = out(name);
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.form_id == fid {
            found = Some(r.clone());
        }
    });
    let _ = std::fs::remove_file(&path);
    found.expect("the record survives the save")
}

#[test]
fn every_body_part_of_a_real_record_is_loaded_in_order() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // DefaultBodyPartData: the humanoid template every NPC points at, and the
    // record that carries all six part types plus a RAGA, so it exercises the
    // named-part path end to end.
    let fid = 0x0000_001D;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");

    assert_eq!(state.edid, "DefaultBodyPartData");
    assert_eq!(
        state.model, "Characters\\_Male\\skeleton.NIF",
        "the record's Model Data collection"
    );
    assert_eq!(state.bptd_ragdoll, Some(0x0006_58D6), "RAGA Ragdoll (RGDL)");

    // Six parts, in the order the file stores them, and the name of each.
    assert_eq!(
        state.bptd_parts.len(),
        6,
        "the record defines six body parts"
    );
    assert_eq!(
        state
            .bptd_parts
            .iter()
            .map(|p| p.label())
            .collect::<Vec<_>>(),
        vec![
            "Head",
            "Torso",
            "Left Arm",
            "Left Leg",
            "Right Leg",
            "Right Arm"
        ],
    );
    assert_eq!(
        state.bptd_parts[0].node, "Bip01 Neck1",
        "BPNN Part Node of the first part"
    );
    assert_eq!(
        state.bptd_parts[0].vats_target, "Bip01 Head",
        "BPNT VATS Target"
    );
    assert_eq!(
        state.bptd_parts[0].ik_start_node, "Bip01 Head",
        "BPNI IK Start Node"
    );
    assert_eq!(
        state.bptd_parts[0].limb_model, "Gore\\HeadBits01.NIF",
        "NAM1 Limb Replacement Model"
    );
    assert_eq!(
        state.bptd_parts[0].gore_target_bone, "Bip01 Neck1",
        "NAM4 Gore Effects - Target Bone"
    );
    assert_eq!(
        state.bptd_parts[0].texture_hashes.len(),
        72,
        "NAM5 Texture File Hashes: three 24-byte hashes"
    );
}

#[test]
fn the_bpnd_struct_reads_every_field_of_a_real_part() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The head of DefaultBodyPartData is the part with the most non-default
    // values in the game's masters, so it is the one worth asserting against.
    let fid = 0x0000_001D;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");
    let d = &state.bptd_parts[0].data;

    assert_eq!(d.damage_mult, 2.0, "Damage Multiplier");
    assert_eq!(d.flags, 59, "Flags: 0x01|0x02|0x08|0x10|0x20");
    // 0x01 Severable is the first flag bit.
    assert_ne!(d.flags & 0x01, 0, "0x01 Severable is set");
    // 0x20 IK Data - Headtracking is the highest bit this part sets.
    assert_ne!(d.flags & 0x20, 0, "0x20 IK Data - Headtracking is set");
    // 0x40 To Hit Chance - Absolute is the spec's last flag bit and is clear
    // here, so the editor must not invent it.
    assert_eq!(d.flags & 0x40, 0, "0x40 To Hit Chance - Absolute is clear");
    assert_eq!(d.part_type, 1, "Part Type 1 = Head 1");
    assert_eq!(d.health_percent, 20, "Health Percent");
    assert_eq!(d.actor_value, 25, "Actor Value 25 = Perception Condition");
    assert_eq!(d.to_hit_chance, 30, "To Hit Chance");
    assert_eq!(
        d.explodable_explosion_chance_percent, 30,
        "Explodable - Explosion Chance %"
    );
    assert_eq!(
        d.explodable_debris_count, 3,
        "Explodable - Debris Count (u16)"
    );
    assert_eq!(
        d.explodable_debris,
        Some(0x0001_B2BF),
        "Explodable - Debris (DEBR)"
    );
    assert_eq!(
        d.explodable_explosion,
        Some(0x0001_540D),
        "Explodable - Explosion (EXPL)"
    );
    assert_eq!(d.tracking_max_angle, 60.0, "Tracking Max Angle");
    assert_eq!(d.explodable_debris_scale, 1.0, "Explodable - Debris Scale");
    assert_eq!(
        d.severable_debris_count, 0,
        "Severable - Debris Count (i32)"
    );
    // A zero FormID is the game's "none", and the editor reports it as such
    // rather than as a FormID pointing at record 0.
    assert_eq!(d.severable_debris, None, "Severable - Debris, unset");
    assert_eq!(d.severable_explosion, None, "Severable - Explosion, unset");
    assert_eq!(d.severable_debris_scale, 1.0, "Severable - Debris Scale");
    assert_eq!(d.gore_x, 0.0, "Gore Effects - Translate X");
    assert_eq!(d.gore_z, 0.0, "Gore Effects - Translate Z");
    // The head's gore rotation is the one field in this record that is not zero.
    assert_eq!(d.gore_ry, 4.625_122_5, "Gore Effects - Y Rotation");
    assert_eq!(d.gore_rz, 0.0, "Gore Effects - Z Rotation");
    assert_eq!(
        d.severable_impact_dataset,
        Some(0x0002_ED47),
        "Severable - Impact Dataset (IPDS)"
    );
    assert_eq!(
        d.explodable_impact_dataset,
        Some(0x0002_ED47),
        "Explodable - Impact Dataset (IPDS)"
    );
    assert_eq!(d.severable_decal_count, 4, "Severable - Decal Count");
    assert_eq!(d.explodable_decal_count, 6, "Explodable - Decal Count");
    assert_eq!(d.limb_replacement_scale, 1.0, "Limb Replacement Scale");
}

#[test]
fn an_unnamed_part_is_loaded_as_one_without_a_name() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // NVVoidBodyPartData is the one record in the masters whose single part has
    // no BPTN at all: it goes straight from MODL to BPNN. The spec allows both
    // forms, so the editor has to be able to read the unnamed one.
    let fid = 0x0017_1553;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");

    assert_eq!(state.edid, "NVVoidBodyPartData");
    assert_eq!(state.bptd_parts.len(), 1, "one body part");
    let part = &state.bptd_parts[0];
    assert_eq!(part.name, None, "no BPTN on this part");
    // The node names the part in the list instead, so it is still addressable.
    assert_eq!(
        part.label(),
        "Bip01 Root",
        "an unnamed part is named by its node"
    );
    assert_eq!(part.node, "Bip01 Root");
    assert_eq!(part.vats_target, "Bip01 Root");
    assert_eq!(part.data.part_type, 0, "Part Type 0 = Torso");
    assert_eq!(part.data.health_percent, 5, "Health Percent");
    assert_eq!(part.data.flags, 0, "no flags set on this part");
    assert_eq!(part.data.actor_value, -1, "Actor Value -1 = None");
    assert_eq!(state.bptd_ragdoll, None, "no RAGA on this record");
    // An empty NAM5 is a real state, not a missing one: it must round-trip as
    // zero bytes rather than being dropped.
    assert!(part.texture_hashes.is_empty(), "no texture hashes");
}

#[test]
fn a_part_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // BrahminBodyPartData: seven parts, a RAGA, and every NAM5 either empty or
    // three hashes — the mix that catches a reorder.
    let fid = 0x0001_5833;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");
    assert_eq!(state.edid, "BrahminBodyPartData");
    assert_eq!(state.bptd_parts.len(), 7, "seven body parts");
    assert_eq!(state.bptd_ragdoll, Some(0x0006_58E3), "RAGA Ragdoll");

    state.save_to_session(&mut s);
    let after = subs(&s, fid);

    assert_eq!(
        after.len(),
        before.len(),
        "the same number of subrecords comes back"
    );
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
}

#[test]
fn an_edited_part_and_a_new_part_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0017_1553;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");
    let original_nodes = nodes(&s, fid);

    // Change fields on the part that is already there.
    state.bptd_parts[0].data.damage_mult = 3.5;
    state.bptd_parts[0].data.health_percent = 90;
    state.bptd_parts[0].data.flags = 0x40;
    state.bptd_parts[0].vats_target = "Bip01 Edited".to_string();
    state.bptd_ragdoll = Some(0x0006_58E3);

    // And add a part, which is how a modder gives a creature a damageable head.
    state.bptd_parts.push(nvre_editor::object_editor::BptdPart {
        name: Some("Added Head".to_string()),
        node: "Bip01 AddedHead".to_string(),
        vats_target: "Bip01 AddedHead".to_string(),
        ik_start_node: "Bip01 AddedHead".to_string(),
        data: nvre_editor::object_editor::BptdNodeData {
            damage_mult: 1.5,
            flags: 0x01,
            part_type: 1,
            health_percent: 50,
            actor_value: 25,
            to_hit_chance: 25,
            explodable_explosion_chance_percent: 0,
            explodable_debris_count: 0,
            explodable_debris: None,
            explodable_explosion: None,
            tracking_max_angle: 0.0,
            explodable_debris_scale: 1.0,
            severable_debris_count: 0,
            severable_debris: None,
            severable_explosion: None,
            severable_debris_scale: 1.0,
            gore_x: 0.0,
            gore_y: 0.0,
            gore_z: 0.0,
            gore_rx: 0.0,
            gore_ry: 0.0,
            gore_rz: 0.0,
            severable_impact_dataset: None,
            explodable_impact_dataset: None,
            severable_decal_count: 0,
            explodable_decal_count: 0,
            limb_replacement_scale: 1.0,
        },
        limb_model: "Gore\\HeadBits01.NIF".to_string(),
        gore_target_bone: "Bip01 Neck1".to_string(),
        texture_hashes: Vec::new(),
    });

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // The original part is still the first part, and the new one follows it.
    // Read from the saved file: a queued edit is not applied to the document
    // until the save, so the document still reads the record's original bytes.
    assert_eq!(
        saved_nodes(&s, fid, "edited"),
        vec![
            original_nodes[0].clone(),
            // A zstring is NUL-terminated, and the saved payload carries it.
            b"Bip01 AddedHead\0".to_vec(),
        ],
        "the original part keeps its place and the new part follows it"
    );

    // The part collection is written as parts, not as one run per signature:
    // each part's eight subrecords are adjacent, in the spec's order.
    let sigs: Vec<[u8; 4]> = after.iter().map(|s| s.sig).collect();
    assert_eq!(
        sigs,
        vec![
            *b"EDID", *b"MODL", *b"BPNN", *b"BPNT", *b"BPNI", *b"BPND", *b"NAM1", *b"NAM4",
            *b"NAM5", *b"BPTN", *b"BPNN", *b"BPNT", *b"BPNI", *b"BPND", *b"NAM1", *b"NAM4",
            *b"NAM5", *b"RAGA",
        ],
        "the unnamed part first (no BPTN), then the named one, then RAGA"
    );

    // The edited fields are the ones that come back.
    let edited = after
        .iter()
        .find(|s| s.sig == *b"BPNN" && s.data == b"Bip01 Root\0".to_vec())
        .expect("the original part");
    let idx = after
        .iter()
        .position(|s| s.sig == *b"BPND")
        .expect("a BPND");
    assert_eq!(idx, after.iter().position(|s| *s == *edited).unwrap() + 3);
    let bpnd = &after[idx].data;
    assert_eq!(
        f32::from_le_bytes([bpnd[0], bpnd[1], bpnd[2], bpnd[3]]),
        3.5,
        "Damage Multiplier"
    );
    assert_eq!(bpnd[4], 0x40, "Flags");
    assert_eq!(bpnd[5], 0, "Part Type");
    assert_eq!(bpnd[6], 90, "Health Percent");
    let raga = after
        .iter()
        .find(|s| s.sig == *b"RAGA")
        .expect("RAGA is written last");
    assert_eq!(
        u32::from_le_bytes([raga.data[0], raga.data[1], raga.data[2], raga.data[3]]),
        0x0006_58E3,
        "RAGA Ragdoll"
    );
}

#[test]
fn a_part_removed_from_the_collection_is_gone_from_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0017_1553;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"BPTD");
    state.bptd_parts.clear();
    state.bptd_ragdoll = None;
    state.save_to_session(&mut s);

    let after: Vec<[u8; 4]> = saved_subs(&s, fid, "cleared")
        .into_iter()
        .map(|s| s.sig)
        .collect();
    assert_eq!(
        after,
        vec![*b"EDID", *b"MODL"],
        "only the record's own header subrecords remain"
    );
}
