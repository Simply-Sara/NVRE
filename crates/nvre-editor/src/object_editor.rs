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
    pub record_flags: u32,
    pub quest_item: bool,

    // Object Bounds (OBND)
    pub obnd_min: [i16; 3],
    pub obnd_max: [i16; 3],

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
    pub weap_repair_list: Option<u32>,

    // ARMO fields
    pub armo_val: i32,
    pub armo_hp: i32,
    pub armo_weight: f32,
    pub armo_dt: f32,
    pub armo_dr: i16,
    pub armo_enchant: Option<u32>,
    pub armo_repair_list: Option<u32>,
    pub armo_type: u8, // 0: Light, 1: Medium, 2: Heavy, 3: Power, 4: Non-Playable
    pub armo_biped_flags: u32,
    pub armo_modulates_voice: bool,
    /// The length of the record's `DNAM` subrecord: 4 on the 21 ARMO records
    /// that carry only the `AR` int16 and two unused bytes, 12 on the other
    /// 368. The length is kept so a save reproduces the record's own bytes
    /// instead of growing a short `DNAM` into a long one.
    pub armo_dnam_len: usize,
    /// The `BMDT` `general` flag byte, kept whole. The armor-type enum names
    /// only bits 3, 5 and 7; bit 6 is "Non-Playable" and is set on 34 of the
    /// game's records, so a rebuild from the enum alone would clear it.
    pub armo_bmdt_general: u8,

    // NPC_ & CREA fields
    pub actor_female: bool,
    pub actor_essential: bool,
    pub actor_respawn: bool,
    pub actor_no_low_level_proc: bool,
    pub actor_no_vats_melee: bool,
    pub actor_allow_pc_dialogue: bool,
    pub actor_cant_open_doors: bool,
    pub actor_no_knockdowns: bool,
    pub actor_allow_pickpocket: bool,
    pub actor_is_ghost: bool,
    pub actor_invulnerable: bool,
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
    pub actor_death_item: Option<u32>,
    pub actor_template: Option<u32>,
    pub actor_template_flags: u16,
    pub actor_combat_style: Option<u32>,
    pub actor_splo: Vec<u32>,
    pub splo_to_add: Option<u32>,
    pub actor_factions: Vec<(u32, i8)>,
    pub faction_to_add: Option<u32>,
    pub faction_rank_to_add: i8,

    // Actor Physical & Combat fields (CREA/NPC_)
    pub crea_attack_reach: u8,
    pub actor_base_scale: f32,
    pub crea_turning_speed: f32,
    pub crea_foot_weight: f32,
    pub actor_impact_material: u32,
    pub actor_sound_level: u32,
    pub actor_unarmed_effect: Option<u32>,
    pub crea_unarmed_anim: u16,
    pub actor_body_part_data: Option<u32>,
    pub crea_impact_dataset: Option<u32>,

    // AI & Economy fields
    pub actor_barter_gold: u16,
    pub actor_disp_base: i16,
    pub actor_aggro_radius: i32,
    pub actor_packages: Vec<u32>,
    pub package_to_add: Option<u32>,

    // Inventory collection (CNTO)
    pub actor_inventory: Vec<(u32, i32)>,
    pub item_to_add: Option<u32>,
    pub item_count_to_add: i32,

    // Sound Type overrides (CSDT/CSDI/CSDC)
    pub crea_sounds: Vec<(u32, u32, u8)>,
    pub sound_type_to_add: u32,
    pub sound_fid_to_add: Option<u32>,
    pub sound_chance_to_add: u8,

    // Model & Animation lists (NIFZ / KFFZ)
    pub crea_models: Vec<String>,
    pub model_to_add: String,
    pub crea_animations: Vec<String>,
    pub anim_to_add: String,

    // Destruction data (DEST)
    pub dest_health: i32,

    // NPC_ specific fields (DNAM, HNAM, LNAM, ENAM, HCLR, NAM6, NAM7, PNAM)
    pub npc_skills: [u8; 14],
    pub npc_skill_offsets: [u8; 14],
    pub npc_hair: Option<u32>,
    pub npc_hair_length: f32,
    pub npc_eyes: Option<u32>,
    pub npc_hair_color: [u8; 4],
    pub npc_height: f32,
    pub npc_weight: f32,
    pub npc_head_parts: Vec<u32>,
    pub head_part_to_add: Option<u32>,

    // ALCH fields
    pub alch_weight: f32,
    pub alch_val: i32,
    pub alch_addict_chance: f32,
    /// `ENIT` Withdrawal Effect, the `SPEL` the ingestible gives when the
    /// addiction is withdrawn. 34 of the 189 ALCH records carry one.
    pub alch_withdrawal: Option<u32>,
    /// `ENIT` Sound - Consume, a `SOUN` FormID.
    pub alch_sound: Option<u32>,
    pub alch_is_food: bool,
    pub alch_is_medicine: bool,
    pub alch_is_poison: bool,
    /// The `ENIT` flag byte, kept whole. The spec names bits 1 (Food) and 2
    /// (Medicine) and marks bit 0 "No Auto-Calc (Unused)"; 180 of the
    /// game's 189 records set it, so a rebuild from the three checkboxes
    /// alone would clear it on every save.
    pub alch_enit_flags: u8,

    // AMMO fields
    pub ammo_val: i32,
    pub ammo_weight: f32,
    pub ammo_speed: f32,
    pub ammo_dmg_mult: f32,
    pub ammo_clip_rounds: u32,
    /// `QNAM` Abbreviation — a cstring, not a FormID. 48 of the 92 AMMO
    /// records carry one, with 23 distinct values ("5mm", ".223", "Bag").
    /// The spec's `RCIL` is the ammo effect, and `DAT2` holds the consumed
    /// ammo, so there is no FormID here to read.
    pub ammo_abbrev: String,

    // CONT & DOOR fields
    pub cont_weight: f32,
    pub cont_respawn: bool,
    pub cont_sound_open: Option<u32>,
    pub cont_sound_close: Option<u32>,
    pub door_auto: bool,
    pub door_hidden: bool,
    pub door_minimal: bool,
    pub door_sliding: bool,
    pub door_sound_open: Option<u32>,

    // ACTI & TERM fields
    pub acti_prompt: String,
    pub acti_sound_loop: Option<u32>,
    pub term_difficulty: u8,
    pub term_welcome: String,
    pub term_sound_loop: Option<u32>,

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
    /// `DATA` Flags 1, the whole u8 word. The spec names bits 0, 1 and 2
    /// (Hidden From PC, Evil, Special Combat); the word is kept whole so a bit
    /// the spec does not name survives an edit.
    pub fact_flags1: u8,
    /// `DATA` Flags 2, the whole u8 word: bit 0 is Track Crime and bit 1 is
    /// Allow Sell. The old editor read and wrote only bit 0, so a faction
    /// that could be sold from lost the bit on every save.
    pub fact_flags2: u8,
    /// The length of the record's `DATA` subrecord: 1 or 4 in the game's own
    /// records. 34 of the 682 FACT records in `FalloutNV.esm` have a one-byte
    /// `DATA` — flags 1 only, no flags 2. The length is kept so a save that
    /// changes nothing reproduces the record byte for byte, and the editor can
    /// extend a one-byte record to the full four.
    pub fact_data_len: usize,
    /// `DATA` bytes 2..4, the spec's "Unused" run, carried through unchanged.
    pub fact_data_unused: [u8; 2],
    /// `CNAM` — an unnamed float32 the spec marks Unused. 34 of the game's
    /// FACT records carry it, every one of them with the value 1.0.
    pub fact_cnam: f32,
    /// Whether the record carries a `CNAM` at all.
    pub fact_has_cnam: bool,
    /// `WMI1` Reputation, a `REPU` record.
    pub fact_reputation: Option<u32>,
    /// `XNAM` — the faction's relations to other factions, in file order.
    /// 1,314 of them across the game's FACT records, up to 45 in one record.
    pub fact_relations: Vec<FactRelation>,
    /// The rank collection, in file order: each entry is an `RNAM` and the
    /// titles that follow it.
    pub fact_ranks: Vec<FactRank>,
    /// The relation the panel is about to add.
    pub fact_relation_to_add: FactRelation,
    /// The rank entry the panel is about to add.
    pub fact_rank_to_add: FactRank,
    pub qust_start_enabled: bool,
    pub qust_repeated_stages: bool,
    pub qust_priority: u8,
    pub qust_delay: f32,
    /// The length of the record's `DATA` subrecord. 434 of the 436 QUST
    /// records in FalloutNV.esm carry eight bytes, and 2 carry only two —
    /// the flags byte and the priority, with no delay. The length is kept so
    /// a save that changes nothing reproduces the record's own bytes.
    pub qust_data_len: usize,

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
    pub lvl_entries: Vec<(i16, u32, i16)>,
    pub lvl_entry_level_to_add: i16,
    pub lvl_entry_fid_to_add: Option<u32>,
    pub lvl_entry_count_to_add: i16,

    // IMOD & CLAS fields
    pub imod_val: u32,
    pub imod_weight: f32,
    pub imod_desc: String,
    /// `Teaches` is an int8 skill enum that starts at -1 ("None"), so it is
    /// signed, not a `u8` index into a 0-based table.
    pub clas_teaches: i8,
    pub clas_max_train: u8,
    pub clas_playable: bool,
    pub clas_guard: bool,
    /// The four `Tag Skill` int32s, in order.
    pub clas_tag_skills: [i32; 4],
    /// `Buys/Sells and Services`, the u32 service-flag word.
    pub clas_services: u32,
    /// The seven `ATTR` attributes, in the spec's order: Strength through Luck.
    pub clas_attributes: [u8; 7],
    /// The `DATA` flag word in full. The spec names bits 0 and 1, but the game's
    /// own classes set bits 2, 3 and 4 too (`VaultDweller`, `VendorFood` and
    /// `PlayerClass` all carry 29), so the word is kept whole and the two named
    /// bits are edited inside it. Writing only the named bits would clear the
    /// rest and change what the class does.
    pub clas_flags: u32,
    /// `DESC` Description — required by the spec, and the class's own blurb in
    /// GECK's Class dialog.
    pub clas_desc: String,

    // CSTY & MGEF fields
    /// `CSTD` Advanced - Standard, all 92 bytes of it. The subrecord is a fixed
    /// layout (every one of the 84 CSTY records in FalloutNV.esm measures
    /// exactly 92 bytes), so the fields are read and written by slice indexing.
    pub csty_std: CstdData,
    /// `CSAD` Advanced - Advanced: 21 floats, 84 bytes. Not read at all before
    /// this, so every modifier an advanced combat style applied was invisible.
    pub csty_adv: CstyAdvData,
    /// `CSSD` Simple: 16 fields, 64 bytes.
    pub csty_simple: CstySimpleData,
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

    // ADDN (Addon Node) fields
    /// `DATA` Node Index — the int32 index of the node in the master particle
    /// system.
    pub addn_node_index: i32,
    /// `DNAM` Master Particle System Cap — the uint16 cap.
    pub addn_master_cap: u16,
    /// `DNAM`'s two unknown bytes, carried through unchanged.
    pub addn_dnam_unknown: [u8; 2],
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    ///
    /// The collection is a set of subrecords that always appear together, so
    /// it is kept as one struct rather than as loose fields. `MODT`'s texture
    /// hashes are kept as raw bytes: the editor cannot author a file hash, and
    /// rewriting one would corrupt the mod.
    pub addn_model: ModelData,

    // AVIF fields
    pub avif_desc: String,
    pub avif_short_name: String,
    pub avif_icon: String,
    pub avif_mico: String,

    // EYES fields
    /// `ICON` Texture — the eye texture's path.
    pub eyes_icon: String,
    /// `DATA` Flags, the u8 flag word: Playable, Not Male, Not Female.
    pub eyes_flags: u8,

    // HAIR fields
    /// `ICON` Texture — the hair texture's path.
    pub hair_icon: String,
    /// `DATA` Flags, the u8 flag word: Playable, Not Male, Not Female, Fixed.
    pub hair_flags: u8,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    ///
    /// The collection is a set of subrecords that always appear together, so
    /// it is kept as one struct rather than as loose fields. `MODT`'s texture
    /// hashes are kept as raw bytes: the editor cannot author a file hash, and
    /// rewriting one would corrupt the mod.
    pub hair_model: ModelData,

    // HDPT fields
    /// `DATA` Flags, the u8 flag word: Playable.
    pub hdpt_flags: u8,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    ///
    /// The collection is a set of subrecords that always appear together, so
    /// it is kept as one struct rather than as loose fields. `MODT`'s texture
    /// hashes are kept as raw bytes: the editor cannot author a file hash, and
    /// rewriting one would corrupt the mod.
    pub hdpt_model: ModelData,
    /// `HNAM` Extra Parts — the FormIDs of other `HDPT` records. The subrecord
    /// repeats: the game's records carry two or three of them.
    pub hdpt_extra_parts: Vec<u32>,
    /// The `HDPT` to add to the extra-parts list.
    pub hdpt_extra_part_to_add: Option<u32>,

    // BPTD fields: one Body Part per entry, in file order.
    // A part is (BPTN?) BPNN BPNT BPNI BPND NAM1 NAM4 NAM5, so the optional
    // Part Name is an Option and every other field is present.
    pub bptd_ragdoll: Option<u32>,
    pub bptd_parts: Vec<BptdPart>,
    pub bptd_part_to_add: BptdPart,

    // CREA specific fields
    pub crea_type: u8,
    pub crea_combat_skill: u8,
    pub crea_magic_skill: u8,
    pub crea_stealth_skill: u8,
    pub crea_biped: bool,
    pub crea_weapon_shield: bool,
    pub crea_swims: bool,
    pub crea_flies: bool,
    pub crea_walks: bool,
    pub crea_no_head_track: bool,
    pub crea_sound_inherit: Option<u32>,
    pub crea_weapon_list: Option<u32>,
}

/// One body part of a `BPTD` record.
///
/// A part in the file is `(BPTN?) BPNN BPNT BPNI BPND NAM1 NAM4 NAM5` [observed:
/// every one of the 304 parts across the 49 BPTD records in FalloutNV.esm follows
/// that order, with `BPTN` present on 301 of 304]. Only the Part Name is optional.
#[derive(Debug, Clone, Default)]
pub struct BptdPart {
    /// `BPTN` Part Name. `None` for an unnamed part, which is legal and appears
    /// in real data (e.g. `NVVoidBodyPartData` has no `BPTN`).
    pub name: Option<String>,
    /// `BPNN` Part Node — the skeleton bone the engine matches the part to.
    pub node: String,
    /// `BPNT` VATS Target — the node a VATS shot is aimed at.
    pub vats_target: String,
    /// `BPNI` IK Data - Start Node.
    pub ik_start_node: String,
    /// `BPND` Node Data — the 84-byte packed struct, field-for-field.
    pub data: BptdNodeData,
    /// `NAM1` Limb Replacement Model — the gore mesh shown when the part is lost.
    pub limb_model: String,
    /// `NAM4` Gore Effects - Target Bone.
    pub gore_target_bone: String,
    /// `NAM5` Limb Replacement Model Texture Hashes, 24 bytes per texture hash.
    /// Kept as raw bytes: the editor cannot author a hash, and rewriting an
    /// unknown one would corrupt the mod.
    pub texture_hashes: Vec<u8>,
}

/// The 84-byte `BPND` struct, in file order.
///
/// The layout is fixed-width and byte-exact (all 304 `BPND` subrecords in
/// `FalloutNV.esm` measure exactly 84 bytes), so it is read and written by
/// direct slice indexing rather than through a schema decode.
#[derive(Debug, Clone, Default)]
pub struct BptdNodeData {
    pub damage_mult: f32,
    pub flags: u8,
    pub part_type: u8,
    pub health_percent: u8,
    pub actor_value: i8,
    pub to_hit_chance: u8,
    pub explodable_explosion_chance_percent: u8,
    pub explodable_debris_count: u16,
    pub explodable_debris: Option<u32>,
    pub explodable_explosion: Option<u32>,
    pub tracking_max_angle: f32,
    pub explodable_debris_scale: f32,
    pub severable_debris_count: i32,
    pub severable_debris: Option<u32>,
    pub severable_explosion: Option<u32>,
    pub severable_debris_scale: f32,
    pub gore_x: f32,
    pub gore_y: f32,
    pub gore_z: f32,
    pub gore_rx: f32,
    pub gore_ry: f32,
    pub gore_rz: f32,
    pub severable_impact_dataset: Option<u32>,
    pub explodable_impact_dataset: Option<u32>,
    pub severable_decal_count: u8,
    pub explodable_decal_count: u8,
    pub limb_replacement_scale: f32,
}

/// The size of a `BPND` payload: 4+1+1+1+1+1+1+2+4+4+4+4+4+4+4+4+ (6x4 gore)
/// +4+4+1+1+2+4 = 84.
const BPND_LEN: usize = 84;

impl BptdNodeData {
    fn from_bytes(b: &[u8]) -> Self {
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let u32at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        // A zero FormID is the game's "none": the engine treats it as absent, and
        // so does the editor, so a cleared dropdown round-trips as None.
        let fid = |i: usize| {
            let v = u32at(i);
            if v == 0 {
                None
            } else {
                Some(v)
            }
        };
        Self {
            damage_mult: f(0),
            flags: b[4],
            part_type: b[5],
            health_percent: b[6],
            actor_value: b[7] as i8,
            to_hit_chance: b[8],
            explodable_explosion_chance_percent: b[9],
            explodable_debris_count: u16::from_le_bytes([b[10], b[11]]),
            explodable_debris: fid(12),
            explodable_explosion: fid(16),
            tracking_max_angle: f(20),
            explodable_debris_scale: f(24),
            severable_debris_count: i32::from_le_bytes([b[28], b[29], b[30], b[31]]),
            severable_debris: fid(32),
            severable_explosion: fid(36),
            severable_debris_scale: f(40),
            gore_x: f(44),
            gore_y: f(48),
            gore_z: f(52),
            gore_rx: f(56),
            gore_ry: f(60),
            gore_rz: f(64),
            severable_impact_dataset: fid(68),
            explodable_impact_dataset: fid(72),
            severable_decal_count: b[76],
            explodable_decal_count: b[77],
            limb_replacement_scale: f(80),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; BPND_LEN];
        fn put(b: &mut [u8], i: usize, bytes: [u8; 4]) {
            b[i..i + 4].copy_from_slice(&bytes);
        }
        put(&mut b, 0, self.damage_mult.to_le_bytes());
        b[4] = self.flags;
        b[5] = self.part_type;
        b[6] = self.health_percent;
        b[7] = self.actor_value as u8;
        b[8] = self.to_hit_chance;
        b[9] = self.explodable_explosion_chance_percent;
        b[10..12].copy_from_slice(&self.explodable_debris_count.to_le_bytes());
        put(
            &mut b,
            12,
            self.explodable_debris.unwrap_or(0).to_le_bytes(),
        );
        put(
            &mut b,
            16,
            self.explodable_explosion.unwrap_or(0).to_le_bytes(),
        );
        put(&mut b, 20, self.tracking_max_angle.to_le_bytes());
        put(&mut b, 24, self.explodable_debris_scale.to_le_bytes());
        b[28..32].copy_from_slice(&self.severable_debris_count.to_le_bytes());
        put(&mut b, 32, self.severable_debris.unwrap_or(0).to_le_bytes());
        put(
            &mut b,
            36,
            self.severable_explosion.unwrap_or(0).to_le_bytes(),
        );
        put(&mut b, 40, self.severable_debris_scale.to_le_bytes());
        put(&mut b, 44, self.gore_x.to_le_bytes());
        put(&mut b, 48, self.gore_y.to_le_bytes());
        put(&mut b, 52, self.gore_z.to_le_bytes());
        put(&mut b, 56, self.gore_rx.to_le_bytes());
        put(&mut b, 60, self.gore_ry.to_le_bytes());
        put(&mut b, 64, self.gore_rz.to_le_bytes());
        put(
            &mut b,
            68,
            self.severable_impact_dataset.unwrap_or(0).to_le_bytes(),
        );
        put(
            &mut b,
            72,
            self.explodable_impact_dataset.unwrap_or(0).to_le_bytes(),
        );
        b[76] = self.severable_decal_count;
        b[77] = self.explodable_decal_count;
        // b[78..80] is the struct's 2-byte Unused field; every BPND in the game's
        // masters carries zeros there, so the editor keeps it zero.
        put(&mut b, 80, self.limb_replacement_scale.to_le_bytes());
        b
    }
}

impl BptdPart {
    /// The part's display name: the Part Name when it has one, else its node, so
    /// an unnamed part is still addressable in the list.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.node)
    }
}

/// The model data collection — `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
///
/// fopdoc's shared Model Data page says these five subrecords hold model data
/// and always appear together. `MODB` and `MODD` are optional, and `MODT`'s
/// texture hashes are opaque file hashes the editor cannot author, so both are
/// kept exactly as the record had them.
///
/// The collection appears more than once in some record types, with the second,
/// third and fourth instances numbered. A record that carries a numbered
/// instance names it in [`ModelData::instance_sigs`].
#[derive(Debug, Clone, Default)]
pub struct ModelData {
    /// `MODL` — the model file's path.
    pub modl: String,
    /// `MODB` — four unknown bytes, carried through unchanged.
    pub modb: Option<[u8; 4]>,
    /// `MODT` — the texture file hashes, 24 bytes each, carried through
    /// unchanged.
    pub modt: Vec<u8>,
    /// `MODS` — the alternate textures.
    pub mods: Vec<AlternateTexture>,
    /// `MODD` — the FaceGen model flags: Head, Torso, Right Hand, Left Hand.
    pub modd: Option<u8>,
    /// Which numbered instance of the collection this is. Zero is the
    /// original (`MODL`/`MODT`/`MODS`/`MODD`); one, two and three are the
    /// second, third and fourth instances. See
    /// [`ModelData::instance_sigs`] for the signatures each one uses.
    pub instance: u8,
}

impl ModelData {
    /// The subrecord signatures this instance of the collection uses.
    ///
    /// The spec's Model Data page tabulates the numbered instances:
    /// `MODL`/`MOD2`/`MOD3`/`MOD4`, `MODT`/`MO2T`/`MO3T`/`MO4T` and
    /// `MODS`/`MO2S`/`MO3S`/`MO4S`. `MODB` exists only in the original
    /// instance, and `MODD` exists in the original and in the *third*
    /// instance, where it is spelled `MOSD` — the second and fourth instances
    /// have none. The two that are always missing are `None` here.
    pub fn instance_sigs(&self) -> ModelInstanceSigs {
        ModelInstanceSigs {
            modl: model_sig(self.instance, b"MODL", b"MOD2", b"MOD3", b"MOD4"),
            modt: model_sig(self.instance, b"MODT", b"MO2T", b"MO3T", b"MO4T"),
            mods: model_sig(self.instance, b"MODS", b"MO2S", b"MO3S", b"MO4S"),
            modd: match self.instance {
                0 => Some(*b"MODD"),
                2 => Some(*b"MOSD"),
                _ => None,
            },
        }
    }
}

