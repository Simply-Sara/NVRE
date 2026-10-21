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
        .unwrap_or_else(|| panic!("a {} subrecord", nvre_util::tag_str(sig)))
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
        .map(|s| nvre_util::tag_str(&s.sig).to_string())
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

/// A subrecord with a cstring payload.
pub fn zsub(sig: &[u8; 4], text: &str) -> fnv_esm::Subrecord {
    fnv_esm::Subrecord {
        sig: *sig,
        data: nvre_edit::document::encode_zstring(text),
        extended: false,
    }
}

/// A subrecord with a raw payload.
pub fn rawsub(sig: &[u8; 4], data: Vec<u8>) -> fnv_esm::Subrecord {
    fnv_esm::Subrecord {
        sig: *sig,
        data,
        extended: false,
    }
}

/// Build a one-record plugin file around a record's subrecords, so a type
/// with no shipped records can still be exercised.
///
/// The file is a minimal ESM: a `TES4` header record, a top-level group and
/// the record itself. The editor opens it like any other plugin.
pub fn write_synthetic_plugin(
    path: &std::path::Path,
    sig: [u8; 4],
    form_id: u32,
    subs: Vec<fnv_esm::Subrecord>,
) {
    use bytes::BufMut;

    let mut out: Vec<u8> = Vec::new();

    // The TES4 header record. The editor reads the record count, the next
    // object id and the masters, so the header carries a plausible set.
    let mut hedr = Vec::new();
    hedr.put_u32_le(1.34f32.to_bits()); // the format version
    hedr.put_i32_le(2); // the record and group count, less the header
    hedr.put_u32_le(form_id + 1); // the next object id
    let mut tes4 = Vec::new();
    tes4.put_slice(b"HEDR");
    tes4.put_u16_le(hedr.len() as u16);
    tes4.put_slice(&hedr);
    write_record(&mut out, *b"TES4", 0x00100000, tes4);

    // The record itself.
    let mut body = Vec::new();
    for sub in &subs {
        body.put_slice(&sub.sig);
        body.put_u16_le(sub.data.len() as u16);
        body.put_slice(&sub.data);
    }
    let body_len = body.len();
    let group_body = body.clone();
    write_record(&mut out, sig, form_id, body);

    // The top-level group holding the record. Its header is the
    // signature, the size, the label, the group type, the stamp, two
    // unknown words and the version.
    let mut group = Vec::new();
    group.put_slice(b"GRUP");
    group.put_u32_le((body_len + 24 + 24) as u32);
    group.put_u32_le(form_id); // the label
    group.put_i32_le(0); // the group type: top level
    group.put_u16_le(0); // the stamp
    group.put_u16_le(0); // unknown
    group.put_u16_le(0); // the version
    group.put_u16_le(0); // unknown
    group.put_slice(&group_body);

    std::fs::write(path, out).expect("write the synthetic plugin");
}

/// Write one record: its signature, the payload size, the flags, the form
/// id and eight more bytes of version-control bookkeeping, then the payload.
fn write_record(out: &mut Vec<u8>, sig: [u8; 4], form_id: u32, body: Vec<u8>) {
    use bytes::BufMut;
    out.put_slice(&sig);
    out.put_u32_le(body.len() as u32);
    out.put_u32_le(0);
    out.put_u32_le(form_id);
    out.put_u32_le(0); // the version-control info
    out.put_u16_le(44); // the form version
    out.put_u16_le(0); // the version-control info
    out.put_slice(&body);
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
        nvre_util::tag_str(sig)
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

    let path = std::env::temp_dir().join(format!("nvre_all_{}.esp", nvre_util::tag_str(sig)));
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
                nvre_util::tag_str(bs)
            );
        }
    }
}
