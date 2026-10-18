//! Object Editor window: human-friendly editing for all GECK object types.
//!
//! Spawns dynamically when an object is opened via double-click or the Edit button
//! in the Objects window, and despawns completely when closed.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashMap;

/// The schedule label for the Object Editor window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectEditorSchedule;

/// Marker component for the dynamic Object Editor window.
#[derive(Component)]
pub struct ObjectEditorWindow;

/// Marker component for the dynamic Object Editor camera.
#[derive(Component)]
pub struct ObjectEditorCamera;

/// Active tab for tabbed editors (Weapon, Armor, NPC, Creature).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTab {
    #[default]
    Main,
    Secondary,
    Models,
    Audio,
    Advanced,
}

/// Dynamic editor state holding all parsed fields for the active record.
#[derive(Resource, Default)]
pub struct ObjectEditorState {
    pub loaded_target: Option<(u32, [u8; 4])>,
    pub tab: EditorTab,
    pub status: String,

    // Common fields
    pub form_id: u32,
    pub sig: [u8; 4],
    pub edid: String,
    pub full_name: String,
    pub model: String,
    pub script: Option<u32>,

    // Subrecord map for uncommitted overlay
    pub raw_subs: HashMap<[u8; 4], Vec<u8>>,

    // WEAP fields
    pub weap_val: i32,
    pub weap_hp: i32,
    pub weap_weight: f32,
    pub weap_dmg: i16,
    pub weap_clip: u8,
    pub weap_ammo: Option<u32>,
    pub weap_crit_dmg: u16,
    pub weap_crit_mult: f32,
    pub weap_crit_effect: Option<u32>,
    pub weap_speed: f32,
    pub weap_reach: f32,
    pub weap_sight_fov: f32,
    pub weap_skill: u8,
    pub weap_str_req: u8,
    pub weap_skill_req: u8,
    pub weap_num_proj: u8,
    pub weap_ap: f32,

    // ARMO fields
    pub armo_val: i32,
    pub armo_hp: i32,
    pub armo_weight: f32,
    pub armo_dt: f32,
    pub armo_dr: i16,
    pub armo_enchant: Option<u32>,
    pub armo_type: u8, // 0: Light, 1: Medium, 2: Heavy, 3: Power, 4: Non-Playable
    pub armo_biped_flags: u32,
    pub armo_modulates_voice: bool,

    // NPC_ & CREA fields
    pub actor_female: bool,
    pub actor_essential: bool,
    pub actor_respawn: bool,
    pub actor_flags: u32,
    pub actor_level_or_mult: i16,
    pub actor_pc_mult: bool,
    pub actor_calc_min: u16,
    pub actor_calc_max: u16,
    pub actor_speed_mult: u16,
    pub actor_karma: f32,
    pub actor_health: i16,
    pub actor_damage: i16,
    pub actor_fatigue: u16,
    pub actor_strength: u8,
    pub actor_perception: u8,
    pub actor_endurance: u8,
    pub actor_charisma: u8,
    pub actor_intelligence: u8,
    pub actor_agility: u8,
    pub actor_luck: u8,
    pub actor_aggression: u8,
    pub actor_confidence: u8,
    pub actor_assistance: i8,
    pub actor_mood: u8,
    pub actor_energy: u8,
    pub actor_resp: u8,
    pub actor_race: Option<u32>,
    pub actor_class: Option<u32>,
    pub actor_voice: Option<u32>,
    pub actor_template: Option<u32>,
    pub actor_combat_style: Option<u32>,

    // ALCH fields
    pub alch_weight: f32,
    pub alch_val: i32,
    pub alch_addict_chance: f32,
    pub alch_addict: Option<u32>,
    pub alch_sound: Option<u32>,
    pub alch_is_food: bool,
    pub alch_is_medicine: bool,
    pub alch_is_poison: bool,

    // AMMO fields
    pub ammo_val: i32,
    pub ammo_weight: f32,
    pub ammo_speed: f32,
    pub ammo_dmg_mult: f32,
    pub ammo_clip_rounds: u32,
    pub ammo_casing: Option<u32>,

    // CONT & DOOR fields
    pub cont_weight: f32,
    pub cont_respawn: bool,
    pub cont_sound_open: Option<u32>,
    pub cont_sound_close: Option<u32>,
    pub door_auto: bool,
    pub door_hidden: bool,
    pub door_minimal: bool,
    pub door_sliding: bool,

    // ACTI & TERM fields
    pub acti_prompt: String,
    pub term_difficulty: u8,
    pub term_welcome: String,

    // LIGH fields
    pub ligh_radius: u32,
    pub ligh_color: [u8; 3],
    pub ligh_falloff: f32,
    pub ligh_fov: f32,
    pub ligh_flags: u32,

    // BOOK & MISC fields
    pub item_val: i32,
    pub item_weight: f32,
    pub book_teaches: u8,
    pub book_text: String,

    // FACT & QUST fields
    pub fact_hidden: bool,
    pub fact_evil: bool,
    pub fact_special_combat: bool,
    pub fact_track_crime: bool,
    pub fact_reputation: Option<u32>,
    pub fact_rank_male: String,
    pub fact_rank_female: String,
    pub qust_start_enabled: bool,
    pub qust_repeated_stages: bool,
    pub qust_priority: u8,
    pub qust_delay: f32,

    // PERK & SPEL fields
    pub perk_trait: bool,
    pub perk_min_level: u8,
    pub perk_num_ranks: u8,
    pub perk_desc: String,
    pub spell_type: u32,
    pub spell_no_autocalc: bool,
    pub spell_always_applies: bool,

    // EXPL & PROJ fields
    pub expl_force: f32,
    pub expl_damage: f32,
    pub expl_radius: f32,
    pub expl_is_radius: f32,
    pub expl_light: Option<u32>,
    pub expl_sound1: Option<u32>,
    pub expl_sound2: Option<u32>,
    pub expl_rad_level: f32,
    pub proj_speed: f32,
    pub proj_gravity: f32,
    pub proj_range: f32,
    pub proj_type: u16,
    pub proj_light: Option<u32>,
    pub proj_sound: Option<u32>,
    pub proj_explosion: Option<u32>,

    // SOUN & NOTE fields
    pub sound_file: String,
    pub sound_min_dist: u8,
    pub sound_max_dist: u8,
    pub sound_freq_adj: i8,
    pub sound_vol_cdb: i16,
    pub sound_random_chance: u8,
    pub sound_loop: bool,
    pub sound_2d: bool,
    pub note_type: u8,
    pub note_text: String,
    pub note_sound: Option<u32>,

    // LVLI / LVLN / LVLC fields
    pub lvl_chance_none: u8,
    pub lvl_all_levels: bool,
    pub lvl_each_item: bool,
    pub lvl_use_all: bool,
    pub lvl_global: Option<u32>,

    // IMOD & CLAS fields
    pub imod_val: u32,
    pub imod_weight: f32,
    pub imod_desc: String,
    pub clas_teaches: u8,
    pub clas_max_train: u8,
    pub clas_playable: bool,
    pub clas_guard: bool,

    // CSTY & MGEF fields
    pub csty_dodge_chance: u8,
    pub csty_lr_chance: u8,
    pub csty_power_chance: u8,
    pub csty_cover_radius: f32,
    pub csty_take_cover_chance: f32,
    pub mgef_hostile: bool,
    pub mgef_recover: bool,
    pub mgef_detrimental: bool,
    pub mgef_self: bool,
    pub mgef_touch: bool,
    pub mgef_target: bool,
    pub mgef_base_cost: f32,
    pub mgef_assoc_item: Option<u32>,

    // WTHR & WATR fields
    pub wthr_wind_speed: u8,
    pub wthr_sun_glare: u8,
    pub wthr_sun_damage: u8,
    pub wthr_cloud_layer0: String,
    pub watr_opacity: u8,
    pub watr_causes_damage: bool,
    pub watr_reflective: bool,
    pub watr_effect: Option<u32>,
    pub watr_sound: Option<u32>,

    // GLOB & GMST fields
    pub glob_type: u8,
    pub glob_val: f32,
    pub gmst_val_str: String,
    pub gmst_val_num: f32,

    // RACE & CLMT fields
    pub race_male_height: f32,
    pub race_female_height: f32,
    pub race_male_weight: f32,
    pub race_female_weight: f32,
    pub race_playable: bool,
    pub race_child: bool,
    pub race_desc: String,
    pub clmt_sunrise_begin: u8,
    pub clmt_sunrise_end: u8,
    pub clmt_sunset_begin: u8,
    pub clmt_sunset_end: u8,
    pub clmt_volatility: u8,
    pub clmt_sun_texture: String,
    pub clmt_sun_glare_texture: String,
}

impl ObjectEditorState {
    /// Load record data from session.
    pub fn load_from_session(
        &mut self,
        session: &crate::session::Session,
        form_id: u32,
        sig: [u8; 4],
    ) {
        self.loaded_target = Some((form_id, sig));
        self.form_id = form_id;
        self.sig = sig;
        self.tab = EditorTab::Main;
        self.status = format!("Loaded {} {:08X}", fnv_util::tag_str(&sig), form_id);

        let (raw_subs, found) = if let Some(rec) = session.doc.record(form_id) {
            (rec.subrecords().unwrap_or_default(), true)
        } else if let Ok(lo) = session.load_order() {
            if let Some((_, orig)) = fnv_world::fields::winner(lo, form_id) {
                (orig.subrecords().unwrap_or_default(), true)
            } else {
                (Vec::new(), false)
            }
        } else {
            (Vec::new(), false)
        };

        if !found {
            return;
        }

        self.raw_subs.clear();
        for s in raw_subs {
            self.raw_subs.insert(s.sig, s.data.clone());
        }
        if let Some(edits) = session.doc.edits_for(form_id) {
            for e in edits {
                if let nvre_edit::Edit::SetSub { sig, data } = e {
                    self.raw_subs.insert(*sig, data.clone());
                }
            }
        }

        // Common strings & FormIDs
        self.edid = self.get_string(b"EDID");
        self.full_name = self.get_string(b"FULL");
        self.model = self.get_string(b"MODL");
        self.script = self.get_fid(b"SCRI");

        match &sig {
            b"WEAP" => self.load_weap(),
            b"ARMO" => self.load_armo(),
            b"NPC_" | b"CREA" => self.load_actor(),
            b"ALCH" => self.load_alch(),
            b"AMMO" => self.load_ammo(),
            b"CONT" => self.load_cont(),
            b"DOOR" => self.load_door(),
            b"ACTI" => self.load_acti(),
            b"TERM" => self.load_term(),
            b"LIGH" => self.load_ligh(),
            b"BOOK" => self.load_book(),
            b"FACT" => self.load_fact(),
            b"QUST" => self.load_qust(),
            b"PERK" => self.load_perk(),
            b"SPEL" | b"ENCH" => self.load_spell(),
            b"EXPL" => self.load_expl(),
            b"PROJ" => self.load_proj(),
            b"SOUN" => self.load_soun(),
            b"NOTE" => self.load_note(),
            b"LVLI" | b"LVLN" | b"LVLC" => self.load_lvl(),
            b"IMOD" => self.load_imod(),
            b"CLAS" => self.load_clas(),
            b"CSTY" => self.load_csty(),
            b"MGEF" => self.load_mgef(),
            b"WTHR" => self.load_wthr(),
            b"WATR" => self.load_watr(),
            b"GLOB" => self.load_glob(),
            b"GMST" => self.load_gmst(),
            b"RACE" => self.load_race(),
            b"CLMT" => self.load_clmt(),
            _ => self.load_misc(),
        }
    }

