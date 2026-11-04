//! The object editor's header, and nothing else.
//!
//! The editor used to be 26,000 lines: 120 `draw_*` page functions, a per-type
//! `load_*`/`save_*` pair for each, and one 845-field state struct they all read.
//! A page is data now — a list of sections in `schemas/<game>/object_window.json`,
//! rendered by `panels::schema_grid` — so none of that is reachable.
//!
//! What is left is the part that belongs to no page: the window, its header (the
//! editor ID, name, model, script, header flags and object bounds), and the two
//! page kinds the registry holds. A page is a view of the record rather than a
//! copy of it, so the only state here is the header's own.
//!
//! `EditorPage` is the seam for the next thing that needs real behaviour. A
//! widget that cannot be described as "draw these subrecords" — a curve editor, a
//! script editor, a dialogue tree — is an `EditorPage`, registered in [`Pages`],
//! named by a schema's `editor` field.

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

/// Active tab for tabbed editors. Retained because the header sets it; no page
/// reads it now that pages are data, and the section headings are the grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTab {
    #[default]
    Main,
    Secondary,
    Models,
    Audio,
    Advanced,
}

/// The object editor's state: the record's header, and which record it is.
///
/// The header is the part of a record that is on every record and edited above the
/// page. Everything else a modder edits is read from the record by the page, so
/// nothing else is copied here — a copy would be a second answer to "what does this
/// field say", and the two would drift.
#[derive(Resource, Default)]
pub struct ObjectEditorState {
    /// The record this state was loaded from. A change is what makes the editor
    /// re-read.
    pub loaded_target: Option<(u32, [u8; 4])>,
    pub tab: EditorTab,
    /// What the footer says.
    pub status: String,

    /// The record's FormID and signature.
    pub form_id: u32,
    pub sig: [u8; 4],

    /// The editor ID, the name, and whether the name is a string-table ID.
    pub edid: String,
    pub full_name: String,
    /// Whether `FULL` is a 4-byte string ID rather than text — which is what a
    /// localized plugin holds.
    ///
    /// The name shown is the ID resolved through the plugin's string table, but
    /// saving writes the record's own bytes back: a localized name is authored in
    /// the string table, not in the record, so editing this text field cannot
    /// change what the game shows.
    pub full_name_localized: bool,
    pub model: String,
    pub script: Option<u32>,

    /// The record header's flag word, as loaded.
    ///
    /// The word is a header field, not a subrecord: its named bits differ per game
    /// *and per record type* (bit 10 is "Quest Item" on a Fallout weapon and
    /// unnamed on Fallout 4's), so the header renders [`Self::header_flag_bits`]
    /// from the schema rather than a constant. This field holds the whole word so a
    /// bit the schema does not name is carried through untouched.
    pub record_flags: u32,
    /// The schema-named header flag bits for this record type: the bit, its name,
    /// and whether it is set.
    pub header_flag_bits: Vec<(u32, String, bool)>,

    /// Object bounds, which are on nearly every object and are edited here rather
    /// than through the page.
    pub obnd_min: [i16; 3],
    pub obnd_max: [i16; 3],

    /// The bytes of each header subrecord, so a field the editor did not change is
    /// written back exactly as the record had it. A modder opening a record and
    /// pressing Apply must not rewrite bytes they never touched.
    pub raw_subs: HashMap<[u8; 4], Vec<u8>>,
}

/// A page of the object editor: one record type's layout, with its own state.
///
/// A page owns the fields it shows. That is what makes a page per game possible:
/// Fallout 4's weapon and Fallout: New Vegas's weapon are different layouts with
/// different fields, so they are different pages rather than one page that
/// branches on the game. A page that owns nothing is a page that can only be the
/// schema grid.
///
/// Which page a record type gets is the schema's answer — the object-window
/// table's `editor` field names one, and its `sections` describe it — so a game
/// customises its object editor by naming pages in
/// `schemas/<game>/object_window.json`, not by adding a window.
pub trait EditorPage: Send + Sync {
    /// The name the schema's table uses for this page.
    fn name(&self) -> &str;

