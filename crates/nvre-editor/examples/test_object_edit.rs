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

    // Test 9: Class (CLAS) 0x0017A636
    let clas_fid = 0x0017A636;
    let mut clas_state = ObjectEditorState::default();
    clas_state.load_from_session(&session, clas_fid, *b"CLAS");
    println!(
        "Loaded CLAS 0x{:08X}: EDID='{}', Teaches={}, Playable={}",
        clas_state.form_id, clas_state.edid, clas_state.clas_teaches, clas_state.clas_playable
    );
    assert_eq!(clas_state.edid, "LegionaryBadAss");

    // Test 10: Leveled Item (LVLI) 0x0017B7E5
    let lvli_fid = 0x0017B7E5;
    let mut lvli_state = ObjectEditorState::default();
    lvli_state.load_from_session(&session, lvli_fid, *b"LVLI");
    println!(
        "Loaded LVLI 0x{:08X}: EDID='{}', ChanceNone={}, AllLevels={}",
        lvli_state.form_id, lvli_state.edid, lvli_state.lvl_chance_none, lvli_state.lvl_all_levels
    );
    assert_eq!(lvli_state.edid, "CondLegionArmorPrimeDecanusAll");

    // Test 11: Weather (WTHR) 0x00176575
    let wthr_fid = 0x00176575;
    let mut wthr_state = ObjectEditorState::default();
    wthr_state.load_from_session(&session, wthr_fid, *b"WTHR");
    println!(
        "Loaded WTHR 0x{:08X}: EDID='{}', WindSpeed={}, SunGlare={}",
        wthr_state.form_id, wthr_state.edid, wthr_state.wthr_wind_speed, wthr_state.wthr_sun_glare
    );
    assert_eq!(wthr_state.edid, "NVWastelandClearEast");

    // Test 12: Global Variable (GLOB) 0x00179B16
    let glob_fid = 0x00179B16;
    let mut glob_state = ObjectEditorState::default();
    glob_state.load_from_session(&session, glob_fid, *b"GLOB");
    println!(
        "Loaded GLOB 0x{:08X}: EDID='{}', Type={}, Val={}",
        glob_state.form_id, glob_state.edid, glob_state.glob_type as char, glob_state.glob_val
    );
    assert_eq!(glob_state.edid, "EuclidPointerRange");

    // Test 13: Game Setting (GMST) 0x0017B37C
    let gmst_fid = 0x0017B37C;
    let mut gmst_state = ObjectEditorState::default();
    gmst_state.load_from_session(&session, gmst_fid, *b"GMST");
    println!(
        "Loaded GMST 0x{:08X}: EDID='{}', StrVal='{}'",
        gmst_state.form_id, gmst_state.edid, gmst_state.gmst_val_str
    );
    assert_eq!(gmst_state.edid, "sPoisonNoWeaponMessage");

    // Test 14: Race (RACE) 0x000987DF
    let race_fid = 0x000987DF;
    let mut race_state = ObjectEditorState::default();
    race_state.load_from_session(&session, race_fid, *b"RACE");
    println!(
        "Loaded RACE 0x{:08X}: EDID='{}', MaleH={}, FemaleH={}",
        race_state.form_id,
        race_state.edid,
        race_state.race_male_height,
        race_state.race_female_height
    );
    assert_eq!(race_state.edid, "CaucasianOldAged");

    // Test 15: Creature (CREA) 0x0017B587
    let crea_fid = 0x0017B587;
    let mut crea_state = ObjectEditorState::default();
    crea_state.load_from_session(&session, crea_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', Name='{}', Type={}, HP={}, Damage={}, Str={}, DeathItem={:08X?}, Voice={:08X?}, CombatStyle={:08X?}",
        crea_state.form_id,
        crea_state.edid,
        crea_state.full_name,
        crea_state.crea_type,
        crea_state.actor_health,
        crea_state.actor_damage,
        crea_state.actor_strength,
        crea_state.actor_death_item,
        crea_state.actor_voice,
        crea_state.actor_combat_style,
    );
    assert_eq!(crea_state.edid, "VStreetCrBrahmin2");
    assert_eq!(crea_state.full_name, "Brahmin");
    assert_eq!(crea_state.actor_death_item, Some(0x0009D413));
    assert_eq!(crea_state.actor_combat_style, Some(0x0003A371));
    assert_eq!(crea_state.actor_voice, Some(0x00094EED));

    let title = nvre_editor::object_editor::window_title(
        &crea_state.sig,
        &crea_state.edid,
        &crea_state.full_name,
    );
    println!("CREA Window Title: '{}'", title);
    assert_eq!(title, "Creature - VStreetCrBrahmin2 (Brahmin)");

    // Test 16: Creature with Weapon List (CREA) 0x0017AF62 (CrSuperMutantDeadMinigunRubyHill)
    let sm_fid = 0x0017AF62;
    let mut sm_state = ObjectEditorState::default();
    sm_state.load_from_session(&session, sm_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', WeaponList={:08X?}",
        sm_state.form_id, sm_state.edid, sm_state.crea_weapon_list
    );
    assert_eq!(sm_state.edid, "CrSuperMutantDeadMinigunRubyHill");
    assert_eq!(sm_state.crea_weapon_list, Some(0x0003A15E));

    let flsts = session.form_lists();
    let vtyps = session.records_by_sig(*b"VTYP");
    let cstys = session.records_by_sig(*b"CSTY");
    let lvlis = session.records_by_sig(*b"LVLI");
    println!(
        "Session indexed {} FormLists, {} VoiceTypes, {} CombatStyles, {} LeveledItems",
        flsts.len(),
        vtyps.len(),
        cstys.len(),
        lvlis.len(),
    );
    assert!(!flsts.is_empty());
    assert_eq!(vtyps.len(), 100);
    assert_eq!(cstys.len(), 84);
    assert!(!lvlis.is_empty());

    let sm_flst = flsts.iter().find(|(fid, _)| *fid == 0x0003A15E);
    assert!(sm_flst.is_some());
    println!("Found FormList in session: {:?}", sm_flst);

    println!("All object editor tests passed successfully!");
}
