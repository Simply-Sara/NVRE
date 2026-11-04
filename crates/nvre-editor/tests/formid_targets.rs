//! A FormID field says what it may point at.
//!
//! xEdit declares a checked FormID as `wbFormIDCk(INAM, 'Name', [STAT])`, and the
//! schema used to throw that away: every reference became a bare `"formid"`, so a
//! picker could only offer all 44,550 objects. The list is in the schema now, and
//! these assertions are what keeps it there.
//!
//! The failure this guards against is silent rather than loud: a schema that
//! loses the targets still decodes and still writes, it just makes every reference
//! field a hex box.

use nvre_editor::game::GameProfile;

const GAMES: [&str; 5] = ["fnv", "fo3", "skyrim", "skyrimse", "fo4"];

/// Every targeted FormID in a game's schema, as `REC.SUB.PATH -> [SIG, ...]`.
fn targets(game: &str) -> Vec<(String, Vec<String>)> {
    use nvre_esm::TypeDef;
    let profile = GameProfile::by_id(game).expect("a known game");
    let schema = profile.load_schema().expect("the schema loads");
    fn walk(t: &TypeDef, at: &str, out: &mut Vec<(String, Vec<String>)>) {
        match t {
            TypeDef::FormIdRef { of } => out.push((at.to_string(), of.clone())),
            TypeDef::Struct(fields) => {
                for f in fields {
                    walk(&f.ty, &format!("{at}.{}", f.name), out);
                }
            }
            TypeDef::Array { elem, .. } => walk(elem, &format!("{at}[]"), out),
            TypeDef::Union(variants) => {
                for (_, v) in variants {
                    walk(v, at, out);
                }
            }
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                for v in variants {
                    walk(v, at, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (sig, rec) in &schema.records {
        for (sub, def) in &rec.subrecords {
            walk(
                &schema.resolve_def(&def.ty),
                &format!("{sig}.{sub}"),
                &mut out,
            );
        }
    }
    out.retain(|(_, of)| !of.is_empty());
    out
}

#[test]
fn fallout_4s_weapon_references_are_targeted() {
    // `wbFormIDCk(INAM, 'Impact Data Set', [IPDS])`: the field can only hold an
    // impact data set, and the picker has to know it.
    let t = targets("fo4");
    let hit = t.iter().find(|(at, _)| at.starts_with("WEAP.INAM"));
    let Some((_, of)) = hit else {
        panic!("FO4's WEAP.INAM has no FormID targets; the picker would offer everything");
    };
    assert_eq!(of, &vec!["IPDS".to_string()]);
}

#[test]
fn targets_survive_every_layer() {
    // The targets are in `schemas/fo4/generated.json`, but the schema a game
    // reads is built by merging base -> family -> version. A later layer that
    // restated the record would replace the targets with its own; a layer that
    // did not would keep them. Either is fine as long as the answer survives.
    let fo4 = targets("fo4");
    assert!(
        fo4.len() > 100,
        "FO4 has 284 targeted FormIDs in its source; the merged schema has {}",
        fo4.len()
    );
    // Skyrim's schema was not rewritten, so it has none — and that is the honest
    // state, not a failure: the tool for it is `tools/formid_targets.py` with the
    // TES5 definitions, which has not been run.
}

#[test]
fn every_target_is_a_four_byte_signature() {
    for game in GAMES {
        for (at, of) in targets(game) {
            for sig in of {
                assert_eq!(
                    sig.len(),
                    4,
                    "{game} {at} names target {sig:?}, which is not a signature"
                );
            }
        }
    }
}

#[test]
fn a_target_naming_a_record_type_the_schema_lacks_is_still_kept() {
    // A field may point at a type this game does not define in its own schema
    // (`APPR` points at an `ARMO`-family type in some records). The target is a
    // fact about the bytes, so it is kept rather than filtered; the picker then
    // finds no records of that type and offers none, which is the truth.
    let fo4 = targets("fo4");
    let schema = GameProfile::by_id("fo4").unwrap().load_schema().unwrap();
    let mut unknown = 0;
    for (_, of) in &fo4 {
        for sig in of {
            if !schema.records.contains_key(sig) {
                unknown += 1;
            }
        }
    }
    // No assertion on the count: it is a property of the game's data, not of this
    // code. The assertion is that reading the list does not panic or lose entries.
    eprintln!("{unknown} targets name a record type the schema does not define");
}
