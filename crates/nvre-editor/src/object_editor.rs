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
    /// The animation type — the uint32 enum: Hand To Hand, Melee, Pistol,
    /// Rifle, and the rest.
    pub weap_animation_type: u32,
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
    /// The type the record's own flag bits say, so a save can tell a
    /// change of type from a save that changes nothing.
    pub armo_type_from_general: u8,
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
    /// The record's own `SNAM` entries, in file order, so the bytes the
    /// spec does not name survive a save.
    pub raw_snam: Vec<Vec<u8>>,
    /// The actor's inventory. Each is a `CNTO` (the item and the count) and
    /// the `COED` that follows it, if it has one.
    pub actor_inventory_entries: Vec<ActorInventoryEntry>,
    /// The actor's destruction stages.
    pub actor_destruction: Vec<DestructionStage>,
    /// The destruction header's flag word.
    pub actor_dest_flags: u8,
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
    /// `DATA` Playable — the u8 enum: No, Yes, Hidden.
    pub perk_playable: u8,
    /// `DATA` Hidden — the u8 enum: No, Yes, Hidden.
    pub perk_hidden: u8,
    /// The perk's own five-byte `DATA` subrecord. The record also carries a
    /// three-byte data per rank, so the two are told apart.
    pub perk_own_data: Option<Vec<u8>>,
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
    /// The sound's attenuation data — the `SNDD` struct.
    pub sound_attenuation: SoundAttenuation,
    /// The sound's minimum attenuation distance.
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
    /// The leveled list entries: the level, the reference and the count.
    ///
    /// The `COED` extra data that follows an `LVLO` belongs to that entry,
    /// so it is kept alongside it rather than dropped.
    pub lvl_entries: Vec<LeveledListEntry>,
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
    /// `WTHR` The four cloud texture layers.
    pub wthr_cloud_layers: [String; 4],
    /// `WTHR` The maximum number of cloud layers — the uint32.
    pub wthr_max_cloud_layers: u32,
    /// `WTHR` The four cloud layer speeds.
    pub wthr_cloud_speeds: [u8; 4],
    /// `WTHR` The cloud layer colours — one `TimeOfDayColors` per layer.
    pub wthr_cloud_colors: Vec<TimeOfDayColors>,
    /// `WTHR` The ten colour types, each with one colour per time of day.
    pub wthr_colors: Vec<TimeOfDayColors>,
    /// `WTHR` The fog distances — six float32s.
    pub wthr_fog_distances: [f32; 6],
    /// `WTHR` The data struct's fields.
    pub wthr_cloud_speed_lower: u8,
    pub wthr_cloud_speed_upper: u8,
    pub wthr_transition_delta: u8,
    pub wthr_precipitation_begin: u8,
    pub wthr_precipitation_end: u8,
    pub wthr_thunder_begin: u8,
    pub wthr_thunder_end: u8,
    pub wthr_thunder_frequency: u8,
    pub wthr_weather_classification: u8,
    pub wthr_lightning_color: [u8; 4],
    /// `WTHR` The image space modifiers, one per time of day.
    pub wthr_image_space_modifiers: [Option<u32>; 6],
    /// `WTHR` The sounds.
    pub wthr_sounds: Vec<WeatherSound>,
    /// `WATR` The visual data — the `DNAM` struct.
    pub watr_visual: WatrVisual,
    /// `GNAM` Daytime — the FormID of the `WATR` record used in the day.
    pub watr_daytime: Option<u32>,
    /// `GNAM` Nighttime — the FormID of the `WATR` record used at night.
    pub watr_nighttime: Option<u32>,
    /// `GNAM` Underwater — the FormID of the `WATR` record used underwater.
    pub watr_underwater: Option<u32>,
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

    // DEHY (Dehydration) fields
    /// `DATA` Trigger Threshold — the uint32.
    pub dehyd_trigger_threshold: u32,
    /// `DATA` Actor Effect — the FormID of a `SPEL` record.
    pub dehyd_actor_effect: Option<u32>,

    // HUNG (Hunger) fields
    /// `DATA` Trigger Threshold — the uint32.
    pub hung_trigger_threshold: u32,
    /// `DATA` Actor Effect — the FormID of a `SPEL` record.
    pub hung_actor_effect: Option<u32>,

    // RADS (Radiation) fields
    /// `DATA` Trigger Threshold — the uint32.
    pub rads_trigger_threshold: u32,
    /// `DATA` Actor Effect — the FormID of a `SPEL` record.
    pub rads_actor_effect: Option<u32>,

    // SLPD (Sleep Deprivation) fields
    /// `DATA` Trigger Threshold — the uint32.
    pub slpd_trigger_threshold: u32,
    /// `DATA` Actor Effect — the FormID of a `SPEL` record.
    pub slpd_actor_effect: Option<u32>,

    // ECZN (Encounter Zone) fields
    /// `DATA` Owner — the FormID of an `NPC_` or `FACT` record.
    pub eczn_owner: Option<u32>,
    /// `DATA` Rank — the int8.
    pub eczn_rank: i8,
    /// `DATA` Minimum Level — the int8.
    pub eczn_minimum_level: i8,
    /// `DATA` Flags — the u8 flag word: Never Resets, Match PC Below Minimum
    /// Level. The word is kept whole, so a bit the spec does not name
    /// survives an edit.
    pub eczn_flags: u8,
    /// `DATA` Unused — the byte the spec does not name, carried through.
    pub eczn_unused: u8,

    // DOBJ (Default Object Manager) fields
    /// `DATA` — the thirty-four default-object FormIDs, in the order the
    /// record stores them. The field names are in `DOBJ_FIELDS`.
    pub dobj_values: Vec<u32>,

    // FURN (Furniture) fields
    /// `MNAM` Marker Flags — the raw bytes.
    pub furn_marker_flags: Vec<u8>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub furn_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub furn_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32.
    pub furn_dest_health: i32,
    /// `DEST` Flags — the u8 flag word.
    pub furn_dest_flags: u8,

    // GRAS (Grass) fields
    /// `DATA` Density — the uint8.
    pub gras_density: u8,
    /// `DATA` Min Slope — the uint8.
    pub gras_min_slope: u8,
    /// `DATA` Max Slope — the uint8.
    pub gras_max_slope: u8,
    /// `DATA` Unit From Water Amount — the uint16.
    pub gras_unit_from_water_amount: u16,
    /// `DATA` Unit From Water Type — the uint32 enum.
    pub gras_unit_from_water_type: u32,
    /// `DATA` Position Range — the float32.
    pub gras_position_range: f32,
    /// `DATA` Height Range — the float32.
    pub gras_height_range: f32,
    /// `DATA` Color Range — the float32.
    pub gras_color_range: f32,
    /// `DATA` Wave Period — the float32.
    pub gras_wave_period: f32,
    /// `DATA` Flags — the u8 flag word: Vertex Lighting, Uniform Scaling, Fit
    /// To Slope. The word is kept whole, so a bit the spec does not name
    /// survives an edit.
    pub gras_flags: u8,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub gras_model: ModelData,

    // IDLM (Idle Animation) fields
    /// `IDLF` Flags — the u8 flag word: Run In Sequence, Do Once. The word is
    /// kept whole, so a bit the spec does not name survives an edit.
    pub idlm_flags: u8,
    /// `IDLC` Animation Count — the u8.
    pub idlm_animation_count: u8,
    /// `IDLC`'s three unused bytes, carried through unchanged.
    pub idlm_idlc_unused: [u8; 3],
    /// `IDLT` Idle Timer Setting — the float32.
    pub idlm_idle_timer: f32,
    /// `IDLA` Animations — the list of `IDLE` record FormIDs.
    pub idlm_animations: Vec<u32>,

    // IMGS (Image Space) fields
    ///
    /// The `DNAM` layout is version-dependent: 152 bytes on 67 of the game's
    /// records, 148 on twelve and 132 on eleven. The schema names the first
    /// fourteen HDR floats; everything after them depends on the record's
    /// form version, so it is carried through as raw bytes rather than
    /// guessed at.
    pub imgs_hdr: [f32; 14],
    /// The rest of `DNAM` — the version-dependent part, carried through
    /// unchanged.
    pub imgs_rest: Vec<u8>,

    // LGTM (Lighting) fields
    /// `DATA` Ambient Color — the byte quadruple (red, green, blue, unused).
    pub lgtm_ambient_color: [u8; 4],
    /// `DATA` Directional Color — the byte quadruple.
    pub lgtm_directional_color: [u8; 4],
    /// `DATA` Fog Color — the byte quadruple.
    pub lgtm_fog_color: [u8; 4],
    pub lgtm_fog_near: f32,
    pub lgtm_fog_far: f32,
    pub lgtm_directional_rotation_xy: i32,
    pub lgtm_directional_rotation_z: i32,
    pub lgtm_directional_fade: f32,
    pub lgtm_fog_clip_distance: f32,
    pub lgtm_fog_power: f32,

    // LSCR (Load Screen) fields
    /// `ICON` — the large icon filename.
    pub lscr_icon: String,
    /// `MICO` — the small icon filename.
    pub lscr_mico: String,
    /// `DESC` — the description.
    pub lscr_desc: String,
    /// `WMI1` Load Screen Type — the FormID of an `LSCT` record.
    pub lscr_load_screen_type: Option<u32>,
    /// `LNAM` — the load screen's locations. `LNAM` is a *repeated*
    /// subrecord, so the locations are a list.
    pub lscr_locations: Vec<LoadScreenLocation>,

    // LSCT (Load Screen Type) fields
    /// `DATA` Type — the uint32 enum.
    pub lsct_type: u32,
    pub lsct_x: u32,
    pub lsct_y: u32,
    pub lsct_width: u32,
    pub lsct_height: u32,
    pub lsct_orientation: f32,
    pub lsct_font1: u32,
    pub lsct_font1_color: [f32; 3],
    pub lsct_font1_alignment: u32,
    pub lsct_font2: u32,
    pub lsct_font2_color: [f32; 3],
    pub lsct_font2_alignment: u32,
    pub lsct_stats: u32,

    // MICN (Mission Icon) fields
    /// `ICON` — the large icon filename.
    pub micn_icon: String,
    /// `MICO` — the small icon filename.
    pub micn_mico: String,

    // MUSC (Music) fields
    /// `FNAM` — the music's filename.
    pub musc_filename: String,
    /// `ANAM` db — the float32. A positive value makes the music loop.
    pub musc_db: f32,

    // RCCT (Recipe Category) fields
    /// `DATA` Flags — the u8 flag word. The word is kept whole, so a bit the
    /// spec does not name survives an edit.
    pub rcct_flags: u8,

    // REPU (Reputation) fields
    /// `DATA` Value — the float32.
    pub repu_value: f32,
    /// `ICON` — the large icon filename.
    pub repu_icon: String,
    /// `MICO` — the small icon filename.
    pub repu_mico: String,

    // RGDL (Ragdoll) fields
    /// `NVER` Version — the uint32.
    pub rgdl_version: u32,
    /// `DATA` Dynamic Bone Count — the uint32.
    pub rgdl_dynamic_bone_count: u32,
    /// `DATA` Feedback Enabled — the u8 enum: 0 is No, 1 is Yes.
    pub rgdl_feedback_enabled: u8,
    /// `DATA` Foot IK Enabled — the u8 enum.
    pub rgdl_foot_ik_enabled: u8,
    /// `DATA` Look IK Enabled — the u8 enum.
    pub rgdl_look_ik_enabled: u8,
    /// `DATA` Grab IK Enabled — the u8 enum.
    pub rgdl_grab_ik_enabled: u8,
    /// `DATA` Pose Matching — the u8 enum.
    pub rgdl_pose_matching: u8,
    /// `XNAM` Actor Base — the FormID of a `CREA` or `NPC_` record.
    pub rgdl_actor_base: Option<u32>,
    /// `TNAM` Body Part Data — the FormID of a `BPTD` record.
    pub rgdl_body_part_data: Option<u32>,
    /// `ANAM` Death Pose — the cstring.
    pub rgdl_death_pose: String,
    /// `RAFB` Feedback Dynamic Bones — the uint16 list.
    pub rgdl_feedback_bones: Vec<u16>,
    /// `RAFD` Dynamic / Keyframe Blend Amount — the float32.
    pub rgdl_dynamic_keyframe_blend: f32,
    /// `RAFD` Hierarchy Gain — the float32.
    pub rgdl_hierarchy_gain: f32,
    /// `RAFD` Position Gain — the float32.
    pub rgdl_position_gain: f32,
    /// `RAFD` Velocity Gain — the float32.
    pub rgdl_velocity_gain: f32,
    /// `RAFD` Acceleration Gain — the float32.
    pub rgdl_acceleration_gain: f32,
    /// `RAFD` Snap Gain — the float32.
    pub rgdl_snap_gain: f32,
    /// `RAFD` Velocity Damping — the float32.
    pub rgdl_velocity_damping: f32,
    /// `RAFD` Snap Max Linear Velocity — the float32.
    pub rgdl_snap_max_linear_velocity: f32,
    /// `RAFD` Snap Max Angular Velocity — the float32.
    pub rgdl_snap_max_angular_velocity: f32,
    /// `RAFD` Snap Max Linear Distance — the float32.
    pub rgdl_snap_max_linear_distance: f32,
    /// `RAFD` Snap Max Angular Distance — the float32.
    pub rgdl_snap_max_angular_distance: f32,
    /// `RAFD` Position Max Linear Velocity — the float32.
    pub rgdl_position_max_linear_velocity: f32,
    /// `RAFD` Position Max Angular Velocity — the float32.
    pub rgdl_position_max_angular_velocity: f32,
    /// `RAFD` Projectile Position Max Velocity — the int32. The game divides
    /// the value by 1000.
    pub rgdl_projectile_position_max_velocity: i32,
    /// `RAFD` Melee Position Max Velocity — the uint32. The game divides the
    /// value by 1000.
    pub rgdl_melee_position_max_velocity: u32,
    /// `RAPS` Match Bone 1 — the uint16.
    pub rgdl_match_bone1: u16,
    /// `RAPS` Match Bone 2 — the uint16.
    pub rgdl_match_bone2: u16,
    /// `RAPS` Match Bone 3 — the uint16.
    pub rgdl_match_bone3: u16,
    /// `RAPS` Flags — the u8 flag word: Disable On Move.
    pub rgdl_aps_flags: u8,
    /// `RAPS` Motors Strength — the float32.
    pub rgdl_motors_strength: f32,
    /// `RAPS` Pose Activation Delay Time — the float32.
    pub rgdl_pose_activation_delay: f32,
    /// `RAPS` Match Error Allowance — the float32.
    pub rgdl_match_error_allowance: f32,
    /// `RAPS` Displacement To Disable — the float32.
    pub rgdl_displacement_to_disable: f32,

    // TREE (Tree) fields
    /// `SNAM` SpeedTree Seeds — the uint32 list.
    pub tree_seeds: Vec<u32>,
    /// `CNAM` Leaf Curvature — the float32.
    pub tree_leaf_curvature: f32,
    /// `CNAM` Minimum Leaf Angle — the float32.
    pub tree_min_leaf_angle: f32,
    /// `CNAM` Maximum Leaf Angle — the float32.
    pub tree_max_leaf_angle: f32,
    /// `CNAM` Branch Dimming Value — the float32.
    pub tree_branch_dimming: f32,
    /// `CNAM` Leaf Dimming Value — the float32.
    pub tree_leaf_dimming: f32,
    /// `CNAM` Shadow Radius — the int32.
    pub tree_shadow_radius: i32,
    /// `CNAM` Rock Speed — the float32.
    pub tree_rock_speed: f32,
    /// `CNAM` Rustle Speed — the float32.
    pub tree_rustle_speed: f32,
    /// `BNAM` Width — the float32.
    pub tree_billboard_width: f32,
    /// `BNAM` Height — the float32.
    pub tree_billboard_height: f32,
    /// `ICON` — the large icon filename.
    pub tree_icon: String,
    /// `MICO` — the small icon filename.
    pub tree_mico: String,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub tree_model: ModelData,

    // TXST (Texture) fields
    /// `TX00` through `TX05` — the six texture paths.
    pub txst_textures: Vec<String>,
    /// `DNAM` Flags — the uint16 flag word: No Specular Map.
    pub txst_flags: u16,
    /// `DODT` Min Width — the float32.
    pub txst_dodt_min_width: f32,
    /// `DODT` Max Width — the float32.
    pub txst_dodt_max_width: f32,
    /// `DODT` Min Height — the float32.
    pub txst_dodt_min_height: f32,
    /// `DODT` Max Height — the float32.
    pub txst_dodt_max_height: f32,
    /// `DODT` Depth — the float32.
    pub txst_dodt_depth: f32,
    /// `DODT` Shininess — the float32.
    pub txst_dodt_shininess: f32,
    /// `DODT` Parallax Scale — the float32.
    pub txst_dodt_parallax_scale: f32,
    /// `DODT` Parallax Passes — the u8.
    pub txst_dodt_parallax_passes: u8,
    /// `DODT` Flags — the u8 flag word: Parallax, Alpha Blending, Alpha
    /// Testing.
    pub txst_dodt_flags: u8,
    /// `DODT` Color — the rgba quadruple.
    pub txst_dodt_color: [u8; 4],

    // VTYP (Voice Type) fields
    /// `DNAM` Flags — the u8 flag word: Allow Default Dialog, Female.
    pub vtyp_flags: u8,

    // TACT (Talking Activator) fields
    /// `SNAM` Looping Sound — the FormID of a `SOUN` record.
    pub tact_looping_sound: Option<u32>,
    /// `VNAM` Voice Type — the FormID of a `VTYP` record.
    pub tact_voice_type: Option<u32>,
    /// `INAM` Radio Template — the FormID of a `SOUN` record.
    pub tact_radio_template: Option<u32>,
    /// `SCRI` Script — the FormID of a `SCPT` record.
    pub tact_script: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub tact_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub tact_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32.
    pub tact_dest_health: i32,
    /// `DEST` Flags — the u8 flag word.
    pub tact_dest_flags: u8,

    // SCOL (Static Collision) fields
    /// The collision parts. Each is an `ONAM` (the static) and a `DATA`
    /// (the placements), and both repeat, so the parts are a list.
    pub scol_parts: Vec<ScolPart>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub scol_model: ModelData,

    // IDLE (Idle Animation) fields
    /// `ANAM` Parent Idle Animation — the FormID of an `IDLE` record.
    pub idle_parent: Option<u32>,
    /// `ANAM` Previous Sibling Idle Animation — the FormID of an `IDLE`
    /// record.
    pub idle_previous_sibling: Option<u32>,
    /// `DATA` Animation Group Section — the u8 enum.
    pub idle_group_section: u8,
    /// `DATA` Looping Min — the u8.
    pub idle_looping_min: u8,
    /// `DATA` Looping Max — the u8.
    pub idle_looping_max: u8,
    /// `DATA` Replay Delay — the int16.
    pub idle_replay_delay: i16,
    /// `DATA` Flags — the u8 flag word.
    pub idle_flags: u8,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub idle_model: ModelData,
    /// The `CTDA` conditions — the repeated condition list.
    pub idle_conditions: Vec<Condition>,

    // MSET (Media Set) fields
    /// `NAM1` Type — the int32 enum: -1 is No Set, 0 is Battle Set, 1 is
    /// Location Set, 2 is Dungeon Set and 3 is Incidental Set.
    pub mset_type: i32,
    /// `NAM2` through `NAM7` — the six cstring track names.
    pub mset_tracks: Vec<String>,
    /// `NAM8` through `CNAM` — the six float32 track levels.
    pub mset_levels: Vec<f32>,
    /// `JNAM` — the six float32 boundary percentages. The subrecord repeats,
    /// so the boundaries are a list.
    pub mset_boundaries: Vec<f32>,
    /// `PNAM` Enable Flags — the u8 flag word.
    pub mset_enable_flags: u8,
    /// `DNAM` Wait Time / Min Time On / Daytime Min — the float32.
    pub mset_wait_time: f32,
    /// `ENAM` Loop Fade Out / Crossfade Overlap / Nighttime Min — the float32.
    pub mset_loop_fade_out: f32,
    /// `FNAM` Recovery Time / Layer Crossfade Time / Daytime Max — the
    /// float32.
    pub mset_recovery_time: f32,
    /// `GNAM` Nighttime Max — the float32.
    pub mset_nighttime_max: f32,
    /// `HNAM` Intro / Daytime — the FormID of a `SOUN` record.
    pub mset_intro: Option<u32>,
    /// `INAM` Outro / Nighttime — the FormID of a `SOUN` record.
    pub mset_outro: Option<u32>,

    // MSTT (Movable Static) fields
    /// `DATA` — the one byte the spec calls Unknown.
    pub mstt_data: u8,
    /// `SNAM` Sound — the FormID of a `SOUN` record.
    pub mstt_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub mstt_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub mstt_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32.
    pub mstt_dest_health: i32,
    /// `DEST` Flags — the u8 flag word.
    pub mstt_dest_flags: u8,

    // PWAT (Water) fields
    /// `DNAM` Flags — the uint32 flag word.
    pub pwat_flags: u32,
    /// `DNAM` Water — the FormID of a `WATR` record.
    pub pwat_water: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub pwat_model: ModelData,

    // FLST (Form List) fields
    /// `LNAM` — the list of FormIDs. The subrecord repeats, so the list is
    /// a list.
    pub flst_entries: Vec<u32>,

    // MESG (Message) fields
    /// `DESC` Description — the cstring.
    pub mesg_desc: String,
    /// `INAM` Icon — the FormID of a `MICN` record.
    pub mesg_icon: Option<u32>,
    /// `DNAM` Flags — the uint32 flag word.
    pub mesg_flags: u32,
    /// `TNAM` Display Time — the uint32.
    pub mesg_display_time: u32,
    /// The menu buttons. Each is an `ITXT` (the text) and a repeated
    /// `CTDA` (the conditions), and both repeat.
    pub mesg_buttons: Vec<MessageButton>,

    // MISC (Miscellaneous) fields
    /// `DATA` Value — the int32.
    pub misc_value: i32,
    /// `DATA` Weight — the float32.
    pub misc_weight: f32,
    /// `ICON` — the large icon filename.
    pub misc_icon: String,
    /// `MICO` — the small icon filename.
    pub misc_mico: String,
    /// `SCRI` Script — the FormID of a `SCPT` record.
    pub misc_script: Option<u32>,
    /// `YNAM` Sound - Pick Up — the FormID of a `SOUN` record.
    pub misc_pickup_sound: Option<u32>,
    /// `ZNAM` Sound - Drop — the FormID of a `SOUN` record.
    pub misc_drop_sound: Option<u32>,
    /// `RNAM` Sound - Random / Looping — the FormID of a `SOUN` record.
    pub misc_random_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub misc_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub misc_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32.
    pub misc_dest_health: i32,
    /// `DEST` Flags — the u8 flag word.
    pub misc_dest_flags: u8,

    // SPEL (Spell) fields
    /// `SPIT` Type — the uint32 enum.
    pub spel_type: u32,
    /// `SPIT` Cost — the uint32.
    pub spel_cost: u32,
    /// `SPIT` Level — the uint32.
    pub spel_level: u32,
    /// `SPIT` Flags — the u8 flag word.
    pub spel_flags: u8,
    /// The effects. Each is an `EFID` (the effect) and an `EFIT` (the
    /// parameters), and both repeat.
    pub spel_effects: Vec<SpellEffect>,

    // STAT (Static) fields
    /// `BRUS` Passthrough Sound — the int8 enum: -1 is None, 0 is BushA and
    /// so on up to 9 being BushJ.
    pub stat_passthrough: i8,
    /// `RNAM` Sound - Looping / Random — the FormID of a `SOUN` record.
    pub stat_looping_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub stat_model: ModelData,

    // KEYM (Key) fields
    /// `DATA` Value — the int32.
    pub keym_value: i32,
    /// `DATA` Weight — the float32.
    pub keym_weight: f32,
    /// `ICON` — the large icon filename.
    pub keym_icon: String,
    /// `MICO` — the small icon filename.
    pub keym_mico: String,
    /// `SCRI` Script — the FormID of a `SCPT` record.
    pub keym_script: Option<u32>,
    /// `YNAM` Sound - Pick Up — the FormID of a `SOUN` record.
    pub keym_pickup_sound: Option<u32>,
    /// `ZNAM` Sound - Drop — the FormID of a `SOUN` record.
    pub keym_drop_sound: Option<u32>,
    /// `RNAM` Sound - Random / Looping — the FormID of a `SOUN` record.
    pub keym_random_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub keym_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub keym_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32.
    pub keym_dest_health: i32,
    /// `DEST` Flags — the u8 flag word.
    pub keym_dest_flags: u8,

    // INGR (Ingredient) fields
    /// `ETYP` Equipment Type — the int32 enum.
    pub ingr_equipment_type: i32,
    /// `DATA` Weight — the float32.
    pub ingr_weight: f32,
    /// `ENIT` Value — the int32.
    pub ingr_effect_value: i32,
    /// `ENIT` Flags — the u8 flag word: No Auto-Calculation, Food Item.
    pub ingr_effect_flags: u8,
    /// `ICON` — the large icon filename.
    pub ingr_icon: String,
    /// `MICO` — the small icon filename.
    pub ingr_mico: String,
    /// `SCRI` Script — the FormID of a `SCPT` record.
    pub ingr_script: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub ingr_model: ModelData,
    /// The effects. Each is an `EFID` (the effect) and an `EFIT` (the
    /// parameters), and both repeat.
    pub ingr_effects: Vec<SpellEffect>,

    // IPCT (Impact) fields
    /// `DATA` Effect - Duration — the float32.
    pub ipct_duration: f32,
    /// `DATA` Effect - Orientation — the uint32 enum.
    pub ipct_orientation: u32,
    /// `DATA` Angle Threshold — the float32.
    pub ipct_angle_threshold: f32,
    /// `DATA` Placement Radius — the float32.
    pub ipct_placement_radius: f32,
    /// `DATA` Sound Level — the uint32.
    pub ipct_sound_level: u32,
    /// `DATA` Flags — the uint32 flag word.
    pub ipct_flags: u32,
    /// `DNAM` Texture Set — the FormID of a `TXST` record.
    pub ipct_texture_set: Option<u32>,
    /// `SNAM` Sound 1 — the FormID of a `SOUN` record.
    pub ipct_sound1: Option<u32>,
    /// `NAM1` Sound 2 — the FormID of a `SOUN` record.
    pub ipct_sound2: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub ipct_model: ModelData,
    /// The `DODT` decal data.
    pub ipct_dodt: Option<DecalData>,

    // IPDS (Impact Data Set) fields
    /// `DATA` — the twelve material impact FormIDs. A null one is written as
    /// zero.
    pub ipds_impacts: Vec<Option<u32>>,

    // RCPE (Recipe) fields
    /// `DATA` Skill — the int32 actor value.
    pub rcpe_skill: i32,
    /// `DATA` Level — the uint32.
    pub rcpe_level: u32,
    /// `DATA` Category — the FormID of an `RCCT` record.
    pub rcpe_category: Option<u32>,
    /// `DATA` Sub-Category — the FormID of an `RCCT` record.
    pub rcpe_sub_category: Option<u32>,
    /// The conditions — the repeated `CTDA` list.
    pub rcpe_conditions: Vec<Condition>,
    /// The ingredients. Each is an `RCIL` (the item) and an `RCQY` (the
    /// quantity), and both repeat.
    pub rcpe_ingredients: Vec<RecipeItem>,
    /// The outputs. Each is an `RCIL` (the item) and an `RCQY` (the
    /// quantity), and both repeat.
    pub rcpe_outputs: Vec<RecipeItem>,

    // SCPT (Script) fields
    /// `SCHR` — the basic script data.
    pub scpt_header: ScriptHeader,
    /// `SCDA` — the compiled script source.
    pub scpt_compiled: Vec<u8>,
    /// `SCTX` — the script source.
    pub scpt_source: String,
    /// The local variables. Each is an `SLSD` (the data) and a `SCVR` (the
    /// name), and both repeat.
    pub scpt_variables: Vec<ScriptVariable>,

    // IMAD (Image Space Adapter) fields
    /// `DNAM` — the data header.
    pub imad_header: ImadHeader,
    /// The curves. Each subrecord is one curve, and the curve's signature
    /// says what it holds.
    pub imad_curves: Vec<ImadCurve>,

    // EFSH (Effect Shader) fields
    /// `DATA` — the effect shader's data struct.
    pub efsh_data: EfshData,
    /// `ICON` — the fill texture's filename.
    pub efsh_icon: String,
    /// `ICO2` — the particle shader texture's filename.
    pub efsh_ico2: String,
    /// `NAM7` — the holes texture's filename.
    pub efsh_nam7: String,

    // PMIS (Placed Missile) fields
    /// `NAME` Base — the FormID of the `PROJ` record.
    pub pmis_base: Option<u32>,
    /// `XEZN` Encounter Zone — the FormID of the `ECZN` record.
    pub pmis_encounter_zone: Option<u32>,
    /// `XPRD` Idle Time — the float32.
    pub pmis_idle_time: f32,
    /// `INAM` Idle — the FormID of the `IDLE` record.
    pub pmis_idle: Option<u32>,
    /// `TNAM` Topic — the FormID of the `DIAL` record.
    pub pmis_topic: Option<u32>,
    /// `XOWN` Owner — the FormID of the owner.
    pub pmis_owner: Option<u32>,
    /// `XRGD` Ragdoll Data — the raw bytes.
    pub pmis_ragdoll: Vec<u8>,
    /// `XRGB` Ragdoll Biped Data — the raw bytes.
    pub pmis_ragdoll_biped: Vec<u8>,
    /// `XSCL` Scale — the float32.
    pub pmis_scale: f32,
    /// `DATA` Position — the three float32s.
    pub pmis_position: [f32; 3],
    /// `DATA` Rotation — the three float32s.
    pub pmis_rotation: [f32; 3],
    /// `DATA` Bounding Radius — the float32.
    pub pmis_bounding_radius: f32,
    /// `DATA` Flags — the uint32.
    pub pmis_flags: u32,
    /// `DATA` Unknown — the six unnamed bytes.
    pub pmis_unknown: [u8; 6],

    // COBJ (Constructible Object) fields
    /// `DATA` Value — the int32.
    pub cobj_value: i32,
    /// `DATA` Weight — the float32.
    pub cobj_weight: f32,
    /// The model data collection.
    pub cobj_model: ModelData,

    // REGN (Region) fields
    /// `RCLR` Map Color — the four bytes: red, green, blue and alpha.
    pub regn_map_color: [u8; 4],
    /// `WNAM` Worldspace — the FormID of a `WRLD` record.
    pub regn_worldspace: Option<u32>,
    /// The region area. Each entry is the edge fall-off and the point list
    /// that belongs to it.
    pub regn_areas: Vec<RegionArea>,
    /// The region data entries.
    pub regn_entries: Vec<RegionEntry>,

    // PACK (Package) fields
    /// `PKDT` General Flags — the uint32 flag word.
    pub pack_general_flags: u32,
    /// `PKDT` Type — the uint8 enum: Find, Follow, Escort, Eat, Sleep,
    /// Wander, Travel, Accompany, Use Item At, Ambush, Flee Not Combat,
    /// Sandbox, Patrol, Guard, Dialogue, Use Weapon.
    pub pack_type: u8,
    /// `PKDT` Unused — the byte the spec leaves unnamed.
    pub pack_unused: u8,
    /// `PKDT` Fallout Behaviour Flags — the uint16 flag word.
    pub pack_fallout_behavior_flags: u16,
    /// `PKDT` Type-Specific Flags — the uint16, or None when the record used
    /// the short form that has no type-specific flags.
    pub pack_type_specific_flags: Option<u16>,
    /// `PKDT` Unused 2 — the two bytes the spec leaves unnamed.
    pub pack_unused2: [u8; 2],
    /// `PSDT` Month — the int8.
    pub pack_month: i8,
    /// `PSDT` Day of Week — the int8 enum.
    pub pack_day_of_week: i8,
    /// `PSDT` Date — the uint8.
    pub pack_date: u8,
    /// `PSDT` Time — the int8.
    pub pack_time: i8,
    /// `PSDT` Duration — the int32, in hours.
    pub pack_duration: i32,
    /// The conditions. Each is a `CTDA` and they repeat.
    pub pack_conditions: Vec<Condition>,
    /// The first location. A `PLDT` subrecord.
    pub pack_location: PackageLocation,
    /// The second location. A `PLD2` subrecord.
    pub pack_location2: PackageLocation,
    /// The first target. A `PTDT` subrecord.
    pub pack_target: PackageTarget,
    /// The second target. A `PTD2` subrecord.
    pub pack_target2: PackageTarget,
    /// `IDLF` Idle Animation Flags — the uint8 flag word.
    pub pack_idle_flags: u8,
    /// `IDLC` Idle Animation Count — the uint8, plus the three unnamed bytes.
    pub pack_idle_count: u8,
    pub pack_idle_count_unused: [u8; 3],
    /// `IDLT` Idle Timer Setting — the float32.
    pub pack_idle_timer: f32,
    /// `IDLA` — the array of `IDLE` record FormIDs.
    pub pack_idle_animations: Vec<u32>,
    /// `IDLB` Unused — the four bytes.
    pub pack_idle_unused: [u8; 4],
    /// `CNAM` Combat Style — the FormID of a `CSTY` record.
    pub pack_combat_style: Option<u32>,
    /// `PKED` Eat Marker — present or not.
    pub pack_eat_marker: bool,
    /// `PKE2` Escort Distance — the uint32.
    pub pack_escort_distance: Option<u32>,
    /// `PKFD` Follow Start Location Trigger Radius — the float32.
    pub pack_follow_radius: Option<f32>,
    /// `PKPT` Patrol Flags — the uint16.
    pub pack_patrol_flags: Option<u16>,
    /// `PKW3` Use Weapon Data.
    pub pack_weapon: PackageWeapon,
    /// `PUID` Use Item Marker — present or not.
    pub pack_use_item_marker: bool,
    /// `PKAM` Ambush Marker — present or not.
    pub pack_ambush_marker: bool,
    /// `PKDD` Dialog Data.
    pub pack_dialog: PackageDialog,
    /// The embedded scripts. Each is a marker (`POBA`, `POEA` or `POCA`), an
    /// `INAM` idle, a script collection and a `TNAM` topic, and they repeat.
    pub pack_scripts: Vec<PackageScript>,

    // TES4 (Plugin Header) fields
    /// `HEDR` version — the float32.
    pub tes4_version: f32,
    /// `HEDR` numRecords — the uint32.
    pub tes4_num_records: u32,
    /// `HEDR` nextObjectId — the uint32.
    pub tes4_next_object_id: u32,
    /// `CNAM` author — the cstring.
    pub tes4_author: String,
    /// `SNAM` description — the cstring.
    pub tes4_description: String,
    /// The master files. Each is a `MAST` (the filename) and a `SSIG` (the
    /// FormID), and both repeat.
    pub tes4_masters: Vec<Tes4Master>,

    // CHAL (Challenge) fields
    /// `DATA` Type — the uint32 enum: the fourteen kinds of challenge.
    pub chal_type: u32,
    /// `DATA` Threshold — the uint32 the challenge is measured against.
    pub chal_threshold: u32,
    /// `DATA` Flags — the uint32 flag word: Start Disabled, Recurring, Show
    /// Zero Progress. The word is kept whole, so a bit the spec does not
    /// name survives an edit.
    pub chal_flags: u32,
    /// `DATA` Interval — the uint32.
    pub chal_interval: u32,
    /// `DATA` Value1 — two bytes whose meaning depends on the type.
    pub chal_value1: [u8; 2],
    /// `DATA` Value2 — two bytes whose meaning depends on the type.
    pub chal_value2: [u8; 2],
    /// `DATA` Value3 — four bytes whose meaning depends on the type.
    pub chal_value3: [u8; 4],
    /// `SNAM` Value3 — the FormID, for the types whose Value3 is a reference.
    pub chal_snam: Option<u32>,
    /// `XNAM` Value4 — the FormID, for the types whose Value4 is a reference.
    pub chal_xnam: Option<u32>,
    /// `ICON` — the Pip-Boy icon.
    pub chal_icon: String,
    /// `MICO` — the message icon.
    pub chal_mico: String,
    /// `DESC` — the description.
    pub chal_desc: String,

    // CPTH (Camera Path) fields
    /// `DATA` Camera Zoom — the u8 enum: Default, Disable, Shot List.
    pub cpth_camera_zoom: u8,
    /// `ANAM` Parent Camera Path — the FormID of another `CPTH` record.
    pub cpth_parent: Option<u32>,
    /// `ANAM` Previous Sibling Camera Path — the FormID of another `CPTH`.
    pub cpth_previous: Option<u32>,
    /// `SNAM` Camera Shot — the FormIDs of the `CAMS` records on the path.
    /// `SNAM` is a *repeated* subrecord, so the shots are a list.
    pub cpth_shots: Vec<u32>,
    /// `CTDA` — the conditions on the path. `CTDA` is a *repeated*
    /// subrecord, so the conditions are a list.
    pub cpth_conditions: Vec<Condition>,

    // CSNO (Casino) fields
    /// `DATA` Decks % Before Shuffle — the float32.
    pub csno_decks_shuffle: f32,
    /// `DATA` BlackJack Payout Ratio — the float32.
    pub csno_payout_ratio: f32,
    /// `DATA` Slot Reel Stop — the seven uint32 values: symbols 1 to 6 and W.
    pub csno_reel_stops: Vec<u32>,
    /// `DATA` Number of Decks — the uint32.
    pub csno_number_of_decks: u32,
    /// `DATA` Max Winnings — the uint32.
    pub csno_max_winnings: u32,
    /// `DATA` Currency — the FormID of a `CHIP` record.
    pub csno_currency: Option<u32>,
    /// `DATA` Casino Winnings Quest — the FormID of a `QUST` record.
    pub csno_winnings_quest: Option<u32>,
    /// `DATA` Flags — the uint32 flag word: Dealer Stay on Soft 17.
    pub csno_flags: u32,
    /// The chip models: `MODL` carries seven of them, then `MOD2`, `MOD3` and
    /// `MOD4` carry the slot machine, blackjack table and roulette table.
    pub csno_models: Vec<String>,
    /// The slot reel textures: `ICON` carries seven of them.
    pub csno_icons: Vec<String>,
    /// The blackjack deck textures: `ICO2` carries four of them.
    pub csno_deck_icons: Vec<String>,

    // DEBR (Debris) fields
    /// The debris models. Each is a `DATA` (percentage, model filename, flags)
    /// and a `MODT` (texture hashes), and both repeat, so the collection is a
    /// list of pairs.
    pub debr_models: Vec<DebrisModel>,

    // CAMS (Camera Shot) fields
    /// `DATA` Action — the uint32 enum: Shoot, Fly, Hit, Zoom.
    pub cams_action: u32,
    /// `DATA` Location — the uint32 enum: Attacker, Projectile, Target.
    pub cams_location: u32,
    /// `DATA` Target — the uint32 enum: Attacker, Projectile, Target.
    pub cams_target: u32,
    /// `DATA` Flags — the uint32 flag word. The word is kept whole, so a bit
    /// the spec does not name survives an edit.
    pub cams_flags: u32,
    /// `DATA` Player Time Multiplier.
    pub cams_player_time_mult: f32,
    /// `DATA` Target Time Multiplier.
    pub cams_target_time_mult: f32,
    /// `DATA` Global Time Multiplier.
    pub cams_global_time_mult: f32,
    /// `DATA` Max Time.
    pub cams_max_time: f32,
    /// `DATA` Min Time.
    pub cams_min_time: f32,
    /// `DATA` Target % Between Actors.
    pub cams_target_percent: f32,
    /// `MNAM` Image Space Modifier — the FormID of an `IMAD` record.
    pub cams_image_space_modifier: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    ///
    /// The collection is a set of subrecords that always appear together, so
    /// it is kept as one struct rather than as loose fields. `MODT`'s texture
    /// hashes are kept as raw bytes: the editor cannot author a file hash, and
    /// rewriting one would corrupt the mod.
    pub cams_model: ModelData,

    // CCRD (Card) fields
    /// `INTV` — the two uint32 values: the card suit and the card value.
    /// `INTV` is a *repeated* subrecord, so the pair is a list.
    pub ccrd_intv: Vec<u32>,
    /// `DATA` Value — the uint32 the card is worth.
    pub ccrd_value: u32,
    /// `ICON` — the large icon filename.
    pub ccrd_icon: String,
    /// `MICO` — the small icon filename.
    pub ccrd_mico: String,
    /// `TX00` — the high-res face image.
    pub ccrd_face: String,
    /// `TX01` — the high-res back image.
    pub ccrd_back: String,
    /// `YNAM` — the pick-up sound.
    pub ccrd_pickup_sound: Option<u32>,
    /// `ZNAM` — the drop sound.
    pub ccrd_drop_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub ccrd_model: ModelData,

    // CDCK (Caravan Deck) fields
    /// `CARD` — the `CCRD` records on the deck. `CARD` is a *repeated*
    /// subrecord, so the deck is a list.
    pub cdck_cards: Vec<u32>,
    /// `DATA` — the uint32 the spec marks broken. It is carried through
    /// unchanged rather than being given a meaning the spec does not state.
    pub cdck_data: u32,

    // CHIP (Casino Chip) fields
    /// `ICON` — the large icon filename.
    pub chip_icon: String,
    /// `MICO` — the small icon filename.
    pub chip_mico: String,
    /// `YNAM` — the pick-up sound.
    pub chip_pickup_sound: Option<u32>,
    /// `ZNAM` — the drop sound.
    pub chip_drop_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub chip_model: ModelData,
    /// The destruction data: the `DEST` header and the `DSTD` stages.
    pub chip_destruction: Vec<DestructionStage>,
    /// `DEST` Health — the int32 the object's health starts at.
    pub chip_dest_health: i32,
    /// `DEST` Flags — the u8 flag word. Bit 0 is "VATS Targetable"; the byte
    /// is kept whole so a bit the spec does not name survives.
    pub chip_dest_flags: u8,

    // CMNY (Money) fields
    /// `DATA` Absolute Value — the uint32 the money is worth.
    pub cmny_value: u32,
    /// `ICON` — the large icon filename.
    pub cmny_icon: String,
    /// `MICO` — the small icon filename.
    pub cmny_mico: String,
    /// `YNAM` — the pick-up sound.
    pub cmny_pickup_sound: Option<u32>,
    /// `ZNAM` — the drop sound.
    pub cmny_drop_sound: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    pub cmny_model: ModelData,

    // ARMA (Armor Addon) fields
    /// `DATA` Value — the int32 the armour is worth.
    pub arma_val: i32,
    /// `DATA` Max Condition — the int32 hit points.
    pub arma_hp: i32,
    /// `DATA` Weight — the float32.
    pub arma_weight: f32,
    /// `DNAM` AR — the int16 armour rating, a hundredth of a point.
    pub arma_dr: i16,
    /// `DNAM` Flags — the uint16 flag word. Only bit 0 ("Modulates Voice")
    /// is named by the spec, so the word is kept whole.
    pub arma_dnam_flags: u16,
    /// `DNAM` DT — the float32 damage threshold.
    pub arma_dt: f32,
    /// The length the record's `DNAM` had: four bytes on 115 of the game's 131
    /// records, twelve on the rest. A rebuild would grow the short ones and
    /// give them fields they never had.
    pub arma_dnam_len: usize,
    /// `BMDT` biped flags — the uint32 word naming the body parts.
    pub arma_biped_flags: u32,
    /// `BMDT` general flags — the u8 byte. "Non-Playable" is set on records
    /// the spec does not name it for, so the byte is kept whole.
    pub arma_bmdt_general: u8,
    /// `ETYP` Equipment Type — the int32 enum.
    pub arma_equip_type: i32,
    /// The four model data instances: male biped, male world, female biped and
    /// female world.
    ///
    /// `MODB` is present only in the original instance, and `MODD` only in the
    /// original and the third, where it is spelled `MOSD`.
    pub arma_models: Vec<ModelData>,
    /// `ICON` and `MICO` — the male inventory and message icons.
    pub arma_icon: String,
    pub arma_mico: String,
    /// `ICO2` and `MIC2` — the female inventory and message icons.
    pub arma_ico2: String,
    pub arma_mic2: String,

    // ASPC (Ambient Sound) fields
    /// `SNAM` — the five sound loops, in the order the record stores them:
    /// dawn, afternoon, dusk, night and walla. Each is a `SOUN` FormID.
    pub aspc_sounds: Vec<u32>,
    /// `WNAM` Walla Trigger Count.
    pub aspc_walla_trigger_count: u32,
    /// `RDAT` Use Sound from Region — the FormID of a `REGN` record.
    pub aspc_region: Option<u32>,
    /// `ANAM` Environment Type — the uint32 enum.
    pub aspc_environment: u32,
    /// `INAM` Is Interior — the uint32 enum: 1 is No, 2 is Yes.
    pub aspc_is_interior: u32,

    // AMEF (Ammo Effect) fields
    /// `DATA` Type — the uint32 enum: Damage Mod, DR Mod, DT Mod, Spread Mod,
    /// Weapon Condition Mod, Fatigue Mod.
    pub amef_type: u32,
    /// `DATA` Operation — the uint32 enum: Add, Multiply, Subtract.
    pub amef_operation: u32,
    /// `DATA` Value — the float32 the effect applies.
    pub amef_value: f32,

    // ANIO (Animation Object) fields
    /// `DATA` Animation — the FormID of an `IDLE` record.
    pub anio_animation: Option<u32>,
    /// The model data collection: `MODL`, `MODB`, `MODT`, `MODS` and `MODD`.
    ///
    /// The collection is a set of subrecords that always appear together, so
    /// it is kept as one struct rather than as loose fields. `MODT`'s texture
    /// hashes are kept as raw bytes: the editor cannot author a file hash, and
    /// rewriting one would corrupt the mod.
    pub anio_model: ModelData,

    // ALOC (Media Location Controller) fields
    /// `NAM1` — four bytes the spec calls "possibly a combination of flags and
    /// enums". The bytes are kept whole and offered raw.
    pub aloc_nam1: [u8; 4],
    /// `NAM2` — four unknown bytes, carried through unchanged.
    pub aloc_nam2: [u8; 4],
    /// `NAM3` — four unknown bytes, carried through unchanged.
    pub aloc_nam3: [u8; 4],
    /// `NAM4` Location Delay, in seconds.
    pub aloc_location_delay: f32,
    /// `NAM5` Day Start — the uint32 the game stores as an hour, with the
    /// fraction of a day over 256.
    pub aloc_day_start: u32,
    /// `NAM6` Night Start — the same encoding as `NAM5`.
    pub aloc_night_start: u32,
    /// `NAM7` Retrigger Delay, in seconds.
    pub aloc_retrigger_delay: f32,
    /// `FNAM` — four unknown bytes, carried through unchanged.
    pub aloc_fnam: [u8; 4],
    /// `RNAM` Conditional Faction — the FormID of a `FACT` record.
    pub aloc_conditional_faction: Option<u32>,
    /// The six media-set lists: `HNAM` (neutral), `ZNAM` (ally), `XNAM`
    /// (friend), `YNAM` (enemy), `LNAM` (location) and `GNAM` (battle).
    ///
    /// Each is a *repeated* subrecord, so each is a list. A per-subrecord
    /// edit would collapse the list onto its last entry.
    pub aloc_sets: Vec<AlocSet>,
    /// The media set to add, and which list it goes on.
    pub aloc_set_to_add: Option<u32>,
    pub aloc_set_list_to_add: usize,

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

/// Which model data instance of an `ARMA` record a signature belongs to.
///
/// `ARMA` carries four model collections: the male biped model (instance 0),
/// the male world model (instance 1), the female biped model (instance 2) and
/// the female world model (instance 3). The signatures are the shared Model
/// Data page's numbered ones: `MODL`/`MOD2`/`MOD3`/`MOD4`, `MODT`/`MO2T`/
/// `MO3T`/`MO4T` and `MODS`/`MO2S`/`MO3S`/`MO4S`. `MODB` exists only in the
/// original instance and `MODD` in the original and the third, where it is
/// spelled `MOSD`.
fn arma_model_instance(sig: &[u8; 4]) -> Option<usize> {
    match sig {
        b"MODL" | b"MODB" | b"MODT" | b"MODS" | b"MODD" => Some(0),
        b"MOD2" | b"MO2T" | b"MO2S" => Some(1),
        b"MOD3" | b"MO3T" | b"MO3S" | b"MOSD" => Some(2),
        b"MOD4" | b"MO4T" | b"MO4S" => Some(3),
        _ => None,
    }
}

/// One destruction stage, from the shared Destruction Data collection.
///
/// The collection is `DEST` (the header) followed by one or more stages, each
/// a run of `DSTD`, `DMDL`, `DMDT` and `DSTF`. The stage's fields are the
/// spec's `DSTD` struct.
#[derive(Debug, Clone, Default)]
pub struct DestructionStage {
    /// `DSTD` Health Percentage — the uint8 percentage of the object's health
    /// at which this stage begins.
    pub health_percent: u8,
    /// `DSTD` Index — the uint8 stage number.
    pub index: u8,
    /// `DSTD` Damage Stage — the uint8.
    pub damage_stage: u8,
    /// `DSTD` Flags — the uint8 flag word: Cap Damage, Disable, Destroy. The
    /// word is kept whole, so a bit the spec does not name survives an edit.
    pub flags: u8,
    /// `DSTD` Self Damage per Second — the int32.
    pub self_damage_per_second: i32,
    /// `DSTD` Explosion — the FormID of an `EXPL` record.
    pub explosion: Option<u32>,
    /// `DSTD` Debris — the FormID of a `DEBR` record.
    pub debris: Option<u32>,
    /// `DSTD` Debris Count — the int32.
    pub debris_count: i32,
    /// `DMDL` — the stage model's filename.
    pub model: String,
    /// `DMDT` — the stage model's texture file hashes, copied through
    /// unchanged: the editor cannot author a file hash.
    pub texture_hashes: Vec<u8>,
}

/// Read the shared Destruction Data collection out of a record's subrecords.
///
/// The collection is `DEST` (the header) followed by one or more stages, each
/// a run of `DSTD`, `DMDL`, `DMDT` and `DSTF`. The stages are walked in file
/// order, because `raw_subs` holds one value per signature and would collapse
/// every stage onto the last one.
fn load_destruction(subs: &[fnv_esm::Subrecord]) -> Vec<DestructionStage> {
    let mut stages: Vec<DestructionStage> = Vec::new();
    let mut cur: Option<DestructionStage> = None;
    for s in subs {
        match &s.sig {
            b"DSTD" => {
                // A new stage starts here. The one in progress, if any, is
                // finished and kept.
                if let Some(stage) = cur.take() {
                    stages.push(stage);
                }
                let mut stage = DestructionStage::default();
                if s.data.len() >= 4 {
                    stage.health_percent = s.data[0];
                    stage.index = s.data[1];
                    stage.damage_stage = s.data[2];
                    stage.flags = s.data[3];
                }
                if s.data.len() >= 8 {
                    stage.self_damage_per_second =
                        i32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                }
                if s.data.len() >= 12 {
                    stage.explosion = Some(u32::from_le_bytes([
                        s.data[8], s.data[9], s.data[10], s.data[11],
                    ]));
                }
                if s.data.len() >= 16 {
                    stage.debris = Some(u32::from_le_bytes([
                        s.data[12], s.data[13], s.data[14], s.data[15],
                    ]));
                }
                if s.data.len() >= 20 {
                    stage.debris_count =
                        i32::from_le_bytes([s.data[16], s.data[17], s.data[18], s.data[19]]);
                }
                cur = Some(stage);
            }
            b"DMDL" => {
                if let Some(stage) = cur.as_mut() {
                    stage.model = zstring(&s.data);
                }
            }
            b"DMDT" => {
                if let Some(stage) = cur.as_mut() {
                    stage.texture_hashes = s.data.clone();
                }
            }
            _ => {}
        }
    }
    if let Some(stage) = cur.take() {
        stages.push(stage);
    }
    stages
}

/// Write the shared Destruction Data collection back into a record.
///
/// The whole collection is written as one sequence, because the stages are
/// not a list of one signature: each is a run of four different subrecords
/// that must keep their order. A per-signature list edit would cluster every
/// `DSTD` together and every `DMDL` together, which the engine cannot read
/// back into stages.
fn save_destruction(
    session: &mut crate::session::Session,
    fid: u32,
    dest_health: i32,
    dest_flags: u8,
    stages: &[DestructionStage],
) {
    if dest_health <= 0 && stages.is_empty() {
        return;
    }
    let mut subs: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    // The DEST header is written over the record's own bytes, so the flag
    // byte — "VATS Targetable" and any bit the spec does not name — survives
    // a save.
    let original_dest = session.doc.record(fid).and_then(|r| {
        r.subrecords().ok().and_then(|subs| {
            subs.iter()
                .find(|s| s.sig == *b"DEST")
                .map(|s| s.data.clone())
        })
    });
    let mut dest = original_dest.unwrap_or_else(|| vec![0u8; 8]);
    dest.resize(8, 0);
    dest[0..4].copy_from_slice(&dest_health.to_le_bytes());
    // The count is the number of stages.
    dest[4] = stages.len().min(255) as u8;
    dest[5] = dest_flags;
    subs.push((*b"DEST", dest));

    for stage in stages {
        let mut dstd = vec![0u8; 20];
        dstd[0] = stage.health_percent;
        dstd[1] = stage.index;
        dstd[2] = stage.damage_stage;
        dstd[3] = stage.flags;
        dstd[4..8].copy_from_slice(&stage.self_damage_per_second.to_le_bytes());
        if let Some(e) = stage.explosion {
            dstd[8..12].copy_from_slice(&e.to_le_bytes());
        }
        if let Some(d) = stage.debris {
            dstd[12..16].copy_from_slice(&d.to_le_bytes());
        }
        dstd[16..20].copy_from_slice(&stage.debris_count.to_le_bytes());
        subs.push((*b"DSTD", dstd));
        if !stage.model.trim().is_empty() {
            subs.push((*b"DMDL", encode_zstring(&stage.model)));
        }
        if !stage.texture_hashes.is_empty() {
            subs.push((*b"DMDT", stage.texture_hashes.clone()));
        }
        subs.push((*b"DSTF", Vec::new()));
    }

    session.doc.set_sub_sequence(
        fid,
        vec![*b"DEST", *b"DSTD", *b"DMDL", *b"DMDT", *b"DSTF"],
        subs,
    );
}

/// One load screen location on an `LSCR` (Load Screen) record.
///
/// The record's `LNAM` subrecords name where the screen appears: a direct
/// cell or worldspace, or an indirect worldspace and a grid position. The
/// subrecord repeats, so the locations are a list.
#[derive(Debug, Clone, Default)]
pub struct LoadScreenLocation {
    /// The direct location — the FormID of a `CELL` or `WRLD` record.
    pub direct: Option<u32>,
    /// The indirect worldspace — the FormID of a `WRLD` record.
    pub indirect_world: Option<u32>,
    /// The indirect grid Y.
    pub indirect_grid_y: i16,
    /// The indirect grid X.
    pub indirect_grid_x: i16,
}

/// One condition, from the shared `CTDA` subrecord.
///
/// The subrecord is twenty-eight bytes: the type word, three unused bytes,
/// the comparison value, the function index, two four-byte parameters, the
/// run-on value and the reference.
///
/// The comparison value is a `GLOB` record's FormID *or* a float32,
///// depending on the function, so the raw four bytes are kept and the
/// interpretation is left to the reader.
#[derive(Debug, Clone, Default)]
pub struct Condition {
    /// `CTDA` Type — the flag word and the comparison: 0x0001 is Combine
    /// Next Condition Using OR, 0x0002 is Run On Target, 0x0004 is Use
    /// Global, and the comparison is the high bits: 0x0000 is Equal To,
    /// 0x2000 is Not Equal To, 0x4000 is Greater Than, 0x6000 is Greater
    /// Than or Equal To, 0x8000 is Less Than and 0xA000 is Less Than or
    /// Equal To.
    pub cond_type: u8,
    /// `CTDA` Comparison Value — the raw four bytes.
    pub comparison_value: u32,
    /// `CTDA` Function — the uint32 function index.
    pub function: u32,
    /// `CTDA` Parameter #1 — the raw four bytes.
    pub parameter1: u32,
    /// `CTDA` Parameter #2 — the raw four bytes.
    pub parameter2: u32,
    /// `CTDA` Run On — the uint32.
    pub run_on: u32,
    /// `CTDA` Reference — the FormID.
    pub reference: Option<u32>,
    /// The three unused bytes, carried through unchanged.
    pub unused: [u8; 3],
    /// The `CTDA` subrecord's own bytes.
    ///
    /// The spec's condition is twenty-eight bytes, but the game's `IDLE`
    /// and `PACK` records use a twenty-byte short form. The condition is
    /// written back over these bytes, so the length it had survives a save.
    pub raw: Vec<u8>,
}

/// Read the shared `CTDA` conditions out of a record's subrecords.
///
/// `CTDA` is a *repeated* subrecord, so the conditions are walked in file
/// order rather than read out of `raw_subs`, which holds one value per
/// signature and would collapse every condition onto the last one.
fn load_conditions(subs: &[fnv_esm::Subrecord]) -> Vec<Condition> {
    let mut conditions = Vec::new();
    for s in subs {
        if s.sig != *b"CTDA" || s.data.len() < 20 {
            continue;
        }
        conditions.push(read_condition(&s.data));
    }
    conditions
}

/// Read one `CTDA` condition out of a subrecord's payload.
///
/// The payload is twenty-eight bytes: the type and flag byte, three unused
/// bytes, the comparison value, the function, two parameters, the run-on
/// value and the reference.
fn read_condition(b: &[u8]) -> Condition {
    if b.len() < 20 {
        return Condition::default();
    }
    let u32_at = |at: usize| -> u32 {
        if b.len() >= at + 4 {
            u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
        } else {
            0
        }
    };
    Condition {
        cond_type: b[0],
        unused: [b[1], b[2], b[3]],
        comparison_value: u32_at(4),
        function: u32_at(8),
        parameter1: u32_at(12),
        parameter2: u32_at(16),
        run_on: u32_at(20),
        reference: if b.len() >= 28 {
            Some(u32_at(24))
        } else {
            None
        },
        raw: b.to_vec(),
    }
}

/// Write one `CTDA` condition out as a subrecord payload.
fn write_condition(c: &Condition) -> Vec<u8> {
    // The condition is written back over its own bytes, so a short form
    // stays short and the bytes the spec does not name survive.
    let mut b = c.raw.clone();
    if b.is_empty() {
        b.resize(28, 0);
    }
    let put = |b: &mut Vec<u8>, at: usize, v: u32| {
        if b.len() >= at + 4 {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    };
    b[0] = c.cond_type;
    if b.len() >= 4 {
        b[1..4].copy_from_slice(&c.unused);
    }
    put(&mut b, 4, c.comparison_value);
    put(&mut b, 8, c.function);
    put(&mut b, 12, c.parameter1);
    put(&mut b, 16, c.parameter2);
    put(&mut b, 20, c.run_on);
    put(&mut b, 24, c.reference.unwrap_or(0));
    b
}

/// Write the shared `CTDA` conditions back onto a record.
///
/// The conditions are written with a list edit: a per-subrecord edit would
/// collapse them onto the last one.
fn save_conditions(session: &mut crate::session::Session, fid: u32, conditions: &[Condition]) {
    if conditions.is_empty() {
        session.doc.remove_sub(fid, b"CTDA");
        return;
    }
    let payloads: Vec<Vec<u8>> = conditions.iter().map(write_condition).collect();
    session.doc.set_sub_list(fid, b"CTDA", payloads);
}

/// One entry in a leveled list, from the shared Leveled List Entry
/// collection.
///
/// The collection is a run of `LVLO` (the base data) and an optional `COED`
/// (the extra data). The `LVLO` subrecord repeats, so the entries are a
/// list.
#[derive(Debug, Clone, Default)]
pub struct LeveledListEntry {
    /// `LVLO` Level — the int16.
    pub level: i16,
    /// `LVLO` Reference — the FormID of the item.
    pub reference: Option<u32>,
    /// `LVLO` Count — the int16.
    pub count: i16,
    /// `LVLO`'s four unused bytes, carried through unchanged.
    pub unused: [u8; 4],
    /// `COED` Owner — the FormID of an `NPC_` or `FACT` record.
    pub owner: Option<u32>,
    /// `COED` Global Variable / Required Rank — the raw four bytes. The
    /// value is a `GLOB` record's FormID, an integer rank, or null.
    pub global_or_rank: u32,
    /// `COED` Item Condition — the float32.
    pub item_condition: f32,
    /// Whether the entry carries a `COED` at all.
    pub has_extra: bool,
}

/// One menu button on a `MESG` (Message) record.
///
/// The buttons are a run of `ITXT` (the text) and repeated `CTDA` (the
/// conditions), and both repeat.
#[derive(Debug, Clone, Default)]
pub struct MessageButton {
    /// `ITXT` Button Text — the cstring.
    pub text: String,
    /// The `ITXT` subrecord's own bytes, so a byte that is not valid UTF-8
    /// survives a save.
    pub raw: Vec<u8>,
    /// The `CTDA` conditions — the repeated condition list.
    pub conditions: Vec<Condition>,
}

/// One effect on a `SPEL` (Spell) record.
///
/// The effects are a run of `EFID` (the effect) and `EFIT` (the
/// parameters), and both repeat.
#[derive(Debug, Clone, Default)]
pub struct SpellEffect {
    /// `EFID` — the effect's FormID.
    pub effect: Option<u32>,
    /// `EFIT` — the effect's parameters, carried through unchanged: the
    /// layout depends on the effect's own type.
    pub parameters: Vec<u8>,
    /// The `CTDA` that follows the effect, if the record has one. The
    /// condition belongs to the effect above it, so the effect collection
    /// is walked in file order rather than collected by signature.
    pub conditions: Vec<Condition>,
}

/// Read the shared `DODT` decal data out of a subrecord's payload.
///
/// The payload is thirty-six bytes: the seven floats, the parallax passes,
/// the flag word, two unused bytes and the rgba colour.
fn read_decal_data(b: &[u8]) -> DecalData {
    let f = |i: usize| {
        let at = i * 4;
        f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    };
    DecalData {
        min_width: f(0),
        max_width: f(1),
        min_height: f(2),
        max_height: f(3),
        depth: f(4),
        shininess: f(5),
        parallax_scale: f(6),
        parallax_passes: b[28],
        flags: b[29],
        color: [b[32], b[33], b[34], b[35]],
    }
}

/// The `DODT` decal data, from the shared Decal Data collection.
#[derive(Debug, Clone, Default)]
pub struct DecalData {
    /// `DODT` Min Width — the float32.
    pub min_width: f32,
    /// `DODT` Max Width — the float32.
    pub max_width: f32,
    /// `DODT` Min Height — the float32.
    pub min_height: f32,
    /// `DODT` Max Height — the float32.
    pub max_height: f32,
    /// `DODT` Depth — the float32.
    pub depth: f32,
    /// `DODT` Shininess — the float32.
    pub shininess: f32,
    /// `DODT` Parallax Scale — the float32.
    pub parallax_scale: f32,
    /// `DODT` Parallax Passes — the u8.
    pub parallax_passes: u8,
    /// `DODT` Flags — the u8 flag word: Parallax, Alpha Blending, Alpha
    /// Testing.
    pub flags: u8,
    /// `DODT` Color — the rgba quadruple.
    pub color: [u8; 4],
}

/// One item in an actor's inventory.
///
/// The inventory is a run of `CNTO` (the item and the count) and an
/// optional `COED` (the extra data), and the extra data belongs to the item
/// before it.
#[derive(Debug, Clone, Default)]
pub struct ActorInventoryEntry {
    /// The item's FormID.
    pub item: u32,
    /// The count — the int32.
    pub count: i32,
    /// The `COED` extra data, if the item has one.
    pub extra: Option<Vec<u8>>,
}

/// The water's visual data — the `DNAM` struct of a `WATR` (Water) record.
///
/// The struct is 196 bytes: a sixteen-byte unknown header, then the
/// water, fog, rain, displacement and noise properties, then a three-byte
/// trailing pad the spec does not name.
#[derive(Debug, Clone, Default)]
pub struct WatrVisual {
    /// `Water Properties - Sun Power` — the float32.
    pub sun_power: f32,
    /// `Water Properties - Reflectivity Amount` — the float32.
    pub reflectivity_amount: f32,
    /// `Water Properties - Fresnel Amount` — the float32.
    pub fresnel_amount: f32,
    /// `Fog Properties - Above Water - Fog Near Plane Distance` — the float32.
    pub above_fog_near: f32,
    /// `Fog Properties - Above Water - Fog Far Plane Distance` — the float32.
    pub above_fog_far: f32,
    /// `Shallow Color` — the rgba.
    pub shallow_color: [u8; 4],
    /// `Deep Color` — the rgba.
    pub deep_color: [u8; 4],
    /// `Reflection Color` — the rgba.
    pub reflection_color: [u8; 4],
    /// `Rain Simulator - Force` — the float32.
    pub rain_force: f32,
    /// `Rain Simulator - Velocity` — the float32.
    pub rain_velocity: f32,
    /// `Rain Simulator - Falloff` — the float32.
    pub rain_falloff: f32,
    /// `Rain Simulator - Dampener` — the float32.
    pub rain_dampener: f32,
    /// `Displacement Simulator - Starting Size` — the float32.
    pub displacement_starting_size: f32,
    /// `Displacement Simulator - Force` — the float32.
    pub displacement_force: f32,
    /// `Displacement Simulator - Velocity` — the float32.
    pub displacement_velocity: f32,
    /// `Displacement Simulator - Falloff` — the float32.
    pub displacement_falloff: f32,
    /// `Displacement Simulator - Dampener` — the float32.
    pub displacement_dampener: f32,
    /// `Rain Simulator - Starting Size` — the float32.
    pub rain_starting_size: f32,
    /// `Noise Properties - Normals - Noise Scale` — the float32.
    pub noise_scale: f32,
    /// `Noise Properties - Noise Layer One - Wind Direction` — the float32.
    pub layer_one_wind_direction: f32,
    /// `Noise Properties - Noise Layer Two - Wind Direction` — the float32.
    pub layer_two_wind_direction: f32,
    /// `Noise Properties - Noise Layer Three - Wind Direction` — the float32.
    pub layer_three_wind_direction: f32,
    /// `Noise Properties - Noise Layer One - Wind Speed` — the float32.
    pub layer_one_wind_speed: f32,
    /// `Noise Properties - Noise Layer Two - Wind Speed` — the float32.
    pub layer_two_wind_speed: f32,
    /// `Noise Properties - Noise Layer Three - Wind Speed` — the float32.
    pub layer_three_wind_speed: f32,
    /// `Noise Properties - Normals - Depth Falloff Start` — the float32.
    pub depth_falloff_start: f32,
    /// `Noise Properties - Normals - Depth Falloff End` — the float32.
    pub depth_falloff_end: f32,
    /// `Fog Properties - Above Water - Fog Amount` — the float32.
    pub above_fog_amount: f32,
    /// `Noise Properties - Normals - UV Scale` — the float32.
    pub normals_uv_scale: f32,
    /// `Fog Properties - Under Water - Fog Amount` — the float32.
    pub under_fog_amount: f32,
    /// `Fog Properties - Under Water - Fog Near Plane Distance` — the float32.
    pub under_fog_near: f32,
    /// `Fog Properties - Under Water - Fog Far Plane Distance` — the float32.
    pub under_fog_far: f32,
    /// `Water Properties - Distortion Amount` — the float32.
    pub distortion_amount: f32,
    /// `Water Properties - Shininess` — the float32.
    pub shininess: f32,
    /// `Water Properties - Reflection HDR Multiplier` — the float32.
    pub reflection_hdr_multiplier: f32,
    /// `Water Properties - Light Radius` — the float32.
    pub light_radius: f32,
    /// `Water Properties - Light Brightness` — the float32.
    pub light_brightness: f32,
    /// `Noise Properties - Noise Layer One - UV Scale` — the float32.
    pub layer_one_uv_scale: f32,
    /// `Noise Properties - Noise Layer Two - UV Scale` — the float32.
    pub layer_two_uv_scale: f32,
    /// `Noise Properties - Noise Layer Three - UV Scale` — the float32.
    pub layer_three_uv_scale: f32,
    /// `Noise Properties - Noise Layer One - Amplitude Scale` — the float32.
    pub layer_one_amplitude_scale: f32,
    /// `Noise Properties - Noise Layer Two - Amplitude Scale` — the float32.
    pub layer_two_amplitude_scale: f32,
    /// `Noise Properties - Noise Layer Three - Amplitude Scale` — the float32.
    pub layer_three_amplitude_scale: f32,
}

/// A sound's attenuation data — the `SNDD` (or `SNDX`) struct of a
/// `SOUN` (Sound) record.
///
/// A record carries one or the other. The `SNDD` form is 36 bytes and
/// holds everything: the two distance bytes, the frequency adjustment,
/// an unused byte, the flag word, the static attenuation, the stop and
/// start times, the five attenuation curve points, the reverb control,
/// the priority, and the sound's position. The `SNDX` form is 12 bytes
/// and holds only the first ten of those; its curve points, reverb
/// control and priority live in `ANAM`, `GNAM` and `HNAM` instead.
#[derive(Debug, Clone, Default)]
pub struct SoundAttenuation {
    /// `Minimum Attenuation Distance` — the uint8, multiplied by five.
    pub min_distance: u8,
    /// `Maximum Attenuation Distancce` — the uint8, multiplied by a
    /// hundred.
    pub max_distance: u8,
    /// `Frequency Adjustment Percentage` — the int8.
    pub frequency_adjustment: i8,
    /// The flag word, kept whole so a bit the spec does not name
    /// survives.
    pub flags: u32,
    /// `Static Attenuation cdB` — the int16.
    pub static_attenuation: i16,
    /// `Stop Time` — the uint8.
    pub stop_time: u8,
    /// `Start Time` — the uint8.
    pub start_time: u8,
    /// `Attenuation Point 1` through `Attenuation Point 5` — the five
    /// int16s on the attenuation curve. The names of the points are
    /// `attenuation_point_1` through `attenuation_point_5`.
    pub attenuation_points: [i16; 5],
    /// `Reverb Attenuation Control` — the int16.
    pub reverb_attenuation: i16,
    /// `Priority` — the int32.
    pub priority: i32,
    /// `X` — the int32.
    pub x: i32,
    /// `Y` — the int32.
    pub y: i32,
}

/// Read a sound's attenuation struct out of a `SNDD` or `SNDX` payload.
fn read_sound_attenuation(b: &[u8]) -> SoundAttenuation {
    let u8_at = |at: usize| -> u8 { b.get(at).copied().unwrap_or(0) };
    let i8_at = |at: usize| -> i8 { b.get(at).copied().unwrap_or(0) as i8 };
    let u16_at = |at: usize| -> u16 {
        if b.len() >= at + 2 {
            u16::from_le_bytes([b[at], b[at + 1]])
        } else {
            0
        }
    };
    let i16_at = |at: usize| -> i16 { u16_at(at) as i16 };
    let u32_at = |at: usize| -> u32 {
        if b.len() >= at + 4 {
            u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
        } else {
            0
        }
    };
    let i32_at = |at: usize| -> i32 { u32_at(at) as i32 };
    SoundAttenuation {
        min_distance: u8_at(0),
        max_distance: u8_at(1),
        frequency_adjustment: i8_at(2),
        flags: u32_at(4),
        static_attenuation: i16_at(8),
        stop_time: u8_at(10),
        start_time: u8_at(11),
        attenuation_points: [i16_at(13), i16_at(15), i16_at(17), i16_at(19), i16_at(21)],
        reverb_attenuation: i16_at(23),
        priority: i32_at(25),
        x: i32_at(29),
        y: i32_at(33),
    }
}

/// Write a sound's attenuation struct back over the record's own bytes.
///
/// The struct is written field by field, and a field the record's own
/// bytes do not reach is left alone — the `SNDX` variant is twelve bytes
/// long and stops after the start time.
fn write_sound_attenuation(original: &[u8], a: &SoundAttenuation) -> Vec<u8> {
    let mut b = original.to_vec();
    /// Write a field's bytes at an offset, if the record's own bytes reach
    /// that far.
    fn put(b: &mut [u8], at: usize, size: usize, bytes: &[u8]) {
        if b.len() >= at + size {
            b[at..at + size].copy_from_slice(bytes);
        }
    }
    put(&mut b, 0, 1, &a.min_distance.to_le_bytes());
    put(&mut b, 1, 1, &a.max_distance.to_le_bytes());
    put(&mut b, 2, 1, &a.frequency_adjustment.to_le_bytes());
    put(&mut b, 4, 4, &a.flags.to_le_bytes());
    put(&mut b, 8, 2, &a.static_attenuation.to_le_bytes());
    put(&mut b, 10, 1, &a.stop_time.to_le_bytes());
    put(&mut b, 11, 1, &a.start_time.to_le_bytes());
    for (i, point) in a.attenuation_points.iter().enumerate() {
        let at = 13 + i * 2;
        put(&mut b, at, 2, &point.to_le_bytes());
    }
    put(&mut b, 23, 2, &a.reverb_attenuation.to_le_bytes());
    put(&mut b, 25, 4, &a.priority.to_le_bytes());
    put(&mut b, 29, 4, &a.x.to_le_bytes());
    put(&mut b, 33, 4, &a.y.to_le_bytes());
    b
}

/// A weather record's colours for one time of day.
///
/// The weather's colours are laid out as ten colour types, each with one
/// colour per time of day: sunrise, day, sunset, night, high noon and
/// midnight.
#[derive(Debug, Clone, Default)]
pub struct TimeOfDayColors {
    /// `Sunrise` — the rgba.
    pub sunrise: [u8; 4],
    /// `Day` — the rgba.
    pub day: [u8; 4],
    /// `Sunset` — the rgba.
    pub sunset: [u8; 4],
    /// `Night` — the rgba.
    pub night: [u8; 4],
    /// `High Noon` — the rgba.
    pub high_noon: [u8; 4],
    /// `Midnight` — the rgba.
    pub midnight: [u8; 4],
}

/// The names of the weather's ten colour types, in the order the `NAM0`
/// subrecord holds them.
const WTHR_COLOR_TYPES: [&str; 10] = [
    "Sky-Upper",
    "Fog",
    "Unused",
    "Ambient",
    "Sunlight",
    "Sun",
    "Stars",
    "Sky-Lower",
    "Horizon",
    "Effect Lighting",
];

/// The names of the times of day, in the order each colour type holds
/// them.
const WTHR_TIMES_OF_DAY: [&str; 6] = ["Sunrise", "Day", "Sunset", "Night", "High Noon", "Midnight"];

/// A weather record's sound — a `SNAM` subrecord.
#[derive(Debug, Clone, Default)]
pub struct WeatherSound {
    /// The sound's FormID.
    pub sound: u32,
    /// The type — the uint32: Default, Precipitation, Wind or Thunder.
    pub kind: u32,
}

/// One ingredient or output on a `RCPE` (Recipe) record.
///
/// The collection is a run of `RCIL` (the item) and `RCQY` (the quantity),
/// and both repeat.
#[derive(Debug, Clone, Default)]
pub struct RecipeItem {
    /// `RCIL` Item — the FormID.
    pub item: Option<u32>,
    /// `RCQY` Quantity — the uint32.
    pub quantity: u32,
}

/// Read a `PLDT` or `PLD2` location subrecord.
///
/// The subrecord is a uint32 type, the location and an int32 radius. What
/// the location means depends on the type, so the bytes are kept whole and
/// the type is kept beside them.
fn read_package_location(data: &[u8]) -> PackageLocation {
    let mut loc = PackageLocation::default();
    if data.len() >= 4 {
        loc.kind = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    }
    if data.len() >= 8 {
        loc.target = data[4..8].to_vec();
    }
    if data.len() >= 12 {
        loc.radius = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    }
    loc
}

/// Read a `PTDT` or `PTD2` target subrecord.
///
/// The subrecord is an int32 type, the target, an int32 count or distance
/// and an unnamed float32.
fn read_package_target(data: &[u8]) -> PackageTarget {
    let mut t = PackageTarget::default();
    if data.len() >= 4 {
        t.kind = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    }
    if data.len() >= 8 {
        t.target = data[4..8].to_vec();
    }
    if data.len() >= 12 {
        t.count_distance = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    }
    if data.len() >= 16 {
        t.unknown = f32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    }
    t
}

/// Read the `PKW3` Use Weapon Data subrecord.
///
/// The subrecord is twenty-four bytes: the flag word, two uint8s, three
/// uint16s, two float32s and four unnamed bytes.
fn read_package_weapon(data: &[u8]) -> PackageWeapon {
    let mut w = PackageWeapon::default();
    if data.len() >= 4 {
        w.flags = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    }
    if data.len() >= 5 {
        w.fire_rate = data[4];
    }
    if data.len() >= 6 {
        w.fire_count = data[5];
    }
    if data.len() >= 8 {
        w.num_bursts = u16::from_le_bytes([data[6], data[7]]);
    }
    if data.len() >= 10 {
        w.shots_min = u16::from_le_bytes([data[8], data[9]]);
    }
    if data.len() >= 12 {
        w.shots_max = u16::from_le_bytes([data[10], data[11]]);
    }
    if data.len() >= 16 {
        w.pause_min = f32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    }
    if data.len() >= 20 {
        w.pause_max = f32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    }
    if data.len() >= 24 {
        w.unused = [data[20], data[21], data[22], data[23]];
    }
    w
}

/// Read the `PKDD` Dialog Data subrecord.
///
/// The subrecord is twenty-four bytes: the field of view, the topic, the
/// flag word, four unnamed bytes and the dialog type.
fn read_package_dialog(data: &[u8]) -> PackageDialog {
    let mut d = PackageDialog::default();
    if data.len() >= 4 {
        d.fov = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    }
    if data.len() >= 8 {
        d.topic = Some(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));
    }
    if data.len() >= 12 {
        d.flags = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    }
    if data.len() >= 16 {
        d.unused = [data[12], data[13], data[14], data[15]];
    }
    if data.len() >= 20 {
        d.dialog_type = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    }
    if data.len() >= 24 {
        d.unused2 = [data[20], data[21], data[22], data[23]];
    }
    d
}

/// Read the `RDOT` object placements of a `REGN` (Region) record.
///
/// Each placement is fifty-two bytes: the object's FormID, the parent index,
/// two unnamed bytes, the density, the clustering and the two slopes, the
/// flag word, the two radii, four unnamed bytes, the max height, the sink
/// and the two variances, the three angle variances and six unnamed bytes.
fn read_region_objects(data: &[u8]) -> Vec<RegionObject> {
    const ENTRY: usize = 52;
    let mut out = Vec::with_capacity(data.len() / ENTRY);
    for chunk in data.as_chunks::<ENTRY>().0 {
        out.push(RegionObject {
            object: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            parent_index: u16::from_le_bytes([chunk[4], chunk[5]]),
            unused: [chunk[6], chunk[7]],
            density: f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
            clustering: chunk[12],
            min_slope: chunk[13],
            max_slope: chunk[14],
            flags: chunk[15],
            parent_radius: u16::from_le_bytes([chunk[16], chunk[17]]),
            radius: u16::from_le_bytes([chunk[18], chunk[19]]),
            unknown: [chunk[20], chunk[21], chunk[22], chunk[23]],
            max_height: f32::from_le_bytes([chunk[24], chunk[25], chunk[26], chunk[27]]),
            sink: f32::from_le_bytes([chunk[28], chunk[29], chunk[30], chunk[31]]),
            sink_variance: f32::from_le_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]),
            size_variance: f32::from_le_bytes([chunk[36], chunk[37], chunk[38], chunk[39]]),
            x_angle_variance: u16::from_le_bytes([chunk[40], chunk[41]]),
            y_angle_variance: u16::from_le_bytes([chunk[42], chunk[43]]),
            z_angle_variance: u16::from_le_bytes([chunk[44], chunk[45]]),
            unknown2: [
                chunk[46], chunk[47], chunk[48], chunk[49], chunk[50], chunk[51],
            ],
        });
    }
    out
}

/// Read the `RDGS` grass entries of a `REGN` (Region) record.
///
/// Each entry is eight bytes: the grass's FormID and four unnamed bytes.
fn read_region_grasses(data: &[u8]) -> Vec<RegionGrass> {
    let mut out = Vec::with_capacity(data.len() / 8);
    for chunk in data.as_chunks::<8>().0 {
        out.push(RegionGrass {
            grass: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            unknown: [chunk[4], chunk[5], chunk[6], chunk[7]],
        });
    }
    out
}

/// Read the `RDSD` sound entries of a `REGN` (Region) record.
///
/// Each entry is twelve bytes: the sound's FormID, the flag word and the
/// chance.
fn read_region_sounds(data: &[u8]) -> Vec<RegionSound> {
    let mut out = Vec::with_capacity(data.len() / 12);
    for chunk in data.as_chunks::<12>().0 {
        out.push(RegionSound {
            sound: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            flags: u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
            chance: u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
        });
    }
    out
}

/// Read the `RDWT` weather entries of a `REGN` (Region) record.
///
/// Each entry is twelve bytes: the weather's FormID, the chance and the
/// global's FormID.
fn read_region_weather(data: &[u8]) -> Vec<RegionWeather> {
    let mut out = Vec::with_capacity(data.len() / 12);
    for chunk in data.as_chunks::<12>().0 {
        out.push(RegionWeather {
            weather: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            chance: u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
            global: Some(u32::from_le_bytes([
                chunk[8], chunk[9], chunk[10], chunk[11],
            ])),
        });
    }
    out
}

/// Read the `DNAM` data header of an `IMAD` (Image Space Adapter) record.
///
/// The struct is a fixed run of fields: the flag word, the duration, the
/// HDR block, seventy-two unnamed bytes, the cinematic block, eight unnamed
/// bytes and the rest. The record's own length decides how much is present,
/// so a short record simply leaves the later fields at zero.
fn read_imad_header(data: &[u8]) -> ImadHeader {
    let mut h = ImadHeader::default();
    let u32_at = |off: usize| -> u32 {
        if data.len() >= off + 4 {
            u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
        } else {
            0
        }
    };
    h.flags = u32_at(0);
    if data.len() >= 8 {
        h.duration = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    }
    h.hdr_eye_adapt_speed_mult = u32_at(8);
    h.hdr_eye_adapt_speed_add = u32_at(12);
    h.hdr_bloom_blur_radius_mult = u32_at(16);
    h.hdr_bloom_blur_radius_add = u32_at(20);
    h.hdr_bloom_threshold_mult = u32_at(24);
    h.hdr_bloom_threshold_add = u32_at(28);
    h.hdr_bloom_scale_mult = u32_at(32);
    h.hdr_bloom_scale_add = u32_at(36);
    h.hdr_target_lum_min_mult = u32_at(40);
    h.hdr_target_lum_min_add = u32_at(44);
    h.hdr_target_lum_max_mult = u32_at(48);
    h.hdr_target_lum_max_add = u32_at(52);
    h.hdr_sunlight_scale_mult = u32_at(56);
    h.hdr_sunlight_scale_add = u32_at(60);
    h.hdr_sky_scale_mult = u32_at(64);
    h.hdr_sky_scale_add = u32_at(68);
    if data.len() >= 144 {
        h.unknown.copy_from_slice(&data[72..144]);
    }
    h.cinematic_saturation_mult = u32_at(144);
    h.cinematic_saturation_add = u32_at(148);
    h.cinematic_brightness_mult = u32_at(152);
    h.cinematic_brightness_add = u32_at(156);
    h.cinematic_contrast_mult = u32_at(160);
    h.cinematic_contrast_add = u32_at(164);
    if data.len() >= 176 {
        h.unknown2.copy_from_slice(&data[168..176]);
    }
    h.tint_color = u32_at(176);
    h.blur_radius = u32_at(180);
    h.double_vision_strength = u32_at(184);
    h.radial_blur_strength = u32_at(188);
    h.radial_blur_ramp_up = u32_at(192);
    h.radial_blur_start = u32_at(196);
    h.radial_blur_flags = u32_at(200);
    h.radial_blur_center_x = u32_at(204);
    h.radial_blur_center_y = u32_at(208);
    h.dof_strength = u32_at(212);
    h.dof_distance = u32_at(216);
    h.dof_range = u32_at(220);
    h.dof_flags = u32_at(224);
    h.radial_blur_ramp_down = u32_at(228);
    h.radial_blur_down_start = u32_at(232);
    h.fade_color = u32_at(236);
    h.motion_blur_strength = u32_at(240);
    h
}

/// Write the `DNAM` data header of an `IMAD` record back over the record's
/// own bytes.
///
/// The struct's length varies — the game's records are 236, 240 and 244
/// bytes long — so the record's own length is kept and only the fields
/// that fit are written.
fn write_imad_header(original: &[u8], h: &ImadHeader) -> Vec<u8> {
    let mut data = original.to_vec();
    let len = data.len();
    let put_u32 = |data: &mut Vec<u8>, off: usize, v: u32| {
        if data.len() >= off + 4 {
            data[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
    };
    put_u32(&mut data, 0, h.flags);
    if len >= 8 {
        data[4..8].copy_from_slice(&h.duration.to_le_bytes());
    }
    put_u32(&mut data, 8, h.hdr_eye_adapt_speed_mult);
    put_u32(&mut data, 12, h.hdr_eye_adapt_speed_add);
    put_u32(&mut data, 16, h.hdr_bloom_blur_radius_mult);
    put_u32(&mut data, 20, h.hdr_bloom_blur_radius_add);
    put_u32(&mut data, 24, h.hdr_bloom_threshold_mult);
    put_u32(&mut data, 28, h.hdr_bloom_threshold_add);
    put_u32(&mut data, 32, h.hdr_bloom_scale_mult);
    put_u32(&mut data, 36, h.hdr_bloom_scale_add);
    put_u32(&mut data, 40, h.hdr_target_lum_min_mult);
    put_u32(&mut data, 44, h.hdr_target_lum_min_add);
    put_u32(&mut data, 48, h.hdr_target_lum_max_mult);
    put_u32(&mut data, 52, h.hdr_target_lum_max_add);
    put_u32(&mut data, 56, h.hdr_sunlight_scale_mult);
    put_u32(&mut data, 60, h.hdr_sunlight_scale_add);
    put_u32(&mut data, 64, h.hdr_sky_scale_mult);
    put_u32(&mut data, 68, h.hdr_sky_scale_add);
    if len >= 144 {
        data[72..144].copy_from_slice(&h.unknown);
    }
    put_u32(&mut data, 144, h.cinematic_saturation_mult);
    put_u32(&mut data, 148, h.cinematic_saturation_add);
    put_u32(&mut data, 152, h.cinematic_brightness_mult);
    put_u32(&mut data, 156, h.cinematic_brightness_add);
    put_u32(&mut data, 160, h.cinematic_contrast_mult);
    put_u32(&mut data, 164, h.cinematic_contrast_add);
    if len >= 176 {
        data[168..176].copy_from_slice(&h.unknown2);
    }
    put_u32(&mut data, 176, h.tint_color);
    put_u32(&mut data, 180, h.blur_radius);
    put_u32(&mut data, 184, h.double_vision_strength);
    put_u32(&mut data, 188, h.radial_blur_strength);
    put_u32(&mut data, 192, h.radial_blur_ramp_up);
    put_u32(&mut data, 196, h.radial_blur_start);
    put_u32(&mut data, 200, h.radial_blur_flags);
    put_u32(&mut data, 204, h.radial_blur_center_x);
    put_u32(&mut data, 208, h.radial_blur_center_y);
    put_u32(&mut data, 212, h.dof_strength);
    put_u32(&mut data, 216, h.dof_distance);
    put_u32(&mut data, 220, h.dof_range);
    put_u32(&mut data, 224, h.dof_flags);
    put_u32(&mut data, 228, h.radial_blur_ramp_down);
    put_u32(&mut data, 232, h.radial_blur_down_start);
    put_u32(&mut data, 236, h.fade_color);
    put_u32(&mut data, 240, h.motion_blur_strength);
    data
}

/// Read the keys of an `IMAD` (Image Space Adapter) curve.
///
/// A Time structure is two float32s; a Color structure is the time and four
/// channels. A trailing remainder too short for a whole key is kept, so the
/// curve can be written back byte for byte.
fn read_imad_keys(data: &[u8], is_color: bool) -> Vec<ImadKey> {
    let mut keys = Vec::new();
    if is_color {
        for chunk in data.as_chunks::<20>().0 {
            keys.push(ImadKey {
                time: f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                value: f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
                green: f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
                blue: f32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]),
                alpha: f32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]),
            });
        }
    } else {
        for chunk in data.as_chunks::<8>().0 {
            keys.push(ImadKey {
                time: f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                value: f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
                ..ImadKey::default()
            });
        }
    }
    keys
}

/// The curves an `IMAD` (Image Space Adapter) record carries.
///
/// The record's subrecords are one per curve. The signatures that start with
/// a control character are spelled with their hex value, as the spec does.
const IMAD_CURVES: &[([u8; 4], &str, bool)] = &[
    (*b"BNAM", "Blur Radius", false),
    (*b"VNAM", "Double Vision Strength", false),
    (*b"TNAM", "Tint Color", true),
    (*b"NAM3", "Fade Color", true),
    (*b"RNAM", "Radial Blur Strength", false),
    (*b"SNAM", "Radial Blur Ramp Up", false),
    (*b"UNAM", "Radial Blur Start", false),
    (*b"NAM1", "Radial Blur Ramp Down", false),
    (*b"NAM2", "Radial Blur Down Start", false),
    (*b"WNAM", "DoF Strength", false),
    (*b"XNAM", "DoF Distance", false),
    (*b"YNAM", "DoF Range", false),
    (*b"NAM4", "Motion Blur Strength", false),
    (*b"\x00IAD", "HDR Eye Adapt Speed Mult", false),
    (*b"@IAD", "HDR Eye Adapt Speed Add", false),
    (*b"\x01IAD", "HDR Bloom Blur Radius Mult", false),
    (*b"AIAD", "HDR Bloom Blur Radius Add", false),
    (*b"\x02IAD", "HDR Bloom Threshold Mult", false),
    (*b"BIAD", "HDR Bloom Threshold Add", false),
    (*b"\x03IAD", "HDR Bloom Scale Mult", false),
    (*b"CIAD", "HDR Bloom Scale Add", false),
    (*b"\x04IAD", "HDR Target Lum Min Mult", false),
    (*b"DIAD", "HDR Target Lum Min Add", false),
    (*b"\x05IAD", "HDR Target Lum Max Mult", false),
    (*b"EIAD", "HDR Target Lum Max Add", false),
    (*b"\x06IAD", "HDR Sunlight Scale Mult", false),
    (*b"FIAD", "HDR Sunlight Scale Add", false),
    (*b"\x07IAD", "HDR Sky Scale Mult", false),
    (*b"GIAD", "HDR Sky Scale Add", false),
    (*b"\x08IAD", "Unknown 08", false),
    (*b"HIAD", "Unknown 09", false),
    (*b"\x09IAD", "Unknown 09", false),
    (*b"IIAD", "Unknown 0A", false),
    (*b"\x0AIAD", "Unknown 0A", false),
    (*b"JIAD", "Unknown 0B", false),
    (*b"\x0BIAD", "Unknown 0B", false),
    (*b"KIAD", "Unknown 0C", false),
    (*b"\x0CIAD", "Unknown 0C", false),
    (*b"LIAD", "Unknown 0D", false),
    (*b"\x0DIAD", "Unknown 0D", false),
    (*b"MIAD", "Unknown 0E", false),
    (*b"\x0EIAD", "Unknown 0E", false),
    (*b"NIAD", "Unknown 0F", false),
    (*b"\x0FIAD", "Unknown 0F", false),
    (*b"OIAD", "Unknown 10", false),
    (*b"\x10IAD", "Unknown 10", false),
    (*b"PIAD", "Cinematic Saturation Mult", false),
    (*b"\x11IAD", "Cinematic Saturation Mult", false),
    (*b"QIAD", "Cinematic Saturation Add", false),
    (*b"\x12IAD", "Cinematic Brightness Mult", false),
    (*b"RIAD", "Cinematic Brightness Add", false),
    (*b"\x13IAD", "Cinematic Contrast Mult", false),
    (*b"SIAD", "Cinematic Contrast Add", false),
    (*b"\x14IAD", "Unknown 14", false),
    (*b"TIAD", "Unknown 14", false),
];

/// The name and colour flag of a curve signature, if it is a curve.
fn imad_curve_for(sig: &[u8; 4]) -> Option<(&'static str, bool)> {
    IMAD_CURVES
        .iter()
        .find(|(s, _, _)| s == sig)
        .map(|(_, name, is_color)| (*name, *is_color))
}

/// One curve on an `IMAD` (Image Space Adapter) record.
///
/// The record carries one subrecord per curve. Every curve is a list of
/// keys, and the keys are either a Time structure (the time and the value)
/// or a Color structure (the time and four channels).
#[derive(Debug, Clone, Default)]
pub struct ImadCurve {
    /// The subrecord's signature.
    pub sig: [u8; 4],
    /// The curve's name, from the spec.
    pub name: &'static str,
    /// Whether the curve's keys carry a colour rather than a value.
    pub is_color: bool,
    /// The keys.
    pub keys: Vec<ImadKey>,
}

/// The `DNAM` data header of an `IMAD` (Image Space Adapter) record.
///
/// The struct is a fixed sequence of named fields followed by a block of
/// unnamed bytes, and the record's own length decides how much of it is
/// present: the game's records are 236, 240 and 244 bytes long. The named
/// fields are therefore written over the record's own bytes, so the length
/// and every unnamed bit survive a save.
#[derive(Debug, Clone)]
pub struct ImadHeader {
    /// The uint32 flag word: Animatable.
    pub flags: u32,
    /// The float32 duration.
    pub duration: f32,
    /// The uint32.
    pub hdr_eye_adapt_speed_mult: u32,
    /// The uint32.
    pub hdr_eye_adapt_speed_add: u32,
    /// The uint32.
    pub hdr_bloom_blur_radius_mult: u32,
    /// The uint32.
    pub hdr_bloom_blur_radius_add: u32,
    /// The uint32.
    pub hdr_bloom_threshold_mult: u32,
    /// The uint32.
    pub hdr_bloom_threshold_add: u32,
    /// The uint32.
    pub hdr_bloom_scale_mult: u32,
    /// The uint32.
    pub hdr_bloom_scale_add: u32,
    /// The uint32.
    pub hdr_target_lum_min_mult: u32,
    /// The uint32.
    pub hdr_target_lum_min_add: u32,
    /// The uint32.
    pub hdr_target_lum_max_mult: u32,
    /// The uint32.
    pub hdr_target_lum_max_add: u32,
    /// The uint32.
    pub hdr_sunlight_scale_mult: u32,
    /// The uint32.
    pub hdr_sunlight_scale_add: u32,
    /// The uint32.
    pub hdr_sky_scale_mult: u32,
    /// The uint32.
    pub hdr_sky_scale_add: u32,
    /// The seventy-two unnamed bytes.
    pub unknown: [u8; 72],
    /// The uint32.
    pub cinematic_saturation_mult: u32,
    /// The uint32.
    pub cinematic_saturation_add: u32,
    /// The uint32.
    pub cinematic_brightness_mult: u32,
    /// The uint32.
    pub cinematic_brightness_add: u32,
    /// The uint32.
    pub cinematic_contrast_mult: u32,
    /// The uint32.
    pub cinematic_contrast_add: u32,
    /// The eight unnamed bytes.
    pub unknown2: [u8; 8],
    /// The uint32.
    pub tint_color: u32,
    /// The uint32.
    pub blur_radius: u32,
    /// The uint32.
    pub double_vision_strength: u32,
    /// The uint32.
    pub radial_blur_strength: u32,
    /// The uint32.
    pub radial_blur_ramp_up: u32,
    /// The uint32.
    pub radial_blur_start: u32,
    /// The uint32 flag word: Use Target.
    pub radial_blur_flags: u32,
    /// The uint32.
    pub radial_blur_center_x: u32,
    /// The uint32.
    pub radial_blur_center_y: u32,
    /// The uint32.
    pub dof_strength: u32,
    /// The uint32.
    pub dof_distance: u32,
    /// The uint32.
    pub dof_range: u32,
    /// The uint32 flag word: Use Target.
    pub dof_flags: u32,
    /// The uint32.
    pub radial_blur_ramp_down: u32,
    /// The uint32.
    pub radial_blur_down_start: u32,
    /// The uint32.
    pub fade_color: u32,
    /// The uint32.
    pub motion_blur_strength: u32,
}

impl Default for ImadHeader {
    fn default() -> Self {
        ImadHeader {
            flags: 0,
            duration: 0.0,
            hdr_eye_adapt_speed_mult: 0,
            hdr_eye_adapt_speed_add: 0,
            hdr_bloom_blur_radius_mult: 0,
            hdr_bloom_blur_radius_add: 0,
            hdr_bloom_threshold_mult: 0,
            hdr_bloom_threshold_add: 0,
            hdr_bloom_scale_mult: 0,
            hdr_bloom_scale_add: 0,
            hdr_target_lum_min_mult: 0,
            hdr_target_lum_min_add: 0,
            hdr_target_lum_max_mult: 0,
            hdr_target_lum_max_add: 0,
            hdr_sunlight_scale_mult: 0,
            hdr_sunlight_scale_add: 0,
            hdr_sky_scale_mult: 0,
            hdr_sky_scale_add: 0,
            unknown: [0; 72],
            cinematic_saturation_mult: 0,
            cinematic_saturation_add: 0,
            cinematic_brightness_mult: 0,
            cinematic_brightness_add: 0,
            cinematic_contrast_mult: 0,
            cinematic_contrast_add: 0,
            unknown2: [0; 8],
            tint_color: 0,
            blur_radius: 0,
            double_vision_strength: 0,
            radial_blur_strength: 0,
            radial_blur_ramp_up: 0,
            radial_blur_start: 0,
            radial_blur_flags: 0,
            radial_blur_center_x: 0,
            radial_blur_center_y: 0,
            dof_strength: 0,
            dof_distance: 0,
            dof_range: 0,
            dof_flags: 0,
            radial_blur_ramp_down: 0,
            radial_blur_down_start: 0,
            fade_color: 0,
            motion_blur_strength: 0,
        }
    }
}

/// One key on an `IMAD` (Image Space Adapter) curve.
///
/// A Time structure is two float32s: the time and the value. A Color
/// structure is the time and four colour channels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImadKey {
    /// The time — the float32.
    pub time: f32,
    /// The value — the float32. For a colour key this is the red channel.
    pub value: f32,
    /// The green channel — the float32.
    pub green: f32,
    /// The blue channel — the float32.
    pub blue: f32,
    /// The alpha channel — the float32.
    pub alpha: f32,
}

/// The field layout of an `EFSH` (Effect Shader) record's `DATA` struct.
///
/// Each entry is the field's offset, its kind and the spec's name. The
/// struct's length varies, so the fields past the end of a short record are
/// simply not present.
const EFSH_FIELDS: &[(usize, &str, EfshKind)] = &[
    (4, "Membrane Source Blend Mode", EfshKind::U32),
    (8, "Membrane Blend Operation", EfshKind::U32),
    (12, "Membrane Z Test Function", EfshKind::U32),
    (16, "Fill Color", EfshKind::Rgba),
    (20, "Fill Alpha Fade In Time", EfshKind::F32),
    (24, "Fill Full Alpha Time", EfshKind::F32),
    (28, "Fill Alpha Fade Out Time", EfshKind::F32),
    (32, "Fill Persistent Alpha Ratio", EfshKind::F32),
    (36, "Fill Alpha Pulse Amplitude", EfshKind::F32),
    (40, "Fill Alpha Pulse Frequency", EfshKind::F32),
    (44, "Texture Animation Speed U", EfshKind::F32),
    (48, "Texture Animation Speed V", EfshKind::F32),
    (52, "Edge Fall Off", EfshKind::F32),
    (56, "Edge Color", EfshKind::Rgba),
    (60, "Edge Alpha Fade In Time", EfshKind::F32),
    (64, "Edge Full Alpha Time", EfshKind::F32),
    (68, "Edge Alpha Fade Out Time", EfshKind::F32),
    (72, "Edge Persistent Alpha Ratio", EfshKind::F32),
    (76, "Edge Alpha Pulse Amplitude", EfshKind::F32),
    (80, "Edge Alpha Pulse Frequency", EfshKind::F32),
    (84, "Edge Full Alpha Ratio", EfshKind::F32),
    (88, "Edge Full Alpha Ratio", EfshKind::F32),
    (92, "Membrane Dest Blend Mode", EfshKind::U32),
    (96, "Particle Source Blend Mode", EfshKind::U32),
    (100, "Particle Blend Operation", EfshKind::U32),
    (104, "Particle Z Test Function", EfshKind::U32),
    (108, "Particle Dest Blend Mode", EfshKind::U32),
    (112, "Particle Birth Ramp Up Time", EfshKind::F32),
    (116, "Full Particle Birth Time", EfshKind::F32),
    (120, "Particle Birth Ramp Down Time", EfshKind::F32),
    (124, "Full Particle Birth Ratio", EfshKind::F32),
    (128, "Persistent Particle Birth Ratio", EfshKind::F32),
    (132, "Particle Lifetime", EfshKind::F32),
    (136, "Particle Lifetime +/-", EfshKind::F32),
    (140, "Initial Speed Along Normal", EfshKind::F32),
    (144, "Acceleration Along Normal", EfshKind::F32),
    (148, "Initial Velocity 1", EfshKind::F32),
    (152, "Initial Velocity 2", EfshKind::F32),
    (156, "Initial Velocity 3", EfshKind::F32),
    (160, "Acceleration 1", EfshKind::F32),
    (164, "Acceleration 2", EfshKind::F32),
    (168, "Acceleration 3", EfshKind::F32),
    (172, "Scale Key 1", EfshKind::F32),
    (176, "Scale Key 2", EfshKind::F32),
    (180, "Scale Key 1 Time", EfshKind::F32),
    (184, "Scale Key 2 Time", EfshKind::F32),
    (188, "Color Key 1", EfshKind::Rgba),
    (192, "Color Key 2", EfshKind::Rgba),
    (196, "Color Key 3", EfshKind::Rgba),
    (200, "Color Key 1 Alpha", EfshKind::F32),
    (204, "Color Key 2 Alpha", EfshKind::F32),
    (208, "Color Key 3 Alpha", EfshKind::F32),
    (212, "Color Key 1 Time", EfshKind::F32),
    (216, "Color Key 2 Time", EfshKind::F32),
    (220, "Color Key 3 Time", EfshKind::F32),
    (224, "Initial Speed Along Normal +/-", EfshKind::F32),
    (228, "Initial Rotation", EfshKind::F32),
    (232, "Initial Rotation +/-", EfshKind::F32),
    (236, "Rotation Speed", EfshKind::F32),
    (240, "Rotation Speed +/-", EfshKind::F32),
    (244, "Addon Models", EfshKind::Fid),
    (248, "Holes Start Time", EfshKind::F32),
    (252, "Holes End Time", EfshKind::F32),
    (256, "Holes Start Value", EfshKind::F32),
    (260, "Holes End Value", EfshKind::F32),
    (264, "Edge Width", EfshKind::F32),
    (268, "Edge Color", EfshKind::Rgba),
    (272, "Explosion Wind Speed", EfshKind::F32),
    (276, "Texture Count U", EfshKind::U32),
    (280, "Texture Count V", EfshKind::U32),
    (284, "Addon Models Fade In Time", EfshKind::F32),
    (288, "Addon Models Fade Out Time", EfshKind::F32),
    (292, "Addon Models Scale Start", EfshKind::F32),
    (296, "Addon Models Scale End", EfshKind::F32),
    (300, "Addon Models Scale In Time", EfshKind::F32),
    (304, "Addon Models Scale Out Time", EfshKind::F32),
];

/// The kind of an `EFSH` struct field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EfshKind {
    /// A float32.
    F32,
    /// A uint32.
    U32,
    /// A FormID.
    Fid,
    /// A four-byte colour.
    Rgba,
}

/// Read an `EFSH` (Effect Shader) record's `DATA` struct.
///
/// The struct is a fixed run of fields, and the record's own length decides
/// which of them are present: the game's records are 224, 248, 300 and 308
/// bytes long.
fn read_efsh_data(data: &[u8]) -> EfshData {
    let mut out = EfshData::default();
    if data.is_empty() {
        return out;
    }
    out.flags = data[0];
    for (offset, name, kind) in EFSH_FIELDS {
        let size = match kind {
            EfshKind::Rgba => 4,
            _ => 4,
        };
        if data.len() < offset + size {
            continue;
        }
        let b = &data[*offset..*offset + size];
        let value = match kind {
            EfshKind::F32 => EfshValue::F32(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            EfshKind::U32 => EfshValue::U32(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            EfshKind::Fid => EfshValue::Fid(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            EfshKind::Rgba => EfshValue::Rgba([b[0], b[1], b[2], b[3]]),
        };
        out.fields.push(EfshField {
            offset: *offset,
            name,
            kind: *kind,
            value,
        });
    }
    out
}

/// Write an `EFSH` record's `DATA` struct back over the record's own bytes.
///
/// Only the fields that fit in the record's own length are written, so a
/// short record does not grow and the bytes the spec does not name survive.
fn write_efsh_data(original: &[u8], d: &EfshData) -> Vec<u8> {
    let mut data = original.to_vec();
    if data.is_empty() {
        return data;
    }
    data[0] = d.flags;
    for field in &d.fields {
        let size = 4;
        if data.len() < field.offset + size {
            continue;
        }
        let bytes = match &field.value {
            EfshValue::F32(v) => v.to_le_bytes(),
            EfshValue::U32(v) => v.to_le_bytes(),
            EfshValue::Fid(v) => v.to_le_bytes(),
            EfshValue::Rgba(v) => [v[0], v[1], v[2], v[3]],
        };
        data[field.offset..field.offset + size].copy_from_slice(&bytes);
    }
    data
}

/// One field of an `EFSH` (Effect Shader) record's `DATA` struct.
#[derive(Debug, Clone)]
pub struct EfshField {
    /// The field's offset in the struct.
    pub offset: usize,
    /// The field's name, from the spec.
    pub name: &'static str,
    /// The field's kind.
    pub kind: EfshKind,
    /// The field's value. A float32, a uint32, a FormID or a four-byte
    /// colour, depending on the kind.
    pub value: EfshValue,
}

/// The value of an `EFSH` struct field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EfshValue {
    /// A float32.
    F32(f32),
    /// A uint32.
    U32(u32),
    /// A FormID.
    Fid(u32),
    /// A four-byte colour.
    Rgba([u8; 4]),
}

impl EfshValue {
    /// The value as a float32, for the fields that are one.
    pub fn as_f32(&self) -> f32 {
        match self {
            EfshValue::F32(v) => *v,
            EfshValue::U32(v) => *v as f32,
            EfshValue::Fid(v) => *v as f32,
            EfshValue::Rgba(v) => v[0] as f32,
        }
    }

    /// The value as a uint32, for the fields that are one.
    pub fn as_u32(&self) -> u32 {
        match self {
            EfshValue::F32(v) => *v as u32,
            EfshValue::U32(v) => *v,
            EfshValue::Fid(v) => *v,
            EfshValue::Rgba(v) => u32::from_le_bytes([v[0], v[1], v[2], v[3]]),
        }
    }
}

/// The `DATA` struct of an `EFSH` (Effect Shader) record.
///
/// The struct is a long run of named fields, and the game's records are 224,
/// 248, 300 and 308 bytes long, so the later fields are simply absent from
/// the shorter ones. The struct is written back over the record's own bytes,
/// so the length it had and every byte the spec does not name survive a
/// save.
#[derive(Debug, Clone, Default)]
pub struct EfshData {
    /// The uint8 flag word: No Membrane Shader, No Particle Shader, Edge
    /// Effect Inverse, Membrane Shader Affects Skin Only.
    pub flags: u8,
    /// The fields, in the order they appear in the struct.
    pub fields: Vec<EfshField>,
}

/// One area of a `REGN` (Region) record.
///
/// The region area is a run of `RPLI` (the edge fall-off) and `RPLD` (the
/// point list), and the record can carry several of them. Each point list
/// belongs to the fall-off before it.
#[derive(Debug, Clone, Default)]
pub struct RegionArea {
    /// `RPLI` Edge Fall-Off — the uint32.
    pub edge_falloff: u32,
    /// `RPLD` Region Point List Data — the x and y float32 pairs.
    pub points: Vec<[f32; 2]>,
}

/// One region data entry on a `REGN` (Region) record.
///
/// The record's region data is a run of `RDAT` (the header) followed by the
/// entry's own subrecords, and the entries repeat. Each header's type says
/// which subrecords the entry carries, so the entries are walked in file
/// order rather than collected by signature.
#[derive(Debug, Clone, Default)]
pub struct RegionEntry {
    /// `RDAT` Type — the uint32: Objects, Weather, Map, Land, Grass, Sound
    /// or Imposter.
    pub kind: u32,
    /// `RDAT` Flags — the uint8: Override.
    pub flags: u8,
    /// `RDAT` Priority — the u8 the spec does not name. The game's records
    /// carry 50 here.
    pub priority: u8,
    /// `RDAT` Unused — the two unnamed bytes.
    pub unused: [u8; 2],
    /// `RDMP` Map Name — the cstring.
    pub map_name: Option<String>,
    /// `ICON` — the map's icon filename. It follows the entry it belongs
    /// to.
    pub icon: Option<String>,
    /// `RDMD` Music Type — the uint32.
    pub music_type: Option<u32>,
    /// `RDMO` Music — the FormID of a `MUSC` record.
    pub music: Option<u32>,
    /// `RDSI` Incidental MediaSet — the FormID of an `MSET` record.
    pub incidental_media: Option<u32>,
    /// `RDSB` Battle MediaSet — the FormIDs of the `MSET` records.
    pub battle_media: Vec<u32>,
    /// `RDOT` Objects — the object placements.
    pub objects: Vec<RegionObject>,
    /// Whether the entry carried an `RDOT` with no objects in it. The
    /// game's records do, and it is a subrecord like any other.
    pub had_empty_rdot: bool,
    /// `RDGS` Grasses — the grass entries.
    pub grasses: Vec<RegionGrass>,
    /// `RDSD` Sounds — the sound entries.
    pub sounds: Vec<RegionSound>,
    /// `RDWT` Weather Types — the weather entries.
    pub weather: Vec<RegionWeather>,
    /// `RDID` Imposters — the FormIDs of the `REFR` records.
    pub imposters: Vec<u32>,
}

/// One object placement on a `REGN` (Region) record.
#[derive(Debug, Clone, Default)]
pub struct RegionObject {
    /// The FormID of the `TREE`, `STAT` or `LTEX` record.
    pub object: u32,
    /// The parent index — the uint16.
    pub parent_index: u16,
    /// The two unnamed bytes.
    pub unused: [u8; 2],
    /// The density — the float32.
    pub density: f32,
    /// The clustering — the uint8.
    pub clustering: u8,
    /// The min slope — the uint8.
    pub min_slope: u8,
    /// The max slope — the uint8.
    pub max_slope: u8,
    /// The flag word — the uint8.
    pub flags: u8,
    /// The radius with respect to the parent — the uint16.
    pub parent_radius: u16,
    /// The radius — the uint16.
    pub radius: u16,
    /// The four unnamed bytes.
    pub unknown: [u8; 4],
    /// The max height — the float32.
    pub max_height: f32,
    /// The sink — the float32.
    pub sink: f32,
    /// The sink variance — the float32.
    pub sink_variance: f32,
    /// The size variance — the float32.
    pub size_variance: f32,
    /// The x angle variance — the uint16.
    pub x_angle_variance: u16,
    /// The y angle variance — the uint16.
    pub y_angle_variance: u16,
    /// The z angle variance — the uint16.
    pub z_angle_variance: u16,
    /// The six unnamed bytes.
    pub unknown2: [u8; 6],
}

/// One grass entry on a `REGN` (Region) record.
#[derive(Debug, Clone, Default)]
pub struct RegionGrass {
    /// The FormID of the `GRAS` record.
    pub grass: u32,
    /// The four unnamed bytes.
    pub unknown: [u8; 4],
}

/// One sound entry on a `REGN` (Region) record.
#[derive(Debug, Clone, Default)]
pub struct RegionSound {
    /// The FormID of the `SOUN` record.
    pub sound: u32,
    /// The flag word — the uint32: Pleasant, Cloudy, Rainy, Snowy.
    pub flags: u32,
    /// The chance — the uint32.
    pub chance: u32,
}

/// One weather entry on a `REGN` (Region) record.
#[derive(Debug, Clone, Default)]
pub struct RegionWeather {
    /// The FormID of the `WTHR` record.
    pub weather: u32,
    /// The chance — the uint32.
    pub chance: u32,
    /// The FormID of the `GLOB` record.
    pub global: Option<u32>,
}

/// One location on a `PACK` (Package) record.
///
/// A location is a `PLDT` or a `PLD2` subrecord: the type, the location
/// itself and a radius. The location's own type decides what the second
/// field means, so it is kept raw alongside the type.
#[derive(Debug, Clone, Default)]
pub struct PackageLocation {
    /// The type: Near Reference, In Cell, Near Current Location, Near Editor
    /// Location, Object ID, Object Type, Near Linked Reference, At Package
    /// Location.
    pub kind: u32,
    /// The location. A FormID for the reference and object-id types, a
    /// uint32 for the cell and object-type types, and the raw bytes for the
    /// types the spec leaves unnamed.
    pub target: Vec<u8>,
    /// The radius — the int32.
    pub radius: i32,
}

/// One target on a `PACK` (Package) record.
///
/// A target is a `PTDT` or a `PTD2` subrecord: the type, the target itself,
/// a count or distance, and an unnamed float32.
#[derive(Debug, Clone, Default)]
pub struct PackageTarget {
    /// The type: Specific Reference, Object ID, Object Type, Linked
    /// Reference.
    pub kind: i32,
    /// The target. A FormID for the reference and object-id types, a uint32
    /// for the object-type type, and the raw bytes for the type the spec
    /// leaves unnamed.
    pub target: Vec<u8>,
    /// The count or distance — the int32.
    pub count_distance: i32,
    /// The unnamed float32.
    pub unknown: f32,
}

/// The `PKW3` Use Weapon Data of a `PACK` record.
#[derive(Debug, Clone, Default)]
pub struct PackageWeapon {
    /// The uint32 flag word.
    pub flags: u32,
    /// The uint8: Auto Fire or Volley Fire.
    pub fire_rate: u8,
    /// The uint8: Number of Bursts or Repeat Fire.
    pub fire_count: u8,
    /// The uint16.
    pub num_bursts: u16,
    /// The uint16.
    pub shots_min: u16,
    /// The uint16.
    pub shots_max: u16,
    /// The float32.
    pub pause_min: f32,
    /// The float32.
    pub pause_max: f32,
    /// The four unnamed bytes.
    pub unused: [u8; 4],
}

/// The `PKDD` Dialog Data of a `PACK` record.
#[derive(Debug, Clone, Default)]
pub struct PackageDialog {
    /// The float32 field of view.
    pub fov: f32,
    /// The FormID of a `DIAL` record.
    pub topic: Option<u32>,
    /// The uint32 flag word.
    pub flags: u32,
    /// The four unnamed bytes.
    pub unused: [u8; 4],
    /// The uint32 dialog type.
    pub dialog_type: u32,
    /// The four trailing unnamed bytes.
    pub unused2: [u8; 4],
}

/// One embedded script on a `PACK` (Package) record.
///
/// The record carries three of these, one for each of the package's
/// on-begin, on-end and on-change events. Each is a marker subrecord
/// (`POBA`, `POEA` or `POCA`), an `INAM` idle animation, the shared Script
/// collection and a `TNAM` topic.
#[derive(Debug, Clone, Default)]
pub struct PackageScript {
    /// The marker that introduces this script.
    pub marker: [u8; 4],
    /// The `SCHR` subrecord's own bytes, so the fields the spec does not
    /// name survive a save.
    pub header_raw: Vec<u8>,
    /// `INAM` — the FormID of an `IDLE` record.
    pub idle: Option<u32>,
    /// `SCHR` — the script header.
    pub header: ScriptHeader,
    /// `SCDA` — the compiled script source.
    pub compiled: Vec<u8>,
    /// `SCTX` — the script source.
    pub source: String,
    /// The local variables. Each is an `SLSD` (the data), a `SCVR` (the
    /// name) and the `SCRV` references that name it.
    pub variables: Vec<ScriptVariable>,
    /// The `SCRO` references.
    pub references: Vec<u32>,
    /// `TNAM` — the FormID of a `DIAL` record.
    pub topic: Option<u32>,
}

/// The `SCHR` header of a script, from the shared Script collection.
#[derive(Debug, Clone, Default)]
pub struct ScriptHeader {
    /// `SCHR` Type — the uint16: Object, Quest or Effect.
    pub script_type: u32,
    /// `SCHR` Flags — the uint16: Enabled.
    pub flags: u16,
    /// `SCHR` Version — the uint32.
    pub version: u32,
    /// `SCHR` — the int32 unknown the spec names.
    pub unknown: i32,
}

/// One local variable in a script, from the shared Script collection.
///
/// The variables are a run of `SLSD` (the data) and `SCVR` (the name), and
/// both repeat.
#[derive(Debug, Clone, Default)]
pub struct ScriptVariable {
    /// `SLSD` Index — the uint32.
    pub index: u32,
    /// `SLSD` Flags — the u8 flag word: IsLongOrShort.
    pub flags: u8,
    /// `SCVR` — the local variable's name.
    pub name: String,
    /// The `SCRV` references that name this variable. Each is a uint32
    /// that points at it, and they follow the variable.
    pub refs: Vec<u32>,
    /// The `SLSD` subrecord's own bytes.
    ///
    /// The spec names the index, the flag word and the bytes around them,
    /// but the game's records carry values in the bytes after the flag word
    /// that the spec does not name. The variable is written back over these
    /// bytes, so those values survive an edit.
    pub raw: Vec<u8>,
}

/// One master file on a `TES4` (Plugin Header) record.
///
/// The masters are a run of `MAST` (the filename) and `SSIG` (the FormID),
/// and both repeat.
#[derive(Debug, Clone, Default)]
pub struct Tes4Master {
    /// `MAST` — the master file's name.
    pub filename: String,
    /// `SSIG` — the master's FormID.
    pub form_id: u32,
}

/// One collision part on a `SCOL` (Static Collision) record.
///
/// The record's collection is a run of `ONAM` (the static the part collides
/// with) and `DATA` (the placements), so the parts are walked in file order.
#[derive(Debug, Clone, Default)]
pub struct ScolPart {
    /// `ONAM` — the `STAT` record the part collides with.
    pub static_ref: Option<u32>,
    /// `DATA` — the placements, each a position, a rotation and a scale.
    pub placements: Vec<[f32; 7]>,
}

/// One debris model on a `DEBR` (Debris) record.
///
/// The record's collection is a run of `DATA` (the percentage, the model
/// filename and the flags) and `MODT` (the texture hashes) per entry, so the
/// entries are walked in file order.
#[derive(Debug, Clone, Default)]
pub struct DebrisModel {
    /// `DATA` Percentage — the uint8 chance this piece is spawned.
    pub percentage: u8,
    /// `DATA` Model Filename — the cstring.
    pub model: String,
    /// `DATA` Flags — the u8 flag word. Bit 0 is "Has Collision Data"; the
    /// byte is kept whole so a bit the spec does not name survives.
    pub flags: u8,
    /// `MODT` — the texture file hashes, copied through unchanged: the
    /// editor cannot author a file hash.
    pub texture_hashes: Vec<u8>,
}

/// One media-set list on an `ALOC` (Media Location Controller) record.
///
/// The record carries six lists of `MSET` records, one per relationship to the
/// player: neutral, ally, friend, enemy, location and battle. Each list is a
/// *repeated* subrecord, so each is a list rather than a single value.
#[derive(Debug, Clone, Default)]
pub struct AlocSet {
    /// The subrecord signature this list is written to.
    pub sig: [u8; 4],
    /// What the list is called in the editor.
    pub label: &'static str,
    /// The `MSET` records on the list, in the order the record stores them.
    pub entries: Vec<u32>,
}

/// The six media-set lists an `ALOC` record carries, in the order the record
/// stores them.
///
/// The signatures are the spec's: `HNAM` neutral, `ZNAM` ally, `XNAM` friend,
/// `YNAM` enemy, `LNAM` location, `GNAM` battle.
pub const ALOC_SET_LISTS: [([u8; 4], &str); 6] = [
    (*b"HNAM", "Neutral"),
    (*b"ZNAM", "Ally"),
    (*b"XNAM", "Friend"),
    (*b"YNAM", "Enemy"),
    (*b"LNAM", "Location"),
    (*b"GNAM", "Battle"),
];

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
        self.raw_snam.clear();
        self.actor_inventory_entries.clear();
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
        self.dobj_values.clear();
        self.furn_model = ModelData::default();
        self.furn_destruction.clear();
        self.furn_marker_flags.clear();
        self.gras_model = ModelData::default();
        self.idlm_animations.clear();
        self.lscr_locations.clear();
        self.cams_model = ModelData::default();
        self.cpth_shots.clear();
        self.cpth_conditions.clear();
        self.csno_models.clear();
        self.csno_icons.clear();
        self.csno_deck_icons.clear();
        self.csno_reel_stops.clear();
        self.debr_models.clear();
        self.ccrd_model = ModelData::default();
        self.ccrd_intv.clear();
        self.cdck_cards.clear();
        self.chip_model = ModelData::default();
        self.chip_destruction.clear();
        self.cmny_model = ModelData::default();
        self.arma_models.clear();
        self.arma_dnam_len = 0;
        self.aspc_sounds.clear();
        self.anio_model = ModelData::default();
        self.anio_animation = None;
        self.amef_type = 0;
        self.amef_operation = 0;
        self.amef_value = 0.0;
        self.aloc_sets.clear();
        self.aloc_conditional_faction = None;
        self.aloc_set_to_add = None;
        self.aloc_set_list_to_add = 0;
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
                self.raw_snam.push(s.data.clone());
            }
            if s.sig == *b"PKID" && s.data.len() >= 4 {
                let pkg_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                self.actor_packages.push(pkg_fid);
            }
            if s.sig == *b"CNTO" && s.data.len() >= 8 {
                let item_fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                let count = i32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                self.actor_inventory.push((item_fid, count));
                self.actor_inventory_entries.push(ActorInventoryEntry {
                    item: item_fid,
                    count,
                    extra: None,
                });
            }
            if s.sig == *b"LVLO" && s.data.len() >= 12 {
                let lvl = i16::from_le_bytes([s.data[0], s.data[1]]);
                let r_fid = u32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                let cnt = i16::from_le_bytes([s.data[10], s.data[11]]);
                self.lvl_entries.push(LeveledListEntry {
                    level: lvl,
                    reference: Some(r_fid),
                    count: cnt,
                    unused: [s.data[2], s.data[3], s.data[8], s.data[9]],
                    has_extra: false,
                    ..Default::default()
                });
            } else if s.sig == *b"COED" && s.data.len() >= 12 {
                // The extra data belongs to the entry before it: an inventory
                // item or a leveled list entry.
                if let Some(entry) = self.actor_inventory_entries.last_mut() {
                    if entry.extra.is_none() {
                        entry.extra = Some(s.data.clone());
                        continue;
                    }
                }
                if let Some(entry) = self.lvl_entries.last_mut() {
                    let b = &s.data;
                    let owner = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    entry.owner = if owner == 0 { None } else { Some(owner) };
                    entry.global_or_rank = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                    entry.item_condition = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
                    entry.has_extra = true;
                }
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
            b"CREA" => {
                self.load_actor();
                self.load_actor_destruction(&raw_subs);
            }
            b"NPC_" => {
                self.load_actor();
                self.load_actor_destruction(&raw_subs);
            }
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
            b"PERK" => self.load_perk_record(&raw_subs),
            b"ENCH" => self.load_spell(),
            b"EXPL" => self.load_expl(),
            b"PROJ" => self.load_proj(),
            b"SOUN" => self.load_soun(),
            b"NOTE" => self.load_note(),
            b"LVLC" => self.load_lvl(),
            b"LVLN" => self.load_lvl(),
            b"LVLI" => self.load_lvl(),
            b"IMOD" => self.load_imod(),
            b"CLAS" => self.load_clas(),
            b"CSTY" => self.load_csty(),
            b"MGEF" => self.load_mgef(),
            b"WTHR" => self.load_wthr(&raw_subs),
            b"WATR" => self.load_watr(),
            b"GLOB" => self.load_glob(),
            b"GMST" => self.load_gmst(),
            b"RACE" => self.load_race(),
            b"CLMT" => self.load_clmt(),
            b"ADDN" => self.load_addn(&raw_subs),
            b"ALOC" => self.load_aloc(&raw_subs),
            b"AMEF" => self.load_amef(),
            b"ARMA" => self.load_arma(&raw_subs),
            b"CAMS" => self.load_cams(&raw_subs),
            b"CHAL" => self.load_chal(),
            b"KEYM" => self.load_keym(&raw_subs),
            b"INGR" => self.load_ingr(&raw_subs),
            b"IPCT" => self.load_ipct(&raw_subs),
            b"IPDS" => self.load_ipds(),
            b"RCPE" => self.load_rcpe(&raw_subs),
            b"SCPT" => self.load_scpt(&raw_subs),
            b"PACK" => self.load_pack(&raw_subs),
            b"EFSH" => self.load_efsh(&raw_subs),
            b"REGN" => self.load_regn(&raw_subs),
            b"IMAD" => self.load_imad(&raw_subs),
            b"COBJ" => self.load_cobj(&raw_subs),
            b"PMIS" => self.load_pmis(&raw_subs),
            b"TES4" => self.load_tes4(&raw_subs),
            b"FLST" => self.load_flst(&raw_subs),
            b"MESG" => self.load_mesg(&raw_subs),
            b"MISC" => self.load_misc(&raw_subs),
            b"SPEL" => self.load_spel(&raw_subs),
            b"STAT" => self.load_stat(&raw_subs),
            b"IDLE" => self.load_idle(&raw_subs),
            b"MSET" => self.load_mset(&raw_subs),
            b"MSTT" => self.load_mstt(&raw_subs),
            b"PWAT" => self.load_pwat(&raw_subs),
            b"RCCT" => self.load_rcct(),
            b"REPU" => self.load_repu(),
            b"RGDL" => self.load_rgdl(),
            b"TREE" => self.load_tree(&raw_subs),
            b"TXST" => self.load_txst(&raw_subs),
            b"VTYP" => self.load_vtyp(),
            b"TACT" => self.load_tact(&raw_subs),
            b"SCOL" => self.load_scol(&raw_subs),
            b"IDLM" => self.load_idlm(&raw_subs),
            b"IMGS" => self.load_imgs(),
            b"LGTM" => self.load_lgtm(),
            b"LSCR" => self.load_lscr(&raw_subs),
            b"LSCT" => self.load_lsct(),
            b"MICN" => self.load_micn(),
            b"MUSC" => self.load_musc(),
            b"DEHY" => self.load_dehy(),
            b"DOBJ" => self.load_dobj(),
            b"ECZN" => self.load_eczn(),
            b"FURN" => self.load_furn(&raw_subs),
            b"GRAS" => self.load_gras(&raw_subs),
            b"HUNG" => self.load_hung(),
            b"RADS" => self.load_rads(),
            b"SLPD" => self.load_slpd(),
            b"CPTH" => self.load_cpth(&raw_subs),
            b"CSNO" => self.load_csno(&raw_subs),
            b"DEBR" => self.load_debr(&raw_subs),
            b"CCRD" => self.load_ccrd(&raw_subs),
            b"CDCK" => self.load_cdck(&raw_subs),
            b"CHIP" => self.load_chip(&raw_subs),
            b"CMNY" => self.load_cmny(&raw_subs),
            b"ASPC" => self.load_aspc(&raw_subs),
            b"ANIO" => self.load_anio(&raw_subs),
            b"AVIF" => self.load_avif(),
            b"EYES" => self.load_eyes(),
            b"HAIR" => self.load_hair(&raw_subs),
            b"HDPT" => self.load_hdpt(&raw_subs),
            b"BPTD" => self.load_bptd(&raw_subs),
            // A type with no page of its own: the header panel still shows
            // its name and FormID, but none of its fields are editable.
            _ => {}
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
            self.weap_animation_type = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
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
            self.armo_type_from_general = self.armo_type;
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
        self.actor_destruction = Vec::new();
        self.actor_dest_flags = 0;
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

    /// Read an actor's destruction data.
    ///
    /// The destruction collection is a `DEST` header followed by one or more
    /// stages, each a run of `DSTD`, `DMDL`, `DMDT` and `DSTF`.
    fn load_actor_destruction(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.actor_destruction = load_destruction(raw_subs);
        self.actor_dest_flags = raw_subs
            .iter()
            .find(|s| s.sig == *b"DEST" && s.data.len() >= 6)
            .map(|s| s.data[5])
            .unwrap_or(0);
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
        self.perk_trait = false;
        self.perk_min_level = 0;
        self.perk_num_ranks = 0;
        self.perk_playable = 0;
        self.perk_hidden = 0;
        self.perk_own_data = None;
        self.perk_desc = self.get_string(b"DESC");
    }

    /// Load a `PERK` (Perk) record.
    ///
    /// The record carries two kinds of `DATA`: the perk's own, which is five
    /// bytes long, and one per rank, which is three. The perk's own comes
    /// first, so it is the first `DATA` of the five-byte kind.
    fn load_perk_record(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.load_perk();
        let mut seen_own = false;
        for sub in raw_subs {
            if sub.sig != *b"DATA" {
                continue;
            }
            if sub.data.len() >= 5 && !seen_own {
                seen_own = true;
                self.perk_own_data = Some(sub.data.clone());
                self.perk_trait = sub.data[0] != 0;
                self.perk_min_level = sub.data[1];
                self.perk_num_ranks = sub.data[2];
                self.perk_playable = sub.data[3];
                self.perk_hidden = sub.data[4];
            }
        }
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
        self.sound_attenuation = SoundAttenuation::default();
        for sig in [b"SNDD", b"SNDX"] {
            if let Some(b) = self.raw_subs.get(sig) {
                self.sound_attenuation = read_sound_attenuation(b);
                break;
            }
        }
        // A record with the short form keeps its curve points, its reverb
        // control and its priority in their own subrecords.
        if let Some(b) = self.raw_subs.get(b"ANAM").filter(|b| b.len() >= 10) {
            for (i, point) in self
                .sound_attenuation
                .attenuation_points
                .iter_mut()
                .enumerate()
            {
                let at = i * 2;
                if b.len() >= at + 2 {
                    *point = i16::from_le_bytes([b[at], b[at + 1]]);
                }
            }
        }
        if let Some(b) = self.raw_subs.get(b"GNAM").filter(|b| b.len() >= 2) {
            self.sound_attenuation.reverb_attenuation = i16::from_le_bytes([b[0], b[1]]);
        }
        if let Some(b) = self.raw_subs.get(b"HNAM").filter(|b| b.len() >= 4) {
            self.sound_attenuation.priority = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        self.sound_min_dist = self.sound_attenuation.min_distance;
        self.sound_max_dist = self.sound_attenuation.max_distance;
        self.sound_freq_adj = self.sound_attenuation.frequency_adjustment;
        self.sound_loop = (self.sound_attenuation.flags & (1 << 4)) != 0;
        self.sound_2d = (self.sound_attenuation.flags & (1 << 6)) != 0;
        self.sound_vol_cdb = self.sound_attenuation.static_attenuation;
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

    /// Load a `WTHR` (Weather) record.
    ///
    /// The record's colours are laid out as ten colour types, each with
    /// one colour per time of day. The game's records carry six times of
    /// day, but eight of the sixty-three records carry four, so the
    /// number of times is measured from the subrecord's length.
    fn load_wthr(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.wthr_cloud_layers = std::array::from_fn(|_| String::new());
        self.wthr_max_cloud_layers = 0;
        self.wthr_cloud_speeds = [0; 4];
        self.wthr_cloud_colors.clear();
        self.wthr_colors.clear();
        self.wthr_fog_distances = [0.0; 6];
        self.wthr_cloud_speed_lower = 0;
        self.wthr_cloud_speed_upper = 0;
        self.wthr_transition_delta = 0;
        self.wthr_precipitation_begin = 0;
        self.wthr_precipitation_end = 0;
        self.wthr_thunder_begin = 0;
        self.wthr_thunder_end = 0;
        self.wthr_thunder_frequency = 0;
        self.wthr_weather_classification = 0;
        self.wthr_lightning_color = [0, 0, 0, 255];
        self.wthr_image_space_modifiers = [None; 6];
        self.wthr_sounds.clear();

        // The image space modifiers. Their signatures begin with an
        // unprintable byte that names the time of day.
        for sub in raw_subs {
            if sub.sig.len() == 4 && sub.sig[1..] == *b"IAD" && sub.data.len() >= 4 {
                let index = sub.sig[0] as usize;
                if index < 6 {
                    self.wthr_image_space_modifiers[index] = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
            }
        }

        for (sig, slot) in [(b"DNAM", 0usize), (b"CNAM", 1), (b"ANAM", 2), (b"BNAM", 3)] {
            if let Some(b) = self.raw_subs.get(sig) {
                self.wthr_cloud_layers[slot] = zstring(b);
            }
        }
        self.wthr_cloud_layer0 = self.wthr_cloud_layers[0].clone();

        if let Some(b) = self.raw_subs.get(b"LNAM").filter(|b| b.len() >= 4) {
            self.wthr_max_cloud_layers = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        if let Some(b) = self.raw_subs.get(b"ONAM") {
            for (i, speed) in self.wthr_cloud_speeds.iter_mut().enumerate() {
                *speed = b.get(i).copied().unwrap_or(0);
            }
        }

        // The cloud layer colours: one time of day colours struct per
        // cloud layer.
        if let Some(b) = self.raw_subs.get(b"PNAM") {
            let times = if b.len() % 96 == 0 { 6 } else { 4 };
            for chunk in b.chunks_exact(times * 4) {
                self.wthr_cloud_colors
                    .push(read_time_of_day_colors(chunk, times));
            }
        }

        // The weather's own colours: ten colour types, each with one
        // colour per time of day.
        if let Some(b) = self.raw_subs.get(b"NAM0") {
            let times = if b.len() % 240 == 0 {
                6
            } else if b.len() % 160 == 0 {
                4
            } else {
                6
            };
            for chunk in b.chunks_exact(times * 4) {
                self.wthr_colors.push(read_time_of_day_colors(chunk, times));
            }
        }

        // The fog distances: six float32s.
        if let Some(b) = self.raw_subs.get(b"FNAM").filter(|b| b.len() >= 24) {
            for (i, d) in self.wthr_fog_distances.iter_mut().enumerate() {
                let at = i * 4;
                *d = f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            }
        }

        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 15) {
            self.wthr_wind_speed = b[0];
            self.wthr_cloud_speed_lower = b[1];
            self.wthr_cloud_speed_upper = b[2];
            self.wthr_transition_delta = b[3];
            self.wthr_sun_glare = b[4];
            self.wthr_sun_damage = b[5];
            self.wthr_precipitation_begin = b[6];
            self.wthr_precipitation_end = b[7];
            self.wthr_thunder_begin = b[8];
            self.wthr_thunder_end = b[9];
            self.wthr_thunder_frequency = b[10];
            self.wthr_weather_classification = b[11];
            self.wthr_lightning_color = [b[12], b[13], b[14], 255];
        }

        // The sounds.
        for sub in raw_subs {
            if sub.sig == *b"SNAM" && sub.data.len() >= 8 {
                self.wthr_sounds.push(WeatherSound {
                    sound: u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]),
                    kind: u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]),
                });
            }
        }
    }

    fn load_watr(&mut self) {
        self.watr_visual = WatrVisual::default();
        self.watr_daytime = None;
        self.watr_nighttime = None;
        self.watr_underwater = None;
        if let Some(b) = self.raw_subs.get(b"DNAM") {
            self.watr_visual = read_watr_visual(b);
        }
        if let Some(b) = self.raw_subs.get(b"GNAM").filter(|b| b.len() >= 12) {
            self.watr_daytime = Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            self.watr_nighttime = Some(u32::from_le_bytes([b[4], b[5], b[6], b[7]]));
            self.watr_underwater = Some(u32::from_le_bytes([b[8], b[9], b[10], b[11]]));
        }
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

    /// Load a `DEHY` (Dehydration) record.
    fn load_dehy(&mut self) {
        self.dehyd_trigger_threshold = 0;
        self.dehyd_actor_effect = None;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.dehyd_trigger_threshold = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.dehyd_actor_effect = Some(u32::from_le_bytes([b[4], b[5], b[6], b[7]]));
        }
    }

    /// Load a `HUNG` (Hunger) record.
    fn load_hung(&mut self) {
        self.hung_trigger_threshold = 0;
        self.hung_actor_effect = None;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.hung_trigger_threshold = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.hung_actor_effect = Some(u32::from_le_bytes([b[4], b[5], b[6], b[7]]));
        }
    }

    /// Load a `RADS` (Radiation) record.
    fn load_rads(&mut self) {
        self.rads_trigger_threshold = 0;
        self.rads_actor_effect = None;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.rads_trigger_threshold = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.rads_actor_effect = Some(u32::from_le_bytes([b[4], b[5], b[6], b[7]]));
        }
    }

    /// Load a `SLPD` (Sleep Deprivation) record.
    fn load_slpd(&mut self) {
        self.slpd_trigger_threshold = 0;
        self.slpd_actor_effect = None;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.slpd_trigger_threshold = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.slpd_actor_effect = Some(u32::from_le_bytes([b[4], b[5], b[6], b[7]]));
        }
    }

    /// Load an `ECZN` (Encounter Zone) record.
    fn load_eczn(&mut self) {
        self.eczn_owner = None;
        self.eczn_rank = 0;
        self.eczn_minimum_level = 0;
        self.eczn_flags = 0;
        self.eczn_unused = 0;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 8) {
            self.eczn_owner = Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            self.eczn_rank = b[4] as i8;
            self.eczn_minimum_level = b[5] as i8;
            self.eczn_flags = b[6];
            self.eczn_unused = b[7];
        }
    }

    /// Load a `DOBJ` (Default Object Manager) record.
    ///
    /// The `DATA` subrecord is a flat array of thirty-four FormIDs, one per
    /// default object. They are read in order and kept as a list, and
    /// `DOBJ_FIELDS` names them.
    fn load_dobj(&mut self) {
        self.dobj_values.clear();
        if let Some(b) = self.raw_subs.get(b"DATA") {
            for chunk in b.as_chunks::<4>().0 {
                self.dobj_values
                    .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
    }

    /// Load a `FURN` (Furniture) record.
    fn load_furn(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.furn_marker_flags.clear();
        self.furn_model = ModelData::default();
        self.furn_destruction.clear();
        self.furn_dest_health = 0;
        self.furn_dest_flags = 0;

        for s in raw_subs {
            match &s.sig {
                b"MNAM" => self.furn_marker_flags = s.data.clone(),
                b"DEST" if s.data.len() >= 8 => {
                    self.furn_dest_health =
                        i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.furn_dest_flags = s.data[5];
                }
                b"MODL" => self.furn_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.furn_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.furn_model.modt = s.data.clone(),
                b"MODS" => self.furn_model.mods = parse_mods(&s.data),
                b"MODD" => self.furn_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
        self.furn_destruction = load_destruction(raw_subs);
    }

    /// Load a `GRAS` (Grass) record.
    ///
    /// `DATA` is thirty-two bytes. The spec marks several fields unused; they
    /// are carried through unchanged rather than being zeroed.
    fn load_gras(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.gras_density = 0;
        self.gras_min_slope = 0;
        self.gras_max_slope = 0;
        self.gras_unit_from_water_amount = 0;
        self.gras_unit_from_water_type = 0;
        self.gras_position_range = 0.0;
        self.gras_height_range = 0.0;
        self.gras_color_range = 0.0;
        self.gras_wave_period = 0.0;
        self.gras_flags = 0;
        self.gras_model = ModelData::default();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if s.data.len() >= 32 => {
                    self.gras_density = s.data[0];
                    self.gras_min_slope = s.data[1];
                    self.gras_max_slope = s.data[2];
                    self.gras_unit_from_water_amount = u16::from_le_bytes([s.data[4], s.data[5]]);
                    self.gras_unit_from_water_type =
                        u32::from_le_bytes([s.data[8], s.data[9], s.data[10], s.data[11]]);
                    self.gras_position_range =
                        f32::from_le_bytes([s.data[12], s.data[13], s.data[14], s.data[15]]);
                    self.gras_height_range =
                        f32::from_le_bytes([s.data[16], s.data[17], s.data[18], s.data[19]]);
                    self.gras_color_range =
                        f32::from_le_bytes([s.data[20], s.data[21], s.data[22], s.data[23]]);
                    self.gras_wave_period =
                        f32::from_le_bytes([s.data[24], s.data[25], s.data[26], s.data[27]]);
                    self.gras_flags = s.data[28];
                }
                b"MODL" => self.gras_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.gras_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.gras_model.modt = s.data.clone(),
                b"MODS" => self.gras_model.mods = parse_mods(&s.data),
                b"MODD" => self.gras_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load an `IDLM` (Idle Animation) record.
    ///
    /// The record is `EDID`, `OBND`, `IDLF` (a flag word), `IDLC` (the
    /// animation count), `IDLT` (the idle timer) and `IDLA` (the list of
    /// `IDLE` records). `IDLA` is a *repeated* subrecord, so the animations
    /// are read in file order and kept as a list.
    fn load_idlm(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.idlm_flags = 0;
        self.idlm_animation_count = 0;
        self.idlm_idlc_unused = [0; 3];
        self.idlm_idle_timer = 0.0;
        self.idlm_animations.clear();

        for s in raw_subs {
            match &s.sig {
                b"IDLF" if !s.data.is_empty() => self.idlm_flags = s.data[0],
                b"IDLC" if !s.data.is_empty() => {
                    self.idlm_animation_count = s.data[0];
                    if s.data.len() >= 4 {
                        self.idlm_idlc_unused = [s.data[1], s.data[2], s.data[3]];
                    }
                }
                b"IDLT" if s.data.len() >= 4 => {
                    self.idlm_idle_timer =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"IDLA" => {
                    // The animation list is a run of four-byte FormIDs.
                    for chunk in s.data.as_chunks::<4>().0 {
                        self.idlm_animations
                            .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                    }
                }
                _ => {}
            }
        }
    }

    /// Load an `IMGS` (Image Space) record.
    ///
    /// `DNAM` is version-dependent — 152 bytes on 67 of the game's records,
    /// 148 on twelve and 132 on eleven. The schema names the first fourteen
    /// HDR floats; everything after them depends on the record's form
    /// version, so it is carried through as raw bytes rather than guessed
    /// at.
    fn load_imgs(&mut self) {
        self.imgs_hdr = [0.0; 14];
        self.imgs_rest.clear();
        if let Some(b) = self.raw_subs.get(b"DNAM") {
            for (i, chunk) in b.as_chunks::<4>().0.iter().take(14).enumerate() {
                self.imgs_hdr[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            if b.len() > 56 {
                self.imgs_rest = b[56..].to_vec();
            }
        }
    }

    /// Load a `LGTM` (Lighting) record.
    fn load_lgtm(&mut self) {
        self.lgtm_ambient_color = [0; 4];
        self.lgtm_directional_color = [0; 4];
        self.lgtm_fog_color = [0; 4];
        self.lgtm_fog_near = 0.0;
        self.lgtm_fog_far = 0.0;
        self.lgtm_directional_rotation_xy = 0;
        self.lgtm_directional_rotation_z = 0;
        self.lgtm_directional_fade = 0.0;
        self.lgtm_fog_clip_distance = 0.0;
        self.lgtm_fog_power = 0.0;

        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 40) {
            self.lgtm_ambient_color = [b[0], b[1], b[2], b[3]];
            self.lgtm_directional_color = [b[4], b[5], b[6], b[7]];
            self.lgtm_fog_color = [b[8], b[9], b[10], b[11]];
            self.lgtm_fog_near = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.lgtm_fog_far = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.lgtm_directional_rotation_xy = i32::from_le_bytes([b[20], b[21], b[22], b[23]]);
            self.lgtm_directional_rotation_z = i32::from_le_bytes([b[24], b[25], b[26], b[27]]);
            self.lgtm_directional_fade = f32::from_le_bytes([b[28], b[29], b[30], b[31]]);
            self.lgtm_fog_clip_distance = f32::from_le_bytes([b[32], b[33], b[34], b[35]]);
            self.lgtm_fog_power = f32::from_le_bytes([b[36], b[37], b[38], b[39]]);
        }
    }

    /// Load an `LSCR` (Load Screen) record.
    ///
    /// The record's `LNAM` locations are *repeated* subrecords, so they are
    /// read in file order and kept as a list.
    fn load_lscr(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.lscr_icon.clear();
        self.lscr_mico.clear();
        self.lscr_desc.clear();
        self.lscr_load_screen_type = None;
        self.lscr_locations.clear();

        for s in raw_subs {
            match &s.sig {
                b"ICON" => self.lscr_icon = zstring(&s.data),
                b"MICO" => self.lscr_mico = zstring(&s.data),
                b"DESC" => self.lscr_desc = zstring(&s.data),
                b"WMI1" if s.data.len() >= 4 => {
                    self.lscr_load_screen_type = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"LNAM" if s.data.len() >= 12 => {
                    self.lscr_locations.push(LoadScreenLocation {
                        direct: Some(u32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ])),
                        indirect_world: Some(u32::from_le_bytes([
                            s.data[4], s.data[5], s.data[6], s.data[7],
                        ])),
                        indirect_grid_y: i16::from_le_bytes([s.data[8], s.data[9]]),
                        indirect_grid_x: i16::from_le_bytes([s.data[10], s.data[11]]),
                    });
                }
                _ => {}
            }
        }
    }

    /// Load an `LSCT` (Load Screen Type) record.
    ///
    /// `DATA` is eighty-eight bytes: the type, the position and size, the
    /// first font's settings and colour, the second font's settings and
    /// colour, and the stats type.
    fn load_lsct(&mut self) {
        self.lsct_type = 0;
        self.lsct_x = 0;
        self.lsct_y = 0;
        self.lsct_width = 0;
        self.lsct_height = 0;
        self.lsct_orientation = 0.0;
        self.lsct_font1 = 0;
        self.lsct_font1_color = [0.0; 3];
        self.lsct_font1_alignment = 0;
        self.lsct_font2 = 0;
        self.lsct_font2_color = [0.0; 3];
        self.lsct_font2_alignment = 0;
        self.lsct_stats = 0;

        // The layout, measured from the game's own record:
        //   0  type          4  x            8  y           12 width
        //  16  height       20  orientation 24 font 1      28 font 1 red
        //  32  font 1 green 36  font 1 blue  40 font 1 align
        //  44..64 the spec's twenty unknown bytes
        //  64  font 2       68  font 2 red   72 font 2 green
        //  76  font 2 blue  80 font 2 align 84 stats type
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 88) {
            self.lsct_type = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.lsct_x = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.lsct_y = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.lsct_width = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.lsct_height = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.lsct_orientation = f32::from_le_bytes([b[20], b[21], b[22], b[23]]);
            self.lsct_font1 = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
            self.lsct_font1_color[0] = f32::from_le_bytes([b[28], b[29], b[30], b[31]]);
            self.lsct_font1_color[1] = f32::from_le_bytes([b[32], b[33], b[34], b[35]]);
            self.lsct_font1_color[2] = f32::from_le_bytes([b[36], b[37], b[38], b[39]]);
            self.lsct_font1_alignment = u32::from_le_bytes([b[40], b[41], b[42], b[43]]);
            self.lsct_font2 = u32::from_le_bytes([b[64], b[65], b[66], b[67]]);
            self.lsct_font2_color[0] = f32::from_le_bytes([b[68], b[69], b[70], b[71]]);
            self.lsct_font2_color[1] = f32::from_le_bytes([b[72], b[73], b[74], b[75]]);
            self.lsct_font2_color[2] = f32::from_le_bytes([b[76], b[77], b[78], b[79]]);
            self.lsct_font2_alignment = u32::from_le_bytes([b[80], b[81], b[82], b[83]]);
            self.lsct_stats = u32::from_le_bytes([b[84], b[85], b[86], b[87]]);
        }
    }

    /// Load a `MICN` (Mission Icon) record.
    fn load_micn(&mut self) {
        self.micn_icon = self.get_string(b"ICON");
        self.micn_mico = self.get_string(b"MICO");
    }

    /// Load a `MUSC` (Music) record.
    fn load_musc(&mut self) {
        self.musc_filename = self.get_string(b"FNAM");
        self.musc_db = self.get_f32(b"ANAM").unwrap_or(0.0);
    }

    /// Load a `RCCT` (Recipe Category) record.
    fn load_rcct(&mut self) {
        self.rcct_flags = 0;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| !b.is_empty()) {
            self.rcct_flags = b[0];
        }
    }

    /// Load a `REPU` (Reputation) record.
    fn load_repu(&mut self) {
        self.repu_value = self.get_f32(b"DATA").unwrap_or(0.0);
        self.repu_icon = self.get_string(b"ICON");
        self.repu_mico = self.get_string(b"MICO");
    }

    /// Load an `RGDL` (Ragdoll) record.
    ///
    /// `RAFD` and `RAPS` are documented but not decoded, so they are carried
    /// through unchanged rather than guessed at.
    fn load_rgdl(&mut self) {
        self.rgdl_version = 0;
        self.rgdl_dynamic_bone_count = 0;
        self.rgdl_feedback_enabled = 0;
        self.rgdl_foot_ik_enabled = 0;
        self.rgdl_look_ik_enabled = 0;
        self.rgdl_grab_ik_enabled = 0;
        self.rgdl_pose_matching = 0;
        self.rgdl_actor_base = None;
        self.rgdl_body_part_data = None;
        self.rgdl_death_pose.clear();
        self.rgdl_feedback_bones.clear();
        self.rgdl_dynamic_keyframe_blend = 0.0;
        self.rgdl_hierarchy_gain = 0.0;
        self.rgdl_position_gain = 0.0;
        self.rgdl_velocity_gain = 0.0;
        self.rgdl_acceleration_gain = 0.0;
        self.rgdl_snap_gain = 0.0;
        self.rgdl_velocity_damping = 0.0;
        self.rgdl_snap_max_linear_velocity = 0.0;
        self.rgdl_snap_max_angular_velocity = 0.0;
        self.rgdl_snap_max_linear_distance = 0.0;
        self.rgdl_snap_max_angular_distance = 0.0;
        self.rgdl_position_max_linear_velocity = 0.0;
        self.rgdl_position_max_angular_velocity = 0.0;
        self.rgdl_projectile_position_max_velocity = 0;
        self.rgdl_melee_position_max_velocity = 0;
        self.rgdl_match_bone1 = 0;
        self.rgdl_match_bone2 = 0;
        self.rgdl_match_bone3 = 0;
        self.rgdl_aps_flags = 0;
        self.rgdl_motors_strength = 0.0;
        self.rgdl_pose_activation_delay = 0.0;
        self.rgdl_match_error_allowance = 0.0;
        self.rgdl_displacement_to_disable = 0.0;

        if let Some(b) = self.raw_subs.get(b"NVER").filter(|b| b.len() >= 4) {
            self.rgdl_version = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 14) {
            self.rgdl_dynamic_bone_count = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.rgdl_feedback_enabled = b[8];
            self.rgdl_foot_ik_enabled = b[9];
            self.rgdl_look_ik_enabled = b[10];
            self.rgdl_grab_ik_enabled = b[11];
            self.rgdl_pose_matching = b[12];
        }
        if let Some(b) = self.raw_subs.get(b"XNAM").filter(|b| b.len() >= 4) {
            self.rgdl_actor_base = Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
        if let Some(b) = self.raw_subs.get(b"TNAM").filter(|b| b.len() >= 4) {
            self.rgdl_body_part_data = Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
        if let Some(b) = self.raw_subs.get(b"ANAM") {
            self.rgdl_death_pose = zstring(b);
        }
        if let Some(b) = self.raw_subs.get(b"RAFB") {
            for chunk in b.as_chunks::<2>().0 {
                self.rgdl_feedback_bones
                    .push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }
        }
        // RAFD is sixty bytes: thirteen float32s, then the two velocities.
        if let Some(b) = self.raw_subs.get(b"RAFD").filter(|b| b.len() >= 60) {
            let f = |i: usize| {
                let at = i * 4;
                f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
            };
            self.rgdl_dynamic_keyframe_blend = f(0);
            self.rgdl_hierarchy_gain = f(1);
            self.rgdl_position_gain = f(2);
            self.rgdl_velocity_gain = f(3);
            self.rgdl_acceleration_gain = f(4);
            self.rgdl_snap_gain = f(5);
            self.rgdl_velocity_damping = f(6);
            self.rgdl_snap_max_linear_velocity = f(7);
            self.rgdl_snap_max_angular_velocity = f(8);
            self.rgdl_snap_max_linear_distance = f(9);
            self.rgdl_snap_max_angular_distance = f(10);
            self.rgdl_position_max_linear_velocity = f(11);
            self.rgdl_position_max_angular_velocity = f(12);
            self.rgdl_projectile_position_max_velocity =
                i32::from_le_bytes([b[52], b[53], b[54], b[55]]);
            self.rgdl_melee_position_max_velocity =
                u32::from_le_bytes([b[56], b[57], b[58], b[59]]);
        }
        // RAPS is twenty-four bytes: three match bones, the flag word, an
        // unused byte and four float32s.
        if let Some(b) = self.raw_subs.get(b"RAPS").filter(|b| b.len() >= 24) {
            self.rgdl_match_bone1 = u16::from_le_bytes([b[0], b[1]]);
            self.rgdl_match_bone2 = u16::from_le_bytes([b[2], b[3]]);
            self.rgdl_match_bone3 = u16::from_le_bytes([b[4], b[5]]);
            self.rgdl_aps_flags = b[6];
            self.rgdl_motors_strength = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.rgdl_pose_activation_delay = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.rgdl_match_error_allowance = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            self.rgdl_displacement_to_disable = f32::from_le_bytes([b[20], b[21], b[22], b[23]]);
        }
    }

    /// Load a `TREE` (Tree) record.
    fn load_tree(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.tree_seeds.clear();
        self.tree_leaf_curvature = 0.0;
        self.tree_min_leaf_angle = 0.0;
        self.tree_max_leaf_angle = 0.0;
        self.tree_branch_dimming = 0.0;
        self.tree_leaf_dimming = 0.0;
        self.tree_shadow_radius = 0;
        self.tree_rock_speed = 0.0;
        self.tree_rustle_speed = 0.0;
        self.tree_billboard_width = 0.0;
        self.tree_billboard_height = 0.0;
        self.tree_icon = self.get_string(b"ICON");
        self.tree_mico = self.get_string(b"MICO");
        self.tree_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.tree_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.tree_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.tree_model.modt = sub.data.clone(),
                b"MODS" => self.tree_model.mods = parse_mods(&sub.data),
                b"MODD" => self.tree_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }

        for s in raw_subs {
            match &s.sig {
                b"SNAM" => {
                    for chunk in s.data.as_chunks::<4>().0 {
                        self.tree_seeds
                            .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                    }
                }
                b"CNAM" if s.data.len() >= 32 => {
                    let f = |i: usize| {
                        let at = i * 4;
                        f32::from_le_bytes([
                            s.data[at],
                            s.data[at + 1],
                            s.data[at + 2],
                            s.data[at + 3],
                        ])
                    };
                    self.tree_leaf_curvature = f(0);
                    self.tree_min_leaf_angle = f(1);
                    self.tree_max_leaf_angle = f(2);
                    self.tree_branch_dimming = f(3);
                    self.tree_leaf_dimming = f(4);
                    // The shadow radius is an int32, not a float.
                    self.tree_shadow_radius =
                        i32::from_le_bytes([s.data[20], s.data[21], s.data[22], s.data[23]]);
                    self.tree_rock_speed = f(6);
                    self.tree_rustle_speed = f(7);
                }
                b"BNAM" if s.data.len() >= 8 => {
                    self.tree_billboard_width =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.tree_billboard_height =
                        f32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                }
                _ => {}
            }
        }
    }

    /// Load a `TXST` (Texture) record.
    fn load_txst(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.txst_textures = vec![String::new(); 6];
        self.txst_flags = 0;
        self.txst_dodt_min_width = 0.0;
        self.txst_dodt_max_width = 0.0;
        self.txst_dodt_min_height = 0.0;
        self.txst_dodt_max_height = 0.0;
        self.txst_dodt_depth = 0.0;
        self.txst_dodt_shininess = 0.0;
        self.txst_dodt_parallax_scale = 0.0;
        self.txst_dodt_parallax_passes = 0;
        self.txst_dodt_flags = 0;
        self.txst_dodt_color = [0; 4];
        for s in raw_subs {
            let idx = match &s.sig {
                b"TX00" => 0,
                b"TX01" => 1,
                b"TX02" => 2,
                b"TX03" => 3,
                b"TX04" => 4,
                b"TX05" => 5,
                b"DNAM" => {
                    if s.data.len() >= 2 {
                        self.txst_flags = u16::from_le_bytes([s.data[0], s.data[1]]);
                    }
                    continue;
                }
                b"DODT" if s.data.len() >= 36 => {
                    let d = read_decal_data(&s.data);
                    self.txst_dodt_min_width = d.min_width;
                    self.txst_dodt_max_width = d.max_width;
                    self.txst_dodt_min_height = d.min_height;
                    self.txst_dodt_max_height = d.max_height;
                    self.txst_dodt_depth = d.depth;
                    self.txst_dodt_shininess = d.shininess;
                    self.txst_dodt_parallax_scale = d.parallax_scale;
                    self.txst_dodt_parallax_passes = d.parallax_passes;
                    self.txst_dodt_flags = d.flags;
                    self.txst_dodt_color = d.color;
                    continue;
                }
                _ => continue,
            };
            self.txst_textures[idx] = zstring(&s.data);
        }
    }

    /// Load a `VTYP` (Voice Type) record.
    fn load_vtyp(&mut self) {
        self.vtyp_flags = 0;
        if let Some(b) = self.raw_subs.get(b"DNAM").filter(|b| !b.is_empty()) {
            self.vtyp_flags = b[0];
        }
    }

    /// Load a `TACT` (Talking Activator) record.
    fn load_tact(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.tact_looping_sound = None;
        self.tact_voice_type = None;
        self.tact_radio_template = None;
        self.tact_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.tact_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.tact_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.tact_model.modt = sub.data.clone(),
                b"MODS" => self.tact_model.mods = parse_mods(&sub.data),
                b"MODD" => self.tact_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }
        self.tact_destruction.clear();
        self.tact_dest_health = 0;
        self.tact_dest_flags = 0;

        for s in raw_subs {
            match &s.sig {
                b"SNAM" if s.data.len() >= 4 => {
                    self.tact_looping_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"VNAM" if s.data.len() >= 4 => {
                    self.tact_voice_type = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"INAM" if s.data.len() >= 4 => {
                    self.tact_radio_template = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"SCRI" if s.data.len() >= 4 => {
                    self.tact_script = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"DEST" => {
                    self.tact_destruction = load_destruction(raw_subs);
                    if s.data.len() >= 5 {
                        self.tact_dest_health =
                            i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                        self.tact_dest_flags = s.data[5];
                    }
                }
                _ => {}
            }
        }
    }

    /// Load a `SCOL` (Static Collision) record.
    ///
    /// The collection is a run of `ONAM` (the static) and `DATA` (the
    /// placements). Both repeat, so the parts are walked in file order: a
    /// `DATA` belongs to the `ONAM` before it.
    fn load_scol(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.scol_parts.clear();
        self.scol_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.scol_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.scol_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.scol_model.modt = sub.data.clone(),
                b"MODS" => self.scol_model.mods = parse_mods(&sub.data),
                b"MODD" => self.scol_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }
        let mut current: Option<ScolPart> = None;

        for s in raw_subs {
            match &s.sig {
                b"ONAM" if s.data.len() >= 4 => {
                    if let Some(part) = current.take() {
                        self.scol_parts.push(part);
                    }
                    current = Some(ScolPart {
                        static_ref: Some(u32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ])),
                        placements: Vec::new(),
                    });
                }
                b"DATA" => {
                    // Each placement is a position, a rotation and a scale.
                    let mut part = current.take().unwrap_or_default();
                    for chunk in s.data.as_chunks::<28>().0 {
                        let f = |i: usize| {
                            let at = i * 4;
                            f32::from_le_bytes([
                                chunk[at],
                                chunk[at + 1],
                                chunk[at + 2],
                                chunk[at + 3],
                            ])
                        };
                        part.placements
                            .push([f(0), f(1), f(2), f(3), f(4), f(5), f(6)]);
                    }
                    current = Some(part);
                }
                _ => {}
            }
        }
        if let Some(part) = current {
            self.scol_parts.push(part);
        }
    }

    /// Load an `IDLE` (Idle Animation) record.
    fn load_idle(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.idle_parent = None;
        self.idle_previous_sibling = None;
        self.idle_group_section = 0;
        self.idle_looping_min = 0;
        self.idle_looping_max = 0;
        self.idle_replay_delay = 0;
        self.idle_flags = 0;
        self.idle_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.idle_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.idle_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.idle_model.modt = sub.data.clone(),
                b"MODS" => self.idle_model.mods = parse_mods(&sub.data),
                b"MODD" => self.idle_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }

        for sub in raw_subs {
            match &sub.sig {
                b"ANAM" if sub.data.len() >= 8 => {
                    let parent =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.idle_parent = if parent == 0 { None } else { Some(parent) };
                    let previous =
                        u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                    self.idle_previous_sibling = if previous == 0 { None } else { Some(previous) };
                }
                b"DATA" if sub.data.len() >= 6 => {
                    self.idle_group_section = sub.data[0];
                    self.idle_looping_min = sub.data[1];
                    self.idle_looping_max = sub.data[2];
                    self.idle_replay_delay = i16::from_le_bytes([sub.data[4], sub.data[5]]);
                    if sub.data.len() >= 7 {
                        self.idle_flags = sub.data[6];
                    }
                }
                _ => {}
            }
        }
        self.idle_conditions = load_conditions(raw_subs);
    }

    /// Load an `MSET` (Media Set) record.
    ///
    /// The `JNAM` boundaries repeat, so they are walked in file order and
    /// kept as a list. The six track names and the six track levels each
    /// have their own signature, so they are read one by one.
    fn load_mset(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.mset_type = -1;
        self.mset_tracks = vec![String::new(); 6];
        self.mset_levels = vec![0.0; 6];
        self.mset_boundaries.clear();
        self.mset_enable_flags = 0;
        self.mset_wait_time = 0.0;
        self.mset_loop_fade_out = 0.0;
        self.mset_recovery_time = 0.0;
        self.mset_nighttime_max = 0.0;
        self.mset_intro = None;
        self.mset_outro = None;

        for s in raw_subs {
            let track = match &s.sig {
                b"NAM1" => {
                    if s.data.len() >= 4 {
                        self.mset_type =
                            i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    }
                    None
                }
                b"NAM2" => Some(0),
                b"NAM3" => Some(1),
                b"NAM4" => Some(2),
                b"NAM5" => Some(3),
                b"NAM6" => Some(4),
                b"NAM7" => Some(5),
                b"NAM8" => Some(0),
                b"NAM9" => Some(1),
                b"NAM0" => Some(2),
                b"ANAM" => Some(3),
                b"BNAM" => Some(4),
                b"CNAM" => Some(5),
                b"JNAM" => {
                    // A boundary percentage. The subrecord repeats.
                    if s.data.len() >= 4 {
                        self.mset_boundaries.push(f32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ]));
                    }
                    None
                }
                b"PNAM" => {
                    if !s.data.is_empty() {
                        self.mset_enable_flags = s.data[0];
                    }
                    None
                }
                b"DNAM" => {
                    self.mset_wait_time =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    None
                }
                b"ENAM" => {
                    self.mset_loop_fade_out =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    None
                }
                b"FNAM" => {
                    self.mset_recovery_time =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    None
                }
                b"GNAM" => {
                    self.mset_nighttime_max =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    None
                }
                b"HNAM" => {
                    if s.data.len() >= 4 {
                        self.mset_intro = Some(u32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ]));
                    }
                    None
                }
                b"INAM" => {
                    if s.data.len() >= 4 {
                        self.mset_outro = Some(u32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ]));
                    }
                    None
                }
                _ => None,
            };
            if let Some(idx) = track {
                if idx < 6 {
                    // NAM2..NAM7 are cstrings; NAM8..CNAM are float32s.
                    let is_name = matches!(
                        &s.sig,
                        b"NAM2" | b"NAM3" | b"NAM4" | b"NAM5" | b"NAM6" | b"NAM7"
                    );
                    if is_name {
                        self.mset_tracks[idx] = zstring(&s.data);
                    } else if s.data.len() >= 4 {
                        self.mset_levels[idx] =
                            f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    }
                }
            }
        }
    }

    /// Load an `MSTT` (Movable Static) record.
    fn load_mstt(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.mstt_data = 0;
        self.mstt_sound = None;
        self.mstt_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.mstt_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.mstt_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.mstt_model.modt = sub.data.clone(),
                b"MODS" => self.mstt_model.mods = parse_mods(&sub.data),
                b"MODD" => self.mstt_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }

        for sub in raw_subs {
            match &sub.sig {
                b"DATA" if !sub.data.is_empty() => self.mstt_data = sub.data[0],
                b"SNAM" if sub.data.len() >= 4 => {
                    self.mstt_sound = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"DEST" => {
                    self.mstt_destruction = load_destruction(raw_subs);
                    if sub.data.len() >= 5 {
                        self.mstt_dest_health = i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        self.mstt_dest_flags = sub.data[5];
                    }
                }
                _ => {}
            }
        }
    }

    /// Load a `PWAT` (Water) record.
    fn load_pwat(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.pwat_flags = 0;
        self.pwat_water = None;
        self.pwat_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.pwat_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.pwat_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.pwat_model.modt = sub.data.clone(),
                b"MODS" => self.pwat_model.mods = parse_mods(&sub.data),
                b"MODD" => self.pwat_model.modd = sub.data.first().copied(),
                b"DNAM" if sub.data.len() >= 8 => {
                    self.pwat_flags =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.pwat_water = Some(u32::from_le_bytes([
                        sub.data[4],
                        sub.data[5],
                        sub.data[6],
                        sub.data[7],
                    ]));
                }
                _ => {}
            }
        }
    }

    /// Load a `FLST` (Form List) record.
    ///
    /// The `LNAM` subrecords repeat, so the list is walked in file order.
    fn load_flst(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.flst_entries.clear();
        for s in raw_subs {
            if s.sig == *b"LNAM" && s.data.len() >= 4 {
                self.flst_entries.push(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
            }
        }
    }

    /// Load an `LVLI` (Leveled Item) record.
    ///
    /// The leveled list entries are a run of `LVLO` and an optional `COED`,
    /// so they are walked in file order: a `COED` belongs to the `LVLO`
    /// before it.
    /// Load an `LVLN` (Leveled NPC) record.
    /// Load a `MESG` (Message) record.
    ///
    /// The menu buttons are a run of `ITXT` and repeated `CTDA`, so they are
    /// walked in file order: the conditions belong to the button before them.
    fn load_mesg(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.mesg_desc = self.get_string(b"DESC");
        self.mesg_icon = self.get_fid(b"INAM");
        self.mesg_flags = 0;
        self.mesg_display_time = 0;
        self.mesg_buttons.clear();

        for s in raw_subs {
            match &s.sig {
                b"DNAM" if s.data.len() >= 4 => {
                    self.mesg_flags =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"TNAM" if s.data.len() >= 4 => {
                    self.mesg_display_time =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                _ => {}
            }
        }

        let mut current: Option<MessageButton> = None;
        for s in raw_subs {
            match &s.sig {
                b"ITXT" => {
                    if let Some(button) = current.take() {
                        self.mesg_buttons.push(button);
                    }
                    current = Some(MessageButton {
                        text: zstring(&s.data),
                        raw: s.data.clone(),
                        conditions: Vec::new(),
                    });
                }
                b"CTDA" => {
                    if let Some(button) = current.as_mut() {
                        let mut cond = Condition::default();
                        if s.data.len() >= 28 {
                            cond.cond_type = s.data[0];
                            cond.unused = [s.data[1], s.data[2], s.data[3]];
                            cond.comparison_value =
                                u32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                            cond.function =
                                u32::from_le_bytes([s.data[8], s.data[9], s.data[10], s.data[11]]);
                            cond.parameter1 = u32::from_le_bytes([
                                s.data[12], s.data[13], s.data[14], s.data[15],
                            ]);
                            cond.parameter2 = u32::from_le_bytes([
                                s.data[16], s.data[17], s.data[18], s.data[19],
                            ]);
                            cond.run_on = u32::from_le_bytes([
                                s.data[20], s.data[21], s.data[22], s.data[23],
                            ]);
                            cond.reference = Some(u32::from_le_bytes([
                                s.data[24], s.data[25], s.data[26], s.data[27],
                            ]));
                        }
                        button.conditions.push(cond);
                    }
                }
                _ => {}
            }
        }
        if let Some(button) = current {
            self.mesg_buttons.push(button);
        }
    }

    /// Load a `MISC` (Miscellaneous) record.
    fn load_misc(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.misc_value = 0;
        self.misc_weight = 0.0;
        self.misc_icon = self.get_string(b"ICON");
        self.misc_mico = self.get_string(b"MICO");
        self.misc_script = self.get_fid(b"SCRI");
        self.misc_pickup_sound = self.get_fid(b"YNAM");
        self.misc_drop_sound = self.get_fid(b"ZNAM");
        self.misc_random_sound = self.get_fid(b"RNAM");
        self.misc_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.misc_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.misc_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.misc_model.modt = sub.data.clone(),
                b"MODS" => self.misc_model.mods = parse_mods(&sub.data),
                b"MODD" => self.misc_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }

        for sub in raw_subs {
            match &sub.sig {
                b"DATA" if sub.data.len() >= 8 => {
                    self.misc_value =
                        i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.misc_weight =
                        f32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"DEST" => {
                    self.misc_destruction = load_destruction(raw_subs);
                    if sub.data.len() >= 5 {
                        self.misc_dest_health = i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        self.misc_dest_flags = sub.data[5];
                    }
                }
                _ => {}
            }
        }
    }

    /// Load a `SPEL` (Spell) record.
    ///
    /// The effects are a run of `EFID` and `EFIT`, so they are walked in file
    /// order: the parameters belong to the effect before them.
    fn load_spel(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.spel_type = 0;
        self.spel_cost = 0;
        self.spel_level = 0;
        self.spel_flags = 0;
        self.spel_effects.clear();

        for s in raw_subs {
            if s.sig == *b"SPIT" && s.data.len() >= 16 {
                self.spel_type = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                self.spel_cost = u32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                self.spel_level =
                    u32::from_le_bytes([s.data[8], s.data[9], s.data[10], s.data[11]]);
                self.spel_flags = s.data[12];
            }
        }

        let mut current: Option<SpellEffect> = None;
        for s in raw_subs {
            match &s.sig {
                b"EFID" if s.data.len() >= 4 => {
                    if let Some(effect) = current.take() {
                        self.spel_effects.push(effect);
                    }
                    current = Some(SpellEffect {
                        effect: Some(u32::from_le_bytes([
                            s.data[0], s.data[1], s.data[2], s.data[3],
                        ])),
                        parameters: Vec::new(),
                        conditions: Vec::new(),
                    });
                }
                b"EFIT" => {
                    if let Some(effect) = current.as_mut() {
                        effect.parameters = s.data.clone();
                    }
                }
                b"CTDA" => {
                    if let Some(effect) = current.as_mut() {
                        effect.conditions.push(read_condition(&s.data));
                    }
                }
                _ => {}
            }
        }
        if let Some(effect) = current {
            self.spel_effects.push(effect);
        }
    }

    /// Load a `STAT` (Static) record.
    fn load_stat(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.stat_passthrough = -1;
        self.stat_looping_sound = None;
        self.stat_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.stat_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.stat_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.stat_model.modt = sub.data.clone(),
                b"MODS" => self.stat_model.mods = parse_mods(&sub.data),
                b"MODD" => self.stat_model.modd = sub.data.first().copied(),
                b"BRUS" if !sub.data.is_empty() => {
                    self.stat_passthrough = sub.data[0] as i8;
                }
                b"RNAM" if sub.data.len() >= 4 => {
                    self.stat_looping_sound = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                _ => {}
            }
        }
    }

    /// Load a `KEYM` (Key) record.
    fn load_keym(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.keym_value = 0;
        self.keym_weight = 0.0;
        self.keym_icon = self.get_string(b"ICON");
        self.keym_mico = self.get_string(b"MICO");
        self.keym_script = self.get_fid(b"SCRI");
        self.keym_pickup_sound = self.get_fid(b"YNAM");
        self.keym_drop_sound = self.get_fid(b"ZNAM");
        self.keym_random_sound = self.get_fid(b"RNAM");
        self.keym_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.keym_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.keym_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.keym_model.modt = sub.data.clone(),
                b"MODS" => self.keym_model.mods = parse_mods(&sub.data),
                b"MODD" => self.keym_model.modd = sub.data.first().copied(),
                b"DATA" if sub.data.len() >= 8 => {
                    self.keym_value =
                        i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.keym_weight =
                        f32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"DEST" => {
                    self.keym_destruction = load_destruction(raw_subs);
                    if sub.data.len() >= 5 {
                        self.keym_dest_health = i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        self.keym_dest_flags = sub.data[5];
                    }
                }
                _ => {}
            }
        }
    }

    /// Load an `INGR` (Ingredient) record.
    fn load_ingr(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.ingr_equipment_type = -1;
        self.ingr_weight = 0.0;
        self.ingr_effect_value = 0;
        self.ingr_effect_flags = 0;
        self.ingr_icon = self.get_string(b"ICON");
        self.ingr_mico = self.get_string(b"MICO");
        self.ingr_script = self.get_fid(b"SCRI");
        self.ingr_model = ModelData::default();
        self.ingr_effects.clear();

        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.ingr_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.ingr_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.ingr_model.modt = sub.data.clone(),
                b"MODS" => self.ingr_model.mods = parse_mods(&sub.data),
                b"MODD" => self.ingr_model.modd = sub.data.first().copied(),
                b"ETYP" if sub.data.len() >= 4 => {
                    self.ingr_equipment_type =
                        i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"DATA" if sub.data.len() >= 4 => {
                    self.ingr_weight =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"ENIT" if sub.data.len() >= 8 => {
                    self.ingr_effect_value =
                        i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.ingr_effect_flags = sub.data[4];
                }
                _ => {}
            }
        }

        // The effects are a run of EFID and EFIT, so they are walked in file
        // order: the parameters belong to the effect before them.
        let mut current: Option<SpellEffect> = None;
        for sub in raw_subs {
            match &sub.sig {
                b"EFID" if sub.data.len() >= 4 => {
                    if let Some(effect) = current.take() {
                        self.ingr_effects.push(effect);
                    }
                    current = Some(SpellEffect {
                        effect: Some(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ])),
                        parameters: Vec::new(),
                        conditions: Vec::new(),
                    });
                }
                b"EFIT" => {
                    if let Some(effect) = current.as_mut() {
                        effect.parameters = sub.data.clone();
                    }
                }
                b"CTDA" => {
                    if let Some(effect) = current.as_mut() {
                        effect.conditions.push(read_condition(&sub.data));
                    }
                }
                _ => {}
            }
        }
        if let Some(effect) = current {
            self.ingr_effects.push(effect);
        }
    }

    /// Load an `IPCT` (Impact) record.
    fn load_ipct(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.ipct_duration = 0.0;
        self.ipct_orientation = 0;
        self.ipct_angle_threshold = 0.0;
        self.ipct_placement_radius = 0.0;
        self.ipct_sound_level = 0;
        self.ipct_flags = 0;
        self.ipct_texture_set = None;
        self.ipct_sound1 = None;
        self.ipct_sound2 = None;
        self.ipct_model = ModelData::default();
        self.ipct_dodt = None;

        for sub in raw_subs {
            match &sub.sig {
                b"MODL" => self.ipct_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.ipct_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.ipct_model.modt = sub.data.clone(),
                b"MODS" => self.ipct_model.mods = parse_mods(&sub.data),
                b"MODD" => self.ipct_model.modd = sub.data.first().copied(),
                b"DNAM" if sub.data.len() >= 4 => {
                    self.ipct_texture_set = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"SNAM" if sub.data.len() >= 4 => {
                    self.ipct_sound1 = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"NAM1" if sub.data.len() >= 4 => {
                    self.ipct_sound2 = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"DODT" if sub.data.len() >= 36 => {
                    self.ipct_dodt = Some(read_decal_data(&sub.data));
                }
                b"DATA" if sub.data.len() >= 24 => {
                    self.ipct_duration =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.ipct_orientation =
                        u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                    self.ipct_angle_threshold =
                        f32::from_le_bytes([sub.data[8], sub.data[9], sub.data[10], sub.data[11]]);
                    self.ipct_placement_radius = f32::from_le_bytes([
                        sub.data[12],
                        sub.data[13],
                        sub.data[14],
                        sub.data[15],
                    ]);
                    self.ipct_sound_level = u32::from_le_bytes([
                        sub.data[16],
                        sub.data[17],
                        sub.data[18],
                        sub.data[19],
                    ]);
                    self.ipct_flags = u32::from_le_bytes([
                        sub.data[20],
                        sub.data[21],
                        sub.data[22],
                        sub.data[23],
                    ]);
                }
                _ => {}
            }
        }
    }

    /// Load an `IPDS` (Impact Data Set) record.
    fn load_ipds(&mut self) {
        self.ipds_impacts = vec![None; 12];
        if let Some(b) = self.raw_subs.get(b"DATA") {
            for (i, chunk) in b.as_chunks::<4>().0.iter().take(12).enumerate() {
                let fid = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                self.ipds_impacts[i] = if fid == 0 { None } else { Some(fid) };
            }
        }
    }

    /// Load an `RCPE` (Recipe) record.
    ///
    /// The ingredients and the outputs are both runs of `RCIL` and `RCQY`.
    /// The spec does not say which is which, so the first half of the pairs
    /// is read as the ingredients and the second as the outputs.
    fn load_rcpe(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.rcpe_skill = 0;
        self.rcpe_level = 0;
        self.rcpe_category = None;
        self.rcpe_sub_category = None;
        self.rcpe_ingredients.clear();
        self.rcpe_outputs.clear();

        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 16) {
            self.rcpe_skill = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.rcpe_level = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.rcpe_category = Some(u32::from_le_bytes([b[8], b[9], b[10], b[11]]));
            self.rcpe_sub_category = Some(u32::from_le_bytes([b[12], b[13], b[14], b[15]]));
        }

        // The pairs are walked in file order: a quantity belongs to the item
        // before it. `RCIL` is an ingredient and `RCOD` is an output, so the
        // two lists are told apart by their signature rather than by their
        // position.
        let mut current: Option<(bool, RecipeItem)> = None;
        for sub in raw_subs {
            match &sub.sig {
                b"RCIL" | b"RCOD" if sub.data.len() >= 4 => {
                    if let Some((is_output, item)) = current.take() {
                        if is_output {
                            self.rcpe_outputs.push(item);
                        } else {
                            self.rcpe_ingredients.push(item);
                        }
                    }
                    current = Some((
                        &sub.sig == b"RCOD",
                        RecipeItem {
                            item: Some(u32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ])),
                            quantity: 0,
                        },
                    ));
                }
                b"RCQY" if sub.data.len() >= 4 => {
                    if let Some((_, item)) = current.as_mut() {
                        item.quantity = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                    }
                }
                _ => {}
            }
        }
        if let Some((is_output, item)) = current {
            if is_output {
                self.rcpe_outputs.push(item);
            } else {
                self.rcpe_ingredients.push(item);
            }
        }
        self.rcpe_conditions = load_conditions(raw_subs);
    }

    /// Load a `SCPT` (Script) record.
    fn load_scpt(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.scpt_header = ScriptHeader::default();
        self.scpt_compiled.clear();
        self.scpt_source.clear();
        self.scpt_variables.clear();

        for sub in raw_subs {
            match &sub.sig {
                b"SCHR" if sub.data.len() >= 20 => {
                    let b = &sub.data;
                    self.scpt_header.script_type = u16::from_le_bytes([b[12], b[13]]) as u32;
                    self.scpt_header.flags = u16::from_le_bytes([b[14], b[15]]);
                    self.scpt_header.version = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
                    self.scpt_header.unknown = i32::from_le_bytes([b[8], b[9], b[10], b[11]]);
                }
                b"SCDA" => self.scpt_compiled = sub.data.clone(),
                b"SCTX" => self.scpt_source = String::from_utf8_lossy(&sub.data).into_owned(),
                b"SLSD" if sub.data.len() >= 24 => {
                    self.scpt_variables.push(ScriptVariable {
                        index: u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]),
                        flags: sub.data[16],
                        name: String::new(),
                        refs: Vec::new(),
                        raw: sub.data.clone(),
                    });
                }
                b"SCVR" => {
                    if let Some(var) = self.scpt_variables.last_mut() {
                        var.name = zstring(&sub.data);
                    }
                }
                _ => {}
            }
        }
    }

    /// Load a `PMIS` (Placed Missile) record.
    ///
    /// The record's format is the same as a placed grenade's.
    fn load_pmis(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.pmis_base = None;
        self.pmis_encounter_zone = None;
        self.pmis_idle_time = 0.0;
        self.pmis_idle = None;
        self.pmis_topic = None;
        self.pmis_owner = None;
        self.pmis_ragdoll.clear();
        self.pmis_ragdoll_biped.clear();
        self.pmis_scale = 1.0;
        self.pmis_position = [0.0; 3];
        self.pmis_rotation = [0.0; 3];
        self.pmis_bounding_radius = 0.0;
        self.pmis_flags = 0;
        self.pmis_unknown = [0; 6];
        for sub in raw_subs {
            match &sub.sig {
                b"NAME" if sub.data.len() >= 4 => {
                    self.pmis_base = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"XEZN" if sub.data.len() >= 4 => {
                    self.pmis_encounter_zone = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"XPRD" if sub.data.len() >= 4 => {
                    self.pmis_idle_time =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"INAM" if sub.data.len() >= 4 => {
                    self.pmis_idle = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"TNAM" if sub.data.len() >= 4 => {
                    self.pmis_topic = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"XOWN" if sub.data.len() >= 4 => {
                    self.pmis_owner = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"XRGD" => self.pmis_ragdoll = sub.data.clone(),
                b"XRGB" => self.pmis_ragdoll_biped = sub.data.clone(),
                b"XSCL" if sub.data.len() >= 4 => {
                    self.pmis_scale =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"DATA" if sub.data.len() >= 34 => {
                    for i in 0..3 {
                        let at = i * 4;
                        self.pmis_position[i] = f32::from_le_bytes([
                            sub.data[at],
                            sub.data[at + 1],
                            sub.data[at + 2],
                            sub.data[at + 3],
                        ]);
                        let at = 12 + i * 4;
                        self.pmis_rotation[i] = f32::from_le_bytes([
                            sub.data[at],
                            sub.data[at + 1],
                            sub.data[at + 2],
                            sub.data[at + 3],
                        ]);
                    }
                    self.pmis_bounding_radius = f32::from_le_bytes([
                        sub.data[24],
                        sub.data[25],
                        sub.data[26],
                        sub.data[27],
                    ]);
                    self.pmis_flags = u32::from_le_bytes([
                        sub.data[28],
                        sub.data[29],
                        sub.data[30],
                        sub.data[31],
                    ]);
                    self.pmis_unknown.copy_from_slice(&sub.data[32..38]);
                }
                _ => {}
            }
        }
    }

    /// Load a `COBJ` (Constructible Object) record.
    fn load_cobj(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.cobj_value = 0;
        self.cobj_weight = 0.0;
        self.cobj_model = ModelData::default();
        for sub in raw_subs {
            match &sub.sig {
                b"DATA" if sub.data.len() >= 8 => {
                    self.cobj_value =
                        i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    self.cobj_weight =
                        f32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"MODL" => self.cobj_model.modl = zstring(&sub.data),
                b"MODB" if sub.data.len() >= 4 => {
                    self.cobj_model.modb =
                        Some([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"MODT" => self.cobj_model.modt = sub.data.clone(),
                b"MODS" => self.cobj_model.mods = parse_mods(&sub.data),
                b"MODD" => self.cobj_model.modd = sub.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load an `IMAD` (Image Space Adapter) record.
    fn load_imad(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.imad_header = ImadHeader::default();
        self.imad_curves.clear();

        for sub in raw_subs {
            if sub.sig == *b"DNAM" {
                self.imad_header = read_imad_header(&sub.data);
                continue;
            }
            let Some((name, is_color)) = imad_curve_for(&sub.sig) else {
                continue;
            };
            self.imad_curves.push(ImadCurve {
                sig: sub.sig,
                name,
                is_color,
                keys: read_imad_keys(&sub.data, is_color),
            });
        }
    }

    /// Load an `EFSH` (Effect Shader) record.
    fn load_efsh(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.efsh_data = EfshData::default();
        self.efsh_icon = String::new();
        self.efsh_ico2 = String::new();
        self.efsh_nam7 = String::new();
        for sub in raw_subs {
            match &sub.sig {
                b"DATA" => self.efsh_data = read_efsh_data(&sub.data),
                b"ICON" => self.efsh_icon = zstring(&sub.data),
                b"ICO2" => self.efsh_ico2 = zstring(&sub.data),
                b"NAM7" => self.efsh_nam7 = zstring(&sub.data),
                _ => {}
            }
        }
    }

    /// Load a `REGN` (Region) record.
    ///
    /// The region data is a run of `RDAT` headers, each followed by the
    /// entry's own subrecords. The entries are walked in file order, because
    /// `raw_subs` holds one value per signature and would collapse the
    /// entries onto the last one.
    fn load_regn(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.regn_map_color = [0; 4];
        self.regn_worldspace = None;
        self.regn_areas.clear();
        self.regn_entries.clear();

        // The region area is a run of RPLI (the edge fall-off) and RPLD (the
        // point list), and both repeat.
        let mut pending_falloff: Option<u32> = None;
        let mut cur: Option<RegionEntry> = None;
        for sub in raw_subs {
            match &sub.sig {
                b"RCLR" if sub.data.len() >= 4 => {
                    self.regn_map_color = [sub.data[0], sub.data[1], sub.data[2], sub.data[3]];
                }
                b"WNAM" if sub.data.len() >= 4 => {
                    self.regn_worldspace = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"RPLI" if sub.data.len() >= 4 => {
                    let v =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    // The fall-off belongs to the point list that follows it.
                    pending_falloff = Some(v);
                }
                b"RPLD" => {
                    let mut points = Vec::with_capacity(sub.data.len() / 8);
                    for chunk in sub.data.as_chunks::<8>().0 {
                        points.push([
                            f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                            f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
                        ]);
                    }
                    self.regn_areas.push(RegionArea {
                        edge_falloff: pending_falloff.take().unwrap_or(0),
                        points,
                    });
                }
                b"RDAT" => {
                    if let Some(entry) = cur.take() {
                        self.regn_entries.push(entry);
                    }
                    let mut entry = RegionEntry::default();
                    if sub.data.len() >= 4 {
                        entry.kind = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                    }
                    if sub.data.len() >= 5 {
                        entry.flags = sub.data[4];
                    }
                    if sub.data.len() >= 6 {
                        entry.priority = sub.data[5];
                    }
                    if sub.data.len() >= 8 {
                        entry.unused = [sub.data[6], sub.data[7]];
                    }
                    cur = Some(entry);
                }
                b"RDMP" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.map_name = Some(zstring(&sub.data));
                    }
                }
                b"ICON" => {
                    // The icon follows the entry it belongs to, so it
                    // belongs to the entry being read.
                    if let Some(entry) = cur.as_mut() {
                        entry.icon = Some(zstring(&sub.data));
                    }
                }
                b"RDMD" if sub.data.len() >= 4 => {
                    if let Some(entry) = cur.as_mut() {
                        entry.music_type = Some(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                }
                b"RDMO" if sub.data.len() >= 4 => {
                    if let Some(entry) = cur.as_mut() {
                        entry.music = Some(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                }
                b"RDSI" if sub.data.len() >= 4 => {
                    if let Some(entry) = cur.as_mut() {
                        entry.incidental_media = Some(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                }
                b"RDSB" if sub.data.len() >= 4 => {
                    if let Some(entry) = cur.as_mut() {
                        entry.battle_media.push(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                }
                b"RDOT" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.had_empty_rdot = sub.data.is_empty();
                        entry.objects = read_region_objects(&sub.data);
                    }
                }
                b"RDGS" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.grasses = read_region_grasses(&sub.data);
                    }
                }
                b"RDSD" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.sounds = read_region_sounds(&sub.data);
                    }
                }
                b"RDWT" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.weather = read_region_weather(&sub.data);
                    }
                }
                b"RDID" => {
                    if let Some(entry) = cur.as_mut() {
                        entry.imposters = sub
                            .data
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                    }
                }
                _ => {}
            }
        }
        if let Some(entry) = cur.take() {
            self.regn_entries.push(entry);
        }
    }

    /// Load a `PACK` (Package) record.
    ///
    /// The record carries three embedded scripts, each introduced by a marker
    /// subrecord and each followed by an idle, the shared Script collection
    /// and a topic. The scripts are walked in file order, because `raw_subs`
    /// holds one value per signature and would collapse the three onto the
    /// last one.
    fn load_pack(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.pack_general_flags = 0;
        self.pack_type = 0;
        self.pack_unused = 0;
        self.pack_fallout_behavior_flags = 0;
        self.pack_type_specific_flags = None;
        self.pack_unused2 = [0; 2];
        self.pack_month = -1;
        self.pack_day_of_week = -1;
        self.pack_date = 0;
        self.pack_time = -1;
        self.pack_duration = 0;
        self.pack_conditions = load_conditions(raw_subs);
        self.pack_location = PackageLocation::default();
        self.pack_location2 = PackageLocation::default();
        self.pack_target = PackageTarget::default();
        self.pack_target2 = PackageTarget::default();
        self.pack_idle_flags = 0;
        self.pack_idle_count = 0;
        self.pack_idle_count_unused = [0; 3];
        self.pack_idle_timer = 0.0;
        self.pack_idle_animations.clear();
        self.pack_idle_unused = [0; 4];
        self.pack_combat_style = None;
        self.pack_eat_marker = false;
        self.pack_escort_distance = None;
        self.pack_follow_radius = None;
        self.pack_patrol_flags = None;
        self.pack_weapon = PackageWeapon::default();
        self.pack_use_item_marker = false;
        self.pack_ambush_marker = false;
        self.pack_dialog = PackageDialog::default();
        self.pack_scripts.clear();

        let mut cur: Option<PackageScript> = None;
        for sub in raw_subs {
            match &sub.sig {
                b"PKDT" => {
                    let b = &sub.data;
                    if b.len() >= 4 {
                        self.pack_general_flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    }
                    if b.len() >= 5 {
                        self.pack_type = b[4];
                    }
                    if b.len() >= 6 {
                        self.pack_unused = b[5];
                    }
                    if b.len() >= 8 {
                        self.pack_fallout_behavior_flags = u16::from_le_bytes([b[6], b[7]]);
                    }
                    if b.len() >= 10 {
                        self.pack_type_specific_flags = Some(u16::from_le_bytes([b[8], b[9]]));
                    }
                    if b.len() >= 12 {
                        self.pack_unused2 = [b[10], b[11]];
                    }
                }
                b"PSDT" => {
                    let b = &sub.data;
                    if !b.is_empty() {
                        self.pack_month = b[0] as i8;
                    }
                    if b.len() >= 2 {
                        self.pack_day_of_week = b[1] as i8;
                    }
                    if b.len() >= 3 {
                        self.pack_date = b[2];
                    }
                    if b.len() >= 4 {
                        self.pack_time = b[3] as i8;
                    }
                    if b.len() >= 8 {
                        self.pack_duration = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                    }
                }
                b"PLDT" => self.pack_location = read_package_location(&sub.data),
                b"PLD2" => self.pack_location2 = read_package_location(&sub.data),
                b"PTDT" => self.pack_target = read_package_target(&sub.data),
                b"PTD2" => self.pack_target2 = read_package_target(&sub.data),
                b"IDLF" if !sub.data.is_empty() => self.pack_idle_flags = sub.data[0],
                b"IDLC" => {
                    let b = &sub.data;
                    if !b.is_empty() {
                        self.pack_idle_count = b[0];
                    }
                    if b.len() >= 4 {
                        self.pack_idle_count_unused = [b[1], b[2], b[3]];
                    }
                }
                b"IDLT" if sub.data.len() >= 4 => {
                    self.pack_idle_timer =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                }
                b"IDLA" => {
                    self.pack_idle_animations = sub
                        .data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect();
                }
                b"IDLB" if sub.data.len() >= 4 => {
                    self.pack_idle_unused = [sub.data[0], sub.data[1], sub.data[2], sub.data[3]];
                }
                b"CNAM" if sub.data.len() >= 4 => {
                    self.pack_combat_style = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"PKED" => self.pack_eat_marker = true,
                b"PKE2" if sub.data.len() >= 4 => {
                    self.pack_escort_distance = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"PKFD" if sub.data.len() >= 4 => {
                    self.pack_follow_radius = Some(f32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                }
                b"PKPT" => {
                    // The record is two bytes: the repeatable flag and one
                    // unnamed byte.
                    if sub.data.len() >= 2 {
                        self.pack_patrol_flags =
                            Some(u16::from_le_bytes([sub.data[0], sub.data[1]]));
                    } else if !sub.data.is_empty() {
                        self.pack_patrol_flags = Some(sub.data[0] as u16);
                    }
                }
                b"PKW3" => self.pack_weapon = read_package_weapon(&sub.data),
                b"PUID" => self.pack_use_item_marker = true,
                b"PKAM" => self.pack_ambush_marker = true,
                b"PKDD" => self.pack_dialog = read_package_dialog(&sub.data),
                b"POBA" | b"POEA" | b"POCA" => {
                    // The marker starts a new embedded script.
                    if let Some(script) = cur.take() {
                        self.pack_scripts.push(script);
                    }
                    cur = Some(PackageScript {
                        marker: sub.sig,
                        ..PackageScript::default()
                    });
                }
                b"INAM" if sub.data.len() >= 4 => {
                    let idle =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    if let Some(script) = cur.as_mut() {
                        script.idle = Some(idle);
                    }
                }
                b"TNAM" if sub.data.len() >= 4 => {
                    let topic =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    if let Some(script) = cur.as_mut() {
                        script.topic = Some(topic);
                    }
                }
                b"SCHR" if sub.data.len() >= 20 => {
                    let b = &sub.data;
                    let header = ScriptHeader {
                        script_type: u16::from_le_bytes([b[12], b[13]]) as u32,
                        flags: u16::from_le_bytes([b[14], b[15]]),
                        version: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
                        unknown: i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
                    };
                    let header_raw = sub.data.clone();
                    if let Some(script) = cur.as_mut() {
                        script.header = header;
                        script.header_raw = header_raw;
                    }
                }
                b"SCDA" => {
                    if let Some(script) = cur.as_mut() {
                        script.compiled = sub.data.clone();
                    }
                }
                b"SCTX" => {
                    if let Some(script) = cur.as_mut() {
                        script.source = String::from_utf8_lossy(&sub.data).into_owned();
                    }
                }
                b"SLSD" if sub.data.len() >= 24 => {
                    if let Some(script) = cur.as_mut() {
                        script.variables.push(ScriptVariable {
                            index: u32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]),
                            flags: sub.data[16],
                            name: String::new(),
                            refs: Vec::new(),
                            raw: sub.data.clone(),
                        });
                    }
                }
                b"SCVR" => {
                    if let Some(script) = cur.as_mut() {
                        if let Some(var) = script.variables.last_mut() {
                            var.name = zstring(&sub.data);
                        }
                    }
                }
                b"SCRO" if sub.data.len() >= 4 => {
                    if let Some(script) = cur.as_mut() {
                        script.references.push(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                }
                b"SCRV" if sub.data.len() >= 4 => {
                    // The reference names the variable above it.
                    let index =
                        u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    if let Some(script) = cur.as_mut() {
                        if let Some(var) = script.variables.iter_mut().find(|v| v.index == index) {
                            var.refs.push(index);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(script) = cur.take() {
            self.pack_scripts.push(script);
        }
    }

    /// Load a `TES4` (Plugin Header) record.
    fn load_tes4(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.tes4_version = 0.0;
        self.tes4_num_records = 0;
        self.tes4_next_object_id = 0;
        self.tes4_author = self.get_string(b"CNAM");
        self.tes4_description = self.get_string(b"SNAM");
        self.tes4_masters.clear();

        if let Some(b) = self.raw_subs.get(b"HEDR").filter(|b| b.len() >= 12) {
            self.tes4_version = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.tes4_num_records = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.tes4_next_object_id = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        }

        // The masters are a run of MAST and SSIG, so they are walked in file
        // order: the FormID belongs to the master before it.
        let mut current: Option<Tes4Master> = None;
        for sub in raw_subs {
            match &sub.sig {
                b"MAST" => {
                    if let Some(master) = current.take() {
                        self.tes4_masters.push(master);
                    }
                    current = Some(Tes4Master {
                        filename: zstring(&sub.data),
                        form_id: 0,
                    });
                }
                b"SSIG" if sub.data.len() >= 4 => {
                    if let Some(master) = current.as_mut() {
                        master.form_id = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                    }
                }
                _ => {}
            }
        }
        if let Some(master) = current {
            self.tes4_masters.push(master);
        }
    }

    /// Load a `CHAL` (Challenge) record.
    ///
    /// The record is `EDID`, `FULL`, `ICON`, `MICO`, `SCRI`, `DESC`, a
    /// twenty-four-byte `DATA`, and the two reference values `SNAM` and
    /// `XNAM`. The meaning of `DATA`'s three value fields depends on the
    /// type, so they are kept as raw bytes.
    fn load_chal(&mut self) {
        self.chal_type = 0;
        self.chal_threshold = 0;
        self.chal_flags = 0;
        self.chal_interval = 0;
        self.chal_value1 = [0; 2];
        self.chal_value2 = [0; 2];
        self.chal_value3 = [0; 4];
        self.chal_snam = self.get_fid(b"SNAM");
        self.chal_xnam = self.get_fid(b"XNAM");
        self.chal_icon = self.get_string(b"ICON");
        self.chal_mico = self.get_string(b"MICO");
        self.chal_desc = self.get_string(b"DESC");

        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 24) {
            self.chal_type = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.chal_threshold = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.chal_flags = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            self.chal_interval = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            self.chal_value1 = [b[16], b[17]];
            self.chal_value2 = [b[18], b[19]];
            self.chal_value3 = [b[20], b[21], b[22], b[23]];
        }
    }

    /// Load a `CPTH` (Camera Path) record.
    ///
    /// The record's `CTDA` conditions and its `SNAM` camera shots are both
    /// *repeated* subrecords, so they are read in file order and kept as
    /// lists.
    fn load_cpth(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.cpth_camera_zoom = 0;
        self.cpth_parent = None;
        self.cpth_previous = None;
        self.cpth_shots.clear();
        self.cpth_conditions.clear();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if !s.data.is_empty() => {
                    self.cpth_camera_zoom = s.data[0];
                }
                b"ANAM" if s.data.len() >= 8 => {
                    self.cpth_parent = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                    self.cpth_previous = Some(u32::from_le_bytes([
                        s.data[4], s.data[5], s.data[6], s.data[7],
                    ]));
                }
                b"SNAM" if s.data.len() >= 4 => {
                    self.cpth_shots.push(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"CTDA" => {}
                _ => {}
            }
        }
        self.cpth_conditions = load_conditions(raw_subs);
    }

    /// Load a `CSNO` (Casino) record.
    ///
    /// The record's chip models (`MODL` repeated seven times, then `MOD2`,
    /// `MOD3` and `MOD4`), its slot reel textures (`ICON` repeated) and its
    /// blackjack deck textures (`ICO2` repeated) are all *repeated*
    /// subrecords, so each is read in file order and kept as a list.
    fn load_csno(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.csno_decks_shuffle = 0.0;
        self.csno_payout_ratio = 0.0;
        self.csno_reel_stops.clear();
        self.csno_number_of_decks = 0;
        self.csno_max_winnings = 0;
        self.csno_currency = None;
        self.csno_winnings_quest = None;
        self.csno_flags = 0;
        self.csno_models.clear();
        self.csno_icons.clear();
        self.csno_deck_icons.clear();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if s.data.len() >= 56 => {
                    self.csno_decks_shuffle =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.csno_payout_ratio =
                        f32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                    for i in 0..7 {
                        let at = 8 + i * 4;
                        self.csno_reel_stops.push(u32::from_le_bytes([
                            s.data[at],
                            s.data[at + 1],
                            s.data[at + 2],
                            s.data[at + 3],
                        ]));
                    }
                    self.csno_number_of_decks =
                        u32::from_le_bytes([s.data[36], s.data[37], s.data[38], s.data[39]]);
                    self.csno_max_winnings =
                        u32::from_le_bytes([s.data[40], s.data[41], s.data[42], s.data[43]]);
                    self.csno_currency = Some(u32::from_le_bytes([
                        s.data[44], s.data[45], s.data[46], s.data[47],
                    ]));
                    self.csno_winnings_quest = Some(u32::from_le_bytes([
                        s.data[48], s.data[49], s.data[50], s.data[51],
                    ]));
                    self.csno_flags =
                        u32::from_le_bytes([s.data[52], s.data[53], s.data[54], s.data[55]]);
                }
                b"MODL" | b"MOD2" | b"MOD3" | b"MOD4" => {
                    self.csno_models.push(zstring(&s.data));
                }
                b"ICON" => {
                    self.csno_icons.push(zstring(&s.data));
                }
                b"ICO2" => {
                    self.csno_deck_icons.push(zstring(&s.data));
                }
                _ => {}
            }
        }
    }

    /// Load a `DEBR` (Debris) record.
    ///
    /// The record's collection is a run of `DATA` and `MODT` per debris
    /// model, so the entries are walked in file order rather than read out of
    /// `raw_subs`, which holds one value per signature.
    fn load_debr(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.debr_models.clear();
        let mut cur: Option<DebrisModel> = None;
        for s in raw_subs {
            match &s.sig {
                b"DATA" => {
                    if let Some(model) = cur.take() {
                        self.debr_models.push(model);
                    }
                    let mut model = DebrisModel::default();
                    if !s.data.is_empty() {
                        model.percentage = s.data[0];
                    }
                    // The model filename is a NUL-terminated string that
                    // follows the percentage byte, and the flags byte is
                    // the last one, after the terminator.
                    if s.data.len() > 1 {
                        let name_end = s.data[1..]
                            .iter()
                            .position(|&b| b == 0)
                            .map(|i| i + 1)
                            .unwrap_or(s.data.len() - 1);
                        model.model = zstring(&s.data[1..name_end]);
                        if s.data.len() > name_end + 1 {
                            model.flags = s.data[name_end + 1];
                        }
                    }
                    cur = Some(model);
                }
                b"MODT" => {
                    if let Some(model) = cur.as_mut() {
                        model.texture_hashes = s.data.clone();
                    }
                }
                _ => {}
            }
        }
        if let Some(model) = cur.take() {
            self.debr_models.push(model);
        }
    }

    /// Load a `CAMS` (Camera Shot) record.
    ///
    /// The record is `EDID`, the model data collection, a forty-byte `DATA`
    /// holding the action, location and target enums, a flag word and six
    /// floats, and `MNAM` naming the image space modifier.
    ///
    /// `DATA` is forty bytes on 272 of the game's 276 records and thirty-six
    /// on the rest, so the length is kept and the fields the short form does
    /// not have are left at their defaults.
    fn load_cams(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.cams_action = 0;
        self.cams_location = 0;
        self.cams_target = 0;
        self.cams_flags = 0;
        self.cams_player_time_mult = 0.0;
        self.cams_target_time_mult = 0.0;
        self.cams_global_time_mult = 0.0;
        self.cams_max_time = 0.0;
        self.cams_min_time = 0.0;
        self.cams_target_percent = 0.0;
        self.cams_image_space_modifier = self.get_fid(b"MNAM");
        self.cams_model = ModelData::default();

        for s in raw_subs {
            match &s.sig {
                b"DATA" => {
                    if s.data.len() >= 4 {
                        self.cams_action =
                            u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    }
                    if s.data.len() >= 8 {
                        self.cams_location =
                            u32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                    }
                    if s.data.len() >= 12 {
                        self.cams_target =
                            u32::from_le_bytes([s.data[8], s.data[9], s.data[10], s.data[11]]);
                    }
                    if s.data.len() >= 16 {
                        self.cams_flags =
                            u32::from_le_bytes([s.data[12], s.data[13], s.data[14], s.data[15]]);
                    }
                    if s.data.len() >= 20 {
                        self.cams_player_time_mult =
                            f32::from_le_bytes([s.data[16], s.data[17], s.data[18], s.data[19]]);
                    }
                    if s.data.len() >= 24 {
                        self.cams_target_time_mult =
                            f32::from_le_bytes([s.data[20], s.data[21], s.data[22], s.data[23]]);
                    }
                    if s.data.len() >= 28 {
                        self.cams_global_time_mult =
                            f32::from_le_bytes([s.data[24], s.data[25], s.data[26], s.data[27]]);
                    }
                    if s.data.len() >= 32 {
                        self.cams_max_time =
                            f32::from_le_bytes([s.data[28], s.data[29], s.data[30], s.data[31]]);
                    }
                    if s.data.len() >= 36 {
                        self.cams_min_time =
                            f32::from_le_bytes([s.data[32], s.data[33], s.data[34], s.data[35]]);
                    }
                    if s.data.len() >= 40 {
                        self.cams_target_percent =
                            f32::from_le_bytes([s.data[36], s.data[37], s.data[38], s.data[39]]);
                    }
                }
                b"MODL" => self.cams_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.cams_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.cams_model.modt = s.data.clone(),
                b"MODS" => self.cams_model.mods = parse_mods(&s.data),
                b"MODD" => self.cams_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load a `CCRD` (Card) record.
    ///
    /// The two `INTV` values — the card suit and the card value — are a
    /// *repeated* subrecord, so they are read in file order and kept as a
    /// list. A per-signature read would collapse the pair onto the last one.
    fn load_ccrd(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.ccrd_intv.clear();
        self.ccrd_value = 0;
        self.ccrd_icon.clear();
        self.ccrd_mico.clear();
        self.ccrd_face.clear();
        self.ccrd_back.clear();
        self.ccrd_pickup_sound = None;
        self.ccrd_drop_sound = None;
        self.ccrd_model = ModelData::default();

        for s in raw_subs {
            match &s.sig {
                b"INTV" if s.data.len() >= 4 => {
                    self.ccrd_intv.push(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"DATA" if s.data.len() >= 4 => {
                    self.ccrd_value =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"ICON" => self.ccrd_icon = zstring(&s.data),
                b"MICO" => self.ccrd_mico = zstring(&s.data),
                b"TX00" => self.ccrd_face = zstring(&s.data),
                b"TX01" => self.ccrd_back = zstring(&s.data),
                b"YNAM" if s.data.len() >= 4 => {
                    self.ccrd_pickup_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"ZNAM" if s.data.len() >= 4 => {
                    self.ccrd_drop_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"MODL" => self.ccrd_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.ccrd_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.ccrd_model.modt = s.data.clone(),
                b"MODS" => self.ccrd_model.mods = parse_mods(&s.data),
                b"MODD" => self.ccrd_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load a `CDCK` (Caravan Deck) record.
    ///
    /// The deck's `CARD` subrecords are *repeated*, so they are read in file
    /// order and kept as a list.
    fn load_cdck(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.cdck_cards.clear();
        self.cdck_data = 0;
        for s in raw_subs {
            match &s.sig {
                b"CARD" if s.data.len() >= 4 => {
                    self.cdck_cards.push(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"DATA" if s.data.len() >= 4 => {
                    self.cdck_data =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                _ => {}
            }
        }
    }

    /// Load a `CHIP` (Casino Chip) record.
    ///
    /// The record carries the model data collection, the two icons, the two
    /// sounds and the shared Destruction Data collection.
    fn load_chip(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.chip_icon.clear();
        self.chip_mico.clear();
        self.chip_pickup_sound = None;
        self.chip_drop_sound = None;
        self.chip_model = ModelData::default();
        self.chip_dest_health = 0;
        self.chip_dest_flags = 0;

        for s in raw_subs {
            match &s.sig {
                b"ICON" => self.chip_icon = zstring(&s.data),
                b"MICO" => self.chip_mico = zstring(&s.data),
                b"YNAM" if s.data.len() >= 4 => {
                    self.chip_pickup_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"ZNAM" if s.data.len() >= 4 => {
                    self.chip_drop_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"DEST" if s.data.len() >= 8 => {
                    self.chip_dest_health =
                        i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.chip_dest_flags = s.data[5];
                }
                b"MODL" => self.chip_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.chip_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.chip_model.modt = s.data.clone(),
                b"MODS" => self.chip_model.mods = parse_mods(&s.data),
                b"MODD" => self.chip_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
        self.chip_destruction = load_destruction(raw_subs);
    }

    /// Load a `CMNY` (Money) record.
    fn load_cmny(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.cmny_value = 0;
        self.cmny_icon.clear();
        self.cmny_mico.clear();
        self.cmny_pickup_sound = None;
        self.cmny_drop_sound = None;
        self.cmny_model = ModelData::default();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if s.data.len() >= 4 => {
                    self.cmny_value =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"ICON" => self.cmny_icon = zstring(&s.data),
                b"MICO" => self.cmny_mico = zstring(&s.data),
                b"YNAM" if s.data.len() >= 4 => {
                    self.cmny_pickup_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"ZNAM" if s.data.len() >= 4 => {
                    self.cmny_drop_sound = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"MODL" => self.cmny_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.cmny_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.cmny_model.modt = s.data.clone(),
                b"MODS" => self.cmny_model.mods = parse_mods(&s.data),
                b"MODD" => self.cmny_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load an `ARMA` (Armor Addon) record.
    ///
    /// The record carries four model data instances — male biped, male world,
    /// female biped and female world — so the model is kept as a list of four
    /// collections rather than one. `MODB` is present only in the original
    /// instance, and `MODD` only in the original and the third, where it is
    /// spelled `MOSD`.
    ///
    /// `DNAM` is variable-length: 115 of the game's 131 records carry four
    /// bytes and the rest twelve. The length is kept, so a save that changes
    /// nothing is byte-identical and a short record is not grown fields it
    /// never had.
    fn load_arma(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.arma_val = 0;
        self.arma_hp = 0;
        self.arma_weight = 0.0;
        self.arma_dr = 0;
        self.arma_dnam_flags = 0;
        self.arma_dt = 0.0;
        self.arma_dnam_len = 0;
        self.arma_biped_flags = 0;
        self.arma_bmdt_general = 0;
        self.arma_equip_type = -1;
        self.arma_icon.clear();
        self.arma_mico.clear();
        self.arma_ico2.clear();
        self.arma_mic2.clear();

        self.arma_models = (0..4)
            .map(|instance| ModelData {
                instance,
                ..ModelData::default()
            })
            .collect();

        for s in raw_subs {
            match &s.sig {
                b"DATA" if s.data.len() >= 12 => {
                    self.arma_val =
                        i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.arma_hp = i32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                    self.arma_weight =
                        f32::from_le_bytes([s.data[8], s.data[9], s.data[10], s.data[11]]);
                }
                b"DNAM" => {
                    self.arma_dnam_len = s.data.len();
                    if s.data.len() >= 2 {
                        self.arma_dr = i16::from_le_bytes([s.data[0], s.data[1]]);
                    }
                    if s.data.len() >= 4 {
                        self.arma_dnam_flags = u16::from_le_bytes([s.data[2], s.data[3]]);
                    }
                    if s.data.len() >= 8 {
                        self.arma_dt =
                            f32::from_le_bytes([s.data[4], s.data[5], s.data[6], s.data[7]]);
                    }
                }
                b"BMDT" if s.data.len() >= 8 => {
                    self.arma_biped_flags =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    self.arma_bmdt_general = s.data[4];
                }
                b"ETYP" if s.data.len() >= 4 => {
                    self.arma_equip_type =
                        i32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"ICON" => self.arma_icon = zstring(&s.data),
                b"MICO" => self.arma_mico = zstring(&s.data),
                b"ICO2" => self.arma_ico2 = zstring(&s.data),
                b"MIC2" => self.arma_mic2 = zstring(&s.data),
                _ => {
                    // One of the four model data instances. The field each
                    // signature feeds is named by the signature itself, not by
                    // the instance: the third instance's model path is `MOD3`,
                    // not `MODL`.
                    if let Some(instance) = arma_model_instance(&s.sig) {
                        if let Some(model) = self.arma_models.get_mut(instance) {
                            match &s.sig {
                                b"MODL" | b"MOD2" | b"MOD3" | b"MOD4" => {
                                    model.modl = zstring(&s.data);
                                }
                                b"MODB" if s.data.len() >= 4 => {
                                    model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                                }
                                b"MODT" | b"MO2T" | b"MO3T" | b"MO4T" => {
                                    model.modt = s.data.clone();
                                }
                                b"MODS" | b"MO2S" | b"MO3S" | b"MO4S" => {
                                    model.mods = parse_mods(&s.data);
                                }
                                b"MODD" | b"MOSD" => model.modd = s.data.first().copied(),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    /// Load an `ASPC` (Ambient Sound) record.
    ///
    /// The five `SNAM` sound loops — dawn, afternoon, dusk, night and walla —
    /// are *repeated* subrecords, so they cannot be read out of `raw_subs`,
    /// which holds one value per signature and would collapse them onto the
    /// last one. The subrecords are walked in file order instead.
    fn load_aspc(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.aspc_sounds.clear();
        self.aspc_walla_trigger_count = 0;
        self.aspc_region = None;
        self.aspc_environment = 0;
        self.aspc_is_interior = 0;

        for s in raw_subs {
            match &s.sig {
                b"SNAM" if s.data.len() >= 4 => {
                    self.aspc_sounds.push(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"WNAM" if s.data.len() >= 4 => {
                    self.aspc_walla_trigger_count =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"RDAT" if s.data.len() >= 4 => {
                    self.aspc_region = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                b"ANAM" if s.data.len() >= 4 => {
                    self.aspc_environment =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"INAM" if s.data.len() >= 4 => {
                    self.aspc_is_interior =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                _ => {}
            }
        }
    }

    /// Load an `AMEF` (Ammo Effect) record.
    ///
    /// The record is `EDID`, `FULL` and a twelve-byte `DATA` holding a type
    /// enum, an operation enum and a float32 value. The enums are kept as
    /// their raw numbers, so a value outside the table stays editable rather
    /// than being clamped to a known one.
    fn load_amef(&mut self) {
        self.amef_type = 0;
        self.amef_operation = 0;
        self.amef_value = 0.0;
        if let Some(b) = self.raw_subs.get(b"DATA").filter(|b| b.len() >= 12) {
            self.amef_type = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            self.amef_operation = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            self.amef_value = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        }
    }

    /// Load an `ANIO` (Animation Object) record.
    ///
    /// The record is `EDID`, the model data collection (`MODL` and, where the
    /// game writes it, `MODT`), and a `DATA` naming the `IDLE` record the
    /// animation plays.
    fn load_anio(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.anio_animation = self.get_fid(b"DATA");
        self.anio_model = ModelData::default();
        for s in raw_subs {
            match &s.sig {
                b"MODL" => self.anio_model.modl = zstring(&s.data),
                b"MODB" if s.data.len() >= 4 => {
                    self.anio_model.modb = Some([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"MODT" => self.anio_model.modt = s.data.clone(),
                b"MODS" => self.anio_model.mods = parse_mods(&s.data),
                b"MODD" => self.anio_model.modd = s.data.first().copied(),
                _ => {}
            }
        }
    }

    /// Load an `ALOC` (Media Location Controller) record.
    ///
    /// The record is `EDID`, `FULL`, seven `NAM` fields, six media-set lists
    /// and `FNAM`. The six lists (`HNAM`, `ZNAM`, `XNAM`, `YNAM`, `LNAM` and
    /// `GNAM`) are *repeated* subrecords, so they cannot be read out of
    /// `raw_subs`, which holds one value per signature and would collapse
    /// every list onto its last entry. The subrecords are walked in file order
    /// instead, so each list keeps the order the record stores it in.
    ///
    /// `NAM1`, `NAM2`, `NAM3` and `FNAM` are four bytes each that the spec
    /// does not name. They are kept whole and carried through unchanged.
    fn load_aloc(&mut self, raw_subs: &[fnv_esm::Subrecord]) {
        self.aloc_nam1 = [0; 4];
        self.aloc_nam2 = [0; 4];
        self.aloc_nam3 = [0; 4];
        self.aloc_fnam = [0; 4];
        self.aloc_location_delay = 0.0;
        self.aloc_day_start = 0;
        self.aloc_night_start = 0;
        self.aloc_retrigger_delay = 0.0;
        self.aloc_conditional_faction = None;

        self.aloc_sets = ALOC_SET_LISTS
            .iter()
            .map(|(sig, label)| AlocSet {
                sig: *sig,
                label,
                entries: Vec::new(),
            })
            .collect();

        for s in raw_subs {
            match &s.sig {
                b"NAM1" if s.data.len() >= 4 => {
                    self.aloc_nam1 = [s.data[0], s.data[1], s.data[2], s.data[3]];
                }
                b"NAM2" if s.data.len() >= 4 => {
                    self.aloc_nam2 = [s.data[0], s.data[1], s.data[2], s.data[3]];
                }
                b"NAM3" if s.data.len() >= 4 => {
                    self.aloc_nam3 = [s.data[0], s.data[1], s.data[2], s.data[3]];
                }
                b"NAM4" if s.data.len() >= 4 => {
                    self.aloc_location_delay =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"NAM5" if s.data.len() >= 4 => {
                    self.aloc_day_start =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"NAM6" if s.data.len() >= 4 => {
                    self.aloc_night_start =
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"NAM7" if s.data.len() >= 4 => {
                    self.aloc_retrigger_delay =
                        f32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                }
                b"FNAM" if s.data.len() >= 4 => {
                    self.aloc_fnam = [s.data[0], s.data[1], s.data[2], s.data[3]];
                }
                b"RNAM" if s.data.len() >= 4 => {
                    self.aloc_conditional_faction = Some(u32::from_le_bytes([
                        s.data[0], s.data[1], s.data[2], s.data[3],
                    ]));
                }
                _ => {
                    // One of the six media-set lists.
                    if let Some(list) = self.aloc_sets.iter_mut().find(|l| l.sig == s.sig) {
                        if s.data.len() >= 4 {
                            list.entries.push(u32::from_le_bytes([
                                s.data[0], s.data[1], s.data[2], s.data[3],
                            ]));
                        }
                    }
                }
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
        if self.raw_subs.contains_key(b"EDID") {
            session.doc.set_sub(
                fid,
                b"EDID",
                encode_text_kept(self.raw_subs.get(b"EDID").map(|v| v.as_slice()), &self.edid),
            );
        } else {
            session.doc.remove_sub(fid, b"EDID");
        }
        if self.raw_subs.contains_key(b"FULL") {
            session.doc.set_sub(
                fid,
                b"FULL",
                encode_text_kept(
                    self.raw_subs.get(b"FULL").map(|v| v.as_slice()),
                    &self.full_name,
                ),
            );
        } else if !self.full_name.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FULL", encode_zstring(&self.full_name));
        }
        // The race's MODL belongs to its head-part collection rather than
        // to the record's own model, so the header panel's model field does
        // not apply to it.
        if self.sig != *b"RACE" {
            if self.raw_subs.contains_key(b"MODL") {
                session.doc.set_sub(
                    fid,
                    b"MODL",
                    encode_text_kept(
                        self.raw_subs.get(b"MODL").map(|v| v.as_slice()),
                        &self.model,
                    ),
                );
            } else if !self.model.trim().is_empty() {
                session
                    .doc
                    .set_sub(fid, b"MODL", encode_zstring(&self.model));
            }
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

        if self.raw_subs.contains_key(b"OBND") {
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
            b"CREA" => self.save_actor(session),
            b"NPC_" => self.save_actor(session),
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
            b"ENCH" => self.save_spell(session),
            b"EXPL" => self.save_expl(session),
            b"PROJ" => self.save_proj(session),
            b"SOUN" => self.save_soun(session),
            b"NOTE" => self.save_note(session),
            b"LVLC" => self.save_lvl(session),
            b"LVLN" => self.save_lvl(session),
            b"LVLI" => self.save_lvl(session),
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
            b"ALOC" => self.save_aloc(session),
            b"AMEF" => self.save_amef(session),
            b"ARMA" => self.save_arma(session),
            b"CAMS" => self.save_cams(session),
            b"CHAL" => self.save_chal(session),
            b"KEYM" => self.save_keym(session),
            b"INGR" => self.save_ingr(session),
            b"IPCT" => self.save_ipct(session),
            b"IPDS" => self.save_ipds(session),
            b"RCPE" => self.save_rcpe(session),
            b"SCPT" => self.save_scpt(session),
            b"PACK" => self.save_pack(session),
            b"EFSH" => self.save_efsh(session),
            b"REGN" => self.save_regn(session),
            b"IMAD" => self.save_imad(session),
            b"COBJ" => self.save_cobj(session),
            b"PMIS" => self.save_pmis(session),
            b"TES4" => self.save_tes4(session),
            b"FLST" => self.save_flst(session),
            b"MESG" => self.save_mesg(session),
            b"MISC" => self.save_misc(session),
            b"SPEL" => self.save_spel(session),
            b"STAT" => self.save_stat(session),
            b"IDLE" => self.save_idle(session),
            b"MSET" => self.save_mset(session),
            b"MSTT" => self.save_mstt(session),
            b"PWAT" => self.save_pwat(session),
            b"RCCT" => self.save_rcct(session),
            b"REPU" => self.save_repu(session),
            b"RGDL" => self.save_rgdl(session),
            b"TREE" => self.save_tree(session),
            b"TXST" => self.save_txst(session),
            b"VTYP" => self.save_vtyp(session),
            b"TACT" => self.save_tact(session),
            b"SCOL" => self.save_scol(session),
            b"IDLM" => self.save_idlm(session),
            b"IMGS" => self.save_imgs(session),
            b"LGTM" => self.save_lgtm(session),
            b"LSCR" => self.save_lscr(session),
            b"LSCT" => self.save_lsct(session),
            b"MICN" => self.save_micn(session),
            b"MUSC" => self.save_musc(session),
            b"DEHY" => self.save_dehy(session),
            b"DOBJ" => self.save_dobj(session),
            b"ECZN" => self.save_eczn(session),
            b"FURN" => self.save_furn(session),
            b"GRAS" => self.save_gras(session),
            b"HUNG" => self.save_hung(session),
            b"RADS" => self.save_rads(session),
            b"SLPD" => self.save_slpd(session),
            b"CPTH" => self.save_cpth(session),
            b"CSNO" => self.save_csno(session),
            b"DEBR" => self.save_debr(session),
            b"CCRD" => self.save_ccrd(session),
            b"CDCK" => self.save_cdck(session),
            b"CHIP" => self.save_chip(session),
            b"CMNY" => self.save_cmny(session),
            b"ASPC" => self.save_aspc(session),
            b"ANIO" => self.save_anio(session),
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
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 15]);
        d.resize(d.len().max(15), 0);
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

        // The critical data is written back over the record's own bytes, so
        // the flags and the bytes the spec does not name survive.
        if let Some(original) = self.raw_subs.get(b"CRDT") {
            let mut crdt = original.clone();
            crdt.resize(crdt.len().max(16), 0);
            crdt[0..2].copy_from_slice(&self.weap_crit_dmg.to_le_bytes());
            crdt[4..8].copy_from_slice(&self.weap_crit_mult.to_le_bytes());
            crdt[12..16].copy_from_slice(&self.weap_crit_effect.unwrap_or(0).to_le_bytes());
            session.doc.set_sub(fid, b"CRDT", crdt);
        }

        let mut dnam = self
            .raw_subs
            .get(b"DNAM")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 108]);
        if dnam.len() < 108 {
            dnam.resize(108, 0);
        }
        // The record's own length is kept: the game's animation data runs to
        // 204 bytes.
        if let Some(original) = self.raw_subs.get(b"DNAM") {
            dnam = original.clone();
            if dnam.len() < 108 {
                dnam.resize(108, 0);
            }
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
        if dnam.len() >= 4 {
            // The animation type is the first field of the animation data.
            dnam[0..4].copy_from_slice(&self.weap_animation_type.to_le_bytes());
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
        // The type's bits are the three the spec names, and the record can
        // carry more than one of them. The bits the record already had are
        // kept unless the modder picks a different type, so a save that
        // changes nothing reproduces the record.
        const TYPE_BITS: u8 = (1 << 3) | (1 << 5) | (1 << 7);
        let type_bit: u8 = match self.armo_type {
            1 => 1 << 3, // Medium
            2 => 1 << 7, // Heavy
            3 => 1 << 5, // Power
            _ => 0,      // Light
        };
        if self.armo_type != self.armo_type_from_general {
            // Clear the three named bits, then set the one the enum selects.
            general &= !TYPE_BITS;
            general |= type_bit;
        }
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

        // The faction list is a run of SNAM, and each one carries three
        // unnamed bytes. The list is written back over the record's own
        // entries, so those bytes survive.
        let snam_buffers: Vec<Vec<u8>> = self
            .actor_factions
            .iter()
            .enumerate()
            .map(|(i, (fac_fid, rank))| {
                let mut buf = self
                    .raw_snam
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| vec![0u8; 8]);
                buf.resize(8, 0);
                buf[0..4].copy_from_slice(&fac_fid.to_le_bytes());
                buf[4] = *rank as u8;
                buf
            })
            .collect();
        if snam_buffers.is_empty() {
            session.doc.remove_sub(fid, b"SNAM");
        } else {
            session.doc.set_sub_list(fid, b"SNAM", snam_buffers);
        }

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

        // The inventory is a run of CNTO, and a COED after one of them is
        // that item's extra data, so the whole run is written as one
        // sequence.
        let mut inv: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for entry in &self.actor_inventory_entries {
            let mut buf = vec![0u8; 8];
            buf[0..4].copy_from_slice(&entry.item.to_le_bytes());
            buf[4..8].copy_from_slice(&entry.count.to_le_bytes());
            inv.push((*b"CNTO", buf));
            if let Some(extra) = &entry.extra {
                inv.push((*b"COED", extra.clone()));
            }
        }
        if inv.is_empty() {
            session.doc.remove_sub(fid, b"CNTO");
            session.doc.remove_sub(fid, b"COED");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"CNTO", *b"COED"], inv);
        }

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

        // The destruction data.
        if self.raw_subs.contains_key(b"DEST") {
            save_destruction(
                session,
                fid,
                self.dest_health,
                self.actor_dest_flags,
                &self.actor_destruction,
            );
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
        // The data and the dat2 are written back over the record's own
        // bytes, so a shorter record is not grown and the bytes the spec
        // does not name survive.
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 13]);
        if d.len() >= 4 {
            d[0..4].copy_from_slice(&self.ammo_speed.to_le_bytes());
        }
        if d.len() >= 12 {
            d[8..12].copy_from_slice(&self.ammo_val.to_le_bytes());
        }
        if d.len() >= 13 {
            d[12] = self.ammo_clip_rounds as u8;
        }
        session.doc.set_sub(fid, b"DATA", d);

        if let Some(original) = self.raw_subs.get(b"DAT2") {
            let mut dat2 = original.clone();
            if dat2.len() >= 12 {
                dat2[8..12].copy_from_slice(&self.ammo_weight.to_le_bytes());
            }
            session.doc.set_sub(fid, b"DAT2", dat2);
        }

        // `QNAM` is the ammo's abbreviation, a cstring. It used to be read as
        // a FormID, which turned "5mm" into a number and wrote those four
        // bytes back over the abbreviation.
        if self.raw_subs.contains_key(b"QNAM") {
            session.doc.set_sub(
                fid,
                b"QNAM",
                encode_text_kept(
                    self.raw_subs.get(b"QNAM").map(|v| v.as_slice()),
                    &self.ammo_abbrev,
                ),
            );
        } else if !self.ammo_abbrev.trim().is_empty() {
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
        // The flags are written into the record's own byte, so a bit the
        // spec does not name survives.
        let mut f = self
            .raw_subs
            .get(b"FNAM")
            .and_then(|d| d.first().copied())
            .unwrap_or(0);
        if self.door_auto {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.door_hidden {
            f |= 2;
        } else {
            f &= !2;
        }
        if self.door_minimal {
            f |= 4;
        } else {
            f &= !4;
        }
        if self.door_sliding {
            f |= 8;
        } else {
            f &= !8;
        }
        // The flag byte is written back over the record's own bytes, so a
        // bit the spec does not name survives.
        if let Some(original) = self.raw_subs.get(b"FNAM") {
            let mut fnam = original.clone();
            if !fnam.is_empty() {
                fnam[0] = f;
            }
            session.doc.set_sub(fid, b"FNAM", fnam);
        } else {
            session.doc.set_sub(fid, b"FNAM", vec![f]);
        }
        if let Some(s) = self.door_sound_open {
            if self.raw_subs.contains_key(b"SNAM") {
                session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
            }
        } else {
            session.doc.remove_sub(fid, b"SNAM");
        }
    }

    fn save_acti(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The prompt is a filename, and the game's records hold bytes in it
        // that are not valid UTF-8, so it is written back over the record's
        // own bytes.
        if self.raw_subs.contains_key(b"RNAM") {
            session.doc.set_sub(
                fid,
                b"RNAM",
                encode_text_kept(
                    self.raw_subs.get(b"RNAM").map(|v| v.as_slice()),
                    &self.acti_prompt,
                ),
            );
        } else if !self.acti_prompt.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"RNAM", encode_zstring(&self.acti_prompt));
        }
        if let Some(s) = self.acti_sound_loop {
            if self.raw_subs.contains_key(b"SNAM") {
                session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
            }
        } else {
            session.doc.remove_sub(fid, b"SNAM");
        }
    }

    fn save_term(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The difficulty byte is written back over the record's own bytes,
        // so the length it had survives.
        if let Some(original) = self.raw_subs.get(b"DNAM") {
            let mut dnam = original.clone();
            if !dnam.is_empty() {
                dnam[0] = self.term_difficulty;
            }
            session.doc.set_sub(fid, b"DNAM", dnam);
        } else {
            session
                .doc
                .set_sub(fid, b"DNAM", vec![self.term_difficulty]);
        }
        if self.raw_subs.contains_key(b"WNAM") {
            session.doc.set_sub(
                fid,
                b"WNAM",
                encode_text_kept(
                    self.raw_subs.get(b"WNAM").map(|v| v.as_slice()),
                    &self.term_welcome,
                ),
            );
        } else if !self.term_welcome.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"WNAM", encode_zstring(&self.term_welcome));
        }
        if let Some(s) = self.term_sound_loop {
            if self.raw_subs.contains_key(b"SNAM") {
                session.doc.set_sub(fid, b"SNAM", s.to_le_bytes().to_vec());
            }
        } else {
            session.doc.remove_sub(fid, b"SNAM");
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
        // The data struct is written back over the record's own bytes, so
        // the length it had and the bytes the spec does not name survive.
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 10]);
        d.resize(d.len().max(10), 0);
        d[1] = self.book_teaches;
        d[2..6].copy_from_slice(&self.item_val.to_le_bytes());
        d[6..10].copy_from_slice(&self.item_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
        if self.raw_subs.contains_key(b"DESC") {
            session.doc.set_sub(
                fid,
                b"DESC",
                encode_text_kept(
                    self.raw_subs.get(b"DESC").map(|v| v.as_slice()),
                    &self.book_text,
                ),
            );
        } else if !self.book_text.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.book_text));
        }
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
        let mut d = self
            .raw_subs
            .get(b"DATA")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 8]);
        // The flags are written into the record's own byte, so a bit the
        // spec does not name survives.
        let mut f = if d.is_empty() { 0 } else { d[0] };
        if self.qust_start_enabled {
            f |= 1;
        } else {
            f &= !1;
        }
        if self.qust_repeated_stages {
            f |= 8;
        } else {
            f &= !8;
        }
        if !d.is_empty() {
            d[0] = f;
        }
        if d.len() >= 2 {
            d[1] = self.qust_priority;
        }
        if d.len() >= 8 {
            d[4..8].copy_from_slice(&self.qust_delay.to_le_bytes());
        }
        session.doc.set_sub(fid, b"DATA", d);
    }

    fn save_perk(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The data struct is written back over the record's own bytes, so
        // the length it had survives: the game's records are three and five
        // bytes long.
        // The perk's own data is the first five-byte one, and the record
        // also carries a three-byte data per rank. The perk's is written
        // back over its own bytes, and the rank's are left alone.
        if let Some(original) = self.perk_own_data.as_ref() {
            let mut d = original.clone();
            d[0] = if self.perk_trait { 1 } else { 0 };
            d[1] = self.perk_min_level;
            d[2] = self.perk_num_ranks;
            d[3] = self.perk_playable;
            d[4] = self.perk_hidden;
            session.doc.set_sub(fid, b"DATA", d);
        }
        if self.raw_subs.contains_key(b"DESC") {
            session.doc.set_sub(
                fid,
                b"DESC",
                encode_text_kept(
                    self.raw_subs.get(b"DESC").map(|v| v.as_slice()),
                    &self.perk_desc,
                ),
            );
        } else if !self.perk_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.perk_desc));
        }
    }

    fn save_spell(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The actor effect's data is an ENIT, not the spell's SPIT, so the
        // SPIT is only written for a spell.
        if self.raw_subs.contains_key(b"SPIT") {
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
        } else {
            session.doc.remove_sub(fid, b"SPIT");
        }
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
        if self.raw_subs.contains_key(b"FNAM") {
            session.doc.set_sub(
                fid,
                b"FNAM",
                encode_text_kept(
                    self.raw_subs.get(b"FNAM").map(|v| v.as_slice()),
                    &self.sound_file,
                ),
            );
        } else if !self.sound_file.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FNAM", encode_zstring(&self.sound_file));
        }
        // The random chance byte is written back over the record's own
        // bytes, so its length survives.
        if let Some(original) = self.raw_subs.get(b"RNAM") {
            let mut rnam = original.clone();
            if !rnam.is_empty() {
                rnam[0] = self.sound_random_chance;
            }
            session.doc.set_sub(fid, b"RNAM", rnam);
        }
        // The sound data is written back over the record's own bytes, so
        // the length it had and the bytes the spec does not name survive.
        for sig in [b"SNDD", b"SNDX"] {
            if let Some(original) = self.raw_subs.get(sig) {
                let mut a = self.sound_attenuation.clone();
                // The two flag bits the checkboxes own are written inside
                // the flag word, so a bit the spec does not name survives.
                if self.sound_loop {
                    a.flags |= 1 << 4;
                } else {
                    a.flags &= !(1 << 4);
                }
                if self.sound_2d {
                    a.flags |= 1 << 6;
                } else {
                    a.flags &= !(1 << 6);
                }
                a.min_distance = self.sound_min_dist;
                a.max_distance = self.sound_max_dist;
                a.frequency_adjustment = self.sound_freq_adj;
                a.static_attenuation = self.sound_vol_cdb;
                let data = write_sound_attenuation(original, &a);
                session.doc.set_sub(fid, sig, data);
            }
        }

        // A record with the short form keeps its curve points, its reverb
        // control and its priority in their own subrecords, and the short
        // struct does not carry them.
        if self.raw_subs.contains_key(b"SNDX") {
            if let Some(original) = self.raw_subs.get(b"ANAM") {
                let mut anam = original.clone();
                anam.resize(anam.len().max(10), 0);
                for (i, point) in self.sound_attenuation.attenuation_points.iter().enumerate() {
                    let at = i * 2;
                    if anam.len() >= at + 2 {
                        anam[at..at + 2].copy_from_slice(&point.to_le_bytes());
                    }
                }
                session.doc.set_sub(fid, b"ANAM", anam);
            }
            if let Some(original) = self.raw_subs.get(b"GNAM") {
                let mut gnam = original.clone();
                gnam.resize(gnam.len().max(2), 0);
                gnam[0..2]
                    .copy_from_slice(&self.sound_attenuation.reverb_attenuation.to_le_bytes());
                session.doc.set_sub(fid, b"GNAM", gnam);
            }
            if let Some(original) = self.raw_subs.get(b"HNAM") {
                let mut hnam = original.clone();
                hnam.resize(hnam.len().max(4), 0);
                hnam[0..4].copy_from_slice(&self.sound_attenuation.priority.to_le_bytes());
                session.doc.set_sub(fid, b"HNAM", hnam);
            }
        }
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

        // The entries are a run of `LVLO` and an optional `COED`, so they
        // are written as one sequence: a pair of list edits would gather the
        // LVLOs together and separate them from their extra data.
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for entry in &self.lvl_entries {
            let mut buf = vec![0u8; 12];
            buf[0..2].copy_from_slice(&entry.level.to_le_bytes());
            buf[2..4].copy_from_slice(&entry.unused[0..2]);
            buf[4..8].copy_from_slice(&entry.reference.unwrap_or(0).to_le_bytes());
            buf[8..10].copy_from_slice(&entry.unused[2..4]);
            buf[10..12].copy_from_slice(&entry.count.to_le_bytes());
            seq.push((*b"LVLO", buf));
            if entry.has_extra {
                let mut coed = vec![0u8; 12];
                coed[0..4].copy_from_slice(&entry.owner.unwrap_or(0).to_le_bytes());
                coed[4..8].copy_from_slice(&entry.global_or_rank.to_le_bytes());
                coed[8..12].copy_from_slice(&entry.item_condition.to_le_bytes());
                seq.push((*b"COED", coed));
            }
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"LVLO");
            session.doc.remove_sub(fid, b"COED");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"LVLO", *b"COED"], seq);
        }

        if !self.model.is_empty() {
            let mut modl = self.model.as_bytes().to_vec();
            modl.push(0);
            session.doc.set_sub(fid, b"MODL", modl);
        } else {
            session.doc.remove_sub(fid, b"MODL");
        }

        if self.raw_subs.contains_key(b"OBND") {
            let mut obnd = vec![0u8; 12];
            for i in 0..3 {
                obnd[i * 2..i * 2 + 2].copy_from_slice(&self.obnd_min[i].to_le_bytes());
                obnd[6 + i * 2..6 + i * 2 + 2].copy_from_slice(&self.obnd_max[i].to_le_bytes());
            }
            session.doc.set_sub(fid, b"OBND", obnd);
        } else {
            session.doc.remove_sub(fid, b"OBND");
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

        // The cloud texture layers.
        for (sig, text) in [
            (b"DNAM", &self.wthr_cloud_layers[0]),
            (b"CNAM", &self.wthr_cloud_layers[1]),
            (b"ANAM", &self.wthr_cloud_layers[2]),
            (b"BNAM", &self.wthr_cloud_layers[3]),
        ] {
            if self.raw_subs.contains_key(sig) {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            } else if !text.trim().is_empty() {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }

        // The maximum number of cloud layers.
        if let Some(original) = self.raw_subs.get(b"LNAM") {
            let mut lnam = original.clone();
            lnam.resize(lnam.len().max(4), 0);
            lnam[0..4].copy_from_slice(&self.wthr_max_cloud_layers.to_le_bytes());
            session.doc.set_sub(fid, b"LNAM", lnam);
        }

        // The cloud layer speeds.
        if let Some(original) = self.raw_subs.get(b"ONAM") {
            let mut onam = original.clone();
            for (i, speed) in self.wthr_cloud_speeds.iter().enumerate() {
                if i < onam.len() {
                    onam[i] = *speed;
                }
            }
            session.doc.set_sub(fid, b"ONAM", onam);
        }

        // The cloud layer colours: one time of day colours struct per
        // cloud layer.
        if let Some(original) = self.raw_subs.get(b"PNAM") {
            let times = if original.len() % 96 == 0 { 6 } else { 4 };
            let mut pnam = original.clone();
            for (i, colors) in self.wthr_cloud_colors.iter().enumerate() {
                let at = i * times * 4;
                if pnam.len() >= at + times * 4 {
                    let written = write_time_of_day_colors(&pnam[at..at + times * 4], colors);
                    pnam[at..at + times * 4].copy_from_slice(&written);
                }
            }
            session.doc.set_sub(fid, b"PNAM", pnam);
        }

        // The weather's own colours: ten colour types, each with one
        // colour per time of day.
        if let Some(original) = self.raw_subs.get(b"NAM0") {
            let times = if original.len() % 240 == 0 {
                6
            } else if original.len() % 160 == 0 {
                4
            } else {
                6
            };
            let mut nam0 = original.clone();
            for (i, colors) in self.wthr_colors.iter().enumerate() {
                let at = i * times * 4;
                if nam0.len() >= at + times * 4 {
                    let written = write_time_of_day_colors(&nam0[at..at + times * 4], colors);
                    nam0[at..at + times * 4].copy_from_slice(&written);
                }
            }
            session.doc.set_sub(fid, b"NAM0", nam0);
        }

        // The fog distances: six float32s.
        if let Some(original) = self.raw_subs.get(b"FNAM") {
            let mut fnam = original.clone();
            fnam.resize(fnam.len().max(24), 0);
            for (i, d) in self.wthr_fog_distances.iter().enumerate() {
                let at = i * 4;
                if fnam.len() >= at + 4 {
                    fnam[at..at + 4].copy_from_slice(&d.to_le_bytes());
                }
            }
            session.doc.set_sub(fid, b"FNAM", fnam);
        }

        // The data struct, written back over the record's own bytes so
        // its length and the bytes the spec does not name survive.
        if let Some(original) = self.raw_subs.get(b"DATA") {
            let mut d = original.clone();
            d.resize(d.len().max(15), 0);
            d[0] = self.wthr_wind_speed;
            d[1] = self.wthr_cloud_speed_lower;
            d[2] = self.wthr_cloud_speed_upper;
            d[3] = self.wthr_transition_delta;
            d[4] = self.wthr_sun_glare;
            d[5] = self.wthr_sun_damage;
            d[6] = self.wthr_precipitation_begin;
            d[7] = self.wthr_precipitation_end;
            d[8] = self.wthr_thunder_begin;
            d[9] = self.wthr_thunder_end;
            d[10] = self.wthr_thunder_frequency;
            d[11] = self.wthr_weather_classification;
            d[12] = self.wthr_lightning_color[0];
            d[13] = self.wthr_lightning_color[1];
            d[14] = self.wthr_lightning_color[2];
            session.doc.set_sub(fid, b"DATA", d);
        }

        // The image space modifiers. Their signatures begin with an
        // unprintable byte that names the time of day.
        for (index, value) in self.wthr_image_space_modifiers.iter().enumerate() {
            let mut sig = [0u8; 4];
            sig[0] = index as u8;
            sig[1..].copy_from_slice(b"IAD");
            if let Some(v) = value {
                session.doc.set_sub(fid, &sig, v.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, &sig);
            }
        }

        // The sounds.
        let sounds: Vec<([u8; 4], Vec<u8>)> = self
            .wthr_sounds
            .iter()
            .map(|s| {
                let mut buf = vec![0u8; 8];
                buf[0..4].copy_from_slice(&s.sound.to_le_bytes());
                buf[4..8].copy_from_slice(&s.kind.to_le_bytes());
                (*b"SNAM", buf)
            })
            .collect();
        if sounds.is_empty() {
            session.doc.remove_sub(fid, b"SNAM");
        } else {
            session.doc.set_sub_sequence(fid, vec![*b"SNAM"], sounds);
        }
    }

    fn save_watr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The visual data is written back over the record's own bytes, so
        // the bytes the spec does not name survive.
        if let Some(original) = self.raw_subs.get(b"DNAM") {
            let data = write_watr_visual(original, &self.watr_visual);
            session.doc.set_sub(fid, b"DNAM", data);
        }
        // The related waters: the daytime, nighttime and underwater
        // records. The struct is written back over the record's own
        // bytes, so its length survives.
        if let Some(original) = self.raw_subs.get(b"GNAM") {
            let mut g = original.clone();
            g.resize(g.len().max(12), 0);
            for (at, value) in [
                (0usize, self.watr_daytime),
                (4, self.watr_nighttime),
                (8, self.watr_underwater),
            ] {
                if let Some(v) = value {
                    g[at..at + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
            session.doc.set_sub(fid, b"GNAM", g);
        }
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
        // The named fields are written into the record's own bytes, so the
        // bytes the spec does not name survive.
        let _ = &d;
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
        // The time struct is written back over the record's own bytes, so
        // the byte the spec does not name survives.
        let mut tnam = self
            .raw_subs
            .get(b"TNAM")
            .cloned()
            .unwrap_or_else(|| vec![0u8; 6]);
        tnam.resize(tnam.len().max(6), 0);
        tnam[0] = self.clmt_sunrise_begin;
        tnam[1] = self.clmt_sunrise_end;
        tnam[2] = self.clmt_sunset_begin;
        tnam[3] = self.clmt_sunset_end;
        tnam[4] = self.clmt_volatility;
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

    /// Save a `DEHY` (Dehydration) record.
    fn save_dehy(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.dehyd_trigger_threshold.to_le_bytes());
        data.extend_from_slice(&self.dehyd_actor_effect.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `HUNG` (Hunger) record.
    fn save_hung(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.hung_trigger_threshold.to_le_bytes());
        data.extend_from_slice(&self.hung_actor_effect.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `RADS` (Radiation) record.
    fn save_rads(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.rads_trigger_threshold.to_le_bytes());
        data.extend_from_slice(&self.rads_actor_effect.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `SLPD` (Sleep Deprivation) record.
    fn save_slpd(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.slpd_trigger_threshold.to_le_bytes());
        data.extend_from_slice(&self.slpd_actor_effect.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save an `ECZN` (Encounter Zone) record.
    ///
    /// `DATA` is eight bytes. The owner may be null, in which case the
    /// FormID is written as zero.
    fn save_eczn(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.eczn_owner.unwrap_or(0).to_le_bytes());
        data.push(self.eczn_rank as u8);
        data.push(self.eczn_minimum_level as u8);
        data.push(self.eczn_flags);
        data.push(self.eczn_unused);
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `DOBJ` (Default Object Manager) record.
    ///
    /// `DATA` is a flat array of thirty-four FormIDs. The record's own length
    /// is kept, so a save that changes nothing is byte-identical.
    fn save_dobj(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(136);
        let mut data = original.unwrap_or_else(|| vec![0u8; 136]);
        data.resize(len, 0);
        for (i, value) in self.dobj_values.iter().enumerate() {
            let at = i * 4;
            if at + 4 <= data.len() {
                data[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `FURN` (Furniture) record.
    fn save_furn(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // MNAM: the marker flags. Every one of the game's FURN records
        // carries four bytes, so it is always written.
        session
            .doc
            .set_sub(fid, b"MNAM", self.furn_marker_flags.clone());

        let model = &self.furn_model;
        let sigs = model.instance_sigs();
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

        // The destruction data.
        save_destruction(
            session,
            fid,
            self.furn_dest_health,
            self.furn_dest_flags,
            &self.furn_destruction,
        );
    }

    /// Save a `GRAS` (Grass) record.
    ///
    /// `DATA` is written back over the record's own bytes, so the fields the
    /// spec marks unused survive an edit.
    fn save_gras(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(32);
        let mut data = original.unwrap_or_else(|| vec![0u8; 32]);
        data.resize(len, 0);

        if data.len() >= 3 {
            data[0] = self.gras_density;
            data[1] = self.gras_min_slope;
            data[2] = self.gras_max_slope;
        }
        if data.len() >= 6 {
            data[4..6].copy_from_slice(&self.gras_unit_from_water_amount.to_le_bytes());
        }
        if data.len() >= 12 {
            data[8..12].copy_from_slice(&self.gras_unit_from_water_type.to_le_bytes());
        }
        if data.len() >= 16 {
            data[12..16].copy_from_slice(&self.gras_position_range.to_le_bytes());
        }
        if data.len() >= 20 {
            data[16..20].copy_from_slice(&self.gras_height_range.to_le_bytes());
        }
        if data.len() >= 24 {
            data[20..24].copy_from_slice(&self.gras_color_range.to_le_bytes());
        }
        if data.len() >= 28 {
            data[24..28].copy_from_slice(&self.gras_wave_period.to_le_bytes());
        }
        if data.len() >= 29 {
            data[28] = self.gras_flags;
        }
        session.doc.set_sub(fid, b"DATA", data);

        let model = &self.gras_model;
        let sigs = model.instance_sigs();
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
    }

    /// Save an `IDLM` (Idle Animation) record.
    ///
    /// `IDLA` is a *repeated* subrecord, so the animation list is written with
    /// a list edit: a per-subrecord edit would collapse the list onto its
    /// last animation.
    fn save_idlm(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // IDLF, IDLC, IDLT and IDLA are optional: 13 of the game's 211 IDLM
        // records carry none of them, and a save must not add them.
        if self.raw_subs.contains_key(b"IDLF") {
            session.doc.set_sub(fid, b"IDLF", vec![self.idlm_flags]);
        } else {
            session.doc.remove_sub(fid, b"IDLF");
        }

        // IDLC is one byte on most of the game's records and four on the
        // rest. The length the record had is kept, so a save that changes
        // nothing is byte-identical.
        let original = self.raw_subs.get(b"IDLC").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(1);
        let mut idlc = original.unwrap_or_else(|| vec![0u8; 1]);
        idlc.resize(len, 0);
        if !idlc.is_empty() {
            idlc[0] = self.idlm_animation_count;
        }
        if idlc.len() >= 4 {
            idlc[1..4].copy_from_slice(&self.idlm_idlc_unused);
        }
        if self.raw_subs.contains_key(b"IDLC") {
            session.doc.set_sub(fid, b"IDLC", idlc);
        } else {
            session.doc.remove_sub(fid, b"IDLC");
        }

        if self.raw_subs.contains_key(b"IDLT") {
            session
                .doc
                .set_sub(fid, b"IDLT", self.idlm_idle_timer.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"IDLT");
        }

        // One IDLA subrecord holds the whole list of FormIDs, so it is
        // written as a single payload rather than as a list of subrecords.
        if self.idlm_animations.is_empty() {
            session.doc.remove_sub(fid, b"IDLA");
        } else if self.raw_subs.contains_key(b"IDLA") {
            let mut payload = Vec::with_capacity(self.idlm_animations.len() * 4);
            for a in &self.idlm_animations {
                payload.extend_from_slice(&a.to_le_bytes());
            }
            session.doc.set_sub(fid, b"IDLA", payload);
        }
    }

    /// Save an `IMGS` (Image Space) record.
    ///
    /// `DNAM` is written back over the record's own bytes, so the
    /// version-dependent part — which depends on the record's form version —
    /// survives an edit.
    fn save_imgs(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"DNAM").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(152);
        let mut data = original.unwrap_or_else(|| vec![0u8; 152]);
        data.resize(len, 0);
        for (i, v) in self.imgs_hdr.iter().enumerate() {
            let at = i * 4;
            if at + 4 <= data.len() {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        session.doc.set_sub(fid, b"DNAM", data);
    }

    /// Save a `LGTM` (Lighting) record.
    ///
    /// `DATA` is forty bytes. Every one of the game's 31 LGTM records carries
    /// exactly forty bytes, so a fixed write is byte-identical to what the
    /// record had.
    fn save_lgtm(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(40);
        data.extend_from_slice(&self.lgtm_ambient_color);
        data.extend_from_slice(&self.lgtm_directional_color);
        data.extend_from_slice(&self.lgtm_fog_color);
        data.extend_from_slice(&self.lgtm_fog_near.to_le_bytes());
        data.extend_from_slice(&self.lgtm_fog_far.to_le_bytes());
        data.extend_from_slice(&self.lgtm_directional_rotation_xy.to_le_bytes());
        data.extend_from_slice(&self.lgtm_directional_rotation_z.to_le_bytes());
        data.extend_from_slice(&self.lgtm_directional_fade.to_le_bytes());
        data.extend_from_slice(&self.lgtm_fog_clip_distance.to_le_bytes());
        data.extend_from_slice(&self.lgtm_fog_power.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save an `LSCR` (Load Screen) record.
    ///
    /// The `LNAM` locations are *repeated* subrecords, so they are written
    /// with a list edit.
    fn save_lscr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        for (sig, text) in [
            (b"ICON", &self.lscr_icon),
            (b"MICO", &self.lscr_mico),
            (b"DESC", &self.lscr_desc),
        ] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }

        match self.lscr_load_screen_type {
            Some(t) => session.doc.set_sub(fid, b"WMI1", t.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"WMI1"),
        }

        let locations: Vec<Vec<u8>> = self
            .lscr_locations
            .iter()
            .map(|l| {
                let mut b = Vec::with_capacity(12);
                b.extend_from_slice(&l.direct.unwrap_or(0).to_le_bytes());
                b.extend_from_slice(&l.indirect_world.unwrap_or(0).to_le_bytes());
                b.extend_from_slice(&l.indirect_grid_y.to_le_bytes());
                b.extend_from_slice(&l.indirect_grid_x.to_le_bytes());
                b
            })
            .collect();
        if locations.is_empty() {
            session.doc.remove_sub(fid, b"LNAM");
        } else {
            session.doc.set_sub_list(fid, b"LNAM", locations);
        }
    }

    /// Save an `LSCT` (Load Screen Type) record.
    ///
    /// `DATA` is eighty-eight bytes. The twenty unknown bytes the spec names
    /// in the middle of the struct are carried through unchanged.
    fn save_lsct(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(88);
        let mut data = original.unwrap_or_else(|| vec![0u8; 88]);
        data.resize(len, 0);

        let put_u32 = |data: &mut Vec<u8>, at: usize, v: u32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };
        let put_f32 = |data: &mut Vec<u8>, at: usize, v: f32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };

        put_u32(&mut data, 0, self.lsct_type);
        put_u32(&mut data, 4, self.lsct_x);
        put_u32(&mut data, 8, self.lsct_y);
        put_u32(&mut data, 12, self.lsct_width);
        put_u32(&mut data, 16, self.lsct_height);
        put_f32(&mut data, 20, self.lsct_orientation);
        put_u32(&mut data, 24, self.lsct_font1);
        put_f32(&mut data, 28, self.lsct_font1_color[0]);
        put_f32(&mut data, 32, self.lsct_font1_color[1]);
        put_f32(&mut data, 36, self.lsct_font1_color[2]);
        put_u32(&mut data, 40, self.lsct_font1_alignment);
        put_u32(&mut data, 64, self.lsct_font2);
        put_f32(&mut data, 68, self.lsct_font2_color[0]);
        put_f32(&mut data, 72, self.lsct_font2_color[1]);
        put_f32(&mut data, 76, self.lsct_font2_color[2]);
        put_u32(&mut data, 80, self.lsct_font2_alignment);
        put_u32(&mut data, 84, self.lsct_stats);
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `MICN` (Mission Icon) record.
    fn save_micn(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        for (sig, text) in [(b"ICON", &self.micn_icon), (b"MICO", &self.micn_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }
    }

    /// Save a `MUSC` (Music) record.
    fn save_musc(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if self.musc_filename.trim().is_empty() {
            session.doc.remove_sub(fid, b"FNAM");
        } else {
            session
                .doc
                .set_sub(fid, b"FNAM", encode_zstring(&self.musc_filename));
        }
        // ANAM is optional: 19 of the game's 20 MUSC records carry it and
        // one does not. It is written only when the record had one, so a
        // save that changes nothing does not add one.
        if self.raw_subs.contains_key(b"ANAM") {
            session
                .doc
                .set_sub(fid, b"ANAM", self.musc_db.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"ANAM");
        }
    }

    /// Save a `RCCT` (Recipe Category) record.
    fn save_rcct(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session.doc.set_sub(fid, b"DATA", vec![self.rcct_flags]);
    }

    /// Save a `REPU` (Reputation) record.
    fn save_repu(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"DATA", self.repu_value.to_le_bytes().to_vec());
        for (sig, text) in [(b"ICON", &self.repu_icon), (b"MICO", &self.repu_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }
    }

    /// Save an `RGDL` (Ragdoll) record.
    ///
    /// `RAFD` and `RAPS` are carried through unchanged: their layout is
    /// documented but the editor does not decode them.
    fn save_rgdl(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"NVER", self.rgdl_version.to_le_bytes().to_vec());

        // DATA is fourteen bytes: the dynamic bone count, four unused bytes,
        // the five enabled flags and one unused byte.
        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(14);
        let mut data = original.unwrap_or_else(|| vec![0u8; 14]);
        data.resize(len, 0);
        if data.len() >= 14 {
            data[0..4].copy_from_slice(&self.rgdl_dynamic_bone_count.to_le_bytes());
            data[8] = self.rgdl_feedback_enabled;
            data[9] = self.rgdl_foot_ik_enabled;
            data[10] = self.rgdl_look_ik_enabled;
            data[11] = self.rgdl_grab_ik_enabled;
            data[12] = self.rgdl_pose_matching;
        }
        session.doc.set_sub(fid, b"DATA", data);

        match self.rgdl_actor_base {
            Some(v) if self.raw_subs.contains_key(b"XNAM") => {
                session.doc.set_sub(fid, b"XNAM", v.to_le_bytes().to_vec());
            }
            _ => {
                session.doc.remove_sub(fid, b"XNAM");
            }
        }
        match self.rgdl_body_part_data {
            Some(v) => session.doc.set_sub(fid, b"TNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"TNAM"),
        }
        if self.rgdl_death_pose.trim().is_empty() {
            session.doc.remove_sub(fid, b"ANAM");
        } else {
            session
                .doc
                .set_sub(fid, b"ANAM", encode_zstring(&self.rgdl_death_pose));
        }
        if self.rgdl_feedback_bones.is_empty() {
            session.doc.remove_sub(fid, b"RAFB");
        } else {
            let mut payload = Vec::with_capacity(self.rgdl_feedback_bones.len() * 2);
            for b in &self.rgdl_feedback_bones {
                payload.extend_from_slice(&b.to_le_bytes());
            }
            session.doc.set_sub(fid, b"RAFB", payload);
        }
        // RAFD is sixty bytes.
        let mut rafd = Vec::with_capacity(60);
        rafd.extend_from_slice(&self.rgdl_dynamic_keyframe_blend.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_hierarchy_gain.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_position_gain.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_velocity_gain.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_acceleration_gain.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_snap_gain.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_velocity_damping.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_snap_max_linear_velocity.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_snap_max_angular_velocity.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_snap_max_linear_distance.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_snap_max_angular_distance.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_position_max_linear_velocity.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_position_max_angular_velocity.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_projectile_position_max_velocity.to_le_bytes());
        rafd.extend_from_slice(&self.rgdl_melee_position_max_velocity.to_le_bytes());
        session.doc.set_sub(fid, b"RAFD", rafd);

        // RAPS is twenty-four bytes. The unused byte the spec names is
        // carried through unchanged.
        let original = self.raw_subs.get(b"RAPS").cloned();
        let mut raps = original.unwrap_or_else(|| vec![0u8; 24]);
        raps.resize(24, 0);
        raps[0..2].copy_from_slice(&self.rgdl_match_bone1.to_le_bytes());
        raps[2..4].copy_from_slice(&self.rgdl_match_bone2.to_le_bytes());
        raps[4..6].copy_from_slice(&self.rgdl_match_bone3.to_le_bytes());
        raps[6] = self.rgdl_aps_flags;
        raps[8..12].copy_from_slice(&self.rgdl_motors_strength.to_le_bytes());
        raps[12..16].copy_from_slice(&self.rgdl_pose_activation_delay.to_le_bytes());
        raps[16..20].copy_from_slice(&self.rgdl_match_error_allowance.to_le_bytes());
        raps[20..24].copy_from_slice(&self.rgdl_displacement_to_disable.to_le_bytes());
        session.doc.set_sub(fid, b"RAPS", raps);
    }

    /// Save a `TREE` (Tree) record.
    fn save_tree(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The model data collection: MODL, MODB, MODT, MODS and MODD.
        let tree_model = &self.tree_model;
        if !tree_model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&tree_model.modl));
        }
        if let Some(modb) = tree_model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !tree_model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", tree_model.modt.clone());
        }
        if !tree_model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, b"MODS", write_mods(&tree_model.mods));
        }
        if let Some(modd) = tree_model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        for (sig, text) in [(b"ICON", &self.tree_icon), (b"MICO", &self.tree_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }

        if self.tree_seeds.is_empty() {
            session.doc.remove_sub(fid, b"SNAM");
        } else if self.raw_subs.contains_key(b"SNAM") {
            let mut payload = Vec::with_capacity(self.tree_seeds.len() * 4);
            for s in &self.tree_seeds {
                payload.extend_from_slice(&s.to_le_bytes());
            }
            session.doc.set_sub(fid, b"SNAM", payload);
        }

        // CNAM is thirty-two bytes: six float32s, the int32 shadow radius
        // and two more float32s. It is written back over the record's own
        // bytes, so the length the record had is kept.
        if let Some(original) = self.raw_subs.get(b"CNAM").cloned() {
            let len = original.len();
            let mut cnam = original;
            cnam.resize(len, 0);
            let mut cnam_bytes = Vec::with_capacity(32);
            cnam_bytes.extend_from_slice(&self.tree_leaf_curvature.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_min_leaf_angle.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_max_leaf_angle.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_branch_dimming.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_leaf_dimming.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_shadow_radius.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_rock_speed.to_le_bytes());
            cnam_bytes.extend_from_slice(&self.tree_rustle_speed.to_le_bytes());
            for (i, byte) in cnam_bytes.iter().enumerate() {
                if i < cnam.len() {
                    cnam[i] = *byte;
                }
            }
            session.doc.set_sub(fid, b"CNAM", cnam);
        } else {
            session.doc.remove_sub(fid, b"CNAM");
        }

        if self.raw_subs.contains_key(b"BNAM") {
            let mut bnam = Vec::with_capacity(8);
            bnam.extend_from_slice(&self.tree_billboard_width.to_le_bytes());
            bnam.extend_from_slice(&self.tree_billboard_height.to_le_bytes());
            session.doc.set_sub(fid, b"BNAM", bnam);
        } else {
            session.doc.remove_sub(fid, b"BNAM");
        }
    }

    /// Save a `TXST` (Texture) record.
    fn save_txst(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        const SIGS: [&[u8; 4]; 6] = [b"TX00", b"TX01", b"TX02", b"TX03", b"TX04", b"TX05"];
        for (i, sig) in SIGS.iter().enumerate() {
            let text = self.txst_textures.get(i).map(String::as_str).unwrap_or("");
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }
        // DNAM is optional: 313 of the game's 493 TXST records carry it, and
        // a save must not add it to the others.
        if self.raw_subs.contains_key(b"DNAM") {
            session
                .doc
                .set_sub(fid, b"DNAM", self.txst_flags.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"DNAM");
        }

        // DODT is thirty-six bytes: the seven floats, the parallax passes,
        // the flag word, two unused bytes and the rgba colour.
        if self.raw_subs.contains_key(b"DODT") {
            let original = self.raw_subs.get(b"DODT").cloned();
            let mut dodt = original.unwrap_or_else(|| vec![0u8; 36]);
            dodt.resize(36, 0);
            let floats = [
                self.txst_dodt_min_width,
                self.txst_dodt_max_width,
                self.txst_dodt_min_height,
                self.txst_dodt_max_height,
                self.txst_dodt_depth,
                self.txst_dodt_shininess,
                self.txst_dodt_parallax_scale,
            ];
            for (i, v) in floats.iter().enumerate() {
                let at = i * 4;
                dodt[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            dodt[28] = self.txst_dodt_parallax_passes;
            dodt[29] = self.txst_dodt_flags;
            dodt[32..36].copy_from_slice(&self.txst_dodt_color);
            session.doc.set_sub(fid, b"DODT", dodt);
        }
    }

    /// Save a `VTYP` (Voice Type) record.
    fn save_vtyp(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // DNAM is optional: 91 of the game's 100 VTYP records carry it, and a
        // save must not add it to the others.
        if self.raw_subs.contains_key(b"DNAM") {
            session.doc.set_sub(fid, b"DNAM", vec![self.vtyp_flags]);
        } else {
            session.doc.remove_sub(fid, b"DNAM");
        }
    }

    /// Save a `TACT` (Talking Activator) record.
    fn save_tact(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The model data collection: MODL, MODB, MODT, MODS and MODD.
        let tact_model = &self.tact_model;
        if !tact_model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&tact_model.modl));
        }
        if let Some(modb) = tact_model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !tact_model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", tact_model.modt.clone());
        }
        if !tact_model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, b"MODS", write_mods(&tact_model.mods));
        }
        if let Some(modd) = tact_model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        for (sig, value) in [
            (b"SNAM", self.tact_looping_sound),
            (b"VNAM", self.tact_voice_type),
            (b"INAM", self.tact_radio_template),
            (b"SCRI", self.tact_script),
        ] {
            match value {
                Some(v) => session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec()),
                None => session.doc.remove_sub(fid, sig),
            }
        }

        if self.tact_destruction.is_empty() && self.tact_dest_health <= 0 {
            session.doc.remove_sub(fid, b"DEST");
            session.doc.remove_sub(fid, b"DSTD");
        } else {
            save_destruction(
                session,
                fid,
                self.tact_dest_health,
                self.tact_dest_flags,
                &self.tact_destruction,
            );
        }
    }

    /// Save a `SCOL` (Static Collision) record.
    ///
    /// The `ONAM` and `DATA` subrecords both repeat, so they are written
    /// with list edits: a per-subrecord edit would collapse the parts onto
    /// their last entry.
    fn save_scol(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The model data collection: MODL, MODB, MODT, MODS and MODD.
        let scol_model = &self.scol_model;
        if !scol_model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&scol_model.modl));
        }
        if let Some(modb) = scol_model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !scol_model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", scol_model.modt.clone());
        }
        if !scol_model.mods.is_empty() {
            session
                .doc
                .set_sub(fid, b"MODS", write_mods(&scol_model.mods));
        }
        if let Some(modd) = scol_model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        // The collection interleaves ONAM and DATA, so it is written as one
        // sequence rather than as two lists: a pair of list edits would
        // gather the ONAMs together and separate them from their DATA.
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for part in &self.scol_parts {
            let mut o = Vec::with_capacity(4);
            o.extend_from_slice(&part.static_ref.unwrap_or(0).to_le_bytes());
            seq.push((*b"ONAM", o));

            let mut d = Vec::with_capacity(part.placements.len() * 28);
            for p in &part.placements {
                for v in p {
                    d.extend_from_slice(&v.to_le_bytes());
                }
            }
            seq.push((*b"DATA", d));
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"ONAM");
            session.doc.remove_sub(fid, b"DATA");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"ONAM", *b"DATA"], seq);
        }
    }

    /// Save an `IDLE` (Idle Animation) record.
    fn save_idle(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The model data collection.
        let model = &self.idle_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        // ANAM carries the parent and the previous sibling, and a null
        // reference is written as zero.
        if self.raw_subs.contains_key(b"ANAM") {
            let mut anam = Vec::with_capacity(8);
            anam.extend_from_slice(&self.idle_parent.unwrap_or(0).to_le_bytes());
            anam.extend_from_slice(&self.idle_previous_sibling.unwrap_or(0).to_le_bytes());
            session.doc.set_sub(fid, b"ANAM", anam);
        } else {
            session.doc.remove_sub(fid, b"ANAM");
        }

        // DATA is six or eight bytes. The record's own length decides, and
        // the unnamed byte is carried through.
        if let Some(original) = self.raw_subs.get(b"DATA") {
            let mut data = original.clone();
            data.resize(original.len(), 0);
            if !data.is_empty() {
                data[0] = self.idle_group_section;
            }
            if data.len() >= 2 {
                data[1] = self.idle_looping_min;
            }
            if data.len() >= 3 {
                data[2] = self.idle_looping_max;
            }
            if data.len() >= 6 {
                data[4..6].copy_from_slice(&self.idle_replay_delay.to_le_bytes());
            }
            if data.len() >= 7 {
                data[6] = self.idle_flags;
            }
            session.doc.set_sub(fid, b"DATA", data);
        } else {
            session.doc.remove_sub(fid, b"DATA");
        }

        if self.raw_subs.contains_key(b"CTDA") {
            save_conditions(session, fid, &self.idle_conditions);
        } else {
            session.doc.remove_sub(fid, b"CTDA");
        }
    }

    /// Save an `MSET` (Media Set) record.
    fn save_mset(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        session
            .doc
            .set_sub(fid, b"NAM1", self.mset_type.to_le_bytes().to_vec());

        const TRACK_SIGS: [&[u8; 4]; 6] = [b"NAM2", b"NAM3", b"NAM4", b"NAM5", b"NAM6", b"NAM7"];
        const LEVEL_SIGS: [&[u8; 4]; 6] = [b"NAM8", b"NAM9", b"NAM0", b"ANAM", b"BNAM", b"CNAM"];

        for (i, sig) in TRACK_SIGS.iter().enumerate() {
            let text = self.mset_tracks.get(i).map(String::as_str).unwrap_or("");
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }
        // The track levels are optional: the game's 140 MSET records carry
        // between one and six of them.
        for (i, sig) in LEVEL_SIGS.iter().enumerate() {
            let level = self.mset_levels.get(i).copied().unwrap_or(0.0);
            if self.raw_subs.contains_key(*sig) {
                session.doc.set_sub(fid, sig, level.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }

        // The boundaries repeat, so they are written with a list edit.
        if self.mset_boundaries.is_empty() {
            session.doc.remove_sub(fid, b"JNAM");
        } else {
            let payloads: Vec<Vec<u8>> = self
                .mset_boundaries
                .iter()
                .map(|v| v.to_le_bytes().to_vec())
                .collect();
            session.doc.set_sub_list(fid, b"JNAM", payloads);
        }

        // PNAM, DNAM, ENAM, FNAM and GNAM are optional: the game's 140 MSET
        // records carry between 44 and 140 of each.
        for (sig, value) in [
            (b"PNAM", vec![self.mset_enable_flags]),
            (b"DNAM", self.mset_wait_time.to_le_bytes().to_vec()),
            (b"ENAM", self.mset_loop_fade_out.to_le_bytes().to_vec()),
            (b"FNAM", self.mset_recovery_time.to_le_bytes().to_vec()),
            (b"GNAM", self.mset_nighttime_max.to_le_bytes().to_vec()),
        ] {
            if self.raw_subs.contains_key(sig) {
                session.doc.set_sub(fid, sig, value);
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }

        match self.mset_intro {
            Some(v) => session.doc.set_sub(fid, b"HNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"HNAM"),
        }
        match self.mset_outro {
            Some(v) => session.doc.set_sub(fid, b"INAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"INAM"),
        }
    }

    /// Save an `MSTT` (Movable Static) record.
    fn save_mstt(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.mstt_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        session.doc.set_sub(fid, b"DATA", vec![self.mstt_data]);
        match self.mstt_sound {
            Some(v) => session.doc.set_sub(fid, b"SNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"SNAM"),
        }

        if self.mstt_destruction.is_empty() && self.mstt_dest_health <= 0 {
            session.doc.remove_sub(fid, b"DEST");
            session.doc.remove_sub(fid, b"DSTD");
        } else {
            save_destruction(
                session,
                fid,
                self.mstt_dest_health,
                self.mstt_dest_flags,
                &self.mstt_destruction,
            );
        }
    }

    /// Save a `PWAT` (Water) record.
    fn save_pwat(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.pwat_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        let mut dnam = Vec::with_capacity(8);
        dnam.extend_from_slice(&self.pwat_flags.to_le_bytes());
        dnam.extend_from_slice(&self.pwat_water.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DNAM", dnam);
    }

    /// Save a `FLST` (Form List) record.
    ///
    /// The `LNAM` subrecords repeat, so they are written with a list edit.
    fn save_flst(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if self.flst_entries.is_empty() {
            session.doc.remove_sub(fid, b"LNAM");
        } else {
            let payloads: Vec<Vec<u8>> = self
                .flst_entries
                .iter()
                .map(|e| e.to_le_bytes().to_vec())
                .collect();
            session.doc.set_sub_list(fid, b"LNAM", payloads);
        }
    }

    /// Save an `LVLI` (Leveled Item) record.
    ///
    /// The entries are a run of `LVLO` and an optional `COED`, so they are
    /// written as one sequence: a pair of list edits would separate an
    /// entry's extra data from the entry itself.
    /// Save an `LVLN` (Leveled NPC) record.
    /// Save a `MESG` (Message) record.
    ///
    /// The buttons are a run of `ITXT` and repeated `CTDA`, so they are
    /// written as one sequence.
    fn save_mesg(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The description is plain text, and the game's records hold it
        // both with and without a trailing null, so the record's own bytes
        // decide the form.
        if !self.raw_subs.contains_key(b"DESC") {
            session.doc.remove_sub(fid, b"DESC");
        } else {
            session.doc.set_sub(
                fid,
                b"DESC",
                encode_text_kept(
                    self.raw_subs.get(b"DESC").map(|v| v.as_slice()),
                    &self.mesg_desc,
                ),
            );
        }
        match self.mesg_icon {
            Some(v) if self.raw_subs.contains_key(b"INAM") => {
                session.doc.set_sub(fid, b"INAM", v.to_le_bytes().to_vec());
            }
            _ => {
                session.doc.remove_sub(fid, b"INAM");
            }
        }
        if self.raw_subs.contains_key(b"DNAM") {
            session
                .doc
                .set_sub(fid, b"DNAM", self.mesg_flags.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"DNAM");
        }
        if self.raw_subs.contains_key(b"TNAM") {
            session
                .doc
                .set_sub(fid, b"TNAM", self.mesg_display_time.to_le_bytes().to_vec());
        } else {
            session.doc.remove_sub(fid, b"TNAM");
        }

        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for button in &self.mesg_buttons {
            seq.push((*b"ITXT", encode_text_kept(Some(&button.raw), &button.text)));
            for cond in &button.conditions {
                let mut b = Vec::with_capacity(28);
                b.push(cond.cond_type);
                b.extend_from_slice(&cond.unused);
                b.extend_from_slice(&cond.comparison_value.to_le_bytes());
                b.extend_from_slice(&cond.function.to_le_bytes());
                b.extend_from_slice(&cond.parameter1.to_le_bytes());
                b.extend_from_slice(&cond.parameter2.to_le_bytes());
                b.extend_from_slice(&cond.run_on.to_le_bytes());
                b.extend_from_slice(&cond.reference.unwrap_or(0).to_le_bytes());
                seq.push((*b"CTDA", b));
            }
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"ITXT");
            session.doc.remove_sub(fid, b"CTDA");
        } else if self.raw_subs.contains_key(b"ITXT") {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"ITXT", *b"CTDA"], seq);
        }
    }

    /// Save a `MISC` (Miscellaneous) record.
    fn save_misc(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.misc_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        for (sig, text) in [(b"ICON", &self.misc_icon), (b"MICO", &self.misc_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }

        for (sig, value) in [
            (b"SCRI", self.misc_script),
            (b"YNAM", self.misc_pickup_sound),
            (b"ZNAM", self.misc_drop_sound),
            (b"RNAM", self.misc_random_sound),
        ] {
            match value {
                Some(v) => session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec()),
                None => session.doc.remove_sub(fid, sig),
            }
        }

        // DATA is eight bytes: the value and the weight.
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&self.misc_value.to_le_bytes());
        data.extend_from_slice(&self.misc_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);

        if self.misc_destruction.is_empty() && self.misc_dest_health <= 0 {
            session.doc.remove_sub(fid, b"DEST");
            session.doc.remove_sub(fid, b"DSTD");
        } else {
            save_destruction(
                session,
                fid,
                self.misc_dest_health,
                self.misc_dest_flags,
                &self.misc_destruction,
            );
        }
    }

    /// Save a `SPEL` (Spell) record.
    ///
    /// The effects are a run of `EFID` and `EFIT`, so they are written as
    /// one sequence.
    fn save_spel(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"SPIT").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(16);
        let mut spit = original.unwrap_or_else(|| vec![0u8; 16]);
        spit.resize(len, 0);
        if spit.len() >= 16 {
            spit[0..4].copy_from_slice(&self.spel_type.to_le_bytes());
            spit[4..8].copy_from_slice(&self.spel_cost.to_le_bytes());
            spit[8..12].copy_from_slice(&self.spel_level.to_le_bytes());
            spit[12] = self.spel_flags;
        }
        session.doc.set_sub(fid, b"SPIT", spit);

        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for effect in &self.spel_effects {
            let mut efid = Vec::with_capacity(4);
            efid.extend_from_slice(&effect.effect.unwrap_or(0).to_le_bytes());
            seq.push((*b"EFID", efid));
            if !effect.parameters.is_empty() {
                seq.push((*b"EFIT", effect.parameters.clone()));
            }
            // The condition belongs to the effect above it, so it is
            // written straight after it.
            for cond in &effect.conditions {
                seq.push((*b"CTDA", write_condition(cond)));
            }
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"EFID");
            session.doc.remove_sub(fid, b"EFIT");
            session.doc.remove_sub(fid, b"CTDA");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"EFID", *b"EFIT", *b"CTDA"], seq);
        }
    }

    /// Save a `STAT` (Static) record.
    fn save_stat(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.stat_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        // BRUS is optional: 3404 of the game's 6785 STAT records carry it.
        if self.raw_subs.contains_key(b"BRUS") {
            session
                .doc
                .set_sub(fid, b"BRUS", vec![self.stat_passthrough as u8]);
        } else {
            session.doc.remove_sub(fid, b"BRUS");
        }
        match self.stat_looping_sound {
            Some(v) => session.doc.set_sub(fid, b"RNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"RNAM"),
        }
    }

    /// Save a `KEYM` (Key) record.
    fn save_keym(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.keym_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }
        for (sig, text) in [(b"ICON", &self.keym_icon), (b"MICO", &self.keym_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }
        for (sig, value) in [
            (b"SCRI", self.keym_script),
            (b"YNAM", self.keym_pickup_sound),
            (b"ZNAM", self.keym_drop_sound),
            (b"RNAM", self.keym_random_sound),
        ] {
            match value {
                Some(v) if self.raw_subs.contains_key(sig) => {
                    session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec());
                }
                _ => {
                    session.doc.remove_sub(fid, sig);
                }
            }
        }
        // DATA is optional: the game's KEYM records carry it only when the
        // key has a value or a weight.
        if self.raw_subs.contains_key(b"DATA") {
            let mut data = Vec::with_capacity(8);
            data.extend_from_slice(&self.keym_value.to_le_bytes());
            data.extend_from_slice(&self.keym_weight.to_le_bytes());
            session.doc.set_sub(fid, b"DATA", data);
        } else {
            session.doc.remove_sub(fid, b"DATA");
        }

        if self.keym_destruction.is_empty() && self.keym_dest_health <= 0 {
            session.doc.remove_sub(fid, b"DEST");
            session.doc.remove_sub(fid, b"DSTD");
        } else {
            save_destruction(
                session,
                fid,
                self.keym_dest_health,
                self.keym_dest_flags,
                &self.keym_destruction,
            );
        }
    }

    /// Save an `INGR` (Ingredient) record.
    fn save_ingr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.ingr_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }
        for (sig, text) in [(b"ICON", &self.ingr_icon), (b"MICO", &self.ingr_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }
        match self.ingr_script {
            Some(v) => session.doc.set_sub(fid, b"SCRI", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"SCRI"),
        }
        session.doc.set_sub(
            fid,
            b"ETYP",
            self.ingr_equipment_type.to_le_bytes().to_vec(),
        );
        session
            .doc
            .set_sub(fid, b"DATA", self.ingr_weight.to_le_bytes().to_vec());

        // ENIT is eight bytes: the value, the flag word and three unused
        // bytes.
        let original = self.raw_subs.get(b"ENIT").cloned();
        let mut enit = original.unwrap_or_else(|| vec![0u8; 8]);
        enit.resize(8, 0);
        enit[0..4].copy_from_slice(&self.ingr_effect_value.to_le_bytes());
        enit[4] = self.ingr_effect_flags;
        session.doc.set_sub(fid, b"ENIT", enit);

        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for effect in &self.ingr_effects {
            let mut efid = Vec::with_capacity(4);
            efid.extend_from_slice(&effect.effect.unwrap_or(0).to_le_bytes());
            seq.push((*b"EFID", efid));
            if !effect.parameters.is_empty() {
                seq.push((*b"EFIT", effect.parameters.clone()));
            }
            // The condition belongs to the effect above it.
            for cond in &effect.conditions {
                seq.push((*b"CTDA", write_condition(cond)));
            }
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"EFID");
            session.doc.remove_sub(fid, b"EFIT");
            session.doc.remove_sub(fid, b"CTDA");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"EFID", *b"EFIT", *b"CTDA"], seq);
        }
    }

    /// Save an `IPCT` (Impact) record.
    fn save_ipct(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.ipct_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }

        // DATA is twenty-four bytes.
        let original = self.raw_subs.get(b"DATA").cloned();
        let mut data = original.unwrap_or_else(|| vec![0u8; 24]);
        data.resize(24, 0);
        data[0..4].copy_from_slice(&self.ipct_duration.to_le_bytes());
        data[4..8].copy_from_slice(&self.ipct_orientation.to_le_bytes());
        data[8..12].copy_from_slice(&self.ipct_angle_threshold.to_le_bytes());
        data[12..16].copy_from_slice(&self.ipct_placement_radius.to_le_bytes());
        data[16..20].copy_from_slice(&self.ipct_sound_level.to_le_bytes());
        data[20..24].copy_from_slice(&self.ipct_flags.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);

        if let Some(dodt) = &self.ipct_dodt {
            // The decal data is written back over the record's own bytes, so
            // the two bytes the spec leaves unnamed keep their values.
            let mut b = self
                .raw_subs
                .get(b"DODT")
                .cloned()
                .unwrap_or_else(|| vec![0u8; 36]);
            b.resize(36, 0);
            let floats = [
                dodt.min_width,
                dodt.max_width,
                dodt.min_height,
                dodt.max_height,
                dodt.depth,
                dodt.shininess,
                dodt.parallax_scale,
            ];
            for (i, v) in floats.iter().enumerate() {
                let at = i * 4;
                b[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            b[28] = dodt.parallax_passes;
            b[29] = dodt.flags;
            b[32..36].copy_from_slice(&dodt.color);
            session.doc.set_sub(fid, b"DODT", b);
        }

        // DNAM, SNAM and NAM1 are optional: 91 of the game's 125 IPCT records
        // carry DNAM and 109 carry SNAM.
        for (sig, value) in [
            (b"DNAM", self.ipct_texture_set),
            (b"SNAM", self.ipct_sound1),
            (b"NAM1", self.ipct_sound2),
        ] {
            match value {
                Some(v) if self.raw_subs.contains_key(sig) => {
                    session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec());
                }
                _ => {
                    session.doc.remove_sub(fid, sig);
                }
            }
        }
    }

    /// Save an `IPDS` (Impact Data Set) record.
    fn save_ipds(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // The impact list is a run of FormIDs, and the record's own length
        // decides how many of them there are: the game's records are 36
        // and 48 bytes long.
        let count = self
            .raw_subs
            .get(b"DATA")
            .map(|b| (b.len() / 4).min(12))
            .unwrap_or(12);
        let mut data = Vec::with_capacity(count * 4);
        for impact in self.ipds_impacts.iter().take(count) {
            data.extend_from_slice(&impact.unwrap_or(0).to_le_bytes());
        }
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save an `RCPE` (Recipe) record.
    ///
    /// The ingredients and the outputs are written as one run of `RCIL` and
    /// `RCQY` pairs: the ingredients first, then the outputs.
    fn save_rcpe(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&self.rcpe_skill.to_le_bytes());
        data.extend_from_slice(&self.rcpe_level.to_le_bytes());
        data.extend_from_slice(&self.rcpe_category.unwrap_or(0).to_le_bytes());
        data.extend_from_slice(&self.rcpe_sub_category.unwrap_or(0).to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);

        // The ingredients and the outputs are written as one run: every
        // ingredient, then every output, each with its quantity.
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for (sig, items) in [
            (*b"RCIL", &self.rcpe_ingredients),
            (*b"RCOD", &self.rcpe_outputs),
        ] {
            for item in items {
                let mut id = Vec::with_capacity(4);
                id.extend_from_slice(&item.item.unwrap_or(0).to_le_bytes());
                seq.push((sig, id));
                seq.push((*b"RCQY", item.quantity.to_le_bytes().to_vec()));
            }
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"RCIL");
            session.doc.remove_sub(fid, b"RCOD");
            session.doc.remove_sub(fid, b"RCQY");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"RCIL", *b"RCOD", *b"RCQY"], seq);
        }

        save_conditions(session, fid, &self.rcpe_conditions);
    }

    /// Save a `SCPT` (Script) record.
    fn save_scpt(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // SCHR is twenty bytes: the four unnamed bytes, the reference count,
        // the compiled size, the variable count, the type and the flags.
        let original = self.raw_subs.get(b"SCHR").cloned();
        let mut schr = original.unwrap_or_else(|| vec![0u8; 20]);
        schr.resize(20, 0);
        schr[8..12].copy_from_slice(&self.scpt_header.unknown.to_le_bytes());
        schr[12..14].copy_from_slice(&(self.scpt_header.script_type as u16).to_le_bytes());
        schr[14..16].copy_from_slice(&self.scpt_header.flags.to_le_bytes());
        schr[16..20].copy_from_slice(&self.scpt_header.version.to_le_bytes());
        session.doc.set_sub(fid, b"SCHR", schr);

        // SCDA and SCTX are optional: a record the modder emptied does not
        // gain one it did not have.
        if self.raw_subs.contains_key(b"SCDA") && !self.scpt_compiled.is_empty() {
            session
                .doc
                .set_sub(fid, b"SCDA", self.scpt_compiled.clone());
        } else {
            session.doc.remove_sub(fid, b"SCDA");
        }
        if self.raw_subs.contains_key(b"SCTX") && !self.scpt_source.trim().is_empty() {
            session.doc.set_sub(
                fid,
                b"SCTX",
                encode_text_kept(
                    self.raw_subs.get(b"SCTX").map(|v| v.as_slice()),
                    &self.scpt_source,
                ),
            );
        } else {
            session.doc.remove_sub(fid, b"SCTX");
        }

        // The variables are a run of SLSD and SCVR, so they are written as
        // one sequence: a pair of list edits would separate a variable's
        // name from its data.
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for var in &self.scpt_variables {
            // SLSD is twenty-four bytes: the index, twelve unused bytes, the
            // flag word and seven more unused bytes. The variable is written
            // back over its own bytes, so the values the spec does not name
            // survive an edit.
            let mut slsd = var.raw.clone();
            slsd.resize(24, 0);
            slsd[0..4].copy_from_slice(&var.index.to_le_bytes());
            slsd[16] = var.flags;
            seq.push((*b"SLSD", slsd));
            seq.push((*b"SCVR", encode_zstring(&var.name)));
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"SLSD");
            session.doc.remove_sub(fid, b"SCVR");
        } else if self.raw_subs.contains_key(b"SLSD") {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"SLSD", *b"SCVR"], seq);
        }
    }

    /// Save a `PMIS` (Placed Missile) record.
    fn save_pmis(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        for (sig, value) in [
            (b"NAME", self.pmis_base),
            (b"XEZN", self.pmis_encounter_zone),
            (b"INAM", self.pmis_idle),
            (b"TNAM", self.pmis_topic),
            (b"XOWN", self.pmis_owner),
        ] {
            match value {
                Some(v) if self.raw_subs.contains_key(sig) => {
                    session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec());
                }
                _ => {
                    session.doc.remove_sub(fid, sig);
                }
            }
        }
        if self.raw_subs.contains_key(b"XPRD") {
            session
                .doc
                .set_sub(fid, b"XPRD", self.pmis_idle_time.to_le_bytes().to_vec());
        }
        if self.raw_subs.contains_key(b"XSCL") {
            session
                .doc
                .set_sub(fid, b"XSCL", self.pmis_scale.to_le_bytes().to_vec());
        }
        if !self.pmis_ragdoll.is_empty() {
            session.doc.set_sub(fid, b"XRGD", self.pmis_ragdoll.clone());
        }
        if !self.pmis_ragdoll_biped.is_empty() {
            session
                .doc
                .set_sub(fid, b"XRGB", self.pmis_ragdoll_biped.clone());
        }
        // The position struct is written back over the record's own bytes, so
        // the length it had and the unnamed bytes survive.
        if let Some(original) = self.raw_subs.get(b"DATA") {
            let mut d = original.clone();
            d.resize(d.len().max(38), 0);
            for i in 0..3 {
                let at = i * 4;
                d[at..at + 4].copy_from_slice(&self.pmis_position[i].to_le_bytes());
                let at = 12 + i * 4;
                d[at..at + 4].copy_from_slice(&self.pmis_rotation[i].to_le_bytes());
            }
            d[24..28].copy_from_slice(&self.pmis_bounding_radius.to_le_bytes());
            d[28..32].copy_from_slice(&self.pmis_flags.to_le_bytes());
            d[32..38].copy_from_slice(&self.pmis_unknown);
            session.doc.set_sub(fid, b"DATA", d);
        }
    }

    /// Save a `COBJ` (Constructible Object) record.
    fn save_cobj(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.cobj_model;
        if !model.modl.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&model.modl));
        }
        if let Some(modb) = model.modb {
            session.doc.set_sub(fid, b"MODB", modb.to_vec());
        }
        if !model.modt.is_empty() {
            session.doc.set_sub(fid, b"MODT", model.modt.clone());
        }
        if !model.mods.is_empty() {
            session.doc.set_sub(fid, b"MODS", write_mods(&model.mods));
        }
        if let Some(modd) = model.modd {
            session.doc.set_sub(fid, b"MODD", vec![modd]);
        }
        // The data struct is written back over the record's own bytes.
        if let Some(original) = self.raw_subs.get(b"DATA") {
            let mut d = original.clone();
            d.resize(d.len().max(8), 0);
            d[0..4].copy_from_slice(&self.cobj_value.to_le_bytes());
            d[4..8].copy_from_slice(&self.cobj_weight.to_le_bytes());
            session.doc.set_sub(fid, b"DATA", d);
        } else {
            let mut d = vec![0u8; 8];
            d[0..4].copy_from_slice(&self.cobj_value.to_le_bytes());
            d[4..8].copy_from_slice(&self.cobj_weight.to_le_bytes());
            session.doc.set_sub(fid, b"DATA", d);
        }
    }

    /// Save an `IMAD` (Image Space Adapter) record.
    fn save_imad(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // The data header is written back over the record's own bytes, so
        // the length it had and every unnamed bit survive.
        if let Some(original) = self.raw_subs.get(b"DNAM") {
            let data = write_imad_header(original, &self.imad_header);
            session.doc.set_sub(fid, b"DNAM", data);
        } else {
            session.doc.remove_sub(fid, b"DNAM");
        }

        // Each curve is one subrecord, so they go out one at a time. A curve
        // the record did not have is not added.
        for curve in &self.imad_curves {
            if !self.raw_subs.contains_key(&curve.sig) {
                continue;
            }
            let stride = if curve.is_color { 20 } else { 8 };
            let mut data = Vec::with_capacity(curve.keys.len() * stride);
            for key in &curve.keys {
                data.extend_from_slice(&key.time.to_le_bytes());
                data.extend_from_slice(&key.value.to_le_bytes());
                if curve.is_color {
                    data.extend_from_slice(&key.green.to_le_bytes());
                    data.extend_from_slice(&key.blue.to_le_bytes());
                    data.extend_from_slice(&key.alpha.to_le_bytes());
                }
            }
            session.doc.set_sub(fid, &curve.sig, data);
        }
    }

    /// Save an `EFSH` (Effect Shader) record.
    fn save_efsh(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        if let Some(original) = self.raw_subs.get(b"DATA") {
            let data = write_efsh_data(original, &self.efsh_data);
            session.doc.set_sub(fid, b"DATA", data);
        } else {
            session.doc.remove_sub(fid, b"DATA");
        }
        for (sig, text) in [
            (b"ICON", &self.efsh_icon),
            (b"ICO2", &self.efsh_ico2),
            (b"NAM7", &self.efsh_nam7),
        ] {
            // A texture the record had is written back even when the name
            // is empty, because an empty name is still a subrecord.
            if self.raw_subs.contains_key(sig) {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }
    }

    /// Save a `REGN` (Region) record.
    fn save_regn(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        if self.raw_subs.contains_key(b"RCLR") {
            session
                .doc
                .set_sub(fid, b"RCLR", self.regn_map_color.to_vec());
        } else {
            session.doc.remove_sub(fid, b"RCLR");
        }

        match self.regn_worldspace {
            Some(v) if self.raw_subs.contains_key(b"WNAM") => {
                session.doc.set_sub(fid, b"WNAM", v.to_le_bytes().to_vec());
            }
            _ => {
                session.doc.remove_sub(fid, b"WNAM");
            }
        }

        // The region area is a run of RPLI and RPLD, and each point list
        // belongs to the fall-off before it, so the pairs are written as
        // one sequence.
        let mut areas: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for area in &self.regn_areas {
            areas.push((*b"RPLI", area.edge_falloff.to_le_bytes().to_vec()));
            let mut points = Vec::with_capacity(area.points.len() * 8);
            for p in &area.points {
                points.extend_from_slice(&p[0].to_le_bytes());
                points.extend_from_slice(&p[1].to_le_bytes());
            }
            areas.push((*b"RPLD", points));
        }
        if self.raw_subs.contains_key(b"RPLD") {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"RPLI", *b"RPLD"], areas);
        } else {
            session.doc.remove_sub(fid, b"RPLD");
            session.doc.remove_sub(fid, b"RPLI");
        }

        // The region data is a run of RDAT headers, each followed by the
        // entry's own subrecords, so the whole thing is written as one
        // sequence: a per-signature list edit would scatter the entries.
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for entry in &self.regn_entries {
            let mut rdat = Vec::with_capacity(8);
            rdat.extend_from_slice(&entry.kind.to_le_bytes());
            rdat.push(entry.flags);
            rdat.push(entry.priority);
            rdat.extend_from_slice(&entry.unused);
            seq.push((*b"RDAT", rdat));
            if let Some(name) = &entry.map_name {
                seq.push((*b"RDMP", encode_zstring(name)));
            }
            if let Some(v) = entry.music_type {
                seq.push((*b"RDMD", v.to_le_bytes().to_vec()));
            }
            if let Some(v) = entry.music {
                seq.push((*b"RDMO", v.to_le_bytes().to_vec()));
            }
            if let Some(v) = entry.incidental_media {
                seq.push((*b"RDSI", v.to_le_bytes().to_vec()));
            }
            if !entry.battle_media.is_empty() {
                for v in &entry.battle_media {
                    seq.push((*b"RDSB", v.to_le_bytes().to_vec()));
                }
            }
            if !entry.objects.is_empty() || entry.had_empty_rdot {
                let mut data = Vec::with_capacity(entry.objects.len() * 52);
                for o in &entry.objects {
                    data.extend_from_slice(&o.object.to_le_bytes());
                    data.extend_from_slice(&o.parent_index.to_le_bytes());
                    data.extend_from_slice(&o.unused);
                    data.extend_from_slice(&o.density.to_le_bytes());
                    data.push(o.clustering);
                    data.push(o.min_slope);
                    data.push(o.max_slope);
                    data.push(o.flags);
                    data.extend_from_slice(&o.parent_radius.to_le_bytes());
                    data.extend_from_slice(&o.radius.to_le_bytes());
                    data.extend_from_slice(&o.unknown);
                    data.extend_from_slice(&o.max_height.to_le_bytes());
                    data.extend_from_slice(&o.sink.to_le_bytes());
                    data.extend_from_slice(&o.sink_variance.to_le_bytes());
                    data.extend_from_slice(&o.size_variance.to_le_bytes());
                    data.extend_from_slice(&o.x_angle_variance.to_le_bytes());
                    data.extend_from_slice(&o.y_angle_variance.to_le_bytes());
                    data.extend_from_slice(&o.z_angle_variance.to_le_bytes());
                    data.extend_from_slice(&o.unknown2);
                }
                seq.push((*b"RDOT", data));
            }
            if !entry.grasses.is_empty() {
                let mut data = Vec::with_capacity(entry.grasses.len() * 8);
                for g in &entry.grasses {
                    data.extend_from_slice(&g.grass.to_le_bytes());
                    data.extend_from_slice(&g.unknown);
                }
                seq.push((*b"RDGS", data));
            }
            if !entry.sounds.is_empty() {
                let mut data = Vec::with_capacity(entry.sounds.len() * 12);
                for s in &entry.sounds {
                    data.extend_from_slice(&s.sound.to_le_bytes());
                    data.extend_from_slice(&s.flags.to_le_bytes());
                    data.extend_from_slice(&s.chance.to_le_bytes());
                }
                seq.push((*b"RDSD", data));
            }
            if !entry.weather.is_empty() {
                let mut data = Vec::with_capacity(entry.weather.len() * 12);
                for w in &entry.weather {
                    data.extend_from_slice(&w.weather.to_le_bytes());
                    data.extend_from_slice(&w.chance.to_le_bytes());
                    data.extend_from_slice(&w.global.unwrap_or(0).to_le_bytes());
                }
                seq.push((*b"RDWT", data));
            }
            if !entry.imposters.is_empty() {
                let mut data = Vec::with_capacity(entry.imposters.len() * 4);
                for i in &entry.imposters {
                    data.extend_from_slice(&i.to_le_bytes());
                }
                seq.push((*b"RDID", data));
            }
            // The icon follows the entry it belongs to.
            if let Some(icon) = &entry.icon {
                seq.push((*b"ICON", encode_zstring(icon)));
            }
        }
        if self.raw_subs.contains_key(b"RDAT") {
            session.doc.set_sub_sequence(
                fid,
                vec![
                    *b"RDAT", *b"RDMP", *b"RDMD", *b"RDMO", *b"RDSI", *b"RDSB", *b"RDOT", *b"RDGS",
                    *b"RDSD", *b"RDWT", *b"RDID", *b"ICON",
                ],
                seq,
            );
        } else {
            for sig in [
                b"RDAT", b"RDMP", b"RDMD", b"RDMO", b"RDSI", b"RDSB", b"RDOT", b"RDGS", b"RDSD",
                b"RDWT", b"RDID", b"ICON",
            ] {
                session.doc.remove_sub(fid, sig);
            }
        }
    }

    /// Save a `PACK` (Package) record.
    fn save_pack(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // PKDT is either ten or twelve bytes: the general flags, the type,
        // one unnamed byte, the fallout behaviour flags, and then the
        // type-specific flags and two more unnamed bytes in the long form.
        // The record's own length decides which form is written back.
        let had_type_specific = self.raw_subs.contains_key(b"PKDT")
            && self.raw_subs.get(b"PKDT").map(|d| d.len()).unwrap_or(0) >= 10;
        if self.raw_subs.contains_key(b"PKDT") {
            let mut pkdt = Vec::with_capacity(12);
            pkdt.extend_from_slice(&self.pack_general_flags.to_le_bytes());
            pkdt.push(self.pack_type);
            pkdt.push(self.pack_unused);
            pkdt.extend_from_slice(&self.pack_fallout_behavior_flags.to_le_bytes());
            if had_type_specific {
                pkdt.extend_from_slice(&self.pack_type_specific_flags.unwrap_or(0).to_le_bytes());
                pkdt.extend_from_slice(&self.pack_unused2);
            }
            session.doc.set_sub(fid, b"PKDT", pkdt);
        } else {
            session.doc.remove_sub(fid, b"PKDT");
        }

        if self.raw_subs.contains_key(b"PSDT") {
            let mut psdt = Vec::with_capacity(8);
            psdt.push(self.pack_month as u8);
            psdt.push(self.pack_day_of_week as u8);
            psdt.push(self.pack_date);
            psdt.push(self.pack_time as u8);
            psdt.extend_from_slice(&self.pack_duration.to_le_bytes());
            session.doc.set_sub(fid, b"PSDT", psdt);
        } else {
            session.doc.remove_sub(fid, b"PSDT");
        }

        // The conditions are a repeated CTDA, so they go out as a list.
        if self.raw_subs.contains_key(b"CTDA") {
            save_conditions(session, fid, &self.pack_conditions);
        } else {
            session.doc.remove_sub(fid, b"CTDA");
        }

        // The locations and the targets are optional.
        for (sig, loc) in [
            (b"PLDT", &self.pack_location),
            (b"PLD2", &self.pack_location2),
        ] {
            if self.raw_subs.contains_key(sig) {
                let mut data = Vec::with_capacity(12);
                data.extend_from_slice(&loc.kind.to_le_bytes());
                data.extend_from_slice(&loc.target);
                data.resize(12, 0);
                data[8..12].copy_from_slice(&loc.radius.to_le_bytes());
                session.doc.set_sub(fid, sig, data);
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }
        for (sig, target) in [(b"PTDT", &self.pack_target), (b"PTD2", &self.pack_target2)] {
            if self.raw_subs.contains_key(sig) {
                let mut data = Vec::with_capacity(16);
                data.extend_from_slice(&target.kind.to_le_bytes());
                data.extend_from_slice(&target.target);
                data.resize(16, 0);
                data[8..12].copy_from_slice(&target.count_distance.to_le_bytes());
                data[12..16].copy_from_slice(&target.unknown.to_le_bytes());
                session.doc.set_sub(fid, sig, data);
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }

        // The idle animation collection.
        for (sig, present) in [
            (b"IDLF", self.raw_subs.contains_key(b"IDLF")),
            (b"IDLC", self.raw_subs.contains_key(b"IDLC")),
            (b"IDLT", self.raw_subs.contains_key(b"IDLT")),
            (b"IDLA", self.raw_subs.contains_key(b"IDLA")),
            (b"IDLB", self.raw_subs.contains_key(b"IDLB")),
        ] {
            if !present {
                session.doc.remove_sub(fid, sig);
            }
        }
        if self.raw_subs.contains_key(b"IDLF") {
            session
                .doc
                .set_sub(fid, b"IDLF", vec![self.pack_idle_flags]);
        }
        if self.raw_subs.contains_key(b"IDLC") {
            // The count is one byte, but two of the game's records use the
            // four-byte form, so the record's own length decides.
            let long = self.raw_subs.get(b"IDLC").map(|d| d.len()).unwrap_or(1) >= 4;
            let mut idlc = Vec::with_capacity(if long { 4 } else { 1 });
            idlc.push(self.pack_idle_count);
            if long {
                idlc.extend_from_slice(&self.pack_idle_count_unused);
            }
            session.doc.set_sub(fid, b"IDLC", idlc);
        }
        if self.raw_subs.contains_key(b"IDLT") {
            session
                .doc
                .set_sub(fid, b"IDLT", self.pack_idle_timer.to_le_bytes().to_vec());
        }
        if self.raw_subs.contains_key(b"IDLA") {
            let mut idla = Vec::with_capacity(self.pack_idle_animations.len() * 4);
            for id in &self.pack_idle_animations {
                idla.extend_from_slice(&id.to_le_bytes());
            }
            session.doc.set_sub(fid, b"IDLA", idla);
        }
        if self.raw_subs.contains_key(b"IDLB") {
            session
                .doc
                .set_sub(fid, b"IDLB", self.pack_idle_unused.to_vec());
        }

        match self.pack_combat_style {
            Some(v) if self.raw_subs.contains_key(b"CNAM") => {
                session.doc.set_sub(fid, b"CNAM", v.to_le_bytes().to_vec());
            }
            _ => {
                session.doc.remove_sub(fid, b"CNAM");
            }
        }

        // The markers are null subrecords: the record either has them or it
        // does not, and a save does not add one.
        for (sig, present) in [
            (b"PKED", self.pack_eat_marker),
            (b"PUID", self.pack_use_item_marker),
            (b"PKAM", self.pack_ambush_marker),
        ] {
            if present && self.raw_subs.contains_key(sig) {
                session.doc.set_sub(fid, sig, Vec::new());
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }

        for (sig, value) in [
            (
                b"PKE2",
                self.pack_escort_distance.map(|v| v.to_le_bytes().to_vec()),
            ),
            (
                b"PKFD",
                self.pack_follow_radius.map(|v| v.to_le_bytes().to_vec()),
            ),
        ] {
            match value {
                Some(v) if self.raw_subs.contains_key(sig) => {
                    session.doc.set_sub(fid, sig, v);
                }
                _ => {
                    session.doc.remove_sub(fid, sig);
                }
            }
        }

        if self.raw_subs.contains_key(b"PKPT") {
            // The record is two bytes, but two of the game's records use the
            // one-byte form, so the record's own length decides.
            let long = self.raw_subs.get(b"PKPT").map(|d| d.len()).unwrap_or(2) >= 2;
            let flags = self.pack_patrol_flags.unwrap_or(0);
            let data = if long {
                flags.to_le_bytes().to_vec()
            } else {
                vec![flags as u8]
            };
            session.doc.set_sub(fid, b"PKPT", data);
        } else {
            session.doc.remove_sub(fid, b"PKPT");
        }

        if self.raw_subs.contains_key(b"PKW3") {
            let w = &self.pack_weapon;
            let mut data = Vec::with_capacity(24);
            data.extend_from_slice(&w.flags.to_le_bytes());
            data.push(w.fire_rate);
            data.push(w.fire_count);
            data.extend_from_slice(&w.num_bursts.to_le_bytes());
            data.extend_from_slice(&w.shots_min.to_le_bytes());
            data.extend_from_slice(&w.shots_max.to_le_bytes());
            data.extend_from_slice(&w.pause_min.to_le_bytes());
            data.extend_from_slice(&w.pause_max.to_le_bytes());
            data.extend_from_slice(&w.unused);
            session.doc.set_sub(fid, b"PKW3", data);
        } else {
            session.doc.remove_sub(fid, b"PKW3");
        }

        if self.raw_subs.contains_key(b"PKDD") {
            let d = &self.pack_dialog;
            // The dialog data is written back over the record's own bytes, so
            // the length it had and the trailing unnamed bytes survive.
            let mut data = self
                .raw_subs
                .get(b"PKDD")
                .cloned()
                .unwrap_or_else(|| vec![0u8; 24]);
            data.resize(data.len().max(24), 0);
            data[0..4].copy_from_slice(&d.fov.to_le_bytes());
            data[4..8].copy_from_slice(&d.topic.unwrap_or(0).to_le_bytes());
            data[8..12].copy_from_slice(&d.flags.to_le_bytes());
            data[12..16].copy_from_slice(&d.unused);
            data[16..20].copy_from_slice(&d.dialog_type.to_le_bytes());
            data[20..24].copy_from_slice(&d.unused2);
            session.doc.set_sub(fid, b"PKDD", data);
        } else {
            session.doc.remove_sub(fid, b"PKDD");
        }

        // The three embedded scripts are a run of marker, idle, script
        // collection and topic, so they are written as one sequence: a
        // per-signature list edit would scatter the three scripts.
        let had_scripts = self
            .raw_subs
            .keys()
            .any(|s| s == b"POBA" || s == b"POEA" || s == b"POCA");
        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for script in &self.pack_scripts {
            seq.push((script.marker, Vec::new()));
            seq.push((*b"INAM", script.idle.unwrap_or(0).to_le_bytes().to_vec()));
            // The script header: the four unnamed bytes, the reference
            // count, the compiled size, the variable count, the type and the
            // flags. It is written back over its own bytes, so the fields
            // the spec does not name survive.
            let mut schr = script.header_raw.clone();
            schr.resize(20, 0);
            schr[8..12].copy_from_slice(&script.header.unknown.to_le_bytes());
            schr[12..14].copy_from_slice(&(script.header.script_type as u16).to_le_bytes());
            schr[14..16].copy_from_slice(&script.header.flags.to_le_bytes());
            schr[16..20].copy_from_slice(&script.header.version.to_le_bytes());
            seq.push((*b"SCHR", schr));
            if !script.compiled.is_empty() {
                seq.push((*b"SCDA", script.compiled.clone()));
            }
            if !script.source.is_empty() {
                seq.push((*b"SCTX", script.source.as_bytes().to_vec()));
            }
            for var in &script.variables {
                let mut slsd = var.raw.clone();
                slsd.resize(24, 0);
                slsd[0..4].copy_from_slice(&var.index.to_le_bytes());
                slsd[16] = var.flags;
                seq.push((*b"SLSD", slsd));
                seq.push((*b"SCVR", encode_zstring(&var.name)));
                // The references that name this variable follow it.
                for r in &var.refs {
                    seq.push((*b"SCRV", r.to_le_bytes().to_vec()));
                }
            }
            for r in &script.references {
                seq.push((*b"SCRO", r.to_le_bytes().to_vec()));
            }
            seq.push((*b"TNAM", script.topic.unwrap_or(0).to_le_bytes().to_vec()));
        }
        if had_scripts {
            session.doc.set_sub_sequence(
                fid,
                vec![
                    *b"POBA", *b"POEA", *b"POCA", *b"INAM", *b"SCHR", *b"SCDA", *b"SCTX", *b"SLSD",
                    *b"SCVR", *b"SCRO", *b"SCRV", *b"TNAM",
                ],
                seq,
            );
        }
    }

    /// Save a `TES4` (Plugin Header) record.
    fn save_tes4(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        // HEDR is twelve bytes: the version, the record count and the next
        // object id.
        let original = self.raw_subs.get(b"HEDR").cloned();
        let mut hedr = original.unwrap_or_else(|| vec![0u8; 12]);
        hedr.resize(12, 0);
        hedr[0..4].copy_from_slice(&self.tes4_version.to_le_bytes());
        hedr[4..8].copy_from_slice(&self.tes4_num_records.to_le_bytes());
        hedr[8..12].copy_from_slice(&self.tes4_next_object_id.to_le_bytes());
        session.doc.set_sub(fid, b"HEDR", hedr);

        for (sig, text) in [
            (b"CNAM", &self.tes4_author),
            (b"SNAM", &self.tes4_description),
        ] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }

        let mut seq: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for master in &self.tes4_masters {
            seq.push((*b"MAST", encode_zstring(&master.filename)));
            seq.push((*b"SSIG", master.form_id.to_le_bytes().to_vec()));
        }
        if seq.is_empty() {
            session.doc.remove_sub(fid, b"MAST");
            session.doc.remove_sub(fid, b"SSIG");
        } else {
            session
                .doc
                .set_sub_sequence(fid, vec![*b"MAST", *b"SSIG"], seq);
        }
    }

    /// Save a `CHAL` (Challenge) record.
    ///
    /// `DATA` is a twenty-four-byte struct. Every one of the game's 105 CHAL
    /// records carries exactly twenty-four bytes, so a fixed write is
    /// byte-identical to what the record had.
    fn save_chal(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&self.chal_type.to_le_bytes());
        data.extend_from_slice(&self.chal_threshold.to_le_bytes());
        data.extend_from_slice(&self.chal_flags.to_le_bytes());
        data.extend_from_slice(&self.chal_interval.to_le_bytes());
        data.extend_from_slice(&self.chal_value1);
        data.extend_from_slice(&self.chal_value2);
        data.extend_from_slice(&self.chal_value3);
        session.doc.set_sub(fid, b"DATA", data);

        // SNAM and XNAM are the two reference values. Only nine of the
        // game's 105 challenges carry an SNAM and seven carry an XNAM, so
        // both are optional.
        match self.chal_snam {
            Some(v) => session.doc.set_sub(fid, b"SNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"SNAM"),
        }
        match self.chal_xnam {
            Some(v) => session.doc.set_sub(fid, b"XNAM", v.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"XNAM"),
        }

        for (sig, text) in [(b"ICON", &self.chal_icon), (b"MICO", &self.chal_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }

        if !self.chal_desc.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"DESC", encode_zstring(&self.chal_desc));
        } else {
            session.doc.remove_sub(fid, b"DESC");
        }
    }

    /// Save a `CPTH` (Camera Path) record.
    ///
    /// The `CTDA` conditions and the `SNAM` camera shots are both *repeated*
    /// subrecords, so they are written with list edits: a per-subrecord edit
    /// would collapse each list onto its last entry.
    fn save_cpth(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        session
            .doc
            .set_sub(fid, b"DATA", vec![self.cpth_camera_zoom]);

        match self.cpth_parent {
            Some(p) => {
                let mut anam = p.to_le_bytes().to_vec();
                anam.extend_from_slice(&self.cpth_previous.unwrap_or(0).to_le_bytes());
                session.doc.set_sub(fid, b"ANAM", anam);
            }
            None => session.doc.remove_sub(fid, b"ANAM"),
        }

        let shots: Vec<Vec<u8>> = self
            .cpth_shots
            .iter()
            .map(|s| s.to_le_bytes().to_vec())
            .collect();
        if shots.is_empty() {
            session.doc.remove_sub(fid, b"SNAM");
        } else {
            session.doc.set_sub_list(fid, b"SNAM", shots);
        }

        save_conditions(session, fid, &self.cpth_conditions);
    }

    /// Save a `CSNO` (Casino) record.
    ///
    /// The chip models, the slot reel textures and the blackjack deck textures
    /// are all *repeated* subrecords, so each is written with a list edit.
    ///
    /// `DATA` is written at the length the record had: all five of the
    /// game's CSNO records carry fifty-six bytes, but the length is kept so a
    /// short record is not grown.
    fn save_csno(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(56);
        let mut data = original.unwrap_or_else(|| vec![0u8; 56]);
        data.resize(len, 0);

        let put_f32 = |data: &mut Vec<u8>, at: usize, v: f32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };
        let put_u32 = |data: &mut Vec<u8>, at: usize, v: u32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };

        put_f32(&mut data, 0, self.csno_decks_shuffle);
        put_f32(&mut data, 4, self.csno_payout_ratio);
        for (i, stop) in self.csno_reel_stops.iter().enumerate() {
            put_u32(&mut data, 8 + i * 4, *stop);
        }
        put_u32(&mut data, 36, self.csno_number_of_decks);
        put_u32(&mut data, 40, self.csno_max_winnings);
        if let Some(c) = self.csno_currency {
            put_u32(&mut data, 44, c);
        }
        if let Some(q) = self.csno_winnings_quest {
            put_u32(&mut data, 48, q);
        }
        put_u32(&mut data, 52, self.csno_flags);
        session.doc.set_sub(fid, b"DATA", data);

        // The models: the spec names eight `MODL` values (the six chip
        // denominations, the roulette chip and the slot machine), then
        // `MOD2` (the slot machine again), `MOD3` (the blackjack table) and
        // `MOD4` (the roulette table).
        const CSNO_MODL: usize = 8;
        let mods: Vec<Vec<u8>> = self
            .csno_models
            .iter()
            .take(CSNO_MODL)
            .map(|m| encode_zstring(m))
            .collect();
        if mods.is_empty() {
            session.doc.remove_sub(fid, b"MODL");
        } else {
            session.doc.set_sub_list(fid, b"MODL", mods);
        }
        let numbered: Vec<Vec<u8>> = self
            .csno_models
            .iter()
            .skip(CSNO_MODL)
            .take(3)
            .map(|m| encode_zstring(m))
            .collect();
        for (i, sig) in [b"MOD2", b"MOD3", b"MOD4"].iter().enumerate() {
            if let Some(m) = numbered.get(i) {
                session.doc.set_sub(fid, sig, m.clone());
            } else {
                session.doc.remove_sub(fid, sig);
            }
        }

        let icons: Vec<Vec<u8>> = self.csno_icons.iter().map(|i| encode_zstring(i)).collect();
        if icons.is_empty() {
            session.doc.remove_sub(fid, b"ICON");
        } else {
            session.doc.set_sub_list(fid, b"ICON", icons);
        }

        let decks: Vec<Vec<u8>> = self
            .csno_deck_icons
            .iter()
            .map(|i| encode_zstring(i))
            .collect();
        if decks.is_empty() {
            session.doc.remove_sub(fid, b"ICO2");
        } else {
            session.doc.set_sub_list(fid, b"ICO2", decks);
        }
    }

    /// Save a `DEBR` (Debris) record.
    ///
    /// The whole collection is rewritten as one sequence, because the debris
    /// models are not a list of one signature: each is a run of `DATA` and
    /// `MODT` that must keep their order. A per-signature list edit would
    /// cluster every `DATA` together and every `MODT` together, which the
    /// engine cannot read back into models.
    fn save_debr(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut subs: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for model in &self.debr_models {
            let mut data = vec![model.percentage];
            data.extend_from_slice(&encode_zstring(&model.model));
            data.push(model.flags);
            subs.push((*b"DATA", data));
            if !model.texture_hashes.is_empty() {
                subs.push((*b"MODT", model.texture_hashes.clone()));
            }
        }
        session
            .doc
            .set_sub_sequence(fid, vec![*b"DATA", *b"MODT"], subs);
    }

    /// Save a `CAMS` (Camera Shot) record.
    ///
    /// `DATA` is written at the length the record had: 272 of the game's 276
    /// records carry forty bytes and four carry thirty-six, and a fixed
    /// forty-byte write would grow the short ones.
    fn save_cams(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let original = self.raw_subs.get(b"DATA").cloned();
        let len = original.as_ref().map(|b| b.len()).unwrap_or(40);
        let mut data = original.unwrap_or_else(|| vec![0u8; 40]);
        data.resize(len, 0);

        let put_u32 = |data: &mut Vec<u8>, at: usize, v: u32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };
        let put_f32 = |data: &mut Vec<u8>, at: usize, v: f32| {
            if data.len() >= at + 4 {
                data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };

        put_u32(&mut data, 0, self.cams_action);
        put_u32(&mut data, 4, self.cams_location);
        put_u32(&mut data, 8, self.cams_target);
        put_u32(&mut data, 12, self.cams_flags);
        put_f32(&mut data, 16, self.cams_player_time_mult);
        put_f32(&mut data, 20, self.cams_target_time_mult);
        put_f32(&mut data, 24, self.cams_global_time_mult);
        put_f32(&mut data, 28, self.cams_max_time);
        put_f32(&mut data, 32, self.cams_min_time);
        put_f32(&mut data, 36, self.cams_target_percent);
        session.doc.set_sub(fid, b"DATA", data);

        // MNAM: the image space modifier.
        match self.cams_image_space_modifier {
            Some(m) => session.doc.set_sub(fid, b"MNAM", m.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"MNAM"),
        }

        // The model data collection.
        let model = &self.cams_model;
        let sigs = model.instance_sigs();
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
    }

    /// Save a `CCRD` (Card) record.
    ///
    /// The two `INTV` values are a *repeated* subrecord, so they are written
    /// with a list edit: a per-subrecord edit would collapse the pair onto
    /// the card value, and the card would lose its suit.
    fn save_ccrd(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let intv: Vec<Vec<u8>> = self
            .ccrd_intv
            .iter()
            .map(|v| v.to_le_bytes().to_vec())
            .collect();
        if intv.is_empty() {
            session.doc.remove_sub(fid, b"INTV");
        } else {
            session.doc.set_sub_list(fid, b"INTV", intv);
        }

        session
            .doc
            .set_sub(fid, b"DATA", self.ccrd_value.to_le_bytes().to_vec());

        for (sig, text) in [
            (b"ICON", &self.ccrd_icon),
            (b"MICO", &self.ccrd_mico),
            (b"TX00", &self.ccrd_face),
            (b"TX01", &self.ccrd_back),
        ] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }

        match self.ccrd_pickup_sound {
            Some(s) => session.doc.set_sub(fid, b"YNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"YNAM"),
        }
        match self.ccrd_drop_sound {
            Some(s) => session.doc.set_sub(fid, b"ZNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"ZNAM"),
        }

        let model = &self.ccrd_model;
        let sigs = model.instance_sigs();
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
    }

    /// Save a `CDCK` (Caravan Deck) record.
    ///
    /// The deck's `CARD` subrecords are *repeated*, so they are written with a
    /// list edit: a per-subrecord edit would collapse the deck onto its last
    /// card, and the deck would lose fifty-one of its fifty-two.
    fn save_cdck(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let cards: Vec<Vec<u8>> = self
            .cdck_cards
            .iter()
            .map(|c| c.to_le_bytes().to_vec())
            .collect();
        if cards.is_empty() {
            session.doc.remove_sub(fid, b"CARD");
        } else {
            session.doc.set_sub_list(fid, b"CARD", cards);
        }

        // DATA: the spec calls this value broken, so it is carried through
        // exactly as the record had it.
        let original = self.raw_subs.get(b"DATA").cloned();
        let mut data = original.unwrap_or_else(|| vec![0u8; 4]);
        data.resize(4, 0);
        data[0..4].copy_from_slice(&self.cdck_data.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);
    }

    /// Save a `CHIP` (Casino Chip) record.
    fn save_chip(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        for (sig, text) in [(b"ICON", &self.chip_icon), (b"MICO", &self.chip_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }

        match self.chip_pickup_sound {
            Some(s) => session.doc.set_sub(fid, b"YNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"YNAM"),
        }
        match self.chip_drop_sound {
            Some(s) => session.doc.set_sub(fid, b"ZNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"ZNAM"),
        }

        let model = &self.chip_model;
        let sigs = model.instance_sigs();
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

        // The destruction data.
        save_destruction(
            session,
            fid,
            self.chip_dest_health,
            self.chip_dest_flags,
            &self.chip_destruction,
        );
    }

    /// Save a `CMNY` (Money) record.
    fn save_cmny(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        session
            .doc
            .set_sub(fid, b"DATA", self.cmny_value.to_le_bytes().to_vec());

        for (sig, text) in [(b"ICON", &self.cmny_icon), (b"MICO", &self.cmny_mico)] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            }
        }

        match self.cmny_pickup_sound {
            Some(s) => session.doc.set_sub(fid, b"YNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"YNAM"),
        }
        match self.cmny_drop_sound {
            Some(s) => session.doc.set_sub(fid, b"ZNAM", s.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"ZNAM"),
        }

        let model = &self.cmny_model;
        let sigs = model.instance_sigs();
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
    }

    /// Save an `ARMA` (Armor Addon) record.
    ///
    /// `DNAM` is written back over the record's own bytes, not rebuilt from
    /// the fields the editor owns. 115 of the 131 ARMA records in
    /// FalloutNV.esm carry a four-byte `DNAM` — the armour rating and the flag
    /// word, with no `DT` — and a rebuild would grow them to twelve bytes and
    /// give them fields they never had. The length the record had is kept, so
    /// a save that changes nothing is byte-identical.
    ///
    /// `BMDT` is likewise written whole: the general-flags byte is copied
    /// through and only the named bits are written inside it, so "Non-Playable"
    /// — set on records the spec does not name it for — survives a save.
    fn save_arma(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&self.arma_val.to_le_bytes());
        data.extend_from_slice(&self.arma_hp.to_le_bytes());
        data.extend_from_slice(&self.arma_weight.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", data);

        // DNAM: the record's own bytes, with only the fields the editor owns
        // written over them.
        let original_dnam = self.raw_subs.get(b"DNAM").cloned();
        let dnam_len = if self.arma_dnam_len >= 8 { 12 } else { 4 };
        let mut dnam = original_dnam.unwrap_or_else(|| vec![0u8; dnam_len]);
        dnam.resize(dnam_len, 0);
        dnam[0..2].copy_from_slice(&self.arma_dr.to_le_bytes());
        if dnam_len >= 4 {
            dnam[2..4].copy_from_slice(&self.arma_dnam_flags.to_le_bytes());
        }
        if dnam_len >= 8 {
            dnam[4..8].copy_from_slice(&self.arma_dt.to_le_bytes());
        }
        session.doc.set_sub(fid, b"DNAM", dnam);

        // BMDT: the biped flag word and the general flag byte, with the
        // general byte's unnamed bits preserved.
        let original_bmdt = self.raw_subs.get(b"BMDT").cloned();
        let mut bmdt = original_bmdt.unwrap_or_else(|| vec![0u8; 8]);
        bmdt.resize(8, 0);
        bmdt[0..4].copy_from_slice(&self.arma_biped_flags.to_le_bytes());
        bmdt[4] = self.arma_bmdt_general;
        session.doc.set_sub(fid, b"BMDT", bmdt);

        // ETYP: the equipment type.
        session
            .doc
            .set_sub(fid, b"ETYP", self.arma_equip_type.to_le_bytes().to_vec());

        // The four model data instances.
        for model in &self.arma_models {
            let sigs = model.instance_sigs();
            if !model.modl.trim().is_empty() {
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
        }

        // The four icons.
        for (sig, text) in [
            (b"ICON", &self.arma_icon),
            (b"MICO", &self.arma_mico),
            (b"ICO2", &self.arma_ico2),
            (b"MIC2", &self.arma_mic2),
        ] {
            if text.trim().is_empty() {
                session.doc.remove_sub(fid, sig);
            } else {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            }
        }
    }

    /// Save an `ASPC` (Ambient Sound) record.
    ///
    /// The five `SNAM` sound loops are *repeated* subrecords, so they are
    /// written with a list edit: a per-subrecord edit would collapse them
    /// onto the last one, and the record would lose four of its five loops.
    fn save_aspc(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let payload: Vec<Vec<u8>> = self
            .aspc_sounds
            .iter()
            .map(|s| s.to_le_bytes().to_vec())
            .collect();
        if payload.is_empty() {
            session.doc.remove_sub(fid, b"SNAM");
        } else {
            session.doc.set_sub_list(fid, b"SNAM", payload);
        }

        session.doc.set_sub(
            fid,
            b"WNAM",
            self.aspc_walla_trigger_count.to_le_bytes().to_vec(),
        );
        match self.aspc_region {
            Some(r) => session.doc.set_sub(fid, b"RDAT", r.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"RDAT"),
        }
        session
            .doc
            .set_sub(fid, b"ANAM", self.aspc_environment.to_le_bytes().to_vec());
        session
            .doc
            .set_sub(fid, b"INAM", self.aspc_is_interior.to_le_bytes().to_vec());
    }

    /// Save an `AMEF` (Ammo Effect) record.
    ///
    /// `DATA` is a twelve-byte struct: a type enum, an operation enum and a
    /// float32 value. All 54 of the game's AMEF records carry exactly twelve
    /// bytes, so a fixed twelve-byte write is byte-identical to what the
    /// record had.
    fn save_amef(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let mut d = Vec::with_capacity(12);
        d.extend_from_slice(&self.amef_type.to_le_bytes());
        d.extend_from_slice(&self.amef_operation.to_le_bytes());
        d.extend_from_slice(&self.amef_value.to_le_bytes());
        session.doc.set_sub(fid, b"DATA", d);
    }

    /// Save an `ANIO` (Animation Object) record.
    ///
    /// `DATA` names the `IDLE` record the animation plays. Every one of the
    /// game's ANIO records carries it, so it is always written.
    ///
    /// The model data collection's subrecords are written individually. `MODT`'s
    /// texture hashes and `MODB`'s unknown bytes are copied through unchanged
    /// — the editor cannot author a file hash, and rewriting one would
    /// corrupt the mod.
    fn save_anio(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;
        let model = &self.anio_model;
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

        // DATA: the animation.
        match self.anio_animation {
            Some(f) => session.doc.set_sub(fid, b"DATA", f.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"DATA"),
        }
    }

    /// Save an `ALOC` (Media Location Controller) record.
    ///
    /// The six media-set lists are *repeated* subrecords, so each is written
    /// with a list edit: a per-subrecord edit would collapse the list onto its
    /// last entry, and the game would read one media set where the record had
    /// sixteen.
    ///
    /// `NAM1`, `NAM2`, `NAM3` and `FNAM` are four bytes each that the spec
    /// does not name. They are written back exactly as the record had them.
    fn save_aloc(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        let mut nam1 = self.aloc_nam1.to_vec();
        let mut nam2 = self.aloc_nam2.to_vec();
        let mut nam3 = self.aloc_nam3.to_vec();
        nam1.resize(4, 0);
        nam2.resize(4, 0);
        nam3.resize(4, 0);
        session.doc.set_sub(fid, b"NAM1", nam1);
        session.doc.set_sub(fid, b"NAM2", nam2);
        session.doc.set_sub(fid, b"NAM3", nam3);

        session.doc.set_sub(
            fid,
            b"NAM4",
            self.aloc_location_delay.to_le_bytes().to_vec(),
        );
        session
            .doc
            .set_sub(fid, b"NAM5", self.aloc_day_start.to_le_bytes().to_vec());
        session
            .doc
            .set_sub(fid, b"NAM6", self.aloc_night_start.to_le_bytes().to_vec());
        session.doc.set_sub(
            fid,
            b"NAM7",
            self.aloc_retrigger_delay.to_le_bytes().to_vec(),
        );

        let mut fnam = self.aloc_fnam.to_vec();
        fnam.resize(4, 0);
        session.doc.set_sub(fid, b"FNAM", fnam);

        // The conditional faction is optional: 8 of the game's 89 ALOC records
        // carry one, the rest do not.
        match self.aloc_conditional_faction {
            Some(f) => session.doc.set_sub(fid, b"RNAM", f.to_le_bytes().to_vec()),
            None => session.doc.remove_sub(fid, b"RNAM"),
        }

        // The six media-set lists.
        for list in &self.aloc_sets {
            let payload: Vec<Vec<u8>> = list
                .entries
                .iter()
                .map(|e| e.to_le_bytes().to_vec())
                .collect();
            if payload.is_empty() {
                session.doc.remove_sub(fid, &list.sig);
            } else {
                session.doc.set_sub_list(fid, &list.sig, payload);
            }
        }
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
        for (sig, text) in [(b"DESC", &self.avif_desc), (b"ANAM", &self.avif_short_name)] {
            if self.raw_subs.contains_key(sig) {
                session.doc.set_sub(
                    fid,
                    sig,
                    encode_text_kept(self.raw_subs.get(sig).map(|v| v.as_slice()), text),
                );
            } else if !text.trim().is_empty() {
                session.doc.set_sub(fid, sig, encode_zstring(text));
            } else {
                session.doc.remove_sub(fid, sig);
            }
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

/// Encode a text field for a subrecord, keeping the record's own bytes when
/// the text has not been edited.
///
/// The game's records hold text that is not valid UTF-8 — a lone accented
/// byte in a name, say. Decoding that as UTF-8 replaces the bad bytes with
/// the replacement character, and writing them back would change the
/// record. So a field the modder has not touched is written out byte for
/// byte, and only an edited one is re-encoded.
fn encode_text_kept(original: Option<&[u8]>, text: &str) -> Vec<u8> {
    match original {
        Some(orig) if zstring(orig) == text => orig.to_vec(),
        _ => encode_zstring(text),
    }
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
        b"ALOC" => "Media Location Controller",
        b"AMEF" => "Ammo Effect",
        b"ARMA" => "Armor Addon",
        b"CAMS" => "Camera Shot",
        b"CHAL" => "Challenge",
        b"KEYM" => "Key",
        b"INGR" => "Ingredient",
        b"IPCT" => "Impact",
        b"IPDS" => "Impact Data Set",
        b"RCPE" => "Recipe",
        b"SCPT" => "Script",
        b"PACK" => "Package",
        b"EFSH" => "Effect Shader",
        b"REGN" => "Region",
        b"IMAD" => "Image Space Adapter",
        b"COBJ" => "Constructible Object",
        b"PMIS" => "Placed Missile",
        b"TES4" => "Plugin Header",
        b"FLST" => "Form List",
        b"MESG" => "Message",
        b"MISC" => "Miscellaneous",
        b"SPEL" => "Spell",
        b"STAT" => "Static",
        b"IDLE" => "Idle Animation",
        b"MSET" => "Media Set",
        b"MSTT" => "Movable Static",
        b"PWAT" => "Water",
        b"RCCT" => "Recipe Category",
        b"REPU" => "Reputation",
        b"RGDL" => "Ragdoll",
        b"TREE" => "Tree",
        b"TXST" => "Texture",
        b"VTYP" => "Voice Type",
        b"TACT" => "Talking Activator",
        b"SCOL" => "Static Collision",
        b"IDLM" => "Idle Animation",
        b"IMGS" => "Image Space",
        b"LGTM" => "Lighting",
        b"LSCR" => "Load Screen",
        b"LSCT" => "Load Screen Type",
        b"MICN" => "Mission Icon",
        b"MUSC" => "Music",
        b"DEHY" => "Dehydration",
        b"DOBJ" => "Default Object Manager",
        b"ECZN" => "Encounter Zone",
        b"FURN" => "Furniture",
        b"GRAS" => "Grass",
        b"HUNG" => "Hunger",
        b"RADS" => "Radiation",
        b"SLPD" => "Sleep Deprivation",
        b"CPTH" => "Camera Path",
        b"CSNO" => "Casino",
        b"DEBR" => "Debris",
        b"CCRD" => "Card",
        b"CDCK" => "Caravan Deck",
        b"CHIP" => "Casino Chip",
        b"CMNY" => "Money",
        b"ASPC" => "Ambient Sound",
        b"ANIO" => "Animation Object",
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

/// The thirty-four `DOBJ` (Default Object Manager) fields, in the order the
/// record stores them.
///
/// The `DATA` subrecord is a flat array of FormIDs; the spec names each one.
/// The names are kept in a table so the editor can label them and so the
/// order is stated once rather than implied by the loader.
pub const DOBJ_FIELDS: [&str; 34] = [
    "Stimpack",
    "SuperStimpack",
    "RadX",
    "RadAway",
    "Morphine",
    "Perk Paralysis",
    "Player Faction",
    "Mysterious Stranger NPC",
    "Mysterious Stranger Faction",
    "Default Music",
    "Battle Music",
    "Death Music",
    "Success Music",
    "Level Up Music",
    "Player Voice (Male)",
    "Player Voice (Male Child)",
    "Player Voice (Female)",
    "Player Voice (Female Child)",
    "Eat Package Default Food",
    "Every Actor Ability",
    "Drug Wears Off Image Space",
    "Doctor's Bag",
    "Miss Fortune NPC",
    "Miss Fortune Faction",
    "Meltdown Explosion",
    "Unarmed Forward PA",
    "Unarmed Backward PA",
    "Unarmed Left PA",
    "Unarmed Right PA",
    "Unarmed Crouch PA",
    "Unarmed Counter PA",
    "Spotter Effect",
    "Item Detected Effect",
    "Cateye Mobile Effect (NYI)",
];

/// `GRAS` `DATA` Unit From Water Type, the uint32 enum from fopdoc's GRAS page.
const GRAS_WATER_TYPES: [(u32, &str); 8] = [
    (0, "Above - At Least"),
    (1, "Above - At Most"),
    (2, "Below - At Least"),
    (3, "Below - At Most"),
    (4, "Either - At Least"),
    (5, "Either - At Most"),
    (6, "Either - At Most Above"),
    (7, "Either - At Most Below"),
];

/// `GRAS` `DATA` Flags, the u8 word from fopdoc's GRAS page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const GRAS_FLAGS: [(u8, &str); 3] = [
    (0x01, "Vertex Lighting"),
    (0x02, "Uniform Scaling"),
    (0x04, "Fit To Slope"),
];

/// `ECZN` `DATA` Flags, the u8 word from fopdoc's ECZN page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const ECZN_FLAGS: [(u8, &str); 2] = [
    (0x01, "Never Resets"),
    (0x02, "Match PC Below Minimum Level"),
];

/// `IDLM` `IDLF` Flags, the u8 word from fopdoc's IDLM page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const IDLM_FLAGS: [(u8, &str); 3] = [
    (0x01, "Run In Sequence"),
    (0x02, "Unknown 0x02"),
    (0x04, "Do Once"),
];

/// `LSCT` `DATA` Type and Stats, the uint32 enum from fopdoc's LSCT page.
const LSCT_TYPES: [(u32, &str); 5] = [
    (0, "None"),
    (1, "XP Progress"),
    (2, "Objective"),
    (3, "Tip"),
    (4, "Stats"),
];

/// `LSCT` `DATA` Font, the uint32 enum from fopdoc's LSCT page.
const LSCT_FONTS: [(u32, &str); 8] = [
    (0, "Unknown"),
    (1, "2"),
    (2, "3"),
    (3, "4"),
    (4, "5"),
    (5, "6"),
    (6, "7"),
    (7, "8"),
];

/// `LSCT` `DATA` Font Alignment, the uint32 enum from fopdoc's LSCT page.
const LSCT_ALIGNMENTS: [(u32, &str); 5] = [
    (0, "Unknown"),
    (1, "Left"),
    (2, "Center"),
    (3, "Unknown"),
    (4, "Right"),
];

/// `RCCT` `DATA` Flags, the u8 word from fopdoc's RCCT page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const RCCT_FLAGS: [(u8, &str); 8] = [
    (0x01, "Subcategory"),
    (0x02, "Unknown 0x02"),
    (0x04, "Unknown 0x04"),
    (0x08, "Unknown 0x08"),
    (0x10, "Unknown 0x10"),
    (0x20, "Unknown 0x20"),
    (0x40, "Unknown 0x40"),
    (0x80, "Unknown 0x80"),
];

/// `MESG` `DNAM` Flags, the uint32 word from fopdoc's MESG page.
const MESG_FLAGS: [(u32, &str); 2] = [(0x0000_0001, "Message Box"), (0x0000_0002, "Auto Display")];

/// `SPEL` `SPIT` Type, the uint32 enum from fopdoc's SPEL page.
const SPEL_TYPES: [(u32, &str); 11] = [
    (0, "Actor Effect"),
    (1, "Disease"),
    (2, "Power"),
    (3, "Lesser Power"),
    (4, "Ability"),
    (5, "Poison"),
    (6, "Unknown 6"),
    (7, "Unknown 7"),
    (8, "Unknown 8"),
    (9, "Unknown 9"),
    (10, "Addiction"),
];

/// `SPEL` `SPIT` Flags, the u8 word from fopdoc's SPEL page.
const SPEL_FLAGS: [(u8, &str); 8] = [
    (0x01, "No Auto-Calc"),
    (0x02, "Immune To Silence 1"),
    (0x04, "PC Start Effect"),
    (0x08, "Immune To Silence 2"),
    (0x10, "Area Effect Ignores LOS"),
    (0x20, "Script Effect Always Applies"),
    (0x40, "Disable Absorb / Reflect"),
    (0x80, "Force Touch Explode"),
];

/// `STAT` `BRUS` Passthrough Sound, the int8 enum from fopdoc's STAT page.
const STAT_BUSHS: [(i8, &str); 11] = [
    (-1, "None"),
    (0, "BushA"),
    (1, "BushB"),
    (2, "BushC"),
    (3, "BushD"),
    (4, "BushE"),
    (5, "BushF"),
    (6, "BushG"),
    (7, "BushH"),
    (8, "BushI"),
    (9, "BushJ"),
];

/// `ETYP` Equipment Type, the int32 enum from fopdoc's ETYP page.
const ETYP_TYPES: [(i32, &str); 15] = [
    (-1, "None"),
    (0, "Big Guns"),
    (1, "Energy Weapons"),
    (2, "Small Guns"),
    (3, "Melee Weapons"),
    (4, "Unarmed Weapon"),
    (5, "Thrown Weapons"),
    (6, "Mine"),
    (7, "Body Wear"),
    (8, "Head Wear"),
    (9, "Hand Wear"),
    (10, "Chems"),
    (11, "Stimpack"),
    (12, "Food"),
    (13, "Alcohol"),
];

/// `INGR` `ENIT` Flags, the u8 word from fopdoc's INGR page.
const INGR_EFFECT_FLAGS: [(u8, &str); 2] = [(0x01, "No Auto-Calculation"), (0x02, "Food Item")];

/// `IPCT` `DATA` Effect - Orientation, the uint32 enum from fopdoc's IPCT
/// page.
const IPCT_ORIENTATIONS: [(u32, &str); 3] = [
    (0, "Surface Normal"),
    (1, "Projectile Vector"),
    (2, "Projectile Reflection"),
];

/// `SCPT` `SCHR` Type, the uint32 enum from fopdoc's Script page.
const SCPT_TYPES: [(u32, &str); 4] = [(0, "Unknown"), (1, "Object"), (2, "Quest"), (3, "Menu")];

/// `IDLE` `DATA` Animation Group Section, the u8 enum from fopdoc's IDLE
/// page.
const IDLE_GROUPS: [(u8, &str); 6] = [
    (0, "Idle"),
    (1, "Movement"),
    (2, "Left Arm"),
    (3, "Left Hand"),
    (4, "Weapon"),
    (5, "Weapon Up"),
];

/// `MSET` `NAM1` Type, the int32 enum from fopdoc's MSET page.
const MSET_TYPES: [(i32, &str); 5] = [
    (-1, "No Set"),
    (0, "Battle Set"),
    (1, "Location Set"),
    (2, "Dungeon Set"),
    (3, "Incidental Set"),
];

/// `MSET` `PNAM` Enable Flags, the u8 word from fopdoc's MSET page.
const MSET_ENABLE_FLAGS: [(u8, &str); 6] = [
    (0x01, "Day Outer"),
    (0x02, "Day Middle"),
    (0x04, "Day Inner"),
    (0x08, "Night Outer"),
    (0x10, "Night Middle"),
    (0x20, "Night Inner"),
];

/// `PWAT` `DNAM` Flags, the uint32 word from fopdoc's PWAT page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const PWAT_FLAGS: [(u32, &str); 18] = [
    (0x0000_0001, "Reflects"),
    (0x0000_0002, "Reflects - Actors"),
    (0x0000_0004, "Reflects - Land"),
    (0x0000_0008, "Reflects - LOD Land"),
    (0x0000_0010, "Reflects - LOD Buildings"),
    (0x0000_0020, "Reflects - Trees"),
    (0x0000_0040, "Reflects - Sky"),
    (0x0000_0080, "Reflects - Dynamic Objects"),
    (0x0000_0100, "Reflects - Dead Bodies"),
    (0x0000_0200, "Refracts"),
    (0x0000_0400, "Refracts - Actors"),
    (0x0000_0800, "Refracts - Land"),
    (0x0000_1000, "Unknown 0x1000"),
    (0x0000_2000, "Unknown 0x2000"),
    (0x0000_4000, "Unknown 0x4000"),
    (0x0000_8000, "Unknown 0x8000"),
    (0x0001_0000, "Refracts - Dynamic Objects"),
    (0x0002_0000, "Unknown 0x20000"),
];

/// `CTDA` Type — the comparison bits, from fopdoc's CTDA page.
///
/// The comparison lives in the high bits of the type word, so the editor
/// masks the word rather than replacing it.
const CTDA_COMPARISONS: [(u8, &str); 6] = [
    (0x00, "Equal To"),
    (0x20, "Not Equal To"),
    (0x40, "Greater Than"),
    (0x60, "Greater Than or Equal To"),
    (0x80, "Less Than"),
    (0xA0, "Less Than or Equal To"),
];

/// `TXST` `DODT` Flags, the u8 word from fopdoc's DODT page.
const TXST_DODT_FLAGS: [(u8, &str); 3] = [
    (0x01, "Parallax"),
    (0x02, "Alpha - Blending"),
    (0x04, "Alpha - Testing"),
];

/// `VTYP` `DNAM` Flags, the u8 word from fopdoc's VTYP page.
const VTYP_FLAGS: [(u8, &str); 2] = [(0x01, "Allow Default Dialog"), (0x02, "Female")];

/// `CHAL` `DATA` Type, the uint32 enum from fopdoc's CHAL page.
const CHAL_TYPES: [(u32, &str); 14] = [
    (0, "Kill From A Form List"),
    (1, "Kill A Specific FormID"),
    (2, "Kill Any In A Category"),
    (3, "Hit An Enemy"),
    (4, "Discover A Map Marker"),
    (5, "Use An Item"),
    (6, "Acquire An Item"),
    (7, "Use A Skill"),
    (8, "Do Damage"),
    (9, "Use An Item From A List"),
    (10, "Acquire An Item From A List"),
    (11, "Miscellaneous Stat"),
    (12, "Craft Using An Item"),
    (13, "Scripted Challenge"),
];

/// `CHAL` `DATA` Flags, the uint32 word from fopdoc's CHAL page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const CHAL_FLAGS: [(u32, &str); 3] = [
    (0x0000_0001, "Start Disabled"),
    (0x0000_0002, "Recurring"),
    (0x0000_0004, "Show Zero Progress"),
];

/// `CPTH` `DATA` Camera Zoom, the u8 enum from fopdoc's CPTH page.
const CPTH_ZOOMS: [(u8, &str); 3] = [(0, "Default"), (1, "Disable"), (2, "Shot List")];

/// `CAMS` `DATA` Action, the uint32 enum from fopdoc's CAMS page.
const CAMS_ACTIONS: [(u32, &str); 4] = [(0, "Shoot"), (1, "Fly"), (2, "Hit"), (3, "Zoom")];

/// `CAMS` `DATA` Location and Target, the uint32 enum from fopdoc's CAMS page.
const CAMS_ROLES: [(u32, &str); 3] = [(0, "Attacker"), (1, "Projectile"), (2, "Target")];

/// `CAMS` `DATA` Flags, the uint32 word from fopdoc's CAMS page.
///
/// The word is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const CAMS_FLAGS: [(u32, &str); 6] = [
    (0x0000_0001, "Position Follows Location"),
    (0x0000_0002, "Rotation Follows Target"),
    (0x0000_0004, "Don't Follow Bone"),
    (0x0000_0008, "First Person Camera"),
    (0x0000_0010, "No Tracer"),
    (0x0000_0020, "Start At Time Zero"),
];

/// `CCRD` `INTV` Card Suit, the uint32 enum from fopdoc's CCRD page.
const CCRD_SUITS: [(u32, &str); 6] = [
    (0, "Unspecified"),
    (1, "Hearts"),
    (2, "Spades"),
    (3, "Diamonds"),
    (4, "Clubs"),
    (5, "Joker"),
];

/// `CCRD` `INTV` Card Value, the uint32 enum from fopdoc's CCRD page.
const CCRD_VALUES: [(u32, &str); 16] = [
    (0, "Unspecified"),
    (1, "Ace"),
    (2, "2"),
    (3, "3"),
    (4, "4"),
    (5, "5"),
    (6, "6"),
    (7, "7"),
    (8, "8"),
    (9, "9"),
    (10, "10"),
    (11, "Unspecified"),
    (12, "Jack"),
    (13, "Queen"),
    (14, "King"),
    (15, "Joker"),
];

/// `ASPC` `ANAM` Environment Type, the uint32 enum from fopdoc's ASPC page.
///
/// The whole table is listed, not a subset: truncating an enum corrupts the
/// records that carry a value past the end of the list.
const ASPC_ENVIRONMENTS: [(u32, &str); 31] = [
    (0, "None"),
    (1, "Default"),
    (2, "Generic"),
    (3, "Padded Cell"),
    (4, "Room"),
    (5, "Bathroom"),
    (6, "Livingroom"),
    (7, "Stone Room"),
    (8, "Auditorium"),
    (9, "Concerthall"),
    (10, "Cave"),
    (11, "Arena"),
    (12, "Hangar"),
    (13, "Carpeted Hallway"),
    (14, "Hallway"),
    (15, "Stone Corridor"),
    (16, "Alley"),
    (17, "Forest"),
    (18, "City"),
    (19, "Mountains"),
    (20, "Quarry"),
    (21, "Plain"),
    (22, "Parkinglot"),
    (23, "Sewerpipe"),
    (24, "Underwater"),
    (25, "Small Room"),
    (26, "Medium Room"),
    (27, "Large Room"),
    (28, "Medium Hall"),
    (29, "Large Hall"),
    (30, "Plate"),
];

/// `ASPC` `INAM` Is Interior, the uint32 enum from fopdoc's ASPC page.
const ASPC_INTERIOR: [(u32, &str); 2] = [(1, "No"), (2, "Yes")];

/// The five `SNAM` sound loops an `ASPC` record carries, in the order the
/// record stores them.
const ASPC_SOUND_SLOTS: [&str; 5] = ["Dawn / Default Loop", "Afternoon", "Dusk", "Night", "Walla"];

/// `BMDT` biped flags, the uint32 word from fopdoc's BMDT page.
const BMDT_BIPED_FLAGS: [(u32, &str); 20] = [
    (0x0000_0001, "Head"),
    (0x0000_0002, "Hair"),
    (0x0000_0004, "Upper Body"),
    (0x0000_0008, "Left Hand"),
    (0x0000_0010, "Right Hand"),
    (0x0000_0020, "Weapon"),
    (0x0000_0040, "PipBoy"),
    (0x0000_0080, "Backpack"),
    (0x0000_0100, "Necklace"),
    (0x0000_0200, "Headband"),
    (0x0000_0400, "Hat"),
    (0x0000_0800, "Eye Glasses"),
    (0x0000_1000, "Nose Ring"),
    (0x0000_2000, "Earrings"),
    (0x0000_4000, "Mask"),
    (0x0000_8000, "Choker"),
    (0x0001_0000, "Mouth Object"),
    (0x0002_0000, "Body AddOn 1"),
    (0x0004_0000, "Body AddOn 2"),
    (0x0008_0000, "Body AddOn 3"),
];

/// `BMDT` general flags, the u8 byte from fopdoc's BMDT page.
///
/// The byte is kept whole when saving, so a bit the spec does not name
/// survives an edit.
const BMDT_GENERAL_FLAGS: [(u8, &str); 8] = [
    (0x01, "Unknown 0x01"),
    (0x02, "Unknown 0x02"),
    (0x04, "Has Backpack"),
    (0x08, "Medium"),
    (0x10, "Unknown 0x10"),
    (0x20, "Power Armor"),
    (0x40, "Non-Playable"),
    (0x80, "Heavy"),
];

/// `ARMA` `DNAM` Flags, the uint16 word from fopdoc's ARMA page.
///
/// Only bit 0 is named by the spec, but the word is kept whole, so a mod's
/// other bits survive an edit.
const ARMA_DNAM_FLAGS: [(u16, &str); 1] = [(0x0001, "Modulates Voice")];

/// `AMEF` `DATA` Type, the uint32 enum from fopdoc's AMEF page.
const AMEF_TYPES: [(u32, &str); 6] = [
    (0, "Damage Mod"),
    (1, "DR Mod"),
    (2, "DT Mod"),
    (3, "Spread Mod"),
    (4, "Weapon Condition Mod"),
    (5, "Fatigue Mod"),
];

/// `AMEF` `DATA` Operation, the uint32 enum from fopdoc's AMEF page.
const AMEF_OPERATIONS: [(u32, &str); 3] = [(0, "Add"), (1, "Multiply"), (2, "Subtract")];

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
                b"CREA" => draw_crea_ui(ui, &session, &mut state),
                b"NPC_" => draw_npc_ui(ui, &session, &mut state),
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
                b"ENCH" => draw_spell_ui(ui, &mut state),
                b"EXPL" => draw_expl_ui(ui, &mut state),
                b"PROJ" => draw_proj_ui(ui, &mut state),
                b"SOUN" => draw_soun_ui(ui, &mut state),
                b"NOTE" => draw_note_ui(ui, &mut state),
                b"LVLC" => draw_lvlc_ui(ui, &session, &mut state),
                b"LVLN" => draw_lvln_ui(ui, &session, &mut state),
                b"LVLI" => draw_lvli_ui(ui, &session, &mut state),
                b"IMOD" => draw_imod_ui(ui, &mut state),
                b"CLAS" => draw_clas_ui(ui, &mut state),
                b"CSTY" => draw_csty_ui(ui, &mut state),
                b"MGEF" => draw_mgef_ui(ui, &mut state),
                b"WTHR" => draw_wthr_ui(ui, &session, &mut state),
                b"WATR" => draw_watr_ui(ui, &session, &mut state),
                b"GLOB" => draw_glob_ui(ui, &mut state),
                b"GMST" => draw_gmst_ui(ui, &mut state),
                b"RACE" => draw_race_ui(ui, &mut state),
                b"CLMT" => draw_clmt_ui(ui, &mut state),
                b"ADDN" => draw_addn_ui(ui, &mut state),
                b"ALOC" => draw_aloc_ui(ui, &session, &mut state),
                b"AMEF" => draw_amef_ui(ui, &mut state),
                b"ARMA" => draw_arma_ui(ui, &mut state),
                b"CAMS" => draw_cams_ui(ui, &session, &mut state),
                b"CHAL" => draw_chal_ui(ui, &mut state),
                b"KEYM" => draw_keym_ui(ui, &session, &mut state),
                b"INGR" => draw_ingr_ui(ui, &session, &mut state),
                b"IPCT" => draw_ipct_ui(ui, &session, &mut state),
                b"IPDS" => draw_ipds_ui(ui, &session, &mut state),
                b"RCPE" => draw_rcpe_ui(ui, &session, &mut state),
                b"SCPT" => draw_scpt_ui(ui, &mut state),
                b"PACK" => draw_pack_ui(ui, &session, &mut state),
                b"EFSH" => draw_efsh_ui(ui, &mut state),
                b"REGN" => draw_regn_ui(ui, &session, &mut state),
                b"IMAD" => draw_imad_ui(ui, &mut state),
                b"COBJ" => draw_cobj_ui(ui, &mut state),
                b"PMIS" => draw_pmis_ui(ui, &session, &mut state),
                b"TES4" => draw_tes4_ui(ui, &mut state),
                b"FLST" => draw_flst_ui(ui, &mut state),
                b"MESG" => draw_mesg_ui(ui, &session, &mut state),
                b"SPEL" => draw_spel_ui(ui, &mut state),
                b"STAT" => draw_stat_ui(ui, &session, &mut state),
                b"IDLE" => draw_idle_ui(ui, &session, &mut state),
                b"MSET" => draw_mset_ui(ui, &session, &mut state),
                b"MSTT" => draw_mstt_ui(ui, &session, &mut state),
                b"PWAT" => draw_pwat_ui(ui, &session, &mut state),
                b"RCCT" => draw_rcct_ui(ui, &mut state),
                b"REPU" => draw_repu_ui(ui, &mut state),
                b"RGDL" => draw_rgdl_ui(ui, &session, &mut state),
                b"TREE" => draw_tree_ui(ui, &mut state),
                b"TXST" => draw_txst_ui(ui, &mut state),
                b"VTYP" => draw_vtyp_ui(ui, &mut state),
                b"TACT" => draw_tact_ui(ui, &session, &mut state),
                b"SCOL" => draw_scol_ui(ui, &mut state),
                b"IDLM" => draw_idlm_ui(ui, &session, &mut state),
                b"IMGS" => draw_imgs_ui(ui, &mut state),
                b"LGTM" => draw_lgtm_ui(ui, &mut state),
                b"LSCR" => draw_lscr_ui(ui, &session, &mut state),
                b"LSCT" => draw_lsct_ui(ui, &mut state),
                b"MICN" => draw_micn_ui(ui, &mut state),
                b"MUSC" => draw_musc_ui(ui, &mut state),
                b"DEHY" => draw_dehy_ui(ui, &session, &mut state),
                b"DOBJ" => draw_dobj_ui(ui, &mut state),
                b"ECZN" => draw_eczn_ui(ui, &session, &mut state),
                b"FURN" => draw_furn_ui(ui, &mut state),
                b"GRAS" => draw_gras_ui(ui, &mut state),
                b"HUNG" => draw_hung_ui(ui, &session, &mut state),
                b"RADS" => draw_rads_ui(ui, &session, &mut state),
                b"SLPD" => draw_slpd_ui(ui, &session, &mut state),
                b"CPTH" => draw_cpth_ui(ui, &session, &mut state),
                b"CSNO" => draw_csno_ui(ui, &session, &mut state),
                b"DEBR" => draw_debr_ui(ui, &mut state),
                b"CCRD" => draw_ccrd_ui(ui, &session, &mut state),
                b"CDCK" => draw_cdck_ui(ui, &session, &mut state),
                b"CHIP" => draw_chip_ui(ui, &session, &mut state),
                b"CMNY" => draw_cmny_ui(ui, &session, &mut state),
                b"ASPC" => draw_aspc_ui(ui, &session, &mut state),
                b"ANIO" => draw_anio_ui(ui, &session, &mut state),
                b"AVIF" => draw_avif_ui(ui, &mut state),
                b"EYES" => draw_eyes_ui(ui, &mut state),
                b"HAIR" => draw_hair_ui(ui, &session, &mut state),
                b"HDPT" => draw_hdpt_ui(ui, &session, &mut state),
                b"BPTD" => draw_bptd_ui(ui, &session, &mut state),
                b"MISC" => draw_misc_ui(ui, &session, &mut state),
                // A type with no page of its own shows nothing below the
                // header panel.
                _ => {}
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

/// The weapon's animation types, from the spec's Weapon Animation Types page.
const WEAP_ANIMATION_TYPES: &[(&str, u32)] = &[
    ("Hand To Hand", 0),
    ("Melee (1 Hand)", 1),
    ("Melee (2 Hand)", 2),
    ("Pistol - Ballistic (1 Hand)", 3),
    ("Pistol - Energy (1 Hand)", 4),
    ("Rifle - Ballistic (2 Hand)", 5),
    ("Rifle - Automatic (2 Hand)", 6),
    ("Rifle - Energy (2 Hand)", 7),
    ("Handle (2 Hand)", 8),
    ("Launcher (2 Hand)", 9),
    ("Grenade Throw (1 Hand)", 10),
    ("Land Mine (1 Hand)", 11),
    ("Mine Drop (1 Hand)", 12),
    ("Thrown (1 Hand)", 13),
];

fn draw_weap_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Combat & Damage Parameters");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Animation Type:");
            egui::ComboBox::from_id_salt("weap_animation_type")
                .selected_text(
                    WEAP_ANIMATION_TYPES
                        .iter()
                        .find(|(_, v)| *v == state.weap_animation_type)
                        .map(|(n, _)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (name, value) in WEAP_ANIMATION_TYPES.iter() {
                        ui.selectable_value(&mut state.weap_animation_type, *value, *name);
                    }
                });
            ui.separator();
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

/// Draw the creature classification and skills section.
fn draw_actor_crea_class(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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

/// Draw the leveling and vital statistics section.
fn draw_actor_vitals(ui: &mut egui::Ui, state: &mut ObjectEditorState, is_creature: bool) {
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
            if is_creature {
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
}

/// Draw the S.P.E.C.I.A.L. attributes section.
fn draw_actor_special(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
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
}

/// Draw the NPC skills and offsets section.
fn draw_actor_npc_skills(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
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
}

/// Draw the NPC appearance, hair and head parts section.
fn draw_actor_npc_appearance(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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

/// Draw the creature sound type overrides section.
fn draw_actor_crea_sounds(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sound Type Overrides (CSDT / CSDI / CSDC)");
        ui.add_space(4.0);

        let sounds_all = session.records_by_sig(*b"SOUN");

        if state.crea_sounds.is_empty() {
            ui.label(
                egui::RichText::new("No sound overrides (inheriting all base sounds).").italics(),
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

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sound Type Overrides (CSDT / CSDI / CSDC)");
        ui.add_space(4.0);

        let sounds_all = session.records_by_sig(*b"SOUN");

        if state.crea_sounds.is_empty() {
            ui.label(
                egui::RichText::new("No sound overrides (inheriting all base sounds).").italics(),
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
}

/// Draw the creature model list and animations section.
fn draw_actor_crea_models(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
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
                if !trimmed.is_empty() && !state.crea_animations.contains(&trimmed.to_string()) {
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

/// Draw the AI behaviour, combat and voice section.
fn draw_actor_ai(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
    is_creature: bool,
) {
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
            let templates = if is_creature {
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
}

/// Draw the actor effects and spells section.
fn draw_actor_effects(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
}

/// Draw the faction memberships section.
fn draw_actor_factions(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
}

/// Draw the AI packages section.
fn draw_actor_packages(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
}

/// Draw the inventory section.
fn draw_actor_inventory(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
}

/// Draw the object bounds section.
fn draw_actor_bounds(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
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

/// Draw the flags and traits section of a creature's page.
fn draw_crea_flags(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Creature Flags, Movement & Traits");
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
            ui.checkbox(&mut state.actor_allow_pc_dialogue, "Allow PC Dialogue");
            ui.checkbox(&mut state.actor_cant_open_doors, "Can't Open Doors");
            ui.checkbox(&mut state.crea_no_head_track, "No Head-Track");
        });

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Movement & Combat:").underline());
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.crea_biped, "Biped");
            ui.checkbox(&mut state.crea_walks, "Walks");
            ui.checkbox(&mut state.crea_swims, "Swims");
            ui.checkbox(&mut state.crea_flies, "Flies");
            ui.checkbox(&mut state.crea_weapon_shield, "Weapon & Shield");
        });
    });
}

/// Draw the flags and traits section of an NPC's page.
fn draw_npc_flags(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Actor Flags & Traits");
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
            ui.checkbox(&mut state.actor_female, "Female");
        });

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
    });
}

/// Draw the physical and combat parameters section of a creature's page.
fn draw_crea_physical(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Physical & Combat Parameters (CREA)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
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
            ui.separator();
            ui.label("Impact Dataset (CNAM):");
            let ipds = session.records_by_sig(*b"IPDS");
            record_dropdown(ui, "crea_cnam", &ipds, &mut state.crea_impact_dataset);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Unarmed Attack Effect (EITM):");
            let spells = session.spells_and_enchantments();
            record_dropdown(ui, "actor_eitm", &spells, &mut state.actor_unarmed_effect);
            ui.separator();
            ui.label("Unarmed Attack Anim (EAMT):");
            ui.add(egui::DragValue::new(&mut state.crea_unarmed_anim).range(0..=255));
        });
    });
}

/// Draw the physical and combat parameters section of an NPC's page.
fn draw_npc_physical(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Physical & Combat Parameters");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
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
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Unarmed Attack Effect (EITM):");
            let spells = session.spells_and_enchantments();
            record_dropdown(ui, "actor_eitm", &spells, &mut state.actor_unarmed_effect);
        });
    });
}

/// Draw the creature (CREA) editor body.
fn draw_crea_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    draw_actor_crea_class(ui, session, state);
    ui.add_space(6.0);
    draw_actor_vitals(ui, state, true);
    ui.add_space(6.0);
    draw_actor_special(ui, state);
    ui.add_space(6.0);
    draw_crea_flags(ui, state);
    ui.add_space(6.0);
    draw_crea_physical(ui, session, state);
    ui.add_space(6.0);
    draw_actor_ai(ui, session, state, true);
    ui.add_space(6.0);
    draw_actor_effects(ui, session, state);
    ui.add_space(6.0);
    draw_actor_factions(ui, session, state);
    ui.add_space(6.0);
    draw_actor_packages(ui, session, state);
    ui.add_space(6.0);
    draw_actor_inventory(ui, session, state);
    ui.add_space(6.0);
    draw_actor_crea_sounds(ui, session, state);
    ui.add_space(6.0);
    draw_actor_crea_models(ui, state);
    ui.add_space(6.0);
    draw_actor_bounds(ui, state);
}

/// Draw the NPC (NPC_) editor body.
fn draw_npc_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    draw_actor_vitals(ui, state, false);
    ui.add_space(6.0);
    draw_actor_special(ui, state);
    ui.add_space(6.0);
    draw_actor_npc_skills(ui, state);
    ui.add_space(6.0);
    draw_actor_npc_appearance(ui, session, state);
    ui.add_space(6.0);
    draw_npc_flags(ui, session, state);
    ui.add_space(6.0);
    draw_npc_physical(ui, session, state);
    ui.add_space(6.0);
    draw_actor_ai(ui, session, state, false);
    ui.add_space(6.0);
    draw_actor_effects(ui, session, state);
    ui.add_space(6.0);
    draw_actor_factions(ui, session, state);
    ui.add_space(6.0);
    draw_actor_packages(ui, session, state);
    ui.add_space(6.0);
    draw_actor_inventory(ui, session, state);
    ui.add_space(6.0);
    draw_actor_bounds(ui, state);
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

    // The attenuation struct. Every field the spec names for it.
    let a = &mut state.sound_attenuation;
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Attenuation (SNDD)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Minimum Attenuation Distance:");
            ui.add(egui::DragValue::new(&mut a.min_distance).range(0..=255));
            ui.label("Maximum Attenuation Distancce:");
            ui.add(egui::DragValue::new(&mut a.max_distance).range(0..=255));
            ui.label("Frequency Adjustment Percentage:");
            ui.add(egui::DragValue::new(&mut a.frequency_adjustment).range(-128..=127));
        });
        ui.horizontal(|ui| {
            ui.label("Static Attenuation cdB:");
            ui.add(
                egui::DragValue::new(&mut a.static_attenuation)
                    .speed(10.0)
                    .range(-10000..=10000),
            );
            ui.label("Stop Time:");
            ui.add(egui::DragValue::new(&mut a.stop_time).range(0..=255));
            ui.label("Start Time:");
            ui.add(egui::DragValue::new(&mut a.start_time).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Reverb Attenuation Control:");
            ui.add(egui::DragValue::new(&mut a.reverb_attenuation).range(-32768..=32767));
            ui.label("Priority:");
            ui.add(egui::DragValue::new(&mut a.priority).range(-2147483648..=2147483647));
        });
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut a.x).range(-2147483648..=2147483647));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut a.y).range(-2147483648..=2147483647));
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Attenuation Curve:").underline());
        ui.horizontal(|ui| {
            for (i, point) in a.attenuation_points.iter_mut().enumerate() {
                ui.label(format!("Point {}:", i + 1));
                ui.add(egui::DragValue::new(point).range(-32768..=32767));
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Flag Word:").underline());
        ui.add(
            egui::DragValue::new(&mut a.flags)
                .range(0..=u32::MAX)
                .prefix("0x"),
        );
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

/// Draw the leveled list's settings and entries.
///
/// The three leveled list types share a format, but each gets its own page
/// so the title, the candidate list and the settings can be set up
/// independently.
fn draw_lvl_ui(
    ui: &mut egui::Ui,
    state: &mut ObjectEditorState,
    entries_title: &str,
    candidates: &[(u32, String)],
    use_all: bool,
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
            if use_all {
                ui.checkbox(&mut state.lvl_use_all, "Use All");
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(entries_title);
        ui.add_space(4.0);

        if state.lvl_entries.is_empty() {
            ui.label(egui::RichText::new("No entries in this leveled list.").italics());
        } else {
            let mut remove_idx = None;
            for (idx, entry) in state.lvl_entries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this entry").clicked() {
                        remove_idx = Some(idx);
                    }
                    ui.label("Level:");
                    ui.add(egui::DragValue::new(&mut entry.level).range(1..=32767));
                    ui.separator();
                    let name = candidates
                        .iter()
                        .find(|(id, _)| Some(*id) == entry.reference)
                        .map(|(_, n)| n.as_str())
                        .unwrap_or("Custom Object");
                    ui.label(format!("{name} ({:08X})", entry.reference.unwrap_or(0)));
                    ui.separator();
                    ui.label("Count:");
                    ui.add(egui::DragValue::new(&mut entry.count).range(1..=32767));
                });

                // The extra data: the owner, the global variable or required
                // rank, and the item condition.
                let mut has_extra = entry.has_extra;
                if ui
                    .checkbox(&mut has_extra, "Has extra data (COED)")
                    .changed()
                {
                    entry.has_extra = has_extra;
                }
                if entry.has_extra {
                    ui.horizontal(|ui| {
                        ui.label("Owner:");
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut entry.owner.map(|v| format!("{v:08X}")).unwrap_or_default(),
                            )
                            .id_salt(format!("lvl_owner_{idx}"))
                            .hint_text("00000000")
                            .desired_width(110.0),
                        );
                        ui.label("Global / Required Rank:");
                        ui.add(egui::DragValue::new(&mut entry.global_or_rank).range(0..=u32::MAX));
                        ui.label("Item Condition:");
                        ui.add(
                            egui::DragValue::new(&mut entry.item_condition)
                                .speed(0.01)
                                .range(-100000.0..=100000.0),
                        );
                    });
                }
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
                candidates,
                &mut state.lvl_entry_fid_to_add,
            );
            ui.label("Count:");
            ui.add(egui::DragValue::new(&mut state.lvl_entry_count_to_add).range(1..=32767));
            if ui.button("+ Add Entry").clicked() {
                if let Some(fid) = state.lvl_entry_fid_to_add {
                    state.lvl_entries.push(LeveledListEntry {
                        level: state.lvl_entry_level_to_add.max(1),
                        reference: Some(fid),
                        count: state.lvl_entry_count_to_add.max(1),
                        ..Default::default()
                    });
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

/// Draw the leveled creature (LVLC) editor body.
fn draw_lvlc_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let candidates = session.creature_templates();
    draw_lvl_ui(
        ui,
        state,
        "Leveled Creature Entries (LVLO)",
        &candidates,
        false,
    );
}

/// Draw the leveled NPC (LVLN) editor body.
fn draw_lvln_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let candidates = session.npc_templates();
    draw_lvl_ui(ui, state, "Leveled NPC Entries (LVLO)", &candidates, false);
}

/// Draw the leveled item (LVLI) editor body.
fn draw_lvli_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let candidates = session.inventory_items();
    draw_lvl_ui(ui, state, "Leveled Item Entries (LVLO)", &candidates, true);
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

/// Read one time of day colours struct out of a payload.
///
/// The struct holds one rgba per time of day: sunrise, day, sunset,
/// night, high noon and midnight.
fn read_time_of_day_colors(b: &[u8], times: usize) -> TimeOfDayColors {
    let c = |i: usize| -> [u8; 4] {
        let at = i * 4;
        if b.len() >= at + 4 {
            [b[at], b[at + 1], b[at + 2], b[at + 3]]
        } else {
            [0; 4]
        }
    };
    let mut t = TimeOfDayColors::default();
    if times >= 1 {
        t.sunrise = c(0);
    }
    if times >= 2 {
        t.day = c(1);
    }
    if times >= 3 {
        t.sunset = c(2);
    }
    if times >= 4 {
        t.night = c(3);
    }
    if times >= 5 {
        t.high_noon = c(4);
    }
    if times >= 6 {
        t.midnight = c(5);
    }
    t
}

/// Write one time of day colours struct back over the record's own bytes.
fn write_time_of_day_colors(original: &[u8], t: &TimeOfDayColors) -> Vec<u8> {
    let mut b = original.to_vec();
    /// Write a colour at an offset, if the record's own bytes reach it.
    fn put(b: &mut [u8], at: usize, color: [u8; 4]) {
        if b.len() >= at + 4 {
            b[at..at + 4].copy_from_slice(&color);
        }
    }
    put(&mut b, 0, t.sunrise);
    put(&mut b, 4, t.day);
    put(&mut b, 8, t.sunset);
    put(&mut b, 12, t.night);
    put(&mut b, 16, t.high_noon);
    put(&mut b, 20, t.midnight);
    b
}

fn draw_wthr_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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
            ui.label("Cloud Speed (Lower):");
            ui.add(egui::DragValue::new(&mut state.wthr_cloud_speed_lower).range(0..=255));
            ui.label("Cloud Speed (Upper):");
            ui.add(egui::DragValue::new(&mut state.wthr_cloud_speed_upper).range(0..=255));
            ui.label("Transition Delta:");
            ui.add(egui::DragValue::new(&mut state.wthr_transition_delta).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Precipitation - Begin Fade In:");
            ui.add(egui::DragValue::new(&mut state.wthr_precipitation_begin).range(0..=255));
            ui.label("Precipitation - End Fade Out:");
            ui.add(egui::DragValue::new(&mut state.wthr_precipitation_end).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Thunder / Lightning - Begin Fade In:");
            ui.add(egui::DragValue::new(&mut state.wthr_thunder_begin).range(0..=255));
            ui.label("Thunder / Lightning - End Fade Out:");
            ui.add(egui::DragValue::new(&mut state.wthr_thunder_end).range(0..=255));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Thunder / Lightning - Frequency:");
            ui.add(egui::DragValue::new(&mut state.wthr_thunder_frequency).range(0..=255));
            ui.label("Weather Classification:");
            ui.add(egui::DragValue::new(&mut state.wthr_weather_classification).range(0..=255));
            ui.label("Lightning Color:");
            ui.color_edit_button_srgba_unmultiplied(&mut state.wthr_lightning_color);
        });
    });

    // The cloud texture layers.
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Cloud Textures");
        ui.add_space(4.0);
        for i in 0..4 {
            ui.horizontal(|ui| {
                ui.label(format!("Layer {i}:"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.wthr_cloud_layers[i])
                        .desired_width(ui.available_width() - 120.0),
                );
            });
        }
        ui.horizontal(|ui| {
            ui.label("Max Cloud Layers:");
            ui.add(egui::DragValue::new(&mut state.wthr_max_cloud_layers));
            ui.label("Cloud Layer Speed:");
            for i in 0..4 {
                ui.add(egui::DragValue::new(&mut state.wthr_cloud_speeds[i]).range(0..=255));
            }
        });
    });

    // The fog distances.
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Fog Distance (FNAM)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Day - Near:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[0]).speed(0.01));
            ui.label("Day - Far:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[1]).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Night - Near:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[2]).speed(0.01));
            ui.label("Night - Far:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[3]).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Day - Power:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[4]).speed(0.01));
            ui.label("Night - Power:");
            ui.add(egui::DragValue::new(&mut state.wthr_fog_distances[5]).speed(0.01));
        });
    });

    // The image space modifiers, one per time of day.
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Image Space Modifiers");
        ui.add_space(4.0);
        let adapters = session.records_by_sig(*b"IMAD");
        for (i, name) in WTHR_TIMES_OF_DAY.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                let mut fid = state.wthr_image_space_modifiers[i];
                record_dropdown(ui, &format!("wthr_iad_{i}"), &adapters, &mut fid);
                state.wthr_image_space_modifiers[i] = fid;
            });
        }
    });

    // The weather's colours: ten colour types, each with one colour per
    // time of day.
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Colors by Time & Type (NAM0)");
        ui.add_space(4.0);
        for (i, type_name) in WTHR_COLOR_TYPES.iter().enumerate() {
            if i >= state.wthr_colors.len() {
                break;
            }
            ui.label(format!("{type_name}:"));
            let colors = &mut state.wthr_colors[i];
            for (j, time) in WTHR_TIMES_OF_DAY.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*time);
                    let color = match j {
                        0 => &mut colors.sunrise,
                        1 => &mut colors.day,
                        2 => &mut colors.sunset,
                        3 => &mut colors.night,
                        4 => &mut colors.high_noon,
                        _ => &mut colors.midnight,
                    };
                    ui.color_edit_button_srgba_unmultiplied(color);
                });
            }
        }
    });
}

/// Read the water's visual data out of a `DNAM` payload.
fn read_watr_visual(b: &[u8]) -> WatrVisual {
    let f = |at: usize| -> f32 {
        if b.len() >= at + 4 {
            f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
        } else {
            0.0
        }
    };
    let c = |at: usize| -> [u8; 4] {
        if b.len() >= at + 4 {
            [b[at], b[at + 1], b[at + 2], b[at + 3]]
        } else {
            [0; 4]
        }
    };
    WatrVisual {
        sun_power: f(16),
        reflectivity_amount: f(20),
        fresnel_amount: f(24),
        above_fog_near: f(32),
        above_fog_far: f(36),
        shallow_color: c(40),
        deep_color: c(44),
        reflection_color: c(48),
        rain_force: f(56),
        rain_velocity: f(60),
        rain_falloff: f(64),
        rain_dampener: f(68),
        displacement_starting_size: f(72),
        displacement_force: f(76),
        displacement_velocity: f(80),
        displacement_falloff: f(84),
        displacement_dampener: f(88),
        rain_starting_size: f(92),
        noise_scale: f(96),
        layer_one_wind_direction: f(100),
        layer_two_wind_direction: f(104),
        layer_three_wind_direction: f(108),
        layer_one_wind_speed: f(112),
        layer_two_wind_speed: f(116),
        layer_three_wind_speed: f(120),
        depth_falloff_start: f(124),
        depth_falloff_end: f(128),
        above_fog_amount: f(132),
        normals_uv_scale: f(136),
        under_fog_amount: f(140),
        under_fog_near: f(144),
        under_fog_far: f(148),
        distortion_amount: f(152),
        shininess: f(156),
        reflection_hdr_multiplier: f(160),
        light_radius: f(164),
        light_brightness: f(168),
        layer_one_uv_scale: f(172),
        layer_two_uv_scale: f(176),
        layer_three_uv_scale: f(180),
        layer_one_amplitude_scale: f(184),
        layer_two_amplitude_scale: f(188),
        layer_three_amplitude_scale: f(192),
    }
}

/// Write the water's visual data back over the record's own bytes.
fn write_watr_visual(original: &[u8], v: &WatrVisual) -> Vec<u8> {
    let mut b = original.to_vec();
    macro_rules! put_f {
        ($at:expr, $field:ident) => {
            if b.len() >= $at + 4 {
                b[$at..$at + 4].copy_from_slice(&v.$field.to_le_bytes());
            }
        };
    }
    macro_rules! put_c {
        ($at:expr, $field:ident) => {
            if b.len() >= $at + 4 {
                b[$at..$at + 4].copy_from_slice(&v.$field);
            }
        };
    }
    put_f!(16, sun_power);
    put_f!(20, reflectivity_amount);
    put_f!(24, fresnel_amount);
    put_f!(32, above_fog_near);
    put_f!(36, above_fog_far);
    put_c!(40, shallow_color);
    put_c!(44, deep_color);
    put_c!(48, reflection_color);
    put_f!(56, rain_force);
    put_f!(60, rain_velocity);
    put_f!(64, rain_falloff);
    put_f!(68, rain_dampener);
    put_f!(72, displacement_starting_size);
    put_f!(76, displacement_force);
    put_f!(80, displacement_velocity);
    put_f!(84, displacement_falloff);
    put_f!(88, displacement_dampener);
    put_f!(92, rain_starting_size);
    put_f!(96, noise_scale);
    put_f!(100, layer_one_wind_direction);
    put_f!(104, layer_two_wind_direction);
    put_f!(108, layer_three_wind_direction);
    put_f!(112, layer_one_wind_speed);
    put_f!(116, layer_two_wind_speed);
    put_f!(120, layer_three_wind_speed);
    put_f!(124, depth_falloff_start);
    put_f!(128, depth_falloff_end);
    put_f!(132, above_fog_amount);
    put_f!(136, normals_uv_scale);
    put_f!(140, under_fog_amount);
    put_f!(144, under_fog_near);
    put_f!(148, under_fog_far);
    put_f!(152, distortion_amount);
    put_f!(156, shininess);
    put_f!(160, reflection_hdr_multiplier);
    put_f!(164, light_radius);
    put_f!(168, light_brightness);
    put_f!(172, layer_one_uv_scale);
    put_f!(176, layer_two_uv_scale);
    put_f!(180, layer_three_uv_scale);
    put_f!(184, layer_one_amplitude_scale);
    put_f!(188, layer_two_amplitude_scale);
    put_f!(192, layer_three_amplitude_scale);
    b
}

fn draw_watr_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
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

    // The related waters.
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Related Waters (GNAM)");
        ui.add_space(4.0);
        let waters = session.records_by_sig(*b"WATR");
        ui.horizontal(|ui| {
            ui.label("Daytime:");
            let mut fid = state.watr_daytime;
            record_dropdown(ui, "watr_day", &waters, &mut fid);
            state.watr_daytime = fid;
            ui.separator();
            ui.label("Nighttime:");
            let mut fid = state.watr_nighttime;
            record_dropdown(ui, "watr_night", &waters, &mut fid);
            state.watr_nighttime = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Underwater:");
            let mut fid = state.watr_underwater;
            record_dropdown(ui, "watr_under", &waters, &mut fid);
            state.watr_underwater = fid;
        });
    });

    // The visual data. Every field the spec names for the `DNAM` struct.
    let v = &mut state.watr_visual;
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Water Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Sun Power:");
            ui.add(egui::DragValue::new(&mut v.sun_power).speed(0.01));
            ui.label("Reflectivity Amount:");
            ui.add(egui::DragValue::new(&mut v.reflectivity_amount).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Fresnel Amount:");
            ui.add(egui::DragValue::new(&mut v.fresnel_amount).speed(0.01));
            ui.label("Distortion Amount:");
            ui.add(egui::DragValue::new(&mut v.distortion_amount).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Shininess:");
            ui.add(egui::DragValue::new(&mut v.shininess).speed(0.01));
            ui.label("Reflection HDR Multiplier:");
            ui.add(egui::DragValue::new(&mut v.reflection_hdr_multiplier).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Light Radius:");
            ui.add(egui::DragValue::new(&mut v.light_radius).speed(0.01));
            ui.label("Light Brightness:");
            ui.add(egui::DragValue::new(&mut v.light_brightness).speed(0.01));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Fog Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Above Water - Fog Amount:");
            ui.add(egui::DragValue::new(&mut v.above_fog_amount).speed(0.01));
            ui.label("Above Water - Fog Near Plane Distance:");
            ui.add(egui::DragValue::new(&mut v.above_fog_near).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Above Water - Fog Far Plane Distance:");
            ui.add(egui::DragValue::new(&mut v.above_fog_far).speed(0.01));
            ui.label("Under Water - Fog Amount:");
            ui.add(egui::DragValue::new(&mut v.under_fog_amount).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Under Water - Fog Near Plane Distance:");
            ui.add(egui::DragValue::new(&mut v.under_fog_near).speed(0.01));
            ui.label("Under Water - Fog Far Plane Distance:");
            ui.add(egui::DragValue::new(&mut v.under_fog_far).speed(0.01));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Rain & Displacement Simulators");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Rain Simulator - Force:");
            ui.add(egui::DragValue::new(&mut v.rain_force).speed(0.01));
            ui.label("Rain Simulator - Velocity:");
            ui.add(egui::DragValue::new(&mut v.rain_velocity).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Rain Simulator - Falloff:");
            ui.add(egui::DragValue::new(&mut v.rain_falloff).speed(0.01));
            ui.label("Rain Simulator - Dampener:");
            ui.add(egui::DragValue::new(&mut v.rain_dampener).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Rain Simulator - Starting Size:");
            ui.add(egui::DragValue::new(&mut v.rain_starting_size).speed(0.01));
            ui.label("Displacement Simulator - Starting Size:");
            ui.add(egui::DragValue::new(&mut v.displacement_starting_size).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Displacement Simulator - Force:");
            ui.add(egui::DragValue::new(&mut v.displacement_force).speed(0.01));
            ui.label("Displacement Simulator - Velocity:");
            ui.add(egui::DragValue::new(&mut v.displacement_velocity).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Displacement Simulator - Falloff:");
            ui.add(egui::DragValue::new(&mut v.displacement_falloff).speed(0.01));
            ui.label("Displacement Simulator - Dampener:");
            ui.add(egui::DragValue::new(&mut v.displacement_dampener).speed(0.01));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Noise Properties");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Normals - Noise Scale:");
            ui.add(egui::DragValue::new(&mut v.noise_scale).speed(0.01));
            ui.label("Normals - UV Scale:");
            ui.add(egui::DragValue::new(&mut v.normals_uv_scale).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Normals - Depth Falloff Start:");
            ui.add(egui::DragValue::new(&mut v.depth_falloff_start).speed(0.01));
            ui.label("Normals - Depth Falloff End:");
            ui.add(egui::DragValue::new(&mut v.depth_falloff_end).speed(0.01));
        });
        for name in ["One", "Two", "Three"] {
            ui.label(format!("Noise Layer {name}"));
            match name {
                "One" => {
                    ui.horizontal(|ui| {
                        ui.label("Wind Direction:");
                        ui.add(egui::DragValue::new(&mut v.layer_one_wind_direction).speed(0.01));
                        ui.label("Wind Speed:");
                        ui.add(egui::DragValue::new(&mut v.layer_one_wind_speed).speed(0.01));
                    });
                    ui.horizontal(|ui| {
                        ui.label("UV Scale:");
                        ui.add(egui::DragValue::new(&mut v.layer_one_uv_scale).speed(0.01));
                        ui.label("Amplitude Scale:");
                        ui.add(egui::DragValue::new(&mut v.layer_one_amplitude_scale).speed(0.01));
                    });
                }
                "Two" => {
                    ui.horizontal(|ui| {
                        ui.label("Wind Direction:");
                        ui.add(egui::DragValue::new(&mut v.layer_two_wind_direction).speed(0.01));
                        ui.label("Wind Speed:");
                        ui.add(egui::DragValue::new(&mut v.layer_two_wind_speed).speed(0.01));
                    });
                    ui.horizontal(|ui| {
                        ui.label("UV Scale:");
                        ui.add(egui::DragValue::new(&mut v.layer_two_uv_scale).speed(0.01));
                        ui.label("Amplitude Scale:");
                        ui.add(egui::DragValue::new(&mut v.layer_two_amplitude_scale).speed(0.01));
                    });
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label("Wind Direction:");
                        ui.add(egui::DragValue::new(&mut v.layer_three_wind_direction).speed(0.01));
                        ui.label("Wind Speed:");
                        ui.add(egui::DragValue::new(&mut v.layer_three_wind_speed).speed(0.01));
                    });
                    ui.horizontal(|ui| {
                        ui.label("UV Scale:");
                        ui.add(egui::DragValue::new(&mut v.layer_three_uv_scale).speed(0.01));
                        ui.label("Amplitude Scale:");
                        ui.add(
                            egui::DragValue::new(&mut v.layer_three_amplitude_scale).speed(0.01),
                        );
                    });
                }
            }
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Colors");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Shallow Color:");
            ui.color_edit_button_srgba_unmultiplied(&mut v.shallow_color);
            ui.label("Deep Color:");
            ui.color_edit_button_srgba_unmultiplied(&mut v.deep_color);
            ui.label("Reflection Color:");
            ui.color_edit_button_srgba_unmultiplied(&mut v.reflection_color);
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

/// Draw a need-trigger record: `DEHY`, `HUNG`, `RADS` or `SLPD`.
///
/// All four have the same two-field `DATA`: a trigger threshold and the
/// actor effect the trigger applies.
/// Draw the dehydration (DEHY) editor body.
fn draw_dehy_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let (mut threshold, mut effect) = (state.dehyd_trigger_threshold, state.dehyd_actor_effect);
    ui.group(|ui| {
        ui.strong("Dehydration");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Trigger Threshold:");
            ui.add(egui::DragValue::new(&mut threshold).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Actor Effect:");
            let spells = session.records_by_sig(*b"SPEL");
            record_dropdown(ui, "dehy_effect", &spells, &mut effect);
        });
    });
    state.dehyd_trigger_threshold = threshold;
    state.dehyd_actor_effect = effect;
}

/// Draw the hunger (HUNG) editor body.
fn draw_hung_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let (mut threshold, mut effect) = (state.hung_trigger_threshold, state.hung_actor_effect);
    ui.group(|ui| {
        ui.strong("Hunger");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Trigger Threshold:");
            ui.add(egui::DragValue::new(&mut threshold).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Actor Effect:");
            let spells = session.records_by_sig(*b"SPEL");
            record_dropdown(ui, "hung_effect", &spells, &mut effect);
        });
    });
    state.hung_trigger_threshold = threshold;
    state.hung_actor_effect = effect;
}

/// Draw the radiation (RADS) editor body.
fn draw_rads_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let (mut threshold, mut effect) = (state.rads_trigger_threshold, state.rads_actor_effect);
    ui.group(|ui| {
        ui.strong("Radiation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Trigger Threshold:");
            ui.add(egui::DragValue::new(&mut threshold).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Actor Effect:");
            let spells = session.records_by_sig(*b"SPEL");
            record_dropdown(ui, "rads_effect", &spells, &mut effect);
        });
    });
    state.rads_trigger_threshold = threshold;
    state.rads_actor_effect = effect;
}

/// Draw the sleep (SLPD) editor body.
fn draw_slpd_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    let (mut threshold, mut effect) = (state.slpd_trigger_threshold, state.slpd_actor_effect);
    ui.group(|ui| {
        ui.strong("Sleep Deprivation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Trigger Threshold:");
            ui.add(egui::DragValue::new(&mut threshold).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Actor Effect:");
            let spells = session.records_by_sig(*b"SPEL");
            record_dropdown(ui, "slpd_effect", &spells, &mut effect);
        });
    });
    state.slpd_trigger_threshold = threshold;
    state.slpd_actor_effect = effect;
}

/// Draw the Encounter Zone (ECZN) editor body.
fn draw_eczn_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Encounter Zone");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Owner:");
            let mut fid = state.eczn_owner;
            record_dropdown(
                ui,
                "eczn_owner",
                &session.records_by_sig(*b"FACT"),
                &mut fid,
            );
            state.eczn_owner = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Rank:");
            ui.add(egui::DragValue::new(&mut state.eczn_rank).range(-128..=127));
            ui.separator();
            ui.label("Minimum Level:");
            ui.add(egui::DragValue::new(&mut state.eczn_minimum_level).range(-128..=127));
        });
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in ECZN_FLAGS {
                let mut set = (state.eczn_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.eczn_flags |= bit;
                    } else {
                        state.eczn_flags &= !bit;
                    }
                }
            }
        });
    });
}

/// Draw the Default Object Manager (DOBJ) editor body.
fn draw_dobj_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Default Objects");
        ui.add_space(4.0);
        for (i, value) in state.dobj_values.iter_mut().enumerate() {
            let name = DOBJ_FIELDS.get(i).copied().unwrap_or("Unknown");
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                ui.add(
                    egui::TextEdit::singleline(&mut value.to_string())
                        .id_salt(format!("dobj_{i}"))
                        .hint_text("00000000")
                        .desired_width(100.0),
                );
            });
        }
    });
}

/// Draw the Furniture (FURN) editor body.
fn draw_furn_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Furniture");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Marker Flags (MNAM):");
            let hex: String = state
                .furn_marker_flags
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            ui.label(hex);
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.furn_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });

    draw_destruction_ui(
        ui,
        &mut state.furn_destruction,
        &mut state.furn_dest_health,
        &mut state.furn_dest_flags,
        "furn",
    );
}

/// Draw the Grass (GRAS) editor body.
fn draw_gras_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Grass");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Density:");
            ui.add(egui::DragValue::new(&mut state.gras_density).range(0..=255));
            ui.separator();
            ui.label("Min Slope:");
            ui.add(egui::DragValue::new(&mut state.gras_min_slope).range(0..=255));
            ui.separator();
            ui.label("Max Slope:");
            ui.add(egui::DragValue::new(&mut state.gras_max_slope).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Unit From Water Amount:");
            ui.add(egui::DragValue::new(&mut state.gras_unit_from_water_amount).range(0..=65535));
            ui.separator();
            ui.label("Unit From Water Type:");
            egui::ComboBox::from_id_salt("gras_water_type")
                .selected_text(
                    GRAS_WATER_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.gras_unit_from_water_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in GRAS_WATER_TYPES.iter() {
                        ui.selectable_value(&mut state.gras_unit_from_water_type, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Position Range:");
            ui.add(
                egui::DragValue::new(&mut state.gras_position_range)
                    .speed(0.1)
                    .range(-10000.0..=10000.0),
            );
            ui.separator();
            ui.label("Height Range:");
            ui.add(
                egui::DragValue::new(&mut state.gras_height_range)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Color Range:");
            ui.add(
                egui::DragValue::new(&mut state.gras_color_range)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Wave Period:");
            ui.add(
                egui::DragValue::new(&mut state.gras_wave_period)
                    .speed(0.1)
                    .range(-10000.0..=10000.0),
            );
        });
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in GRAS_FLAGS {
                let mut set = (state.gras_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.gras_flags |= bit;
                    } else {
                        state.gras_flags &= !bit;
                    }
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.gras_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });
}

/// Draw the Idle Animation (IDLM) editor body.
fn draw_idlm_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Idle Animation");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in IDLM_FLAGS {
                let mut set = (state.idlm_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.idlm_flags |= bit;
                    } else {
                        state.idlm_flags &= !bit;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Animation Count (IDLC):");
            ui.add(egui::DragValue::new(&mut state.idlm_animation_count).range(0..=255));
            ui.separator();
            ui.label("Idle Timer Setting (IDLT):");
            ui.add(
                egui::DragValue::new(&mut state.idlm_idle_timer)
                    .speed(0.1)
                    .range(0.0..=3600.0),
            );
        });
        ui.label(format!(
            "IDLC unused bytes: {:02X?} — preserved as-is",
            state.idlm_idlc_unused
        ));
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Animations (IDLA) — {}",
            state.idlm_animations.len()
        ));
        ui.add_space(4.0);
        let idles = session.records_by_sig(*b"IDLE");
        let mut remove_idx = None;
        for (idx, anim) in state.idlm_animations.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button("✖")
                    .on_hover_text("Remove this animation")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
                let mut fid = Some(*anim);
                record_dropdown(ui, &format!("idlm_anim_{idx}"), &idles, &mut fid);
                *anim = fid.unwrap_or(0);
            });
        }
        if let Some(idx) = remove_idx {
            state.idlm_animations.remove(idx);
        }
    });
}

/// Draw the Image Space (IMGS) editor body.
fn draw_imgs_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Image Space");
        ui.add_space(4.0);
        let names = [
            "HDR Eye Adapt Speed",
            "HDR Blur Radius",
            "HDR Blur Passes",
            "HDR Emissive Multiplier",
            "HDR Target LUM",
            "HDR Upper LUM Clamp",
            "HDR Bright Scale",
            "HDR Bright Clamp",
            "HDR LUM Ramp No Tex",
            "HDR LUM Ramp Min",
            "HDR LUM Ramp Max",
            "HDR Sunlight Dimmer",
            "HDR Grass Dimmer",
            "HDR Tree Dimmer",
        ];
        for (i, name) in names.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(*name);
                ui.add(
                    egui::DragValue::new(&mut state.imgs_hdr[i])
                        .speed(0.01)
                        .range(-1000.0..=1000.0),
                );
            });
        }
        ui.label(format!(
            "The rest of DNAM ({} bytes) depends on the record's form version — preserved as-is.",
            state.imgs_rest.len()
        ));
    });
}

/// Draw the Lighting (LGTM) editor body.
fn draw_lgtm_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Lighting");
        ui.add_space(4.0);
        for (name, color) in [
            ("Ambient Color", &mut state.lgtm_ambient_color),
            ("Directional Color", &mut state.lgtm_directional_color),
            ("Fog Color", &mut state.lgtm_fog_color),
        ] {
            ui.horizontal(|ui| {
                ui.label(name);
                for c in color.iter_mut() {
                    ui.add(egui::DragValue::new(c).range(0..=255));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Fog Near:");
            ui.add(
                egui::DragValue::new(&mut state.lgtm_fog_near)
                    .speed(0.1)
                    .range(-100000.0..=100000.0),
            );
            ui.separator();
            ui.label("Fog Far:");
            ui.add(
                egui::DragValue::new(&mut state.lgtm_fog_far)
                    .speed(0.1)
                    .range(-100000.0..=100000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Directional Rotation XY:");
            ui.add(egui::DragValue::new(&mut state.lgtm_directional_rotation_xy).range(-360..=360));
            ui.separator();
            ui.label("Directional Rotation Z:");
            ui.add(egui::DragValue::new(&mut state.lgtm_directional_rotation_z).range(-360..=360));
        });
        ui.horizontal(|ui| {
            ui.label("Directional Fade:");
            ui.add(
                egui::DragValue::new(&mut state.lgtm_directional_fade)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Fog Clip Distance:");
            ui.add(
                egui::DragValue::new(&mut state.lgtm_fog_clip_distance)
                    .speed(0.1)
                    .range(-100000.0..=100000.0),
            );
            ui.separator();
            ui.label("Fog Power:");
            ui.add(
                egui::DragValue::new(&mut state.lgtm_fog_power)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
    });
}

/// Draw the Load Screen (LSCR) editor body.
fn draw_lscr_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Load Screen");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.lscr_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.lscr_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Load Screen Type (WMI1):");
            let types = session.records_by_sig(*b"LSCT");
            let mut fid = state.lscr_load_screen_type;
            record_dropdown(ui, "lscr_type", &types, &mut fid);
            state.lscr_load_screen_type = fid;
        });
        ui.label("Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.lscr_desc)
                .desired_rows(6)
                .desired_width(ui.available_width() - 20.0),
        );
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Locations (LNAM) — {}", state.lscr_locations.len()));
        ui.add_space(4.0);
        let mut remove_idx = None;
        for (idx, loc) in state.lscr_locations.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button("✖")
                    .on_hover_text("Remove this location")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
                ui.label(format!("#{}", idx + 1));
                ui.label("Direct:");
                ui.add(
                    egui::TextEdit::singleline(
                        &mut loc.direct.map(|v| format!("{v:08X}")).unwrap_or_default(),
                    )
                    .id_salt(format!("lscr_direct_{idx}"))
                    .hint_text("00000000")
                    .desired_width(100.0),
                );
                ui.label("World:");
                ui.add(
                    egui::TextEdit::singleline(
                        &mut loc
                            .indirect_world
                            .map(|v| format!("{v:08X}"))
                            .unwrap_or_default(),
                    )
                    .id_salt(format!("lscr_world_{idx}"))
                    .hint_text("00000000")
                    .desired_width(100.0),
                );
                ui.label("Grid X/Y:");
                ui.add(egui::DragValue::new(&mut loc.indirect_grid_x).range(-32768..=32767));
                ui.add(egui::DragValue::new(&mut loc.indirect_grid_y).range(-32768..=32767));
            });
        }
        if let Some(idx) = remove_idx {
            state.lscr_locations.remove(idx);
        }
    });
}

/// Draw the Load Screen Type (LSCT) editor body.
fn draw_lsct_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Load Screen Type");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("lsct_type")
                .selected_text(
                    LSCT_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.lsct_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in LSCT_TYPES.iter() {
                        ui.selectable_value(&mut state.lsct_type, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut state.lsct_x).range(0..=10000));
            ui.separator();
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut state.lsct_y).range(0..=10000));
            ui.separator();
            ui.label("Width:");
            ui.add(egui::DragValue::new(&mut state.lsct_width).range(0..=10000));
            ui.separator();
            ui.label("Height:");
            ui.add(egui::DragValue::new(&mut state.lsct_height).range(0..=10000));
        });
        ui.horizontal(|ui| {
            ui.label("Orientation:");
            ui.add(
                egui::DragValue::new(&mut state.lsct_orientation)
                    .speed(0.01)
                    .range(-360.0..=360.0),
            );
            ui.separator();
            ui.label("Stats:");
            egui::ComboBox::from_id_salt("lsct_stats")
                .selected_text(
                    LSCT_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.lsct_stats)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in LSCT_TYPES.iter() {
                        ui.selectable_value(&mut state.lsct_stats, *v, *name);
                    }
                });
        });
        for (label, font, color, alignment) in [
            (
                "Font 1",
                &mut state.lsct_font1,
                &mut state.lsct_font1_color,
                &mut state.lsct_font1_alignment,
            ),
            (
                "Font 2",
                &mut state.lsct_font2,
                &mut state.lsct_font2_color,
                &mut state.lsct_font2_alignment,
            ),
        ] {
            ui.separator();
            ui.strong(label);
            ui.horizontal(|ui| {
                ui.label("Font:");
                egui::ComboBox::from_id_salt(format!("lsct_{label}"))
                    .selected_text(
                        LSCT_FONTS
                            .iter()
                            .find(|(v, _)| *v == *font)
                            .map(|(_, n)| *n)
                            .unwrap_or("Custom"),
                    )
                    .show_ui(ui, |ui| {
                        for (v, name) in LSCT_FONTS.iter() {
                            ui.selectable_value(font, *v, *name);
                        }
                    });
                ui.separator();
                ui.label("Alignment:");
                egui::ComboBox::from_id_salt(format!("lsct_{label}_align"))
                    .selected_text(
                        LSCT_ALIGNMENTS
                            .iter()
                            .find(|(v, _)| *v == *alignment)
                            .map(|(_, n)| *n)
                            .unwrap_or("Custom"),
                    )
                    .show_ui(ui, |ui| {
                        for (v, name) in LSCT_ALIGNMENTS.iter() {
                            ui.selectable_value(alignment, *v, *name);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Color:");
                for c in color.iter_mut() {
                    ui.add(egui::DragValue::new(c).speed(0.01).range(0.0..=255.0));
                }
            });
        }
    });
}

/// Draw the Mission Icon (MICN) editor body.
fn draw_micn_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Mission Icon");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.micn_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.micn_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });
}

/// Draw the Music (MUSC) editor body.
fn draw_musc_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Music");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Filename (FNAM):");
            ui.add(
                egui::TextEdit::singleline(&mut state.musc_filename)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("db (ANAM):");
            ui.add(
                egui::DragValue::new(&mut state.musc_db)
                    .speed(0.01)
                    .range(-100.0..=100.0),
            );
            ui.label("A positive value makes the music loop.");
        });
    });
}

/// Draw the Recipe Category (RCCT) editor body.
fn draw_rcct_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Recipe Category");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in RCCT_FLAGS {
                let mut set = (state.rcct_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.rcct_flags |= bit;
                    } else {
                        state.rcct_flags &= !bit;
                    }
                }
            }
        });
        ui.label(format!(
            "Unknown flag bits: 0x{:02X}",
            state.rcct_flags & !0x01
        ));
    });
}

/// Draw the Reputation (REPU) editor body.
fn draw_repu_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Reputation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (DATA):");
            ui.add(
                egui::DragValue::new(&mut state.repu_value)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.repu_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.repu_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });
}

/// Draw the Ragdoll (RGDL) editor body.
fn draw_rgdl_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Ragdoll");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Version (NVER):");
            ui.add(egui::DragValue::new(&mut state.rgdl_version).range(0..=1000));
        });
        ui.horizontal(|ui| {
            ui.label("Dynamic Bone Count (DATA):");
            ui.add(egui::DragValue::new(&mut state.rgdl_dynamic_bone_count).range(0..=1000));
        });
        for (label, value) in [
            ("Feedback Enabled", &mut state.rgdl_feedback_enabled),
            ("Foot IK Enabled", &mut state.rgdl_foot_ik_enabled),
            ("Look IK Enabled", &mut state.rgdl_look_ik_enabled),
            ("Grab IK Enabled", &mut state.rgdl_grab_ik_enabled),
            ("Pose Matching", &mut state.rgdl_pose_matching),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.selectable_value(value, 0, "No");
                ui.selectable_value(value, 1, "Yes");
            });
        }
        ui.horizontal(|ui| {
            ui.label("Actor Base (XNAM):");
            let mut actors: Vec<(u32, String)> = session.records_by_sig(*b"CREA").to_vec();
            actors.extend(session.records_by_sig(*b"NPC_").iter().cloned());
            let mut fid = state.rgdl_actor_base;
            record_dropdown(ui, "rgdl_actor_base", &actors, &mut fid);
            state.rgdl_actor_base = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Body Part Data (TNAM):");
            let bptds = session.records_by_sig(*b"BPTD");
            let mut fid = state.rgdl_body_part_data;
            record_dropdown(ui, "rgdl_bptd", &bptds, &mut fid);
            state.rgdl_body_part_data = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Death Pose (ANAM):");
            ui.add(
                egui::TextEdit::singleline(&mut state.rgdl_death_pose)
                    .desired_width(ui.available_width() - 160.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Feedback Dynamic Bones (RAFB) — {}",
            state.rgdl_feedback_bones.len()
        ));
        ui.add_space(4.0);
        let mut remove_idx = None;
        for (idx, bone) in state.rgdl_feedback_bones.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.button("✖").on_hover_text("Remove").clicked() {
                    remove_idx = Some(idx);
                }
                ui.label(format!("#{}", idx + 1));
                ui.add(egui::DragValue::new(bone).range(0..=65535));
            });
        }
        if let Some(idx) = remove_idx {
            state.rgdl_feedback_bones.remove(idx);
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Feedback Data (RAFD)");
        ui.add_space(4.0);
        for (label, value) in [
            (
                "Dynamic / Keyframe Blend Amount",
                &mut state.rgdl_dynamic_keyframe_blend,
            ),
            ("Hierarchy Gain", &mut state.rgdl_hierarchy_gain),
            ("Position Gain", &mut state.rgdl_position_gain),
            ("Velocity Gain", &mut state.rgdl_velocity_gain),
            ("Acceleration Gain", &mut state.rgdl_acceleration_gain),
            ("Snap Gain", &mut state.rgdl_snap_gain),
            ("Velocity Damping", &mut state.rgdl_velocity_damping),
            (
                "Snap Max Linear Velocity",
                &mut state.rgdl_snap_max_linear_velocity,
            ),
            (
                "Snap Max Angular Velocity",
                &mut state.rgdl_snap_max_angular_velocity,
            ),
            (
                "Snap Max Linear Distance",
                &mut state.rgdl_snap_max_linear_distance,
            ),
            (
                "Snap Max Angular Distance",
                &mut state.rgdl_snap_max_angular_distance,
            ),
            (
                "Position Max Linear Velocity",
                &mut state.rgdl_position_max_linear_velocity,
            ),
            (
                "Position Max Angular Velocity",
                &mut state.rgdl_position_max_angular_velocity,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .range(-100000.0..=100000.0),
                );
            });
        }
        ui.horizontal(|ui| {
            ui.label("Projectile Position Max Velocity:");
            ui.add(
                egui::DragValue::new(&mut state.rgdl_projectile_position_max_velocity)
                    .range(-1000000..=1000000),
            );
            ui.separator();
            ui.label("Melee Position Max Velocity:");
            ui.add(
                egui::DragValue::new(&mut state.rgdl_melee_position_max_velocity)
                    .range(0..=1000000),
            );
        });
        ui.label("Both velocities are divided by 1000 by the game.");
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Pose Matching Data (RAPS)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Match Bones:");
            ui.add(egui::DragValue::new(&mut state.rgdl_match_bone1).range(0..=65535));
            ui.add(egui::DragValue::new(&mut state.rgdl_match_bone2).range(0..=65535));
            ui.add(egui::DragValue::new(&mut state.rgdl_match_bone3).range(0..=65535));
        });
        ui.horizontal(|ui| {
            ui.label("Flags:");
            let mut disable = (state.rgdl_aps_flags & 0x01) != 0;
            if ui.checkbox(&mut disable, "Disable On Move").changed() {
                if disable {
                    state.rgdl_aps_flags |= 0x01;
                } else {
                    state.rgdl_aps_flags &= !0x01;
                }
            }
            ui.label(format!("0x{:02X}", state.rgdl_aps_flags));
        });
        for (label, value) in [
            ("Motors Strength", &mut state.rgdl_motors_strength),
            (
                "Pose Activation Delay Time",
                &mut state.rgdl_pose_activation_delay,
            ),
            (
                "Match Error Allowance",
                &mut state.rgdl_match_error_allowance,
            ),
            (
                "Displacement To Disable",
                &mut state.rgdl_displacement_to_disable,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .range(-100000.0..=100000.0),
                );
            });
        }
    });
}

/// Draw the Tree (TREE) editor body.
fn draw_tree_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Tree");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Leaf Curvature (CNAM):");
            ui.add(
                egui::DragValue::new(&mut state.tree_leaf_curvature)
                    .speed(0.01)
                    .range(-100.0..=100.0),
            );
            ui.separator();
            ui.label("Min Leaf Angle:");
            ui.add(
                egui::DragValue::new(&mut state.tree_min_leaf_angle)
                    .speed(0.01)
                    .range(-360.0..=360.0),
            );
            ui.separator();
            ui.label("Max Leaf Angle:");
            ui.add(
                egui::DragValue::new(&mut state.tree_max_leaf_angle)
                    .speed(0.01)
                    .range(-360.0..=360.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Branch Dimming:");
            ui.add(
                egui::DragValue::new(&mut state.tree_branch_dimming)
                    .speed(0.01)
                    .range(0.0..=10.0),
            );
            ui.separator();
            ui.label("Leaf Dimming:");
            ui.add(
                egui::DragValue::new(&mut state.tree_leaf_dimming)
                    .speed(0.01)
                    .range(0.0..=10.0),
            );
            ui.separator();
            ui.label("Shadow Radius:");
            ui.add(egui::DragValue::new(&mut state.tree_shadow_radius).range(-10000..=10000));
        });
        ui.horizontal(|ui| {
            ui.label("Rock Speed:");
            ui.add(
                egui::DragValue::new(&mut state.tree_rock_speed)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
            ui.separator();
            ui.label("Rustle Speed:");
            ui.add(
                egui::DragValue::new(&mut state.tree_rustle_speed)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Billboard Width (BNAM):");
            ui.add(
                egui::DragValue::new(&mut state.tree_billboard_width)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Height:");
            ui.add(
                egui::DragValue::new(&mut state.tree_billboard_height)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.tree_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.tree_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "SpeedTree Seeds (SNAM) — {}",
            state.tree_seeds.len()
        ));
        ui.add_space(4.0);
        let mut remove_idx = None;
        for (idx, seed) in state.tree_seeds.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.button("✖").on_hover_text("Remove").clicked() {
                    remove_idx = Some(idx);
                }
                ui.label(format!("#{}", idx + 1));
                ui.add(egui::DragValue::new(seed).range(0..=999999999));
            });
        }
        if let Some(idx) = remove_idx {
            state.tree_seeds.remove(idx);
        }
    });
}

/// Draw the Texture (TXST) editor body.
fn draw_txst_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Texture");
        ui.add_space(4.0);
        const SIGS: [&str; 6] = ["TX00", "TX01", "TX02", "TX03", "TX04", "TX05"];
        const LABELS: [&str; 6] = [
            "Base Image / Transparency",
            "Normal Map / Specular",
            "Environment Map Mask",
            "Glow Map",
            "Parallax Map",
            "Environment Map",
        ];
        for i in 0..6 {
            ui.horizontal(|ui| {
                ui.label(format!("{} ({})", LABELS[i], SIGS[i]));
                if let Some(text) = state.txst_textures.get_mut(i) {
                    ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(ui.available_width() - 240.0),
                    );
                }
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Flags (DNAM):");
            let mut no_spec = (state.txst_flags & 0x0001) != 0;
            if ui.checkbox(&mut no_spec, "No Specular Map").changed() {
                if no_spec {
                    state.txst_flags |= 0x0001;
                } else {
                    state.txst_flags &= !0x0001;
                }
            }
            ui.label(format!("0x{:04X}", state.txst_flags));
        });
    });

    if state.raw_subs.contains_key(b"DODT") {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Decal Data (DODT)");
            ui.add_space(4.0);
            for (label, value) in [
                ("Min Width", &mut state.txst_dodt_min_width),
                ("Max Width", &mut state.txst_dodt_max_width),
                ("Min Height", &mut state.txst_dodt_min_height),
                ("Max Height", &mut state.txst_dodt_max_height),
                ("Depth", &mut state.txst_dodt_depth),
                ("Shininess", &mut state.txst_dodt_shininess),
                ("Parallax Scale", &mut state.txst_dodt_parallax_scale),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.01)
                            .range(-100000.0..=100000.0),
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Parallax Passes:");
                ui.add(egui::DragValue::new(&mut state.txst_dodt_parallax_passes).range(0..=255));
                ui.separator();
                ui.label("Flags:");
                for (bit, name) in TXST_DODT_FLAGS {
                    let mut set = (state.txst_dodt_flags & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            state.txst_dodt_flags |= bit;
                        } else {
                            state.txst_dodt_flags &= !bit;
                        }
                    }
                }
                ui.label(format!("0x{:02X}", state.txst_dodt_flags));
            });
            ui.horizontal(|ui| {
                ui.label("Color:");
                for c in state.txst_dodt_color.iter_mut() {
                    ui.add(egui::DragValue::new(c).range(0..=255));
                }
            });
        });
    }
}

/// Draw the Voice Type (VTYP) editor body.
fn draw_vtyp_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Voice Type");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in VTYP_FLAGS {
                let mut set = (state.vtyp_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.vtyp_flags |= bit;
                    } else {
                        state.vtyp_flags &= !bit;
                    }
                }
            }
        });
        ui.label(format!(
            "Unknown flag bits: 0x{:02X}",
            state.vtyp_flags & !0x03
        ));
    });
}

/// Draw the Talking Activator (TACT) editor body.
fn draw_tact_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Talking Activator");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Looping Sound (SNAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.tact_looping_sound;
            record_dropdown(ui, "tact_snam", &sounds, &mut fid);
            state.tact_looping_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Voice Type (VNAM):");
            let vtypes = session.records_by_sig(*b"VTYP");
            let mut fid = state.tact_voice_type;
            record_dropdown(ui, "tact_vnam", &vtypes, &mut fid);
            state.tact_voice_type = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Radio Template (INAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.tact_radio_template;
            record_dropdown(ui, "tact_inam", &sounds, &mut fid);
            state.tact_radio_template = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Script (SCRI):");
            let scripts = session.records_by_sig(*b"SCPT");
            let mut fid = state.tact_script;
            record_dropdown(ui, "tact_scri", &scripts, &mut fid);
            state.tact_script = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Destruction Data — {} stage(s)",
            state.tact_destruction.len()
        ));
        ui.add_space(4.0);
        for (i, stage) in state.tact_destruction.iter().enumerate() {
            ui.label(format!(
                "  Stage {}: {} — destruction flag word 0x{:02X}",
                i + 1,
                stage.model,
                stage.flags
            ));
        }
    });
}

/// Draw the Static Collision (SCOL) editor body.
fn draw_scol_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Static Collision");
        ui.add_space(4.0);
        ui.label(format!("{} collision part(s)", state.scol_parts.len()));
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.scol_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    for (idx, part) in state.scol_parts.iter_mut().enumerate() {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong(format!("Part {}", idx + 1));
            ui.horizontal(|ui| {
                ui.label("Static (ONAM):");
                ui.add(
                    egui::TextEdit::singleline(
                        &mut part
                            .static_ref
                            .map(|v| format!("{v:08X}"))
                            .unwrap_or_default(),
                    )
                    .id_salt(format!("scol_onam_{idx}"))
                    .hint_text("00000000")
                    .desired_width(120.0),
                );
                ui.label(format!("{} placement(s)", part.placements.len()));
            });
            let mut remove_idx = None;
            for (pidx, p) in part.placements.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove").clicked() {
                        remove_idx = Some(pidx);
                    }
                    ui.label(format!("P{}", pidx + 1));
                    const AXES: [&str; 7] = ["X", "Y", "Z", "RX", "RY", "RZ", "S"];
                    for axis in 0..7 {
                        ui.label(AXES[axis]);
                        ui.add(
                            egui::DragValue::new(&mut p[axis])
                                .speed(0.01)
                                .range(-100000.0..=100000.0),
                        );
                    }
                });
            }
            if let Some(pidx) = remove_idx {
                part.placements.remove(pidx);
            }
        });
    }
}

/// Draw the Idle Animation (IDLE) editor body.
fn draw_idle_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Idle Animation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Parent Idle Animation (ANAM):");
            let idles = session.records_by_sig(*b"IDLE");
            let mut fid = state.idle_parent;
            record_dropdown(ui, "idle_parent", &idles, &mut fid);
            state.idle_parent = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Previous Sibling (ANAM):");
            let idles = session.records_by_sig(*b"IDLE");
            let mut fid = state.idle_previous_sibling;
            record_dropdown(ui, "idle_previous", &idles, &mut fid);
            state.idle_previous_sibling = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Animation Group Section (DATA):");
            egui::ComboBox::from_id_salt("idle_group")
                .selected_text(
                    IDLE_GROUPS
                        .iter()
                        .find(|(v, _)| *v == state.idle_group_section)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in IDLE_GROUPS.iter() {
                        ui.selectable_value(&mut state.idle_group_section, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Looping Min:");
            ui.add(egui::DragValue::new(&mut state.idle_looping_min).range(0..=255));
            ui.separator();
            ui.label("Looping Max:");
            ui.add(egui::DragValue::new(&mut state.idle_looping_max).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Replay Delay:");
            ui.add(egui::DragValue::new(&mut state.idle_replay_delay).range(-32768..=32767));
            ui.separator();
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.idle_flags).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.idle_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Conditions (CTDA) — {}",
            state.idle_conditions.len()
        ));
        ui.add_space(4.0);
        for (idx, cond) in state.idle_conditions.iter_mut().enumerate() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Condition {}", idx + 1));
                let mut combine = (cond.cond_type & 0x01) != 0;
                if ui.checkbox(&mut combine, "OR").changed() {
                    if combine {
                        cond.cond_type |= 0x01;
                    } else {
                        cond.cond_type &= !0x01;
                    }
                }
                let mut on_target = (cond.cond_type & 0x02) != 0;
                if ui.checkbox(&mut on_target, "Run On Target").changed() {
                    if on_target {
                        cond.cond_type |= 0x02;
                    } else {
                        cond.cond_type &= !0x02;
                    }
                }
                let mut use_global = (cond.cond_type & 0x04) != 0;
                if ui.checkbox(&mut use_global, "Use Global").changed() {
                    if use_global {
                        cond.cond_type |= 0x04;
                    } else {
                        cond.cond_type &= !0x04;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Comparison:");
                egui::ComboBox::from_id_salt(format!("idle_ctda_cmp_{idx}"))
                    .selected_text(
                        CTDA_COMPARISONS
                            .iter()
                            .find(|(mask, _)| cond.cond_type & mask == *mask)
                            .map(|(_, n)| *n)
                            .unwrap_or("Custom"),
                    )
                    .show_ui(ui, |ui| {
                        // The comparison bits are masked out and the new
                        // comparison put in, so the flag bits below stay.
                        let base = cond.cond_type & !0xE0;
                        for (mask, name) in CTDA_COMPARISONS.iter() {
                            ui.selectable_value(&mut cond.cond_type, base | mask, *name);
                        }
                    });
                ui.separator();
                ui.label("Function:");
                ui.add(egui::DragValue::new(&mut cond.function).range(0..=999));
            });
            ui.horizontal(|ui| {
                ui.label("Comparison Value:");
                ui.add(egui::DragValue::new(&mut cond.comparison_value).range(0..=u32::MAX));
                ui.separator();
                ui.label("Reference:");
                let mut reference = cond.reference.unwrap_or(0);
                ui.add(egui::DragValue::new(&mut reference).range(0..=u32::MAX));
                cond.reference = Some(reference);
            });
            ui.horizontal(|ui| {
                ui.label("Parameter 1:");
                ui.add(egui::DragValue::new(&mut cond.parameter1).range(0..=u32::MAX));
                ui.separator();
                ui.label("Parameter 2:");
                ui.add(egui::DragValue::new(&mut cond.parameter2).range(0..=u32::MAX));
                ui.separator();
                ui.label("Run On:");
                ui.add(egui::DragValue::new(&mut cond.run_on).range(0..=u32::MAX));
            });
        }
    });
}

/// Draw the Media Set (MSET) editor body.
fn draw_mset_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Media Set");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type (NAM1):");
            egui::ComboBox::from_id_salt("mset_type")
                .selected_text(
                    MSET_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.mset_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in MSET_TYPES.iter() {
                        ui.selectable_value(&mut state.mset_type, *v, *name);
                    }
                });
        });

        const TRACK_LABELS: [&str; 6] = [
            "Loop / Battle / Day Outer",
            "Explore / Day Middle",
            "Suspense / Day Inner",
            "Night Outer",
            "Night Middle",
            "Night Inner",
        ];
        for (i, label) in TRACK_LABELS.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", label));
                if let Some(text) = state.mset_tracks.get_mut(i) {
                    ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(ui.available_width() - 220.0),
                    );
                }
                if let Some(level) = state.mset_levels.get_mut(i) {
                    ui.add(egui::DragValue::new(level).speed(0.1).range(-100.0..=100.0));
                }
            });
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Enable Flags (PNAM):");
            for (bit, name) in MSET_ENABLE_FLAGS {
                let mut set = (state.mset_enable_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.mset_enable_flags |= bit;
                    } else {
                        state.mset_enable_flags &= !bit;
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Wait Time / Min Time On (DNAM):");
            ui.add(
                egui::DragValue::new(&mut state.mset_wait_time)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Loop Fade Out (ENAM):");
            ui.add(
                egui::DragValue::new(&mut state.mset_loop_fade_out)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Recovery Time (FNAM):");
            ui.add(
                egui::DragValue::new(&mut state.mset_recovery_time)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Nighttime Max (GNAM):");
            ui.add(
                egui::DragValue::new(&mut state.mset_nighttime_max)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Intro / Daytime (HNAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.mset_intro;
            record_dropdown(ui, "mset_hnam", &sounds, &mut fid);
            state.mset_intro = fid;
            ui.separator();
            ui.label("Outro / Nighttime (INAM):");
            let mut fid = state.mset_outro;
            record_dropdown(ui, "mset_inam", &sounds, &mut fid);
            state.mset_outro = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Boundaries (JNAM) — {}",
            state.mset_boundaries.len()
        ));
        ui.add_space(4.0);
        let mut remove_idx = None;
        for (idx, boundary) in state.mset_boundaries.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.button("✖").on_hover_text("Remove").clicked() {
                    remove_idx = Some(idx);
                }
                ui.label(format!("#{}", idx + 1));
                ui.add(egui::DragValue::new(boundary).speed(0.1).range(0.0..=100.0));
            });
        }
        if let Some(idx) = remove_idx {
            state.mset_boundaries.remove(idx);
        }
    });
}

/// Draw the Movable Static (MSTT) editor body.
fn draw_mstt_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Movable Static");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Unknown (DATA):");
            ui.add(egui::DragValue::new(&mut state.mstt_data).range(0..=255));
        });
        ui.horizontal(|ui| {
            ui.label("Sound (SNAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.mstt_sound;
            record_dropdown(ui, "mstt_snam", &sounds, &mut fid);
            state.mstt_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.mstt_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Destruction Data — {} stage(s)",
            state.mstt_destruction.len()
        ));
        ui.add_space(4.0);
        for (i, stage) in state.mstt_destruction.iter().enumerate() {
            ui.label(format!(
                "  Stage {}: {} — destruction flag word 0x{:02X}",
                i + 1,
                stage.model,
                stage.flags
            ));
        }
    });
}

/// Draw the Water (PWAT) editor body.
fn draw_pwat_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Water");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in PWAT_FLAGS {
                let mut set = (state.pwat_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.pwat_flags |= bit;
                    } else {
                        state.pwat_flags &= !bit;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Water (DNAM):");
            let waters = session.records_by_sig(*b"WATR");
            let mut fid = state.pwat_water;
            record_dropdown(ui, "pwat_water", &waters, &mut fid);
            state.pwat_water = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.pwat_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });
}

/// Draw the Form List (FLST) editor body.
fn draw_flst_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Form List");
        ui.add_space(4.0);
        ui.label(format!("{} entry/entries", state.flst_entries.len()));
    });

    // The list holds FormIDs of any record type, so each entry is edited
    // as a raw FormID and its name is looked up in the session.
    let mut remove_idx = None;
    for (idx, entry) in state.flst_entries.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui.button("✖").on_hover_text("Remove").clicked() {
                remove_idx = Some(idx);
            }
            ui.label(format!("#{}", idx + 1));
            let mut hex = format!("{entry:08X}");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut hex)
                        .id_salt(format!("flst_{idx}"))
                        .hint_text("00000000")
                        .desired_width(110.0),
                )
                .changed()
            {
                if let Ok(v) = u32::from_str_radix(hex.trim(), 16) {
                    *entry = v;
                }
            }
        });
    }
    if let Some(idx) = remove_idx {
        state.flst_entries.remove(idx);
    }
}

/// Draw the Leveled Item (LVLI) editor body.
/// Draw the Leveled NPC (LVLN) editor body.
/// Draw the Message (MESG) editor body.
fn draw_mesg_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Message");
        ui.add_space(4.0);
        ui.label("Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.mesg_desc)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
        ui.horizontal(|ui| {
            ui.label("Icon (INAM):");
            let icons = session.records_by_sig(*b"MICN");
            let mut fid = state.mesg_icon;
            record_dropdown(ui, "mesg_icon", &icons, &mut fid);
            state.mesg_icon = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Flags (DNAM):");
            for (bit, name) in MESG_FLAGS {
                let mut set = (state.mesg_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.mesg_flags |= bit;
                    } else {
                        state.mesg_flags &= !bit;
                    }
                }
            }
            ui.label(format!("0x{:08X}", state.mesg_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Display Time (TNAM):");
            ui.add(egui::DragValue::new(&mut state.mesg_display_time).range(0..=100000));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Menu Buttons — {}", state.mesg_buttons.len()));
        ui.add_space(4.0);
        for (idx, button) in state.mesg_buttons.iter_mut().enumerate() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(format!("Button {}", idx + 1));
                ui.add(
                    egui::TextEdit::singleline(&mut button.text)
                        .desired_width(ui.available_width() - 160.0),
                );
            });
            ui.label(format!("{} condition(s)", button.conditions.len()));
        }
    });
}

/// Draw the Miscellaneous (MISC) editor body.
fn draw_misc_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Miscellaneous");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (DATA):");
            ui.add(egui::DragValue::new(&mut state.misc_value).range(-1000000..=1000000));
            ui.separator();
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.misc_weight)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.misc_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.misc_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Script (SCRI):");
            let scripts = session.records_by_sig(*b"SCPT");
            let mut fid = state.misc_script;
            record_dropdown(ui, "misc_scri", &scripts, &mut fid);
            state.misc_script = fid;
        });
        for (label, value) in [
            ("Sound - Pick Up (YNAM)", &mut state.misc_pickup_sound),
            ("Sound - Drop (ZNAM)", &mut state.misc_drop_sound),
            ("Sound - Random (RNAM)", &mut state.misc_random_sound),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                let sounds = session.records_by_sig(*b"SOUN");
                let mut fid = *value;
                record_dropdown(ui, label, &sounds, &mut fid);
                *value = fid;
            });
        }
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.misc_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Destruction Data — {} stage(s)",
            state.misc_destruction.len()
        ));
        ui.add_space(4.0);
        for (i, stage) in state.misc_destruction.iter().enumerate() {
            ui.label(format!(
                "  Stage {}: {} — destruction flag word 0x{:02X}",
                i + 1,
                stage.model,
                stage.flags
            ));
        }
    });
}

/// Draw the Spell (SPEL) editor body.
fn draw_spel_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Spell");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type (SPIT):");
            egui::ComboBox::from_id_salt("spel_type")
                .selected_text(
                    SPEL_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.spel_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in SPEL_TYPES.iter() {
                        ui.selectable_value(&mut state.spel_type, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Cost:");
            ui.add(egui::DragValue::new(&mut state.spel_cost).range(0..=1000000));
            ui.separator();
            ui.label("Level:");
            ui.add(egui::DragValue::new(&mut state.spel_level).range(0..=1000));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Flags:");
            for (bit, name) in SPEL_FLAGS {
                let mut set = (state.spel_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.spel_flags |= bit;
                    } else {
                        state.spel_flags &= !bit;
                    }
                }
            }
            ui.label(format!("0x{:02X}", state.spel_flags));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Effects — {}", state.spel_effects.len()));
        ui.add_space(4.0);
        for (idx, effect) in state.spel_effects.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{}", idx + 1));
                ui.label("Effect (EFID):");
                ui.add(
                    egui::TextEdit::singleline(
                        &mut effect
                            .effect
                            .map(|v| format!("{v:08X}"))
                            .unwrap_or_default(),
                    )
                    .id_salt(format!("spel_efid_{idx}"))
                    .hint_text("00000000")
                    .desired_width(110.0),
                );
                ui.label(format!("{} parameter byte(s)", effect.parameters.len()));
            });
        }
    });
}

/// Draw the Static (STAT) editor body.
fn draw_stat_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Static");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Passthrough Sound (BRUS):");
            egui::ComboBox::from_id_salt("stat_brus")
                .selected_text(
                    STAT_BUSHS
                        .iter()
                        .find(|(v, _)| *v == state.stat_passthrough)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in STAT_BUSHS.iter() {
                        ui.selectable_value(&mut state.stat_passthrough, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Sound - Looping / Random (RNAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.stat_looping_sound;
            record_dropdown(ui, "stat_rnam", &sounds, &mut fid);
            state.stat_looping_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.stat_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });
}

/// Draw the Key (KEYM) editor body.
fn draw_keym_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Key");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value (DATA):");
            ui.add(egui::DragValue::new(&mut state.keym_value).range(-1000000..=1000000));
            ui.separator();
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.keym_weight)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.keym_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.keym_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Script (SCRI):");
            let scripts = session.records_by_sig(*b"SCPT");
            let mut fid = state.keym_script;
            record_dropdown(ui, "keym_scri", &scripts, &mut fid);
            state.keym_script = fid;
        });
        for (label, value) in [
            ("Sound - Pick Up (YNAM)", &mut state.keym_pickup_sound),
            ("Sound - Drop (ZNAM)", &mut state.keym_drop_sound),
            ("Sound - Random (RNAM)", &mut state.keym_random_sound),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                let sounds = session.records_by_sig(*b"SOUN");
                let mut fid = *value;
                record_dropdown(ui, label, &sounds, &mut fid);
                *value = fid;
            });
        }
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.keym_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Destruction Data — {} stage(s)",
            state.keym_destruction.len()
        ));
        ui.add_space(4.0);
        for (i, stage) in state.keym_destruction.iter().enumerate() {
            ui.label(format!(
                "  Stage {}: {} — destruction flag word 0x{:02X}",
                i + 1,
                stage.model,
                stage.flags
            ));
        }
    });
}

/// Draw the Ingredient (INGR) editor body.
fn draw_ingr_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Ingredient");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Equipment Type (ETYP):");
            egui::ComboBox::from_id_salt("ingr_etyp")
                .selected_text(
                    ETYP_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.ingr_equipment_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in ETYP_TYPES.iter() {
                        ui.selectable_value(&mut state.ingr_equipment_type, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Weight (DATA):");
            ui.add(
                egui::DragValue::new(&mut state.ingr_weight)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Effect Value (ENIT):");
            ui.add(egui::DragValue::new(&mut state.ingr_effect_value).range(-1000000..=1000000));
            ui.separator();
            ui.label("Effect Flags:");
            for (bit, name) in INGR_EFFECT_FLAGS {
                let mut set = (state.ingr_effect_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.ingr_effect_flags |= bit;
                    } else {
                        state.ingr_effect_flags &= !bit;
                    }
                }
            }
            ui.label(format!("0x{:02X}", state.ingr_effect_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ingr_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ingr_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Script (SCRI):");
            let scripts = session.records_by_sig(*b"SCPT");
            let mut fid = state.ingr_script;
            record_dropdown(ui, "ingr_scri", &scripts, &mut fid);
            state.ingr_script = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ingr_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Effects — {}", state.ingr_effects.len()));
        ui.add_space(4.0);
        for (idx, effect) in state.ingr_effects.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{}", idx + 1));
                ui.label("Effect (EFID):");
                ui.add(
                    egui::TextEdit::singleline(
                        &mut effect
                            .effect
                            .map(|v| format!("{v:08X}"))
                            .unwrap_or_default(),
                    )
                    .id_salt(format!("ingr_efid_{idx}"))
                    .hint_text("00000000")
                    .desired_width(110.0),
                );
                ui.label(format!("{} parameter byte(s)", effect.parameters.len()));
            });
        }
    });
}

/// Draw the Impact (IPCT) editor body.
fn draw_ipct_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Impact");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Effect - Duration:");
            ui.add(
                egui::DragValue::new(&mut state.ipct_duration)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
            ui.separator();
            ui.label("Effect - Orientation:");
            egui::ComboBox::from_id_salt("ipct_orientation")
                .selected_text(
                    IPCT_ORIENTATIONS
                        .iter()
                        .find(|(v, _)| *v == state.ipct_orientation)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in IPCT_ORIENTATIONS.iter() {
                        ui.selectable_value(&mut state.ipct_orientation, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Angle Threshold:");
            ui.add(
                egui::DragValue::new(&mut state.ipct_angle_threshold)
                    .speed(0.1)
                    .range(-360.0..=360.0),
            );
            ui.separator();
            ui.label("Placement Radius:");
            ui.add(
                egui::DragValue::new(&mut state.ipct_placement_radius)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Sound Level:");
            egui::ComboBox::from_id_salt("ipct_sound_level")
                .selected_text(
                    SOUND_LEVELS
                        .get(state.ipct_sound_level as usize)
                        .copied()
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in SOUND_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut state.ipct_sound_level, i as u32, *name);
                    }
                });
            ui.separator();
            ui.label("Flags:");
            let mut no_decal = (state.ipct_flags & 0x01) != 0;
            if ui.checkbox(&mut no_decal, "No Decal Data").changed() {
                if no_decal {
                    state.ipct_flags |= 0x01;
                } else {
                    state.ipct_flags &= !0x01;
                }
            }
            ui.label(format!("0x{:08X}", state.ipct_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Texture Set (DNAM):");
            let textures = session.records_by_sig(*b"TXST");
            let mut fid = state.ipct_texture_set;
            record_dropdown(ui, "ipct_dnam", &textures, &mut fid);
            state.ipct_texture_set = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Sound 1 (SNAM):");
            let sounds = session.records_by_sig(*b"SOUN");
            let mut fid = state.ipct_sound1;
            record_dropdown(ui, "ipct_snam", &sounds, &mut fid);
            state.ipct_sound1 = fid;
            ui.separator();
            ui.label("Sound 2 (NAM1):");
            let mut fid = state.ipct_sound2;
            record_dropdown(ui, "ipct_nam1", &sounds, &mut fid);
            state.ipct_sound2 = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ipct_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    if let Some(dodt) = state.ipct_dodt.as_mut() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Decal Data (DODT)");
            ui.add_space(4.0);
            for (label, value) in [
                ("Min Width", &mut dodt.min_width),
                ("Max Width", &mut dodt.max_width),
                ("Min Height", &mut dodt.min_height),
                ("Max Height", &mut dodt.max_height),
                ("Depth", &mut dodt.depth),
                ("Shininess", &mut dodt.shininess),
                ("Parallax Scale", &mut dodt.parallax_scale),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.01)
                            .range(-100000.0..=100000.0),
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Parallax Passes:");
                ui.add(egui::DragValue::new(&mut dodt.parallax_passes).range(0..=255));
                ui.separator();
                ui.label("Flags:");
                for (bit, name) in TXST_DODT_FLAGS {
                    let mut set = (dodt.flags & bit) != 0;
                    if ui.checkbox(&mut set, name).changed() {
                        if set {
                            dodt.flags |= bit;
                        } else {
                            dodt.flags &= !bit;
                        }
                    }
                }
                ui.label(format!("0x{:02X}", dodt.flags));
            });
            ui.horizontal(|ui| {
                ui.label("Color:");
                for c in dodt.color.iter_mut() {
                    ui.add(egui::DragValue::new(c).range(0..=255));
                }
            });
        });
    }
}

/// Draw the Impact Data Set (IPDS) editor body.
fn draw_ipds_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    const MATERIALS: [&str; 12] = [
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
    ui.group(|ui| {
        ui.strong("Impact Data Set");
        ui.add_space(4.0);
        let impacts = session.records_by_sig(*b"IPCT");
        for (i, name) in MATERIALS.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(*name);
                let mut fid = state.ipds_impacts.get(i).copied().flatten();
                record_dropdown(ui, &format!("ipds_{i}"), &impacts, &mut fid);
                if let Some(slot) = state.ipds_impacts.get_mut(i) {
                    *slot = fid;
                }
            });
        }
    });
}

/// Draw one list of recipe ingredients or outputs.
fn draw_recipe_items(ui: &mut egui::Ui, title: &str, items: &mut [RecipeItem]) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("{title} — {}", items.len()));
        ui.add_space(4.0);
        for (idx, item) in items.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{}", idx + 1));
                ui.label("Item:");
                let mut hex = item.item.map(|v| format!("{v:08X}")).unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut hex)
                            .id_salt(format!("rcpe_item_{title}_{idx}"))
                            .hint_text("00000000")
                            .desired_width(110.0),
                    )
                    .changed()
                {
                    if let Ok(v) = u32::from_str_radix(hex.trim(), 16) {
                        item.item = Some(v);
                    }
                }
                ui.label("Quantity:");
                ui.add(egui::DragValue::new(&mut item.quantity).range(0..=100000));
            });
        }
    });
}

/// Draw the Recipe (RCPE) editor body.
fn draw_rcpe_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Recipe");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Skill:");
            ui.add(egui::DragValue::new(&mut state.rcpe_skill).range(-1..=200));
            ui.separator();
            ui.label("Level:");
            ui.add(egui::DragValue::new(&mut state.rcpe_level).range(0..=100));
        });
        ui.horizontal(|ui| {
            ui.label("Category:");
            let categories = session.records_by_sig(*b"RCCT");
            let mut fid = state.rcpe_category;
            record_dropdown(ui, "rcpe_category", &categories, &mut fid);
            state.rcpe_category = fid;
            ui.separator();
            ui.label("Sub-Category:");
            let mut fid = state.rcpe_sub_category;
            record_dropdown(ui, "rcpe_sub_category", &categories, &mut fid);
            state.rcpe_sub_category = fid;
        });
    });

    draw_recipe_items(ui, "Ingredients", &mut state.rcpe_ingredients);
    draw_recipe_items(ui, "Outputs", &mut state.rcpe_outputs);

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Conditions (CTDA) — {}",
            state.rcpe_conditions.len()
        ));
        ui.add_space(4.0);
        for (idx, cond) in state.rcpe_conditions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Condition {}", idx + 1));
                ui.label("Function:");
                ui.add(egui::DragValue::new(&mut cond.function).range(0..=999));
                ui.label("Comparison Value:");
                ui.add(egui::DragValue::new(&mut cond.comparison_value).range(0..=u32::MAX));
            });
        }
    });
}

/// The package types, from the `PKDT` Type field.
const PACK_TYPES: &[(u8, &str)] = &[
    (0, "Find"),
    (1, "Follow"),
    (2, "Escort"),
    (3, "Eat"),
    (4, "Sleep"),
    (5, "Wander"),
    (6, "Travel"),
    (7, "Accompany"),
    (8, "Use Item At"),
    (9, "Ambush"),
    (10, "Flee Not Combat"),
    (11, "Unknown"),
    (12, "Sandbox"),
    (13, "Patrol"),
    (14, "Guard"),
    (15, "Dialogue"),
    (16, "Use Weapon"),
];

/// The days of the week, from the `PSDT` Day of Week field.
const PACK_DAYS: &[(i8, &str)] = &[
    (-1, "Any"),
    (0, "Sunday"),
    (1, "Monday"),
    (2, "Tuesday"),
    (3, "Wednesday"),
    (4, "Thursday"),
    (5, "Friday"),
    (6, "Saturday"),
    (7, "Weekdays"),
    (8, "Weekends"),
    (9, "Monday, Wednesday, Friday"),
    (10, "Tuesday, Thursday"),
];

/// The location types, from the `PLDT` and `PLD2` Type field.
const PACK_LOCATION_TYPES: &[(u32, &str)] = &[
    (0, "Near Reference"),
    (1, "In Cell"),
    (2, "Near Current Location"),
    (3, "Near Editor Location"),
    (4, "Object ID"),
    (5, "Object Type"),
    (6, "Near Linked Reference"),
    (7, "At Package Location"),
];

/// The target types, from the `PTDT` and `PTD2` Type field.
const PACK_TARGET_TYPES: &[(i32, &str)] = &[
    (0, "Specific Reference"),
    (1, "Object ID"),
    (2, "Object Type"),
    (3, "Linked Reference"),
];

/// The dialog types, from the `PKDD` Dialog Type field.
const PACK_DIALOG_TYPES: &[(u32, &str)] = &[(0, "Say To"), (1, "Say To")];

/// Draw the Placed Missile (PMIS) editor body.
fn draw_pmis_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Placed Missile");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Base:");
            let projs = session.records_by_sig(*b"PROJ");
            let mut fid = state.pmis_base;
            record_dropdown(ui, "pmis_base", &projs, &mut fid);
            state.pmis_base = fid;
            ui.separator();
            ui.label("Encounter Zone:");
            let zones = session.records_by_sig(*b"ECZN");
            let mut fid = state.pmis_encounter_zone;
            record_dropdown(ui, "pmis_zone", &zones, &mut fid);
            state.pmis_encounter_zone = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Idle Time:");
            ui.add(egui::DragValue::new(&mut state.pmis_idle_time).speed(0.01));
            ui.separator();
            ui.label("Scale:");
            ui.add(egui::DragValue::new(&mut state.pmis_scale).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Idle:");
            let idles = session.records_by_sig(*b"IDLE");
            let mut fid = state.pmis_idle;
            record_dropdown(ui, "pmis_idle", &idles, &mut fid);
            state.pmis_idle = fid;
            ui.separator();
            ui.label("Topic:");
            let topics = session.records_by_sig(*b"DIAL");
            let mut fid = state.pmis_topic;
            record_dropdown(ui, "pmis_topic", &topics, &mut fid);
            state.pmis_topic = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Owner:");
            let mut fid = state.pmis_owner;
            formid_field(ui, "pmis_owner", &mut fid);
            state.pmis_owner = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Position");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Position:");
            for i in 0..3 {
                ui.add(egui::DragValue::new(&mut state.pmis_position[i]).speed(0.5));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Rotation:");
            for i in 0..3 {
                ui.add(egui::DragValue::new(&mut state.pmis_rotation[i]).speed(0.01));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Bounding Radius:");
            ui.add(egui::DragValue::new(&mut state.pmis_bounding_radius).speed(0.01));
            ui.separator();
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.pmis_flags).range(0..=u32::MAX));
        });
    });
}

/// Draw the Constructible Object (COBJ) editor body.
fn draw_cobj_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Constructible Object");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value:");
            ui.add(egui::DragValue::new(&mut state.cobj_value));
            ui.separator();
            ui.label("Weight:");
            ui.add(egui::DragValue::new(&mut state.cobj_weight).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Model Path (.nif):");
            ui.add(
                egui::TextEdit::singleline(&mut state.cobj_model.modl)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });
}

/// Draw the Image Space Adapter (IMAD) editor body.
fn draw_imad_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    let h = &mut state.imad_header;
    ui.group(|ui| {
        ui.strong("Data (DNAM)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut h.flags).range(0..=u32::MAX));
            ui.separator();
            ui.label("Duration:");
            ui.add(egui::DragValue::new(&mut h.duration).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Eye Adapt Speed:");
            ui.add(egui::DragValue::new(&mut h.hdr_eye_adapt_speed_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_eye_adapt_speed_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Bloom Blur Radius:");
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_blur_radius_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_blur_radius_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Bloom Threshold:");
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_threshold_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_threshold_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Bloom Scale:");
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_scale_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_bloom_scale_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Target Lum Min:");
            ui.add(egui::DragValue::new(&mut h.hdr_target_lum_min_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_target_lum_min_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Target Lum Max:");
            ui.add(egui::DragValue::new(&mut h.hdr_target_lum_max_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_target_lum_max_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Sunlight Scale:");
            ui.add(egui::DragValue::new(&mut h.hdr_sunlight_scale_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_sunlight_scale_add));
        });
        ui.horizontal(|ui| {
            ui.label("HDR Sky Scale:");
            ui.add(egui::DragValue::new(&mut h.hdr_sky_scale_mult));
            ui.add(egui::DragValue::new(&mut h.hdr_sky_scale_add));
        });
        ui.horizontal(|ui| {
            ui.label("Cinematic Saturation:");
            ui.add(egui::DragValue::new(&mut h.cinematic_saturation_mult));
            ui.add(egui::DragValue::new(&mut h.cinematic_saturation_add));
        });
        ui.horizontal(|ui| {
            ui.label("Cinematic Brightness:");
            ui.add(egui::DragValue::new(&mut h.cinematic_brightness_mult));
            ui.add(egui::DragValue::new(&mut h.cinematic_brightness_add));
        });
        ui.horizontal(|ui| {
            ui.label("Cinematic Contrast:");
            ui.add(egui::DragValue::new(&mut h.cinematic_contrast_mult));
            ui.add(egui::DragValue::new(&mut h.cinematic_contrast_add));
        });
        ui.horizontal(|ui| {
            ui.label("Tint Color:");
            ui.add(egui::DragValue::new(&mut h.tint_color));
            ui.separator();
            ui.label("Fade Color:");
            ui.add(egui::DragValue::new(&mut h.fade_color));
        });
        ui.horizontal(|ui| {
            ui.label("Blur Radius:");
            ui.add(egui::DragValue::new(&mut h.blur_radius));
            ui.separator();
            ui.label("Double Vision:");
            ui.add(egui::DragValue::new(&mut h.double_vision_strength));
        });
        ui.horizontal(|ui| {
            ui.label("Radial Blur Strength:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_strength));
            ui.separator();
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Radial Blur Ramp Up:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_ramp_up));
            ui.separator();
            ui.label("Start:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_start));
        });
        ui.horizontal(|ui| {
            ui.label("Radial Blur Center:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_center_x));
            ui.add(egui::DragValue::new(&mut h.radial_blur_center_y));
        });
        ui.horizontal(|ui| {
            ui.label("Radial Blur Ramp Down:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_ramp_down));
            ui.separator();
            ui.label("Down Start:");
            ui.add(egui::DragValue::new(&mut h.radial_blur_down_start));
        });
        ui.horizontal(|ui| {
            ui.label("DoF Strength:");
            ui.add(egui::DragValue::new(&mut h.dof_strength));
            ui.separator();
            ui.label("Distance:");
            ui.add(egui::DragValue::new(&mut h.dof_distance));
        });
        ui.horizontal(|ui| {
            ui.label("DoF Range:");
            ui.add(egui::DragValue::new(&mut h.dof_range));
            ui.separator();
            ui.label("DoF Flags:");
            ui.add(egui::DragValue::new(&mut h.dof_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Motion Blur Strength:");
            ui.add(egui::DragValue::new(&mut h.motion_blur_strength));
        });
    });

    ui.add_space(6.0);
    for curve in state.imad_curves.iter_mut() {
        ui.group(|ui| {
            ui.strong(format!("{} — {} keys", curve.name, curve.keys.len()));
            ui.add_space(4.0);
            for key in curve.keys.iter_mut() {
                ui.horizontal(|ui| {
                    ui.label("Time:");
                    ui.add(egui::DragValue::new(&mut key.time).speed(0.01));
                    if curve.is_color {
                        ui.label("R:");
                        ui.add(egui::DragValue::new(&mut key.value).speed(0.01));
                        ui.label("G:");
                        ui.add(egui::DragValue::new(&mut key.green).speed(0.01));
                        ui.label("B:");
                        ui.add(egui::DragValue::new(&mut key.blue).speed(0.01));
                        ui.label("A:");
                        ui.add(egui::DragValue::new(&mut key.alpha).speed(0.01));
                    } else {
                        ui.label("Value:");
                        ui.add(egui::DragValue::new(&mut key.value).speed(0.01));
                    }
                });
            }
        });
    }
}

/// The region data entry types, from the `RDAT` Type field.
const REGN_ENTRY_TYPES: &[(u32, &str)] = &[
    (0, "Unknown"),
    (1, "Unknown"),
    (2, "Objects"),
    (3, "Weather"),
    (4, "Map"),
    (5, "Land"),
    (6, "Grass"),
    (7, "Sound"),
    (8, "Imposter"),
    (9, "Unknown"),
];

/// The music types, from the `RDMD` field.
const REGN_MUSIC_TYPES: &[(u32, &str)] = &[(0, "Default"), (1, "Public"), (2, "Dungeon")];

/// Draw the Effect Shader (EFSH) editor body.
fn draw_efsh_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Textures");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Fill:");
            ui.add(
                egui::TextEdit::singleline(&mut state.efsh_icon)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Particle:");
            ui.add(
                egui::TextEdit::singleline(&mut state.efsh_ico2)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Holes:");
            ui.add(
                egui::TextEdit::singleline(&mut state.efsh_nam7)
                    .desired_width(ui.available_width() - 140.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Data (DATA)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.efsh_data.flags).range(0..=u8::MAX));
            ui.label(format!("0x{:02X}", state.efsh_data.flags));
        });
        for field in state.efsh_data.fields.iter_mut() {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", field.name));
                match &mut field.value {
                    EfshValue::F32(v) => {
                        ui.add(egui::DragValue::new(v).speed(0.01));
                    }
                    EfshValue::U32(v) => {
                        ui.add(egui::DragValue::new(v).range(0..=u32::MAX));
                    }
                    EfshValue::Fid(v) => {
                        ui.add(egui::DragValue::new(v).range(0..=u32::MAX));
                    }
                    EfshValue::Rgba(v) => {
                        let mut rgb = [v[0], v[1], v[2]];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            v[0] = rgb[0];
                            v[1] = rgb[1];
                            v[2] = rgb[2];
                        }
                        ui.add(egui::DragValue::new(&mut v[3]).range(0..=u8::MAX));
                    }
                }
            });
        }
    });
}

/// Draw the Region (REGN) editor body.
fn draw_regn_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Region");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Map Color:");
            let mut rgb = [
                state.regn_map_color[0],
                state.regn_map_color[1],
                state.regn_map_color[2],
            ];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                state.regn_map_color[0] = rgb[0];
                state.regn_map_color[1] = rgb[1];
                state.regn_map_color[2] = rgb[2];
            }
            ui.label("Alpha:");
            ui.add(egui::DragValue::new(&mut state.regn_map_color[3]).range(0..=u8::MAX));
            ui.separator();
            ui.label("Worldspace:");
            let worlds = session.records_by_sig(*b"WRLD");
            let mut fid = state.regn_worldspace;
            record_dropdown(ui, "regn_worldspace", &worlds, &mut fid);
            state.regn_worldspace = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Areas:");
            ui.label(format!("{}", state.regn_areas.len()));
        });
        for (i, area) in state.regn_areas.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Area {}", i + 1));
                ui.label("Edge Fall-Off:");
                ui.add(egui::DragValue::new(&mut area.edge_falloff));
                ui.separator();
                ui.label("Points:");
                ui.label(format!("{}", area.points.len()));
            });
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Region Data Entries — {}",
            state.regn_entries.len()
        ));
        ui.add_space(4.0);
        for (i, entry) in state.regn_entries.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Entry {}", i + 1));
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt("regn_entry_type")
                        .selected_text(
                            REGN_ENTRY_TYPES
                                .iter()
                                .find(|(v, _)| *v == entry.kind)
                                .map(|(_, n)| *n)
                                .unwrap_or("Custom"),
                        )
                        .show_ui(ui, |ui| {
                            for (v, name) in REGN_ENTRY_TYPES.iter() {
                                ui.selectable_value(&mut entry.kind, *v, *name);
                            }
                        });
                    ui.separator();
                    ui.label("Flags:");
                    ui.add(egui::DragValue::new(&mut entry.flags).range(0..=u8::MAX));
                });
                if let Some(name) = &entry.map_name {
                    ui.horizontal(|ui| {
                        ui.label("Map Name:");
                        ui.label(name);
                    });
                }
                if entry.music_type.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Music Type:");
                        egui::ComboBox::from_id_salt("regn_music_type")
                            .selected_text(
                                REGN_MUSIC_TYPES
                                    .iter()
                                    .find(|(val, _)| *val == entry.music_type.unwrap_or(0))
                                    .map(|(_, n)| *n)
                                    .unwrap_or("Custom"),
                            )
                            .show_ui(ui, |ui| {
                                for (val, name) in REGN_MUSIC_TYPES.iter() {
                                    ui.selectable_value(&mut entry.music_type, Some(*val), *name);
                                }
                            });
                        ui.separator();
                        ui.label("Music:");
                        let musics = session.records_by_sig(*b"MUSC");
                        let mut fid = entry.music;
                        record_dropdown(ui, "regn_music", &musics, &mut fid);
                        entry.music = fid;
                    });
                }
                if entry.incidental_media.is_some() || !entry.battle_media.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Incidental MediaSet:");
                        let sets = session.records_by_sig(*b"MSET");
                        let mut fid = entry.incidental_media;
                        record_dropdown(ui, "regn_incidental", &sets, &mut fid);
                        entry.incidental_media = fid;
                    });
                    ui.horizontal(|ui| {
                        ui.label("Battle MediaSets:");
                        ui.label(format!("{}", entry.battle_media.len()));
                    });
                }
                if !entry.objects.is_empty() {
                    ui.label(format!("Objects: {}", entry.objects.len()));
                    for o in entry.objects.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.label("Object:");
                            let mut fid = Some(o.object);
                            formid_field(ui, "regn_object", &mut fid);
                            o.object = fid.unwrap_or(0);
                            ui.label("Density:");
                            ui.add(egui::DragValue::new(&mut o.density).speed(0.01));
                            ui.label("Flags:");
                            ui.add(egui::DragValue::new(&mut o.flags).range(0..=u8::MAX));
                        });
                    }
                }
                if !entry.grasses.is_empty() {
                    ui.label(format!("Grasses: {}", entry.grasses.len()));
                }
                if !entry.sounds.is_empty() {
                    ui.label(format!("Sounds: {}", entry.sounds.len()));
                    for s in entry.sounds.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.label("Sound:");
                            let mut fid = Some(s.sound);
                            formid_field(ui, "regn_sound", &mut fid);
                            s.sound = fid.unwrap_or(0);
                            ui.label("Flags:");
                            ui.add(egui::DragValue::new(&mut s.flags).range(0..=u32::MAX));
                            ui.label("Chance:");
                            ui.add(egui::DragValue::new(&mut s.chance).range(0..=u32::MAX));
                        });
                    }
                }
                if !entry.weather.is_empty() {
                    ui.label(format!("Weather Types: {}", entry.weather.len()));
                    for w in entry.weather.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.label("Weather:");
                            let mut fid = Some(w.weather);
                            formid_field(ui, "regn_weather", &mut fid);
                            w.weather = fid.unwrap_or(0);
                            ui.label("Chance:");
                            ui.add(egui::DragValue::new(&mut w.chance).range(0..=u32::MAX));
                        });
                    }
                }
                if !entry.imposters.is_empty() {
                    ui.label(format!("Imposters: {}", entry.imposters.len()));
                }
            });
        }
    });
}

/// Draw the Package (PACK) editor body.
fn draw_pack_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("General");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("pack_type")
                .selected_text(
                    PACK_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.pack_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in PACK_TYPES.iter() {
                        ui.selectable_value(&mut state.pack_type, *v, *name);
                    }
                });
            ui.separator();
            ui.label("General Flags:");
            ui.add(egui::DragValue::new(&mut state.pack_general_flags).range(0..=u32::MAX));
            ui.label(format!("0x{:08X}", state.pack_general_flags));
        });
        ui.horizontal(|ui| {
            ui.label("Fallout Behaviour Flags:");
            ui.add(
                egui::DragValue::new(&mut state.pack_fallout_behavior_flags).range(0..=u16::MAX),
            );
            ui.separator();
            ui.label("Type-Specific Flags:");
            ui.add(
                egui::DragValue::new(&mut state.pack_type_specific_flags.unwrap_or(0))
                    .range(0..=u16::MAX),
            );
            if state.pack_type_specific_flags.is_none() {
                state.pack_type_specific_flags = Some(0);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Unused:");
            ui.add(egui::DragValue::new(&mut state.pack_unused).range(0..=u8::MAX));
            ui.separator();
            ui.label("Unused 2:");
            ui.add(egui::DragValue::new(&mut state.pack_unused2[0]).range(0..=u8::MAX));
            ui.add(egui::DragValue::new(&mut state.pack_unused2[1]).range(0..=u8::MAX));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Schedule");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Month:");
            ui.add(egui::DragValue::new(&mut state.pack_month).range(-128..=127));
            ui.separator();
            ui.label("Day of Week:");
            egui::ComboBox::from_id_salt("pack_dow")
                .selected_text(
                    PACK_DAYS
                        .iter()
                        .find(|(v, _)| *v == state.pack_day_of_week)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in PACK_DAYS.iter() {
                        ui.selectable_value(&mut state.pack_day_of_week, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Date:");
            ui.add(egui::DragValue::new(&mut state.pack_date).range(0..=u8::MAX));
        });
        ui.horizontal(|ui| {
            ui.label("Time:");
            ui.add(egui::DragValue::new(&mut state.pack_time).range(-128..=127));
            ui.separator();
            ui.label("Duration (hours):");
            ui.add(egui::DragValue::new(&mut state.pack_duration).range(0..=i32::MAX));
        });
    });

    ui.add_space(6.0);
    draw_package_location(ui, "Location 1 (PLDT)", &mut state.pack_location);
    draw_package_location(ui, "Location 2 (PLD2)", &mut state.pack_location2);
    draw_package_target(ui, "Target 1 (PTDT)", &mut state.pack_target);
    draw_package_target(ui, "Target 2 (PTD2)", &mut state.pack_target2);

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Idle Animation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.pack_idle_flags).range(0..=u8::MAX));
            ui.separator();
            ui.label("Count:");
            ui.add(egui::DragValue::new(&mut state.pack_idle_count).range(0..=u8::MAX));
            ui.separator();
            ui.label("Timer:");
            ui.add(egui::DragValue::new(&mut state.pack_idle_timer).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Animations:");
            let mut remove = None;
            for i in 0..state.pack_idle_animations.len() {
                let id = state.pack_idle_animations[i];
                ui.horizontal(|ui| {
                    let mut fid = Some(id);
                    formid_field(ui, &format!("pack_idle_{i}"), &mut fid);
                    state.pack_idle_animations[i] = fid.unwrap_or(0);
                    if ui.button("x").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                state.pack_idle_animations.remove(i);
            }
            if ui.button("Add Animation").clicked() {
                state.pack_idle_animations.push(0);
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Options");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Combat Style:");
            let styles = session.records_by_sig(*b"CSTY");
            let mut fid = state.pack_combat_style;
            record_dropdown(ui, "pack_combat_style", &styles, &mut fid);
            state.pack_combat_style = fid;
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.pack_eat_marker, "Eat Marker (PKED)");
            ui.separator();
            ui.checkbox(&mut state.pack_use_item_marker, "Use Item Marker (PUID)");
            ui.separator();
            ui.checkbox(&mut state.pack_ambush_marker, "Ambush Marker (PKAM)");
        });
        ui.horizontal(|ui| {
            ui.label("Escort Distance:");
            ui.add(
                egui::DragValue::new(&mut state.pack_escort_distance.unwrap_or(0))
                    .range(0..=u32::MAX),
            );
            ui.separator();
            ui.label("Follow Trigger Radius:");
            ui.add(
                egui::DragValue::new(&mut state.pack_follow_radius.unwrap_or(0.0))
                    .speed(0.01)
                    .range(0.0..=10000.0),
            );
            ui.separator();
            ui.label("Patrol Flags:");
            ui.add(
                egui::DragValue::new(&mut state.pack_patrol_flags.unwrap_or(0)).range(0..=u16::MAX),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Use Weapon (PKW3)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.pack_weapon.flags).range(0..=u32::MAX));
            ui.label(format!("0x{:08X}", state.pack_weapon.flags));
            ui.separator();
            ui.label("Fire Rate:");
            egui::ComboBox::from_id_salt("pack_fire_rate")
                .selected_text(if state.pack_weapon.fire_rate == 0 {
                    "Auto Fire"
                } else {
                    "Volley Fire"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.pack_weapon.fire_rate, 0, "Auto Fire");
                    ui.selectable_value(&mut state.pack_weapon.fire_rate, 1, "Volley Fire");
                });
            ui.separator();
            ui.label("Fire Count:");
            egui::ComboBox::from_id_salt("pack_fire_count")
                .selected_text(if state.pack_weapon.fire_count == 0 {
                    "Number of Bursts"
                } else {
                    "Repeat Fire"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.pack_weapon.fire_count, 0, "Number of Bursts");
                    ui.selectable_value(&mut state.pack_weapon.fire_count, 1, "Repeat Fire");
                });
        });
        ui.horizontal(|ui| {
            ui.label("Bursts:");
            ui.add(egui::DragValue::new(&mut state.pack_weapon.num_bursts).range(0..=u16::MAX));
            ui.separator();
            ui.label("Shots:");
            ui.add(egui::DragValue::new(&mut state.pack_weapon.shots_min).range(0..=u16::MAX));
            ui.add(egui::DragValue::new(&mut state.pack_weapon.shots_max).range(0..=u16::MAX));
        });
        ui.horizontal(|ui| {
            ui.label("Pause:");
            ui.add(
                egui::DragValue::new(&mut state.pack_weapon.pause_min)
                    .speed(0.01)
                    .range(0.0..=1000.0),
            );
            ui.add(
                egui::DragValue::new(&mut state.pack_weapon.pause_max)
                    .speed(0.01)
                    .range(0.0..=1000.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Dialog (PKDD)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("FOV:");
            ui.add(egui::DragValue::new(&mut state.pack_dialog.fov).speed(0.5));
            ui.separator();
            ui.label("Flags:");
            ui.add(egui::DragValue::new(&mut state.pack_dialog.flags).range(0..=u32::MAX));
            ui.label(format!("0x{:08X}", state.pack_dialog.flags));
            ui.separator();
            ui.label("Type:");
            egui::ComboBox::from_id_salt("pack_dialog_type")
                .selected_text(
                    PACK_DIALOG_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.pack_dialog.dialog_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in PACK_DIALOG_TYPES.iter() {
                        ui.selectable_value(&mut state.pack_dialog.dialog_type, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Topic:");
            let topics = session.records_by_sig(*b"DIAL");
            let mut fid = state.pack_dialog.topic;
            record_dropdown(ui, "pack_dialog_topic", &topics, &mut fid);
            state.pack_dialog.topic = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Embedded Scripts — {}", state.pack_scripts.len()));
        ui.add_space(4.0);
        for (i, script) in state.pack_scripts.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let name = match script.marker {
                    x if x == *b"POBA" => "On Begin",
                    x if x == *b"POEA" => "On End",
                    _ => "On Change",
                };
                ui.horizontal(|ui| {
                    ui.strong(name);
                    ui.separator();
                    ui.label("Idle:");
                    let idles = session.records_by_sig(*b"IDLE");
                    let mut fid = script.idle;
                    record_dropdown(ui, "pack_script_idle", &idles, &mut fid);
                    script.idle = fid;
                });
                ui.horizontal(|ui| {
                    ui.label("Topic:");
                    let topics = session.records_by_sig(*b"DIAL");
                    let mut fid = script.topic;
                    record_dropdown(ui, "pack_script_topic", &topics, &mut fid);
                    script.topic = fid;
                });
                ui.horizontal(|ui| {
                    ui.label("Script Type:");
                    ui.add(
                        egui::DragValue::new(&mut script.header.script_type).range(0..=u32::MAX),
                    );
                    ui.separator();
                    ui.label("Version:");
                    ui.add(egui::DragValue::new(&mut script.header.version).range(0..=u32::MAX));
                });
                ui.horizontal(|ui| {
                    ui.label("Source:");
                    ui.add(
                        egui::TextEdit::singleline(&mut script.source)
                            .desired_width(ui.available_width() - 60.0),
                    );
                });
                ui.label(format!(
                    "Compiled bytes: {}   Local variables: {}   References: {}",
                    script.compiled.len(),
                    script.variables.len(),
                    script.references.len()
                ));
            });
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Conditions (CTDA) — {}",
            state.pack_conditions.len()
        ));
        ui.add_space(4.0);
        for (idx, cond) in state.pack_conditions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Condition {}", idx + 1));
                ui.label("Function:");
                ui.add(egui::DragValue::new(&mut cond.function).range(0..=999));
                ui.label("Comparison Value:");
                ui.add(egui::DragValue::new(&mut cond.comparison_value).range(0..=u32::MAX));
            });
        }
    });
}

/// Draw one of a package record's two locations.
fn draw_package_location(ui: &mut egui::Ui, label: &str, loc: &mut PackageLocation) {
    ui.group(|ui| {
        ui.strong(label);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("pack_loc_{label}"))
                .selected_text(
                    PACK_LOCATION_TYPES
                        .iter()
                        .find(|(v, _)| *v == loc.kind)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in PACK_LOCATION_TYPES.iter() {
                        ui.selectable_value(&mut loc.kind, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Radius:");
            ui.add(egui::DragValue::new(&mut loc.radius));
        });
        ui.horizontal(|ui| {
            ui.label("Location:");
            let mut hex = loc
                .target
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            if ui.text_edit_singleline(&mut hex).changed() {
                let mut bytes = Vec::with_capacity(4);
                let mut chars = hex.chars().filter(|c| !c.is_whitespace());
                while let Some(pair) = chars.next() {
                    let high = pair.to_digit(16);
                    let low = chars.next().and_then(|c| c.to_digit(16));
                    if let (Some(h), Some(l)) = (high, low) {
                        bytes.push((h * 16 + l) as u8);
                    }
                }
                loc.target = bytes;
            }
        });
    });
}

/// Draw one of a package record's two targets.
fn draw_package_target(ui: &mut egui::Ui, label: &str, target: &mut PackageTarget) {
    ui.group(|ui| {
        ui.strong(label);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("pack_tgt_{label}"))
                .selected_text(
                    PACK_TARGET_TYPES
                        .iter()
                        .find(|(v, _)| *v == target.kind)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in PACK_TARGET_TYPES.iter() {
                        ui.selectable_value(&mut target.kind, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Count / Distance:");
            ui.add(egui::DragValue::new(&mut target.count_distance));
            ui.separator();
            ui.label("Unknown:");
            ui.add(egui::DragValue::new(&mut target.unknown).speed(0.01));
        });
        ui.horizontal(|ui| {
            ui.label("Target:");
            let mut hex = target
                .target
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            if ui.text_edit_singleline(&mut hex).changed() {
                let mut bytes = Vec::with_capacity(4);
                let mut chars = hex.chars().filter(|c| !c.is_whitespace());
                while let Some(pair) = chars.next() {
                    let high = pair.to_digit(16);
                    let low = chars.next().and_then(|c| c.to_digit(16));
                    if let (Some(h), Some(l)) = (high, low) {
                        bytes.push((h * 16 + l) as u8);
                    }
                }
                target.target = bytes;
            }
        });
    });
}

/// Draw the Script (SCPT) editor body.
fn draw_scpt_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Script");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("scpt_type")
                .selected_text(
                    SCPT_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.scpt_header.script_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in SCPT_TYPES.iter() {
                        ui.selectable_value(&mut state.scpt_header.script_type, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Version:");
            ui.add(egui::DragValue::new(&mut state.scpt_header.version).range(0..=1000));
        });
        ui.label(format!(
            "Compiled script: {} byte(s)",
            state.scpt_compiled.len()
        ));
        ui.label("Script Source (SCTX):");
        ui.add(
            egui::TextEdit::multiline(&mut state.scpt_source)
                .desired_rows(10)
                .desired_width(ui.available_width() - 20.0),
        );
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Local Variables — {}", state.scpt_variables.len()));
        ui.add_space(4.0);
        for (idx, var) in state.scpt_variables.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{}", idx + 1));
                ui.label("Index:");
                ui.add(egui::DragValue::new(&mut var.index).range(0..=u32::MAX));
                ui.label("Name:");
                ui.add(
                    egui::TextEdit::singleline(&mut var.name)
                        .id_salt(format!("scpt_var_{idx}"))
                        .desired_width(200.0),
                );
                ui.label("IsLongOrShort:");
                let mut long = (var.flags & 0x01) != 0;
                if ui.checkbox(&mut long, "").changed() {
                    if long {
                        var.flags |= 0x01;
                    } else {
                        var.flags &= !0x01;
                    }
                }
            });
        }
    });
}

/// Draw the Plugin Header (TES4) editor body.
fn draw_tes4_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Plugin Header");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Version (HEDR):");
            ui.add(
                egui::DragValue::new(&mut state.tes4_version)
                    .speed(0.01)
                    .range(0.0..=10.0),
            );
            ui.separator();
            ui.label("Number of Records:");
            ui.add(egui::DragValue::new(&mut state.tes4_num_records).range(0..=10000000));
            ui.separator();
            ui.label("Next Object Id:");
            ui.add(egui::DragValue::new(&mut state.tes4_next_object_id).range(0..=0x00FF_FFFF));
        });
        ui.label("Author (CNAM):");
        ui.add(
            egui::TextEdit::singleline(&mut state.tes4_author)
                .desired_width(ui.available_width() - 20.0),
        );
        ui.label("Description (SNAM):");
        ui.add(
            egui::TextEdit::multiline(&mut state.tes4_description)
                .desired_rows(4)
                .desired_width(ui.available_width() - 20.0),
        );
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Master Files — {}", state.tes4_masters.len()));
        ui.add_space(4.0);
        for (idx, master) in state.tes4_masters.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{}", idx + 1));
                ui.add(
                    egui::TextEdit::singleline(&mut master.filename)
                        .id_salt(format!("tes4_master_{idx}"))
                        .desired_width(ui.available_width() - 200.0),
                );
                ui.label("FormID:");
                ui.add(egui::DragValue::new(&mut master.form_id).range(0..=u32::MAX));
            });
        }
    });
}

/// Draw the Challenge (CHAL) editor body.
fn draw_chal_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Challenge");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("chal_type")
                .selected_text(
                    CHAL_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.chal_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CHAL_TYPES.iter() {
                        ui.selectable_value(&mut state.chal_type, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Threshold:");
            ui.add(egui::DragValue::new(&mut state.chal_threshold).range(0..=1000000));
            ui.separator();
            ui.label("Interval:");
            ui.add(egui::DragValue::new(&mut state.chal_interval).range(0..=1000000));
        });
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in CHAL_FLAGS {
                let mut set = (state.chal_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.chal_flags |= bit;
                    } else {
                        state.chal_flags &= !bit;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Value1:");
            ui.label(format!("{:02X?}", state.chal_value1));
            ui.separator();
            ui.label("Value2:");
            ui.label(format!("{:02X?}", state.chal_value2));
            ui.separator();
            ui.label("Value3:");
            ui.label(format!("{:02X?}", state.chal_value3));
        });
        ui.label("The three value fields' meaning depends on the type.");
        ui.horizontal(|ui| {
            ui.label("Value3 reference (SNAM):");
            ui.add(
                egui::TextEdit::singleline(
                    &mut state
                        .chal_snam
                        .map(|v| format!("{v:08X}"))
                        .unwrap_or_default(),
                )
                .id_salt("chal_snam")
                .hint_text("00000000")
                .desired_width(100.0),
            );
            ui.separator();
            ui.label("Value4 reference (XNAM):");
            ui.add(
                egui::TextEdit::singleline(
                    &mut state
                        .chal_xnam
                        .map(|v| format!("{v:08X}"))
                        .unwrap_or_default(),
                )
                .id_salt("chal_xnam")
                .hint_text("00000000")
                .desired_width(100.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Presentation");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Pip-Boy Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.chal_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Message Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.chal_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.label("Description (DESC):");
        ui.add(
            egui::TextEdit::multiline(&mut state.chal_desc)
                .desired_rows(3)
                .desired_width(ui.available_width() - 20.0),
        );
    });
}

/// Draw the Camera Path (CPTH) editor body.
fn draw_cpth_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Camera Path");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Camera Zoom (DATA):");
            egui::ComboBox::from_id_salt("cpth_zoom")
                .selected_text(
                    CPTH_ZOOMS
                        .iter()
                        .find(|(v, _)| *v == state.cpth_camera_zoom)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CPTH_ZOOMS.iter() {
                        ui.selectable_value(&mut state.cpth_camera_zoom, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Parent Camera Path (ANAM):");
            let paths = session.records_by_sig(*b"CPTH");
            let mut parent = state.cpth_parent;
            record_dropdown(ui, "cpth_parent", &paths, &mut parent);
            state.cpth_parent = parent;
        });
        ui.horizontal(|ui| {
            ui.label("Previous Sibling (ANAM):");
            let paths = session.records_by_sig(*b"CPTH");
            let mut previous = state.cpth_previous;
            record_dropdown(ui, "cpth_previous", &paths, &mut previous);
            state.cpth_previous = previous;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Camera Shots (SNAM) — {}", state.cpth_shots.len()));
        ui.add_space(4.0);
        let shots = session.records_by_sig(*b"CAMS");
        let mut remove_idx = None;
        for (idx, shot) in state.cpth_shots.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button("✖")
                    .on_hover_text("Remove this camera shot")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
                let mut fid = Some(*shot);
                record_dropdown(ui, &format!("cpth_shot_{idx}"), &shots, &mut fid);
                *shot = fid.unwrap_or(0);
            });
        }
        if let Some(idx) = remove_idx {
            state.cpth_shots.remove(idx);
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Conditions (CTDA) — {}",
            state.cpth_conditions.len()
        ));
        ui.add_space(4.0);
        for (idx, cond) in state.cpth_conditions.iter_mut().enumerate() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Condition {}", idx + 1));
                let mut combine = (cond.cond_type & 0x01) != 0;
                if ui.checkbox(&mut combine, "OR").changed() {
                    if combine {
                        cond.cond_type |= 0x01;
                    } else {
                        cond.cond_type &= !0x01;
                    }
                }
                let mut on_target = (cond.cond_type & 0x02) != 0;
                if ui.checkbox(&mut on_target, "Run On Target").changed() {
                    if on_target {
                        cond.cond_type |= 0x02;
                    } else {
                        cond.cond_type &= !0x02;
                    }
                }
                let mut use_global = (cond.cond_type & 0x04) != 0;
                if ui.checkbox(&mut use_global, "Use Global").changed() {
                    if use_global {
                        cond.cond_type |= 0x04;
                    } else {
                        cond.cond_type &= !0x04;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Comparison:");
                egui::ComboBox::from_id_salt(format!("ctda_cmp_{idx}"))
                    .selected_text(
                        CTDA_COMPARISONS
                            .iter()
                            .find(|(mask, _)| cond.cond_type & mask == *mask)
                            .map(|(_, n)| *n)
                            .unwrap_or("Custom"),
                    )
                    .show_ui(ui, |ui| {
                        let base = cond.cond_type & !0xE0;
                        for (mask, name) in CTDA_COMPARISONS.iter() {
                            ui.selectable_value(&mut cond.cond_type, base | mask, *name);
                        }
                    });
                ui.separator();
                ui.label("Function:");
                ui.add(egui::DragValue::new(&mut cond.function).range(0..=999));
            });
            ui.horizontal(|ui| {
                ui.label("Comparison Value:");
                ui.add(egui::DragValue::new(&mut cond.comparison_value).range(0..=u32::MAX));
                ui.separator();
                ui.label("Reference:");
                ui.add(egui::DragValue::new(&mut cond.reference.unwrap_or(0)).range(0..=u32::MAX));
                cond.reference = Some(cond.reference.unwrap_or(0));
            });
            ui.horizontal(|ui| {
                ui.label("Parameter 1:");
                ui.add(egui::DragValue::new(&mut cond.parameter1).range(0..=u32::MAX));
                ui.separator();
                ui.label("Parameter 2:");
                ui.add(egui::DragValue::new(&mut cond.parameter2).range(0..=u32::MAX));
                ui.separator();
                ui.label("Run On:");
                ui.add(egui::DragValue::new(&mut cond.run_on).range(0..=u32::MAX));
            });
        }
    });
}

/// Draw the Casino (CSNO) editor body.
fn draw_csno_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Casino");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Decks % Before Shuffle:");
            ui.add(
                egui::DragValue::new(&mut state.csno_decks_shuffle)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
            ui.separator();
            ui.label("BlackJack Payout Ratio:");
            ui.add(
                egui::DragValue::new(&mut state.csno_payout_ratio)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Number of Decks:");
            ui.add(egui::DragValue::new(&mut state.csno_number_of_decks).range(0..=1000));
            ui.separator();
            ui.label("Max Winnings:");
            ui.add(egui::DragValue::new(&mut state.csno_max_winnings).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Slot Reel Stops:");
            for (i, stop) in state.csno_reel_stops.iter_mut().enumerate() {
                ui.label(format!("#{i}"));
                ui.add(egui::DragValue::new(stop).range(0..=1000));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Currency (CHIP):");
            let chips = session.records_by_sig(*b"CHIP");
            let mut fid = state.csno_currency;
            record_dropdown(ui, "csno_currency", &chips, &mut fid);
            state.csno_currency = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Casino Winnings Quest (QUST):");
            let quests = session.records_by_sig(*b"QUST");
            let mut fid = state.csno_winnings_quest;
            record_dropdown(ui, "csno_winnings_quest", &quests, &mut fid);
            state.csno_winnings_quest = fid;
        });
        ui.horizontal_wrapped(|ui| {
            let mut set = (state.csno_flags & 0x0000_0001) != 0;
            if ui.checkbox(&mut set, "Dealer Stay on Soft 17").changed() {
                if set {
                    state.csno_flags |= 0x0000_0001;
                } else {
                    state.csno_flags &= !0x0000_0001;
                }
            }
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Models — {}", state.csno_models.len()));
        ui.add_space(4.0);
        let labels = [
            "Casino $1 Chip Model",
            "Casino $5 Chip Model",
            "Casino $10 Chip Model",
            "Casino $25 Chip Model",
            "Casino $100 Chip Model",
            "Casino $500 Chip Model",
            "Casino Roulette Chip Model",
            "Slot Machine Model",
            "Slot Machine Model (MOD2)",
            "BlackJack Table Model",
            "Roulette Table Model",
        ];
        for (i, model) in state.csno_models.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(labels.get(i).copied().unwrap_or("Extra model"));
                ui.add(
                    egui::TextEdit::singleline(model)
                        .id_salt(format!("csno_model_{i}"))
                        .desired_width(ui.available_width() - 220.0),
                );
            });
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Slot Reel Textures — {}", state.csno_icons.len()));
        ui.add_space(4.0);
        for (i, icon) in state.csno_icons.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Symbol {}", i + 1));
                ui.add(
                    egui::TextEdit::singleline(icon)
                        .id_salt(format!("csno_icon_{i}"))
                        .desired_width(ui.available_width() - 160.0),
                );
            });
        }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "BlackJack Deck Textures — {}",
            state.csno_deck_icons.len()
        ));
        ui.add_space(4.0);
        for (i, icon) in state.csno_deck_icons.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Deck {}", i + 1));
                ui.add(
                    egui::TextEdit::singleline(icon)
                        .id_salt(format!("csno_deck_{i}"))
                        .desired_width(ui.available_width() - 160.0),
                );
            });
        }
    });
}

/// Draw the Debris (DEBR) editor body.
fn draw_debr_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong(format!("Debris Models — {}", state.debr_models.len()));
        ui.add_space(4.0);
        for (idx, model) in state.debr_models.iter_mut().enumerate() {
            ui.separator();
            ui.strong(format!("#{idx}"));
            ui.horizontal(|ui| {
                ui.label("Percentage:");
                ui.add(egui::DragValue::new(&mut model.percentage).range(0..=100));
                ui.separator();
                let mut collision = (model.flags & 0x01) != 0;
                if ui.checkbox(&mut collision, "Has Collision Data").changed() {
                    if collision {
                        model.flags |= 0x01;
                    } else {
                        model.flags &= !0x01;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Model Filename:");
                ui.add(
                    egui::TextEdit::singleline(&mut model.model)
                        .id_salt(format!("debr_model_{idx}"))
                        .desired_width(ui.available_width() - 180.0),
                );
            });
            let hashes = model.texture_hashes.len() / 24;
            ui.label(format!(
                "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
                model.texture_hashes.len()
            ));
        }
    });
}

/// Draw the Camera Shot (CAMS) editor body.
fn draw_cams_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Camera Shot");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Action:");
            egui::ComboBox::from_id_salt("cams_action")
                .selected_text(
                    CAMS_ACTIONS
                        .iter()
                        .find(|(v, _)| *v == state.cams_action)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CAMS_ACTIONS.iter() {
                        ui.selectable_value(&mut state.cams_action, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Location:");
            egui::ComboBox::from_id_salt("cams_location")
                .selected_text(
                    CAMS_ROLES
                        .iter()
                        .find(|(v, _)| *v == state.cams_location)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CAMS_ROLES.iter() {
                        ui.selectable_value(&mut state.cams_location, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Target:");
            egui::ComboBox::from_id_salt("cams_target")
                .selected_text(
                    CAMS_ROLES
                        .iter()
                        .find(|(v, _)| *v == state.cams_target)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CAMS_ROLES.iter() {
                        ui.selectable_value(&mut state.cams_target, *v, *name);
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in CAMS_FLAGS {
                let mut set = (state.cams_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.cams_flags |= bit;
                    } else {
                        state.cams_flags &= !bit;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Player Time Mult:");
            ui.add(
                egui::DragValue::new(&mut state.cams_player_time_mult)
                    .speed(0.01)
                    .range(-100.0..=100.0),
            );
            ui.separator();
            ui.label("Target Time Mult:");
            ui.add(
                egui::DragValue::new(&mut state.cams_target_time_mult)
                    .speed(0.01)
                    .range(-100.0..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Global Time Mult:");
            ui.add(
                egui::DragValue::new(&mut state.cams_global_time_mult)
                    .speed(0.01)
                    .range(-100.0..=100.0),
            );
            ui.separator();
            ui.label("Max Time:");
            ui.add(
                egui::DragValue::new(&mut state.cams_max_time)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Min Time:");
            ui.add(
                egui::DragValue::new(&mut state.cams_min_time)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
            ui.separator();
            ui.label("Target % Between Actors:");
            ui.add(
                egui::DragValue::new(&mut state.cams_target_percent)
                    .speed(0.01)
                    .range(0.0..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Image Space Modifier (MNAM):");
            let imads = session.records_by_sig(*b"IMAD");
            let mut fid = state.cams_image_space_modifier;
            record_dropdown(ui, "cams_imad", &imads, &mut fid);
            state.cams_image_space_modifier = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.cams_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });
}

/// Draw the Card (CCRD) editor body.
///
/// The suit and the value are two `INTV` subrecords, so the page edits them
/// as a pair.
fn draw_ccrd_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Card");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Suit (INTV):");
            let suit = state.ccrd_intv.first().copied().unwrap_or(0);
            egui::ComboBox::from_id_salt("ccrd_suit")
                .selected_text(
                    CCRD_SUITS
                        .iter()
                        .find(|(v, _)| *v == suit)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CCRD_SUITS.iter() {
                        if ui
                            .selectable_value(&mut state.ccrd_intv[0], *v, *name)
                            .clicked()
                        {
                            state.ccrd_intv[0] = *v;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Value (INTV):");
            let val = state.ccrd_intv.get(1).copied().unwrap_or(0);
            egui::ComboBox::from_id_salt("ccrd_value")
                .selected_text(
                    CCRD_VALUES
                        .iter()
                        .find(|(v, _)| *v == val)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in CCRD_VALUES.iter() {
                        if ui
                            .selectable_value(&mut state.ccrd_intv[1], *v, *name)
                            .clicked()
                        {
                            state.ccrd_intv[1] = *v;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Value (DATA):");
            ui.add(egui::DragValue::new(&mut state.ccrd_value).range(0..=100000));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Images");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ccrd_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ccrd_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("High Res Face (TX00):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ccrd_face)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("High Res Back (TX01):");
            ui.add(
                egui::TextEdit::singleline(&mut state.ccrd_back)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sounds");
        ui.add_space(4.0);
        let sounds = session.records_by_sig(*b"SOUN");
        ui.horizontal(|ui| {
            ui.label("Pick Up (YNAM):");
            let mut fid = state.ccrd_pickup_sound;
            record_dropdown(ui, "ccrd_pickup", &sounds, &mut fid);
            state.ccrd_pickup_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Drop (ZNAM):");
            let mut fid = state.ccrd_drop_sound;
            record_dropdown(ui, "ccrd_drop", &sounds, &mut fid);
            state.ccrd_drop_sound = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.ccrd_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });
}

/// Draw the Caravan Deck (CDCK) editor body.
///
/// The deck's cards are a *repeated* subrecord, so the page edits them as a
/// list.
fn draw_cdck_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong(format!("Caravan Deck — {} cards", state.cdck_cards.len()));
        ui.add_space(4.0);
        if state.cdck_cards.is_empty() {
            ui.label(egui::RichText::new("This deck has no cards.").italics());
        } else {
            let cards = session.records_by_sig(*b"CCRD");
            let mut remove_idx = None;
            for (idx, card) in state.cdck_cards.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button("✖").on_hover_text("Remove this card").clicked() {
                        remove_idx = Some(idx);
                    }
                    let mut fid = Some(*card);
                    record_dropdown(ui, &format!("cdck_card_{idx}"), &cards, &mut fid);
                    *card = fid.unwrap_or(0);
                });
            }
            if let Some(idx) = remove_idx {
                state.cdck_cards.remove(idx);
            }
        }
        ui.horizontal(|ui| {
            ui.label("DATA (broken in the spec):");
            ui.add(egui::DragValue::new(&mut state.cdck_data).range(0..=100000));
        });
    });
}

/// Draw the Casino Chip (CHIP) editor body.
fn draw_chip_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Casino Chip");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.chip_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.chip_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sounds");
        ui.add_space(4.0);
        let sounds = session.records_by_sig(*b"SOUN");
        ui.horizontal(|ui| {
            ui.label("Pick Up (YNAM):");
            let mut fid = state.chip_pickup_sound;
            record_dropdown(ui, "chip_pickup", &sounds, &mut fid);
            state.chip_pickup_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Drop (ZNAM):");
            let mut fid = state.chip_drop_sound;
            record_dropdown(ui, "chip_drop", &sounds, &mut fid);
            state.chip_drop_sound = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.chip_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });

    draw_destruction_ui(
        ui,
        &mut state.chip_destruction,
        &mut state.chip_dest_health,
        &mut state.chip_dest_flags,
        "chip",
    );
}

/// Draw the shared Destruction Data collection.
fn draw_destruction_ui(
    ui: &mut egui::Ui,
    stages: &mut [DestructionStage],
    health: &mut i32,
    flags: &mut u8,
    prefix: &str,
) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Destruction Data — {} stage(s)", stages.len()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Health (DEST):");
            ui.add(egui::DragValue::new(health).range(0..=50000));
            ui.separator();
            let mut vats = (*flags & 0x01) != 0;
            if ui.checkbox(&mut vats, "VATS Targetable").changed() {
                if vats {
                    *flags |= 0x01;
                } else {
                    *flags &= !0x01;
                }
            }
        });
        for (idx, stage) in stages.iter_mut().enumerate() {
            ui.separator();
            ui.strong(format!("Stage {idx}"));
            ui.horizontal(|ui| {
                ui.label("Health %:");
                ui.add(egui::DragValue::new(&mut stage.health_percent).range(0..=100));
                ui.separator();
                ui.label("Index:");
                ui.add(egui::DragValue::new(&mut stage.index).range(0..=255));
                ui.separator();
                ui.label("Damage Stage:");
                ui.add(egui::DragValue::new(&mut stage.damage_stage).range(0..=255));
            });
            ui.horizontal(|ui| {
                ui.label("Self Damage/s:");
                ui.add(egui::DragValue::new(&mut stage.self_damage_per_second).range(-1000..=1000));
                ui.separator();
                ui.label("Debris Count:");
                ui.add(egui::DragValue::new(&mut stage.debris_count).range(0..=1000));
            });
            ui.horizontal(|ui| {
                ui.label("Model (DMDL):");
                ui.add(
                    egui::TextEdit::singleline(&mut stage.model)
                        .id_salt(format!("dest_model_{prefix}_{idx}"))
                        .desired_width(ui.available_width() - 200.0),
                );
            });
            let hashes = stage.texture_hashes.len() / 24;
            ui.label(format!(
                "Texture File Hashes (DMDT): {hashes} hash(es), {} bytes — preserved as-is",
                stage.texture_hashes.len()
            ));
        }
    });
}

/// Draw the Money (CMNY) editor body.
fn draw_cmny_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Money");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Absolute Value (DATA):");
            ui.add(egui::DragValue::new(&mut state.cmny_value).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Large Icon (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.cmny_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Small Icon (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.cmny_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sounds");
        ui.add_space(4.0);
        let sounds = session.records_by_sig(*b"SOUN");
        ui.horizontal(|ui| {
            ui.label("Pick Up (YNAM):");
            let mut fid = state.cmny_pickup_sound;
            record_dropdown(ui, "cmny_pickup", &sounds, &mut fid);
            state.cmny_pickup_sound = fid;
        });
        ui.horizontal(|ui| {
            ui.label("Drop (ZNAM):");
            let mut fid = state.cmny_drop_sound;
            record_dropdown(ui, "cmny_drop", &sounds, &mut fid);
            state.cmny_drop_sound = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);
        let model = &mut state.cmny_model;
        ui.horizontal(|ui| {
            ui.label("Model (MODL):");
            if model.instance == 0 {
                ui.label(egui::RichText::new("edit the Model Path in the header above").weak());
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            }
        });
        let hashes = model.modt.len() / 24;
        ui.label(format!(
            "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
            model.modt.len()
        ));
    });
}

/// Draw the Armor Addon (ARMA) editor body.
///
/// The record carries four model collections, so the page has a section for
/// each. `DNAM`'s length is shown, because it is four bytes on 115 of the
/// game's 131 records and twelve on the rest, and the editor writes back the
/// length the record had.
fn draw_arma_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Armor Addon");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Value:");
            ui.add(egui::DragValue::new(&mut state.arma_val).range(-100000..=100000));
            ui.separator();
            ui.label("Max Condition:");
            ui.add(egui::DragValue::new(&mut state.arma_hp).range(0..=100000));
            ui.separator();
            ui.label("Weight:");
            ui.add(
                egui::DragValue::new(&mut state.arma_weight)
                    .speed(0.1)
                    .range(0.0..=1000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Equipment Type (ETYP):");
            ui.add(egui::DragValue::new(&mut state.arma_equip_type).range(-1..=13));
        });
    });

    // --- DNAM ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!(
            "Damage Resistance (DNAM) — {} bytes",
            state.arma_dnam_len
        ));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("AR:");
            ui.add(egui::DragValue::new(&mut state.arma_dr).range(-1000..=1000));
            ui.separator();
            ui.label("DT:");
            ui.add(
                egui::DragValue::new(&mut state.arma_dt)
                    .speed(0.1)
                    .range(-1000.0..=1000.0),
            );
        });
        // The flag word is kept whole and the named bit is edited inside it.
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in ARMA_DNAM_FLAGS {
                let mut set = (state.arma_dnam_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.arma_dnam_flags |= bit;
                    } else {
                        state.arma_dnam_flags &= !bit;
                    }
                }
            }
        });
        if state.arma_dnam_len < 8 {
            ui.label(
                egui::RichText::new(
                    "This record's DNAM is four bytes: it has no DT field, and the editor keeps it that way.",
                )
                .italics(),
            );
        }
    });

    // --- BMDT ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Biped Data (BMDT)");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in BMDT_BIPED_FLAGS {
                let mut set = (state.arma_biped_flags & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.arma_biped_flags |= bit;
                    } else {
                        state.arma_biped_flags &= !bit;
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (bit, name) in BMDT_GENERAL_FLAGS {
                let mut set = (state.arma_bmdt_general & bit) != 0;
                if ui.checkbox(&mut set, name).changed() {
                    if set {
                        state.arma_bmdt_general |= bit;
                    } else {
                        state.arma_bmdt_general &= !bit;
                    }
                }
            }
        });
    });

    // --- The four icons ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Icons");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Male inventory (ICON):");
            ui.add(
                egui::TextEdit::singleline(&mut state.arma_icon)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Male message (MICO):");
            ui.add(
                egui::TextEdit::singleline(&mut state.arma_mico)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Female inventory (ICO2):");
            ui.add(
                egui::TextEdit::singleline(&mut state.arma_ico2)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Female message (MIC2):");
            ui.add(
                egui::TextEdit::singleline(&mut state.arma_mic2)
                    .desired_width(ui.available_width() - 180.0),
            );
        });
    });

    // --- The four model collections ---
    for model in state.arma_models.iter_mut() {
        let title = match model.instance {
            0 => "Male Biped Model Data",
            1 => "Male World Model Data",
            2 => "Female Biped Model Data",
            _ => "Female World Model Data",
        };
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong(title);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Model:");
                ui.add(
                    egui::TextEdit::singleline(&mut model.modl)
                        .desired_width(ui.available_width() - 200.0),
                );
            });
            // MODT's texture hashes are file hashes the editor cannot compute
            // or author, so it reports what is there rather than offering to
            // change it: rewriting one would corrupt the mod.
            let hashes = model.modt.len() / 24;
            ui.label(format!(
                "Texture File Hashes (MODT): {hashes} hash(es), {} bytes — preserved as-is",
                model.modt.len()
            ));
            ui.horizontal(|ui| {
                ui.label("MODB:");
                match &model.modb {
                    Some(b) => ui.label(format!("{b:?} — preserved as-is")),
                    None => ui.label(egui::RichText::new("absent").italics()),
                }
            });
        });
    }
}

/// Draw the Ambient Sound (ASPC) editor body.
///
/// The five `SNAM` sound loops are edited as a list, because they are
/// repeated subrecords.
fn draw_aspc_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Ambient Sound");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Walla Trigger Count (WNAM):");
            ui.add(egui::DragValue::new(&mut state.aspc_walla_trigger_count).range(0..=1000000));
        });
        ui.horizontal(|ui| {
            ui.label("Environment Type (ANAM):");
            egui::ComboBox::from_id_salt("aspc_environment")
                .selected_text(
                    ASPC_ENVIRONMENTS
                        .iter()
                        .find(|(v, _)| *v == state.aspc_environment)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in ASPC_ENVIRONMENTS.iter() {
                        ui.selectable_value(&mut state.aspc_environment, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Is Interior (INAM):");
            egui::ComboBox::from_id_salt("aspc_interior")
                .selected_text(
                    ASPC_INTERIOR
                        .iter()
                        .find(|(v, _)| *v == state.aspc_is_interior)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in ASPC_INTERIOR.iter() {
                        ui.selectable_value(&mut state.aspc_is_interior, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Use Sound from Region (RDAT):");
            let regions = session.records_by_sig(*b"REGN");
            let mut fid = state.aspc_region;
            record_dropdown(ui, "aspc_region", &regions, &mut fid);
            state.aspc_region = fid;
        });
    });

    // --- The five sound loops ---
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong(format!("Sound Loops (SNAM) — {}", state.aspc_sounds.len()));
        ui.add_space(4.0);
        let sounds = session.records_by_sig(*b"SOUN");
        let mut remove_idx = None;
        for (idx, sound) in state.aspc_sounds.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button("✖")
                    .on_hover_text("Remove this sound loop")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
                ui.label(ASPC_SOUND_SLOTS.get(idx).copied().unwrap_or("Extra"));
                let mut fid = Some(*sound);
                record_dropdown(ui, &format!("aspc_sound_{idx}"), &sounds, &mut fid);
                *sound = fid.unwrap_or(0);
            });
        }
        if let Some(idx) = remove_idx {
            state.aspc_sounds.remove(idx);
        }
    });
}

/// Draw the Ammo Effect (AMEF) editor body.
///
/// The type and the operation are enums, so they are dropdowns. The raw
/// number is kept underneath, so a value outside the table stays visible and
/// editable instead of being clamped to a known one.
fn draw_amef_ui(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    ui.group(|ui| {
        ui.strong("Ammo Effect");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("amef_type")
                .selected_text(
                    AMEF_TYPES
                        .iter()
                        .find(|(v, _)| *v == state.amef_type)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in AMEF_TYPES.iter() {
                        ui.selectable_value(&mut state.amef_type, *v, *name);
                    }
                });
            ui.separator();
            ui.label("Operation:");
            egui::ComboBox::from_id_salt("amef_operation")
                .selected_text(
                    AMEF_OPERATIONS
                        .iter()
                        .find(|(v, _)| *v == state.amef_operation)
                        .map(|(_, n)| *n)
                        .unwrap_or("Custom"),
                )
                .show_ui(ui, |ui| {
                    for (v, name) in AMEF_OPERATIONS.iter() {
                        ui.selectable_value(&mut state.amef_operation, *v, *name);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Value:");
            ui.add(
                egui::DragValue::new(&mut state.amef_value)
                    .speed(0.01)
                    .range(-1000.0..=1000.0),
            );
        });
    });
}

/// Draw the Animation Object (ANIO) editor body.
fn draw_anio_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Animation Object");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Animation (DATA):");
            let idles = session.records_by_sig(*b"IDLE");
            let mut fid = state.anio_animation;
            record_dropdown(ui, "anio_animation", &idles, &mut fid);
            state.anio_animation = fid;
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Model Data");
        ui.add_space(4.0);

        let model = &mut state.anio_model;

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
    });
}

/// Draw the Media Location Controller (ALOC) editor body.
///
/// The six media-set lists are edited as lists, because each is a repeated
/// subrecord. `NAM1`, `NAM2`, `NAM3` and `FNAM` are four bytes each that the
/// spec does not name; they are reported and carried through unchanged.
fn draw_aloc_ui(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    state: &mut ObjectEditorState,
) {
    ui.group(|ui| {
        ui.strong("Media Location Controller");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Location Delay (NAM4):");
            ui.add(
                egui::DragValue::new(&mut state.aloc_location_delay)
                    .speed(0.1)
                    .range(0.0..=86400.0),
            );
            ui.separator();
            ui.label("Retrigger Delay (NAM7):");
            ui.add(
                egui::DragValue::new(&mut state.aloc_retrigger_delay)
                    .speed(0.1)
                    .range(0.0..=86400.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Day Start (NAM5):");
            ui.add(egui::DragValue::new(&mut state.aloc_day_start).range(0..=100000));
            ui.separator();
            ui.label("Night Start (NAM6):");
            ui.add(egui::DragValue::new(&mut state.aloc_night_start).range(0..=100000));
        });
        ui.label("The day and night values are the hour, with the fraction of a day over 256.");
        ui.horizontal(|ui| {
            ui.label("NAM1 (unnamed):");
            ui.label(format!("{:02X?}", state.aloc_nam1));
            ui.separator();
            ui.label("NAM2 (unnamed):");
            ui.label(format!("{:02X?}", state.aloc_nam2));
            ui.separator();
            ui.label("NAM3 (unnamed):");
            ui.label(format!("{:02X?}", state.aloc_nam3));
            ui.separator();
            ui.label("FNAM (unnamed):");
            ui.label(format!("{:02X?}", state.aloc_fnam));
        });
        ui.horizontal(|ui| {
            ui.label("Conditional Faction (RNAM):");
            let factions = session.records_by_sig(*b"FACT");
            let mut fid = state.aloc_conditional_faction;
            record_dropdown(ui, "aloc_conditional_faction", &factions, &mut fid);
            state.aloc_conditional_faction = fid;
        });
    });

    // --- The six media-set lists ---
    ui.add_space(6.0);
    let msets = session.records_by_sig(*b"MSET");
    for (list_idx, list) in state.aloc_sets.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.strong(format!(
                "{} Media Set ({}) — {}",
                list.label,
                fnv_util::tag_str(&list.sig),
                list.entries.len()
            ));
            ui.add_space(4.0);

            if list.entries.is_empty() {
                ui.label(egui::RichText::new("No media sets on this list.").italics());
            } else {
                let mut remove_idx = None;
                for (idx, entry) in list.entries.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .button("✖")
                            .on_hover_text("Remove this media set")
                            .clicked()
                        {
                            remove_idx = Some(idx);
                        }
                        let mut fid = Some(*entry);
                        record_dropdown(
                            ui,
                            &format!("aloc_set_{}_{}", list_idx, idx),
                            &msets,
                            &mut fid,
                        );
                        *entry = fid.unwrap_or(0);
                    });
                }
                if let Some(idx) = remove_idx {
                    list.entries.remove(idx);
                }
            }
        });
    }

    // --- Add a media set ---
    ui.add_space(4.0);
    ui.group(|ui| {
        ui.strong("Add Media Set");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("To list:");
            egui::ComboBox::from_id_salt("aloc_add_list")
                .selected_text(
                    state
                        .aloc_sets
                        .get(state.aloc_set_list_to_add)
                        .map(|l| l.label)
                        .unwrap_or("Neutral"),
                )
                .show_ui(ui, |ui| {
                    for (i, l) in state.aloc_sets.iter().enumerate() {
                        ui.selectable_value(&mut state.aloc_set_list_to_add, i, l.label);
                    }
                });
            ui.separator();
            let mut add = state.aloc_set_to_add;
            record_dropdown(ui, "aloc_add_set", &msets, &mut add);
            if ui.button("Add").clicked() {
                if let Some(fid) = add {
                    if let Some(list) = state.aloc_sets.get_mut(state.aloc_set_list_to_add) {
                        list.entries.push(fid);
                    }
                }
            }
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