/// The subrecord signatures one instance of a model data collection uses.
#[derive(Debug, Clone, Copy)]
pub struct ModelInstanceSigs {
    /// The model file's subrecord.
    pub modl: [u8; 4],
    /// The texture hash subrecord.
    pub modt: [u8; 4],
    /// The alternate textures subrecord.
    pub mods: [u8; 4],
    /// The FaceGen flags subrecord, or None when this instance has none.
    pub modd: Option<[u8; 4]>,
}

/// The signature the given instance of a model collection uses.
///
/// Instance 0 is the original signature; instance 1, 2 and 3 use the numbered
/// ones.
fn model_sig(
    instance: u8,
    base: &[u8; 4],
    second: &[u8; 4],
    third: &[u8; 4],
    fourth: &[u8; 4],
) -> [u8; 4] {
    match instance {
        0 => *base,
        1 => *second,
        2 => *third,
        3 => *fourth,
        _ => *base,
    }
}

/// One alternate texture in a `MODS` subrecord.
///
/// The struct is the spec's own: a `uint32` name length, the 3D name of that
/// length, a `formid` naming a `TXST` record, and an `int32` 3D index.
#[derive(Debug, Clone, Default)]
pub struct AlternateTexture {
    /// The 3D name of the alternate texture.
    pub name: String,
    /// The `TXST` record holding the new texture.
    pub texture: u32,
    /// The 3D index the texture applies to.
    pub index: i32,
}

impl AlternateTexture {
    /// Read one alternate texture out of a `MODS` payload, returning it and
    /// the offset just past it.
    fn from_bytes(b: &[u8], off: usize) -> Option<(Self, usize)> {
        if off + 4 > b.len() {
            return None;
        }
        let name_len = u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as usize;
        let mut at = off + 4;
        if at + name_len + 8 > b.len() {
            return None;
        }
        let name = String::from_utf8_lossy(&b[at..at + name_len]).to_string();
        at += name_len;
        let texture = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        at += 4;
        let index = i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        at += 4;
        Some((
            AlternateTexture {
                name,
                texture,
                index,
            },
            at,
        ))
    }

    /// Write one alternate texture into a `MODS` payload.
    fn to_bytes(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let mut b = Vec::with_capacity(4 + name.len() + 8);
        b.extend_from_slice(&(name.len() as u32).to_le_bytes());
        b.extend_from_slice(name);
        b.extend_from_slice(&self.texture.to_le_bytes());
        b.extend_from_slice(&self.index.to_le_bytes());
        b
    }
}

/// Read a `MODS` payload into its alternate textures.
fn parse_mods(b: &[u8]) -> Vec<AlternateTexture> {
    if b.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let mut out = Vec::with_capacity(count.min(64));
    let mut off = 4usize;
    for _ in 0..count {
        let Some((alt, next)) = AlternateTexture::from_bytes(b, off) else {
            break;
        };
        out.push(alt);
        off = next;
    }
    out
}

/// Write alternate textures back into a `MODS` payload.
fn write_mods(alts: &[AlternateTexture]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&(alts.len() as u32).to_le_bytes());
    for a in alts {
        b.extend_from_slice(&a.to_bytes());
    }
    b
}

/// One `XNAM` (Relation) subrecord of a `FACT` record.
///
/// The struct is fixed-width: a `formid` to the related faction (bytes 0..4),
/// an `int32` modifier (bytes 4..8) and a `uint32` group combat reaction
/// (bytes 8..12). Every one of the 1,314 `XNAM` subrecords in the game's FACT
/// records measures exactly 12 bytes, so it is read and written by slice
/// indexing.
#[derive(Debug, Clone, Default)]
pub struct FactRelation {
    /// The related `FACT` (or `RACE`) record. None means the relation names
    /// no faction, which the game never writes but the editor can express.
    pub faction: Option<u32>,
    /// The modifier: how much this faction's opinion of the other is shifted.
    /// The game's own values run from -100 to 100.
    pub modifier: i32,
    /// The group combat reaction: 0 Neutral, 1 Enemy, 2 Ally, 3 Friend.
    pub group_combat_reaction: u32,
}

/// The size of an `XNAM` payload: the spec's own total, and the only size the
/// game's masters carry.
const XNAM_LEN: usize = 12;

impl FactRelation {
    fn from_bytes(b: &[u8]) -> Self {
        if b.len() < XNAM_LEN {
            return Self::default();
        }
        Self {
            faction: Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            modifier: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            group_combat_reaction: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; XNAM_LEN];
        b[0..4].copy_from_slice(&self.faction.unwrap_or(0).to_le_bytes());
        b[4..8].copy_from_slice(&self.modifier.to_le_bytes());
        b[8..12].copy_from_slice(&self.group_combat_reaction.to_le_bytes());
        b
    }
}

/// Read a rank collection back out of a queued subrecord sequence.
///
/// The sequence is the run `RNAM (MNAM)? (FNAM)? (INAM)?` repeated, so the
/// entries are walked in order: an `RNAM` opens an entry and the titles that
/// follow belong to it.
fn fact_ranks_from_sequence(subs: &[([u8; 4], Vec<u8>)]) -> Vec<FactRank> {
    let mut ranks: Vec<FactRank> = Vec::new();
    let mut cur: Option<FactRank> = None;
    for (sig, data) in subs {
        if *sig == *b"RNAM" {
            if let Some(r) = cur.take() {
                ranks.push(r);
            }
            cur = Some(FactRank {
                number: if data.len() >= 4 {
                    i32::from_le_bytes([data[0], data[1], data[2], data[3]])
                } else {
                    0
                },
                ..Default::default()
            });
        } else if *sig == *b"MNAM" || *sig == *b"FNAM" || *sig == *b"INAM" {
            if cur.is_none() {
                cur = Some(FactRank::default());
            }
            let rank = cur.as_mut().expect("just opened an entry");
            let text = String::from_utf8_lossy(data)
                .trim_end_matches('\0')
                .to_string();
            match sig {
                b"MNAM" => rank.male = text,
                b"FNAM" => rank.female = text,
                _ => rank.insignia = Some(text),
            }
        }
    }
    if let Some(r) = cur.take() {
        ranks.push(r);
    }
    ranks
}

/// One entry of a `FACT` record's rank collection.
///
/// A rank is an `RNAM` (the rank number) followed by the titles that belong to
/// it: `MNAM` (male title), `FNAM` (female title) and `INAM` (an insignia
/// path the spec marks unused). The entries are a collection of runs, not a
/// list of one signature, so they are walked in file order and written back as
/// one sequence.
#[derive(Debug, Clone, Default)]
pub struct FactRank {
    /// `RNAM` — the rank number.
    pub number: i32,
    /// `MNAM` — the male title.
    pub male: String,
    /// `FNAM` — the female title.
    pub female: String,
    /// `INAM` — the insignia path, which the spec marks unused. None of the
    /// game's FACT records carry one, but the editor can write it.
    pub insignia: Option<String>,
}

/// The 92-byte `CSTD` (Advanced - Standard) struct, in file order.
///
/// The layout is fixed-width and byte-exact — every one of the 84 CSTY records
/// in `FalloutNV.esm` measures exactly 92 bytes — so it is read and written by
/// direct slice indexing rather than through a schema decode. The four Unused
/// runs the spec marks are not fields the editor owns; they are preserved as
/// the bytes they already are.
#[derive(Debug, Clone, Default)]
pub struct CstdData {
    // Maneuver Decision
    pub dodge_chance: u8,
    pub left_right_chance: u8,
    pub dodge_lr_timer_min: f32,
    pub dodge_lr_timer_max: f32,
    pub dodge_forward_timer_min: f32,
    pub dodge_forward_timer_max: f32,
    pub dodge_back_timer_min: f32,
    pub dodge_back_timer_max: f32,
    pub idle_timer_min: f32,
    pub idle_timer_max: f32,
    // Melee Decision
    pub block_chance: u8,
    pub attack_chance: u8,
    pub recoil_stagger_bonus_to_attack: f32,
    pub unconscious_bonus_to_attack: f32,
    pub hand_to_hand_bonus_to_attack: f32,
    pub power_attack_chance: u8,
    pub recoil_stagger_bonus_to_power: f32,
    pub unconscious_bonus_to_power_attack: f32,
    pub power_attack_normal: u8,
    pub power_attack_forward: u8,
    pub power_attack_back: u8,
    pub power_attack_left: u8,
    pub power_attack_right: u8,
    pub hold_timer_min: f32,
    pub hold_timer_max: f32,
    pub flags: u16,
    // Maneuver Decision (cont.)
    pub acrobatic_dodge_chance: u8,
    // Melee Decision - Power Attacks (cont.)
    pub rushing_attack_chance: u8,
    pub rushing_attack_distance_mult: f32,
}

/// The size of a `CSTD` payload: the spec's own total, and the only size the
/// game's masters carry.
const CSTD_LEN: usize = 92;

impl CstdData {
    fn from_bytes(b: &[u8]) -> Self {
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Self {
            dodge_chance: b[0],
            left_right_chance: b[1],
            dodge_lr_timer_min: f(4),
            dodge_lr_timer_max: f(8),
            dodge_forward_timer_min: f(12),
            dodge_forward_timer_max: f(16),
            dodge_back_timer_min: f(20),
            dodge_back_timer_max: f(24),
            idle_timer_min: f(28),
            idle_timer_max: f(32),
            block_chance: b[36],
            attack_chance: b[37],
            recoil_stagger_bonus_to_attack: f(40),
            unconscious_bonus_to_attack: f(44),
            hand_to_hand_bonus_to_attack: f(48),
            power_attack_chance: b[52],
            recoil_stagger_bonus_to_power: f(56),
            unconscious_bonus_to_power_attack: f(60),
            power_attack_normal: b[64],
            power_attack_forward: b[65],
            power_attack_back: b[66],
            power_attack_left: b[67],
            power_attack_right: b[68],
            hold_timer_min: f(72),
            hold_timer_max: f(76),
            flags: u16::from_le_bytes([b[80], b[81]]),
            acrobatic_dodge_chance: b[84],
            rushing_attack_chance: b[85],
            rushing_attack_distance_mult: f(88),
        }
    }

    /// Write the struct back over `original`, so every byte the editor does not
    /// own keeps the value the record had.
    ///
    /// The spec marks several runs "Unused" and xEdit agrees, but the game does
    /// not leave them zero: 40 of the 84 CSTD subrecords in `FalloutNV.esm`
    /// carry the ASCII fragments "im", "pr", "ial" and ".kf" at offsets 2, 38,
    /// 53 and 69 — the same four in all 40, with the other 44 all-zero. A
    /// leftover string from whichever editor wrote them, not a documented
    /// field, so the editor copies the bytes through rather than deciding what
    /// they mean. Zeroing them would change 40 records for no reason.
    fn write_into(&self, original: &[u8]) -> Vec<u8> {
        let mut b = original.to_vec();
        b.resize(CSTD_LEN, 0);
        fn put(b: &mut [u8], i: usize, bytes: [u8; 4]) {
            b[i..i + 4].copy_from_slice(&bytes);
        }
        b[0] = self.dodge_chance;
        b[1] = self.left_right_chance;
        put(&mut b, 4, self.dodge_lr_timer_min.to_le_bytes());
        put(&mut b, 8, self.dodge_lr_timer_max.to_le_bytes());
        put(&mut b, 12, self.dodge_forward_timer_min.to_le_bytes());
        put(&mut b, 16, self.dodge_forward_timer_max.to_le_bytes());
        put(&mut b, 20, self.dodge_back_timer_min.to_le_bytes());
        put(&mut b, 24, self.dodge_back_timer_max.to_le_bytes());
        put(&mut b, 28, self.idle_timer_min.to_le_bytes());
        put(&mut b, 32, self.idle_timer_max.to_le_bytes());
        b[36] = self.block_chance;
        b[37] = self.attack_chance;
        put(
            &mut b,
            40,
            self.recoil_stagger_bonus_to_attack.to_le_bytes(),
        );
        put(&mut b, 44, self.unconscious_bonus_to_attack.to_le_bytes());
        put(&mut b, 48, self.hand_to_hand_bonus_to_attack.to_le_bytes());
        b[52] = self.power_attack_chance;
        put(&mut b, 56, self.recoil_stagger_bonus_to_power.to_le_bytes());
        put(
            &mut b,
            60,
            self.unconscious_bonus_to_power_attack.to_le_bytes(),
        );
        b[64] = self.power_attack_normal;
        b[65] = self.power_attack_forward;
        b[66] = self.power_attack_back;
        b[67] = self.power_attack_left;
        b[68] = self.power_attack_right;
        put(&mut b, 72, self.hold_timer_min.to_le_bytes());
        put(&mut b, 76, self.hold_timer_max.to_le_bytes());
        b[80..82].copy_from_slice(&self.flags.to_le_bytes());
        b[84] = self.acrobatic_dodge_chance;
        b[85] = self.rushing_attack_chance;
        put(&mut b, 88, self.rushing_attack_distance_mult.to_le_bytes());
        // The spec's Unused runs — [2..4], [38..40], [53..56], [69..72],
        // [82..84] and [86..88] — are left exactly as the record had them.
        b
    }
}

/// The 84-byte `CSAD` (Advanced - Advanced) struct: 21 floats, in file order.
///
/// Every one of the 84 CSAD subrecords in `FalloutNV.esm` measures exactly 84
/// bytes. The editor did not read this subrecord at all, so an advanced combat
/// style's dodge fatigue, encumbrance, block, attack and power-attack modifiers
/// were invisible and could not be edited.
#[derive(Debug, Clone, Default)]
pub struct CstyAdvData {
    pub dodge_fatigue_mod_mult: f32,
    pub dodge_fatigue_mod_base: f32,
    pub encumbered_speed_mod_base: f32,
    pub encumbered_speed_mod_mult: f32,
    pub dodge_while_under_attack_mult: f32,
    pub dodge_not_under_attack_mult: f32,
    pub dodge_back_while_under_attack_mult: f32,
    pub dodge_back_not_under_attack_mult: f32,
    pub dodge_forward_while_attacking_mult: f32,
    pub dodge_forward_not_attacking_mult: f32,
    pub block_skill_modifier_mult: f32,
    pub block_skill_modifier_base: f32,
    pub block_while_under_attack_mult: f32,
    pub block_not_under_attack_mult: f32,
    pub attack_skill_modifier_mult: f32,
    pub attack_skill_modifier_base: f32,
    pub attack_while_under_attack_mult: f32,
    pub attack_not_under_attack_mult: f32,
    pub attack_during_block_mult: f32,
    pub power_attack_fatigue_mod_base: f32,
    pub power_attack_fatigue_mod_mult: f32,
}

/// The size of a `CSAD` payload: 21 floats.
const CSAD_LEN: usize = 84;

impl CstyAdvData {
    fn from_bytes(b: &[u8]) -> Self {
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Self {
            dodge_fatigue_mod_mult: f(0),
            dodge_fatigue_mod_base: f(4),
            encumbered_speed_mod_base: f(8),
            encumbered_speed_mod_mult: f(12),
            dodge_while_under_attack_mult: f(16),
            dodge_not_under_attack_mult: f(20),
            dodge_back_while_under_attack_mult: f(24),
            dodge_back_not_under_attack_mult: f(28),
            dodge_forward_while_attacking_mult: f(32),
            dodge_forward_not_attacking_mult: f(36),
            block_skill_modifier_mult: f(40),
            block_skill_modifier_base: f(44),
            block_while_under_attack_mult: f(48),
            block_not_under_attack_mult: f(52),
            attack_skill_modifier_mult: f(56),
            attack_skill_modifier_base: f(60),
            attack_while_under_attack_mult: f(64),
            attack_not_under_attack_mult: f(68),
            attack_during_block_mult: f(72),
            power_attack_fatigue_mod_base: f(76),
            power_attack_fatigue_mod_mult: f(80),
        }
    }

    /// Write the struct back over `original`, so a byte the editor does not own
    /// keeps the value the record had.
    fn write_into(&self, original: &[u8]) -> Vec<u8> {
        let mut b = original.to_vec();
        b.resize(CSAD_LEN, 0);
        fn put(b: &mut [u8], i: usize, v: f32) {
            b[i..i + 4].copy_from_slice(&v.to_le_bytes());
        }
        put(&mut b, 0, self.dodge_fatigue_mod_mult);
        put(&mut b, 4, self.dodge_fatigue_mod_base);
        put(&mut b, 8, self.encumbered_speed_mod_base);
        put(&mut b, 12, self.encumbered_speed_mod_mult);
        put(&mut b, 16, self.dodge_while_under_attack_mult);
        put(&mut b, 20, self.dodge_not_under_attack_mult);
        put(&mut b, 24, self.dodge_back_while_under_attack_mult);
        put(&mut b, 28, self.dodge_back_not_under_attack_mult);
        put(&mut b, 32, self.dodge_forward_while_attacking_mult);
        put(&mut b, 36, self.dodge_forward_not_attacking_mult);
        put(&mut b, 40, self.block_skill_modifier_mult);
        put(&mut b, 44, self.block_skill_modifier_base);
        put(&mut b, 48, self.block_while_under_attack_mult);
        put(&mut b, 52, self.block_not_under_attack_mult);
        put(&mut b, 56, self.attack_skill_modifier_mult);
        put(&mut b, 60, self.attack_skill_modifier_base);
        put(&mut b, 64, self.attack_while_under_attack_mult);
        put(&mut b, 68, self.attack_not_under_attack_mult);
        put(&mut b, 72, self.attack_during_block_mult);
        put(&mut b, 76, self.power_attack_fatigue_mod_base);
        put(&mut b, 80, self.power_attack_fatigue_mod_mult);
        b
    }
}

/// The 64-byte `CSSD` (Simple) struct, in file order.
///
/// Every one of the 84 CSSD subrecords in `FalloutNV.esm` measures exactly 64
/// bytes. The editor read only the first two fields before this.
#[derive(Debug, Clone, Default)]
pub struct CstySimpleData {
    pub cover_search_radius: f32,
    pub take_cover_chance: f32,
    pub wait_timer_min: f32,
    pub wait_timer_max: f32,
    pub wait_to_fire_timer_min: f32,
    pub wait_to_fire_timer_max: f32,
    pub fire_timer_min: f32,
    pub fire_timer_max: f32,
    pub ranged_weapon_range_mult_min: f32,
    /// The spec's Weapon Restrictions enum: 0 None, 1 Melee Only, 2 Ranged Only.
    pub weapon_restrictions: u32,
    pub ranged_weapon_range_mult_max: f32,
    pub max_targeting_fov: f32,
    pub combat_radius: f32,
    pub semi_auto_firing_delay_mult_min: f32,
    pub semi_auto_firing_delay_mult_max: f32,
}

/// The size of a `CSSD` payload.
const CSSD_LEN: usize = 64;

impl CstySimpleData {
    fn from_bytes(b: &[u8]) -> Self {
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Self {
            cover_search_radius: f(0),
            take_cover_chance: f(4),
            wait_timer_min: f(8),
            wait_timer_max: f(12),
            wait_to_fire_timer_min: f(16),
            wait_to_fire_timer_max: f(20),
            fire_timer_min: f(24),
            fire_timer_max: f(28),
            ranged_weapon_range_mult_min: f(32),
            weapon_restrictions: u32::from_le_bytes([b[40], b[41], b[42], b[43]]),
            ranged_weapon_range_mult_max: f(44),
            max_targeting_fov: f(48),
            combat_radius: f(52),
            semi_auto_firing_delay_mult_min: f(56),
            semi_auto_firing_delay_mult_max: f(60),
        }
    }

    /// Write the struct back over `original`, so a byte the editor does not own
    /// keeps the value the record had.
    fn write_into(&self, original: &[u8]) -> Vec<u8> {
        let mut b = original.to_vec();
        b.resize(CSSD_LEN, 0);
        fn put(b: &mut [u8], i: usize, v: f32) {
            b[i..i + 4].copy_from_slice(&v.to_le_bytes());
        }
        put(&mut b, 0, self.cover_search_radius);
        put(&mut b, 4, self.take_cover_chance);
        put(&mut b, 8, self.wait_timer_min);
        put(&mut b, 12, self.wait_timer_max);
        put(&mut b, 16, self.wait_to_fire_timer_min);
        put(&mut b, 20, self.wait_to_fire_timer_max);
        put(&mut b, 24, self.fire_timer_min);
        put(&mut b, 28, self.fire_timer_max);
        put(&mut b, 32, self.ranged_weapon_range_mult_min);
        b[40..44].copy_from_slice(&self.weapon_restrictions.to_le_bytes());
        put(&mut b, 44, self.ranged_weapon_range_mult_max);
        put(&mut b, 48, self.max_targeting_fov);
        put(&mut b, 52, self.combat_radius);
        put(&mut b, 56, self.semi_auto_firing_delay_mult_min);
        put(&mut b, 60, self.semi_auto_firing_delay_mult_max);
        // b[36..40] is the spec's 4-byte Unused run; it is left exactly as the
        // record had it, for the same reason as CSTD's.
        b
    }
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

        let (raw_subs, flags, found) = if let Some(rec) = session.doc.record(form_id) {
            (rec.subrecords().unwrap_or_default(), rec.header.flags, true)
        } else if let Ok(lo) = session.load_order() {
            if let Some((_, orig)) = fnv_world::fields::winner(lo, form_id) {
                (
                    orig.subrecords().unwrap_or_default(),
                    orig.header.flags,
                    true,
                )
            } else {
                (Vec::new(), 0, false)
            }
        } else {
            (Vec::new(), 0, false)
        };

        if !found {
            return;
        }

        self.record_flags = flags;
        self.quest_item = (flags & (1 << 10)) != 0;

