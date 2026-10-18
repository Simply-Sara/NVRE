//! The Creature Editor window: inspect and edit all creature stats, traits, AI, and models.

use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashMap;

/// Which tab the creature editor is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreatureTab {
    #[default]
    Stats,
    FlagsAndTraits,
    AiData,
    ModelAndAudio,
    Template,
}

/// A creature's complete decoded data.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatureData {
    pub form_id: u32,
    pub edid: String,
    pub full_name: String,
    pub model: String,
    pub script: Option<u32>,
    pub voice: Option<u32>,
    pub template: Option<u32>,
    pub combat_style: Option<u32>,
    pub body_part_data: Option<u32>,
    pub death_item: Option<u32>,
    pub inherits_sounds_from: Option<u32>,
    pub impact_dataset: Option<u32>,
    pub melee_weapon_list: Option<u32>,
    pub unarmed_attack_anim: u16,
    pub attack_reach: u8,
    pub turning_speed: f32,
    pub base_scale: f32,
    pub foot_weight: f32,
    pub impact_material: u32,
    pub sound_level: u32,

    // ACBS configuration:
    pub flags: u32,
    pub fatigue: u16,
    pub barter_gold: u16,
    pub level_or_mult: i16,
    pub calc_min: u16,
    pub calc_max: u16,
    pub speed_mult: u16,
    pub karma: f32,
    pub disposition_base: i16,
    pub template_flags: u16,

    // DATA stats & attributes:
    pub creature_type: u8,
    pub combat_skill: u8,
    pub magic_skill: u8,
    pub stealth_skill: u8,
    pub health: i16,
    pub damage: i16,
    pub strength: u8,
    pub perception: u8,
    pub endurance: u8,
    pub charisma: u8,
    pub intelligence: u8,
    pub agility: u8,
    pub luck: u8,

    // AIDT AI data:
    pub aggression: u8,
    pub confidence: u8,
    pub energy_level: u8,
    pub responsibility: u8,
    pub mood: u8,
    pub assistance: i8,
    pub aggro_radius_behavior: bool,
    pub aggro_radius: i32,
}