    fn get_string(&self, sig: &[u8; 4]) -> String {
        self.raw_subs
            .get(sig)
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default()
    }

    fn get_fid(&self, sig: &[u8; 4]) -> Option<u32> {
        self.raw_subs
            .get(sig)
            .filter(|b| b.len() >= 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn load_weap(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 15) {
            self.weap_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.weap_hp = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.weap_weight = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.weap_dmg = i16::from_le_bytes([b[12], b[13]]);
            self.weap_clip = b[14];
        }
        self.weap_ammo = self.get_fid(b"NAM0");
        if let Some(b) = self.raw_subs.get(b"CRDT").filter(|b| b.len() >= 12) {
            self.weap_crit_dmg = u16::from_le_bytes([b[0], b[1]]);
            self.weap_crit_mult = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.weap_crit_effect = if b.len() >= 16 {
                Some(u32::from_le_bytes([b[12], b[13], b[14], b[15]]))
            } else {
                None
            };
        }
        if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| b.len() >= 44) {
            self.weap_speed = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.weap_reach = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            if b.len() >= 32 {
                self.weap_sight_fov = f32::from_le_bytes([b[28], b[29], b[30], b[31]]);
            }
            if b.len() > 42 {
                self.weap_num_proj = b[42];
            }
            if b.len() >= 72 {
                self.weap_ap = f32::from_le_bytes([b[68], b[69], b[70], b[71]]);
            }
            if b.len() >= 108 {
                let av = u32::from_le_bytes([b[104], b[105], b[106], b[107]]);
                self.weap_skill = match av {
                    33 => 1, // Guns
                    34 => 2, // Energy Weapons
                    37 => 3, // Melee
                    45 => 4, // Unarmed
                    35 => 5, // Explosives
                    _ => 0,
                };
            }
            if b.len() >= 180 {
                self.weap_str_req = b[176];
            }
        }
    }