    /// The games whose layouts this page is correct for.
    ///
    /// The guard that keeps a page from being served to a game it does not fit. A
    /// page written against Fallout: New Vegas's byte offsets reads garbage from
    /// Fallout 4's record of the same signature, and nothing in the UI would say
    /// so — the fields would simply be wrong. Naming the games makes that a
    /// message instead.
    fn games(&self) -> &'static [&'static str];

    /// Read the record into this page's own state.
    ///
    /// Called when the object editor opens a record, and again after a revert.
    fn load(&mut self, _session: &crate::session::Session, _form_id: u32, _sig: [u8; 4]) {}

    /// Draw the page.
    ///
    /// `session` is mutable because a field edit goes straight into the document
    /// as it is made — the same path the reference property grid uses. `shared`
    /// is the record's header, which every page shows and no page owns.
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut crate::session::Session,
        shared: &mut ObjectEditorState,
    );

    /// Write this page's edits into the document.
    ///
    /// Empty for a page that writes as it edits, which is what the schema grid
    /// does: a field edit is one subrecord's re-encoded bytes, and that is what
    /// the document stores.
    fn save(&self, _session: &mut crate::session::Session, _form_id: u32) {}
}

/// The schema grid as a page.
///
/// A record type can name this page outright — `"editor": "grid"` — which is how
/// a game says "this type needs no grouping, show the layout flat". It is also
/// what the object editor falls back to when the table describes nothing.
#[derive(Default)]
pub struct GridPage {
    /// The grid's own widget state: the last field touched, and why it failed.
    edit: crate::panels::FieldEdit,
}

impl EditorPage for GridPage {
    fn name(&self) -> &str {
        "grid"
    }

    fn games(&self) -> &'static [&'static str] {
        // The grid reads the schema and nothing else, so it is correct for every
        // game whose schema decodes — which is every game.
        crate::game::ALL_IDS
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut crate::session::Session,
        shared: &mut ObjectEditorState,
    ) {
        crate::panels::schema_grid(ui, session, shared.form_id, &mut self.edit);
    }
}

/// A page described by its schema sections rather than written in code.
///
/// The schema's object-window table gives a category a list of sections — a
/// heading and the subrecords under it — and this page draws them. That is the
/// whole page: there is no per-record-type code here, so a game customises its
/// object editor by writing sections in `schemas/<game>/object_window.json`.
///
/// What makes one list serve several games is that a section names a subrecord and
/// the record decides whether it has one. Fallout: New Vegas's weapon carries
/// `VATS` and Fallout 4's does not, so the same `VATS` section is content in one
/// and absent in the other — no branching on the game, and no second page to keep
/// in step.
///
/// A subrecord the sections do not name is still reachable: it is listed under
/// "Everything else" at the end. A page that quietly dropped a subrecord would
/// make part of the record uneditable with nothing saying so, which is the one
/// failure this editor exists not to have.
#[derive(Default)]
pub struct SectionPage {
    /// The grid's own widget state: the last field touched, and why it failed.
    edit: crate::panels::FieldEdit,
}

impl EditorPage for SectionPage {
    fn name(&self) -> &str {
        "sections"
    }

    fn games(&self) -> &'static [&'static str] {
        // The sections come from the game's own table and the layouts from its own
        // schema, so the page is correct wherever its table is.
        crate::game::ALL_IDS
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut crate::session::Session,
        shared: &mut ObjectEditorState,
    ) {
        // The flattened page: the template's sections first, then this game's own,
        // with a shared heading merged rather than repeated.
        let sections = session
            .schema
            .page_sections(String::from_utf8_lossy(&shared.sig).as_ref());
        if sections.is_empty() {
            // A category with no sections is not a described page; the grid is the
            // honest fallback rather than an empty body.
            crate::panels::schema_grid(ui, session, shared.form_id, &mut self.edit);
            return;
        }
        crate::panels::sectioned_grid(ui, session, shared.form_id, &sections, &mut self.edit);
    }
}