impl CreatureData {
    /// Load a creature from the session (active plugin or load order fallback).
    pub fn load(session: &crate::session::Session, form_id: u32) -> Option<CreatureData> {
        let (raw_subs, is_record_found) = if let Some(rec) = session.doc.record(form_id) {
            (rec.subrecords().ok()?, true)
        } else if let Ok(lo) = session.load_order() {
            if let Some((_, orig)) = fnv_world::fields::winner(lo, form_id) {
                (orig.subrecords().ok()?, true)
            } else {
                (Vec::new(), false)
            }
        } else {
            (Vec::new(), false)
        };

        if !is_record_found {
            return None;
        }

        // Overlay pending uncommitted edits
        let mut subs: HashMap<[u8; 4], Vec<u8>> = HashMap::new();
        for s in raw_subs {
            subs.insert(s.sig, s.data.clone());
        }
        if let Some(edits) = session.doc.edits_for(form_id) {
            for e in edits {
                if let nvre_edit::Edit::SetSub { sig, data } = e {
                    subs.insert(*sig, data.clone());
                }
            }
        }

        let edid = subs
            .get(b"EDID")
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();

        let full_name = subs
            .get(b"FULL")
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();

        let model = subs
            .get(b"MODL")
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();

        let read_fid = |sig: &[u8; 4]| -> Option<u32> {
            subs.get(sig)
                .filter(|b| b.len() >= 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };

        let script = read_fid(b"SCRI");
        let voice = read_fid(b"VTCK");
        let template = read_fid(b"TPLT");
        let combat_style = read_fid(b"ZNAM");
        let body_part_data = read_fid(b"PNAM");
        let death_item = read_fid(b"INAM");
        let inherits_sounds_from = read_fid(b"CSCR");
        let impact_dataset = read_fid(b"CNAM");
        let melee_weapon_list = read_fid(b"LNAM");

        let unarmed_attack_anim = subs
            .get(b"EAMT")
            .filter(|b| b.len() >= 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .unwrap_or(0);

        let attack_reach = subs
            .get(b"RNAM")
            .and_then(|b| b.first().copied())
            .unwrap_or(32);

        let turning_speed = subs
            .get(b"TNAM")
            .filter(|b| b.len() >= 4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(30.0);

        let base_scale = subs
            .get(b"BNAM")
            .filter(|b| b.len() >= 4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(1.0);

        let foot_weight = subs
            .get(b"WNAM")
            .filter(|b| b.len() >= 4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(20.0);

        let impact_material = subs
            .get(b"NAM4")
            .filter(|b| b.len() >= 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0);

        let sound_level = subs
            .get(b"NAM5")
            .filter(|b| b.len() >= 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(1);

        // ACBS
        let mut flags = 0u32;
        let mut fatigue = 50u16;
        let mut barter_gold = 0u16;
        let mut level_or_mult = 1i16;
        let mut calc_min = 0u16;
        let mut calc_max = 0u16;
        let mut speed_mult = 100u16;
        let mut karma = 0.0f32;
        let mut disposition_base = 0i16;
        let mut template_flags = 0u16;

        if let Some(b) = subs.get(b"ACBS").filter(|b| b.len() >= 24) {
            flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            fatigue = u16::from_le_bytes([b[4], b[5]]);
            barter_gold = u16::from_le_bytes([b[6], b[7]]);
            level_or_mult = i16::from_le_bytes([b[8], b[9]]);
            calc_min = u16::from_le_bytes([b[10], b[11]]);
            calc_max = u16::from_le_bytes([b[12], b[13]]);
            speed_mult = u16::from_le_bytes([b[14], b[15]]);
            karma = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
            disposition_base = i16::from_le_bytes([b[20], b[21]]);
            template_flags = u16::from_le_bytes([b[22], b[23]]);
        }

        // DATA
        let mut creature_type = 0u8;
        let mut combat_skill = 25u8;
        let mut magic_skill = 50u8;
        let mut stealth_skill = 50u8;
        let mut health = 20i16;
        let mut damage = 5i16;
        let mut strength = 5u8;
        let mut perception = 5u8;
        let mut endurance = 5u8;
        let mut charisma = 5u8;
        let mut intelligence = 5u8;
        let mut agility = 5u8;
        let mut luck = 5u8;

        if let Some(b) = subs.get(b"DATA").filter(|b| b.len() >= 17) {
            creature_type = b[0];
            combat_skill = b[1];
            magic_skill = b[2];
            stealth_skill = b[3];
            health = i16::from_le_bytes([b[4], b[5]]);
            damage = i16::from_le_bytes([b[8], b[9]]);
            strength = b[10];
            perception = b[11];
            endurance = b[12];
            charisma = b[13];
            intelligence = b[14];
            agility = b[15];
            luck = b[16];
        }

        // AIDT
        let mut aggression = 0u8;
        let mut confidence = 2u8;
        let mut energy_level = 50u8;
        let mut responsibility = 50u8;
        let mut mood = 0u8;
        let mut assistance = 0i8;
        let mut aggro_radius_behavior = false;
        let mut aggro_radius = 0i32;

        if let Some(b) = subs.get(b"AIDT").filter(|b| b.len() >= 20) {
            aggression = b[0];
            confidence = b[1];
            energy_level = b[2];
            responsibility = b[3];
            mood = b[4];
            assistance = b[14] as i8;
            aggro_radius_behavior = (b[15] & 1) != 0;
            aggro_radius = i32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        }

        Some(CreatureData {
            form_id,
            edid,
            full_name,
            model,
            script,
            voice,
            template,
            combat_style,
            body_part_data,
            death_item,
            inherits_sounds_from,
            impact_dataset,
            melee_weapon_list,
            unarmed_attack_anim,
            attack_reach,
            turning_speed,
            base_scale,
            foot_weight,
            impact_material,
            sound_level,
            flags,
            fatigue,
            barter_gold,
            level_or_mult,
            calc_min,
            calc_max,
            speed_mult,
            karma,
            disposition_base,
            template_flags,
            creature_type,
            combat_skill,
            magic_skill,
            stealth_skill,
            health,
            damage,
            strength,
            perception,
            endurance,
            charisma,
            intelligence,
            agility,
            luck,
            aggression,
            confidence,
            energy_level,
            responsibility,
            mood,
            assistance,
            aggro_radius_behavior,
            aggro_radius,
        })
    }

    /// Save the edited values back to the document as pending subrecord edits.
    pub fn save(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        // Strings
        session
            .doc
            .set_sub(fid, b"EDID", encode_zstring(&self.edid));
        session
            .doc
            .set_sub(fid, b"FULL", encode_zstring(&self.full_name));
        if !self.model.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"MODL", encode_zstring(&self.model));
        }

        // Scalars & Floats
        session
            .doc
            .set_sub(fid, b"BNAM", self.base_scale.to_le_bytes().to_vec());
        session
            .doc
            .set_sub(fid, b"TNAM", self.turning_speed.to_le_bytes().to_vec());
        session
            .doc
            .set_sub(fid, b"WNAM", self.foot_weight.to_le_bytes().to_vec());
        session.doc.set_sub(fid, b"RNAM", vec![self.attack_reach]);
        session.doc.set_sub(
            fid,
            b"EAMT",
            self.unarmed_attack_anim.to_le_bytes().to_vec(),
        );
        session
            .doc
            .set_sub(fid, b"NAM4", self.impact_material.to_le_bytes().to_vec());
        session
            .doc
            .set_sub(fid, b"NAM5", self.sound_level.to_le_bytes().to_vec());

        // FormIDs
        let mut save_fid = |sig: &[u8; 4], opt: Option<u32>| {
            if let Some(f) = opt {
                session.doc.set_sub(fid, sig, f.to_le_bytes().to_vec());
            } else {
                session.doc.remove_sub(fid, sig);
            }
        };

        save_fid(b"SCRI", self.script);
        save_fid(b"VTCK", self.voice);
        save_fid(b"TPLT", self.template);
        save_fid(b"ZNAM", self.combat_style);
        save_fid(b"PNAM", self.body_part_data);
        save_fid(b"INAM", self.death_item);
        save_fid(b"CSCR", self.inherits_sounds_from);
        save_fid(b"CNAM", self.impact_dataset);
        save_fid(b"LNAM", self.melee_weapon_list);

        // DATA (17 bytes)
        let mut data_bytes = vec![0u8; 17];
        data_bytes[0] = self.creature_type;
        data_bytes[1] = self.combat_skill;
        data_bytes[2] = self.magic_skill;
        data_bytes[3] = self.stealth_skill;
        data_bytes[4..6].copy_from_slice(&self.health.to_le_bytes());
        data_bytes[8..10].copy_from_slice(&self.damage.to_le_bytes());
        data_bytes[10] = self.strength;
        data_bytes[11] = self.perception;
        data_bytes[12] = self.endurance;
        data_bytes[13] = self.charisma;
        data_bytes[14] = self.intelligence;
        data_bytes[15] = self.agility;
        data_bytes[16] = self.luck;
        session.doc.set_sub(fid, b"DATA", data_bytes);

        // ACBS (24 bytes)
        let mut acbs_bytes = vec![0u8; 24];
        acbs_bytes[0..4].copy_from_slice(&self.flags.to_le_bytes());
        acbs_bytes[4..6].copy_from_slice(&self.fatigue.to_le_bytes());
        acbs_bytes[6..8].copy_from_slice(&self.barter_gold.to_le_bytes());
        acbs_bytes[8..10].copy_from_slice(&self.level_or_mult.to_le_bytes());
        acbs_bytes[10..12].copy_from_slice(&self.calc_min.to_le_bytes());
        acbs_bytes[12..14].copy_from_slice(&self.calc_max.to_le_bytes());
        acbs_bytes[14..16].copy_from_slice(&self.speed_mult.to_le_bytes());
        acbs_bytes[16..20].copy_from_slice(&self.karma.to_le_bytes());
        acbs_bytes[20..22].copy_from_slice(&self.disposition_base.to_le_bytes());
        acbs_bytes[22..24].copy_from_slice(&self.template_flags.to_le_bytes());
        session.doc.set_sub(fid, b"ACBS", acbs_bytes);

        // AIDT (20 bytes)
        let mut aidt_bytes = vec![0u8; 20];
        aidt_bytes[0] = self.aggression;
        aidt_bytes[1] = self.confidence;
        aidt_bytes[2] = self.energy_level;
        aidt_bytes[3] = self.responsibility;
        aidt_bytes[4] = self.mood;
        aidt_bytes[14] = self.assistance as u8;
        aidt_bytes[15] = if self.aggro_radius_behavior { 1 } else { 0 };
        aidt_bytes[16..20].copy_from_slice(&self.aggro_radius.to_le_bytes());
        session.doc.set_sub(fid, b"AIDT", aidt_bytes);

        session.status = format!("queued creature edits on {:08X}", fid);
    }
}

fn encode_zstring(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if !v.ends_with(&[0]) {
        v.push(0);
    }
    v
}

/// Editor state holding the active creature being edited.
#[derive(Resource, Default)]
pub struct CreatureEditorState {
    pub loaded_fid: Option<u32>,
    pub data: Option<CreatureData>,
    pub tab: CreatureTab,
    pub status: String,
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

const AGGRESSION_LEVELS: [&str; 4] = ["Unaggressive", "Aggressive", "Very Aggressive", "Frenzied"];
const CONFIDENCE_LEVELS: [&str; 5] = ["Cowardly", "Cautious", "Average", "Brave", "Foolhardy"];
const ASSISTANCE_LEVELS: [&str; 3] = ["Helps Nobody", "Helps Allies", "Helps Friends and Allies"];
const MOOD_LEVELS: [&str; 8] = [
    "Neutral", "Afraid", "Annoyed", "Cocky", "Drugged", "Pleasant", "Angry", "Sad",
];
const SOUND_LEVELS: [&str; 5] = ["Loud", "Normal", "Silent", "Very Loud", "Quiet"];
const IMPACT_MATERIALS: [&str; 14] = [
    "Stone",
    "Cloth",
    "Dirt",
    "Glass",
    "Grass",
    "Metal",
    "Organic",
    "Skin",
    "Water",
    "Wood",
    "Heavy Stone",
    "Heavy Metal",
    "Armor",
    "Chainlink",
];
const ATTACK_ANIMS: [&str; 5] = ["Any", "Left", "Right", "Forward", "Back"];

/// The UI rendering system for the creature editor window.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<6>>>,
    mut session: ResMut<crate::session::Session>,
    mut state: ResMut<CreatureEditorState>,
) {
    if session.editing_creature.is_none() {
        return;
    }

    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    // Check if creature selection changed
    if session.editing_creature != state.loaded_fid {
        if let Some(fid) = session.editing_creature {
            state.data = CreatureData::load(&session, fid);
            state.loaded_fid = Some(fid);
            state.status = format!("Loaded creature {:08X}", fid);
        } else {
            state.data = None;
            state.loaded_fid = None;
            state.status.clear();
        }
    }

    let CreatureEditorState {
        loaded_fid: _,
        ref mut data,
        ref mut tab,
        ref mut status,
    } = *state;

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "creature"), |ui| {
        let Some(data) = data else {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Creature Editor");
                    ui.add_space(8.0);
                    ui.label("No creature selected.");
                    ui.label("Double-click any creature in the Objects window (Actors -> Creature) to edit.");
                });
            });
            return;
        };