        self.raw_subs.clear();
        self.actor_splo.clear();
        self.actor_factions.clear();
        self.actor_packages.clear();
        self.actor_inventory.clear();
        self.crea_sounds.clear();
        self.lvl_entries.clear();
        self.npc_head_parts.clear();
        self.bptd_parts.clear();
        self.bptd_ragdoll = None;
        self.bptd_part_to_add = BptdPart::default();
        self.fact_relations.clear();
        self.fact_ranks.clear();
        self.hair_model = ModelData::default();
        self.hdpt_model = ModelData::default();
        self.hdpt_extra_parts.clear();
        self.addn_model = ModelData::default();
        self.addn_node_index = 0;
        self.addn_master_cap = 0;
        self.addn_dnam_unknown = [0, 0];
        let mut cur_csdt = None;
        let mut cur_csdi = None;
        for s in &raw_subs {
            self.raw_subs.insert(s.sig, s.data.clone());
            if s.sig == *b"PNAM" && s.data.len() >= 4 && self.sig == *b"NPC_" {
                let p_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                self.npc_head_parts.push(p_fid);
            }
            if s.sig == *b"SPLO" && s.data.len() >= 4 {
                let fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                self.actor_splo.push(fid);
            }
            if s.sig == *b"SNAM" && s.data.len() >= 5 {
                let fac_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                let rank = s.data[4] as i8;
                self.actor_factions.push((fac_fid, rank));
            }
            if s.sig == *b"PKID" && s.data.len() >= 4 {
                let pkg_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                self.actor_packages.push(pkg_fid);
            }
            if s.sig == *b"CNTO" && s.data.len() >= 8 {
                let item_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                let count = i32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                self.actor_inventory.push((item_fid, count));
            }
            if s.sig == *b"LVLO" && s.data.len() >= 12 {
                let lvl = i16::from_le_bytes([s.data[0], s.data[1]]);
                let r_fid = u32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                let cnt = i16::from_le_bytes([s.data[8], s.data[9]]);
                self.lvl_entries.push((lvl, r_fid, cnt));
            }
            if s.sig == *b"CSDT" && s.data.len() >= 4 {
                cur_csdt = Some(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
                cur_csdi = None;
            } else if s.sig == *b"CSDI" && s.data.len() >= 4 {
                cur_csdi = Some(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
            } else if s.sig == *b"CSDC" && !s.data.is_empty() {
                let chance = s.data[0];
                if let (Some(t), Some(snd)) = (cur_csdt, cur_csdi) {
                    self.crea_sounds.push((t, snd, chance));
                }
            }
        }

        self.crea_models.clear();
        if let Some(b) = self.raw_subs.get(b"NIFZ") {
            for slice in b.split(|&b| b == 0) {
                if !slice.is_empty() {
                    self.crea_models
                        .push(String::from_utf8_lossy(slice).to_string());
                }
            }
        }

        self.crea_animations.clear();
        if let Some(b) = self.raw_subs.get(b"KFFZ") {
            for slice in b.split(|&b| b == 0) {
                if !slice.is_empty() {
                    self.crea_animations
                        .push(String::from_utf8_lossy(slice).to_string());
                }
            }
        }

        self.dest_health = self
            .raw_subs
            .get(b"DEST")
            .filter(|b| b.len() >= 4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0);
        if let Some(edits) = session.doc.edits_for(form_id) {
            for e in edits {
                match e {
                    nvre_edit::Edit::SetSub { sig, data } => {
                        self.raw_subs.insert(*sig, data.clone());
                    }
                    nvre_edit::Edit::SetSubList {
                        sig: [b'S', b'P', b'L', b'O'],
                        data,
                    } => {
                        self.actor_splo.clear();
                        for d in data {
                            if d.len() >= 4 {
                                self.actor_splo
                                    .push(u32::from_le_bytes([d[0], d[1], d[2], d[3]]));
                            }
                        }
                    }
                    nvre_edit::Edit::SetSubList {
                        sig: [b'S', b'N', b'A', b'M'],
                        data,
                    } => {
                        self.actor_factions.clear();
                        for d in data {
                            if d.len() >= 5 {
                                let fac_fid = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                                let rank = d[4] as i8;
                                self.actor_factions.push((fac_fid, rank));
                            }
                        }
                    }
                    nvre_edit::Edit::SetSubList {
                        sig: [b'P', b'K', b'I', b'D'],
                        data,
                    } => {
                        self.actor_packages.clear();
                        for d in data {
                            if d.len() >= 4 {
                                self.actor_packages
                                    .push(u32::from_le_bytes([d[0], d[1], d[2], d[3]]));
                            }
                        }
                    }
                    nvre_edit::Edit::SetSubList {
                        sig: [b'C', b'N', b'T', b'O'],
                        data,
                    } => {
                        self.actor_inventory.clear();
                        for d in data {
                            if d.len() >= 8 {
                                let item_fid = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                                let count = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
                                self.actor_inventory.push((item_fid, count));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(b) = self.raw_subs.get(b"OBND").filter(|b| b.len() >= 12) {
            self.obnd_min = [
                i16::from_le_bytes([b[0], b[1]]),
                i16::from_le_bytes([b[2], b[3]]),
                i16::from_le_bytes([b[4], b[5]]),
            ];
            self.obnd_max = [
                i16::from_le_bytes([b[6], b[7]]),
                i16::from_le_bytes([b[8], b[9]]),
                i16::from_le_bytes([b[10], b[11]]),
            ];
        } else {
            self.obnd_min = [0, 0, 0];
            self.obnd_max = [0, 0, 0];
        }

        // Common strings & FormIDs
        self.edid = self.get_string(b"EDID");
        self.full_name = self.get_string(b"FULL");
        self.model = self.get_string(b"MODL");
        self.script = self.get_fid(b"SCRI");

        // The FACT relations and the rank collection, queued by the faction
        // editor, are applied after the type loader: the loader reads them off
        // disk, and the queued copy is what the mod will actually carry. Each
        // is tracked on its own, so queuing one does not blank the other.
        let mut fact_relations_overlay: Option<Vec<FactRelation>> = None;
        let mut fact_ranks_overlay: Option<Vec<FactRank>> = None;
        if let Some(edits) = session.doc.edits_for(form_id) {
            for e in edits {
                match e {
                    nvre_edit::Edit::SetSubList {
                        sig: [b'X', b'N', b'A', b'M'],
                        data,
                    } => {
                        fact_relations_overlay =
                            Some(data.iter().map(|d| FactRelation::from_bytes(d)).collect());
                    }
                    nvre_edit::Edit::SetSubrecordSequence { remove_sigs, subs }
                        if remove_sigs.contains(b"RNAM") =>
                    {
                        fact_ranks_overlay = Some(fact_ranks_from_sequence(subs));
                    }
                    _ => {}
                }
            }
        }

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
            b"FACT" => self.load_fact(&raw_subs),
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
            b"ADDN" => self.load_addn(&raw_subs),
            b"AVIF" => self.load_avif(),
            b"EYES" => self.load_eyes(),
            b"HAIR" => self.load_hair(&raw_subs),
            b"HDPT" => self.load_hdpt(&raw_subs),
            b"BPTD" => self.load_bptd(&raw_subs),
            _ => self.load_misc(),
        }

        // The queued relations and ranks win over what is on disk: the mod is
        // what the editor is showing, not the master it was opened from.
        if let Some(relations) = fact_relations_overlay {
            self.fact_relations = relations;
        }
        if let Some(ranks) = fact_ranks_overlay {
            self.fact_ranks = ranks;
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

    fn get_u8(&self, sig: &[u8; 4]) -> Option<u8> {
        self.raw_subs.get(sig).and_then(|b| b.first().copied())
    }

    fn get_u16(&self, sig: &[u8; 4]) -> Option<u16> {
        self.raw_subs
            .get(sig)
            .filter(|b| b.len() >= 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn get_u32(&self, sig: &[u8; 4]) -> Option<u32> {
        self.raw_subs
            .get(sig)
            .filter(|b| b.len() >= 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn get_f32(&self, sig: &[u8; 4]) -> Option<f32> {
        self.raw_subs
            .get(sig)
            .filter(|b| b.len() >= 4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
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
        self.weap_repair_list = self.get_fid(b"REPL");
    }

    fn load_armo(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 12) {
            self.armo_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.armo_hp = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.armo_weight = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        }
        // `DNAM` is 12 bytes on 368 of the 389 ARMO records in FalloutNV.esm
        // and 4 bytes on the other 21, which carry only the `AR` int16 and two
        // unused bytes. `DT` and the `Modulates Voice` flag exist only in the
        // long form, so the fields are reset before the read: a record opened
        // after a long one must not still show the previous record's `DT`.
        self.armo_dt = 0.0;
        self.armo_modulates_voice = false;
        self.armo_dnam_len = 0;
        if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| b.len() >= 4) {
            self.armo_dnam_len = b.len();
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
            // The `general` byte carries more than the three bits the type
            // enum names — bit 6 is "Non-Playable" and is set on 34 of the
            // game's records. The whole byte is kept so a save that changes
            // nothing reproduces it.
            self.armo_bmdt_general = general;
        }
        self.armo_enchant = self.get_fid(b"EITM");
        self.armo_repair_list = self.get_fid(b"REPL");
    }

    fn load_actor(&mut self) {
        if let Some(b) = self.raw_subs.get(b"ACBS").filter(|b| b.len() >= 24) {
            self.actor_flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.actor_fatigue = u16::from_le_bytes([b[4], b[5]]);
            self.actor_barter_gold = u16::from_le_bytes([b[6], b[7]]);
            self.actor_level_or_mult = i16::from_le_bytes([b[8], b[9]]);
            self.actor_calc_min = u16::from_le_bytes([b[10], b[11]]);
            self.actor_calc_max = u16::from_le_bytes([b[12], b[13]]);
            self.actor_speed_mult = u16::from_le_bytes([b[14], b[15]]);
            self.actor_karma = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.actor_disp_base = i16::from_le_bytes([b[20], b[21]]);
            self.actor_template_flags = u16::from_le_bytes([b[22], b[23]]);

            // Shared & NPC flags
            self.actor_female = (self.actor_flags & (1 << 0)) != 0;
            self.actor_essential = (self.actor_flags & (1 << 1)) != 0;
            self.actor_respawn = (self.actor_flags & (1 << 3)) != 0;
            self.actor_pc_mult = (self.actor_flags & (1 << 7)) != 0;
            self.actor_no_low_level_proc = (self.actor_flags & (1 << 9)) != 0;
            self.actor_no_vats_melee = (self.actor_flags & (1 << 20)) != 0;
            self.actor_allow_pc_dialogue = (self.actor_flags & (1 << 21)) != 0;
            self.actor_cant_open_doors = (self.actor_flags & (1 << 22)) != 0;
            self.actor_no_knockdowns = (self.actor_flags & (1 << 26)) != 0;
            self.actor_allow_pickpocket = (self.actor_flags & (1 << 28)) != 0;
            self.actor_is_ghost = (self.actor_flags & (1 << 29)) != 0;
            self.actor_invulnerable = (self.actor_flags & (1 << 31)) != 0;

            // Creature movement/traits
            self.crea_biped = (self.actor_flags & (1 << 0)) != 0;
            self.crea_weapon_shield = (self.actor_flags & (1 << 2)) != 0;
            self.crea_swims = (self.actor_flags & (1 << 4)) != 0;
            self.crea_flies = (self.actor_flags & (1 << 5)) != 0;
            self.crea_walks = (self.actor_flags & (1 << 6)) != 0;
            self.crea_no_head_track = (self.actor_flags & (1 << 30)) != 0;
        }
        if self.sig == *b"CREA" {
            if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 17) {
                self.crea_type = b[0];
                self.crea_combat_skill = b[1];
                self.crea_magic_skill = b[2];
                self.crea_stealth_skill = b[3];
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
            self.actor_body_part_data = self.get_fid(b"PNAM");
            self.crea_impact_dataset = self.get_fid(b"CNAM");
        } else if self.sig == *b"NPC_" {
            if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 11) {
                self.actor_health = i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i16;
                self.actor_strength = b[4];
                self.actor_perception = b[5];
                self.actor_endurance = b[6];
                self.actor_charisma = b[7];
                self.actor_intelligence = b[8];
                self.actor_agility = b[9];
                self.actor_luck = b[10];
            }
            if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| b.len() >= 28) {
                self.npc_skills.copy_from_slice(&b[0..14]);
                self.npc_skill_offsets.copy_from_slice(&b[14..28]);
            }
            self.npc_hair = self.get_fid(b"HNAM");
            self.npc_hair_length = self.get_f32(b"LNAM").unwrap_or(1.0);
            self.npc_eyes = self.get_fid(b"ENAM");
            if let Some(b) = self.raw_subs.get(b"HCLR").filter(|b| b.len() >= 4) {
                self.npc_hair_color = [b[0], b[1], b[2], b[3]];
            }
            self.npc_height = self.get_f32(b"NAM6").unwrap_or(1.0);
            self.npc_weight = self.get_f32(b"NAM7").unwrap_or(1.0);
        }
        if let Some(b) = self.raw_subs.get(b"AIDT").filter(|b| b.len() >= 20) {
            self.actor_aggression = b[0];
            self.actor_confidence = b[1];
            self.actor_energy = b[2];
            self.actor_resp = b[3];
            self.actor_mood = b[4];
            self.actor_assistance = b[14] as i8;
            self.actor_aggro_radius = i32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        }
        self.actor_race = self.get_fid(b"RNAM");
        self.actor_class = self.get_fid(b"CNAM");
        self.actor_voice = self.get_fid(b"VTCK");
        self.actor_death_item = self.get_fid(b"INAM");
        self.actor_template = self.get_fid(b"TPLT");
        self.actor_combat_style = self.get_fid(b"ZNAM");
        self.crea_sound_inherit = self.get_fid(b"CSCR");
        self.crea_weapon_list = self.get_fid(b"LNAM");

        self.crea_attack_reach = self.get_u8(b"RNAM").unwrap_or(0);
        self.actor_base_scale = self.get_f32(b"BNAM").unwrap_or(1.0);
        self.crea_turning_speed = self.get_f32(b"TNAM").unwrap_or(0.0);
        self.crea_foot_weight = self.get_f32(b"WNAM").unwrap_or(0.0);
        self.actor_impact_material = self.get_u32(b"NAM4").unwrap_or(6);
        self.actor_sound_level = self.get_u32(b"NAM5").unwrap_or(1);
        self.actor_unarmed_effect = self.get_fid(b"EITM");
        self.crea_unarmed_anim = self.get_u16(b"EAMT").unwrap_or(255);
        self.actor_body_part_data = self.get_fid(b"PNAM");
        if self.sig == *b"CREA" {
            self.crea_impact_dataset = self.get_fid(b"CNAM");
        }
    }

    fn load_alch(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 4) {
            self.alch_weight = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        // `ENIT` is 20 bytes on every one of the 189 ALCH records. The spec's
        // layout is: value i32 (0..4), flags u8 (4), unused (5..8), Withdrawal
        // Effect formid (8..12), Addiction Chance f32 (12..16), Sound -
        // Consume formid (16..20). The editor had the last two the wrong way
        // round, so it showed a FormID where the chance was and a chance
        // where the FormID was.
        self.alch_withdrawal = None;
        self.alch_sound = None;
        self.alch_addict_chance = 0.0;
        self.alch_enit_flags = 0;
        if let Some(b) = self.raw_subs.get(b"ENIT").filter(|b| b.len() >= 20) {
            self.alch_val = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.alch_enit_flags = b[4];
            self.alch_is_food = b[4] & 2 != 0;
            self.alch_is_medicine = b[4] & 4 != 0;
            self.alch_is_poison = b[4] & 8 != 0;
            self.alch_withdrawal = Some(u32::from_le_bytes([b[8], b[9], b[10], b[11]]));
            self.alch_addict_chance = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.alch_sound = Some(u32::from_le_bytes([b[16], b[17], b[18], b[19]]));
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
        self.ammo_abbrev = self.get_string(b"QNAM");
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
        self.door_sound_open = self.get_fid(b"SNAM");
    }

    fn load_acti(&mut self) {
        self.acti_prompt = self.get_string(b"RNAM");
        self.acti_sound_loop = self.get_fid(b"SNAM");
    }

    fn load_term(&mut self) {
        if let Some(b) = self.raw_subs.get(b"DNAM") {
            self.term_difficulty = b.first().copied().unwrap_or(0);
        }
        self.term_welcome = self.get_string(b"WNAM");
        self.term_sound_loop = self.get_fid(b"SNAM");
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

    /// Load a `FACT` (Faction) record.
    ///
    /// The relations (`XNAM`) and the rank collection are both lists of
    /// subrecords that repeat, so they cannot be read out of `raw_subs`, which
    /// holds one value per signature: it would collapse every relation onto the
    /// last one and every rank title onto the last rank's. The subrecords are
    /// walked in file order instead.
    fn load_fact(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        // DATA: flags 1, flags 2, then the spec's two "Unused" bytes. 34 of the
        // 682 FACT records in FalloutNV.esm have a one-byte DATA — the game
        // wrote only flags 1 — and those carry no flags 2 at all.
        self.fact_data_len = 0;
        self.fact_data_unused = [0, 0];
        if let Some(b) = self.raw_subs.get(b"DATA") {
            self.fact_data_len = b.len();
            if !b.is_empty() {
                self.fact_flags1 = b[0];
            }
            if b.len() > 1 {
                self.fact_flags2 = b[1];
            }
            if b.len() >= 4 {
                self.fact_data_unused = [b[2], b[3]];
            }
        }

        // CNAM is an unnamed float32 the spec marks Unused. The game writes it
        // on 34 of the records, every one of them with the value 1.0.
        self.fact_has_cnam = false;
        self.fact_cnam = 0.0;
        if let Some(b) = self.raw_subs.get(b"CNAM").filter(|b| b.len() >= 4) {
            self.fact_has_cnam = true;
            self.fact_cnam = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }

        self.fact_reputation = self.get_fid(b"WMI1");

        // The relations and the rank collection, in file order.
        self.fact_relations.clear();
        self.fact_ranks.clear();
        self.hair_model = ModelData::default();
        let mut cur_rank: Option<FactRank> = None;
        for s in raw_subs {
            match &s.sig {
                b"XNAM" => {
                    self.fact_relations.push(FactRelation::from_bytes(&s.data));
                }
                b"RNAM" => {
                    if let Some(r) = cur_rank.take() {
                        self.fact_ranks.push(r);
                    }
                    cur_rank = Some(FactRank {
                        number: if s.data.len() >= 4 {
                            i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])
                        } else {
                            0
                        },
                        ..Default::default()
                    });
                }
                b"MNAM" | b"FNAM" | b"INAM" => {
                    // A title with no rank in front of it: the game never writes
                    // one, but the editor can express it, so it opens an entry
                    // of its own rather than dropping the title.
                    if cur_rank.is_none() {
                        cur_rank = Some(FactRank::default());
                    }
                    let rank = cur_rank.as_mut().expect("just opened an entry");
                    let text = String::from_utf8_lossy(&s.data)
                        .trim_end_matches('\0')
                        .to_string();
                    match &s.sig {
                        b"MNAM" => rank.male = text,
                        b"FNAM" => rank.female = text,
                        _ => rank.insignia = Some(text),
                    }
                }
                _ => {}
            }
        }
        if let Some(r) = cur_rank.take() {
            self.fact_ranks.push(r);
        }
    }

    fn load_qust(&mut self) {
        // `DATA` is eight bytes on 434 of the 436 QUST records and two bytes
        // on the other two, which carry the flags and the priority and no
        // delay at all. The length is kept so a save reproduces the record's
        // own bytes instead of growing the short form to eight.
        self.qust_data_len = 0;
        self.qust_delay = 0.0;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 2) {
            self.qust_data_len = b.len();
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
        // DATA is a fixed 28-byte struct: four int32 tag skills, the u32 flag
        // word, the u32 service word, int8 teaches, u8 max training level, and
        // two unused bytes. Every one of the 74 CLAS records in FalloutNV.esm
        // measures exactly 28 bytes, so the offsets are the spec's own.
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 26) {
            for i in 0..4 {
                let o = i * 4;
                self.clas_tag_skills[i] = i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            }
            let flags = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.clas_flags = flags;
            self.clas_playable = (flags & 1) != 0;
            self.clas_guard = (flags & 2) != 0;
            self.clas_services = u32::from_le_bytes([b[20], b[21], b[22], b[23]]);
            self.clas_teaches = b[24] as i8;
            self.clas_max_train = b[25];
        }
        // ATTR is the seven SPECIAL attributes in the spec's order.
        if let Some(b) = self.raw_subs.get(b"ATTR").filter(|b| b.len() >= 7) {
            self.clas_attributes.copy_from_slice(&b[0..7]);
        }
        self.clas_desc = self.get_string(b"DESC");
    }

    fn load_csty(&mut self) {
        // CSTD is a fixed 92-byte struct; every CSTY record in the game's
        // masters carries exactly that, so the whole of it is read.
        if let Some(b) = self.raw_subs.get(b"CSTD").filter(|b| b.len() >= CSTD_LEN) {
            self.csty_std = CstdData::from_bytes(b);
        }
        // CSAD is 21 floats and was not read at all before this.
        if let Some(b) = self.raw_subs.get(b"CSAD").filter(|b| b.len() >= CSAD_LEN) {
            self.csty_adv = CstyAdvData::from_bytes(b);
        }
        // CSSD is a fixed 64-byte struct; the old editor read only its first
        // two fields.
        if let Some(b) = self.raw_subs.get(b"CSSD").filter(|b| b.len() >= CSSD_LEN) {
            self.csty_simple = CstySimpleData::from_bytes(b);
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

    fn load_eyes(&mut self) {
        self.eyes_icon = self.get_string(b"ICON");
        self.eyes_flags = self.get_u8(b"DATA").unwrap_or(0);
    }

    /// Load a `HAIR` (Hair) record.
    ///
    /// The record is `EDID`, `FULL`, the model data collection (`MODL`,
    /// `MODT`, `ICON`) and a one-byte `DATA`. The model data collection's
    /// subrecords are read from the record's own subrecord list rather than
    /// from `raw_subs`, because the collection can appear more than once in a
    /// record and `raw_subs` holds one value per signature.
    fn load_hair(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.hair_icon = self.get_string(b"ICON");
        self.hair_flags = self.get_u8(b"DATA").unwrap_or(0);
        self.hair_model = ModelData::default();

        // The original instance of the collection, and any numbered ones.
        for s in raw_subs {
            match &s.sig {
                b"MODL" => self.hair_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.hair_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.hair_model.modt = s.data.clone(),
                b"MODS" => self.hair_model.mods = parse_mods(&s.data),
                b"MODD" => self.hair_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load a `HDPT` (Head Part) record.
    ///
    /// The record is `EDID`, `FULL`, the model data collection (`MODL`,
    /// `MODT`, `MODS`, and optionally `MODB` and `MODD`), a one-byte `DATA`
    /// flag word, and a repeated `HNAM` naming other head parts. The model
    /// collection's subrecords and the extra parts are read from the record's
    /// own subrecord list rather than from `raw_subs`, which holds one value
    /// per signature.
    fn load_hdpt(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.hdpt_flags = self.get_u8(b"DATA").unwrap_or(0);
        self.hdpt_model = ModelData::default();
        self.hdpt_extra_parts.clear();
        for s in raw_subs {
            match &s.sig {
                b"MODL" => self.hdpt_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.hdpt_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.hdpt_model.modt = s.data.clone(),
                b"MODS" => self.hdpt_model.mods = parse_mods(&s.data),
                b"MODD" => self.hdpt_model.modd = s.data.first().copied(),
                b"HNAM" if s.data.len() >= 4 => {
                    self.hdpt_extra_parts.push(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                _ => {}
            }
        }
    }

    /// Load an `ADDN` (Addon Node) record.
    ///
    /// The record is `EDID`, `OBND`, the model data collection (`MODL`,
    /// `MODT`, and optionally `MODB`, `MODS` and `MODD`), a four-byte `DATA`
    /// holding the node index, and a four-byte `DNAM` holding the master
    /// particle system cap and two bytes the spec does not name.
    ///
    /// The model collection's subrecords are read from the record's own
    /// subrecord list rather than from `raw_subs`, which holds one value per
    /// signature.
    fn load_addn(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.addn_node_index = 0;
        self.addn_master_cap = 0;
        self.addn_dnam_unknown = [0, 0];
        self.addn_model = ModelData::default();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if s.data.len() >= 4 => {
                    self.addn_node_index =
                        i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"DNAM" if s.data.len() >= 4 => {
                    self.addn_master_cap = u16::from_le_bytes([s.data[0], s.data[1]]);
                    self.addn_dnam_unknown = [s.data[2], s.data[3]];
                }
                b"MODL" => self.addn_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.addn_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.addn_model.modt = s.data.clone(),
                b"MODS" => self.addn_model.mods = parse_mods(&s.data),
                b"MODD" => self.addn_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    fn load_avif(&mut self) {
        self.avif_desc = self.get_string(b"DESC");
        self.avif_short_name = self.get_string(b"ANAM");
        self.avif_icon = self.get_string(b"ICON");
        self.avif_mico = self.get_string(b"MICO");
    }

    /// Load a `BPTD` (Body Part Data) record.
    ///
    /// The parts are a repeating collection — `(BPTN?) BPNN BPNT BPNI BPND NAM1
    /// NAM4 NAM5` — so they cannot be read out of `raw_subs`, which is a
    /// single-value-per-signature map and would collapse every part's BPNN onto
    /// the last one. The subrecords are walked in order instead, and the whole
    /// list is written back with one sequence edit.
    fn load_bptd(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.bptd_parts.clear();
        self.bptd_ragdoll = None;
        let mut part: Option<BptdPart> = None;
        for s in raw_subs {
            let str_at = |i: usize| {
                String::from_utf8_lossy(&s.data[i..])
                    .trim_end_matches('\0')
                    .to_string()
            };
            match &s.sig {
                b"RAGA" if s.data.len() >= 4 => {
                    self.bptd_ragdoll = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                // A part name opens a named part. An unnamed part has no BPTN, so
                // its BPNN opens the part instead.
                b"BPTN" => {
                    if let Some(p) = part.take() {
                        self.bptd_parts.push(p);
                    }
                    part = Some(BptdPart {
                        name: Some(str_at(0)),
                        ..BptdPart::default()
                    });
                }
                b"BPNN" => {
                    if part.as_ref().is_none_or(|p| p.name.is_none()) {
                        if let Some(p) = part.take() {
                            self.bptd_parts.push(p);
                        }
                        part = Some(BptdPart::default());
                    }
                    if let Some(p) = part.as_mut() {
                        p.node = str_at(0);
                    }
                }
                b"BPNT" => {
                    if let Some(p) = part.as_mut() {
                        p.vats_target = str_at(0);
                    }
                }
                b"BPNI" => {
                    if let Some(p) = part.as_mut() {
                        p.ik_start_node = str_at(0);
                    }
                }
                b"BPND" if s.data.len() >= BPND_LEN => {
                    if let Some(p) = part.as_mut() {
                        p.data = BptdNodeData::from_bytes(&s.data);
                    }
                }
                b"NAM1" => {
                    if let Some(p) = part.as_mut() {
                        p.limb_model = str_at(0);
                    }
                }
                b"NAM4" => {
                    if let Some(p) = part.as_mut() {
                        p.gore_target_bone = str_at(0);
                    }
                }
                b"NAM5" => {
                    if let Some(p) = part.as_mut() {
                        p.texture_hashes = s.data.clone();
                    }
                }
                _ => {}
            }
        }
        if let Some(p) = part.take() {
            self.bptd_parts.push(p);
        }
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

        let mut flags = self.record_flags;
        if self.quest_item {
            flags |= 1 << 10;
        } else {
            flags &= !(1 << 10);
        }
        session.doc.set_flags(fid, flags);

        if self.obnd_min != [0, 0, 0]
            || self.obnd_max != [0, 0, 0]
            || self.raw_subs.contains_key(b"OBND")
        {
            let mut obnd = vec![0u8; 12];
            obnd[0..2].copy_from_slice(&self.obnd_min[0].to_le_bytes());
            obnd[2..4].copy_from_slice(&self.obnd_min[1].to_le_bytes());
            obnd[4..6].copy_from_slice(&self.obnd_min[2].to_le_bytes());
            obnd[6..8].copy_from_slice(&self.obnd_max[0].to_le_bytes());
            obnd[8..10].copy_from_slice(&self.obnd_max[1].to_le_bytes());
            obnd[10..12].copy_from_slice(&self.obnd_max[2].to_le_bytes());
            session.doc.set_sub(fid, b"OBND", obnd);
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
            b"ADDN" => self.save_addn(session),
            b"AVIF" => self.save_avif(session),
            b"EYES" => self.save_eyes(session),
            b"HAIR" => self.save_hair(session),
            b"HDPT" => self.save_hdpt(session),
            b"BPTD" => self.save_bptd(session),
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

        if let Some(r) = self.weap_repair_list {
            session.doc.set_sub(fid, b"REPL", r.to_le_bytes().to_vec());
        }
    }

    fn save_armo(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = vec![0u8; 12];
        d[0..4].copy_from_slice(&self.armo_val.to_le_bytes());
        d[4..8].copy_from_slice(&self.armo_hp.to_le_bytes());
        d[8..12].copy_from_slice(&self.armo_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);

        // `DNAM` is written back over the record's own bytes, not rebuilt from
        // the fields the editor owns. 21 of the 389 ARMO records in
        // FalloutNV.esm carry a 4-byte `DNAM` with no `DT` and no flags; a
        // rebuild would grow them to 12 bytes and give them values they
        // never had. The length the record had is kept, so a save that
        // changes nothing is byte-identical.
        let original_dnam = self.raw_subs.get(b"DNAM").cloned();
        let dnam_len = if self.armo_dnam_len >= 8 { 12 } else { 4 };
        let mut dnam = original_dnam.unwrap_or_else(|| vec![0u8; dnam_len]);
        dnam.resize(dnam_len, 0);
        dnam[0..2].copy_from_slice(&self.armo_dr.to_le_bytes());
        if dnam_len >= 8 {
            dnam[4..8].copy_from_slice(&self.armo_dt.to_le_bytes());
        }
        if dnam_len >= 10 {
            let flags: u16 = if self.armo_modulates_voice { 1 } else { 0 };
            dnam[8..10].copy_from_slice(&flags.to_le_bytes());
        }
        session.doc.set_sub(fid, b"DNAM", dnam);

        // `BMDT` likewise: the `general` byte is written whole, with the
        // armor-type enum's three bits written inside it, so a bit the enum
        // does not name — "Non-Playable", set on 34 of the game's records —
        // survives a save.
        let original_bmdt = self.raw_subs.get(b"BMDT").cloned();
        let mut bmdt = original_bmdt.unwrap_or_else(|| vec![0u8; 8]);
        bmdt.resize(8, 0);
        bmdt[0..4].copy_from_slice(&self.armo_biped_flags.to_le_bytes());
        let mut general = self.armo_bmdt_general;
        let type_bit: u8 = match self.armo_type {
            1 => 1 << 3, // Medium
            2 => 1 << 7, // Heavy
            3 => 1 << 5, // Power
            _ => 0,      // Light
        };
        // Clear the three named bits, then set the one the enum selects.
        general &= !((1 << 3) | (1 << 5) | (1 << 7));
        general |= type_bit;
        bmdt[4] = general;
        session.doc.set_sub(fid, b"BMDT", bmdt);

        if let Some(ench) = self.armo_enchant {
            session
                .doc
                .set_sub(fid, b"EITM", ench.to_le_bytes().to_vec());
        }

        if let Some(r) = self.armo_repair_list {
            session.doc.set_sub(fid, b"REPL", r.to_le_bytes().to_vec());
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

        let set_flag = |f: &mut u32, bit: u8, val: bool| {
            if val {
                *f |= 1 << bit;
            } else {
                *f &= !(1 << bit);
            }
        };

        // Shared actor flags
        set_flag(&mut f, 1, self.actor_essential);
        set_flag(&mut f, 3, self.actor_respawn);
        set_flag(&mut f, 7, self.actor_pc_mult);
        set_flag(&mut f, 9, self.actor_no_low_level_proc);
        set_flag(&mut f, 20, self.actor_no_vats_melee);
        set_flag(&mut f, 26, self.actor_no_knockdowns);
        set_flag(&mut f, 28, self.actor_allow_pickpocket);
        set_flag(&mut f, 29, self.actor_is_ghost);
        set_flag(&mut f, 31, self.actor_invulnerable);

        if self.sig == *b"CREA" {
            set_flag(&mut f, 0, self.crea_biped);
            set_flag(&mut f, 2, self.crea_weapon_shield);
            set_flag(&mut f, 4, self.crea_swims);
            set_flag(&mut f, 5, self.crea_flies);
            set_flag(&mut f, 6, self.crea_walks);
            set_flag(&mut f, 21, self.actor_allow_pc_dialogue);
            set_flag(&mut f, 22, self.actor_cant_open_doors);
            set_flag(&mut f, 30, self.crea_no_head_track);
        } else {
            set_flag(&mut f, 0, self.actor_female);
        }
        acbs[0..4].copy_from_slice(&f.to_le_bytes());
        acbs[4..6].copy_from_slice(&self.actor_fatigue.to_le_bytes());
        acbs[6..8].copy_from_slice(&self.actor_barter_gold.to_le_bytes());
        acbs[8..10].copy_from_slice(&self.actor_level_or_mult.to_le_bytes());
        acbs[10..12].copy_from_slice(&self.actor_calc_min.to_le_bytes());
        acbs[12..14].copy_from_slice(&self.actor_calc_max.to_le_bytes());
        acbs[14..16].copy_from_slice(&self.actor_speed_mult.to_le_bytes());
        acbs[16..20].copy_from_slice(&self.actor_karma.to_le_bytes());
        acbs[20..22].copy_from_slice(&self.actor_disp_base.to_le_bytes());
        acbs[22..24].copy_from_slice(&self.actor_template_flags.to_le_bytes());
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
        aidt[16..20].copy_from_slice(&self.actor_aggro_radius.to_le_bytes());
        session.doc.set_sub(fid, b"AIDT", aidt);

        if self.sig == *b"NPC_" {
            let mut data = vec![0u8; 11];
            let hp_i32 = self.actor_health as i32;
            data[0..4].copy_from_slice(&hp_i32.to_le_bytes());
            data[4] = self.actor_strength;
            data[5] = self.actor_perception;
            data[6] = self.actor_endurance;
            data[7] = self.actor_charisma;
            data[8] = self.actor_intelligence;
            data[9] = self.actor_agility;
            data[10] = self.actor_luck;
            session.doc.set_sub(fid, b"DATA", data);

            let mut dnam = vec![0u8; 28];
            dnam[0..14].copy_from_slice(&self.npc_skills);
            dnam[14..28].copy_from_slice(&self.npc_skill_offsets);
            session.doc.set_sub(fid, b"DNAM", dnam);

            if let Some(h) = self.npc_hair {
                session.doc.set_sub(fid, b"HNAM", h.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"HNAM");
            }
            session
                .doc
                .set_sub(fid, b"LNAM", self.npc_hair_length.to_le_bytes().to_vec());

            if let Some(e) = self.npc_eyes {
                session.doc.set_sub(fid, b"ENAM", e.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"ENAM");
            }

            session
                .doc
                .set_sub(fid, b"HCLR", self.npc_hair_color.to_vec());
            session
                .doc
                .set_sub(fid, b"NAM6", self.npc_height.to_le_bytes().to_vec());
            session
                .doc
                .set_sub(fid, b"NAM7", self.npc_weight.to_le_bytes().to_vec());

            let pnam_buffers: Vec<Vec<u8>> = self
                .npc_head_parts
                .iter()
                .map(|id| id.to_le_bytes().to_vec())
                .collect();
            session.doc.set_sub_list(fid, b"PNAM", pnam_buffers);
        } else {
            let mut data = self
                .raw_subs
                .get(b"DATA")
                .cloned()
                .unwrap_or_else(|| vec![0u8; 17]);
            if data.len() < 17 {
                data.resize(17, 0);
            }
            data[0] = self.crea_type;
            data[1] = self.crea_combat_skill;
            data[2] = self.crea_magic_skill;
            data[3] = self.crea_stealth_skill;
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

            if let Some(s) = self.crea_sound_inherit {
                session.doc.set_sub(fid, b"CSCR", s.to_le_bytes().to_vec());
            }

            if let Some(w) = self.crea_weapon_list {
                session.doc.set_sub(fid, b"LNAM", w.to_le_bytes().to_vec());
            }
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
        if let Some(di) = self.actor_death_item {
            session.doc.set_sub(fid, b"INAM", di.to_le_bytes().to_vec());
        }
        if let Some(t) = self.actor_template {
            session.doc.set_sub(fid, b"TPLT", t.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"TPLT");
        }
        if let Some(cs) = self.actor_combat_style {
            session.doc.set_sub(fid, b"ZNAM", cs.to_le_bytes().to_vec());
        }

        let splo_buffers: Vec<Vec<u8>> = self
            .actor_splo
            .iter()
            .map(|fid| fid.to_le_bytes().to_vec())
            .collect();
        session.doc.set_sub_list(fid, b"SPLO", splo_buffers);

        let snam_buffers: Vec<Vec<u8>> = self
            .actor_factions
            .iter()
            .map(|(fac_fid, rank)| {
                let mut buf = vec![0u8; 8];
                buf[0..4].copy_from_slice(&fac_fid.to_le_bytes());
                buf[4] = *rank as u8;
                buf
            })
            .collect();
        session.doc.set_sub_list(fid, b"SNAM", snam_buffers);

        if self.sig == *b"CREA" {
            session
                .doc
                .set_sub(fid, b"RNAM", vec![self.crea_attack_reach]);
            session
                .doc
                .set_sub(fid, b"BNAM", self.actor_base_scale.to_le_bytes().to_vec());
            session
                .doc
                .set_sub(fid, b"TNAM", self.crea_turning_speed.to_le_bytes().to_vec());
            session
                .doc
                .set_sub(fid, b"WNAM", self.crea_foot_weight.to_le_bytes().to_vec());
            session.doc.set_sub(
                fid,
                b"NAM4",
                self.actor_impact_material.to_le_bytes().to_vec(),
            );
            session
                .doc
                .set_sub(fid, b"NAM5", self.actor_sound_level.to_le_bytes().to_vec());
            session
                .doc
                .set_sub(fid, b"EAMT", self.crea_unarmed_anim.to_le_bytes().to_vec());

            if let Some(eff) = self.actor_unarmed_effect {
                session
                    .doc
                    .set_sub(fid, b"EITM", eff.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"EITM");
            }
            if let Some(bptd) = self.actor_body_part_data {
                session
                    .doc
                    .set_sub(fid, b"PNAM", bptd.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"PNAM");
            }
            if let Some(ipds) = self.crea_impact_dataset {
                session
                    .doc
                    .set_sub(fid, b"CNAM", ipds.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"CNAM");
            }
        } else if self.sig == *b"NPC_" {
            session
                .doc
                .set_sub(fid, b"BNAM", self.actor_base_scale.to_le_bytes().to_vec());
            if let Some(eff) = self.actor_unarmed_effect {
                session
                    .doc
                    .set_sub(fid, b"EITM", eff.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"EITM");
            }
            if let Some(bptd) = self.actor_body_part_data {
                session
                    .doc
                    .set_sub(fid, b"PNAM", bptd.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, b"PNAM");
            }
        }

        let pkid_buffers: Vec<Vec<u8>> = self
            .actor_packages
            .iter()
            .map(|fid| fid.to_le_bytes().to_vec())
            .collect();
        session.doc.set_sub_list(fid, b"PKID", pkid_buffers);

        let cnto_buffers: Vec<Vec<u8>> = self
            .actor_inventory
            .iter()
            .map(|(item_fid, count)| {
                let mut buf = vec![0u8; 8];
                buf[0..4].copy_from_slice(&item_fid.to_le_bytes());
                buf[4..8].copy_from_slice(&count.to_le_bytes());
                buf
            })
            .collect();
        session.doc.set_sub_list(fid, b"CNTO", cnto_buffers);

        let mut sound_subs = Vec::new();
        for (stype, ssound, schance) in &self.crea_sounds {
            sound_subs.push((*b"CSDT", stype.to_le_bytes().to_vec()));
            sound_subs.push((*b"CSDI", ssound.to_le_bytes().to_vec()));
            sound_subs.push((*b"CSDC", vec![*schance]));
        }
        session
            .doc
            .set_sub_sequence(fid, vec![*b"CSDT", *b"CSDI", *b"CSDC"], sound_subs);

        if !self.crea_models.is_empty() {
            let mut nifz = Vec::new();
            for m in &self.crea_models {
                nifz.extend_from_slice(m.as_bytes());
                nifz.push(0);
            }
            nifz.push(0);
            session.doc.set_sub(fid, b"NIFZ", nifz);
        } else {
            session.doc.remove_sub(fid, b"NIFZ");
        }

        if !self.crea_animations.is_empty() {
            let mut kffz = Vec::new();
            for a in &self.crea_animations {
                kffz.extend_from_slice(a.as_bytes());
                kffz.push(0);
            }
            kffz.push(0);
            session.doc.set_sub(fid, b"KFFZ", kffz);
        } else {
            session.doc.remove_sub(fid, b"KFFZ");
        }

        if self.dest_health > 0 {
            let mut dest = vec![0u8; 8];
            dest[0..4].copy_from_slice(&self.dest_health.to_le_bytes());
            dest[4] = 1;
            session.doc.set_sub(fid, b"DEST", dest);
        } else {
            session.doc.remove_sub(fid, b"DEST");
        }
    }

    fn save_alch(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"DATA", self.alch_weight.to_le_bytes().to_vec());

        // `ENIT` is written back over the record's own bytes. The flag byte is
        // kept whole and the three named bits are written inside it, so bit 0
        // ("No Auto-Calc", set on 180 of the game's 189 records) survives.
        let original_enit = self.raw_subs.get(b"ENIT").cloned();
        let mut enit = original_enit.unwrap_or_else(|| vec![0u8; 20]);
        enit.resize(20, 0);
        enit[0..4].copy_from_slice(&self.alch_val.to_le_bytes());
        let mut flags = self.alch_enit_flags;
        flags &= !((1 << 1) | (1 << 2) | (1 << 3));
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
        if let Some(w) = self.alch_withdrawal {
            enit[8..12].copy_from_slice(&w.to_le_bytes());
        }
        enit[12..16].copy_from_slice(&self.alch_addict_chance.to_le_bytes());
        if let Some(s) = self.alch_sound {
            enit[16..20].copy_from_slice(&s.to_le_bytes());
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

        // `QNAM` is the ammo's abbreviation, a cstring. It used to be read as
        // a FormID, which turned "5mm" into a number and wrote those four
        // bytes back over the abbreviation.
        if !self.ammo_abbrev.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"QNAM", encode_zstring(&self.ammo_abbrev));
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
        if let Some(s) = self.door_sound_open {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
        }
    }

    fn save_acti(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if !self.acti_prompt.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"RNAM", encode_zstring(&self.acti_prompt));
        }
        if let Some(s) = self.acti_sound_loop {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
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
        if let Some(s) = self.term_sound_loop {
            session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
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

    /// Save a `FACT` (Faction) record.
    ///
    /// The relations and the rank collection are both runs of repeating
    /// subrecords, so they are written with list and sequence edits rather
    /// than one `set_sub` per signature: a per-signature edit would cluster
    /// every relation together and every rank title together, which the engine
    /// cannot read back.
    fn save_fact(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // DATA. 648 of the game's 682 FACT records carry four bytes and 34 carry
        // one, and the one-byte form has no flags 2 at all. The length the
        // record had is kept, so a save that changes nothing is byte-identical.
        let mut data = vec![self.fact_flags1];
        if self.fact_data_len >= 4 {
            data.push(self.fact_flags2);
            data.push(self.fact_data_unused[0]);
            data.push(self.fact_data_unused[1]);
        }
        session.doc.set_sub(fid, b"DATA", data);

        // CNAM is the spec's unnamed float32. The game writes 1.0 on the 34
        // records that have it, and the editor writes it only when the record
        // carries one, so an untouched record keeps its own bytes.
        if self.fact_has_cnam {
            session
                .doc
                .set_sub(fid, b"CNAM", self.fact_cnam.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"CNAM");
        }

        // WMI1 is the faction's reputation: a REPU record.
        match self.fact_reputation {
            Some(rep) => {
                session
                    .doc
                    .set_sub(fid, b"WMI1", rep.to_le_bytes().to_vec());
            }
            None => session.doc.remove_sub(fid, b"WMI1"),
        }

        // The relations, as one list of XNAM.
        let relations: Vec<Vec<u8>> = self.fact_relations.iter().map(|r| r.to_bytes()).collect();
        session.doc.set_sub_list(fid, b"XNAM", relations);

        // The rank collection, as one sequence: each entry is an RNAM and the
        // titles that belong to it, in that order.
        let mut ranks: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for r in &self.fact_ranks {
            ranks.push((*b"RNAM", r.number.to_le_bytes().to_vec()));
            if !r.male.trim().is_empty() {
                ranks.push((*b"MNAM", encode_zstring(&r.male)));
            }
            if !r.female.trim().is_empty() {
                ranks.push((*b"FNAM", encode_zstring(&r.female)));
            }
            if let Some(insignia) = &r.insignia {
                if !insignia.trim().is_empty() {
                    ranks.push((*b"INAM", encode_zstring(insignia)));
                }
            }
        }
        session
            .doc
            .set_sub_sequence(fid, vec![*b"RNAM", *b"MNAM", *b"FNAM", *b"INAM"], ranks);
    }

    fn save_qust(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // `DATA` is written back over the record's own bytes. Two of the 436
        // QUST records in FalloutNV.esm carry only two bytes, with no delay,
        // and a rebuild would grow them to eight.
        let original_data = self.raw_subs.get(b"DATA").cloned();
        let data_len = if self.qust_data_len >= 8 { 8 } else { 2 };
        let mut d = original_data.unwrap_or_else(|| vec![0u8; data_len]);
        d.resize(data_len, 0);
        let mut f = 0u8;
        if self.qust_start_enabled {
            f |= 1;
        }
        if self.qust_repeated_stages {
            f |= 8;
        }
        d[0] = f;
        d[1] = self.qust_priority;
        if data_len >= 8 {
            d[4..8].copy_from_slice(&self.qust_delay.to_le_bytes());
        }
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
        } else {
            session.doc.remove_sub(fid, b"LVLG");
        }

        let mut lvlo_buffers = Vec::new();
        for (lvl, r_fid, cnt) in &self.lvl_entries {
            let mut buf = vec![0u8; 12];
            buf[0..2].copy_from_slice(&lvl.to_le_bytes());
            buf[4..8].copy_from_slice(&r_fid.to_le_bytes());
            buf[8..10].copy_from_slice(&cnt.to_le_bytes());
            lvlo_buffers.push(buf);
        }
        session.doc.set_sub_list(fid, b"LVLO", lvlo_buffers);

        if !self.model.is_empty() {
            let mut modl = self.model.as_bytes().to_vec();
            modl.push(0);
            session.doc.set_sub(fid, b"MODL", modl);
        } else {
            session.doc.remove_sub(fid, b"MODL");
        }

        let mut obnd = vec![0u8; 12];
        for i in 0..3 {
            obnd[i * 2..i * 2 + 2].copy_from_slice(&self.obnd_min[i].to_le_bytes());
            obnd[6 + i * 2..6 + i * 2 + 2].copy_from_slice(&self.obnd_max[i].to_le_bytes());
        }
        session.doc.set_sub(fid, b"OBND", obnd);
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
        // 28 bytes is the spec's size and the only size in the game's masters;
        // the two trailing bytes are the struct's Unused field, kept zero.
        let mut d = vec![0u8; 28];
        for (i, skill) in self.clas_tag_skills.iter().enumerate() {
            let o = i * 4;
            d[o..o + 4].copy_from_slice(&skill.to_le_bytes());
        }
        // The flag word is edited in place, not rebuilt: bits 2, 3 and 4 are set
        // by the game's own classes and are not named by the spec, so they have
        // to survive the save unchanged.
        let mut f = self.clas_flags;
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
        d[20..24].copy_from_slice(&self.clas_services.to_le_bytes());
        d[24] = self.clas_teaches as u8;
        d[25] = self.clas_max_train;
        session.doc.set_sub(fid, b"DATA", d);
        session
            .doc
            .set_sub(fid, b"ATTR", self.clas_attributes.to_vec());
        // DESC is required by the spec. An empty description is a real state
        // (every class in the game's masters carries an empty one), so it is
        // written rather than removed.
        session
            .doc
            .set_sub(fid, b"DESC", encode_zstring(&self.clas_desc));
    }

    fn save_csty(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // Each subrecord is written back over its own bytes, so a range the spec
        // marks Unused — but the game fills with data — keeps its value.
        let raw = &self.raw_subs;
        let cstd = raw
            .get(b"CSTD")
            .map(|b| self.csty_std.write_into(b))
            .unwrap_or_else(|| self.csty_std.write_into(&[]));
        let csad = raw
            .get(b"CSAD")
            .map(|b| self.csty_adv.write_into(b))
            .unwrap_or_else(|| self.csty_adv.write_into(&[]));
        let cssd = raw
            .get(b"CSSD")
            .map(|b| self.csty_simple.write_into(b))
            .unwrap_or_else(|| self.csty_simple.write_into(&[]));
        session.doc.set_sub(fid, b"CSTD", cstd);
        session.doc.set_sub(fid, b"CSAD", csad);
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

    /// Save a `HAIR` (Hair) record.
    ///
    /// The model data collection's subrecords are written individually, in the
    /// order the game writes them: `MODL`, `MODB`, `MODT`, `MODS`, `MODD`.
    /// `MODT`'s texture hashes and `MODB`'s unknown bytes are copied through
    /// unchanged — the editor cannot author a file hash, and rewriting one
    /// would corrupt the mod.
    fn save_hair(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.hair_model;
        let sigs = model.instance_sigs();

        // The original instance's MODL is the common "Model Path" field the
        // header panel owns and has already written, so it is not written
        // again here. A numbered instance's model path has no such field, so
        // it is written from the collection.
        if model.instance > 0 && !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.modl, encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            if model.instance == 0 {
                session.doc.set_sub(fid, b"MODB", modb.to_vec());
            }
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, &sigs.modt, model.modt.clone());
        }
        if !model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.mods, write_mods(&model.mods));
        }
        if let (Some(modd), Some(sig)) = (model.modd, sigs.modd) {
            session.doc.set_sub(fid, &sig, vec![modd]);
        }

        // ICON is the hair's texture.
        if self.hair_icon.trim().is_empty() {
            session.doc.remove_sub(fid, b"ICON");
        } else {
            session
                .doc
                .set_sub(fid, b"ICON", encode_zstring(&self.hair_icon));
        }

        // DATA is a single u8 flag word. Every one of the game's 67 HAIR
        // records carries it, so it is always written.
        session.doc.set_sub(fid, b"DATA", vec![self.hair_flags]);
    }

    /// Save an `ADDN` (Addon Node) record.
    ///
    /// `DATA` is a four-byte node index and `DNAM` a four-byte master
    /// particle system cap plus two bytes the spec does not name. Both are
    /// written at the length the record already had: every one of the game's
    /// ADDN records carries four bytes of each, so a fixed four-byte write is
    /// byte-identical to what the record had.
    ///
    /// The model data collection's subrecords are written individually, in
    /// the order the game writes them. `MODT`'s texture hashes and `MODB`'s
    /// unknown bytes are copied through unchanged — the editor cannot author
    /// a file hash, and rewriting one would corrupt the mod.
    fn save_addn(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.addn_model;
        let sigs = model.instance_sigs();

        // The original instance's MODL is the common "Model Path" field the
        // header panel owns and has already written, so it is not written
        // again here. A numbered instance's model path has no such field, so
        // it is written from the collection.
        if model.instance > 0 && !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.modl, encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            if model.instance == 0 {
                session.doc.set_sub(fid, b"MODB", modb.to_vec());
            }
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, &sigs.modt, model.modt.clone());
        }
        if !model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.mods, write_mods(&model.mods));
        }
        if let (Some(modd), Some(sig)) = (model.modd, sigs.modd) {
            session.doc.set_sub(fid, &sig, vec![modd]);
        }

        // DATA: the node index.
        session
            .doc
            .set_sub(fid, b"DATA", self.addn_node_index.to_le_bytes().to_vec());

        // DNAM: the master particle system cap, and the two bytes the spec
        // does not name, carried through unchanged.
        let mut dnam = Vec::with_capacity(4);
        dnam.extend_from_slice(&self.addn_master_cap.to_le_bytes());
        dnam.extend_from_slice(&self.addn_dnam_unknown);
        session.doc.set_sub(fid, b"DNAM", dnam);
    }

    /// Save a `HDPT` (Head Part) record.
    ///
    /// The model data collection's subrecords are written individually, in the
    /// order the game writes them: `MODL`, `MODB`, `MODT`, `MODS`, `MODD`.
    /// `MODT`'s texture hashes and `MODB`'s unknown bytes are copied through
    /// unchanged — the editor cannot author a file hash, and rewriting one
    /// would corrupt the mod.
    ///
    /// `HNAM` is a repeated subrecord, so the extra parts are written as one
    /// list: a per-subrecord edit would collapse them onto the last one.
    fn save_hdpt(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.hdpt_model;
        let sigs = model.instance_sigs();

        // The original instance's MODL is the common "Model Path" field the
        // header panel owns and has already written, so it is not written
        // again here. A numbered instance's model path has no such field, so
        // it is written from the collection.
        if model.instance > 0 && !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.modl, encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            if model.instance == 0 {
                session.doc.set_sub(fid, b"MODB", modb.to_vec());
            }
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, &sigs.modt, model.modt.clone());
        }
        if !model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, &sigs.mods, write_mods(&model.mods));
        }
        if let (Some(modd), Some(sig)) = (model.modd, sigs.modd) {
            session.doc.set_sub(fid, &sig, vec![modd]);
        }

        // DATA is a single u8 flag word. Every one of the game's 61 HDPT
        // records carries it, so it is always written.
        session.doc.set_sub(fid, b"DATA", vec![self.hdpt_flags]);

        // HNAM is the list of extra parts, each a FormID to another HDPT.
        let parts: Vec<Vec<u8>> = self
            .hdpt_extra_parts
            .iter()
            .map(|id| id.to_le_bytes().to_vec())
            .collect();
        session.doc.set_sub_list(fid, b"HNAM", parts);
    }

    fn save_eyes(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // DATA is a single u8 flag word. Every EYES record in the game's
        // masters carries it, so it is always written.
        session.doc.set_sub(fid, b"DATA", vec![self.eyes_flags]);
        // ICON is optional: 11 of the 12 records carry one, and eyeReanimate
        // does not. An empty field means no texture, so it is removed rather
        // than written as an empty string.
        if self.eyes_icon.trim().is_empty() {
            session.doc.remove_sub(fid, b"ICON");
        } else {
            session
                .doc
                .set_sub(fid, b"ICON", encode_zstring(&self.eyes_icon));
        }
    }

    fn save_avif(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if !self.avif_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.avif_desc));
        } else {
            session.doc.remove_sub(fid, b"DESC");
        }
        if !self.avif_short_name.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"ANAM", encode_zstring(&self.avif_short_name));
        } else {
            session.doc.remove_sub(fid, b"ANAM");
        }
        if !self.avif_icon.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"ICON", encode_zstring(&self.avif_icon));
        } else {
            session.doc.remove_sub(fid, b"ICON");
        }
        if !self.avif_mico.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MICO", encode_zstring(&self.avif_mico));
        } else {
            session.doc.remove_sub(fid, b"MICO");
        }
    }

    /// Save a `BPTD` (Body Part Data) record.
    ///
    /// The whole part collection is rewritten as one sequence, because the parts
    /// are not a list of one signature: each is a run of eight different
    /// subrecords that must keep their order. A per-signature list edit would
    /// cluster every BPNN together and every NAM5 together, which the engine
    /// cannot read back into parts.
    fn save_bptd(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut subs: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for p in &self.bptd_parts {
            if let Some(name) = &p.name {
                subs.push((*b"BPTN", encode_zstring(name)));
            }
            subs.push((*b"BPNN", encode_zstring(&p.node)));
            subs.push((*b"BPNT", encode_zstring(&p.vats_target)));
            subs.push((*b"BPNI", encode_zstring(&p.ik_start_node)));
            subs.push((*b"BPND", p.data.to_bytes()));
            subs.push((*b"NAM1", encode_zstring(&p.limb_model)));
            subs.push((*b"NAM4", encode_zstring(&p.gore_target_bone)));
            subs.push((*b"NAM5", p.texture_hashes.clone()));
        }
        // RAGA sits after the last part in every record that has one [observed:
        // all 40 BPTD records carrying a RAGA put it last], so it is appended
        // here rather than as a separate edit.
        if let Some(ragdoll) = self.bptd_ragdoll {
            subs.push((*b"RAGA", ragdoll.to_le_bytes().to_vec()));
        }
        session.doc.set_sub_sequence(
            fid,
            vec![
                *b"BPTN", *b"BPNN", *b"BPNT", *b"BPNI", *b"BPND", *b"NAM1", *b"NAM4", *b"NAM5",
                *b"RAGA",
            ],
            subs,
        );
    }
}

/// Read a NUL-terminated string out of a subrecord payload.
fn zstring(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches('\0')
        .to_string()
}

fn encode_zstring(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if !v.ends_with(&[0]) {
        v.push(0);
    }
    v
}

/// Generate a clean, human-readable window title for any record.
pub fn window_title(sig: &[u8; 4], edid: &str, full_name: &str) -> String {
    let fallback = fnv_util::tag_str(sig);
    let type_name = match sig {
        b"CREA" => "Creature",
        b"NPC_" => "NPC",
        b"WEAP" => "Weapon",
        b"ARMO" => "Armor",
        b"AMMO" => "Ammo",
        b"ALCH" => "Ingestible",
        b"BOOK" => "Book",
        b"NOTE" => "Note",
        b"CONT" => "Container",
        b"DOOR" => "Door",
        b"ACTI" => "Activator",
        b"TERM" => "Terminal",
        b"LIGH" => "Light",
        b"FACT" => "Faction",
        b"QUST" => "Quest",
        b"PERK" => "Perk",
        b"CLAS" => "Class",
        b"CSTY" => "Combat Style",
        b"MGEF" => "Magic Effect",
        b"WTHR" => "Weather",
        b"WATR" => "Water",
        b"GLOB" => "Global",
        b"GMST" => "Game Setting",
        b"RACE" => "Race",
        b"LVLI" => "Leveled Item",
        b"LVLN" => "Leveled NPC",
        b"LVLC" => "Leveled Creature",
        b"ADDN" => "Addon Node",
        b"AVIF" => "Actor Value",
        b"EYES" => "Eyes",
        b"HAIR" => "Hair",
        b"HDPT" => "Head Part",
        b"BPTD" => "Body Part Data",
        _ => fallback.as_str(),
    };
    if !full_name.trim().is_empty() {
        format!("{type_name} - {edid} ({full_name})")
    } else if !edid.trim().is_empty() {
        format!("{type_name} - {edid}")
    } else {
        format!("{type_name} Editor")
    }
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

    if let Some((_fid, sig)) = session.editing_object {
        let current_title = window_title(&sig, &state.edid, &state.full_name);

        if let Some((_, mut win)) = win_opt {
            if win.title != current_title {
                win.title = current_title;
            }
        } else {
            let window = commands
                .spawn((
                    Window {
                        title: current_title,
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

/// `EYES` `DATA` Flags, the u8 flag word from fopdoc's EYES page.
const EYES_FLAGS: [(u8, &str); 3] = [(0x01, "Playable"), (0x02, "Not Male"), (0x04, "Not Female")];

/// `CLAS` `DATA.Teaches`, the skill enum from fopdoc's Skills page. It is an
/// int8 that starts at -1 ("None"), so the table is indexed from -1.
const CLAS_TEACHES: [(i8, &str); 15] = [
    (-1, "None"),
    (0, "Barter"),
    (1, "Big Guns"),
    (2, "Energy Weapons"),
    (3, "Explosives"),
    (4, "Lockpick"),
    (5, "Medicine"),
    (6, "Melee Weapons"),
    (7, "Repair"),
    (8, "Science"),
    (9, "Guns"),
    (10, "Sneak"),
    (11, "Speech"),
    (12, "Survival"),
    (13, "Unarmed"),
];

/// `CLAS` `DATA` Tag Skill. The spec links this to the Actor Values page, not
/// the Skills page, so the values are the actor-value enum: Barter is 32 and
/// Guns is 41, not 0 and 9. A class names up to four, and "None" (-1) is a real
/// value: 81 of the 296 tag-skill slots across the game's 74 classes are None,
/// so an empty slot must stay empty rather than defaulting to a skill.
const CLAS_TAG_SKILLS: [(i32, &str); 15] = [
    (-1, "None"),
    (32, "Barter"),
    (33, "Big Guns"),
    (34, "Energy Weapons"),
    (35, "Explosives"),
    (36, "Lockpick"),
    (37, "Medicine"),
    (38, "Melee Weapons"),
    (39, "Repair"),
    (40, "Science"),
    (41, "Guns"),
    (42, "Sneak"),
    (43, "Speech"),
    (44, "Survival"),
    (45, "Unarmed"),
];

/// `CLAS` `DATA` Buys/Sells and Services, the u32 flag word from fopdoc's
/// Services page. The bits fopdoc marks with a `?` are unnamed in the spec and
/// are left out, exactly as the schema leaves them out: naming a bit that is
/// not documented would invent behaviour.
const CLAS_SERVICES: [(u32, &str); 12] = [
    (0x0000_0001, "Weapons"),
    (0x0000_0002, "Armor"),
    (0x0000_0004, "Alcohol"),
    (0x0000_0008, "Books"),
    (0x0000_0010, "Food"),
    (0x0000_0020, "Chems"),
    (0x0000_0040, "Stimpaks"),
    (0x0000_0080, "Lights"),
    (0x0000_0400, "Miscellaneous"),
    (0x0000_4000, "Training"),
    (0x0001_0000, "Recharge"),
    (0x0002_0000, "Repair"),
];

/// `CSTD` Flags, the u16 flag word from fopdoc's CSTY flag table.
///
/// Bit 3 (0x0008) is marked `??` in the spec and is left out, exactly as the
/// schema leaves it out: naming a bit that is not documented would invent
/// behaviour. The word is kept whole when saving, so an undocumented bit that is
/// set survives.
const CSTY_FLAGS: [(u16, &str); 8] = [
    (0x0001, "Choose Attack Using % Chance"),
    (0x0002, "Melee Alert OK"),
    (0x0004, "Flee Based on Personal Survival"),
    (0x0010, "Ignore Threats"),
    (0x0020, "Ignore Damaging Self"),
    (0x0040, "Ignore Damaging Group"),
    (0x0080, "Ignore Damaging Spectators"),
    (0x0100, "Cannot Use Stealthboy"),
];

/// `CSSD` Weapon Restrictions, the enum from fopdoc's CSTY page.
const CSTY_WEAPON_RESTRICTIONS: [(u32, &str); 3] =
    [(0, "None"), (1, "Melee Only"), (2, "Ranged Only")];

/// The seven `CLAS` `ATTR` attributes, in the spec's order.
const CLAS_ATTRIBUTES: [&str; 7] = [
    "Strength",
    "Perception",
    "Endurance",
    "Charisma",
    "Intelligence",
    "Agility",
    "Luck",
];

/// `FACT` `DATA` Flags 1, the u8 word from fopdoc's FACT flag table.
///
/// The spec names bits 0, 1 and 2. The word is kept whole when saving, so a
/// bit the spec does not name survives an edit: the game's own FACT records
/// use only the three named bits, but a mod may set another.
const FACT_FLAGS1: [(u8, &str); 3] = [
    (0x01, "Hidden From PC"),
    (0x02, "Evil"),
    (0x04, "Special Combat"),
];

/// `FACT` `DATA` Flags 2, the u8 word from fopdoc's FACT flag table.
const FACT_FLAGS2: [(u8, &str); 2] = [(0x01, "Track Crime"), (0x02, "Allow Sell")];

/// `FACT` `XNAM` Group Combat Reaction, the enum from fopdoc's XNAM page.
const FACT_REACTIONS: [(u32, &str); 4] = [(0, "Neutral"), (1, "Enemy"), (2, "Ally"), (3, "Friend")];

/// `HAIR` `DATA` Flags, the u8 word from fopdoc's HAIR flag table.
///
/// The spec names bits 0, 1, 2 and 3. The word is kept whole when saving, so
/// a bit the spec does not name survives an edit.
const HAIR_FLAGS: [(u8, &str); 4] = [
    (0x01, "Playable"),
    (0x02, "Not Male"),
    (0x04, "Not Female"),
    (0x08, "Fixed"),
];

/// `HDPT` `DATA` Flags, the u8 word from fopdoc's HDPT flag table.
///
/// The spec names one bit, 0x01 "Playable". The word is kept whole when
/// saving, so a bit the spec does not name survives an edit.
const HDPT_FLAGS: [(u8, &str); 1] = [(0x01, "Playable")];

/// `MODD` FaceGen Model Flags, the u8 word from fopdoc's Model Data page.
const MODD_FLAGS: [(u8, &str); 4] = [
    (0x01, "Head"),
    (0x02, "Torso"),
    (0x04, "Right Hand"),
    (0x08, "Left Hand"),
];

/// Draw the Object Editor UI.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<6>>>,
    mut session: ResMut<crate::session::Session>,
    mut state: ResMut<ObjectEditorState>,
) {
    let Some((fid, sig)) = session.editing_object else {
        return;
    };

    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    if state.loaded_target != Some((fid, sig)) {
        state.load_from_session(&session, fid, sig);
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
                ui.add(egui::TextEdit::singleline(&mut state.full_name).desired_width(170.0));
                ui.separator();
                ui.checkbox(&mut state.quest_item, "Quest Item");
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
                b"WEAP" => draw_weap_ui(ui, &session, &mut state),
                b"ARMO" => draw_armo_ui(ui, &session, &mut state),
                b"NPC_" | b"CREA" => draw_actor_ui(ui, &session, &mut state),
                b"ALCH" => draw_alch_ui(ui, &mut state),
                b"AMMO" => draw_ammo_ui(ui, &mut state),
                b"CONT" => draw_cont_ui(ui, &mut state),
                b"DOOR" => draw_door_ui(ui, &mut state),
                b"ACTI" => draw_acti_ui(ui, &mut state),
                b"TERM" => draw_term_ui(ui, &mut state),
                b"LIGH" => draw_ligh_ui(ui, &mut state),
                b"BOOK" => draw_book_ui(ui, &mut state),
                b"FACT" => draw_fact_ui(ui, &session, &mut state),
                b"QUST" => draw_qust_ui(ui, &mut state),
                b"PERK" => draw_perk_ui(ui, &mut state),
                b"SPEL" | b"ENCH" => draw_spell_ui(ui, &mut state),
                b"EXPL" => draw_expl_ui(ui, &mut state),
                b"PROJ" => draw_proj_ui(ui, &mut state),
                b"SOUN" => draw_soun_ui(ui, &mut state),
                b"NOTE" => draw_note_ui(ui, &mut state),
                b"LVLI" | b"LVLN" | b"LVLC" => draw_lvl_ui(ui, &session, &mut state),
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
                b"ADDN" => draw_addn_ui(ui, &mut state),
                b"AVIF" => draw_avif_ui(ui, &mut state),
                b"EYES" => draw_eyes_ui(ui, &mut state),
                b"HAIR" => draw_hair_ui(ui, &session, &mut state),
                b"HDPT" => draw_hdpt_ui(ui, &session, &mut state),
                b"BPTD" => draw_bptd_ui(ui, &session, &mut state),
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

fn record_dropdown(
    ui: &mut egui::Ui,
    id_salt: &str,
    items: &[(u32, String)],
    opt: &mut Option<u32>,
) {
    let cur_name = match opt {
        Some(fid) => items
            .iter()
            .find(|(id, _)| *id == *fid)
            .map(|(_, name)| name.as_str())
            .unwrap_or("Custom / FormID"),
        None => "NONE",
    };

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(cur_name)
            .width(200.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(opt.is_none(), "NONE").clicked() {
                    *opt = None;
                }
                for (fid, name) in items {
                    let is_sel = *opt == Some(*fid);
                    if ui
                        .selectable_label(is_sel, format!("{name} ({fid:08X})"))
                        .clicked()
                    {
                        *opt = Some(*fid);
                    }
                }
            });
        formid_field(ui, &format!("{id_salt}_fid"), opt);
    });
}

fn form_list_dropdown(
    ui: &mut egui::Ui,
    id_salt: &str,
    session: &crate::session::Session,
    opt: &mut Option<u32>,
) {
    let form_lists = session.form_lists();
    record_dropdown(ui, id_salt, &form_lists, opt);
}

fn draw_weap_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
            ui.label("Repair List (FormList):");
            form_list_dropdown(ui, "weap_repl", session, &mut state.weap_repair_list);
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

fn draw_armo_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
            ui.separator();
            ui.label("Repair List (FormList):");
            form_list_dropdown(ui, "armo_repl", session, &mut state.armo_repair_list);
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

const CREATURE_TYPES: [&str; 8] = [
    "Animal",
    "Mutated Animal",
    "Mutated Insect",
    "Abomination",
    "Super Mutant",
    "Feral Ghoul",
    "Robot",
    "Giant",
];

const IMPACT_MATERIALS: [&str; 12] = [
    "Stone",
    "Dirt",
    "Grass",
    "Glass",
    "Metal",
    "Wood",
    "Organic",
    "Cloth",
    "Water",
    "Hollow Metal",
    "Organic Bug",
    "Organic Glow",
];

const SOUND_LEVELS: [&str; 3] = ["Loud", "Normal", "Silent"];

const MOOD_NAMES: [&str; 8] = [
    "Neutral", "Afraid", "Annoyed", "Cocky", "Drugged", "Pleasant", "Angry", "Sad",
];

const ASSISTANCE_NAMES: [&str; 3] = ["Helps Nobody", "Helps Allies", "Helps Friends and Allies"];

/// `BPND` Flags, one bit per value in fopdoc's BPTD flag table.
const BPTD_FLAGS: [(u8, &str); 7] = [
    (0x01, "Severable"),
    (0x02, "IK Data"),
    (0x04, "IK Data - Biped Data"),
    (0x08, "Explodable"),
    (0x10, "IK Data - Is Head"),
    (0x20, "IK Data - Headtracking"),
    (0x40, "To Hit Chance - Absolute"),
];

/// `BPND` Part Type, the BodyLocation enum in fopdoc's BPTD part-type table.
const BPTD_PART_TYPES: [&str; 15] = [
    "Torso",
    "Head 1",
    "Head 2",
    "Left Arm 1",
    "Left Arm 2",
    "Right Arm 1",
    "Right Arm 2",
    "Left Leg 1",
    "Left Leg 2",
    "Left Leg 3",
    "Right Leg 1",
    "Right Leg 2",
    "Right Leg 3",
    "Brain",
    "Weapon",
];

/// `BPND` Actor Value — the engine's actor-value enum, of which a body part names
/// the one whose condition the part tracks (e.g. a head reports Perception
/// Condition). Only the range a body part can actually use is offered.
const BPTD_ACTOR_VALUES: [(i8, &str); 30] = [
    (-1, "None"),
    (0, "Aggression"),
    (1, "Confidence"),
    (2, "Energy"),
    (3, "Responsibility"),
    (4, "Mood"),
    (5, "Strength"),
    (6, "Perception"),
    (7, "Endurance"),
    (8, "Charisma"),
    (9, "Intelligence"),
    (10, "Agility"),
    (11, "Luck"),
    (12, "Action Points"),
    (13, "Carry Weight"),
    (14, "Critical Chance"),
    (15, "Heal Rate"),
    (16, "Health"),
    (17, "Melee Damage"),
    (18, "Damage Resistance"),
    (19, "Poison Resistance"),
    (20, "Rad Resistance"),
    (21, "Speed Multiplier"),
    (22, "Fatigue"),
    (23, "Karma"),
    (24, "XP"),
    (25, "Perception Condition"),
    (26, "Endurance Condition"),
    (27, "Left Attack Condition"),
    (28, "Right Attack Condition"),
];

const CSDT_TYPES: [&str; 22] = [
    "Left Foot",
    "Right Foot",
    "Left Back Foot",
    "Right Back Foot",
    "Idle",
    "Aware",
    "Attack",
    "Hit",
    "Death",
    "Weapon",
    "Movement Loop",
    "Conscious Loop",
    "Auxiliary 1",
    "Auxiliary 2",
    "Auxiliary 3",
    "Auxiliary 4",
    "Auxiliary 5",
    "Auxiliary 6",
    "Auxiliary 7",
    "Auxiliary 8",
    "Jump",
    "Play Random / Loop",
];

const NPC_SKILL_NAMES: [&str; 14] = [
    "Barter",
    "Big Guns",
    "Energy Weapons",
    "Explosives",
    "Lockpick",
    "Medicine",
    "Melee Weapons",
    "Repair",
    "Science",
    "Guns",
    "Sneak",
    "Speech",
    "Survival",
    "Unarmed",
];

fn draw_actor_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    if state.sig == *b"CREA" {
        ui.group(|ui| {
            ui.strong("Creature Classification & Skills");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Creature Type:");
                let cur_type = (state.crea_type as usize).min(CREATURE_TYPES.len() - 1);
                let cur_lbl = CREATURE_TYPES.get(cur_type).copied().unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("crea_type")
                    .selected_text(cur_lbl)
                    .show_ui(ui, |ui| {
                        for (i, &name) in CREATURE_TYPES.iter().enumerate() {
                            ui.selectable_value(&mut state.crea_type, i as u8, name);
                        }
                    });
                ui.separator();
                ui.label("Combat Skill:");
                ui.add(egui::DragValue::new(&mut state.crea_combat_skill).range(0..=100));
                ui.label("Magic Skill:");
                ui.add(egui::DragValue::new(&mut state.crea_magic_skill).range(0..=100));
                ui.label("Stealth Skill:");
                ui.add(egui::DragValue::new(&mut state.crea_stealth_skill).range(0..=100));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Weapon List (FormList):");
                form_list_dropdown(ui, "crea_weap_list", session, &mut state.crea_weapon_list);
                ui.separator();
                ui.label("Inherits Sounds From (CSCR FormID):");
                formid_field(ui, "crea_sound_inherit", &mut state.crea_sound_inherit);
            });
        });
        ui.add_space(6.0);
    }

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
            if state.sig == *b"CREA" {
                ui.label("Damage:");
                ui.add(egui::DragValue::new(&mut state.actor_damage).range(0..=10000));
            }
            ui.label("Fatigue:");
            ui.add(egui::DragValue::new(&mut state.actor_fatigue).range(0..=50000));
            ui.label("Speed Mult %:");
            ui.add(egui::DragValue::new(&mut state.actor_speed_mult).range(10..=1000));
            ui.separator();
            ui.label("Alignment:");
            let cur_align = if state.actor_karma >= 750.0 {
                "Very Good"
            } else if state.actor_karma >= 250.0 {
                "Good"
            } else if state.actor_karma > -250.0 {
                "Neutral"
            } else if state.actor_karma > -750.0 {
                "Evil"
            } else {
                "Very Evil"
            };
            egui::ComboBox::from_id_salt("actor_alignment")
                .selected_text(cur_align)
                .show_ui(ui, |ui| {
                    const ALIGN_PRESETS: [(&str, f32); 5] = [
                        ("Very Good", 1000.0),
                        ("Good", 500.0),
                        ("Neutral", 0.0),
                        ("Evil", -500.0),
                        ("Very Evil", -1000.0),
                    ];
                    for (label, k_val) in ALIGN_PRESETS {
                        if ui.selectable_label(cur_align == label, label).clicked() {
                            state.actor_karma = k_val;
                        }
                    }
                });
            ui.label("Karma:");
            ui.add(
                egui::DragValue::new(&mut state.actor_karma)
                    .speed(10.0)
                    .range(-1000.0..=1000.0),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Barter Gold:");
            ui.add(egui::DragValue::new(&mut state.actor_barter_gold).range(0..=65535));
            ui.separator();
            ui.label("Base Disposition:");
            ui.add(egui::DragValue::new(&mut state.actor_disp_base).range(-100..=100));
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

    if state.sig == *b"NPC_" {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("NPC Skills & Offsets (DNAM)");
            ui.add_space(4.0);
            egui::Grid::new("npc_skills_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Skill").strong());
                    ui.label(egui::RichText::new("Base Value").strong());
                    ui.label(egui::RichText::new("Offset").strong());
                    ui.label(egui::RichText::new("Skill").strong());
                    ui.label(egui::RichText::new("Base Value").strong());
                    ui.label(egui::RichText::new("Offset").strong());
                    ui.end_row();

                    for row in 0..7 {
                        // Left column
                        let idx1 = row;
                        ui.label(NPC_SKILL_NAMES[idx1]);
                        ui.add(egui::DragValue::new(&mut state.npc_skills[idx1]).range(0..=100));
                        ui.add(egui::DragValue::new(&mut state.npc_skill_offsets[idx1]));

                        // Right column
                        let idx2 = row + 7;
                        ui.label(NPC_SKILL_NAMES[idx2]);
                        ui.add(egui::DragValue::new(&mut state.npc_skills[idx2]).range(0..=100));
                        ui.add(egui::DragValue::new(&mut state.npc_skill_offsets[idx2]));
                        ui.end_row();
                    }
                });
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Appearance, Hair & Head Parts");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Height (NAM6):");
                ui.add(
                    egui::DragValue::new(&mut state.npc_height)
                        .speed(0.01)
                        .range(0.1..=2.5),
                );
                ui.separator();
                ui.label("Weight (NAM7):");
                ui.add(
                    egui::DragValue::new(&mut state.npc_weight)
                        .speed(0.01)
                        .range(0.1..=2.5),
                );
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Hair (HNAM):");
                let hairs = session.records_by_sig(*b"HAIR");
                record_dropdown(ui, "npc_hair", &hairs, &mut state.npc_hair);
                ui.separator();
                ui.label("Hair Length (LNAM):");
                ui.add(
                    egui::DragValue::new(&mut state.npc_hair_length)
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Eyes (ENAM):");
                let eyes = session.records_by_sig(*b"EYES");
                record_dropdown(ui, "npc_eyes", &eyes, &mut state.npc_eyes);
                ui.separator();
                ui.label("Hair Color (RGBA):");
                ui.color_edit_button_srgba_unmultiplied(&mut state.npc_hair_color);
            });

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Head Parts (PNAM):").underline());
            let hdpts_all = session.records_by_sig(*b"HDPT");
            let mut remove_hp = None;
            for (idx, &fid) in state.npc_head_parts.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").clicked() {
                        remove_hp = Some(idx);
                    }
                    let name = hdpts_all
                        .iter()
                        .find(|(id, _)| *id == fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Head Part");
                    ui.label(format!("• {name} ({fid:08X})"));
                });
            }
            if let Some(idx) = remove_hp {
                state.npc_head_parts.remove(idx);
            }
            ui.horizontal(|ui| {
                record_dropdown(ui, "npc_add_hdpt", &hdpts_all, &mut state.head_part_to_add);
                if ui.button("+ Add Head Part").clicked() {
                    if let Some(fid) = state.head_part_to_add {
                        if !state.npc_head_parts.contains(&fid) {
                            state.npc_head_parts.push(fid);
                        }
                    }
                }
            });
        });
    }

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(if state.sig == *b"CREA" {
            "Creature Flags, Movement & Traits"
        } else {
            "Actor Flags & Traits"
        });
        ui.add_space(4.0);

        ui.label(egui::RichText::new("Flags:").underline());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut state.actor_essential, "Essential");
            ui.checkbox(&mut state.actor_respawn, "Respawn");
            ui.checkbox(&mut state.quest_item, "Quest Item");
            ui.checkbox(&mut state.actor_invulnerable, "Invulnerable");
            ui.checkbox(&mut state.actor_is_ghost, "Is Ghost");
            ui.checkbox(&mut state.actor_no_knockdowns, "No Knockdowns");
            ui.checkbox(&mut state.actor_no_low_level_proc, "No Level Processing");
            ui.checkbox(&mut state.actor_no_vats_melee, "No VATS Melee");
            ui.checkbox(&mut state.actor_allow_pickpocket, "Allow Pickpocket");
            if state.sig == *b"CREA" {
                ui.checkbox(&mut state.actor_allow_pc_dialogue, "Allow PC Dialogue");
                ui.checkbox(&mut state.actor_cant_open_doors, "Can't Open Doors");
                ui.checkbox(&mut state.crea_no_head_track, "No Head-Track");
            } else {
                ui.checkbox(&mut state.actor_female, "Female");
            }
        });

        if state.sig == *b"CREA" {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Movement & Combat:").underline());
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.crea_biped, "Biped");
                ui.checkbox(&mut state.crea_walks, "Walks");
                ui.checkbox(&mut state.crea_swims, "Swims");
                ui.checkbox(&mut state.crea_flies, "Flies");
                ui.checkbox(&mut state.crea_weapon_shield, "Weapon & Shield");
            });
        }

        if state.sig == *b"NPC_" {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Race:");
                let races = session.records_by_sig(*b"RACE");
                record_dropdown(ui, "act_race", &races, &mut state.actor_race);
                ui.separator();
                ui.label("Class:");
                let classes = session.records_by_sig(*b"CLAS");
                record_dropdown(ui, "act_class", &classes, &mut state.actor_class);
            });
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(if state.sig == *b"CREA" {
            "Physical & Combat Parameters (CREA)"
        } else {
            "Physical & Combat Parameters"
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if state.sig == *b"CREA" {
                ui.label("Attack Reach (RNAM):");
                ui.add(egui::DragValue::new(&mut state.crea_attack_reach).range(0..=255));
                ui.label("Turning Speed (TNAM):");
                ui.add(
                    egui::DragValue::new(&mut state.crea_turning_speed)
                        .speed(1.0)
                        .range(0.0..=500.0),
                );
                ui.label("Foot Weight (WNAM):");
                ui.add(
                    egui::DragValue::new(&mut state.crea_foot_weight)
                        .speed(1.0)
                        .range(0.0..=1000.0),
                );
            }
            ui.label("Base Scale (BNAM):");
            ui.add(
                egui::DragValue::new(&mut state.actor_base_scale)
                    .speed(0.05)
                    .range(0.1..=10.0),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Impact Material (NAM4):");
            let cur_mat = (state.actor_impact_material as usize).min(IMPACT_MATERIALS.len() - 1);
            let mat_lbl = IMPACT_MATERIALS.get(cur_mat).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("actor_mat")
                .selected_text(mat_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in IMPACT_MATERIALS.iter().enumerate() {
                        ui.selectable_value(&mut state.actor_impact_material, i as u32, name);
                    }
                });

            ui.separator();
            ui.label("Sound Level (NAM5):");
            let cur_snd = (state.actor_sound_level as usize).min(SOUND_LEVELS.len() - 1);
            let snd_lbl = SOUND_LEVELS.get(cur_snd).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("actor_snd_lvl")
                .selected_text(snd_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in SOUND_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut state.actor_sound_level, i as u32, name);
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Body Part Data (PNAM):");
            let bptds = session.records_by_sig(*b"BPTD");
            record_dropdown(ui, "actor_pnam", &bptds, &mut state.actor_body_part_data);
            if state.sig == *b"CREA" {
                ui.separator();
                ui.label("Impact Dataset (CNAM):");
                let ipds = session.records_by_sig(*b"IPDS");
                record_dropdown(ui, "crea_cnam", &ipds, &mut state.crea_impact_dataset);
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Unarmed Attack Effect (EITM):");
            let spells = session.spells_and_enchantments();
            record_dropdown(ui, "actor_eitm", &spells, &mut state.actor_unarmed_effect);

            if state.sig == *b"CREA" {
                ui.separator();
                ui.label("Unarmed Attack Anim (EAMT):");
                ui.add(egui::DragValue::new(&mut state.crea_unarmed_anim).range(0..=255));
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("AI Behavior, Combat & Voice");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Aggression (0-3):");
            ui.add(egui::DragValue::new(&mut state.actor_aggression).range(0..=3));
            ui.label("Confidence (0-4):");
            ui.add(egui::DragValue::new(&mut state.actor_confidence).range(0..=4));
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Energy (0-100):");
            ui.add(egui::DragValue::new(&mut state.actor_energy).range(0..=100));
            ui.label("Responsibility (0-100):");
            ui.add(egui::DragValue::new(&mut state.actor_resp).range(0..=100));
            ui.label("Aggro Radius:");
            ui.add(
                egui::DragValue::new(&mut state.actor_aggro_radius)
                    .speed(50.0)
                    .range(0..=100000),
            );
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Mood:");
            let cur_mood = (state.actor_mood as usize).min(MOOD_NAMES.len() - 1);
            let mood_lbl = MOOD_NAMES.get(cur_mood).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("actor_mood")
                .selected_text(mood_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in MOOD_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.actor_mood, i as u8, name);
                    }
                });

            ui.separator();
            ui.label("Assistance:");
            let cur_ast = (state.actor_assistance.max(0) as usize).min(ASSISTANCE_NAMES.len() - 1);
            let ast_lbl = ASSISTANCE_NAMES.get(cur_ast).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("actor_assist")
                .selected_text(ast_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in ASSISTANCE_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.actor_assistance, i as i8, name);
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Template (TPLT):");
            let templates = if state.sig == *b"CREA" {
                session.creature_templates()
            } else {
                session.npc_templates()
            };
            record_dropdown(ui, "act_tplt", &templates, &mut state.actor_template);
        });

        if state.actor_template.is_some() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Template Inheritance Flags:").underline());
            ui.horizontal_wrapped(|ui| {
                const TPLT_FLAGS: [(u16, &str); 10] = [
                    (0x0001, "Use Traits"),
                    (0x0002, "Use Stats"),
                    (0x0004, "Use Factions"),
                    (0x0008, "Use Spells/Effects"),
                    (0x0010, "Use AI Data"),
                    (0x0020, "Use AI Packages"),
                    (0x0040, "Use Model/Animation"),
                    (0x0080, "Use Base Data"),
                    (0x0100, "Use Inventory"),
                    (0x0200, "Use Script"),
                ];
                for (bit, name) in TPLT_FLAGS {
                    let mut set = (state.actor_template_flags & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            state.actor_template_flags |= bit;
                        } else {
                            state.actor_template_flags &= !bit;
                        }
                    }
                }
            });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Voice Type:");
            let voices = session.records_by_sig(*b"VTYP");
            record_dropdown(ui, "act_voice", &voices, &mut state.actor_voice);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Combat Style:");
            let styles = session.records_by_sig(*b"CSTY");
            record_dropdown(ui, "act_cstyle", &styles, &mut state.actor_combat_style);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Death Item (LVLI / Item):");
            let death_items = session.records_by_sig(*b"LVLI");
            record_dropdown(
                ui,
                "act_death_item",
                &death_items,
                &mut state.actor_death_item,
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Actor Effects & Spells (SPLO)");
        ui.add_space(4.0);

        let spells_all = session.spells_and_enchantments();

        if state.actor_splo.is_empty() {
            ui.label(egui::RichText::new("No active effects/spells.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, &fid) in state.actor_splo.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this effect").clicked() {
                        remove_idx = Some(idx);
                    }
                    let name = spells_all
                        .iter()
                        .find(|(id, _)| *id == fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Effect");
                    ui.label(format!("{name} ({fid:08X})"));
                });
            }
            if let Some(idx) = remove_idx {
                state.actor_splo.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Effect:");
            record_dropdown(ui, "actor_add_splo", &spells_all, &mut state.splo_to_add);
            if ui.button("+ Add").clicked() {
                if let Some(fid) = state.splo_to_add {
                    if !state.actor_splo.contains(&fid) {
                        state.actor_splo.push(fid);
                    }
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Faction Memberships (SNAM)");
        ui.add_space(4.0);

        let factions_all = session.records_by_sig(*b"FACT");

        if state.actor_factions.is_empty() {
            ui.label(egui::RichText::new("No faction memberships.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, (fid, rank)) in state.actor_factions.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this faction")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    let name = factions_all
                        .iter()
                        .find(|(id, _)| *id == *fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Faction");
                    ui.label(format!("• {name} ({fid:08X})"));
                    ui.separator();
                    ui.label("Rank:");
                    ui.add(egui::DragValue::new(rank).range(-1..=127));
                });
            }
            if let Some(idx) = remove_idx {
                state.actor_factions.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Faction:");
            record_dropdown(
                ui,
                "actor_add_fact",
                &factions_all,
                &mut state.faction_to_add,
            );
            ui.label("Rank:");
            ui.add(egui::DragValue::new(&mut state.faction_rank_to_add).range(-1..=127));
            if ui.button("+ Add").clicked() {
                if let Some(fid) = state.faction_to_add {
                    if !state.actor_factions.iter().any(|(f, _)| *f == fid) {
                        state.actor_factions.push((fid, state.faction_rank_to_add));
                    }
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("AI Packages (PKID)");
        ui.add_space(4.0);

        let packages_all = session.records_by_sig(*b"PACK");

        if state.actor_packages.is_empty() {
            ui.label(egui::RichText::new("No assigned AI packages.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, &fid) in state.actor_packages.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this package")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    let name = packages_all
                        .iter()
                        .find(|(id, _)| *id == fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Package");
                    ui.label(format!("• {name} ({fid:08X})"));
                });
            }
            if let Some(idx) = remove_idx {
                state.actor_packages.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Package:");
            record_dropdown(
                ui,
                "actor_add_pkg",
                &packages_all,
                &mut state.package_to_add,
            );
            if ui.button("+ Add").clicked() {
                if let Some(fid) = state.package_to_add {
                    if !state.actor_packages.contains(&fid) {
                        state.actor_packages.push(fid);
                    }
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Inventory Items (CNTO)");
        ui.add_space(4.0);

        let items_all = session.inventory_items();

        if state.actor_inventory.is_empty() {
            ui.label(egui::RichText::new("Inventory is empty.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, (fid, count)) in state.actor_inventory.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this item").clicked() {
                        remove_idx = Some(idx);
                    }
                    let name = items_all
                        .iter()
                        .find(|(id, _)| *id == *fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Item");
                    ui.label(format!("• {name} ({fid:08X})"));
                    ui.separator();
                    ui.label("Count:");
                    ui.add(egui::DragValue::new(count).range(1..=100000));
                });
            }
            if let Some(idx) = remove_idx {
                state.actor_inventory.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Item:");
            record_dropdown(ui, "actor_add_item", &items_all, &mut state.item_to_add);
            ui.label("Count:");
            ui.add(egui::DragValue::new(&mut state.item_count_to_add).range(1..=10000));
            if ui.button("+ Add").clicked() {
                if let Some(fid) = state.item_to_add {
                    if let Some(existing) =
                        state.actor_inventory.iter_mut().find(|(f, _)| *f == fid)
                    {
                        existing.1 += state.item_count_to_add.max(1);
                    } else {
                        state
                            .actor_inventory
                            .push((fid, state.item_count_to_add.max(1)));
                    }
                }
            }
        });
    });

    if state.sig == *b"CREA" {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Sound Type Overrides (CSDT / CSDI / CSDC)");
            ui.add_space(4.0);

            let sounds_all = session.records_by_sig(*b"SOUN");

            if state.crea_sounds.is_empty() {
                ui.label(
                    egui::RichText::new("No sound overrides (inheriting all base sounds).")
                        .italics(),
                );
            } else {
                let mut remove_idx = None;
                for (idx, (stype, sfid, chance)) in state.crea_sounds.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .button("✖")
                            .on_hover_text("Remove this sound override")
                            .clicked()
                        {
                            remove_idx = Some(idx);
                        }
                        let t_name = CSDT_TYPES.get(*stype as usize).copied().unwrap_or("Custom");
                        let s_name = sounds_all
                            .iter()
                            .find(|(id, _)| *id == *sfid)
                            .map(|(_, n)| n.as_str())
                            .unwrap_or("Custom Sound");
                        ui.label(format!("• {t_name}: {s_name} ({sfid:08X})"));
                        ui.separator();
                        ui.label("Chance %:");
                        ui.add(egui::DragValue::new(chance).range(0..=100));
                    });
                }
                if let Some(idx) = remove_idx {
                    state.crea_sounds.remove(idx);
                }
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Add Sound:");
                let cur_type = (state.sound_type_to_add as usize).min(CSDT_TYPES.len() - 1);
                let cur_lbl = CSDT_TYPES.get(cur_type).copied().unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("crea_add_csdt")
                    .selected_text(cur_lbl)
                    .show_ui(ui, |ui| {
                        for (i, &name) in CSDT_TYPES.iter().enumerate() {
                            ui.selectable_value(&mut state.sound_type_to_add, i as u32, name);
                        }
                    });
                record_dropdown(
                    ui,
                    "crea_add_csdi",
                    &sounds_all,
                    &mut state.sound_fid_to_add,
                );
                ui.label("Chance %:");
                ui.add(egui::DragValue::new(&mut state.sound_chance_to_add).range(0..=100));
                if ui.button("+ Add").clicked() {
                    if let Some(fid) = state.sound_fid_to_add {
                        state.crea_sounds.push((
                            state.sound_type_to_add,
                            fid,
                            state.sound_chance_to_add.max(1),
                        ));
                    }
                }
            });
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Model List & Animations (NIFZ / KFFZ)");
            ui.add_space(4.0);

            ui.label(egui::RichText::new("Alternate Models (NIFZ):").underline());
            let mut remove_m = None;
            for (idx, m) in state.crea_models.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").clicked() {
                        remove_m = Some(idx);
                    }
                    ui.label(format!("• {m}"));
                });
            }
            if let Some(idx) = remove_m {
                state.crea_models.remove(idx);
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.model_to_add)
                        .hint_text("model.nif")
                        .desired_width(180.0),
                );
                if ui.button("+ Add Model").clicked() {
                    let trimmed = state.model_to_add.trim();
                    if !trimmed.is_empty() && !state.crea_models.contains(&trimmed.to_string()) {
                        state.crea_models.push(trimmed.to_string());
                        state.model_to_add.clear();
                    }
                }
            });

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Animations (KFFZ):").underline());
            let mut remove_a = None;
            for (idx, a) in state.crea_animations.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").clicked() {
                        remove_a = Some(idx);
                    }
                    ui.label(format!("• {a}"));
                });
            }
            if let Some(idx) = remove_a {
                state.crea_animations.remove(idx);
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.anim_to_add)
                        .hint_text("anim.kf")
                        .desired_width(180.0),
                );
                if ui.button("+ Add Animation").clicked() {
                    let trimmed = state.anim_to_add.trim();
                    if !trimmed.is_empty() && !state.crea_animations.contains(&trimmed.to_string())
                    {
                        state.crea_animations.push(trimmed.to_string());
                        state.anim_to_add.clear();
                    }
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Destruction Health Stage (DEST):");
                ui.add(egui::DragValue::new(&mut state.dest_health).range(0..=50000));
            });
        });
    }

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Object Bounds (OBND)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Min X:");
            ui.add(egui::DragValue::new(&mut state.obnd_min[0]));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut state.obnd_min[1]));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut state.obnd_min[2]));
            ui.separator();
            ui.label("Max X:");
            ui.add(egui::DragValue::new(&mut state.obnd_max[0]));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut state.obnd_max[1]));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut state.obnd_max[2]));
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

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Withdrawal Effect (SPEL):");
            formid_field(ui, "alch_withdrawal", &mut state.alch_withdrawal);
            ui.label("Consume Sound (SOUN):");
            formid_field(ui, "alch_sound", &mut state.alch_sound);
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
            ui.label("Abbreviation (QNAM):");
            ui.add(egui::TextEdit::singleline(&mut state.ammo_abbrev).desired_width(120.0));
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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Open Sound (SNAM FormID):");
            formid_field(ui, "door_sound_open", &mut state.door_sound_open);
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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Looping Sound (SNAM FormID):");
            formid_field(ui, "acti_sound_loop", &mut state.acti_sound_loop);
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
        ui.horizontal(|ui| {
            ui.label("Looping Sound (SNAM FormID):");
            formid_field(ui, "term_sound_loop", &mut state.term_sound_loop);
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

fn draw_fact_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Faction Flags (DATA)");
        ui.add_space(4.0);
        // The checkboxes write the named bits of the whole word, so the two
        // controls cannot disagree, and a bit the spec does not name survives.
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in FACT_FLAGS1 {
                let mut set = (state.fact_flags1 & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.fact_flags1 |= bit;
                    } else {
                        state.fact_flags1 &= !bit;
                    }
                }
            }
        });
        // The second byte of DATA holds flags 2. 34 of the game's FACT records
        // have a one-byte DATA with no flags 2 at all, so the row is offered
        // only when the record carries the byte — and a one-byte record can be
        // extended to the full four.
        let mut has_flags2 = state.fact_data_len >= 2;
        if ui
            .checkbox(&mut has_flags2, "DATA carries a Flags 2 byte")
            .on_hover_text(
                "Extending a one-byte DATA to four bytes adds the flags 2 byte and the \
                 two unused bytes the spec lists.",
            )
            .changed()
        {
            state.fact_data_len = if has_flags2 { 4 } else { 1 };
        }
        if has_flags2 {
            ui.horizontal_wrapped(|ui| {
                for (bit, name) in FACT_FLAGS2 {
                    let mut set = (state.fact_flags2 & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            state.fact_flags2 |= bit;
                        } else {
                            state.fact_flags2 &= !bit;
                        }
                    }
                }
            });
        } else {
            ui.label(
                egui::RichText::new(
                    "This record's DATA carries only Flags 1: it has no Flags 2 byte.",
                )
                .italics(),
            );
        }
        ui.add_space(4.0);
        // The raw words, because the bits fopdoc marks with a `?` are real and
        // a mod may set them; the editor keeps them without naming them.
        ui.horizontal(|ui| {
            ui.label("Raw flags 1:");
            ui.add(
                egui::DragValue::new(&mut state.fact_flags1)
                    .range(0..=u8::MAX as i64)
                    .speed(1.0),
            );
            ui.label("Raw flags 2:");
            ui.add(
                egui::DragValue::new(&mut state.fact_flags2)
                    .range(0..=u8::MAX as i64)
                    .speed(1.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Reputation (WMI1)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let repus = session.records_by_sig(*b"REPU");
            record_dropdown(ui, "fact_rep", &repus, &mut state.fact_reputation);
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Unused CNAM (float32)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut has = state.fact_has_cnam;
            if ui.checkbox(&mut has, "Record has a CNAM").changed() {
                state.fact_has_cnam = has;
            }
            ui.label("Value:");
            ui.add(egui::DragValue::new(&mut state.fact_cnam).speed(0.1));
        });
    });

    // --- Relations (XNAM) ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Relations (XNAM) — {}", state.fact_relations.len()));
        ui.add_space(4.0);

        let factions_all = session.records_by_sig(*b"FACT");

        if state.fact_relations.is_empty() {
            ui.label(egui::RichText::new("No faction relations.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, rel) in state.fact_relations.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this relation")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    let name = rel
                        .faction
                        .and_then(|id| {
                            factions_all
                                .iter()
                                .find(|(fid, _)| *fid == id)
                                .map(|(_, n)| n.as_str())
                        })
                        .unwrap_or("Custom Faction");
                    ui.label(format!("• {name} ({:08X})", rel.faction.unwrap_or(0)));
                    ui.separator();
                    ui.label("Modifier:");
                    ui.add(egui::DragValue::new(&mut rel.modifier).range(-100..=100));
                    ui.label("Reaction:");
                    egui::ComboBox::from_id_salt(("fact_reaction", idx))
                        .selected_text(
                            FACT_REACTIONS
                                .iter()
                                .find(|(v, _)| *v == rel.group_combat_reaction)
                                .map(|(_, n)| *n)
                                .unwrap_or("Custom"),
                        )
                        .show_ui(ui, |ui| {
                            for (v, name) in FACT_REACTIONS.iter() {
                                ui.selectable_value(&mut rel.group_combat_reaction, *v, *name);
                            }
                        });
                });
            }
            if let Some(idx) = remove_idx {
                state.fact_relations.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Relation:");
            let add = &mut state.fact_relation_to_add;
            record_dropdown(ui, "fact_add_relation", &factions_all, &mut add.faction);
            ui.label("Modifier:");
            ui.add(egui::DragValue::new(&mut add.modifier).range(-100..=100));
            ui.label("Reaction:");
            egui::ComboBox::from_id_salt("fact_add_reaction")
                .selected_text(
                    FACT_REACTIONS
                        .iter()
                        .find(|(v, _)| *v == add.group_combat_reaction)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in FACT_REACTIONS.iter() {
                        ui.selectable_value(&mut add.group_combat_reaction, *v, *name);
                    }
                });
            if ui.button("+ Add").clicked() {
                state
                    .fact_relations
                    .push(state.fact_relation_to_add.clone());
            }
        });
    });

    // --- Rank collection ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Ranks — {}", state.fact_ranks.len()));
        ui.add_space(4.0);

        if state.fact_ranks.is_empty() {
            ui.label(egui::RichText::new("This faction has no ranks.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, rank) in state.fact_ranks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this rank").clicked() {
                        remove_idx = Some(idx);
                    }
                    ui.label(format!("{}.", idx + 1));
                    ui.label("Number:");
                    ui.add(egui::DragValue::new(&mut rank.number).range(-1..=127));
                    ui.label("Male:");
                    ui.add(egui::TextEdit::singleline(&mut rank.male).desired_width(140.0));
                    ui.label("Female:");
                    ui.add(egui::TextEdit::singleline(&mut rank.female).desired_width(140.0));
                });
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label("Insignia (INAM, unused):");
                    let mut text = rank.insignia.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut text)
                                .desired_width(240.0)
                                .hint_text("(none)"),
                        )
                        .changed()
                    {
                        rank.insignia = if text.trim().is_empty() {
                            None
                        } else {
                            Some(text)
                        };
                    }
                });
            }
            if let Some(idx) = remove_idx {
                state.fact_ranks.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let add = &mut state.fact_rank_to_add;
            ui.label("Add Rank — Number:");
            ui.add(egui::DragValue::new(&mut add.number).range(-1..=127));
            ui.label("Male:");
            ui.add(egui::TextEdit::singleline(&mut add.male).desired_width(120.0));
            ui.label("Female:");
            ui.add(egui::TextEdit::singleline(&mut add.female).desired_width(120.0));
            if ui.button("+ Add").clicked() {
                state.fact_ranks.push(add.clone());
            }
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

fn draw_lvl_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
            if state.sig == *b"LVLI" {
                ui.checkbox(&mut state.lvl_use_all, "Use All");
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        let title = match &state.sig {
            b"LVLC" => "Leveled Creature Entries (LVLO)",
            b"LVLN" => "Leveled NPC Entries (LVLO)",
            _ => "Leveled Item Entries (LVLO)",
        };
        ui.strong(title);
        ui.add_space(4.0);

        let candidates = match &state.sig {
            b"LVLC" => session.creature_templates(),
            b"LVLN" => session.npc_templates(),
            _ => session.inventory_items(),
        };

        if state.lvl_entries.is_empty() {
            ui.label(egui::RichText::new("No entries in this leveled list.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, (level, r_fid, count)) in state.lvl_entries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this entry").clicked() {
                        remove_idx = Some(idx);
                    }
                    ui.label("Level:");
                    ui.add(egui::DragValue::new(level).range(1..=32767));
                    ui.separator();
                    let name = candidates
                        .iter()
                        .find(|(id, _)| *id == *r_fid)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Object");
                    ui.label(format!("{name} ({r_fid:08X})"));
                    ui.separator();
                    ui.label("Count:");
                    ui.add(egui::DragValue::new(count).range(1..=32767));
                });
            }
            if let Some(idx) = remove_idx {
                state.lvl_entries.remove(idx);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Add Entry - Level:");
            if state.lvl_entry_level_to_add == 0 {
                state.lvl_entry_level_to_add = 1;
            }
            if state.lvl_entry_count_to_add == 0 {
                state.lvl_entry_count_to_add = 1;
            }
            ui.add(egui::DragValue::new(&mut state.lvl_entry_level_to_add).range(1..=32767));
            ui.label("Object:");
            record_dropdown(
                ui,
                "lvl_add_ref",
                &candidates,
                &mut state.lvl_entry_fid_to_add,
            );
            ui.label("Count:");
            ui.add(egui::DragValue::new(&mut state.lvl_entry_count_to_add).range(1..=32767));
            if ui.button("+ Add Entry").clicked() {
                if let Some(fid) = state.lvl_entry_fid_to_add {
                    state.lvl_entries.push((
                        state.lvl_entry_level_to_add.max(1),
                        fid,
                        state.lvl_entry_count_to_add.max(1),
                    ));
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Object Bounds (OBND)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Min X/Y/Z:");
            ui.add(egui::DragValue::new(&mut state.obnd_min[0]));
            ui.add(egui::DragValue::new(&mut state.obnd_min[1]));
            ui.add(egui::DragValue::new(&mut state.obnd_min[2]));
        });
        ui.horizontal(|ui| {
            ui.label("Max X/Y/Z:");
            ui.add(egui::DragValue::new(&mut state.obnd_max[0]));
            ui.add(egui::DragValue::new(&mut state.obnd_max[1]));
            ui.add(egui::DragValue::new(&mut state.obnd_max[2]));
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
        ui.label("Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.clas_desc)
                .desired_rows(3)
                .desired_width(ui.available_width() - 20.0),
        );
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Attributes (ATTR)");
        ui.add_space(4.0);
        // Two rows of the seven SPECIAL attributes, in the spec's order.
        ui.horizontal_wrapped(|ui| {
            for (i, name) in CLAS_ATTRIBUTES.iter().enumerate() {
                ui.label(*name);
                ui.add(egui::DragValue::new(&mut state.clas_attributes[i]).range(0..=255));
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Tag Skills (DATA)");
        ui.add_space(4.0);
        // Four dropdowns, in order. Each names one actor value, and "None" is a
        // real choice: most classes leave a slot empty.
        ui.horizontal_wrapped(|ui| {
            for i in 0..4 {
                ui.label(format!("Tag Skill {}:", i + 1));
                let cur = CLAS_TAG_SKILLS
                    .iter()
                    .find(|(v, _)| *v == state.clas_tag_skills[i])
                    .map(|(_, n)| *n);
                egui::ComboBox::from_id_salt(("clas_tag", i))
                    .selected_text(cur.unwrap_or("Custom"))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for (v, name) in CLAS_TAG_SKILLS {
                            ui.selectable_value(&mut state.clas_tag_skills[i], v, name);
                        }
                    });
                // The raw value stays reachable: the enum runs past the 14 skills
                // the game ships, and a mod may name any actor value.
                ui.label("Raw:");
                ui.add(egui::DragValue::new(&mut state.clas_tag_skills[i]).range(-1..=255));
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Class Configuration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // The checkboxes write the named bits of the whole word, so the two
            // controls cannot disagree.
            let mut playable = (state.clas_flags & 1) != 0;
            if ui.checkbox(&mut playable, "Playable").changed() {
                if playable {
                    state.clas_flags |= 1;
                } else {
                    state.clas_flags &= !1;
                }
                state.clas_playable = playable;
            }
            let mut guard = (state.clas_flags & 2) != 0;
            if ui.checkbox(&mut guard, "Guard").changed() {
                if guard {
                    state.clas_flags |= 2;
                } else {
                    state.clas_flags &= !2;
                }
                state.clas_guard = guard;
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Teaches Skill:");
            let cur = CLAS_TEACHES
                .iter()
                .find(|(v, _)| *v == state.clas_teaches)
                .map(|(_, n)| *n);
            egui::ComboBox::from_id_salt("clas_teaches")
                .selected_text(cur.unwrap_or("Custom"))
                .show_ui(ui, |ui| {
                    for (v, name) in CLAS_TEACHES {
                        ui.selectable_value(&mut state.clas_teaches, v, name);
                    }
                });
            ui.label("Max Training Level:");
            ui.add(egui::DragValue::new(&mut state.clas_max_train).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // The raw word is the source of truth: editing it moves the two
            // checkboxes with it, and the bits the spec does not name stay
            // reachable here.
            ui.label("Raw flags:");
            let before = state.clas_flags;
            if ui
                .add(
                    egui::DragValue::new(&mut state.clas_flags)
                        .range(0..=u32::MAX as i64)
                        .speed(1.0),
                )
                .changed()
                && state.clas_flags != before
            {
                state.clas_playable = (state.clas_flags & 1) != 0;
                state.clas_guard = (state.clas_flags & 2) != 0;
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Buys/Sells and Services");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in CLAS_SERVICES {
                let mut set = (state.clas_services & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.clas_services |= bit;
                    } else {
                        state.clas_services &= !bit;
                    }
                }
            }
        });
        ui.add_space(4.0);
        // The raw word, because the bits fopdoc marks with a `?` are real and a
        // mod may set them; the editor keeps them without naming them.
        ui.horizontal(|ui| {
            ui.label("Raw flags:");
            ui.add(
                egui::DragValue::new(&mut state.clas_services)
                    .range(0..=u32::MAX as i64)
                    .speed(1.0),
            );
        });
    });
}

fn draw_csty_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    // Every subrecord is a fixed-size struct, so the panel edits each one as a
    // whole: a field the panel does not show keeps the value the record had.
    ui.group(|ui| {
        ui.strong("Maneuver Decision (CSTD)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Dodge % Chance:");
            ui.add(egui::DragValue::new(&mut state.csty_std.dodge_chance).range(0..=100));
            ui.label("Left/Right % Chance:");
            ui.add(egui::DragValue::new(&mut state.csty_std.left_right_chance).range(0..=100));
            ui.label("Acrobatic Dodge %:");
            ui.add(egui::DragValue::new(&mut state.csty_std.acrobatic_dodge_chance).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Dodge L/R Timer:");
            ui.add(egui::DragValue::new(&mut std.dodge_lr_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut std.dodge_lr_timer_max).speed(0.05));
            ui.label("Forward Timer:");
            ui.add(egui::DragValue::new(&mut std.dodge_forward_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut std.dodge_forward_timer_max).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Dodge Back Timer:");
            ui.add(egui::DragValue::new(&mut std.dodge_back_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut std.dodge_back_timer_max).speed(0.05));
            ui.label("Idle Timer:");
            ui.add(egui::DragValue::new(&mut std.idle_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut std.idle_timer_max).speed(0.05));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Melee Decision (CSTD)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Block % Chance:");
            ui.add(egui::DragValue::new(&mut state.csty_std.block_chance).range(0..=100));
            ui.label("Attack % Chance:");
            ui.add(egui::DragValue::new(&mut state.csty_std.attack_chance).range(0..=100));
            ui.label("Power Attack % Chance:");
            ui.add(egui::DragValue::new(&mut state.csty_std.power_attack_chance).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Recoil/Stagger Bonus to Attack:");
            ui.add(egui::DragValue::new(&mut std.recoil_stagger_bonus_to_attack).speed(0.5));
            ui.label("Unconscious Bonus:");
            ui.add(egui::DragValue::new(&mut std.unconscious_bonus_to_attack).speed(0.5));
            ui.label("Hand-To-Hand Bonus:");
            ui.add(egui::DragValue::new(&mut std.hand_to_hand_bonus_to_attack).speed(0.5));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Hold Timer:");
            ui.add(egui::DragValue::new(&mut std.hold_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut std.hold_timer_max).speed(0.05));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Power Attacks - Directional Weights:").underline());
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Normal:");
            ui.add(egui::DragValue::new(&mut std.power_attack_normal).range(0..=100));
            ui.label("Forward:");
            ui.add(egui::DragValue::new(&mut std.power_attack_forward).range(0..=100));
            ui.label("Back:");
            ui.add(egui::DragValue::new(&mut std.power_attack_back).range(0..=100));
            ui.label("Left:");
            ui.add(egui::DragValue::new(&mut std.power_attack_left).range(0..=100));
            ui.label("Right:");
            ui.add(egui::DragValue::new(&mut std.power_attack_right).range(0..=100));
        });
        ui.horizontal_wrapped(|ui| {
            let std = &mut state.csty_std;
            ui.label("Recoil/Stagger Bonus to Power:");
            ui.add(egui::DragValue::new(&mut std.recoil_stagger_bonus_to_power).speed(0.5));
            ui.label("Unconscious Bonus:");
            ui.add(egui::DragValue::new(&mut std.unconscious_bonus_to_power_attack).speed(0.5));
            ui.label("Rushing Attack %:");
            ui.add(egui::DragValue::new(&mut std.rushing_attack_chance).range(0..=100));
            ui.label("Rushing Distance Mult:");
            ui.add(egui::DragValue::new(&mut std.rushing_attack_distance_mult).speed(0.05));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Flags (CSTD)");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in CSTY_FLAGS {
                let mut set = (state.csty_std.flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.csty_std.flags |= bit;
                    } else {
                        state.csty_std.flags &= !bit;
                    }
                }
            }
        });
        ui.add_space(4.0);
        // The raw word, because bit 3 is marked `??` in the spec and a mod may
        // set it; the editor keeps it without naming it.
        ui.horizontal(|ui| {
            ui.label("Raw flags:");
            ui.add(
                egui::DragValue::new(&mut state.csty_std.flags)
                    .range(0..=u16::MAX as i64)
                    .speed(1.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Advanced Modifiers (CSAD)");
        ui.add_space(4.0);
        let adv = &mut state.csty_adv;
        ui.horizontal_wrapped(|ui| {
            ui.label("Dodge Fatigue Mod Mult:");
            ui.add(egui::DragValue::new(&mut adv.dodge_fatigue_mod_mult).speed(0.5));
            ui.label("Base:");
            ui.add(egui::DragValue::new(&mut adv.dodge_fatigue_mod_base).speed(0.5));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Encumbered Speed Mod Base:");
            ui.add(egui::DragValue::new(&mut adv.encumbered_speed_mod_base).speed(0.5));
            ui.label("Mult:");
            ui.add(egui::DragValue::new(&mut adv.encumbered_speed_mod_mult).speed(0.05));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Dodge Multipliers:").underline());
        ui.horizontal_wrapped(|ui| {
            ui.label("Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.dodge_while_under_attack_mult).speed(0.05));
            ui.label("Not Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.dodge_not_under_attack_mult).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Back, Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.dodge_back_while_under_attack_mult).speed(0.05));
            ui.label("Back, Not Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.dodge_back_not_under_attack_mult).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Forward, While Attacking:");
            ui.add(egui::DragValue::new(&mut adv.dodge_forward_while_attacking_mult).speed(0.05));
            ui.label("Forward, Not Attacking:");
            ui.add(egui::DragValue::new(&mut adv.dodge_forward_not_attacking_mult).speed(0.05));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Block Modifiers:").underline());
        ui.horizontal_wrapped(|ui| {
            ui.label("Skill Modifier Mult:");
            ui.add(egui::DragValue::new(&mut adv.block_skill_modifier_mult).speed(0.5));
            ui.label("Base:");
            ui.add(egui::DragValue::new(&mut adv.block_skill_modifier_base).speed(0.5));
            ui.label("While Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.block_while_under_attack_mult).speed(0.05));
            ui.label("Not Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.block_not_under_attack_mult).speed(0.05));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Attack Modifiers:").underline());
        ui.horizontal_wrapped(|ui| {
            ui.label("Skill Modifier Mult:");
            ui.add(egui::DragValue::new(&mut adv.attack_skill_modifier_mult).speed(0.5));
            ui.label("Base:");
            ui.add(egui::DragValue::new(&mut adv.attack_skill_modifier_base).speed(0.5));
            ui.label("While Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.attack_while_under_attack_mult).speed(0.05));
            ui.label("Not Under Attack:");
            ui.add(egui::DragValue::new(&mut adv.attack_not_under_attack_mult).speed(0.05));
            ui.label("During Block:");
            ui.add(egui::DragValue::new(&mut adv.attack_during_block_mult).speed(0.05));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Power Attack Fatigue Mod:").underline());
        ui.horizontal_wrapped(|ui| {
            ui.label("Base:");
            ui.add(egui::DragValue::new(&mut adv.power_attack_fatigue_mod_base).speed(0.5));
            ui.label("Mult:");
            ui.add(egui::DragValue::new(&mut adv.power_attack_fatigue_mod_mult).speed(0.5));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Simple (CSSD)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Cover Search Radius:");
            ui.add(
                egui::DragValue::new(&mut state.csty_simple.cover_search_radius)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
            ui.label("Take Cover Chance %:");
            ui.add(
                egui::DragValue::new(&mut state.csty_simple.take_cover_chance)
                    .speed(0.05)
                    .range(0.0..=1.0),
            );
            ui.label("Combat Radius:");
            ui.add(
                egui::DragValue::new(&mut state.csty_simple.combat_radius)
                    .speed(10.0)
                    .range(0.0..=100000.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Weapon Restrictions:");
            let cur = CSTY_WEAPON_RESTRICTIONS
                .iter()
                .find(|(v, _)| *v == state.csty_simple.weapon_restrictions)
                .map(|(_, n)| *n);
            egui::ComboBox::from_id_salt("csty_weapon_restr")
                .selected_text(cur.unwrap_or("Custom"))
                .show_ui(ui, |ui| {
                    for (v, name) in CSTY_WEAPON_RESTRICTIONS {
                        ui.selectable_value(&mut state.csty_simple.weapon_restrictions, v, name);
                    }
                });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let s = &mut state.csty_simple;
            ui.label("Wait Timer:");
            ui.add(egui::DragValue::new(&mut s.wait_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.wait_timer_max).speed(0.05));
            ui.label("Wait to Fire Timer:");
            ui.add(egui::DragValue::new(&mut s.wait_to_fire_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.wait_to_fire_timer_max).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            let s = &mut state.csty_simple;
            ui.label("Fire Timer:");
            ui.add(egui::DragValue::new(&mut s.fire_timer_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.fire_timer_max).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            let s = &mut state.csty_simple;
            ui.label("Ranged Weapon Range Mult:");
            ui.add(egui::DragValue::new(&mut s.ranged_weapon_range_mult_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.ranged_weapon_range_mult_max).speed(0.05));
        });
        ui.horizontal_wrapped(|ui| {
            let s = &mut state.csty_simple;
            ui.label("Max Targeting FOV:");
            ui.add(egui::DragValue::new(&mut s.max_targeting_fov).speed(0.05));
            ui.label("Semi-Auto Firing Delay Mult:");
            ui.add(egui::DragValue::new(&mut s.semi_auto_firing_delay_mult_min).speed(0.05));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.semi_auto_firing_delay_mult_max).speed(0.05));
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

fn draw_hair_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Hair Flags (DATA)");
        ui.add_space(4.0);
        // The checkboxes write the named bits of the whole word, so the two
        // controls cannot disagree, and a bit the spec does not name survives.
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in HAIR_FLAGS {
                let mut set = (state.hair_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.hair_flags |= bit;
                    } else {
                        state.hair_flags &= !bit;
                    }
                }
            }
        });
        ui.add_space(4.0);
        // The raw word, because the bits fopdoc marks with a `?` are real and
        // a mod may set them; the editor keeps them without naming them.
        ui.horizontal(|ui| {
            ui.label("Raw flags:");
            ui.add(
                egui::DragValue::new(&mut state.hair_flags)
                    .range(0..=u8::MAX as i64)
                    .speed(1.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Texture (ICON)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.hair_icon)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
    });

    // --- The model data collection ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);

        let model = &mut state.hair_model;

        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                // The original instance's model path is the header panel's
                // "Model Path" field, so it is edited there.
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });

        // MODT's texture hashes are file hashes the editor cannot compute or
        // author, so it reports what is there rather than offering to change
        // it: rewriting one would corrupt the mod.
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));

        // MODB is four unknown bytes. They are reported, and copied through.
        ui.horizontal(|ui| {
            ui.label("MODB:");
            match &model.modb {
                Some(b) => ui.label(format!("{:?} — preserved as-is", b)),
                None => ui.label(egui::RichText::new("absent").italics()),
            }
        });

        // MODD: the FaceGen model flags.
        ui.horizontal(|ui| {
            let mut has = model.modd.is_some();
            if ui
                .checkbox(&mut has, "Record has MODD (FaceGen flags)")
                .changed()
            {
                model.modd = if has { Some(0) } else { None };
            }
            if let Some(modd) = model.modd.as_mut() {
                for (bit, name) in MODD_FLAGS {
                    let mut set = (*modd & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            *modd |= bit;
                        } else {
                            *modd &= !bit;
                        }
                    }
                }
            }
        });

        // MODS: the alternate textures.
        ui.add_space(4.0);
        ui.strong(format!("Alternate Textures (MODS) — {}", model.mods.len()));
        ui.add_space(4.0);

        if model.mods.is_empty() {
            ui.label(egui::RichText::new("This model has no alternate textures.").italics());
        } else {
            let txsts = session.records_by_sig(*b"TXST");
            let mut remove_idx = None;
            for (idx, alt) in model.mods.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this alternate texture")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    ui.label("3D Name:");
                    ui.add(egui::TextEdit::singleline(&mut alt.name).desired_width(180.0));
                    ui.label("Texture (TXST):");
                    let mut fid = Some(alt.texture);
                    record_dropdown(ui, &format!("hair_alts_txst_{idx}"), &txsts, &mut fid);
                    alt.texture = fid.unwrap_or(0);
                    ui.label("3D Index:");
                    ui.add(egui::DragValue::new(&mut alt.index).range(-1..=255));
                });
            }
            if let Some(idx) = remove_idx {
                model.mods.remove(idx);
            }
        }
    });
}

/// Draw the Head Part (HDPT) editor body.
///
/// The spec's `DATA` names a single flag, "Playable", but the word is kept
/// whole and the raw byte is offered, so a bit a mod sets but the spec does
/// not name is never silently cleared.
fn draw_hdpt_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Flags (DATA)");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in HDPT_FLAGS {
                let mut set = (state.hdpt_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.hdpt_flags |= bit;
                    } else {
                        state.hdpt_flags &= !bit;
                    }
                }
            }
        });
        ui.add_space(4.0);
        // The raw byte, so a bit the spec does not name stays reachable.
        ui.horizontal(|ui| {
            ui.label("Raw flags:");
            ui.add(
                egui::DragValue::new(&mut state.hdpt_flags)
                    .range(0..=u8::MAX as i64)
                    .speed(1.0),
            );
        });
    });

    // --- The model data collection ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);

        let model = &mut state.hdpt_model;

        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                // The original instance's model path is the header panel's
                // "Model Path" field, so it is edited there.
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });

        // MODT's texture hashes are file hashes the editor cannot compute or
        // author, so it reports what is there rather than offering to change
        // it: rewriting one would corrupt the mod.
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));

        // MODB is four unknown bytes. They are reported, and copied through.
        ui.horizontal(|ui| {
            ui.label("MODB:");
            match &model.modb {
                Some(b) => ui.label(format!("{b:?} — preserved as-is")),
                None => ui.label(egui::RichText::new("absent").italics()),
            }
        });

        // MODD: the FaceGen model flags.
        ui.horizontal(|ui| {
            let mut has = model.modd.is_some();
            if ui
                .checkbox(&mut has, "Record has MODD (FaceGen flags)")
                .changed()
            {
                model.modd = if has { Some(0) } else { None };
            }
            if let Some(modd) = model.modd.as_mut() {
                for (bit, name) in MODD_FLAGS {
                    let mut set = (*modd & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            *modd |= bit;
                        } else {
                            *modd &= !bit;
                        }
                    }
                }
            }
        });

        // MODS: the alternate textures.
        ui.add_space(4.0);
        ui.strong(format!("Alternate Textures (MODS) — {}", model.mods.len()));
        ui.add_space(4.0);

        if model.mods.is_empty() {
            ui.label(egui::RichText::new("This model has no alternate textures.").italics());
        } else {
            let txsts = session.records_by_sig(*b"TXST");
            let mut remove_idx = None;
            for (idx, alt) in model.mods.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this alternate texture")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    ui.label("3D Name:");
                    ui.add(egui::TextEdit::singleline(&mut alt.name).desired_width(180.0));
                    ui.label("Texture (TXST):");
                    let mut fid = Some(alt.texture);
                    record_dropdown(ui, &format!("hdpt_alts_txst_{idx}"), &txsts, &mut fid);
                    alt.texture = fid.unwrap_or(0);
                    ui.label("3D Index:");
                    ui.add(egui::DragValue::new(&mut alt.index).range(-1..=255));
                });
            }
            if let Some(idx) = remove_idx {
                model.mods.remove(idx);
            }
        }
    });

    // --- HNAM: the extra parts ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Extra Parts (HNAM) — {}",
            state.hdpt_extra_parts.len()
        ));
        ui.add_space(4.0);

        let hdpts_all = session.records_by_sig(*b"HDPT");
        let mut remove_idx = None;
        for (idx, &fid) in state.hdpt_extra_parts.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button("✖")
                    .on_hover_text("Remove this extra part")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
                let name = hdpts_all
                    .iter()
                    .find(|(id, _)| *id == fid)
                    .map(|(_, n)| n.as_str())
                    .unwrap_or("Custom Head Part");
                ui.label(format!("• {name} ({fid:08X})"));
            });
        }
        if let Some(idx) = remove_idx {
            state.hdpt_extra_parts.remove(idx);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            record_dropdown(
                ui,
                "hdpt_add_extra",
                &hdpts_all,
                &mut state.hdpt_extra_part_to_add,
            );
            if ui.button("+ Add Extra Part").clicked() {
                if let Some(fid) = state.hdpt_extra_part_to_add {
                    if !state.hdpt_extra_parts.contains(&fid) {
                        state.hdpt_extra_parts.push(fid);
                    }
                }
            }
        });
    });
}