/// Every page the object editor can open, by the name the schema's table uses.
///
/// Two pages, and a name that is neither is a schema typo — reported on the page
/// rather than silently drawn as the default, so a typo is visible. The registry
/// creates a page the first time its name is used and keeps it after, so a page can
/// own the fields a modder is in the middle of editing.
#[derive(Resource, Default)]
pub struct Pages {
    map: std::collections::BTreeMap<String, Box<dyn EditorPage>>,
}

impl Pages {
    /// The page registered under `name`, or `None` when nothing is.
    pub fn get(&mut self, name: &str) -> Option<&mut (dyn EditorPage + '_)> {
        if !self.map.contains_key(name) {
            let page: Option<Box<dyn EditorPage>> = match name {
                "grid" => Some(Box::new(GridPage::default())),
                "sections" => Some(Box::new(SectionPage::default())),
                _ => None,
            };
            self.map.insert(name.to_string(), page?);
        }
        let page = self.map.get_mut(name)?;
        Some(&mut **page)
    }

    /// Whether a page is registered under `name`, without creating it.
    pub fn has(&self, name: &str) -> bool {
        name == "grid" || name == "sections" || self.map.contains_key(name)
    }

    /// The pages that have been opened this session, by name.
    pub fn opened(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
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

/// Read a NUL-terminated string out of a subrecord payload.
fn zstring(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches('\0')
        .to_string()
}

impl ObjectEditorState {
    /// Load a record's header into the editor.
    ///
    /// The header is all that is left of the editor's own state. It used to parse
    /// every subrecord of every record type into 845 fields, because a page was a
    /// function that read them; pages are data now, and the grid reads the record
    /// itself. What the header owns is the handful of fields that are on every
    /// record and edited above the page: the editor ID, the name, the model, the
    /// script, the header flags and the object bounds.
    ///
    /// The record is read through [`crate::session::Session::resolved_record`],
    /// which is the one answer that covers all three sources: a record staged
    /// this session (a duplicate, which no plugin on disk holds), a record the
    /// plugin being edited defines, and a record overridden from a master with
    /// this session's edits applied.
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
        self.status = format!("Loaded {} {:08X}", nvre_util::tag_str(&sig), form_id);

        // The plugin whose strings a localized name is resolved through: the
        // plugin being edited for its own records, and the defining plugin for
        // one inherited from a master. A staged duplicate names the plugin
        // being edited, which is where it will be written.
        let source_name = if session.doc.record(form_id).is_some() {
            Some(session.doc.name().to_string())
        } else {
            session
                .load_order()
                .ok()
                .and_then(|lo| {
                    nvre_world::fields::winner(lo, form_id)
                        .map(|(pi, _)| lo.plugins[pi].name.clone())
                })
                .or_else(|| Some(session.doc.name().to_string()))
        };
        let (raw_subs, flags, found) = match (session.resolved_record(form_id), source_name.as_deref())
        {
            (Some(rec), Some(name)) => {
                let localized = session
                    .load_order()
                    .ok()
                    .and_then(|lo| {
                        lo.plugins
                            .iter()
                            .find(|p| p.name.eq_ignore_ascii_case(name))
                            .map(|p| p.header.is_localized())
                    })
                    .unwrap_or(false);
                let strings = session.strings.for_plugin(name, localized);
                let subs = rec.subrecords().unwrap_or_default();
                let edid = subs
                    .iter()
                    .find(|s| &s.sig == b"EDID")
                    .map(|s| zstring(&s.data))
                    .unwrap_or_default();
                let (full_name, full_name_localized) = match subs.iter().find(|s| &s.sig == b"FULL") {
                    Some(s) if localized => (
                        // `for_plugin` answers `None` for a plugin that is not
                        // localized, so a localized record always has a table —
                        // possibly an empty one, which yields no name rather than
                        // four bytes of decoded noise.
                        strings
                            .and_then(|t| {
                                t.get(u32::from_le_bytes([
                                    s.data[0], s.data[1], s.data[2], s.data[3],
                                ]))
                            })
                            .unwrap_or_default()
                            .to_string(),
                        true,
                    ),
                    Some(s) => (zstring(&s.data), false),
                    None => (String::new(), false),
                };
                let model = subs
                    .iter()
                    .find(|s| &s.sig == b"MODL")
                    .map(|s| zstring(&s.data))
                    .unwrap_or_default();
                let script = subs
                    .iter()
                    .find(|s| &s.sig == b"SCRI")
                    .and_then(|s| {
                        (s.data.len() >= 4).then(|| {
                            u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])
                        })
                    })
                    .filter(|f| *f != 0);
                let mut raw: HashMap<[u8; 4], Vec<u8>> = HashMap::new();
                for s in &subs {
                    raw.entry(s.sig).or_insert_with(|| s.data.clone());
                }
                // The object bounds are the one subrecord the header edits in
                // place rather than through the grid, because they are on nearly
                // every object and are three pairs of numbers.
                let (obnd_min, obnd_max) = match subs.iter().find(|s| &s.sig == b"OBND") {
                    Some(s) if s.data.len() >= 12 => {
                        let i16_at = |o: usize| {
                            i16::from_le_bytes([s.data[o], s.data[o + 1]])
                        };
                        (
                            [i16_at(0), i16_at(2), i16_at(4)],
                            [i16_at(6), i16_at(8), i16_at(10)],
                        )
                    }
                    _ => ([0; 3], [0; 3]),
                };
                self.edid = edid;
                self.full_name = full_name;
                self.full_name_localized = full_name_localized;
                self.model = model;
                self.script = script;
                self.obnd_min = obnd_min;
                self.obnd_max = obnd_max;
                (raw, rec.header.flags, true)
            }
            _ => (HashMap::new(), 0, false),
        };