        // Header section (Identity)
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("FormID: {:08X}", data.form_id));
                ui.separator();
                ui.label("Editor ID:");
                ui.add(egui::TextEdit::singleline(&mut data.edid).desired_width(180.0));
                ui.separator();
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut data.full_name).desired_width(180.0));
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Creature Type:");
                let cur_type = data.creature_type as usize;
                let cur_label = CREATURE_TYPES.get(cur_type).copied().unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("crea_type")
                    .selected_text(cur_label)
                    .show_ui(ui, |ui| {
                        for (i, &name) in CREATURE_TYPES.iter().enumerate() {
                            ui.selectable_value(&mut data.creature_type, i as u8, name);
                        }
                    });

                ui.separator();
                ui.label("Script:");
                formid_field(ui, "crea_script", &mut data.script);
            });
        });

        ui.add_space(4.0);

        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(tab, CreatureTab::Stats, "Stats & Attributes");
            ui.selectable_value(tab, CreatureTab::FlagsAndTraits, "Flags & Traits");
            ui.selectable_value(tab, CreatureTab::AiData, "AI Data");
            ui.selectable_value(tab, CreatureTab::ModelAndAudio, "Model & Audio");
            ui.selectable_value(tab, CreatureTab::Template, "Template");
        });
        ui.separator();

        // Main Tab Content inside ScrollArea
        let avail_h = (ui.available_height() - 48.0).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("crea_scroll")
            .max_height(avail_h)
            .show(ui, |ui| match *tab {
                CreatureTab::Stats => {
                    draw_stats_tab(ui, data);
                }
                CreatureTab::FlagsAndTraits => {
                    draw_flags_tab(ui, data);
                }
                CreatureTab::AiData => {
                    draw_ai_tab(ui, data);
                }
                CreatureTab::ModelAndAudio => {
                    draw_model_tab(ui, data);
                }
                CreatureTab::Template => {
                    draw_template_tab(ui, data);
                }
            });

        ui.separator();

        // Bottom Action Bar
        let mut do_revert = false;
        ui.horizontal(|ui| {
            if ui
                .add_sized([160.0, 28.0], egui::Button::new("Apply Changes to Mod"))
                .clicked()
            {
                data.save(&mut session);
                *status = format!(
                    "Changes to {:08X} queued in active mod (save with Status panel).",
                    data.form_id
                );
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
                session.close_creature();
            }

            ui.add_space(8.0);
            ui.label(&*status);
        });

        if do_revert {
            if let Some(loaded) = CreatureData::load(&session, data.form_id) {
                *data = loaded;
            }
            *status = "Reverted changes to disk/mod state.".to_string();
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

fn draw_stats_tab(ui: &mut egui::Ui, data: &mut CreatureData) {
    ui.columns(2, |cols| {
        // Left Column: Leveling and Vitals
        cols[0].group(|ui| {
            ui.strong("Leveling & Experience");
            ui.add_space(4.0);

            let mut is_mult = (data.flags & (1 << 7)) != 0;
            if ui.checkbox(&mut is_mult, "PC Level Mult").changed() {
                if is_mult {
                    data.flags |= 1 << 7;
                } else {
                    data.flags &= !(1 << 7);
                }
            }

            ui.horizontal(|ui| {
                if is_mult {
                    ui.label("Level Mult:");
                    let mut mult_f = data.level_or_mult as f32 / 1000.0;
                    if ui
                        .add(
                            egui::DragValue::new(&mut mult_f)
                                .speed(0.05)
                                .range(0.1..=10.0),
                        )
                        .changed()
                    {
                        data.level_or_mult = (mult_f * 1000.0) as i16;
                    }
                } else {
                    ui.label("Level:");
                    ui.add(egui::DragValue::new(&mut data.level_or_mult).range(1..=100));
                }
            });

            ui.horizontal(|ui| {
                ui.label("Calc Min:");
                ui.add(egui::DragValue::new(&mut data.calc_min).range(0..=100));
                ui.label("Calc Max:");
                ui.add(egui::DragValue::new(&mut data.calc_max).range(0..=100));
            });

            ui.horizontal(|ui| {
                ui.label("Speed Mult %:");
                ui.add(egui::DragValue::new(&mut data.speed_mult).range(10..=1000));
            });
        });

        cols[0].add_space(6.0);
        cols[0].group(|ui| {
            ui.strong("Vitals & Economy");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Health (HP):");
                ui.add(egui::DragValue::new(&mut data.health).range(1..=50000));
                ui.label("Fatigue:");
                ui.add(egui::DragValue::new(&mut data.fatigue).range(0..=50000));
            });

            ui.horizontal(|ui| {
                ui.label("Karma / Alignment:");
                ui.add(
                    egui::DragValue::new(&mut data.karma)
                        .speed(10.0)
                        .range(-1000.0..=1000.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Barter Gold:");
                ui.add(egui::DragValue::new(&mut data.barter_gold).range(0..=65535));
                ui.label("Disp Base:");
                ui.add(egui::DragValue::new(&mut data.disposition_base).range(-100..=100));
            });

            ui.horizontal(|ui| {
                ui.label("Base Scale:");
                ui.add(
                    egui::DragValue::new(&mut data.base_scale)
                        .speed(0.02)
                        .range(0.1..=10.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Turning Speed:");
                ui.add(
                    egui::DragValue::new(&mut data.turning_speed)
                        .speed(0.5)
                        .range(0.0..=360.0),
                );
                ui.label("Foot Weight:");
                ui.add(
                    egui::DragValue::new(&mut data.foot_weight)
                        .speed(0.5)
                        .range(0.0..=1000.0),
                );
            });
        });

        // Right Column: Combat Skills and SPECIAL
        cols[1].group(|ui| {
            ui.strong("Combat Skills & Attacks");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Combat Skill:");
                ui.add(egui::DragValue::new(&mut data.combat_skill).range(0..=100));
                ui.label("Magic Skill:");
                ui.add(egui::DragValue::new(&mut data.magic_skill).range(0..=100));
            });

            ui.horizontal(|ui| {
                ui.label("Stealth Skill:");
                ui.add(egui::DragValue::new(&mut data.stealth_skill).range(0..=100));
                ui.label("Unarmed Damage:");
                ui.add(egui::DragValue::new(&mut data.damage).range(0..=1000));
            });

            ui.horizontal(|ui| {
                ui.label("Attack Reach:");
                ui.add(egui::DragValue::new(&mut data.attack_reach).range(0..=255));
                ui.label("Attack Anim:");
                let anim_idx = data.unarmed_attack_anim as usize;
                let anim_label = ATTACK_ANIMS.get(anim_idx).copied().unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("crea_anim")
                    .selected_text(anim_label)
                    .show_ui(ui, |ui| {
                        for (i, &name) in ATTACK_ANIMS.iter().enumerate() {
                            ui.selectable_value(&mut data.unarmed_attack_anim, i as u16, name);
                        }
                    });
            });
        });

        cols[1].add_space(6.0);
        cols[1].group(|ui| {
            ui.strong("S.P.E.C.I.A.L. Attributes");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Strength:");
                ui.add(egui::DragValue::new(&mut data.strength).range(1..=10));
                ui.label("Perception:");
                ui.add(egui::DragValue::new(&mut data.perception).range(1..=10));
            });

            ui.horizontal(|ui| {
                ui.label("Endurance:");
                ui.add(egui::DragValue::new(&mut data.endurance).range(1..=10));
                ui.label("Charisma:");
                ui.add(egui::DragValue::new(&mut data.charisma).range(1..=10));
            });

            ui.horizontal(|ui| {
                ui.label("Intelligence:");
                ui.add(egui::DragValue::new(&mut data.intelligence).range(1..=10));
                ui.label("Agility:");
                ui.add(egui::DragValue::new(&mut data.agility).range(1..=10));
            });

            ui.horizontal(|ui| {
                ui.label("Luck:");
                ui.add(egui::DragValue::new(&mut data.luck).range(1..=10));
            });
        });
    });
}

