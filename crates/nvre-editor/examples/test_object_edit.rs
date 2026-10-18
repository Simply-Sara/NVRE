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

    println!("All object editor tests passed successfully!");
}