    fn load_armo(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 12) {
            self.armo_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.armo_hp = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.armo_weight = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        }
        if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| b.len() >= 4) {
            self.armo_dr = i16::from_le_bytes([b[0], b[1]]);
            if b.len() >= 8 {
                self.armo_dt = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            }
            if b.len() >= 10 {
                self.armo_modulates_voice = (u16::from_le_bytes([b[8], b[9]]) & 1) != 0;
            }
        }
        if let Some(b) = self.raw_subs.get(b"BMDT").filter(|b| b.len() >= 5) {
            self.armo_biped_flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let general = b[4];
            self.armo_type = if general & (1 << 7) != 0 {
                2 // Heavy
            } else if general & (1 << 3) != 0 {
                1 // Medium
            } else if general & (1 << 5) != 0 {
                3 // Power Armor
            } else {
                0 // Light
            };
        }
        self.armo_enchant = self.get_fid(b"EITM");
    }

    fn load_actor(&mut self) {
        if let Some(b) = self.raw_subs.get(b"ACBS").filter(|b| b.len() >= 24) {
            self.actor_flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.actor_fatigue = u16::from_le_bytes([b[4], b[5]]);
            self.actor_level_or_mult = i16::from_le_bytes([b[8], b[9]]);
            self.actor_calc_min = u16::from_le_bytes([b[10], b[11]]);
            self.actor_calc_max = u16::from_le_bytes([b[12], b[13]]);
            self.actor_speed_mult = u16::from_le_bytes([b[14], b[15]]);
            self.actor_karma = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.actor_female = (self.actor_flags & 1) != 0;
            self.actor_essential = (self.actor_flags & 2) != 0;
            self.actor_respawn = (self.actor_flags & 8) != 0;
            self.actor_pc_mult = (self.actor_flags & (1 << 7)) != 0;
        }
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 17) {
            self.actor_health = i16::from_le_bytes([b[4], b[5]]);
            self.actor_damage = i16::from_le_bytes([b[8], b[9]]);
            self.actor_strength = b[10];
            self.actor_perception = b[11];
            self.actor_endurance = b[12];
            self.actor_charisma = b[13];
            self.actor_intelligence = b[14];
            self.actor_agility = b[15];
            self.actor_luck = b[16];
        }
        if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| b.len() >= 7) {
            self.actor_strength = b[0];
            self.actor_perception = b[1];
            self.actor_endurance = b[2];
            self.actor_charisma = b[3];
            self.actor_intelligence = b[4];
            self.actor_agility = b[5];
            self.actor_luck = b[6];
        }
        if let Some(b) = self.raw_subs.get(b"AIDT").filter(|b| b.len() >= 20) {
            self.actor_aggression = b[0];
            self.actor_confidence = b[1];
            self.actor_energy = b[2];
            self.actor_resp = b[3];
            self.actor_mood = b[4];
            self.actor_assistance = b[14] as i8;
        }
        self.actor_race = self.get_fid(b"RNAM");
        self.actor_class = self.get_fid(b"CNAM");
        self.actor_voice = self.get_fid(b"VTCK");
        self.actor_template = self.get_fid(b"TPLT");
        self.actor_combat_style = self.get_fid(b"ZNAM");
    }

    fn load_alch(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 4) {
            self.alch_weight = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        if let Some(b) = self.raw_subs.get(b"ENIT").filter(|b| b.len() >= 16) {
            self.alch_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let flags = b[4];
            self.alch_is_food = flags & 2 != 0;
            self.alch_is_medicine = flags & 4 != 0;
            self.alch_is_poison = flags & 8 != 0;
            self.alch_addict_chance = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.alch_addict = Some(u32::from_le_bytes([b[12], b[13], b[14], b[15]]));
        }
    }

    fn load_ammo(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 12) {
            self.ammo_speed = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.ammo_val = i32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            if b.len() > 12 {
                self.ammo_clip_rounds = b[12] as u32;
            }
        }
        if let Some(b) = self.raw_subs.get(b"DAT2").filter(|b| b.len() >= 12) {
            self.ammo_weight = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        }
        self.ammo_casing = self.get_fid(b"QNAM");
    }

    fn load_cont(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 5) {
            self.cont_respawn = (b[0] & 2) != 0;
            self.cont_weight = f32::from_le_bytes([b[1], b[2], b[3], b[4]]);
        }
        self.cont_sound_open = self.get_fid(b"SNAM");
        self.cont_sound_close = self.get_fid(b"QNAM");
    }

    fn load_door(&mut self) {
        if let Some(b) = self.raw_subs.get(b"FNAM") {
            let f = b.first().copied().unwrap_or(0);
            self.door_auto = f & 1 != 0;
            self.door_hidden = f & 2 != 0;
            self.door_minimal = f & 4 != 0;
            self.door_sliding = f & 8 != 0;
        }
    }

    fn load_acti(&mut self) {
        self.acti_prompt = self.get_string(b"RNAM");
    }

    fn load_term(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DNAM") {
            self.term_difficulty = b.first().copied().unwrap_or(0);
        }
        self.term_welcome = self.get_string(b"WNAM");
    }

    fn load_ligh(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 32) {
            self.ligh_radius = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.ligh_color = [b[8], b[9], b[10]];
            self.ligh_flags = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.ligh_falloff = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.ligh_fov = f32::from_le_bytes([b[20], b[21], b[22], b[23]]);
        }
    }

    fn load_book(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 10) {
            self.book_teaches = b[1];
            self.item_val = i32::from_le_bytes([b[2], b[3], b[4], b[5]]);
            self.item_weight = f32::from_le_bytes([b[6], b[7], b[8], b[9]]);
        }
        self.book_text = self.get_string(b"DESC");
    }

    fn load_misc(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.item_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.item_weight = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        }
    }

    fn load_fact(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| !b.is_empty()) {
            let f1 = b[0];
            self.fact_hidden = (f1 & 1) != 0;
            self.fact_evil = (f1 & 2) != 0;
            self.fact_special_combat = (f1 & 4) != 0;
            if b.len() > 1 {
                self.fact_track_crime = (b[1] & 1) != 0;
            }
        }
        self.fact_reputation = self.get_fid(b"WMI1");
        self.fact_rank_male = self.get_string(b"MNAM");
        self.fact_rank_female = self.get_string(b"FNAM");
    }

    fn load_qust(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 2) {
            let f = b[0];
            self.qust_start_enabled = (f & 1) != 0;
            self.qust_repeated_stages = (f & 8) != 0;
            self.qust_priority = b[1];
            if b.len() >= 8 {
                self.qust_delay = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            }
        }
    }

    fn load_perk(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 3) {
            self.perk_trait = b[0] != 0;
            self.perk_min_level = b[1];
            self.perk_num_ranks = b[2];
        }
        self.perk_desc = self.get_string(b"DESC");
    }

    fn load_spell(&mut self) {
        if let Some(b) = self.raw_subs.get(b"SPIT").filter(|b| b.len() >= 13) {
            self.spell_type = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let f = b[12];
            self.spell_no_autocalc = (f & 1) != 0;
            self.spell_always_applies = (f & 32) != 0;
        }
    }

    fn load_expl(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 28) {
            self.expl_force = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.expl_damage = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.expl_radius = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.expl_light = Some(u32::from_le_bytes([b[12], b[13], b[14], b[15]]));
            self.expl_sound1 = Some(u32::from_le_bytes([b[16], b[17], b[18], b[19]]));
            self.expl_is_radius = f32::from_le_bytes([b[24], b[25], b[26], b[27]]);
            if b.len() >= 40 {
                self.expl_sound2 = Some(u32::from_le_bytes([b[32], b[33], b[34], b[35]]));
                self.expl_rad_level = f32::from_le_bytes([b[36], b[37], b[38], b[39]]);
            }
        }
    }

    fn load_proj(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 16) {
            self.proj_type = u16::from_le_bytes([b[2], b[3]]);
            self.proj_gravity = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.proj_speed = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.proj_range = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            if b.len() >= 20 {
                self.proj_light = Some(u32::from_le_bytes([b[16], b[17], b[18], b[19]]));
            }
            if b.len() >= 44 {
                self.proj_explosion = Some(u32::from_le_bytes([b[36], b[37], b[38], b[39]]));
                self.proj_sound = Some(u32::from_le_bytes([b[40], b[41], b[42], b[43]]));
            }
        }
    }

    fn load_soun(&mut self) {
        self.sound_file = self.get_string(b"FNAM");
        if let Some(b) = self.raw_subs.get(b"RNAM") {
            self.sound_random_chance = b.first().copied().unwrap_or(100);
        }
        if let Some(b) = self.raw_subs.get(b"SNDD").filter(|b| b.len() >= 10) {
            self.sound_min_dist = b[0];
            self.sound_max_dist = b[1];
            self.sound_freq_adj = b[2] as i8;
            let flags = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.sound_loop = (flags & (1 << 4)) != 0;
            self.sound_2d = (flags & (1 << 6)) != 0;
            self.sound_vol_cdb = i16::from_le_bytes([b[8], b[9]]);
        }
    }

    fn load_note(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA") {
            self.note_type = b.first().copied().unwrap_or(1);
        }
        self.note_text = self.get_string(b"DESC");
        self.note_sound = self.get_fid(b"SNAM");
    }

    fn load_lvl(&mut self) {
        if let Some(b) = self.raw_subs.get(b"LVLD") {
            self.lvl_chance_none = b.first().copied().unwrap_or(0);
        }
        if let Some(b) = self.raw_subs.get(b"LVLF") {
            let f = b.first().copied().unwrap_or(0);
            self.lvl_all_levels = (f & 1) != 0;
            self.lvl_each_item = (f & 2) != 0;
            self.lvl_use_all = (f & 4) != 0;
        }
        self.lvl_global = self.get_fid(b"LVLG");
    }

    fn load_imod(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.imod_val = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.imod_weight = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        }
        self.imod_desc = self.get_string(b"DESC");
    }

    fn load_clas(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 26) {
            let flags = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.clas_playable = (flags & 1) != 0;
            self.clas_guard = (flags & 2) != 0;
            self.clas_teaches = b[24];
            self.clas_max_train = b[25];
        }
    }

    fn load_csty(&mut self) {
        if let Some(b) = self.raw_subs.get(b"CSTD").filter(|b| b.len() >= 25) {
            self.csty_dodge_chance = b[0];
            self.csty_lr_chance = b[1];
            self.csty_power_chance = b[24];
        }
        if let Some(b) = self.raw_subs.get(b"CSSD").filter(|b| b.len() >= 8) {
            self.csty_cover_radius = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.csty_take_cover_chance = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        }
    }

    fn load_mgef(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 12) {
            let f = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.mgef_hostile = (f & (1 << 0)) != 0;
            self.mgef_recover = (f & (1 << 1)) != 0;
            self.mgef_detrimental = (f & (1 << 2)) != 0;
            self.mgef_self = (f & (1 << 4)) != 0;
            self.mgef_touch = (f & (1 << 5)) != 0;
            self.mgef_target = (f & (1 << 6)) != 0;
            self.mgef_base_cost = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.mgef_assoc_item = Some(u32::from_le_bytes([b[8], b[9], b[10], b[11]]));
        }
    }

    fn load_wthr(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 6) {
            self.wthr_wind_speed = b[0];
            self.wthr_sun_glare = b[4];
            self.wthr_sun_damage = b[5];
        }
        self.wthr_cloud_layer0 = self.get_string(b"DNAM");
    }

    fn load_watr(&mut self) {
        if let Some(b) = self.raw_subs.get(b"ANAM") {
            self.watr_opacity = b.first().copied().unwrap_or(255);
        }
        if let Some(b) = self.raw_subs.get(b"FNAM") {
            let f = b.first().copied().unwrap_or(0);
            self.watr_causes_damage = (f & 1) != 0;
            self.watr_reflective = (f & 2) != 0;
        }
        self.watr_effect = self.get_fid(b"XNAM");
        self.watr_sound = self.get_fid(b"SNAM");
    }

    fn load_glob(&mut self) {
        if let Some(b) = self.raw_subs.get(b"FNAM") {
            self.glob_type = b.first().copied().unwrap_or(b'f');
        }
        if let Some(b) = self.raw_subs.get(b"FLTV").filter(|b| b.len() >= 4) {
            self.glob_val = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
    }

    fn load_gmst(&mut self) {
        if self.edid.starts_with('s') || self.edid.starts_with('S') {
            self.gmst_val_str = self.get_string(b"DATA");
        } else if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 4) {
            if self.edid.starts_with('i') || self.edid.starts_with('b') {
                self.gmst_val_num = i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32;
            } else {
                self.gmst_val_num = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            }
        }
    }

    fn load_race(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 36) {
            self.race_male_height = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.race_female_height = f32::from_le_bytes([b[20], b[21], b[22], b[23]]);
            self.race_male_weight = f32::from_le_bytes([b[24], b[25], b[26], b[27]]);
            self.race_female_weight = f32::from_le_bytes([b[28], b[29], b[30], b[31]]);
            let f = u32::from_le_bytes([b[32], b[33], b[34], b[35]]);
            self.race_playable = (f & 1) != 0;
            self.race_child = (f & 4) != 0;
        }
        self.race_desc = self.get_string(b"DESC");
    }

    fn load_clmt(&mut self) {
        if let Some(b) = self.raw_subs.get(b"TNAM").filter(|b| b.len() >= 5) {
            self.clmt_sunrise_begin = b[0];
            self.clmt_sunrise_end = b[1];
            self.clmt_sunset_begin = b[2];
            self.clmt_sunset_end = b[3];
            self.clmt_volatility = b[4];
        }
        self.clmt_sun_texture = self.get_string(b"FNAM");
        self.clmt_sun_glare_texture = self.get_string(b"GNAM");
    }

    /// Save the current state back to session as uncommitted edits.
    pub fn save_to_session(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // Strings
        session
            .doc
            .set_sub(fid, b"EDID", encode_zstring(&self.edid));
        if !self.full_name.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FULL", encode_zstring(&self.full_name));
        }
        if !self.model.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&self.model));
        }
        if let Some(s) = self.script {
            session.doc.set_sub(fid, b"SCRI", s.to_le_bytes().to_vec());
        }

        match &self.sig {
            b"WEAP" => self.save_weap(session),
            b"ARMO" => self.save_armo(session),
            b"NPC_" | b"CREA" => self.save_actor(session),
            b"ALCH" => self.save_alch(session),
            b"AMMO" => self.save_ammo(session),
            b"CONT" => self.save_cont(session),
            b"DOOR" => self.save_door(session),
            b"ACTI" => self.save_acti(session),
            b"TERM" => self.save_term(session),
            b"LIGH" => self.save_ligh(session),
            b"BOOK" => self.save_book(session),
            b"FACT" => self.save_fact(session),
            b"QUST" => self.save_qust(session),
            b"PERK" => self.save_perk(session),
            b"SPEL" | b"ENCH" => self.save_spell(session),
            b"EXPL" => self.save_expl(session),
            b"PROJ" => self.save_proj(session),
            b"SOUN" => self.save_soun(session),
            b"NOTE" => self.save_note(session),
            b"LVLI" | b"LVLN" | b"LVLC" => self.save_lvl(session),
            b"IMOD" => self.save_imod(session),
            b"CLAS" => self.save_clas(session),
            b"CSTY" => self.save_csty(session),
            b"MGEF" => self.save_mgef(session),
            b"WTHR" => self.save_wthr(session),
            b"WATR" => self.save_watr(session),
            b"GLOB" => self.save_glob(session),
            b"GMST" => self.save_gmst(session),
            b"RACE" => self.save_race(session),
            b"CLMT" => self.save_clmt(session),
            _ => self.save_misc(session),
        }

        session.status = format!(
            "queued edits on {} {:08X}",
            fnv_util::tag_str(&self.sig),
            fid
        );
    }

    fn save_weap(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 15];
        d[0..4].copy_from_slice(&self.weap_val.to_le_bytes());
        d[4..8].copy_from_slice(&self.weap_hp.to_le_bytes());
        d[8..12].copy_from_slice(&self.weap_weight.to_le_bytes());
        d[12..14].copy_from_slice(&self.weap_dmg.to_le_bytes());
        d[14] = self.weap_clip;
        session.doc.set_sub(fid, b"DATA", d);

        if let Some(ammo) = self.weap_ammo {
            session
                .doc
                .set_sub(fid, b"NAM0", ammo.to_le_bytes().to_vec());
        }

        let mut crdt = vec![0u8; 16];
        crdt[0..2].copy_from_slice(&self.weap_crit_dmg.to_le_bytes());
        crdt[4..8].copy_from_slice(&self.weap_crit_mult.to_le_bytes());
        if let Some(eff) = self.weap_crit_effect {
            crdt[12..16].copy_from_slice(&eff.to_le_bytes());
        }
        session.doc.set_sub(fid, b"CRDT", crdt);

        let mut dnam = self
            .raw_subs
            .get(b"DNAM")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 108]);
        if dnam.len() < 108 {
            dnam.resize(108, 0);
        }
        dnam[4..8].copy_from_slice(&self.weap_speed.to_le_bytes());
        dnam[8..12].copy_from_slice(&self.weap_reach.to_le_bytes());
        if dnam.len() >= 32 {
            dnam[28..32].copy_from_slice(&self.weap_sight_fov.to_le_bytes());
        }
        if dnam.len() > 42 {
            dnam[42] = self.weap_num_proj;
        }
        if dnam.len() >= 72 {
            dnam[68..72].copy_from_slice(&self.weap_ap.to_le_bytes());
        }
        if dnam.len() >= 108 {
            let av: u32 = match self.weap_skill {
                1 => 33, // Guns
                2 => 34, // Energy Weapons
                3 => 37, // Melee
                4 => 45, // Unarmed
                5 => 35, // Explosives
                _ => 0,
            };
            dnam[104..108].copy_from_slice(&av.to_le_bytes());
        }
        if dnam.len() >= 180 {
            dnam[176] = self.weap_str_req;
        }
        session.doc.set_sub(fid, b"DNAM", dnam);
    }

    fn save_armo(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 12];
        d[0..4].copy_from_slice(&self.armo_val.to_le_bytes());
        d[4..8].copy_from_slice(&self.armo_hp.to_le_bytes());
        d[8..12].copy_from_slice(&self.armo_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);

        let mut dnam = vec![0u8; 12];
        dnam[0..2].copy_from_slice(&self.armo_dr.to_le_bytes());
        dnam[4..8].copy_from_slice(&self.armo_dt.to_le_bytes());
        let flags: u16 = if self.armo_modulates_voice { 1 } else { 0 };
        dnam[8..10].copy_from_slice(&flags.to_le_bytes());
        session.doc.set_sub(fid, b"DNAM", dnam);

        let mut bmdt = vec![0u8; 8];
        bmdt[0..4].copy_from_slice(&self.armo_biped_flags.to_le_bytes());
        let general: u8 = match self.armo_type {
            1 => 1 << 3, // Medium
            2 => 1 << 7, // Heavy
            3 => 1 << 5, // Power
            _ => 0,      // Light
        };
        bmdt[4] = general;
        session.doc.set_sub(fid, b"BMDT", bmdt);

        if let Some(ench) = self.armo_enchant {
            session
                .doc
                .set_sub(fid, b"EITM", ench.to_le_bytes().to_vec());
        }
    }

    fn save_actor(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut acbs = self
            .raw_subs
            .get(b"ACBS")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 24]);
        if acbs.len() < 24 {
            acbs.resize(24, 0);
        }
        let mut f = self.actor_flags;
        if self.actor_female {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.actor_essential {
            f |= 2;
        } else {
            f &= !2;
        }
        if self.actor_respawn {
            f |= 8;
        } else {
            f &= !8;
        }
        if self.actor_pc_mult {
            f |= 1 << 7;
        } else {
            f &= !(1 << 7);
        }
        acbs[0..4].copy_from_slice(&f.to_le_bytes());
        acbs[4..6].copy_from_slice(&self.actor_fatigue.to_le_bytes());
        acbs[8..10].copy_from_slice(&self.actor_level_or_mult.to_le_bytes());
        acbs[10..12].copy_from_slice(&self.actor_calc_min.to_le_bytes());
        acbs[12..14].copy_from_slice(&self.actor_calc_max.to_le_bytes());
        acbs[14..16].copy_from_slice(&self.actor_speed_mult.to_le_bytes());
        acbs[16..20].copy_from_slice(&self.actor_karma.to_le_bytes());
        session.doc.set_sub(fid, b"ACBS", acbs);

        let mut aidt = self
            .raw_subs
            .get(b"AIDT")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 20]);
        if aidt.len() < 20 {
            aidt.resize(20, 0);
        }
        aidt[0] = self.actor_aggression;
        aidt[1] = self.actor_confidence;
        aidt[2] = self.actor_energy;
        aidt[3] = self.actor_resp;
        aidt[4] = self.actor_mood;
        aidt[14] = self.actor_assistance as u8;
        session.doc.set_sub(fid, b"AIDT", aidt);

        if self.sig == *b"NPC_" {
            let dnam = vec![
                self.actor_strength,
                self.actor_perception,
                self.actor_endurance,
                self.actor_charisma,
                self.actor_intelligence,
                self.actor_agility,
                self.actor_luck,
            ];
            session.doc.set_sub(fid, b"DNAM", dnam);
        } else {
            let mut data = self
                .raw_subs
                .get(b"DATA")
                .cloned()
                .unwrap_or_else(|| vec![0u8; 17]);
            if data.len() < 17 {
                data.resize(17, 0);
            }
            data[4..6].copy_from_slice(&self.actor_health.to_le_bytes());
            data[8..10].copy_from_slice(&self.actor_damage.to_le_bytes());
            data[10] = self.actor_strength;
            data[11] = self.actor_perception;
            data[12] = self.actor_endurance;
            data[13] = self.actor_charisma;
            data[14] = self.actor_intelligence;
            data[15] = self.actor_agility;
            data[16] = self.actor_luck;
            session.doc.set_sub(fid, b"DATA", data);
        }

        if let Some(r) = self.actor_race {
            session.doc.set_sub(fid, b"RNAM", r.to_le_bytes().to_vec());
        }
        if let Some(c) = self.actor_class {
            session.doc.set_sub(fid, b"CNAM", c.to_le_bytes().to_vec());
        }
        if let Some(v) = self.actor_voice {
            session.doc.set_sub(fid, b"VTCK", v.to_le_bytes().to_vec());
        }
        if let Some(t) = self.actor_template {
            session.doc.set_sub(fid, b"TPLT", t.to_le_bytes().to_vec());
        }
        if let Some(cs) = self.actor_combat_style {
            session.doc.set_sub(fid, b"ZNAM", cs.to_le_bytes().to_vec());
        }
    }

    fn save_alch(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"DATA", self.alch_weight.to_le_bytes().to_vec());

        let mut enit = self
            .raw_subs
            .get(b"ENIT")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 16]);
        if enit.len() < 16 {
            enit.resize(16, 0);
        }
        enit[0..4].copy_from_slice(&self.alch_val.to_le_bytes());
        let mut flags = 0u8;
        if self.alch_is_food {
            flags |= 2;
        }
        if self.alch_is_medicine {
            flags |= 4;
        }
        if self.alch_is_poison {
            flags |= 8;
        }
        enit[4] = flags;
        enit[8..12].copy_from_slice(&self.alch_addict_chance.to_le_bytes());
        if let Some(add) = self.alch_addict {
            enit[12..16].copy_from_slice(&add.to_le_bytes());
        }
        session.doc.set_sub(fid, b"ENIT", enit);
    }

    fn save_ammo(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 13]);
        if d.len() < 13 {
            d.resize(13, 0);
        }
        d[0..4].copy_from_slice(&self.ammo_speed.to_le_bytes());
        d[8..12].copy_from_slice(&self.ammo_val.to_le_bytes());
        d[12] = self.ammo_clip_rounds as u8;
        session.doc.set_sub(fid, b"DATA", d);

        let mut dat2 = self
            .raw_subs
            .get(b"DAT2")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 12]);
        if dat2.len() < 12 {
            dat2.resize(12, 0);
        }
        dat2[8..12].copy_from_slice(&self.ammo_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DAT2", dat2);

        if let Some(c) = self.ammo_casing {
            session.doc.set_sub(fid, b"QNAM", c.to_le_bytes().to_vec());
        }
    }

    fn save_cont(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 5];
        d[0] = if self.cont_respawn { 2 } else { 0 };
        d[1..5].copy_from_slice(&self.cont_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
        if let Some(s) = self.cont_sound_open {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
        }
        if let Some(s) = self.cont_sound_close {
            session.doc.set_sub(fid, b"QNAM", s.to_le_bytes().to_vec());
        }
    }

    fn save_door(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut f = 0u8;
        if self.door_auto {
            f |= 1;
        }
        if self.door_hidden {
            f |= 2;
        }
        if self.door_minimal {
            f |= 4;
        }
        if self.door_sliding {
            f |= 8;
        }
        session.doc.set_sub(fid, b"FNAM", vec![f]);
    }

    fn save_acti(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if !self.acti_prompt.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"RNAM", encode_zstring(&self.acti_prompt));
        }
    }

    fn save_term(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"DNAM", vec![self.term_difficulty]);
        if !self.term_welcome.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"WNAM", encode_zstring(&self.term_welcome));
        }
    }

    fn save_ligh(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 32]);
        if d.len() < 32 {
            d.resize(32, 0);
        }
        d[4..8].copy_from_slice(&self.ligh_radius.to_le_bytes());
        d[8] = self.ligh_color[0];
        d[9] = self.ligh_color[1];
        d[10] = self.ligh_color[2];
        d[12..16].copy_from_slice(&self.ligh_flags.to_le_bytes());
        d[16..20].copy_from_slice(&self.ligh_falloff.to_le_bytes());
        d[20..24].copy_from_slice(&self.ligh_fov.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_book(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 10];
        d[1] = self.book_teaches;
        d[2..6].copy_from_slice(&self.item_val.to_le_bytes());
        d[6..10].copy_from_slice(&self.item_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
        if !self.book_text.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.book_text));
        }
    }

    fn save_misc(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 8];
        d[0..4].copy_from_slice(&self.item_val.to_le_bytes());
        d[4..8].copy_from_slice(&self.item_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_fact(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut f1 = 0u8;
        if self.fact_hidden {
            f1 |= 1;
        }
        if self.fact_evil {
            f1 |= 2;
        }
        if self.fact_special_combat {
            f1 |= 4;
        }
        let mut f2 = 0u8;
        if self.fact_track_crime {
            f2 |= 1;
        }
        session.doc.set_sub(fid, b"DATA", vec![f1, f2, 0, 0]);
        if let Some(rep) = self.fact_reputation {
            session
                .doc
                .set_sub(fid, b"WMI1", rep.to_le_bytes().to_vec());
        }
        if !self.fact_rank_male.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MNAM", encode_zstring(&self.fact_rank_male));
        }
        if !self.fact_rank_female.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FNAM", encode_zstring(&self.fact_rank_female));
        }
    }

    fn save_qust(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 8];
        let mut f = 0u8;
        if self.qust_start_enabled {
            f |= 1;
        }
        if self.qust_repeated_stages {
            f |= 8;
        }
        d[0] = f;
        d[1] = self.qust_priority;
        d[4..8].copy_from_slice(&self.qust_delay.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_perk(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let d = vec![
            if self.perk_trait { 1 } else { 0 },
            self.perk_min_level,
            self.perk_num_ranks,
        ];
        session.doc.set_sub(fid, b"DATA", d);
        if !self.perk_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.perk_desc));
        }
    }

    fn save_spell(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut spit = self
            .raw_subs
            .get(b"SPIT")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 16]);
        if spit.len() < 16 {
            spit.resize(16, 0);
        }
        spit[0..4].copy_from_slice(&self.spell_type.to_le_bytes());
        let mut f = spit[12];
        if self.spell_no_autocalc {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.spell_always_applies {
            f |= 32;
        } else {
            f &= !32;
        }
        spit[12] = f;
        session.doc.set_sub(fid, b"SPIT", spit);
    }

    fn save_expl(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 48]);
        if d.len() < 48 {
            d.resize(48, 0);
        }
        d[0..4].copy_from_slice(&self.expl_force.to_le_bytes());
        d[4..8].copy_from_slice(&self.expl_damage.to_le_bytes());
        d[8..12].copy_from_slice(&self.expl_radius.to_le_bytes());
        if let Some(l) = self.expl_light {
            d[12..16].copy_from_slice(&l.to_le_bytes());
        }
        if let Some(s) = self.expl_sound1 {
            d[16..20].copy_from_slice(&s.to_le_bytes());
        }
        d[24..28].copy_from_slice(&self.expl_is_radius.to_le_bytes());
        if let Some(s) = self.expl_sound2 {
            d[32..36].copy_from_slice(&s.to_le_bytes());
        }
        d[36..40].copy_from_slice(&self.expl_rad_level.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_proj(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 44]);
        if d.len() < 44 {
            d.resize(44, 0);
        }
        d[2..4].copy_from_slice(&self.proj_type.to_le_bytes());
        d[4..8].copy_from_slice(&self.proj_gravity.to_le_bytes());
        d[8..12].copy_from_slice(&self.proj_speed.to_le_bytes());
        d[12..16].copy_from_slice(&self.proj_range.to_le_bytes());
        if let Some(l) = self.proj_light {
            d[16..20].copy_from_slice(&l.to_le_bytes());
        }
        if let Some(ex) = self.proj_explosion {
            d[36..40].copy_from_slice(&ex.to_le_bytes());
        }
        if let Some(s) = self.proj_sound {
            d[40..44].copy_from_slice(&s.to_le_bytes());
        }
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_soun(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if !self.sound_file.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FNAM", encode_zstring(&self.sound_file));
        }
        session
            .doc
            .set_sub(fid, b"RNAM", vec![self.sound_random_chance]);
        let mut sndd = self
            .raw_subs
            .get(b"SNDD")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 10]);
        if sndd.len() < 10 {
            sndd.resize(10, 0);
        }
        sndd[0] = self.sound_min_dist;
        sndd[1] = self.sound_max_dist;
        sndd[2] = self.sound_freq_adj as u8;
        let mut flags = u32::from_le_bytes([sndd[4], sndd[5], sndd[6], sndd[7]]);
        if self.sound_loop {
            flags |= 1 << 4;
        } else {
            flags &= !(1 << 4);
        }
        if self.sound_2d {
            flags |= 1 << 6;
        } else {
            flags &= !(1 << 6);
        }
        sndd[4..8].copy_from_slice(&flags.to_le_bytes());
        sndd[8..10].copy_from_slice(&self.sound_vol_cdb.to_le_bytes());
        session.doc.set_sub(fid, b"SNDD", sndd);
    }

    fn save_note(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session.doc.set_sub(fid, b"DATA", vec![self.note_type]);
        if !self.note_text.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.note_text));
        }
        if let Some(s) = self.note_sound {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
        }
    }

    fn save_lvl(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"LVLD", vec![self.lvl_chance_none]);
        let mut f = 0u8;
        if self.lvl_all_levels {
            f |= 1;
        }
        if self.lvl_each_item {
            f |= 2;
        }
        if self.lvl_use_all {
            f |= 4;
        }
        session.doc.set_sub(fid, b"LVLF", vec![f]);
        if let Some(g) = self.lvl_global {
            session.doc.set_sub(fid, b"LVLG", g.to_le_bytes().to_vec());
        }
    }

    fn save_imod(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 8];
        d[0..4].copy_from_slice(&self.imod_val.to_le_bytes());
        d[4..8].copy_from_slice(&self.imod_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
        if !self.imod_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.imod_desc));
        }
    }

    fn save_clas(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 26]);
        if d.len() < 26 {
            d.resize(26, 0);
        }
        let mut f = u32::from_le_bytes([d[16], d[17], d[18], d[19]]);
        if self.clas_playable {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.clas_guard {
            f |= 2;
        } else {
            f &= !2;
        }
        d[16..20].copy_from_slice(&f.to_le_bytes());
        d[24] = self.clas_teaches;
        d[25] = self.clas_max_train;
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_csty(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut cstd = self
            .raw_subs
            .get(b"CSTD")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 28]);
        if cstd.len() < 28 {
            cstd.resize(28, 0);
        }
        cstd[0] = self.csty_dodge_chance;
        cstd[1] = self.csty_lr_chance;
        cstd[24] = self.csty_power_chance;
        session.doc.set_sub(fid, b"CSTD", cstd);

        let mut cssd = self
            .raw_subs
            .get(b"CSSD")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 8]);
        if cssd.len() < 8 {
            cssd.resize(8, 0);
        }
        cssd[0..4].copy_from_slice(&self.csty_cover_radius.to_le_bytes());
        cssd[4..8].copy_from_slice(&self.csty_take_cover_chance.to_le_bytes());
        session.doc.set_sub(fid, b"CSSD", cssd);
    }

    fn save_mgef(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 16]);
        if d.len() < 16 {
            d.resize(16, 0);
        }
        let mut f = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
        if self.mgef_hostile {
            f |= 1 << 0;
        } else {
            f &= !(1 << 0);
        }
        if self.mgef_recover {
            f |= 1 << 1;
        } else {
            f &= !(1 << 1);
        }
        if self.mgef_detrimental {
            f |= 1 << 2;
        } else {
            f &= !(1 << 2);
        }
        if self.mgef_self {
            f |= 1 << 4;
        } else {
            f &= !(1 << 4);
        }
        if self.mgef_touch {
            f |= 1 << 5;
        } else {
            f &= !(1 << 5);
        }
        if self.mgef_target {
            f |= 1 << 6;
        } else {
            f &= !(1 << 6);
        }
        d[0..4].copy_from_slice(&f.to_le_bytes());
        d[4..8].copy_from_slice(&self.mgef_base_cost.to_le_bytes());
        if let Some(it) = self.mgef_assoc_item {
            d[8..12].copy_from_slice(&it.to_le_bytes());
        }
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_wthr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 8]);
        if d.len() < 8 {
            d.resize(8, 0);
        }
        d[0] = self.wthr_wind_speed;
        d[4] = self.wthr_sun_glare;
        d[5] = self.wthr_sun_damage;
        session.doc.set_sub(fid, b"DATA", d);
        if !self.wthr_cloud_layer0.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DNAM", encode_zstring(&self.wthr_cloud_layer0));
        }
    }

    fn save_watr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session.doc.set_sub(fid, b"ANAM", vec![self.watr_opacity]);
        let mut f = 0u8;
        if self.watr_causes_damage {
            f |= 1;
        }
        if self.watr_reflective {
            f |= 2;
        }
        session.doc.set_sub(fid, b"FNAM", vec![f]);
        if let Some(x) = self.watr_effect {
            session.doc.set_sub(fid, b"XNAM", x.to_le_bytes().to_vec());
        }
        if let Some(s) = self.watr_sound {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
        }
    }

    fn save_glob(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session.doc.set_sub(fid, b"FNAM", vec![self.glob_type]);
        session
            .doc
            .set_sub(fid, b"FLTV", self.glob_val.to_le_bytes().to_vec());
    }

    fn save_gmst(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let data = if self.edid.starts_with('s') || self.edid.starts_with('S') {
            encode_zstring(&self.gmst_val_str)
        } else if self.edid.starts_with('i') || self.edid.starts_with('b') {
            (self.gmst_val_num as i32).to_le_bytes().to_vec()
        } else {
            self.gmst_val_num.to_le_bytes().to_vec()
        };
        session.doc.set_sub(fid, b"DATA", data);
    }

    fn save_race(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 36]);
        if d.len() < 36 {
            d.resize(36, 0);
        }
        d[16..20].copy_from_slice(&self.race_male_height.to_le_bytes());
        d[20..24].copy_from_slice(&self.race_female_height.to_le_bytes());
        d[24..28].copy_from_slice(&self.race_male_weight.to_le_bytes());
        d[28..32].copy_from_slice(&self.race_female_weight.to_le_bytes());
        let mut f = u32::from_le_bytes([d[32], d[33], d[34], d[35]]);
        if self.race_playable {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.race_child {
            f |= 4;
        } else {
            f &= !4;
        }
        d[32..36].copy_from_slice(&f.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
        if !self.race_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.race_desc));
        }
    }

    fn save_clmt(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let tnam = vec![
            self.clmt_sunrise_begin,
            self.clmt_sunrise_end,
            self.clmt_sunset_begin,
            self.clmt_sunset_end,
            self.clmt_volatility,
            1, // Moon phase length
        ];
        session.doc.set_sub(fid, b"TNAM", tnam);
        if !self.clmt_sun_texture.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FNAM", encode_zstring(&self.clmt_sun_texture));
        }
        if !self.clmt_sun_glare_texture.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"GNAM", encode_zstring(&self.clmt_sun_glare_texture));
        }
    }
}