fn draw_eyes_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Eyes");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Texture (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.eyes_icon)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Flags (DATA)");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in EYES_FLAGS {
                let mut set = (state.eyes_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.eyes_flags |= bit;
                    } else {
                        state.eyes_flags &= !bit;
                    }
                }
            }
        });
        ui.add_space(4.0);
        // The raw byte, so a bit the spec does not name stays reachable.
        ui.horizontal(|ui| {
            ui.label("Raw flags:");
            ui.add(
                egui::DragValue::new(&mut state.eyes_flags)
                    .range(0..=u8::MAX as i64)
                    .speed(1.0),
            );
        });
    });
}

/// Draw the Addon Node (ADDN) editor body.
///
/// The spec's `DNAM` names a "Master Particle System Cap" and two unknown
/// bytes. The unknown bytes are reported and carried through unchanged: the
/// editor cannot know what they mean, and zeroing them would change bytes the
/// game wrote.
fn draw_addn_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Addon Node");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Node Index (DATA):");
            ui.add(egui::DragValue::new(&mut state.addn_node_index).range(-1..=100000));
        });
        ui.horizontal(|ui| {
            ui.label("Master Particle System Cap (DNAM):");
            ui.add(egui::DragValue::new(&mut state.addn_master_cap).range(0..=65535));
        });
        ui.horizontal(|ui| {
            ui.label("DNAM unknown bytes:");
            ui.label(format!("{:?} — preserved as-is", state.addn_dnam_unknown));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);

        let model = &mut state.addn_model;

        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                // The original instance's model path is the header panel's
                // "Model Path" field, so it is edited there.
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });

        // MODT's texture hashes are file hashes the editor cannot compute or
        // author, so it reports what is there rather than offering to change
        // it: rewriting one would corrupt the mod.
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));

        // MODB is four unknown bytes. They are reported, and copied through.
        ui.horizontal(|ui| {
            ui.label("MODB:");
            match &model.modb {
                Some(b) => ui.label(format!("{b:?} — preserved as-is")),
                None => ui.label(egui::RichText::new("absent").italics()),
            }
        });

        // MODD: the FaceGen model flags.
        ui.horizontal(|ui| {
            let mut has = model.modd.is_some();
            if ui
                .checkbox(&mut has, "Record has MODD (FaceGen flags)")
                .changed()
            {
                model.modd = if has { Some(0) } else { None };
            }
            if let Some(modd) = model.modd.as_mut() {
                for (bit, name) in MODD_FLAGS {
                    let mut set = (*modd & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            *modd |= bit;
                        } else {
                            *modd &= !bit;
                        }
                    }
                }
            }
        });
    });
}