        if !found {
            return;
        }

        self.raw_subs = raw_subs;
        self.record_flags = flags;
        self.header_flag_bits = session
            .schema
            .header_flags_for(&sig)
            .iter()
            .map(|(bit, name)| (*bit, name.clone(), flags & (1 << bit) != 0))
            .collect();
    }

    /// Write the header's fields into the document.
    ///
    /// Only the header: the page's fields are written as they are edited, through
    /// [`crate::session::Session::set_object_field`], because a page is a view of
    /// the record rather than a copy of it. What is left here is the four text
    /// fields and the flag word, which are on every record and belong to no page.
    ///
    /// Each text field is written back with `encode_text_kept`, so the bytes the
    /// record already had are preserved when the text has not changed — a modder
    /// opening a record and pressing Apply must not rewrite a subrecord the editor
    /// did not touch.
    pub fn save_to_session(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        if self.raw_subs.contains_key(b"EDID") {
            session.doc.set_sub(
                fid,
                b"EDID",
                encode_text_kept(self.raw_subs.get(b"EDID").map(|v| v.as_slice()), &self.edid),
            );
        } else if !self.edid.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"EDID", encode_zstring(&self.edid));
        }
        if self.raw_subs.contains_key(b"FULL") {
            // A localized name is a string ID in the record and its text in the
            // plugin's string table, so what is written back is the record's own
            // bytes: the display text cannot change the ID, and encoding it would
            // replace the ID with the resolved name.
            let bytes = if self.full_name_localized {
                self.raw_subs.get(b"FULL").cloned().unwrap_or_default()
            } else {
                encode_text_kept(
                    self.raw_subs.get(b"FULL").map(|v| v.as_slice()),
                    &self.full_name,
                )
            };
            session.doc.set_sub(fid, b"FULL", bytes);
        } else if !self.full_name.trim().is_empty() {
            session
                .doc
                .set_sub(fid, b"FULL", encode_zstring(&self.full_name));
        }
        // The race's MODL belongs to its head-part collection rather than to the
        // record's own model, so the header panel's model field does not apply.
        if self.sig != *b"RACE" {
            if self.raw_subs.contains_key(b"MODL") {
                session.doc.set_sub(
                    fid,
                    b"MODL",
                    encode_text_kept(self.raw_subs.get(b"MODL").map(|v| v.as_slice()), &self.model),
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

        // The named header bits go back into the word the record was loaded with.
        // A bit the schema does not name is not touched: the word is a header
        // field, and rewriting it from a table that names 3 of 32 bits would
        // clear the other 29.
        let mut flags = self.record_flags;
        for (bit, _, set) in &self.header_flag_bits {
            if *bit >= 32 {
                continue;
            }
            if *set {
                flags |= 1 << bit;
            } else {
                flags &= !(1 << bit);
            }
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
    }
}

/// Draw the Object Editor UI.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<6>>>,
    mut session: ResMut<crate::session::Session>,
    mut state: ResMut<ObjectEditorState>,
    mut pages: ResMut<Pages>,
) {
    let Some((fid, sig)) = session.editing_object else {
        return;
    };

    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    // Reload on a new target *or* when the document has changed since the last
    // read. The target alone is what this used to check, which left the grid
    // showing the record as it was: an EDID typed into the header and applied
    // wrote through to the document without re-reading it, so the editor and
    // the object window's list both went stale.
    if state.loaded_target != Some((fid, sig)) || session.stale {
        state.load_from_session(&session, fid, sig);
        session.stale = false;
    }

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "object_editor"), |ui| {
        // Top Header
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Type: {}", nvre_util::tag_str(&state.sig)));
                ui.separator();
                ui.label(format!("FormID: {:08X}", state.form_id));
                ui.separator();
                ui.label("Editor ID:");
                ui.add(egui::TextEdit::singleline(&mut state.edid).desired_width(180.0));
                ui.separator();
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut state.full_name).desired_width(170.0));
                ui.separator();
                header_flags_menu(ui, &mut state);
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

        let _ = body(ui, &mut session, &mut state, &mut pages);

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

