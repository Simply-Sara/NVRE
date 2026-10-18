use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;
use std::path::Path;

fn main() {
    let data_dir = std::env::var("NVRE_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new("/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data")
                .to_path_buf()
        });
    let schema_dir = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../NVForever/schemas/fnv"
    ));

    println!("Loading session...");
    let mut session = Session::open(
        &data_dir.join("FalloutNV.esm"),
        schema_dir,
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("failed to open FalloutNV.esm");

    // Test 1: Varmint Rifle (WEAP) 0x00004334
    let varmint_fid = 0x00004334;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, varmint_fid, *b"WEAP");
    println!(
        "Loaded WEAP 0x{:08X}: EDID='{}', Name='{}', Dmg={}, Speed={}, Clip={}, Val={}",
        state.form_id,
        state.edid,
        state.full_name,
        state.weap_dmg,
        state.weap_speed,
        state.weap_clip,
        state.weap_val
    );
    assert_eq!(state.edid, "WeapKnife");
    assert_eq!(state.full_name, "Knife");
    assert!(state.weap_dmg > 0);
    assert!(state.weap_clip > 0);

    // Modify weapon and save
    state.weap_dmg = 45;
    state.save_to_session(&mut session);
    assert!(session.doc.edits_for(varmint_fid).is_some());
    println!("Weapon edit queued successfully.");

    // Test 2: Combat Armor (ARMO) 0x0002042F
    let combat_armor_fid = 0x0002042F;
    let mut armo_state = ObjectEditorState::default();
    armo_state.load_from_session(&session, combat_armor_fid, *b"ARMO");
    println!(
        "Loaded ARMO 0x{:08X}: EDID='{}', Name='{}', DT={}, DR={}, Type={}, Val={}",
        armo_state.form_id,
        armo_state.edid,
        armo_state.full_name,
        armo_state.armo_dt,
        armo_state.armo_dr,
        armo_state.armo_type,
        armo_state.armo_val
    );
    assert_eq!(armo_state.edid, "ArmorRaider01");
    assert_eq!(armo_state.full_name, "Raider Painspike Armor");
    assert!(armo_state.armo_dt > 0.0);

    // Test 3: Stimpak (ALCH) 0x00015169
    let stimpak_fid = 0x00015169;
    let mut alch_state = ObjectEditorState::default();
    alch_state.load_from_session(&session, stimpak_fid, *b"ALCH");
    println!(
        "Loaded ALCH 0x{:08X}: EDID='{}', Name='{}', Weight={}, Val={}, is_medicine={}",
        alch_state.form_id,
        alch_state.edid,
        alch_state.full_name,
        alch_state.alch_weight,
        alch_state.alch_val,
        alch_state.alch_is_medicine
    );
    assert_eq!(alch_state.edid, "Stimpak");
    assert_eq!(alch_state.full_name, "Stimpak");
    assert!(alch_state.alch_is_medicine);

    // Test 4: 5.56mm Round (AMMO) 0x00004240
    let ammo_fid = 0x00004240;
    let mut ammo_state = ObjectEditorState::default();
    ammo_state.load_from_session(&session, ammo_fid, *b"AMMO");
    println!(
        "Loaded AMMO 0x{:08X}: EDID='{}', Name='{}', Speed={}, DmgMult={}, Val={}",
        ammo_state.form_id,
        ammo_state.edid,
        ammo_state.full_name,
        ammo_state.ammo_speed,
        ammo_state.ammo_dmg_mult,
        ammo_state.ammo_val
    );
    assert_eq!(ammo_state.edid, "Ammo556mm");
    assert_eq!(ammo_state.full_name, "5.56mm Round");

    // Test 5: Faction (FACT) 0x0017B76D
    let fact_fid = 0x0017B76D;
    let mut fact_state = ObjectEditorState::default();
    fact_state.load_from_session(&session, fact_fid, *b"FACT");
    println!(
        "Loaded FACT 0x{:08X}: EDID='{}', Hidden={}, Evil={}",
        fact_state.form_id, fact_state.edid, fact_state.fact_hidden, fact_state.fact_evil
    );
    assert_eq!(fact_state.edid, "OmertaBossFaction2");

    // Test 6: Sound (SOUN) 0x0017AE34
    let soun_fid = 0x0017AE34;
    let mut soun_state = ObjectEditorState::default();
    soun_state.load_from_session(&session, soun_fid, *b"SOUN");
    println!(
        "Loaded SOUN 0x{:08X}: EDID='{}', File='{}', Chance={}",
        soun_state.form_id, soun_state.edid, soun_state.sound_file, soun_state.sound_random_chance
    );
    assert_eq!(soun_state.edid, "EMTCampfireInactive02");
    assert!(!soun_state.sound_file.is_empty());

    // Test 7: Projectile (PROJ) 0x0017B7B9
    let proj_fid = 0x0017B7B9;
    let mut proj_state = ObjectEditorState::default();
    proj_state.load_from_session(&session, proj_fid, *b"PROJ");
    println!(
        "Loaded PROJ 0x{:08X}: EDID='{}', Type={}, Speed={}",
        proj_state.form_id, proj_state.edid, proj_state.proj_type, proj_state.proj_speed
    );
    assert_eq!(proj_state.edid, "SecuritronGrenadeProjectile");

    // Test 8: Note (NOTE) 0x00177875
    let note_fid = 0x00177875;
    let mut note_state = ObjectEditorState::default();
    note_state.load_from_session(&session, note_fid, *b"NOTE");
    println!(
        "Loaded NOTE 0x{:08X}: EDID='{}', Type={}, TextLen={}",
        note_state.form_id,
        note_state.edid,
        note_state.note_type,
        note_state.note_text.len()
    );
    assert_eq!(note_state.edid, "vYesManBunkerNote");

    println!("All object editor tests passed successfully!");
}