fn draw_avif_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Actor Value Information");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Short Name / Abbreviation (ANAM):");
            ui.add(egui::TextEdit::singleline(&mut state.avif_short_name).desired_width(120.0));
        });
        ui.add_space(4.0);
        ui.label("Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.avif_desc)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.avif_icon)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.avif_mico)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });
}

/// Draw the one editable body part, in the order the record stores it.
fn draw_bptd_part_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
    idx: usize,
) {
    let Some(part) = state.bptd_parts.get_mut(idx) else {
        return;
    };
    let d = &mut part.data;

    ui.group(|ui| {
        ui.strong("Part Identity");
        ui.add_space(4.0);
        // The name is optional in the file, so it is a checkbox plus a field:
        // an unnamed part is legal and the editor has to be able to express it.
        let mut named = part.name.is_some();
        ui.horizontal(|ui| {
            if ui.checkbox(&mut named, "Named Part (BPTN)").changed() {
                part.name = if named { Some(String::new()) } else { None };
            }
            if let Some(name) = part.name.as_mut() {
                ui.label("Part Name:");
                ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
            } else {
                ui.label(
                    egui::RichText::new("this part has no BPTN, which the game accepts").italics(),
                );
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Part Node (BPNN):");
            ui.add(
                egui::TextEdit::singleline(&mut part.node)
                    .desired_width(ui.available_width() - 160.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("VATS Target (BPNT):");
            ui.add(
                egui::TextEdit::singleline(&mut part.vats_target)
                    .desired_width(ui.available_width() - 160.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("IK Data - Start Node (BPNI):");
            ui.add(
                egui::TextEdit::singleline(&mut part.ik_start_node)
                    .desired_width(ui.available_width() - 200.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Node Data (BPND)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Damage Multiplier:");
            ui.add(
                egui::DragValue::new(&mut d.damage_mult)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
            ui.label("Health Percent:");
            ui.add(egui::DragValue::new(&mut d.health_percent).range(0..=255));
            ui.label("To Hit Chance:");
            ui.add(egui::DragValue::new(&mut d.to_hit_chance).range(0..=100));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Part Type:");
            let cur_type = (d.part_type as usize).min(BPTD_PART_TYPES.len() - 1);
            let type_lbl = BPTD_PART_TYPES.get(cur_type).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt(("bptd_part_type", idx))
                .selected_text(type_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in BPTD_PART_TYPES.iter().enumerate() {
                        ui.selectable_value(&mut d.part_type, i as u8, name);
                    }
                });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Actor Value:");
            let cur_av = BPTD_ACTOR_VALUES
                .iter()
                .find(|(v, _)| *v == d.actor_value)
                .map(|(_, n)| *n);
            egui::ComboBox::from_id_salt(("bptd_actor_value", idx))
                .selected_text(cur_av.unwrap_or("Custom"))
                .show_ui(ui, |ui| {
                    for (v, name) in BPTD_ACTOR_VALUES {
                        ui.selectable_value(&mut d.actor_value, v, name);
                    }
                });
            // The raw value stays reachable, because the enum is a superset of
            // what the dropdown offers and a record may name anything.
            ui.label("Raw:");
            ui.add(
                egui::DragValue::new(&mut d.actor_value)
                    .range(-1..=127)
                    .speed(0.2),
            );
        });

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Flags:").underline());
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in BPTD_FLAGS {
                let mut set = (d.flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        d.flags |= bit;
                    } else {
                        d.flags &= !bit;
                    }
                }
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Tracking Max Angle:");
            ui.add(
                egui::DragValue::new(&mut d.tracking_max_angle)
                    .speed(1.0)
                    .range(0.0..=360.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Explodable");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Explosion Chance %:");
            ui.add(egui::DragValue::new(&mut d.explodable_explosion_chance_percent).range(0..=100));
            ui.label("Debris Count:");
            ui.add(
                egui::DragValue::new(&mut d.explodable_debris_count)
                    .range(0..=65535)
                    .speed(0.2),
            );
            ui.label("Debris Scale:");
            ui.add(
                egui::DragValue::new(&mut d.explodable_debris_scale)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
            ui.label("Decal Count:");
            ui.add(egui::DragValue::new(&mut d.explodable_decal_count).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Debris (DEBR):");
            let debrs = session.records_by_sig(*b"DEBR");
            record_dropdown(ui, "bptd_expl_debris", &debrs, &mut d.explodable_debris);
        });
        ui.horizontal(|ui| {
            ui.label("Explosion (EXPL):");
            let expls = session.records_by_sig(*b"EXPL");
            record_dropdown(
                ui,
                "bptd_expl_explosion",
                &expls,
                &mut d.explodable_explosion,
            );
        });
        ui.horizontal(|ui| {
            ui.label("Impact Dataset (IPDS):");
            let ipdss = session.records_by_sig(*b"IPDS");
            record_dropdown(
                ui,
                "bptd_expl_impact",
                &ipdss,
                &mut d.explodable_impact_dataset,
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Severable");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Debris Count:");
            ui.add(egui::DragValue::new(&mut d.severable_debris_count).speed(0.2));
            ui.label("Debris Scale:");
            ui.add(
                egui::DragValue::new(&mut d.severable_debris_scale)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
            ui.label("Decal Count:");
            ui.add(egui::DragValue::new(&mut d.severable_decal_count).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Debris (DEBR):");
            let debrs = session.records_by_sig(*b"DEBR");
            record_dropdown(ui, "bptd_sev_debris", &debrs, &mut d.severable_debris);
        });
        ui.horizontal(|ui| {
            ui.label("Explosion (EXPL):");
            let expls = session.records_by_sig(*b"EXPL");
            record_dropdown(ui, "bptd_sev_explosion", &expls, &mut d.severable_explosion);
        });
        ui.horizontal(|ui| {
            ui.label("Impact Dataset (IPDS):");
            let ipdss = session.records_by_sig(*b"IPDS");
            record_dropdown(
                ui,
                "bptd_sev_impact",
                &ipdss,
                &mut d.severable_impact_dataset,
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Gore Effects");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Translate X:");
            ui.add(egui::DragValue::new(&mut d.gore_x).speed(0.1));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut d.gore_y).speed(0.1));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut d.gore_z).speed(0.1));
        });
        ui.horizontal(|ui| {
            ui.label("X Rotation:");
            ui.add(egui::DragValue::new(&mut d.gore_rx).speed(0.1));
            ui.label("Y Rotation:");
            ui.add(egui::DragValue::new(&mut d.gore_ry).speed(0.1));
            ui.label("Z Rotation:");
            ui.add(egui::DragValue::new(&mut d.gore_rz).speed(0.1));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Target Bone (NAM4):");
            ui.add(
                egui::TextEdit::singleline(&mut part.gore_target_bone)
                    .desired_width(ui.available_width() - 160.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Limb Replacement");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Model (NAM1):");
            ui.add(
                egui::TextEdit::singleline(&mut part.limb_model)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Scale:");
            ui.add(
                egui::DragValue::new(&mut d.limb_replacement_scale)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
        });
        // A texture hash is a file hash the editor cannot compute or author, so
        // it reports what is there rather than offering to change it: rewriting
        // it would corrupt the mod.
        let hashes = part.texture_hashes.len() / 24;
        ui.label(format!(
            "Texture File Hashes (NAM5): {hashes} hash(es), {} bytes — preserved as-is",
            part.texture_hashes.len()
        ));
    });
}

fn draw_bptd_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Ragdoll (RAGA)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let ragdolls = session.records_by_sig(*b"RGDL");
            record_dropdown(ui, "bptd_ragdoll", &ragdolls, &mut state.bptd_ragdoll);
        });
    });

    ui.add_space(6.0);
    ui.strong(format!("Body Parts ({})", state.bptd_parts.len()));
    ui.add_space(4.0);

    if state.bptd_parts.is_empty() {
        ui.label(
            egui::RichText::new("No body parts: this record defines no damageable parts.")
                .italics(),
        );
    }

    let mut remove_idx = None;
    for idx in 0..state.bptd_parts.len() {
        let label = state.bptd_parts[idx].label().to_string();
        let type_lbl = {
            let t = state.bptd_parts[idx].data.part_type as usize;
            BPTD_PART_TYPES.get(t).copied().unwrap_or("Unknown")
        };
        egui::CollapsingHeader::new(format!("{}. {label} — {type_lbl}", idx + 1))
            .id_salt(("bptd_part", idx))
            .default_open(idx == 0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("✖")
                        .on_hover_text("Remove this body part")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    ui.label(
                        egui::RichText::new("part subrecords are written in file order").weak(),
                    );
                });
                draw_bptd_part_ui(ui, session, state, idx);
            });
    }
    if let Some(idx) = remove_idx {
        state.bptd_parts.remove(idx);
    }

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Add Body Part");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let add = &mut state.bptd_part_to_add;
            let mut named = add.name.is_some();
            if ui.checkbox(&mut named, "Named Part (BPTN)").changed() {
                add.name = if named { Some(String::new()) } else { None };
            }
            if let Some(name) = add.name.as_mut() {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(name).desired_width(140.0));
            }
            ui.label("Part Node (BPNN):");
            ui.add(
                egui::TextEdit::singleline(&mut add.node)
                    .desired_width(ui.available_width() - 380.0),
            );
        });
        ui.horizontal(|ui| {
            let add = &mut state.bptd_part_to_add;
            ui.label("Part Type:");
            let cur_type = (add.data.part_type as usize).min(BPTD_PART_TYPES.len() - 1);
            let type_lbl = BPTD_PART_TYPES.get(cur_type).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("bptd_add_part_type")
                .selected_text(type_lbl)
                .show_ui(ui, |ui| {
                    for (i, &name) in BPTD_PART_TYPES.iter().enumerate() {
                        ui.selectable_value(&mut add.data.part_type, i as u8, name);
                    }
                });
            if ui.button("+ Add Part").clicked() {
                let new_part = std::mem::take(&mut state.bptd_part_to_add);
                // A part the game can read needs a node to match against the
                // skeleton; without one the engine ignores the part entirely.
                if new_part.node.trim().is_empty() {
                    state.status = "a body part needs a Part Node (BPNN)".to_string();
                } else {
                    state.bptd_parts.push(new_part);
                }
            }
        });
    });
}