fn draw_flags_tab(ui: &mut egui::Ui, data: &mut CreatureData) {
    ui.group(|ui| {
        ui.strong("Creature Flags");
        ui.add_space(4.0);

        ui.columns(3, |cols| {
            cols[0].strong("Movement & Behavior");
            flag_checkbox(&mut cols[0], &mut data.flags, 0, "Biped");
            flag_checkbox(&mut cols[0], &mut data.flags, 6, "Walks");
            flag_checkbox(&mut cols[0], &mut data.flags, 4, "Swims");
            flag_checkbox(&mut cols[0], &mut data.flags, 5, "Flies");
            flag_checkbox(&mut cols[0], &mut data.flags, 22, "Can't Open Doors");
            flag_checkbox(&mut cols[0], &mut data.flags, 23, "Immobile");
            flag_checkbox(&mut cols[0], &mut data.flags, 18, "No Combat in Water");

            cols[1].strong("Combat & Health");
            flag_checkbox(&mut cols[1], &mut data.flags, 1, "Essential");
            flag_checkbox(&mut cols[1], &mut data.flags, 3, "Respawn");
            flag_checkbox(&mut cols[1], &mut data.flags, 30, "Invulnerable");
            flag_checkbox(&mut cols[1], &mut data.flags, 28, "Is Ghost");
            flag_checkbox(&mut cols[1], &mut data.flags, 26, "No Knockdowns");
            flag_checkbox(&mut cols[1], &mut data.flags, 27, "Not Pushable");
            flag_checkbox(&mut cols[1], &mut data.flags, 20, "No VATS Melee");

            cols[2].strong("Visuals & Processing");
            flag_checkbox(&mut cols[2], &mut data.flags, 11, "No Blood Spray");
            flag_checkbox(&mut cols[2], &mut data.flags, 12, "No Blood Decal");
            flag_checkbox(&mut cols[2], &mut data.flags, 19, "No Shadow");
            flag_checkbox(&mut cols[2], &mut data.flags, 24, "Tilt Front/Back");
            flag_checkbox(&mut cols[2], &mut data.flags, 25, "Tilt Left/Right");
            flag_checkbox(
                &mut cols[2],
                &mut data.flags,
                29,
                "No Rotating to Head-track",
            );
            flag_checkbox(&mut cols[2], &mut data.flags, 15, "No Head");
            flag_checkbox(&mut cols[2], &mut data.flags, 21, "Allow PC Dialogue");
            flag_checkbox(&mut cols[2], &mut data.flags, 9, "No Low Level Proc");
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Sound & Material Properties");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Impact Material Type:");
            let cur_mat = data.impact_material as usize;
            let mat_label = IMPACT_MATERIALS.get(cur_mat).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_mat")
                .selected_text(mat_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in IMPACT_MATERIALS.iter().enumerate() {
                        ui.selectable_value(&mut data.impact_material, i as u32, name);
                    }
                });

            ui.separator();
            ui.label("Sound Level:");
            let cur_snd = data.sound_level as usize;
            let snd_label = SOUND_LEVELS.get(cur_snd).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_snd_lvl")
                .selected_text(snd_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in SOUND_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut data.sound_level, i as u32, name);
                    }
                });
        });
    });
}