fn encode_zstring(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if !v.ends_with(&[0]) {
        v.push(0);
    }
    v
}

/// Dynamically spawn or despawn the Object Editor window.
pub fn manage_window(
    mut commands: Commands,
    session: Res<crate::session::Session>,
    mut win_query: Query<(Entity, &mut Window), With<ObjectEditorWindow>>,
    cam_query: Query<Entity, With<ObjectEditorCamera>>,
    state: Res<ObjectEditorState>,
) {
    let win_opt = win_query.iter_mut().next();
    let cam_opt = cam_query.iter().next();

    if session.editing_object.is_some() {
        if let Some((_, mut win)) = win_opt {
            let expected_title = if !state.full_name.trim().is_empty() {
                format!(
                    "{} - {} ({})",
                    fnv_util::tag_str(&state.sig),
                    state.edid,
                    state.full_name
                )
            } else {
                format!("{} - {}", fnv_util::tag_str(&state.sig), state.edid)
            };
            if win.title != expected_title {
                win.title = expected_title;
            }
        } else {
            let title = if !state.full_name.trim().is_empty() {
                format!(
                    "{} - {} ({})",
                    fnv_util::tag_str(&state.sig),
                    state.edid,
                    state.full_name
                )
            } else {
                format!("{} - {}", fnv_util::tag_str(&state.sig), state.edid)
            };

            let window = commands
                .spawn((
                    Window {
                        title,
                        resolution: (880, 780).into(),
                        position: WindowPosition::new(IVec2::new(480, 50)),
                        ..default()
                    },
                    ObjectEditorWindow,
                ))
                .id();

            commands.spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_2d()),
                bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
                bevy_egui::EguiSchedule::new(ObjectEditorSchedule),
                crate::windows::PanelContext::<6>,
                ObjectEditorCamera,
            ));
        }
    } else if let Some((win_ent, _)) = win_opt {
        commands.entity(win_ent).despawn();
        if let Some(cam_ent) = cam_opt {
            commands.entity(cam_ent).despawn();
        }
    }
}

