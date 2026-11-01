//! Does duplicating an object round-trip through a save?
//!
//! The Objects window's Duplicate stages a copy of a base object under a new
//! Editor ID; the save has to give it a FormID in the new plugin's own range,
//! put it in that signature's `TOP` group, and remap the subrecord FormIDs it
//! inherited into the new plugin's numbering. This probe drives
//! `Session::duplicate_object` and reads the saved file back.
//!
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example duplicate_probe

use nvre_editor::session::Session;

/// Every subrecord of the record with `form_id`, as `(sig, data)`, with the
/// record's own signature.
fn subrecords_of(nodes: &[nvre_esm::raw::Node], form_id: u32) -> (Vec<(String, Vec<u8>)>, [u8; 4]) {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut sig = [0u8; 4];
    nvre_esm::raw::visit_nodes(nodes, &mut |r, _| {
        if r.header.form_id != form_id {
            return;
        }
        sig = r.header.sig;
        if let Ok(subs) = r.subrecords() {
            out = subs.iter().map(|x| (x.sig_str(), x.data.clone())).collect();
        }
    });
    (out, sig)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let master = std::path::Path::new(&data).join("FalloutNV.esm");
    let mut s = Session::open(&master, &schema_dir, vec!["FalloutNV.esm".to_string()])?;

    // A STAT that is certainly in the master: the first Static in the palette.
    let stat = s
        .object_groups("")
        .iter()
        .find(|g| g.sig == *b"STAT")
        .and_then(|g| g.objects.first())
        .cloned()
        .expect("a STAT in the load order");
    println!(
        "duplicating STAT {:08X} (edid {:?}, name {:?})",
        stat.fid, stat.edid, stat.name
    );

    let new = s.duplicate_object(stat.fid, *b"STAT", "NvreDuplicateProbe")?;
    println!("staged as {new:08X}");
    println!("staged top-level records: {}", s.doc.inserted_top().len());

    let out = std::env::temp_dir().join("NvreDuplicateProbe.esp");
    let report = s.save(&out)?;
    println!(
        "saved: {} records, {} inserted, {} bytes",
        report.records, report.inserted, report.bytes
    );
    if !report.unremapped.is_empty() {
        println!("unremapped: {:?}", report.unremapped);
    }

    // Read the saved file back: the copy must be there, in a TOP: STAT group,
    // with its own FormID and its own Editor ID.
    let saved = nvre_esm::Plugin::open(&out)?;
    let orig_plugin = nvre_esm::Plugin::open(&master)?;
    let mut found: Option<u32> = None;
    let mut ids: Vec<u32> = Vec::new();
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, group| {
        if r.header.sig == *b"TES4" {
            return;
        }
        ids.push(r.header.form_id);
        let in_stat_group = group
            .iter()
            .any(|g| g.group_type == nvre_esm::raw::group_type::TOP && g.label == *b"STAT");
        if !in_stat_group {
            return;
        }
        let edid = r
            .subrecords()
            .ok()
            .and_then(|subs| {
                subs.iter()
                    .find(|x| &x.sig == b"EDID")
                    .map(|x| nvre_util::cp1252::decode_z(&x.data))
            })
            .unwrap_or_default();
        if edid == "NvreDuplicateProbe" {
            found = Some(r.header.form_id);
        }
    });
    let Some(fid) = found else {
        println!("FAILED: the copy is not in the saved file");
        std::fs::remove_file(&out).ok();
        return Ok(());
    };
    println!("found the copy: {fid:08X} EDID NvreDuplicateProbe");
    let own = saved.masters().len() as u32;
    println!(
        "top byte {} == the plugin's own index {own}: {}",
        fid >> 24,
        (fid >> 24) == own
    );
    println!(
        "no duplicate FormID in the file: {}",
        ids.len() == ids.iter().collect::<std::collections::HashSet<_>>().len()
    );

    // The copy must be the original record with its EDID swapped and its
    // FormID-bearing words remapped — nothing else changed. Compare
    // subrecord by subrecord, skipping the EDID on both sides.
    let (copy_subs, copy_sig) = subrecords_of(&saved.nodes, fid);
    let (orig_subs, orig_sig) = subrecords_of(&orig_plugin.nodes, stat.fid);
    println!(
        "signatures: original {} copy {}",
        nvre_util::tag_str(&orig_sig),
        nvre_util::tag_str(&copy_sig)
    );
    let a: Vec<_> = orig_subs.iter().filter(|(s, _)| s != "EDID").collect();
    let b: Vec<_> = copy_subs.iter().filter(|(s, _)| s != "EDID").collect();
    println!("subrecords kept: {} of {}", b.len(), a.len());
    let mut all_match = a.len() == b.len();
    for ((a_sig, a_data), (b_sig, b_data)) in a.iter().zip(b.iter()) {
        if a_sig != b_sig {
            all_match = false;
            println!("  MISMATCH sig {a_sig} vs {b_sig}");
            continue;
        }
        if a_data == b_data {
            println!("  {a_sig}: byte-identical ({} bytes)", a_data.len());
        } else if a_data.len() == b_data.len() {
            // A same-width difference is the FormID remap. The words are not
            // at fixed offsets — MODT and MODS carry variable-length strings —
            // so the check that holds everywhere is the one below: the save's
            // own remap, run on the original's subrecord, must produce
            // exactly these bytes.
            println!(
                "  {a_sig}: {} of {} bytes changed (checked against the save's remap below)",
                a_data
                    .iter()
                    .zip(b_data.iter())
                    .filter(|(x, y)| x != y)
                    .count(),
                a_data.len()
            );
        } else {
            all_match = false;
            println!(
                "  MISMATCH {a_sig}: {} bytes vs {}",
                a_data.len(),
                b_data.len()
            );
        }
    }
    println!("the copy is the original with its EDID swapped: {all_match}");

    // And each subrecord's payload is exactly what the save's own remap
    // produces from the original's: the copy must not have been re-encoded
    // from a decoded form, only remapped.
    let schema = nvre_esm::Schema::load_dir(&schema_dir)?;
    let map: std::collections::HashMap<u32, u32> = [(0u32, own)].into_iter().collect();
    for (sig, data) in a.iter() {
        let sub = nvre_esm::raw::Subrecord {
            sig: sig4(sig),
            data: data.clone(),
            extended: false,
        };
        let remapped = nvre_edit::remap_sub(&schema, &orig_sig, &sub, &|local| {
            let idx = local >> 24;
            let low = local & 0x00FF_FFFF;
            map.get(&idx).map(|new_idx| (new_idx << 24) | low)
        });
        let expected = b
            .iter()
            .find(|(s, _)| s == sig)
            .map(|(_, d)| d)
            .cloned()
            .unwrap_or_default();
        let ok = remapped.as_ref() == Some(&expected);
        all_match &= ok;
        println!("  {sig}: matches the save's remap: {ok}");
    }
    println!("the duplicate round-trips exactly: {all_match}");
    std::fs::remove_file(&out).ok();
    Ok(())
}

/// A 4-byte signature from text.
fn sig4(s: &str) -> [u8; 4] {
    let b = s.as_bytes();
    [b[0], b[1], b[2], b[3]]
}
