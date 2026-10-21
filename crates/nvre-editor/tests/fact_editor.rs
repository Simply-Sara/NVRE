//! The Faction (FACT) editor reads and writes every field of every subrecord
//! in fopdoc's FACT spec, and keeps the record intact through a save.
//!
//! The spec lists seven subrecords: `EDID`, `FULL`, `XNAM` (Relation), `DATA`,
//! `CNAM`, the rank collection (`RNAM`/`MNAM`/`FNAM`/`INAM`) and `WMI1`. What
//! is checked here is that the values are the game's own values, and that a
//! save reproduces the record byte for byte when nothing was changed.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with
//! `fnvtool esm survey` and `fnvtool esm decode`, not against a fixture
//! written from memory. The measured facts the tests lean on:
//!
//! - 682 FACT records; 648 have a four-byte `DATA` and 34 have a one-byte one.
//! - The one-byte records are exactly the 34 that carry a `CNAM`, and every
//!   `CNAM` in the master holds the float 1.0.
//! - 1,314 `XNAM` subrecords, every one of them exactly 12 bytes, all naming
//!   a `FACT` record.
//! - 111 `RNAM` rank numbers across 57 records, with 94 `MNAM` and 53 `FNAM`
//!   titles, and no `INAM` at all.

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

/// The same, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_fact_{name}.esp"));
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
    found
        .expect("the record survives the save")
        .subrecords()
        .expect("subrecords parse")
}

/// A subrecord's payload by signature.
fn payload(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<u8> {
    subs.iter()
        .find(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .unwrap_or_else(|| panic!("a {} subrecord", nvre_util::tag_str(sig)))
}

/// Every payload with a signature, in file order.
fn all_payloads(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<Vec<u8>> {
    subs.iter()
        .filter(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .collect()
}

/// The signature sequence of a record, in file order.
fn sig_order(subs: &[fnv_esm::Subrecord]) -> Vec<String> {
    subs.iter()
        .map(|s| nvre_util::tag_str(&s.sig).to_string())
        .collect()
}

/// Every FACT record in the master.
fn fact_fids(s: &Session) -> Vec<u32> {
    s.doc
        .records()
        .into_iter()
        .filter(|(_, r)| r.header.sig == *b"FACT")
        .map(|(f, _)| f)
        .collect()
}

/// SuperMutantFaction (0x0001D1EB): the richest record in the master. It has
/// 27 relations, five ranks with both a male and a female title, flags 1 =
/// 3 (Hidden From PC and Evil) and flags 2 = 0, and no `WMI1`.
const SUPERMUTANT: u32 = 0x0001_D1EB;

/// VBrotherhoodOfSteelFaction (0x00154307): nine ranks, no relations at all,
/// and flags 2 = 1 (Track Crime).
const BROTHERHOOD: u32 = 0x0015_4307;

/// NCRFactionTechMineNV (0x0016A8A0): one relation, one `WMI1`, and a
/// four-byte `DATA` with flags 1 = 1 (Hidden From PC).
const NCR_TECH_MINE: u32 = 0x0016_A8A0;

/// MetroRobotFaction (0x0003BEB9): one of the 34 records with a one-byte
/// `DATA` and a `CNAM` of 1.0.
const METRO_ROBOT: u32 = 0x0003_BEB9;

#[test]
fn every_field_of_a_real_faction_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, SUPERMUTANT, *b"FACT");

    assert_eq!(state.edid, "SuperMutantFaction");
    assert_eq!(state.full_name, "Supermutants");

    // --- DATA: flags 1 and flags 2 ---
    assert_eq!(state.fact_flags1, 3, "Flags 1: Hidden From PC and Evil");
    assert_ne!(state.fact_flags1 & 0x01, 0, "0x01 Hidden From PC");
    assert_ne!(state.fact_flags1 & 0x02, 0, "0x02 Evil");
    assert_eq!(state.fact_flags1 & 0x04, 0, "0x04 Special Combat is clear");
    assert_eq!(state.fact_flags2, 0, "Flags 2");
    assert_eq!(state.fact_data_len, 4, "DATA is four bytes");
    assert_eq!(state.fact_data_unused, [0, 0], "the two unused bytes");

    // --- XNAM: the relations, in file order ---
    assert_eq!(state.fact_relations.len(), 27, "27 relations");
    let first = &state.fact_relations[0];
    assert_eq!(
        first.faction,
        Some(0x0001_D1EB),
        "the faction relates to itself"
    );
    assert_eq!(first.modifier, 0, "Modifier");
    assert_eq!(
        first.group_combat_reaction, 2,
        "Group Combat Reaction: Ally"
    );
    // The third relation carries the modifier -100, the extreme the game uses.
    assert_eq!(state.fact_relations[2].modifier, -100, "Modifier");
    assert_eq!(state.fact_relations[2].group_combat_reaction, 2, "Ally");

    // --- The rank collection ---
    assert_eq!(state.fact_ranks.len(), 5, "five ranks");
    assert_eq!(state.fact_ranks[0].number, 0, "RNAM: rank 0");
    assert_eq!(state.fact_ranks[0].male, "Grunt", "MNAM");
    assert_eq!(state.fact_ranks[0].female, "Grunt", "FNAM");
    assert_eq!(state.fact_ranks[4].number, 4, "RNAM: rank 4");
    assert_eq!(state.fact_ranks[4].male, "Behemoth", "MNAM");
    assert_eq!(state.fact_ranks[4].female, "Behemoth", "FNAM");
    assert_eq!(
        state.fact_ranks[0].insignia, None,
        "no record in the master carries an INAM"
    );

    // --- CNAM and WMI1 ---
    assert!(!state.fact_has_cnam, "this record has no CNAM");
    assert_eq!(state.fact_reputation, None, "this record has no WMI1");
}

#[test]
fn the_faction_with_nine_ranks_and_no_relations_is_read_in_full() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VBrotherhoodOfSteelFaction: nine ranks, no relations, flags 2 = 1.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, BROTHERHOOD, *b"FACT");

    assert_eq!(state.edid, "VBrotherhoodOfSteelFaction");
    assert_eq!(state.fact_relations.len(), 0, "no relations");
    assert_eq!(state.fact_flags1, 0, "Flags 1");
    assert_eq!(state.fact_flags2, 1, "Flags 2: Track Crime");
    assert_ne!(state.fact_flags2 & 0x01, 0, "0x01 Track Crime");
    assert_eq!(state.fact_flags2 & 0x02, 0, "0x02 Allow Sell is clear");
    assert_eq!(state.fact_ranks.len(), 9, "nine ranks");
    for (i, rank) in state.fact_ranks.iter().enumerate() {
        assert_eq!(rank.number, i as i32, "RNAM is the rank number");
    }
    assert_eq!(state.fact_ranks[0].male, "Initiate", "MNAM");
    assert_eq!(state.fact_ranks[0].female, "Initiate", "FNAM");
    assert_eq!(state.fact_ranks[8].male, "Elder", "MNAM");
    assert_eq!(state.fact_ranks[8].female, "Elder", "FNAM");
}

