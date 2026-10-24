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
    let schema_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv"));

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
        "Loaded FACT 0x{:08X}: EDID='{}', Flags1={:#04x}, Flags2={:#04x}, Relations={}",
        fact_state.form_id,
        fact_state.edid,
        fact_state.fact_flags1,
        fact_state.fact_flags2,
        fact_state.fact_relations.len()
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
    assert_eq!(crea_state.actor_template, Some(0x0014D1FD));
    assert_eq!(crea_state.actor_splo, vec![0x00060C16]);
    println!("CREA SPLO Spells: {:08X?}", crea_state.actor_splo);
    assert_eq!(
        crea_state.actor_factions,
        vec![(0x0005FF9F, 0), (0x00021472, 0), (0x0007E268, 0)]
    );
    println!("CREA SNAM Factions: {:08X?}", crea_state.actor_factions);

    assert_eq!(crea_state.crea_attack_reach, 32);
    assert!((crea_state.actor_base_scale - 1.25).abs() < 1e-4);
    assert!((crea_state.crea_turning_speed - 30.0).abs() < 1e-4);
    assert!((crea_state.crea_foot_weight - 20.0).abs() < 1e-4);
    assert_eq!(crea_state.actor_impact_material, 6); // Organic
    assert_eq!(crea_state.actor_sound_level, 1); // Normal
    assert_eq!(crea_state.actor_body_part_data, Some(0x00015833));
    assert_eq!(crea_state.crea_impact_dataset, Some(0x0007DB64));
    assert_eq!(crea_state.actor_packages, vec![0x0014D1FF, 0x000200D5]);
    assert_eq!(crea_state.obnd_min, [-40, -80, 0]);
    assert_eq!(crea_state.obnd_max, [40, 80, 80]);
    println!("CREA Physical & Combat: Reach={}, Scale={}, Turn={}, Foot={}, Mat={}, Snd={}, BPTD={:08X?}, IPDS={:08X?}, PKID={:08X?}, OBND={:?}..{:?}",
        crea_state.crea_attack_reach,
        crea_state.actor_base_scale,
        crea_state.crea_turning_speed,
        crea_state.crea_foot_weight,
        crea_state.actor_impact_material,
        crea_state.actor_sound_level,
        crea_state.actor_body_part_data,
        crea_state.crea_impact_dataset,
        crea_state.actor_packages,
        crea_state.obnd_min,
        crea_state.obnd_max,
    );

    let title = nvre_editor::object_editor::window_title(
        &session.schema,
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
        "Loaded CREA 0x{:08X}: EDID='{}', WeaponList={:08X?}, QuestItem={}, Pickpocket={}, NoLowLevelProc={}",
        sm_state.form_id,
        sm_state.edid,
        sm_state.crea_weapon_list,
        sm_state.quest_item,
        sm_state.actor_allow_pickpocket,
        sm_state.actor_no_low_level_proc
    );
    assert_eq!(sm_state.edid, "CrSuperMutantDeadMinigunRubyHill");
    assert_eq!(sm_state.crea_weapon_list, Some(0x0003A15E));
    assert_eq!(sm_state.actor_template, Some(0x00027FB3));
    assert!(sm_state.quest_item);
    assert!(sm_state.actor_allow_pickpocket);
    assert!(sm_state.actor_no_low_level_proc);

    // Test 17: Ghost creature (NVCRTumbleweed 0x0016938D)
    let tw_fid = 0x0016938D;
    let mut tw_state = ObjectEditorState::default();
    tw_state.load_from_session(&session, tw_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', IsGhost={}, NoKnockdowns={}, NoHeadTrack={}",
        tw_state.form_id,
        tw_state.edid,
        tw_state.actor_is_ghost,
        tw_state.actor_no_knockdowns,
        tw_state.crea_no_head_track
    );
    assert_eq!(tw_state.edid, "NVCRTumbleweed");
    assert!(tw_state.actor_is_ghost);
    assert!(tw_state.actor_no_knockdowns);
    assert!(tw_state.crea_no_head_track);

    // Test 18: Invulnerable creature (YesManMainframe 0x0016A21D)
    let ym_fid = 0x0016A21D;
    let mut ym_state = ObjectEditorState::default();
    ym_state.load_from_session(&session, ym_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', Invulnerable={}",
        ym_state.form_id, ym_state.edid, ym_state.actor_invulnerable
    );
    assert_eq!(ym_state.edid, "YesManMainframe");
    assert!(ym_state.actor_invulnerable);

    // Test 19: Creature with 3 SPLO spells (VCrTier3FeralGhoulGlowingOneNRSewers 0x0017B060)
    let ghoul_fid = 0x0017B060;
    let mut ghoul_state = ObjectEditorState::default();
    ghoul_state.load_from_session(&session, ghoul_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', SPLO count={}, Spells={:08X?}",
        ghoul_state.form_id,
        ghoul_state.edid,
        ghoul_state.actor_splo.len(),
        ghoul_state.actor_splo
    );
    assert_eq!(ghoul_state.edid, "VCrTier3FeralGhoulGlowingOneNRSewers");
    assert_eq!(ghoul_state.actor_template, Some(0x00167EE6));
    assert_eq!(
        ghoul_state.actor_splo,
        vec![0x0008AF5C, 0x0006E770, 0x000617BE]
    );

    // Test 20: Creature with Sound Type Overrides (AudioTemplateDeathclawBaby 0x0016AC7A)
    let dc_fid = 0x0016AC7A;
    let mut dc_state = ObjectEditorState::default();
    dc_state.load_from_session(&session, dc_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', Sound Overrides count={}",
        dc_state.form_id,
        dc_state.edid,
        dc_state.crea_sounds.len()
    );
    assert_eq!(dc_state.edid, "AudioTemplateDeathclawBaby");
    assert_eq!(dc_state.crea_sounds.len(), 14);
    assert_eq!(dc_state.crea_sounds[0], (0, 0x000A5D3C, 100)); // Left foot
    assert_eq!(dc_state.crea_sounds[4], (4, 0x000C029C, 50)); // Idle 50%
    assert_eq!(dc_state.crea_sounds[5], (8, 0x0016AC84, 100)); // Death 100%

    // Test 21: Creature with Animations (REPCONMrHandy2 0x00174971)
    let handy_fid = 0x00174971;
    let mut handy_state = ObjectEditorState::default();
    handy_state.load_from_session(&session, handy_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', Animations count={}, First={}",
        handy_state.form_id,
        handy_state.edid,
        handy_state.crea_animations.len(),
        handy_state
            .crea_animations
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
    );
    assert_eq!(handy_state.edid, "REPCONMrHandy2");
    assert_eq!(handy_state.crea_animations.len(), 15);
    assert_eq!(handy_state.crea_animations[0], "1hmaim.kf");

    // Test 22: Creature with Destruction Data (V34TurretCeiling01 0x00173FD6)
    let turret_fid = 0x00173FD6;
    let mut turret_state = ObjectEditorState::default();
    turret_state.load_from_session(&session, turret_fid, *b"CREA");
    println!(
        "Loaded CREA 0x{:08X}: EDID='{}', Destruction Health={}",
        turret_state.form_id, turret_state.edid, turret_state.dest_health
    );
    assert_eq!(turret_state.edid, "V34TurretCeiling01");
    assert_eq!(turret_state.dest_health, 40);

    // Test 23: Leveled Creature (VEncVarLL1Tier2Valley35 0x00175F0D)
    let lvlc_fid = 0x00175F0D;
    let mut lvlc_state = ObjectEditorState::default();
    lvlc_state.load_from_session(&session, lvlc_fid, *b"LVLC");
    println!(
        "Loaded LVLC 0x{:08X}: EDID='{}', ChanceNone={}, AllLevels={}, Entries count={}",
        lvlc_state.form_id,
        lvlc_state.edid,
        lvlc_state.lvl_chance_none,
        lvlc_state.lvl_all_levels,
        lvlc_state.lvl_entries.len()
    );
    assert_eq!(lvlc_state.edid, "VEncVarLL1Tier2Valley35");
    assert_eq!(lvlc_state.lvl_chance_none, 0);
    assert!(lvlc_state.lvl_all_levels);
    assert!(!lvlc_state.lvl_each_item);
    assert_eq!(lvlc_state.lvl_entries.len(), 5);
    assert_eq!(lvlc_state.lvl_entries[0].level, 1);
    assert_eq!(lvlc_state.lvl_entries[0].reference, Some(0x00156A9F));
    assert_eq!(lvlc_state.lvl_entries[0].count, 1);
    assert_eq!(lvlc_state.lvl_entries[1].level, 1);
    assert_eq!(lvlc_state.lvl_entries[1].reference, Some(0x00157B25));
    assert_eq!(lvlc_state.lvl_entries[1].count, 1);

    // Modify and save LVLC
    lvlc_state.lvl_chance_none = 25;
    lvlc_state
        .lvl_entries
        .push(nvre_editor::object_editor::LeveledListEntry {
            level: 5,
            reference: Some(0x00156A9F),
            count: 2,
            ..Default::default()
        });
    lvlc_state.save_to_session(&mut session);
    println!("LVLC edit queued successfully.");

    // Test 24: Leveled NPC (VarVHDNCRRanger9Veteran 0x0017A2FE)
    let lvln_fid = 0x0017A2FE;
    let mut lvln_state = ObjectEditorState::default();
    lvln_state.load_from_session(&session, lvln_fid, *b"LVLN");
    println!(
        "Loaded LVLN 0x{:08X}: EDID='{}', ChanceNone={}, EachItem={}, Entries count={}",
        lvln_state.form_id,
        lvln_state.edid,
        lvln_state.lvl_chance_none,
        lvln_state.lvl_each_item,
        lvln_state.lvl_entries.len()
    );
    assert_eq!(lvln_state.edid, "VarVHDNCRRanger9Veteran");
    assert_eq!(lvln_state.lvl_chance_none, 0);
    assert!(!lvln_state.lvl_all_levels);
    assert!(lvln_state.lvl_each_item);
    assert_eq!(lvln_state.lvl_entries.len(), 2);
    assert_eq!(lvln_state.lvl_entries[0].level, 33);
    assert_eq!(lvln_state.lvl_entries[0].reference, Some(0x0017A2F4));
    assert_eq!(lvln_state.lvl_entries[0].count, 1);
    assert_eq!(lvln_state.lvl_entries[1].level, 33);
    assert_eq!(lvln_state.lvl_entries[1].reference, Some(0x0017A2F5));
    assert_eq!(lvln_state.lvl_entries[1].count, 1);

    let lvln_title = nvre_editor::object_editor::window_title(
        &session.schema,
        &lvln_state.sig,
        &lvln_state.edid,
        &lvln_state.full_name,
    );
    assert_eq!(lvln_title, "Leveled NPC - VarVHDNCRRanger9Veteran");

    // Modify and save LVLN
    lvln_state.lvl_chance_none = 10;
    lvln_state
        .lvl_entries
        .push(nvre_editor::object_editor::LeveledListEntry {
            level: 35,
            reference: Some(0x0017A2F4),
            count: 1,
            ..Default::default()
        });
    lvln_state.save_to_session(&mut session);
    println!("LVLN edit queued successfully.");

    // Test 25: Non-Player Character (NVProspectorFemaleT 0x0017B586)
    let npc_fid = 0x0017B586;
    let mut npc_state = ObjectEditorState::default();
    npc_state.load_from_session(&session, npc_fid, *b"NPC_");
    println!(
        "Loaded NPC_ 0x{:08X}: EDID='{}', HP={}, STR={}, LCK={}, Hair={:08X?}, Eyes={:08X?}, HairColor={:?}, Skills={:?}",
        npc_state.form_id,
        npc_state.edid,
        npc_state.actor_health,
        npc_state.actor_strength,
        npc_state.actor_luck,
        npc_state.npc_hair,
        npc_state.npc_eyes,
        npc_state.npc_hair_color,
        &npc_state.npc_skills[0..5]
    );
    assert_eq!(npc_state.edid, "NVProspectorFemaleT");
    assert_eq!(npc_state.actor_health, 60);
    assert_eq!(npc_state.actor_strength, 6);
    assert_eq!(npc_state.actor_perception, 6);
    assert_eq!(npc_state.actor_endurance, 5);
    assert_eq!(npc_state.actor_charisma, 4);
    assert_eq!(npc_state.actor_intelligence, 4);
    assert_eq!(npc_state.actor_agility, 6);
    assert_eq!(npc_state.actor_luck, 4);
    assert_eq!(npc_state.npc_hair, Some(0x0005DC6B));
    assert_eq!(npc_state.npc_eyes, Some(0x00004252));
    assert_eq!(npc_state.npc_hair_color, [74, 37, 19, 0]);
    assert_eq!(
        npc_state.npc_skills,
        [12, 47, 14, 14, 14, 12, 51, 12, 12, 51, 51, 12, 12, 12]
    );
    assert_eq!(
        npc_state.npc_skill_offsets,
        [15, 0, 20, 30, 10, 5, 0, 25, 5, 4, 0, 10, 40, 20]
    );

    // Modify and save NPC_
    npc_state.actor_health = 75;
    npc_state.actor_luck = 7;
    npc_state.npc_skills[0] = 25; // Barter
    npc_state.save_to_session(&mut session);
    println!("NPC_ edit queued successfully.");

    // Test 26: Actor Value (AVEndurance 0x000003EA)
    let avif_fid = 0x000003EA;
    let mut avif_state = ObjectEditorState::default();
    avif_state.load_from_session(&session, avif_fid, *b"AVIF");
    println!(
        "Loaded AVIF 0x{:08X}: EDID='{}', Name='{}', ShortName='{}', Icon='{}'",
        avif_state.form_id,
        avif_state.edid,
        avif_state.full_name,
        avif_state.avif_short_name,
        avif_state.avif_icon
    );
    assert_eq!(avif_state.edid, "AVEndurance");
    assert_eq!(avif_state.full_name, "Endurance");
    assert_eq!(avif_state.avif_short_name, "END");
    assert!(avif_state.avif_desc.starts_with("Endurance is a measure"));
    assert_eq!(
        avif_state.avif_icon,
        "Interface\\Icons\\PipboyImages\\S.P.E.C.I.A.L\\SPECIAL_Endurance.dds"
    );

    let avif_title = nvre_editor::object_editor::window_title(
        &session.schema,
        &avif_state.sig,
        &avif_state.edid,
        &avif_state.full_name,
    );
    assert_eq!(
        avif_title,
        "ActorValue Information - AVEndurance (Endurance)"
    );

    // Modify and save AVIF
    avif_state.avif_short_name = "END_MOD".to_string();
    avif_state.save_to_session(&mut session);
    println!("AVIF edit queued successfully.");

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