/// What [`body`] drew, so a caller can act on it without reaching into egui's
/// widget store for an id it would have to guess.
///
/// The window ignores this; the tests use it. It is also the natural place for a
/// future "scroll to the bounds" button that does not want to search the page.
#[derive(Clone, Copy, Debug)]
pub struct BodyLayout {
    /// Where the bounds group landed on screen, when the record carries one.
    pub bounds: Option<egui::Rect>,
    /// The window's visible part of the scroll area, on screen.
    ///
    /// A widget whose rect is outside this is drawn but cannot be seen: it is
    /// scrolled out of view, above or below. That is the difference between a
    /// group inside the page — reachable by scrolling — and one drawn under the
    /// page, which no scroll ever brings into view.
    pub visible: egui::Rect,
    /// The page's full size, including the part scrolled out of view.
    ///
    /// This is what the scroll bar's handle is proportional to, and — the reason
    /// it is reported — it is what proves a widget is *in* the page. A group
    /// drawn after the scroll area closed is not counted in the page's height at
    /// all, so it sits below this rect and the bar never scrolls to it.
    pub content_size: egui::Vec2,
}

impl Default for BodyLayout {
    fn default() -> Self {
        Self {
            bounds: None,
            visible: egui::Rect::ZERO,
            content_size: egui::Vec2::ZERO,
        }
    }
}