#[test]
fn the_reputation_and_the_one_relation_of_a_small_faction_are_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // NCRFactionTechMineNV: one relation, one WMI1 pointing at RepNVNCR.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, NCR_TECH_MINE, *b"FACT");

    assert_eq!(state.edid, "NCRFactionTechMineNV");
    assert_eq!(state.fact_flags1, 1, "Flags 1: Hidden From PC");
    assert_eq!(state.fact_flags2, 0, "Flags 2");
    assert_eq!(state.fact_relations.len(), 1, "one relation");
    assert_eq!(
        state.fact_relations[0].faction,
        Some(0x000A_46E7),
        "the related faction"
    );
    assert_eq!(state.fact_relations[0].modifier, 0, "Modifier");
    assert_eq!(state.fact_relations[0].group_combat_reaction, 2, "Ally");
    assert_eq!(
        state.fact_reputation,
        Some(0x000F_43DE),
        "WMI1 points at RepNVNCR"
    );
}

#[test]
fn the_one_byte_data_records_keep_their_cnam() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // 34 of the master's FACT records have a one-byte DATA, and every one of
    // them carries a CNAM of 1.0. The old editor wrote a four-byte DATA for
    // them and dropped the CNAM entirely.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, METRO_ROBOT, *b"FACT");

    assert_eq!(state.edid, "MetroRobotFaction");
    assert_eq!(state.fact_data_len, 1, "DATA is one byte");
    assert_eq!(state.fact_flags1, 0, "Flags 1");
    assert!(state.fact_has_cnam, "the record carries a CNAM");
    assert_eq!(state.fact_cnam, 1.0, "the CNAM holds 1.0");

    // The one-byte DATA really is one byte on disk.
    let data = payload(&subs(&s, METRO_ROBOT), b"DATA");
    assert_eq!(data.len(), 1, "the record's DATA is one byte");
    assert_eq!(data[0], 0, "flags 1 is zero");
    let cnam = payload(&subs(&s, METRO_ROBOT), b"CNAM");
    assert_eq!(cnam.len(), 4, "the CNAM is a float32");
    assert_eq!(
        f32::from_le_bytes([cnam[0], cnam[1], cnam[2], cnam[3]]),
        1.0,
        "the CNAM holds 1.0"
    );
}

