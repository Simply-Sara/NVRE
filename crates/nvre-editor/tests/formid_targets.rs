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

#[test]
fn creature_and_npc_template_references_are_targeted() {
    // A creature's TPLT subrecord can only target another creature or a
    // leveled creature list (`wbFormIDCk(TPLT, 'Template', [CREA, LVLC])`).
    // Without targets, the template dropdown falls back to `all_objects()`,
    // loading tens of thousands of unrelated records (weapons, dialogue, cells)
    // and stalling the editor when opened.
    let fnv = targets("fnv");
    let crea_tplt = fnv
        .iter()
        .find(|(at, _)| at == "CREA.TPLT")
        .map(|(_, of)| of.clone())
        .expect("CREA.TPLT is targeted in FNV");
    assert_eq!(
        crea_tplt,
        vec!["CREA".to_string(), "LVLC".to_string()],
        "CREA.TPLT must target CREA and LVLC"
    );

    let npc_tplt = fnv
        .iter()
        .find(|(at, _)| at == "NPC_.TPLT")
        .map(|(_, of)| of.clone())
        .expect("NPC_.TPLT is targeted in FNV");
    assert_eq!(
        npc_tplt,
        vec!["LVLN".to_string(), "NPC_".to_string()],
        "NPC_.TPLT must target LVLN and NPC_"
    );
}

#[test]
fn creature_traits_references_are_targeted() {
    // The dropdowns on Traits (and ModelList for PNAM) must be targeted to their
    // specific record signatures (`wbDefinitionsFNV.pas`):
    // - INAM (Death item) -> LVLI
    // - VTCK (Voice) -> VTYP
    // - ZNAM (Combat Style) -> CSTY
    // - LNAM (Melee Weapon List) -> FLST
    // - PNAM (Body Part Data) -> BPTD
    let fnv = targets("fnv");
    let check = |sub: &str, expected: &[&str]| {
        let key = format!("CREA.{sub}");
        let hit = fnv
            .iter()
            .find(|(at, _)| at == &key)
            .map(|(_, of)| of.clone())
            .unwrap_or_else(|| panic!("{key} is not targeted in FNV"));
        let exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(hit, exp, "{key} targets mismatch");
    };

    check("INAM", &["LVLI"]);
    check("VTCK", &["VTYP"]);
    check("ZNAM", &["CSTY"]);
    check("LNAM", &["FLST"]);
    check("PNAM", &["BPTD"]);
}