/// The record's body: the page for its type, then the object bounds.
///
/// The body is the schema's answer, not the editor's own map: the
/// object-window table names the page for this record type, and the registry
/// holds it. A signature the table names a page for gets that page; a signature
/// it names **no** page for gets the schema grid, which is every field the
/// layout declares and nothing a game has to be named to see.
///
/// A page also declares which games it is correct for, and a page served to a
/// game it does not fit is a message rather than a silently wrong form: a page
/// written against Fallout: New Vegas's byte offsets would read garbage from
/// Fallout 4's record of the same signature.
///
/// **Everything here scrolls together.** The object bounds are the last thing
/// *inside* the scroll area, not a band pinned under it. Pinned, a page with many
/// sections pushed them past the window's bottom edge, where they were drawn but
/// unreachable: the page scrolled and the bounds stayed put, out of view. Inside,
/// they follow the fields they belong to.
///
/// Broader than the window's own `ui` so a test can render the body into a bare
/// `egui::Context` and read back where the bounds landed.
pub fn body(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut ObjectEditorState,
    pages: &mut Pages,
) -> BodyLayout {
    let mut bounds = None;
    let avail_h = (ui.available_height() - 48.0).max(200.0);
    let sig_key = String::from_utf8_lossy(&state.sig).into_owned();
    // A page name wins over sections, because a page is the more specific
    // answer: it exists for the record types whose editing is not "show the
    // fields" — a leveled list needs rows added and removed, a package needs its
    // block list — and sections cannot express that. Everything else is
    // described by its sections, which is the default and the majority.
    let category = session.schema.object_window.categories.get(&sig_key);
    let named = category.and_then(|c| c.editor.clone());
    let described = named.is_none() && session.schema.page_is_described(&sig_key);
    let page_name: Option<String> = if described {
        Some("sections".to_string())
    } else {
        named
    };
    let game = session.game.id;
    // The bar sits at the panel's right edge, not at the end of the widest
    // row — see [`crate::panels::vertical_scroll`].
    let shown = crate::panels::vertical_scroll("obj_scroll")
        .max_height(avail_h)
        .show(ui, |ui| {
            match page_name.as_deref() {
                Some(name) => match pages.get(name) {
                    Some(page) if page.games().contains(&game) => {
                        page.ui(ui, session, state)
                    }
                    // The page exists but is not for this game. Say which games
                    // it is for, so the fix (name a page this game has, or write
                    // one) is visible in the message.
                    Some(page) => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!(
                                "the schema names page {name:?} for {}, and that page is \
                                 only for {}",
                                session.game.name,
                                page.games().join(", ")
                            ),
                        );
                    }
                    // A page name the registry does not know is a table typo;
                    // say so rather than silently showing nothing.
                    None => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!(
                                "the schema names editor block {name:?}, which is not registered"
                            ),
                        );
                    }
                },
                // No page named: the schema grid. This is the case for every
                // record type in a game whose pages have not been written yet,
                // and for a signature no game has a page for at all.
                None => match pages.get("grid") {
                    Some(page) => page.ui(ui, session, state),
                    None => {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            "the schema grid is not registered",
                        );
                    }
                },
            }

            // The object bounds are a shared subrecord: every type that carries
            // one shows them here, so no page has to repeat them.
            if state.raw_subs.contains_key(b"OBND") {
                ui.add_space(6.0);
                let group = ui.group(|ui| {
                    ui.strong("Object Bounds (OBND)");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Min:");
                        for i in 0..3 {
                            ui.add(
                                egui::DragValue::new(&mut state.obnd_min[i])
                                    .range(-32768..=32767),
                            );
                        }
                        ui.separator();
                        ui.label("Max:");
                        for i in 0..3 {
                            ui.add(
                                egui::DragValue::new(&mut state.obnd_max[i])
                                    .range(-32768..=32767),
                            );
                        }
                    });
                });
                bounds = Some(group.response.rect);
            }
        });
    BodyLayout {
        bounds,
        visible: shown.inner_rect,
        content_size: shown.content_size,
    }
}