const SKILL_NAMES: [&str; 14] = [
    "None",
    "Guns",
    "Energy Weapons",
    "Melee Weapons",
    "Unarmed",
    "Explosives",
    "Lockpick",
    "Science",
    "Repair",
    "Sneak",
    "Medicine",
    "Barter",
    "Speech",
    "Survival",
];
const ARMOR_TYPES: [&str; 5] = ["Light", "Medium", "Heavy", "Power Armor", "Non-Playable"];
const DIFFICULTY_LEVELS: [&str; 6] = [
    "Very Easy",
    "Easy",
    "Average",
    "Hard",
    "Very Hard",
    "Requires Key",
];

/// Draw the Object Editor UI.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<6>>>,
    mut session: ResMut<crate::session::Session>,
    mut state: ResMut<ObjectEditorState>,
) {
    if session.editing_object.is_none() {
        return;
    }

    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    if let Some((fid, sig)) = session.editing_object {
        if state.loaded_target != Some((fid, sig)) {
            state.load_from_session(&session, fid, sig);
        }
    }

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "object_editor"), |ui| {
        // Top Header
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Type: {}", fnv_util::tag_str(&state.sig)));
                ui.separator();
                ui.label(format!("FormID: {:08X}", state.form_id));
                ui.separator();
                ui.label("Editor ID:");
                ui.add(egui::TextEdit::singleline(&mut state.edid).desired_width(180.0));
                ui.separator();
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut state.full_name).desired_width(180.0));
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Model Path (.nif):");
                ui.add(
                    egui::TextEdit::singleline(&mut state.model)
                        .desired_width(ui.available_width() - 140.0),
                );
                ui.separator();
                ui.label("Script:");
                formid_field(ui, "obj_script", &mut state.script);
            });
        });

        ui.add_space(6.0);

        // Body based on record type
        let avail_h = (ui.available_height() - 48.0).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("obj_scroll")
            .max_height(avail_h)
            .show(ui, |ui| match &state.sig {
                b"WEAP" => draw_weap_ui(ui, &mut state),
                b"ARMO" => draw_armo_ui(ui, &mut state),
                b"NPC_" | b"CREA" => draw_actor_ui(ui, &mut state),
                b"ALCH" => draw_alch_ui(ui, &mut state),
                b"AMMO" => draw_ammo_ui(ui, &mut state),
                b"CONT" => draw_cont_ui(ui, &mut state),
                b"DOOR" => draw_door_ui(ui, &mut state),
                b"ACTI" => draw_acti_ui(ui, &mut state),
                b"TERM" => draw_term_ui(ui, &mut state),
                b"LIGH" => draw_ligh_ui(ui, &mut state),
                b"BOOK" => draw_book_ui(ui, &mut state),
                b"FACT" => draw_fact_ui(ui, &mut state),
                b"QUST" => draw_qust_ui(ui, &mut state),
                b"PERK" => draw_perk_ui(ui, &mut state),
                b"SPEL" | b"ENCH" => draw_spell_ui(ui, &mut state),
                b"EXPL" => draw_expl_ui(ui, &mut state),
                b"PROJ" => draw_proj_ui(ui, &mut state),
                b"SOUN" => draw_soun_ui(ui, &mut state),
                b"NOTE" => draw_note_ui(ui, &mut state),
                b"LVLI" | b"LVLN" | b"LVLC" => draw_lvl_ui(ui, &mut state),
                b"IMOD" => draw_imod_ui(ui, &mut state),
                b"CLAS" => draw_clas_ui(ui, &mut state),
                b"CSTY" => draw_csty_ui(ui, &mut state),
                b"MGEF" => draw_mgef_ui(ui, &mut state),
                b"WTHR" => draw_wthr_ui(ui, &mut state),
                b"WATR" => draw_watr_ui(ui, &mut state),
                b"GLOB" => draw_glob_ui(ui, &mut state),
                b"GMST" => draw_gmst_ui(ui, &mut state),
                b"RACE" => draw_race_ui(ui, &mut state),
                b"CLMT" => draw_clmt_ui(ui, &mut state),
                _ => draw_misc_ui(ui, &mut state),
            });

        ui.separator();

        // Footer Actions
        let mut do_revert = false;
        ui.horizontal(|ui| {
            if ui
                .add_sized([160.0, 28.0], egui::Button::new("Apply Changes to Mod"))
                .clicked()
            {
                state.save_to_session(&mut session);
                state.status = format!("Changes queued in active mod for {:08X}.", state.form_id);
            }

            if ui
                .add_sized([100.0, 28.0], egui::Button::new("Revert"))
                .clicked()
            {
                do_revert = true;
            }

            if ui
                .add_sized([100.0, 28.0], egui::Button::new("Close"))
                .clicked()
            {
                session.close_object_editor();
            }

            ui.add_space(8.0);
            ui.label(&state.status);
        });

        if do_revert {
            if let Some((fid, sig)) = session.editing_object {
                state.load_from_session(&session, fid, sig);
                state.status = "Reverted changes to disk/mod state.".to_string();
            }
        }
    });
}