fn flag_checkbox(ui: &mut egui::Ui, flags: &mut u32, bit: u8, label: &str) {
    let mut set = (*flags & (1 << bit)) != 0;
    if ui.checkbox(&mut set, label).changed() {
        if set {
            *flags |= 1 << bit;
        } else {
            *flags &= !(1 << bit);
        }
    }
}

fn draw_ai_tab(ui: &mut egui::Ui, data: &mut CreatureData) {
    ui.group(|ui| {
        ui.strong("Behavior & Personality");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Aggression:");
            let aggr_idx = data.aggression as usize;
            let aggr_label = AGGRESSION_LEVELS
                .get(aggr_idx)
                .copied()
                .unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_aggr")
                .selected_text(aggr_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in AGGRESSION_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut data.aggression, i as u8, name);
                    }
                });

            ui.separator();
            ui.label("Confidence:");
            let conf_idx = data.confidence as usize;
            let conf_label = CONFIDENCE_LEVELS
                .get(conf_idx)
                .copied()
                .unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_conf")
                .selected_text(conf_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in CONFIDENCE_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut data.confidence, i as u8, name);
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Assistance:");
            let asst_idx = data.assistance.max(0) as usize;
            let asst_label = ASSISTANCE_LEVELS
                .get(asst_idx)
                .copied()
                .unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_asst")
                .selected_text(asst_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in ASSISTANCE_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut data.assistance, i as i8, name);
                    }
                });

            ui.separator();
            ui.label("Mood:");
            let mood_idx = data.mood as usize;
            let mood_label = MOOD_LEVELS.get(mood_idx).copied().unwrap_or("Unknown");
            egui::ComboBox::from_id_salt("crea_mood")
                .selected_text(mood_label)
                .show_ui(ui, |ui| {
                    for (i, &name) in MOOD_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut data.mood, i as u8, name);
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Energy Level:");
            ui.add(egui::Slider::new(&mut data.energy_level, 0..=100));
            ui.separator();
            ui.label("Responsibility:");
            ui.add(egui::Slider::new(&mut data.responsibility, 0..=100));
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Aggro Radius & Combat Style");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(
                &mut data.aggro_radius_behavior,
                "Aggro Radius Behavior (Warn -> Attack)",
            );
            if data.aggro_radius_behavior {
                ui.label("Radius:");
                ui.add(
                    egui::DragValue::new(&mut data.aggro_radius)
                        .speed(10)
                        .range(0..=10000),
                );
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Combat Style (ZNAM FormID):");
            formid_field(ui, "crea_cstyle", &mut data.combat_style);
        });
    });
}