/// The record header's flags, as a menu of the bits the schema names.
///
/// A menu rather than a row of checkboxes because the count is per record type:
/// Fallout 4's `ACTI` names 19 bits and Fallout: New Vegas's `WEAP` names 3, so a
/// row would either wrap into the header's second line or leave a gap. The
/// button's label says how many of the named bits are set, which is the one
/// thing worth reading at a glance; the popup is where they are edited.
///
/// A record type the schema names no flags for gets no menu at all. That is the
/// honest answer rather than an empty popup: it means xEdit names no flag for
/// that type, not that the record has no flags.
fn header_flags_menu(ui: &mut egui::Ui, state: &mut ObjectEditorState) {
    if state.header_flag_bits.is_empty() {
        return;
    }
    let set = state
        .header_flag_bits
        .iter()
        .filter(|(_, _, on)| *on)
        .count();
    let label = if set == 0 {
        format!("Flags ({})", state.header_flag_bits.len())
    } else {
        format!("Flags: {set} of {}", state.header_flag_bits.len())
    };
    ui.menu_button(label, |ui| {
        ui.set_min_width(260.0);
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for (bit, name, on) in state.header_flag_bits.iter_mut() {
                    // The bit number is shown beside the name: the schema's names
                    // are what xEdit calls a bit, and xEdit leaves some unnamed
                    // ("Unknown 27"), so the number is how a modder reconciles
                    // the two.
                    let mut want = *on;
                    if ui
                        .checkbox(&mut want, format!("{name}  (bit {bit})"))
                        .changed()
                    {
                        *on = want;
                    }
                }
            });
    });
}

/// The record types a FormID field at `path` may point at, if the schema says.
///
/// A FormID is one field inside a subrecord's layout, so the path is walked
/// through the type the same way the grid walks the value: `vats.effect` for a
/// FormID inside a struct, the empty path for a subrecord that *is* a FormID.
/// Empty when the schema does not restrict the field, which is the "anything"
/// case.
pub fn formid_targets_in(
    schema: &nvre_esm::Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    path: &str,
) -> Vec<String> {
    let Some(def) = schema.lookup_in_block(rec_sig, sub_sig, block) else {
        return Vec::new();
    };
    // The walk stays in `TypeDef`, which is the resolved form: a struct's fields
    // already carry resolved types, so nothing here has to resolve again.
    fn at_path(t: &nvre_esm::TypeDef, parts: &[&str]) -> Option<nvre_esm::TypeDef> {
        let Some((part, rest)) = parts.split_first() else {
            return Some(t.clone());
        };
        match t {
            nvre_esm::TypeDef::Struct(fields) => {
                let f = fields.iter().find(|f| f.name == *part)?;
                at_path(&f.ty, rest)
            }
            nvre_esm::TypeDef::Union(variants) => {
                // The record chose one variant; the grid shows that one's
                // fields, and any variant that names this field is where the
                // FormID's targets come from.
                for (_, v) in variants {
                    if let Some(found) = at_path(v, parts) {
                        return Some(found);
                    }
                }
                None
            }
            nvre_esm::TypeDef::PeekUnion { variants, .. }
            | nvre_esm::TypeDef::ValueUnion { variants, .. } => {
                for v in variants {
                    if let Some(found) = at_path(v, parts) {
                        return Some(found);
                    }
                }
                None
            }
            // An array's elements are addressed by index, which a field path
            // does not carry.
            _ => None,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    match at_path(&schema.resolve_def(&def.ty), &parts) {
        Some(nvre_esm::TypeDef::FormIdRef { of }) => of,
        _ => Vec::new(),
    }
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

pub fn record_dropdown(
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

/// Generate a clean, human-readable window title for any record.
///
/// The type's name is the schema's answer, the same one the Objects window
/// labels the category with: the object-window table's own name for the
/// category where it has one, otherwise the record definition's name.
pub fn window_title(
    schema: &nvre_esm::Schema,
    sig: &[u8; 4],
    edid: &str,
    full_name: &str,
) -> String {
    let key = String::from_utf8_lossy(sig);
    let type_name = schema
        .object_window
        .categories
        .get(key.as_ref())
        .and_then(|c| c.name.clone())
        .or_else(|| schema.records.get(key.as_ref()).map(|d| d.name.clone()))
        .unwrap_or_else(|| nvre_util::tag_str(sig));
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
        let current_title = window_title(&session.schema, &sig, &state.edid, &state.full_name);

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