#[test]
fn a_faction_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // A record with everything: relations, ranks with both titles, and a
    // four-byte DATA.
    let fid = SUPERMUTANT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
    // And the relations and the ranks keep their order.
    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the subrecords come back in the order they were in"
    );
    assert_eq!(
        all_payloads(&after, b"XNAM").len(),
        27,
        "every relation comes back"
    );
}

#[test]
fn a_faction_with_a_one_byte_data_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The one-byte DATA is the case the old editor broke: it wrote four bytes
    // and dropped the CNAM.
    let fid = METRO_ROBOT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "onebyte");

    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the CNAM survives the save"
    );
    assert_eq!(
        payload(&after, b"DATA").len(),
        1,
        "the DATA is still one byte"
    );
    assert_eq!(
        payload(&after, b"CNAM"),
        payload(&before, b"CNAM"),
        "the CNAM is byte-identical"
    );
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = SUPERMUTANT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");

    // DATA: both flag words, and a bit the spec does not name.
    state.fact_flags1 = 0x02; // Evil only
    state.fact_flags2 = 0x03; // Track Crime and Allow Sell

    // XNAM: the first relation's modifier and reaction.
    state.fact_relations[0].modifier = -25;
    state.fact_relations[0].group_combat_reaction = 1; // Enemy

    // The rank collection: rename a title and add a rank.
    state.fact_ranks[0].male = "Grunt Leader".to_string();
    state.fact_ranks.push(nvre_editor::object_editor::FactRank {
        number: 5,
        male: "Overlord".to_string(),
        female: "Overlady".to_string(),
        insignia: None,
    });

    // CNAM: the record has none, so one is written.
    state.fact_has_cnam = true;
    state.fact_cnam = 2.5;

    // WMI1: a reputation the record does not have.
    state.fact_reputation = Some(0x000F_43DE);

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // DATA.
    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 4, "DATA is four bytes");
    assert_eq!(data[0], 0x02, "Flags 1: Evil only");
    assert_eq!(data[1], 0x03, "Flags 2: Track Crime and Allow Sell");
    assert_eq!(data[2], 0, "the unused byte");
    assert_eq!(data[3], 0, "the unused byte");

    // XNAM: the first relation, and the count is unchanged.
    let xnams = all_payloads(&after, b"XNAM");
    assert_eq!(xnams.len(), 27, "the relations are not duplicated");
    assert_eq!(
        u32::from_le_bytes([xnams[0][0], xnams[0][1], xnams[0][2], xnams[0][3]]),
        0x0001_D1EB,
        "the related faction is unchanged"
    );
    assert_eq!(
        i32::from_le_bytes([xnams[0][4], xnams[0][5], xnams[0][6], xnams[0][7]]),
        -25,
        "the modifier"
    );
    assert_eq!(
        u32::from_le_bytes([xnams[0][8], xnams[0][9], xnams[0][10], xnams[0][11]]),
        1,
        "the group combat reaction"
    );

    // The rank collection: the renamed title, then the added rank.
    let rnam: Vec<i32> = all_payloads(&after, b"RNAM")
        .iter()
        .map(|d| i32::from_le_bytes([d[0], d[1], d[2], d[3]]))
        .collect();
    assert_eq!(rnam, vec![0, 1, 2, 3, 4, 5], "the ranks, with the new one");
    let mnam: Vec<String> = all_payloads(&after, b"MNAM")
        .iter()
        .map(|d| {
            String::from_utf8_lossy(d)
                .trim_end_matches('\0')
                .to_string()
        })
        .collect();
    assert_eq!(mnam[0], "Grunt Leader", "the renamed male title");
    assert_eq!(mnam[5], "Overlord", "the added rank's male title");
    let fnam: Vec<String> = all_payloads(&after, b"FNAM")
        .iter()
        .map(|d| {
            String::from_utf8_lossy(d)
                .trim_end_matches('\0')
                .to_string()
        })
        .collect();
    assert_eq!(fnam.len(), 6, "the added rank has a female title too");
    assert_eq!(fnam[5], "Overlady", "the added rank's female title");

    // The rank titles stay attached to their own rank: the sequence edit
    // writes RNAM, MNAM, FNAM in order rather than clustering the signatures.
    let order = sig_order(&after);
    let rnam_idx: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, s)| *s == "RNAM")
        .map(|(i, _)| i)
        .collect();
    let mnam_idx: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, s)| *s == "MNAM")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(rnam_idx.len(), 6, "six ranks");
    for (i, r) in rnam_idx.iter().enumerate() {
        assert!(
            mnam_idx[i] > *r,
            "the male title of rank {i} follows its RNAM"
        );
    }

    // CNAM and WMI1.
    let cnam = payload(&after, b"CNAM");
    assert_eq!(
        f32::from_le_bytes([cnam[0], cnam[1], cnam[2], cnam[3]]),
        2.5,
        "the CNAM"
    );
    assert_eq!(
        u32::from_le_bytes([
            payload(&after, b"WMI1")[0],
            payload(&after, b"WMI1")[1],
            payload(&after, b"WMI1")[2],
            payload(&after, b"WMI1")[3],
        ]),
        0x000F_43DE,
        "the reputation"
    );
}