fn formid_field(ui: &mut egui::Ui, id: &str, opt: &mut Option<u32>) {
    let mut text = opt.map(|f| format!("{:08X}", f)).unwrap_or_default();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id_salt(id)
            .hint_text("00000000")
            .desired_width(80.0),
    );
    if resp.changed() {
        let clean = text.trim().trim_start_matches("0x");
        if clean.is_empty() {
            *opt = None;
        } else if let Ok(parsed) = u32::from_str_radix(clean, 16) {
            *opt = Some(parsed);
        }
    }
}

fn draw_weap_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Combat & Damage Parameters");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Damage:");
            ui.add(egui::DragValue::new(&mut state.weap_dmg).range(0..=1000));
            ui.label("Attack Speed (Rate):");
            ui.add(
                egui::DragValue::new(&mut state.weap_speed)
                    .speed(0.1)
                    .range(0.1..=50.0),
            );
            ui.label("Clip Size:");
            ui.add(egui::DragValue::new(&mut state.weap_clip).range(0..=255));
            ui.label("Num Projectiles:");
            ui.add(egui::DragValue::new(&mut state.weap_num_proj).range(1..=50));
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Ammo Type (FormID):");
            formid_field(ui, "weap_ammo", &mut state.weap_ammo);
            ui.separator();
            ui.label("Skill Type:");
            let cur_skill = (state.weap_skill as usize).min(SKILL_NAMES.len() - 1);
            let skill_lbl = SKILL_NAMES.get(cur_skill).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("weap_skill")
                .selected_text(skill_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in SKILL_NAMES.iter().take(6).enumerate() {
                        ui.selectable_value(&mut state.weap_skill, i as u8, name);
                    }
                });
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Requirements & Criticals");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("STR Req:");
            ui.add(egui::DragValue::new(&mut state.weap_str_req).range(1..=10));
            ui.label("Skill Req:");
            ui.add(egui::DragValue::new(&mut state.weap_skill_req).range(0..=100));
            ui.label("VATS AP Cost:");
            ui.add(
                egui::DragValue::new(&mut state.weap_ap)
                    .speed(0.5)
                    .range(0.0..=100.0),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Crit Damage:");
            ui.add(egui::DragValue::new(&mut state.weap_crit_dmg).range(0..=1000));
            ui.label("Crit % Mult:");
            ui.add(
                egui::DragValue::new(&mut state.weap_crit_mult)
                    .speed(0.1)
                    .range(0.0..=10.0),
            );
            ui.label("Crit Effect:");
            formid_field(ui, "weap_crit_eff", &mut state.weap_crit_effect);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.weap_val).range(0..=100000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.weap_weight)
                    .speed(0.1)
                    .range(0.0..=1000.0),
            );
            ui.label("Health (Max Condition):");
            ui.add(egui::DragValue::new(&mut state.weap_hp).range(1..=100000));
        });
    });
}

fn draw_armo_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Protection & Defense");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Damage Threshold (DT):");
            ui.add(
                egui::DragValue::new(&mut state.armo_dt)
                    .speed(0.5)
                    .range(0.0..=100.0),
            );
            ui.label("Damage Resistance (DR):");
            ui.add(egui::DragValue::new(&mut state.armo_dr).range(0..=100));
            ui.label("Armor Type:");
            let cur_type = (state.armo_type as usize).min(ARMOR_TYPES.len() - 1);
            let type_lbl = ARMOR_TYPES.get(cur_type).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("armo_type")
                .selected_text(type_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in ARMOR_TYPES.iter().enumerate() {
                        ui.selectable_value(&mut state.armo_type, i as u8, name);
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.armo_val).range(0..=100000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.armo_weight)
                    .speed(0.1)
                    .range(0.0..=1000.0),
            );
            ui.label("Health (Condition):");
            ui.add(egui::DragValue::new(&mut state.armo_hp).range(1..=100000));
            ui.checkbox(&mut state.armo_modulates_voice, "Modulates Voice");
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Enchantment / Object Effect (EITM):");
            formid_field(ui, "armo_ench", &mut state.armo_enchant);
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Biped Equipment Slots");
        ui.add_space(4.0);
        let slots: [(u8, &str); 12] = [
            (0, "Head"),
            (1, "Hair"),
            (2, "Upper Body"),
            (3, "Left Hand"),
            (4, "Right Hand"),
            (7, "Backpack"),
            (8, "Necklace"),
            (9, "Headband"),
            (10, "Hat"),
            (11, "Eye Glasses"),
            (14, "Mask"),
            (15, "Choker"),
        ];
        ui.columns(3, |cols| {
            for (i, &(bit, name)) in slots.iter().enumerate() {
                let col = &mut cols[i % 3];
                let mut set = (state.armo_biped_flags & (1 << bit)) != 0;
                if col.checkbox(&mut set, name).changed() {
                    if set {
                        state.armo_biped_flags |= 1 << bit;
                    } else {
                        state.armo_biped_flags &= !(1 << bit);
                    }
                }
            }
        });
    });
}