fn draw_model_tab(ui: &mut egui::Ui, data: &mut CreatureData) {
    ui.group(|ui| {
        ui.strong("3D Model File");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Model Path (.nif):");
            ui.add(
                egui::TextEdit::singleline(&mut data.model)
                    .desired_width(ui.available_width() - 40.0),
            );
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Audio & Voice");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Voice Type (VTCK):");
            formid_field(ui, "crea_voice", &mut data.voice);

            ui.separator();
            ui.label("Inherits Sounds from (CSCR):");
            formid_field(ui, "crea_sounds", &mut data.inherits_sounds_from);
        });
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Combat & Damage Datasets");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Body Part Data (PNAM):");
            formid_field(ui, "crea_body_part", &mut data.body_part_data);

            ui.separator();
            ui.label("Impact Dataset (CNAM):");
            formid_field(ui, "crea_impact", &mut data.impact_dataset);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Death Item (INAM):");
            formid_field(ui, "crea_death_item", &mut data.death_item);

            ui.separator();
            ui.label("Melee Weapon List (LNAM):");
            formid_field(ui, "crea_melee_list", &mut data.melee_weapon_list);
        });
    });
}

fn draw_template_tab(ui: &mut egui::Ui, data: &mut CreatureData) {
    ui.group(|ui| {
        ui.strong("Creature Template");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Template Creature (TPLT FormID):");
            formid_field(ui, "crea_template", &mut data.template);
        });

        ui.add_space(6.0);
        ui.label("Inherit data from template:");

        let template_flags_def: [(u8, &str); 10] = [
            (0, "Use Traits"),
            (1, "Use Stats"),
            (2, "Use Factions"),
            (3, "Use Actor Effect List"),
            (4, "Use AI Data"),
            (5, "Use AI Packages"),
            (6, "Use Model/Animation"),
            (7, "Use Base Data"),
            (8, "Use Inventory"),
            (9, "Use Script"),
        ];

        ui.columns(2, |cols| {
            for (i, &(bit, name)) in template_flags_def.iter().enumerate() {
                let col = &mut cols[i % 2];
                let mut set = (data.template_flags & (1 << bit)) != 0;
                if col.checkbox(&mut set, name).changed() {
                    if set {
                        data.template_flags |= 1 << bit;
                    } else {
                        data.template_flags &= !(1 << bit);
                    }
                }
            }
        });
    });
}