#[test]
fn a_relation_and_a_rank_can_be_added_and_removed() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = NCR_TECH_MINE;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    assert_eq!(state.fact_relations.len(), 1, "the record starts with one");
    assert_eq!(state.fact_ranks.len(), 0, "and no ranks");

    // Add a relation and a rank.
    state
        .fact_relations
        .push(nvre_editor::object_editor::FactRelation {
            faction: Some(0x0001_D1EB),
            modifier: 100,
            group_combat_reaction: 3, // Friend
        });
    state.fact_ranks.push(nvre_editor::object_editor::FactRank {
        number: 0,
        male: "Boss".to_string(),
        female: "Boss".to_string(),
        insignia: None,
    });
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "added");
    assert_eq!(
        all_payloads(&after, b"XNAM").len(),
        2,
        "the relation is added"
    );
    assert_eq!(all_payloads(&after, b"RNAM").len(), 1, "the rank is added");
    assert_eq!(
        all_payloads(&after, b"MNAM").len(),
        1,
        "the rank's male title is added"
    );

    // Now remove both and save again.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    assert_eq!(
        state.fact_relations.len(),
        2,
        "the queued relation is there"
    );
    assert_eq!(state.fact_ranks.len(), 1, "the queued rank is there");
    state.fact_relations.clear();
    state.fact_ranks.clear();
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "removed");
    assert_eq!(
        all_payloads(&after, b"XNAM").len(),
        0,
        "the relations are removed"
    );
    assert_eq!(
        all_payloads(&after, b"RNAM").len(),
        0,
        "the ranks are removed"
    );
    assert_eq!(all_payloads(&after, b"MNAM").len(), 0, "and their titles");
}

#[test]
fn the_allow_sell_flag_survives_an_edit() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The old editor read and wrote only bit 0 of flags 2, so a faction that
    // could be sold from lost the bit on every save. Find a record that
    // carries bit 1 and check it survives an edit of bit 0.
    let fids = fact_fids(&s);
    let mut target = None;
    for fid in fids {
        let data = payload(&subs(&s, fid), b"DATA");
        if data.len() > 1 && data[1] & 0x02 != 0 {
            target = Some((fid, data[1]));
            break;
        }
    }
    let Some((fid, flags2)) = target else {
        eprintln!("skipped: no FACT record sets the Allow Sell flag");
        return;
    };
    eprintln!("{fid:08X} carries flags 2 {flags2:#04x}");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    state.fact_flags2 &= !0x01; // clear Track Crime
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "allowsell"), b"DATA");
    assert_eq!(after[1], flags2 & !0x01, "only the edited bit moved");
    assert_ne!(after[1] & 0x02, 0, "the Allow Sell bit is preserved");
}

#[test]
fn a_faction_with_no_relations_and_no_ranks_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The other end of the range: a faction with nothing but an EDID and a
    // DATA. The editor must not invent a relation or a rank for it.
    let fid = 0x0017_622E; // CassFaction
    let before = subs(&s, fid);
    assert!(
        !before
            .iter()
            .any(|x| x.sig == *b"XNAM" || x.sig == *b"RNAM"),
        "this record really has neither"
    );
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FACT");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "bare");
    assert_eq!(sig_order(&after), sig_order(&before), "nothing is added");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
}