fn draw_actor_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Leveling & Vital Stats");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.actor_pc_mult, "PC Level Mult");
            if state.actor_pc_mult {
                ui.label("Mult x1000:");
                ui.add(egui::DragValue::new(&mut state.actor_level_or_mult).range(100..=10000));
            } else {
                ui.label("Level:");
                ui.add(egui::DragValue::new(&mut state.actor_level_or_mult).range(1..=100));
            }
            ui.label("Calc Min:");
            ui.add(egui::DragValue::new(&mut state.actor_calc_min).range(0..=100));
            ui.label("Calc Max:");
            ui.add(egui::DragValue::new(&mut state.actor_calc_max).range(0..=100));
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Health (HP):");
            ui.add(egui::DragValue::new(&mut state.actor_health).range(1..=50000));
            ui.label("Fatigue:");
            ui.add(egui::DragValue::new(&mut state.actor_fatigue).range(0..=50000));
            ui.label("Speed Mult %:");
            ui.add(egui::DragValue::new(&mut state.actor_speed_mult).range(10..=1000));
            ui.label("Karma:");
            ui.add(
                egui::DragValue::new(&mut state.actor_karma)
                    .speed(10.0)
                    .range(-1000.0..=1000.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("S.P.E.C.I.A.L. Attributes");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("STR:");
            ui.add(egui::DragValue::new(&mut state.actor_strength).range(1..=10));
            ui.label("PER:");
            ui.add(egui::DragValue::new(&mut state.actor_perception).range(1..=10));
            ui.label("END:");
            ui.add(egui::DragValue::new(&mut state.actor_endurance).range(1..=10));
            ui.label("CHA:");
            ui.add(egui::DragValue::new(&mut state.actor_charisma).range(1..=10));
            ui.label("INT:");
            ui.add(egui::DragValue::new(&mut state.actor_intelligence).range(1..=10));
            ui.label("AGI:");
            ui.add(egui::DragValue::new(&mut state.actor_agility).range(1..=10));
            ui.label("LCK:");
            ui.add(egui::DragValue::new(&mut state.actor_luck).range(1..=10));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Flags & AI Behavior");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.actor_female, "Female");
            ui.checkbox(&mut state.actor_essential, "Essential");
            ui.checkbox(&mut state.actor_respawn, "Respawn");
            ui.separator();
            ui.label("Aggression (0-3):");
            ui.add(egui::DragValue::new(&mut state.actor_aggression).range(0..=3));
            ui.label("Confidence (0-4):");
            ui.add(egui::DragValue::new(&mut state.actor_confidence).range(0..=4));
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Voice Type:");
            formid_field(ui, "act_voice", &mut state.actor_voice);
            ui.label("Combat Style:");
            formid_field(ui, "act_cstyle", &mut state.actor_combat_style);
            ui.label("Template:");
            formid_field(ui, "act_tplt", &mut state.actor_template);
        });
    });
}

fn draw_alch_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Consumable / Chem Stats");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.alch_val).range(0..=100000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.alch_weight)
                    .speed(0.1)
                    .range(0.0..=100.0),
            );
            ui.label("Addiction Chance %:");
            ui.add(
                egui::DragValue::new(&mut state.alch_addict_chance)
                    .speed(0.01)
                    .range(0.0..=1.0),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.alch_is_food, "Food Item");
            ui.checkbox(&mut state.alch_is_medicine, "Medicine");
            ui.checkbox(&mut state.alch_is_poison, "Poison");
        });
    });
}

fn draw_ammo_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Ammunition Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.ammo_val).range(0..=10000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.ammo_weight)
                    .speed(0.005)
                    .range(0.0..=10.0),
            );
            ui.label("Projectile Speed:");
            ui.add(
                egui::DragValue::new(&mut state.ammo_speed)
                    .speed(50.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Damage Mult:");
            ui.add(
                egui::DragValue::new(&mut state.ammo_dmg_mult)
                    .speed(0.05)
                    .range(0.0..=10.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Casing Item (QNAM FormID):");
            formid_field(ui, "ammo_casing", &mut state.ammo_casing);
        });
    });
}

fn draw_cont_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Container Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Max Weight Capacity (0 = unlimited):");
            ui.add(
                egui::DragValue::new(&mut state.cont_weight)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
            ui.checkbox(&mut state.cont_respawn, "Respawns Contents");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Open Sound (FormID):");
            formid_field(ui, "cont_open", &mut state.cont_sound_open);
            ui.label("Close Sound (FormID):");
            formid_field(ui, "cont_close", &mut state.cont_sound_close);
        });
    });
}

fn draw_door_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Door Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.door_auto, "Automatic");
            ui.checkbox(&mut state.door_hidden, "Hidden");
            ui.checkbox(&mut state.door_minimal, "Minimal Use");
            ui.checkbox(&mut state.door_sliding, "Sliding");
        });
    });
}

fn draw_acti_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Activator Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Activate Text Prompt (RNAM):");
            ui.add(egui::TextEdit::singleline(&mut state.acti_prompt).desired_width(200.0));
        });
    });
}

fn draw_term_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Terminal Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Hack Difficulty:");
            let cur_diff = (state.term_difficulty as usize).min(DIFFICULTY_LEVELS.len() - 1);
            let diff_lbl = DIFFICULTY_LEVELS
                .get(cur_diff)
                .copied()
                .unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("term_diff")
                .selected_text(diff_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in DIFFICULTY_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut state.term_difficulty, i as u8, name);
                    }
                });
        });
        ui.add_space(4.0);
        ui.label("Welcome Text (WNAM):");
        ui.add(
            egui::TextEdit::multiline(&mut state.term_welcome)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

fn draw_ligh_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Light Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Radius:");
            ui.add(
                egui::DragValue::new(&mut state.ligh_radius)
                    .speed(10.0)
                    .range(0..=100000),
            );
            ui.label("Light Color (RGB):");
            ui.color_edit_button_srgb(&mut state.ligh_color);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Falloff Exponent:");
            ui.add(
                egui::DragValue::new(&mut state.ligh_falloff)
                    .speed(0.1)
                    .range(0.0..=10.0),
            );
            ui.label("FOV Angle:");
            ui.add(
                egui::DragValue::new(&mut state.ligh_fov)
                    .speed(1.0)
                    .range(0.0..=180.0),
            );
        });
    });
}

fn draw_book_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Book & Magazine Stats");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.item_val).range(0..=10000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.item_weight)
                    .speed(0.1)
                    .range(0.0..=100.0),
            );
            ui.label("Teaches Skill:");
            let cur_skill = (state.book_teaches as usize).min(SKILL_NAMES.len() - 1);
            let skill_lbl = SKILL_NAMES.get(cur_skill).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("book_skill")
                .selected_text(skill_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in SKILL_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.book_teaches, i as u8, name);
                    }
                });
        });
        ui.add_space(4.0);
        ui.label("Book Content Text (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.book_text)
                .desired_rows(6)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

fn draw_misc_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Item Value & Weight");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.item_val).range(0..=100000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.item_weight)
                    .speed(0.1)
                    .range(0.0..=1000.0),
            );
        });
    });
}

fn draw_fact_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Faction Flags & Behavior");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.fact_hidden, "Hidden from PC");
            ui.checkbox(&mut state.fact_evil, "Evil");
            ui.checkbox(&mut state.fact_special_combat, "Special Combat");
            ui.checkbox(&mut state.fact_track_crime, "Track Crime");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Reputation (FormID):");
            formid_field(ui, "fact_rep", &mut state.fact_reputation);
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Rank Titles");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Male Rank Title:");
            ui.add(egui::TextEdit::singleline(&mut state.fact_rank_male).desired_width(180.0));
            ui.label("Female Rank Title:");
            ui.add(egui::TextEdit::singleline(&mut state.fact_rank_female).desired_width(180.0));
        });
    });
}

fn draw_qust_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Quest Flags & Scheduling");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.qust_start_enabled, "Start Game Enabled");
            ui.checkbox(&mut state.qust_repeated_stages, "Allow Repeated Stages");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Priority:");
            ui.add(egui::DragValue::new(&mut state.qust_priority).range(0..=100));
            ui.label("Script Processing Delay (s):");
            ui.add(
                egui::DragValue::new(&mut state.qust_delay)
                    .speed(0.1)
                    .range(0.0..=60.0),
            );
        });
    });
}

fn draw_perk_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Perk Requirements & Rank");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.perk_trait, "Is Trait");
            ui.label("Min Level:");
            ui.add(egui::DragValue::new(&mut state.perk_min_level).range(1..=100));
            ui.label("Num Ranks:");
            ui.add(egui::DragValue::new(&mut state.perk_num_ranks).range(1..=10));
        });
        ui.add_space(4.0);
        ui.label("Perk Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.perk_desc)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

const SPELL_TYPES: [(u32, &str); 7] = [
    (0, "Actor Effect"),
    (1, "Disease"),
    (2, "Power"),
    (3, "Lesser Power"),
    (4, "Ability"),
    (5, "Poison"),
    (10, "Addiction"),
];

fn draw_spell_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Spell / Effect Settings");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            let cur_lbl = SPELL_TYPES
                .iter()
                .find(|(val, _)| *val == state.spell_type)
                .map(|(_, name)| *name)
                .unwrap_or("Custom");
            egui::ComboBox::from_id_salt("spell_type")
                .selected_text(cur_lbl)
                .show_ui(ui, |ui| {
                    for &(val, name) in &SPELL_TYPES {
                        ui.selectable_value(&mut state.spell_type, val, name);
                    }
                });
            ui.checkbox(&mut state.spell_no_autocalc, "No Auto-Calc");
            ui.checkbox(&mut state.spell_always_applies, "Always Applies");
        });
    });
}

fn draw_expl_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Explosion Parameters");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Damage:");
            ui.add(
                egui::DragValue::new(&mut state.expl_damage)
                    .speed(5.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Force:");
            ui.add(
                egui::DragValue::new(&mut state.expl_force)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Radius:");
            ui.add(
                egui::DragValue::new(&mut state.expl_radius)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Light (FormID):");
            formid_field(ui, "expl_light", &mut state.expl_light);
            ui.label("Sound 1:");
            formid_field(ui, "expl_snd1", &mut state.expl_sound1);
            ui.label("Sound 2:");
            formid_field(ui, "expl_snd2", &mut state.expl_sound2);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Radiation Level:");
            ui.add(
                egui::DragValue::new(&mut state.expl_rad_level)
                    .speed(1.0)
                    .range(0.0..=10000.0),
            );
            ui.label("IS Radius:");
            ui.add(
                egui::DragValue::new(&mut state.expl_is_radius)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
        });
    });
}

