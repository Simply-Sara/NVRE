//! Shared helpers for the object editor's per-record-type tests.
//!
//! Every type's test loads a real record out of the shipped masters, asserts
//! the values the game actually wrote, and proves that a save with no edit
//! reproduces the record byte for byte.
//!
//! This module is compiled into each test binary, and each binary uses a
//! different subset of the helpers, so the unused ones are expected.
#![allow(dead_code)]

//! Shared helpers for the object editor's per-record-type tests.
//!
//! Every type's test loads a real record out of the shipped masters, asserts
//! the values the game actually wrote, and proves that a save with no edit
//! reproduces the record byte for byte.

/// The game's Data folder, when the environment names one.
pub fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

/// A session over `FalloutNV.esm`.
pub fn open_master() -> Option<nvre_editor::session::Session> {
    let data = data_dir()?;
    nvre_editor::session::Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// The record's subrecords, in file order.
pub fn subs(s: &nvre_editor::session::Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The same, read from the file the editor actually wrote.
pub fn saved_subs(
    s: &nvre_editor::session::Session,
    fid: u32,
    name: &str,
) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_{name}.esp"));
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
pub fn payload(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<u8> {
    subs.iter()
        .find(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .unwrap_or_else(|| panic!("a {} subrecord", fnv_util::tag_str(sig)))
}

/// Every payload with this signature, in file order.
pub fn payloads(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<Vec<u8>> {
    subs.iter()
        .filter(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .collect()
}

/// The signature sequence of a record, in file order.
pub fn sig_order(subs: &[fnv_esm::Subrecord]) -> Vec<String> {
    subs.iter()
        .map(|s| fnv_util::tag_str(&s.sig).to_string())
        .collect()
}

/// Every record of a type in the master.
pub fn fids_of(s: &nvre_editor::session::Session, sig: &[u8; 4]) -> Vec<u32> {
    s.doc
        .records()
        .into_iter()
        .filter(|(_, r)| &r.header.sig == sig)
        .map(|(f, _)| f)
        .collect()
}

/// Load every record of a type and save it back with no edit, then assert that
/// every one comes back byte for byte.
///
/// This is the strongest statement the editor can make about a type: that a
/// save the modder did not ask for changes nothing.
///
/// The edits are batched into one save. Each `Session::save()` rewrites the
/// whole master — 245 MB for `FalloutNV.esm` — so saving once per record
/// would turn a 37-record probe into gigabytes of writes.
pub fn assert_all_save_back_byte_identical(
    s: &mut nvre_editor::session::Session,
    sig: &[u8; 4],
    expected_count: usize,
) {
    let fids = fids_of(s, sig);
    assert_eq!(
        fids.len(),
        expected_count,
        "the master has {expected_count} {} records",
        fnv_util::tag_str(sig)
    );

    let mut before = std::collections::BTreeMap::new();
    for fid in &fids {
        let subs = subs(s, *fid);
        before.insert(
            *fid,
            subs.iter()
                .map(|x| (x.sig, x.data.clone()))
                .collect::<Vec<_>>(),
        );
    }

    for fid in &fids {
        let mut state = nvre_editor::object_editor::ObjectEditorState::default();
        state.load_from_session(s, *fid, *sig);
        state.save_to_session(s);
    }

    let path = std::env::temp_dir().join(format!("nvre_all_{}.esp", fnv_util::tag_str(sig)));
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut after = std::collections::BTreeMap::new();
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if &r.header.sig == sig {
            after.insert(
                r.header.form_id,
                r.subrecords()
                    .expect("subs")
                    .iter()
                    .map(|x| (x.sig, x.data.clone()))
                    .collect::<Vec<_>>(),
            );
        }
    });

    for fid in &fids {
        let b = before.get(fid).expect("before");
        let a = after.get(fid).expect("after");
        assert_eq!(
            a.len(),
            b.len(),
            "{fid:08X}: the same number of subrecords comes back"
        );
        for (i, (bs, bd)) in b.iter().enumerate() {
            let (asig, ad) = &a[i];
            assert_eq!(
                (asig, ad),
                (bs, bd),
                "{fid:08X}: subrecord {i} ({}) is byte-identical",
                fnv_util::tag_str(bs)
            );
        }
    }
}