/// Component marking the dynamic Creature window entity.
#[derive(Component)]
pub struct CreatureWindow;

/// Component marking the dynamic Creature camera entity.
#[derive(Component)]
pub struct CreatureCamera;

/// Dynamically spawn or despawn the Creature window based on `session.editing_creature`.
pub fn manage_window(
    mut commands: Commands,
    session: Res<crate::session::Session>,
    mut win_query: Query<(Entity, &mut Window), With<CreatureWindow>>,
    cam_query: Query<Entity, With<CreatureCamera>>,
    state: Res<CreatureEditorState>,
) {
    let win_opt = win_query.iter_mut().next();
    let cam_opt = cam_query.iter().next();

    if session.editing_creature.is_some() {
        if let Some((_, mut win)) = win_opt {
            // Window is already open, keep its title up to date
            if let Some(data) = &state.data {
                let expected_title = if !data.full_name.trim().is_empty() {
                    format!("Creature - {} ({})", data.edid, data.full_name)
                } else {
                    format!("Creature - {}", data.edid)
                };
                if win.title != expected_title {
                    win.title = expected_title;
                }
            }
        } else {
            // Window is NOT open -> dynamically spawn it now!
            let title = state
                .data
                .as_ref()
                .map(|d| {
                    if !d.full_name.trim().is_empty() {
                        format!("Creature - {} ({})", d.edid, d.full_name)
                    } else {
                        format!("Creature - {}", d.edid)
                    }
                })
                .unwrap_or_else(|| "Creature Editor".to_string());

            let window = commands
                .spawn((
                    Window {
                        title,
                        resolution: (840, 760).into(),
                        position: WindowPosition::new(IVec2::new(500, 60)),
                        ..default()
                    },
                    CreatureWindow,
                ))
                .id();

            commands.spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_2d()),
                bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
                bevy_egui::EguiSchedule::new(crate::windows::Panel::Creature),
                crate::windows::PanelContext::<6>,
                CreatureCamera,
            ));
        }
    } else if let Some((win_ent, _)) = win_opt {
        // Editing creature is None -> despawn the window and camera immediately!
        commands.entity(win_ent).despawn();
        if let Some(cam_ent) = cam_opt {
            commands.entity(cam_ent).despawn();
        }
    }
}