const PROJ_TYPES: [(u16, &str); 5] = [
    (1, "Missile"),
    (2, "Lobber"),
    (4, "Beam"),
    (8, "Flame"),
    (16, "Continuous Beam"),
];

fn draw_proj_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Projectile Ballistics & Appearance");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            let cur_lbl = PROJ_TYPES
                .iter()
                .find(|(val, _)| *val == state.proj_type)
                .map(|(_, name)| *name)
                .unwrap_or("Custom");
            egui::ComboBox::from_id_salt("proj_type")
                .selected_text(cur_lbl)
                .show_ui(ui, |ui| {
                    for &(val, name) in &PROJ_TYPES {
                        ui.selectable_value(&mut state.proj_type, val, name);
                    }
                });
            ui.label("Speed:");
            ui.add(
                egui::DragValue::new(&mut state.proj_speed)
                    .speed(50.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Gravity:");
            ui.add(
                egui::DragValue::new(&mut state.proj_gravity)
                    .speed(0.1)
                    .range(0.0..=100.0),
            );
            ui.label("Range:");
            ui.add(
                egui::DragValue::new(&mut state.proj_range)
                    .speed(50.0)
                    .range(0.0..=100000.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Light:");
            formid_field(ui, "proj_light", &mut state.proj_light);
            ui.label("Sound:");
            formid_field(ui, "proj_snd", &mut state.proj_sound);
            ui.label("Explosion:");
            formid_field(ui, "proj_expl", &mut state.proj_explosion);
        });
    });
}

fn draw_soun_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Sound Marker Settings");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Sound File Path:");
            ui.add(
                egui::TextEdit::singleline(&mut state.sound_file)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Chance %:");
            ui.add(egui::DragValue::new(&mut state.sound_random_chance).range(0..=100));
            ui.label("Min Distance:");
            ui.add(egui::DragValue::new(&mut state.sound_min_dist));
            ui.label("Max Distance:");
            ui.add(egui::DragValue::new(&mut state.sound_max_dist));
            ui.label("Volume (cdB):");
            ui.add(
                egui::DragValue::new(&mut state.sound_vol_cdb)
                    .speed(10.0)
                    .range(-10000..=0),
            );
            ui.checkbox(&mut state.sound_loop, "Loop");
            ui.checkbox(&mut state.sound_2d, "2D");
        });
    });
}

const NOTE_TYPES: [(u8, &str); 4] = [(0, "Sound"), (1, "Text"), (2, "Image"), (3, "Voice")];

fn draw_note_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Note Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            let cur_lbl = NOTE_TYPES
                .iter()
                .find(|(val, _)| *val == state.note_type)
                .map(|(_, name)| *name)
                .unwrap_or("Custom");
            egui::ComboBox::from_id_salt("note_type")
                .selected_text(cur_lbl)
                .show_ui(ui, |ui| {
                    for &(val, name) in &NOTE_TYPES {
                        ui.selectable_value(&mut state.note_type, val, name);
                    }
                });
            ui.label("Sound / Voice (FormID):");
            formid_field(ui, "note_sound", &mut state.note_sound);
        });
        ui.add_space(4.0);
        ui.label("Note Text Content (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.note_text)
                .desired_rows(6)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

fn draw_lvl_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Leveled List Settings");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Chance None %:");
            ui.add(egui::DragValue::new(&mut state.lvl_chance_none).range(0..=100));
            ui.label("Global:");
            formid_field(ui, "lvl_glob", &mut state.lvl_global);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut state.lvl_all_levels,
                "Calculate from all levels <= player's level",
            );
            ui.checkbox(&mut state.lvl_each_item, "Calculate for each item in count");
            ui.checkbox(&mut state.lvl_use_all, "Use All");
        });
    });
}

fn draw_imod_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Weapon / Item Mod Stats");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (Caps):");
            ui.add(egui::DragValue::new(&mut state.imod_val).range(0..=100000));
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.imod_weight)
                    .speed(0.1)
                    .range(0.0..=100.0),
            );
        });
        ui.add_space(4.0);
        ui.label("Mod Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.imod_desc)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

fn draw_clas_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Class Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Teaches Skill:");
            let cur_skill = (state.clas_teaches as usize).min(SKILL_NAMES.len() - 1);
            let skill_lbl = SKILL_NAMES.get(cur_skill).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("clas_skill")
                .selected_text(skill_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in SKILL_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.clas_teaches, i as u8, name);
                    }
                });
            ui.label("Max Training Level:");
            ui.add(egui::DragValue::new(&mut state.clas_max_train).range(0..=100));
            ui.checkbox(&mut state.clas_playable, "Playable");
            ui.checkbox(&mut state.clas_guard, "Guard");
        });
    });
}

fn draw_csty_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Combat Style Behavior & Chances");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Dodge Chance %:");
            ui.add(egui::DragValue::new(&mut state.csty_dodge_chance).range(0..=100));
            ui.label("Left/Right Dodge %:");
            ui.add(egui::DragValue::new(&mut state.csty_lr_chance).range(0..=100));
            ui.label("Power Attack %:");
            ui.add(egui::DragValue::new(&mut state.csty_power_chance).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Cover Search Radius:");
            ui.add(
                egui::DragValue::new(&mut state.csty_cover_radius)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Take Cover Chance %:");
            ui.add(
                egui::DragValue::new(&mut state.csty_take_cover_chance)
                    .speed(0.05)
                    .range(0.0..=1.0),
            );
        });
    });
}

fn draw_mgef_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Magic Effect Flags & Cost");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Base Cost:");
            ui.add(
                egui::DragValue::new(&mut state.mgef_base_cost)
                    .speed(1.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Associated Item / Spell:");
            formid_field(ui, "mgef_assoc", &mut state.mgef_assoc_item);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.mgef_hostile, "Hostile");
            ui.checkbox(&mut state.mgef_recover, "Recover");
            ui.checkbox(&mut state.mgef_detrimental, "Detrimental");
            ui.checkbox(&mut state.mgef_self, "Self");
            ui.checkbox(&mut state.mgef_touch, "Touch");
            ui.checkbox(&mut state.mgef_target, "Target");
        });
    });
}

fn draw_wthr_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Weather Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Wind Speed:");
            ui.add(egui::DragValue::new(&mut state.wthr_wind_speed).range(0..=255));
            ui.label("Sun Glare %:");
            ui.add(egui::DragValue::new(&mut state.wthr_sun_glare).range(0..=255));
            ui.label("Sun Damage:");
            ui.add(egui::DragValue::new(&mut state.wthr_sun_damage).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Cloud Layer 0 Texture:");
            ui.add(
                egui::TextEdit::singleline(&mut state.wthr_cloud_layer0)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
    });
}

fn draw_watr_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Water Visuals & Radiation/Damage");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Opacity:");
            ui.add(egui::DragValue::new(&mut state.watr_opacity).range(0..=255));
            ui.checkbox(&mut state.watr_causes_damage, "Causes Damage");
            ui.checkbox(&mut state.watr_reflective, "Reflective");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Actor Effect (Rads/Dmg):");
            formid_field(ui, "watr_eff", &mut state.watr_effect);
            ui.label("Sound:");
            formid_field(ui, "watr_snd", &mut state.watr_sound);
        });
    });
}

const GLOB_TYPES: [(u8, &str); 3] = [
    (115, "Short (Integer 16)"),
    (108, "Long (Integer 32)"),
    (102, "Float"),
];

fn draw_glob_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Global Variable Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            let cur_lbl = GLOB_TYPES
                .iter()
                .find(|(val, _)| *val == state.glob_type)
                .map(|(_, name)| *name)
                .unwrap_or("Float");
            egui::ComboBox::from_id_salt("glob_type")
                .selected_text(cur_lbl)
                .show_ui(ui, |ui| {
                    for &(val, name) in &GLOB_TYPES {
                        ui.selectable_value(&mut state.glob_type, val, name);
                    }
                });
            ui.label("Current Value:");
            ui.add(egui::DragValue::new(&mut state.glob_val).speed(0.1));
        });
    });
}

fn draw_gmst_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Game Setting Value");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if state.edid.starts_with('s') || state.edid.starts_with('S') {
                ui.label("String Setting:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.gmst_val_str)
                        .desired_width(ui.available_width() - 20.0),
                );
            } else if state.edid.starts_with('i') || state.edid.starts_with('b') {
                ui.label("Integer / Flag Value:");
                ui.add(egui::DragValue::new(&mut state.gmst_val_num).speed(1.0));
            } else {
                ui.label("Float Setting:");
                ui.add(egui::DragValue::new(&mut state.gmst_val_num).speed(0.1));
            }
        });
    });
}

fn draw_race_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Race Physical Dimensions & Flags");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Male Height:");
            ui.add(
                egui::DragValue::new(&mut state.race_male_height)
                    .speed(0.01)
                    .range(0.1..=10.0),
            );
            ui.label("Female Height:");
            ui.add(
                egui::DragValue::new(&mut state.race_female_height)
                    .speed(0.01)
                    .range(0.1..=10.0),
            );
            ui.label("Male Weight:");
            ui.add(
                egui::DragValue::new(&mut state.race_male_weight)
                    .speed(0.01)
                    .range(0.1..=10.0),
            );
            ui.label("Female Weight:");
            ui.add(
                egui::DragValue::new(&mut state.race_female_weight)
                    .speed(0.01)
                    .range(0.1..=10.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.race_playable, "Playable");
            ui.checkbox(&mut state.race_child, "Child");
        });
        ui.add_space(4.0);
        ui.label("Race Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.race_desc)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

fn draw_clmt_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Climate Day/Night Cycle & Sky");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Sunrise Begin:");
            ui.add(egui::DragValue::new(&mut state.clmt_sunrise_begin).range(0..=24));
            ui.label("Sunrise End:");
            ui.add(egui::DragValue::new(&mut state.clmt_sunrise_end).range(0..=24));
            ui.label("Sunset Begin:");
            ui.add(egui::DragValue::new(&mut state.clmt_sunset_begin).range(0..=24));
            ui.label("Sunset End:");
            ui.add(egui::DragValue::new(&mut state.clmt_sunset_end).range(0..=24));
            ui.label("Volatility:");
            ui.add(egui::DragValue::new(&mut state.clmt_volatility).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Sun Texture:");
            ui.add(
                egui::TextEdit::singleline(&mut state.clmt_sun_texture)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Sun Glare Texture:");
            ui.add(
                egui::TextEdit::singleline(&mut state.clmt_sun_glare_texture)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
    });
}
