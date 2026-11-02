//! The Cell Editor window: a cell's own `CELL` record, field by field.
//!
//! Opened from the Cells window's right-click menu, on the row it was opened
//! on. GECK's Cell View has no such dialog — its cell properties live in the
//! Edit dialog's tabs — but a window of its own is what the rest of this
//! editor does (the Object Editor, File Details, Size Info), and the cell's
//! data is large enough to deserve one.
//!
//! # Why a window and not a panel in the Cells window
//!
//! A cell's record carries twenty-odd subrecords: flags, the grid, the
//! lighting, the water, and a dozen references to other records. The Cells
//! window is a browser over 30,000 rows; a property grid that opens inside it
//! would either take the whole window or scroll out of sight. This follows
//! [`crate::object_editor`]'s pattern instead: the window is spawned and
//! despawned every frame by [`manage_window`] on the strength of
//! [`crate::session::Session::editing_cell`], so it is only mapped while it is
//! wanted and never leaves a frame behind when it closes.
//!
//! # The tabs
//!
//! GECK's cell edit dialog is four tabs, and this is the same four, in the
//! same order: **Common Data**, **Lighting**, **Interior Data** and
//! **Directional Ambient Lighting**.
//!
//! The split is GECK's own, and it is worth keeping even where it does not
//! match the subrecords. `Has Water` is a `DATA` bit and `Height` is `XCLW`,
//! but they sit together under Common Data because that is where a modder
//! looks for them. The six-axis ambient cube is part of `XCLL`, and GECK gives
//! it a tab of its own anyway, because it is a thing you set once rather than
//! part of the cell's own lighting.
//!
//! A field is edited in exactly one place. The four `DATA` bits Common Data
//! offers are not repeated in the Interior Data tab, and the water height and
//! type are not repeated in the water noise section: one value with two
//! editors is a value that can be shown twice and mean two things.
//!
//! # Byte preservation
//!
//! A subrecord a save does not touch is written back byte for byte. Every
//! struct here is read from the record's own bytes and written back over them
//! — see [`crate::object_editor`]'s notes on why rebuilding from the fields
//! the editor owns destroys whatever the spec does not name. The unused byte
//! after each `XCLL` colour and the three unused bytes at the end of an
//! exterior cell's `XCLC` are examples: they are present in the game's own
//! records and are not the editor's to decide.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_egui::egui;

use crate::session::cell_lighting;

/// The schedule label for the Cell Editor window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellEditorSchedule;

/// Marker component for the Cell Editor window.
#[derive(Component)]
pub struct CellEditorWindow;

/// Marker component for the Cell Editor's camera.
#[derive(Component)]
pub struct CellEditorCamera;

/// `CELL` `DATA` flags, the uint8 flag word from fopdoc's CELL page.
///
/// The names are the spec's. Bit 0x10 is the one the spec marks unknown, so it
/// is listed as such rather than dropped: a flag word is kept whole and only
/// the named bits are interpreted, or a value the spec does not name would be
/// silently cleared by a checkbox that does not know it.
const CELL_DATA_FLAGS: &[(u8, &str)] = &[
    (0x01, "Is Interior Cell"),
    (0x02, "Has Water"),
    (0x04, "Invert Fast Travel Behaviour"),
    (0x08, "No LOD Water"),
    (0x10, "(Unknown)"),
    (0x20, "Public Place"),
    (0x40, "Hand Changed"),
    (0x80, "Behave Like Exterior"),
];

/// `CELL` `LNAM` light-template inherit flags, from fopdoc's Light Template
/// section on the same page.
const CELL_LNAM_FLAGS: &[(u32, &str)] = &[
    (0x0000_0001, "Ambient Color"),
    (0x0000_0002, "Directional Color"),
    (0x0000_0004, "Fog Color"),
    (0x0000_0008, "Fog Near"),
    (0x0000_0010, "Fog Far"),
    (0x0000_0020, "Directional Rotation"),
    (0x0000_0040, "Directional Fade"),
    (0x0000_0080, "Fog Clip Distance"),
    (0x0000_0100, "Fog Power"),
];

/// `CELL` `XCLC` Force Hide Land flags, from fopdoc's XCLC section.
///
/// The spec types this a uint32 flag word, but an exterior cell's own bytes
/// are a single byte plus three unused ones — measured on every one of
/// FalloutNV.esm's 30,109 exterior cells. So the byte is read and written and
/// the three after it are the record's own.
const CELL_LAND_FLAGS: &[(u8, &str)] = &[
    (0x01, "Hide Quad 1"),
    (0x02, "Hide Quad 2"),
    (0x04, "Hide Quad 3"),
    (0x08, "Hide Quad 4"),
];

/// The Cell Editor's editable state.
///
/// The loaded subrecords are kept in [`CellEditorState::raw_subs`] as well as
/// being decoded into the fields below: a field the user has not touched has
/// to be written back as the record's own bytes, which needs the original
/// payload, and a struct the editor does not model at all (Skyrim's 52-byte
/// `XCLL` tail) has to survive a save untouched.
#[derive(Resource, Default)]
pub struct CellEditorState {
    /// The cell loaded, so a re-read only happens when the target moves or the
    /// document changes under it.
    pub loaded_target: Option<u32>,
    pub form_id: u32,

    // EDID / FULL
    pub edid: String,
    pub full_name: String,
    /// Whether `FULL` is a 4-byte string ID rather than text, which is what a
    /// localized plugin holds. Saving writes the record's own bytes back.
    pub full_name_localized: bool,

    /// `DATA` flags as the raw byte, with each named bit masked inside it.
    pub data_flags: u8,

    /// `XCLC` grid: x, y, and the Force Hide Land byte.
    pub grid_x: i32,
    pub grid_y: i32,
    pub land_flags: u8,
    /// Whether the record carries an `XCLC` at all. An interior does not.
    pub has_grid: bool,

    /// `XCLL` lighting, decoded by [`crate::session::cell_lighting`].
    pub lighting: Option<crate::session::CellLighting>,
    /// Whether the record carries an `XCLL`. Every exterior does not.
    pub has_lighting: bool,

    /// `LTMP` — the light template record, `None` for the engine's null.
    pub light_template: Option<u32>,
    /// `LNAM` — the inherit flags as the raw word.
    pub lnam_flags: u32,

    /// `XCLW` water height.
    pub water_height: f32,
    /// Whether the record carries an `XCLW`.
    pub has_water_height: bool,
    /// `XNAM` water noise texture.
    pub water_noise: String,
    /// `XCWT` water record.
    pub water: Option<u32>,

    /// `XCLR` regions, as FormIDs.
    pub regions: Vec<u32>,

    /// `XEZN` encounter zone.
    pub encounter_zone: Option<u32>,
    /// `XOWN` owner.
    pub owner: Option<u32>,
    /// `XRNK` faction rank, only meaningful when the owner is a `FACT`.
    pub owner_rank: i32,
    /// `XCAS` acoustic space.
    pub acoustic_space: Option<u32>,
    /// `XCIM` image space.
    pub image_space: Option<u32>,
    /// `XCMO` music type.
    pub music_type: Option<u32>,
    /// `XCCM` climate.
    pub climate: Option<u32>,

    /// The loaded subrecords, one per signature. A signature that appears
    /// twice keeps its last value here — a `CELL` record carries each at most
    /// once, so that is not a loss for this type.
    pub raw_subs: std::collections::HashMap<[u8; 4], Vec<u8>>,
    /// Every `XCLR` payload in file order, so the list is rewritten whole.
    pub raw_xclr: Vec<Vec<u8>>,

    /// Which tab is showing.
    ///
    /// GECK's cell edit dialog is four tabs, and this is the same four. A tab
    /// is state rather than a local because it has to survive the frames: an
    /// egui widget built from a local is discarded unread, so the tab would
    /// revert the moment the pointer left the button.
    pub tab: CellEditorTab,

    /// The window's own status line, under the buttons.
    pub status: String,
}

/// GECK's four cell-edit tabs.
///
/// The names are GECK's own. The interior data and the directional ambient
/// lighting are separated from the lighting and common data the way GECK
/// separates them, because that is where a modder looks for them: the ambient
/// cube is a six-axis thing you set once, not part of the cell's own lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellEditorTab {
    /// The water, the flags that say what a cell is, and the three reference
    /// dropdowns.
    #[default]
    CommonData,
    /// `XCLL` and the light template it can inherit from.
    Lighting,
    /// Interior Data: the ownership and the encounter zone.
    InteriorData,
    /// The six-axis directional ambient cube.
    DirectionalAmbient,
}

impl CellEditorTab {
    /// The tab strip's labels, in GECK's own order.
    pub const ALL: [CellEditorTab; 4] = [
        CellEditorTab::CommonData,
        CellEditorTab::Lighting,
        CellEditorTab::InteriorData,
        CellEditorTab::DirectionalAmbient,
    ];

    /// The button's label.
    pub fn label(self) -> &'static str {
        match self {
            CellEditorTab::CommonData => "Common Data",
            CellEditorTab::Lighting => "Lighting",
            CellEditorTab::InteriorData => "Interior Data",
            CellEditorTab::DirectionalAmbient => "Directional Ambient Lighting",
        }
    }

    /// GECK's own note on what the tab is for, shown as the tab's hover text.
    pub fn hint(self) -> &'static str {
        match self {
            CellEditorTab::CommonData => {
                "the water, the flags that say what the cell is, and its default acoustic, music and image space"
            }
            CellEditorTab::Lighting => {
                "the cell's own ambient, directional and fog colours, and the template it can inherit them from"
            }
            CellEditorTab::InteriorData => {
                "the cell's ownership, encounter zone and climate"
            }
            CellEditorTab::DirectionalAmbient => {
                "the six-axis directional ambient cube, one colour per axis"
            }
        }
    }
}

/// Read a `u32` FormID payload, or `None` for a short one or an all-zero one.
///
/// Zero is the engine's null reference, and a cell whose subrecord holds it
/// has no such reference — showing it as a chosen FormID would be wrong.
fn formid(b: Option<&Vec<u8>>) -> Option<u32> {
    let b = b?;
    if b.len() < 4 {
        return None;
    }
    let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    (v != 0).then_some(v)
}

fn zstring(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches('\0')
        .to_string()
}

impl CellEditorState {
    /// Read the cell's record out of the session.
    ///
    /// Every field is reset first: a cell that has no `XCLL` must not show the
    /// previous one's lighting, which is what a reload without the reset does.
    pub fn load_from_session(&mut self, session: &crate::session::Session, cell: u32) {
        self.loaded_target = Some(cell);
        self.form_id = cell;
        // Back to the first tab for a new cell, the way GECK's dialog opens on
        // Common Data. Keeping the previous cell's tab would put the user on
        // Directional Ambient Lighting for a cell that has no ambient cube.
        self.tab = CellEditorTab::CommonData;
        self.status = format!("Loaded cell {cell:08X}");

        self.edid.clear();
        self.full_name.clear();
        self.full_name_localized = false;
        self.data_flags = 0;
        self.grid_x = 0;
        self.grid_y = 0;
        self.land_flags = 0;
        self.has_grid = false;
        self.lighting = None;
        self.has_lighting = false;
        self.light_template = None;
        self.lnam_flags = 0;
        self.water_height = 0.0;
        self.has_water_height = false;
        self.water_noise.clear();
        self.water = None;
        self.regions.clear();
        self.encounter_zone = None;
        self.owner = None;
        self.owner_rank = 0;
        self.acoustic_space = None;
        self.image_space = None;
        self.music_type = None;
        self.climate = None;
        self.raw_subs.clear();
        self.raw_xclr.clear();

        // The record as the mod will carry it, which is the one a save writes.
        let Some(rec) = session.resolved_record(cell) else {
            self.status = format!("cell {cell:08X} is not in the load order");
            return;
        };
        let Ok(subs) = rec.subrecords() else {
            self.status = format!("cell {cell:08X} has no readable subrecords");
            return;
        };

        // The plugin whose strings a localized name resolves through.
        let localized = session.doc.record(cell).is_some() && session.doc.is_localized();

        for s in &subs {
            let d = &s.data;
            match &s.sig {
                b"EDID" => self.edid = zstring(d),
                b"FULL" => {
                    self.full_name = zstring(d);
                    self.full_name_localized = localized;
                }
                b"DATA" => {
                    if !d.is_empty() {
                        self.data_flags = d[0];
                    }
                }
                b"XCLC" => {
                    self.has_grid = true;
                    if d.len() >= 8 {
                        self.grid_x = i32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                        self.grid_y = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
                    }
                    if d.len() >= 9 {
                        self.land_flags = d[8];
                    }
                }
                b"XCLL" => {
                    self.has_lighting = true;
                    self.lighting = cell_lighting(d);
                }
                b"LTMP" => self.light_template = formid(Some(d)),
                b"LNAM" => {
                    if d.len() >= 4 {
                        self.lnam_flags = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                    }
                }
                b"XCLW" => {
                    self.has_water_height = true;
                    if d.len() >= 4 {
                        self.water_height = f32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                    }
                }
                b"XNAM" => self.water_noise = zstring(d),
                b"XCWT" => self.water = formid(Some(d)),
                b"XCLR" => {
                    // A list: one FormID per subrecord, in file order.
                    if d.len() >= 4 {
                        self.raw_xclr.push(d.clone());
                        let v = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                        if v != 0 {
                            self.regions.push(v);
                        }
                    }
                }
                b"XEZN" => self.encounter_zone = formid(Some(d)),
                b"XOWN" => self.owner = formid(Some(d)),
                b"XRNK" => {
                    if d.len() >= 4 {
                        self.owner_rank = i32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                    }
                }
                b"XCAS" => self.acoustic_space = formid(Some(d)),
                b"XCIM" => self.image_space = formid(Some(d)),
                b"XCMO" => self.music_type = formid(Some(d)),
                b"XCCM" => self.climate = formid(Some(d)),
                _ => {}
            }
            // One entry per signature, the last winning. Every subrecord the
            // editor does not model lands here too, so a save leaves it alone.
            self.raw_subs.insert(s.sig, d.clone());
        }
    }

    /// Queue every field the window owns onto the document.
    ///
    /// Each write starts from the record's own bytes where it has them, so a
    /// field the user did not touch is written back unchanged and an unmodelled
    /// tail survives.
    pub fn save_to_session(&self, session: &mut crate::session::Session) {
        let fid = self.form_id;

        if self.raw_subs.contains_key(b"EDID") {
            session.doc.set_sub(
                fid,
                b"EDID",
                text_kept(self.raw_subs.get(b"EDID").map(|v| v.as_slice()), &self.edid),
            );
        }
        if self.raw_subs.contains_key(b"FULL") {
            // A localized name is a string ID in the record and its text in
            // the plugin's string table, so what is written back is the
            // record's own bytes: the display text cannot change the ID.
            let bytes = if self.full_name_localized {
                self.raw_subs.get(b"FULL").cloned().unwrap_or_default()
            } else {
                text_kept(
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

        // DATA is a required subrecord — fopdoc marks it `+` — but a record
        // with none still must not gain one: a save that changes nothing
        // changes the record.
        if self.raw_subs.contains_key(b"DATA") {
            session.doc.set_sub(fid, b"DATA", vec![self.data_flags]);
        }
        // The header's own flags are not written. The window no longer edits
        // them, so an apply has nothing to say about them, and writing the
        // word unchanged would still mark the record as touched — which is
        // what puts it in the mod's ONAM array as an override it did not ask
        // for. They stay editable on the reference's own record.

        // XCLC: the grid's own bytes with the two int32s and the hide byte
        // overwritten. An interior has none, and writing one would give it a
        // grid the game would then read.
        //
        // The length is the record's own, not the spec's: 30,098 of the
        // master's exterior cells store twelve bytes and eleven store eight —
        // the spec marks the hide byte and its three unused bytes optional, and
        // the game takes it at its word. Resizing to nine would grow those
        // eleven on a save that changed nothing.
        if self.has_grid {
            let mut b = self.raw_subs.get(b"XCLC").cloned().unwrap_or_default();
            if b.len() < 9 {
                b.resize(9, 0);
            }
            b[0..4].copy_from_slice(&self.grid_x.to_le_bytes());
            b[4..8].copy_from_slice(&self.grid_y.to_le_bytes());
            b[8] = self.land_flags;
            // An eight-byte subrecord has no hide byte, so it is left at eight.
            if self.raw_subs.get(b"XCLC").map(|o| o.len()) == Some(8) {
                b.truncate(8);
            }
            session.doc.set_sub(fid, b"XCLC", b);
        }

        // XCLL: the record's own payload, resized to the struct it already
        // had, with the forty bytes the FNV layout names overwritten. A
        // Skyrim cell's extra fifty-two survive because the length is the
        // record's, not the spec's.
        if self.has_lighting {
            if let Some(l) = self.lighting {
                let mut b = self.raw_subs.get(b"XCLL").cloned().unwrap_or_default();
                if b.len() < 40 {
                    b.resize(40, 0);
                }
                b[0..3].copy_from_slice(&l.ambient);
                b[4..7].copy_from_slice(&l.directional);
                b[8..11].copy_from_slice(&l.fog);
                b[12..16].copy_from_slice(&l.fog_near.to_le_bytes());
                b[16..20].copy_from_slice(&l.fog_far.to_le_bytes());
                b[20..24].copy_from_slice(&l.rotation_xy.to_le_bytes());
                b[24..28].copy_from_slice(&l.rotation_z.to_le_bytes());
                b[28..32].copy_from_slice(&l.directional_fade.to_le_bytes());
                b[32..36].copy_from_slice(&l.fog_clip_dist.to_le_bytes());
                b[36..40].copy_from_slice(&l.fog_power.to_le_bytes());
                session.doc.set_sub(fid, b"XCLL", b);
            }
        }

        // The light template is a pair: writing one without the other leaves
        // the cell pointing at a template it does not inherit from.
        if self.light_template.is_some() || self.raw_subs.contains_key(b"LTMP") {
            let v = self.light_template.unwrap_or(0);
            session.doc.set_sub(fid, b"LTMP", v.to_le_bytes().to_vec());
            if self.raw_subs.contains_key(b"LNAM") || self.light_template.is_some() {
                session
                    .doc
                    .set_sub(fid, b"LNAM", self.lnam_flags.to_le_bytes().to_vec());
            }
        } else if self.raw_subs.contains_key(b"LNAM") {
            session
                .doc
                .set_sub(fid, b"LNAM", self.lnam_flags.to_le_bytes().to_vec());
        }

        if self.has_water_height {
            session
                .doc
                .set_sub(fid, b"XCLW", self.water_height.to_le_bytes().to_vec());
        }
        if self.raw_subs.contains_key(b"XNAM") {
            session.doc.set_sub(
                fid,
                b"XNAM",
                text_kept(
                    self.raw_subs.get(b"XNAM").map(|v| v.as_slice()),
                    &self.water_noise,
                ),
            );
        }

        // XCLR is a list, so it is written as one. Removing the last region
        // clears the list rather than leaving an empty subrecord behind.
        if self.raw_subs.contains_key(b"XCLR") || !self.regions.is_empty() {
            let payloads: Vec<Vec<u8>> = if self.regions.is_empty() {
                Vec::new()
            } else {
                self.regions
                    .iter()
                    .map(|f| f.to_le_bytes().to_vec())
                    .collect()
            };
            session.doc.set_sub_list(fid, b"XCLR", payloads);
        }

        set_formid_sub(session, fid, b"XCWT", self.water, &self.raw_subs);
        set_formid_sub(session, fid, b"XEZN", self.encounter_zone, &self.raw_subs);
        set_formid_sub(session, fid, b"XCAS", self.acoustic_space, &self.raw_subs);
        set_formid_sub(session, fid, b"XCIM", self.image_space, &self.raw_subs);
        set_formid_sub(session, fid, b"XCMO", self.music_type, &self.raw_subs);
        set_formid_sub(session, fid, b"XCCM", self.climate, &self.raw_subs);
        // The owner is written whenever the record carried one, because
        // clearing it is a choice the user makes by emptying the field; but a
        // rank without an owner is meaningless, so it follows the owner.
        set_formid_sub(session, fid, b"XOWN", self.owner, &self.raw_subs);
        if self.owner.is_some() {
            if self.raw_subs.contains_key(b"XRNK") || self.owner_rank != 0 {
                session
                    .doc
                    .set_sub(fid, b"XRNK", self.owner_rank.to_le_bytes().to_vec());
            }
        } else if self.raw_subs.contains_key(b"XRNK") {
            session.doc.remove_sub(fid, b"XRNK");
        }

        session.stale = true;
        session.status = format!("queued cell {fid:08X} changes");
    }
}

/// Write a text field back, keeping the record's own bytes when it is unchanged.
///
/// The game's records hold text that is not valid UTF-8, and re-encoding an
/// untouched field would replace those bytes with the replacement character.
fn text_kept(original: Option<&[u8]>, text: &str) -> Vec<u8> {
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

/// Write an optional FormID subrecord, removing it when the field is cleared
/// and adding it when the record never had one.
///
/// A save must not add a subrecord the record never had: a cell that never
/// named a climate would gain one, which is a change to a cell that was not
/// edited.
fn set_formid_sub(
    session: &mut crate::session::Session,
    fid: u32,
    sig: &[u8; 4],
    value: Option<u32>,
    raw_subs: &std::collections::HashMap<[u8; 4], Vec<u8>>,
) {
    match (value, raw_subs.contains_key(sig)) {
        (Some(v), _) => session.doc.set_sub(fid, sig, v.to_le_bytes().to_vec()),
        (None, true) => session.doc.remove_sub(fid, sig),
        (None, false) => {}
    }
}

/// Spawn or despawn the window.
///
/// Runs every frame, which is what makes the window appear the same frame the
/// menu is clicked and disappear the same frame it is closed: the state is the
/// only thing that decides, and the window is its consequence.
pub fn manage_window(
    mut commands: Commands,
    session: Res<crate::session::Session>,
    mut state: ResMut<CellEditorState>,
    mut win_query: Query<(Entity, &mut Window), With<CellEditorWindow>>,
    cam_query: Query<Entity, With<CellEditorCamera>>,
) {
    let win_opt = win_query.iter_mut().next();
    let cam_opt = cam_query.iter().next();

    if let Some(cell) = session.editing_cell {
        // A cell the load order no longer has cannot be edited, so the window
        // is not opened for it rather than opening empty.
        if session.resolved_record(cell).is_none() {
            return;
        }
        let title = cell_title(&session, cell);
        if let Some((_, mut win)) = win_opt {
            if win.title != title {
                win.title = title;
            }
        } else {
            let window = commands
                .spawn((
                    Window {
                        title,
                        // Wider than it was before the reference rows gained
                        // labels: a label plus a 220px dropdown plus the hex
                        // field does not fit in 860.
                        resolution: (940, 820).into(),
                        position: WindowPosition::new(IVec2::new(400, 40)),
                        ..default()
                    },
                    CellEditorWindow,
                ))
                .id();
            commands.spawn((
                Camera2d,
                Projection::Orthographic(bevy::camera::OrthographicProjection::default_2d()),
                bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
                bevy_egui::EguiSchedule::new(CellEditorSchedule),
                // After the panels (0..=5), the Object Editor (6), File
                // Details (7) and Size Info (8).
                crate::windows::PanelContext::<9>,
                CellEditorCamera,
            ));
        }
    } else if let Some((win_ent, _)) = win_opt {
        commands.entity(win_ent).despawn();
        if let Some(cam_ent) = cam_opt {
            commands.entity(cam_ent).despawn();
        }
        // The state is dropped with the window, so the next cell opens on a
        // clean slate rather than on the previous one's fields.
        state.loaded_target = None;
    }
}

/// The window's title: the cell's Editor ID and FormID.
fn cell_title(session: &crate::session::Session, cell: u32) -> String {
    let name = session
        .index
        .cells
        .get(&cell)
        .and_then(|c| c.edid.clone())
        .unwrap_or_else(|| format!("{cell:08X}"));
    format!("Cell Editor - {name} ({cell:08X})")
}

/// Draw the window.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<9>>>,
    mut session: ResMut<crate::session::Session>,
    mut state: ResMut<CellEditorState>,
) {
    let Some(cell) = session.editing_cell else {
        return;
    };
    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    // Reload on a new target or when the document has changed since the last
    // read, the same two conditions the object editor reloads on.
    if state.loaded_target != Some(cell) || session.stale {
        state.load_from_session(&session, cell);
        session.stale = false;
    }

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "cell_editor"), |ui| {
        header_group(ui, &mut state);
        ui.add_space(4.0);
        tab_strip(ui, &mut state);

        // The body scrolls, the tab strip does not: GECK's own dialog keeps
        // the tabs pinned above the fields, and a strip that scrolled away
        // would leave the user unable to say which tab they are on.
        egui::ScrollArea::vertical().show(ui, |ui| match state.tab {
            CellEditorTab::CommonData => common_data_group(ui, &mut session, &mut state),
            CellEditorTab::Lighting => {
                lighting_group(ui, &mut state);
                template_group(ui, &mut session, &mut state);
            }
            CellEditorTab::InteriorData => {
                flags_group(ui, &mut state);
                grid_group(ui, &mut state);
                water_noise_group(ui, &mut state);
                interior_data_group(ui, &mut session, &mut state);
            }
            CellEditorTab::DirectionalAmbient => directional_ambient_group(ui, &mut state),
        });

        ui.separator();
        let mut do_revert = false;
        ui.horizontal(|ui| {
            if ui
                .add_sized([180.0, 28.0], egui::Button::new("Apply Changes to Mod"))
                .clicked()
            {
                state.save_to_session(&mut session);
                state.status = format!("Changes queued for cell {:08X}.", state.form_id);
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
                session.close_cell_editor();
            }
            ui.add_space(8.0);
            ui.label(&state.status);
        });
        if do_revert {
            // The cell is read out into a local first: `state` is borrowed
            // mutably by the load and its own `form_id` is the argument.
            let cell = state.form_id;
            state.load_from_session(&session, cell);
            state.status = "Reverted to the mod's state.".to_string();
        }
    });
}

/// Editor ID and name.
///
/// The header's own flags are not shown here. `Quest Item` and `Persistent`
/// are per-reference concerns in a cell — a cell's own references carry them,
/// and a `CELL` record setting them is not something the game's cells do — so
/// a checkbox for them on the cell would be a control with no meaning. They
/// stay editable where they belong, on the reference's own record in the
/// Object Editor.
fn header_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(format!("FormID: {:08X}", state.form_id));
            ui.separator();
            ui.label("Editor ID:");
            ui.add(egui::TextEdit::singleline(&mut state.edid).desired_width(220.0));
        });
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut state.full_name)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
    });
}

/// **Common Data** — GECK's own first tab in the cell edit dialog.
///
/// GECK's [Common Data page](https://ck.uesp.net/wiki/Common_Data) leads with
/// the three reference dropdowns, then the water, then the four flags that say
/// what a cell *is*, and this is the same order. They are pulled out of the
/// per-subrecord groups below and put here because that is how a modder meets
/// them: the tab is where you open a cell to change its water or to stop the
/// player fast-travelling out of it, and the subrecord each field lives in is
/// not what they are thinking about.
///
/// The fields are grouped by what they do rather than by subrecord, so the
/// section's own name is not a signature: `Has Water` is a `DATA` bit and
/// `Height` is `XCLW`, and they sit together because GECK puts them together.
///
/// The tab strip, above the body.
///
/// A strip of selectable buttons rather than egui's dockable tab panels: these
/// are not dockable, and a button strip is the lighter thing. The chosen tab is
/// `state.tab`, which is what makes the choice survive the frames while the
/// pointer is elsewhere.
fn tab_strip(ui: &mut egui::Ui, state: &mut CellEditorState) {
    ui.horizontal(|ui| {
        for tab in CellEditorTab::ALL {
            if ui
                .selectable_label(state.tab == tab, egui::RichText::new(tab.label()).strong())
                .on_hover_text(tab.hint())
                .clicked()
            {
                state.tab = tab;
            }
        }
    });
    ui.separator();
}

/// **Interior Data** — GECK's third tab.
///
/// What is left of the record once the water, the lighting and the ambient
/// cube have their own tabs: the flags that say what the cell is, the grid it
/// sits in, the water noise texture, the regions it belongs to, and the
/// ownership.
///
/// GECK's own Interior Data tab is the interior-only half of this — the
/// encounter zone, the owner and the public-area flag — and the rest is what
/// the `CELL` record still carries with nowhere better to go. They are one tab
/// here because GECK's split leaves an exterior cell with only Common Data and
/// Lighting, and an exterior still has a grid, regions and flags.
fn interior_data_group(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut CellEditorState,
) {
    flags_group(ui, state);
    grid_group(ui, state);
    water_noise_group(ui, state);
    references_group(ui, session, state);
    ownership_group(ui, session, state);
}

/// **Directional Ambient Lighting** — GECK's fourth tab.
///
/// The six-axis directional ambient cube: X+, X-, Y+, Y-, Z+ and Z-, each a
/// colour. Only Skyrim's and Fallout 4's `XCLL` carries one, so a Fallout cell
/// says so rather than showing six empty colour rows.
///
/// GECK also offers "Set From Ambient", which copies the cell's own ambient
/// colour into all six axes — a one-click starting point for a cube you then
/// adjust. That is what the button here does.
fn directional_ambient_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Directional Ambient Lighting");
        ui.add_space(4.0);
        let Some(l) = state.lighting.as_mut() else {
            ui.weak("this cell carries no XCLL lighting to take an ambient cube from");
            return;
        };
        let Some(dalc) = l.directional_ambient.as_mut() else {
            ui.weak(
                "this cell's XCLL is the 40-byte Fallout layout, which has no directional ambient cube",
            );
            return;
        };
        if ui
            .button("Set From Ambient")
            .on_hover_text(
                "copy the cell's own ambient colour into all six axes, then adjust from there",
            )
            .clicked()
        {
            let ambient = l.ambient;
            for axis in dalc.iter_mut() {
                *axis = ambient;
            }
        }
        ui.add_space(4.0);
        // The six axes in GECK's order, named as GECK names them.
        const AXES: [(&str, usize); 6] = [
            ("X +", 0),
            ("X -", 1),
            ("Y +", 2),
            ("Y -", 3),
            ("Z +", 4),
            ("Z -", 5),
        ];
        for (name, i) in AXES {
            color_row(ui, &format!("{name}:"), &mut dalc[i]);
        }
        if let Some(spec) = l.specular.as_mut() {
            ui.add_space(4.0);
            color_row(ui, "Specular:", spec);
        }
        if let Some(scale) = l.directional_scale.as_mut() {
            ui.horizontal(|ui| {
                ui.label("Scale:");
                ui.add(egui::DragValue::new(scale).speed(0.1));
            });
        }
    });
}

/// GECK's notes on which fields apply are kept as the row's own hint: Has
/// Water and Height are interior-only, and Force Hide Land is exterior-only.
fn common_data_group(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut CellEditorState,
) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Common Data");
        ui.add_space(4.0);

        // The three references GECK leads with. The labels are GECK's own
        // names for them, not the subrecord signatures.
        let aspc = session.records_of(*b"ASPC");
        record_field(
            ui,
            "default_acoustic",
            "Default Acoustic:",
            &aspc,
            &mut state.acoustic_space,
        );
        let musc = session.records_of(*b"MUSC");
        record_field(
            ui,
            "music_type",
            "Music type:",
            &musc,
            &mut state.music_type,
        );
        let imgs = session.records_of(*b"IMGS");
        record_field(
            ui,
            "image_space",
            "ImageSpace:",
            &imgs,
            &mut state.image_space,
        );

        ui.separator();
        ui.strong("Water");
        ui.add_space(2.0);
        // Has Water is a DATA bit, so the checkbox writes into the same byte
        // the flags section shows — the two are one value, and a save that
        // let them disagree would write whichever was applied last.
        let mut has_water = state.data_flags & 0x02 != 0;
        if ui.checkbox(&mut has_water, "Has Water").changed() {
            if has_water {
                state.data_flags |= 0x02;
            } else {
                state.data_flags &= !0x02;
            }
        }
        ui.horizontal(|ui| {
            ui.label("Height:");
            ui.add_enabled(
                state.has_water_height,
                egui::DragValue::new(&mut state.water_height).speed(1.0),
            );
        });
        if !state.has_water_height {
            ui.weak("this cell carries no XCLW subrecord — the height is not saved");
        }
        let waters = session.records_of(*b"WATR");
        record_field(ui, "water", "Water:", &waters, &mut state.water);

        ui.separator();
        ui.strong("Force Hide Land");
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            for &(bit, name) in CELL_LAND_FLAGS {
                let mut on = state.land_flags & bit != 0;
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        state.land_flags |= bit;
                    } else {
                        state.land_flags &= !bit;
                    }
                }
            }
        });
        ui.weak("exterior cells only; the collision remains even when the land is hidden");

        ui.separator();
        // The three flags that are not about the water or the land. They are
        // DATA bits, written into the same byte as the flags section below.
        let mut no_lod = state.data_flags & 0x08 != 0;
        if ui.checkbox(&mut no_lod, "No LOD Water").changed() {
            if no_lod {
                state.data_flags |= 0x08;
            } else {
                state.data_flags &= !0x08;
            }
        }
        let mut no_travel = state.data_flags & 0x04 != 0;
        if ui
            .checkbox(&mut no_travel, "Can't Travel From Here")
            .changed()
        {
            if no_travel {
                state.data_flags |= 0x04;
            } else {
                state.data_flags &= !0x04;
            }
        }
        let mut hand_changed = state.data_flags & 0x40 != 0;
        if ui.checkbox(&mut hand_changed, "Hand Changed").changed() {
            if hand_changed {
                state.data_flags |= 0x40;
            } else {
                state.data_flags &= !0x40;
            }
        }
    });
}

/// `DATA` — the cell's flags, one checkbox per named bit.
///
/// The raw byte is what the checkboxes write into, so a bit the spec does not
/// name survives: clearing bit 0x10 because the spec calls it unknown is not
/// something a checkbox should do.
///
/// The four flags the Common Data section already offers are not repeated
/// here: one value with two editors is a value that can be shown twice and
/// mean two things. This is the whole word, for the bits that are left.
fn flags_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Flags (DATA)");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for &(bit, name) in CELL_DATA_FLAGS {
                // The four Common Data edits, offered there and not twice.
                if COMMON_DATA_FLAG_BITS.contains(&bit) {
                    continue;
                }
                let mut on = state.data_flags & bit != 0;
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        state.data_flags |= bit;
                    } else {
                        state.data_flags &= !bit;
                    }
                }
            }
        });
        if !state.raw_subs.contains_key(b"DATA") {
            ui.weak("this cell carries no DATA subrecord");
        }
    });
}

/// The `DATA` bits Common Data edits, so the flags section does not show them
/// twice. One value, one editor.
const COMMON_DATA_FLAG_BITS: &[u8] = &[0x02, 0x04, 0x08, 0x40];

/// `XCLC` — the grid square, without the Force Hide Land flags.
///
/// The quads moved to Common Data, where GECK puts them, so what is left is
/// the coordinate itself.
fn grid_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    if !state.has_grid {
        return;
    }
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Grid (XCLC)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut state.grid_x));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut state.grid_y));
        });
        ui.weak("the game reads an exterior cell's grid to place it in the world");
    });
}

/// A colour's three bytes: egui's colour picker, and the three raw bytes.
///
/// The picker is `color_edit_button_srgb`, which takes and returns the same
/// `[u8; 3]` the record stores — no conversion, no rounding. A float 0-1 slider
/// would have to round, and rounding is how a colour stops matching the bytes
/// the game wrote.
///
/// The three drag values stay beside it, for two reasons. The picker's HSV
/// space cannot reach every sRGB byte triple exactly, so a value the game wrote
/// that the picker cannot express would become uneditable without them; and
/// typing a byte is faster than dragging a hue wheel when you know the number
/// you want.
fn color_row(ui: &mut egui::Ui, label: &str, rgb: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_srgb(rgb);
        ui.separator();
        for c in rgb.iter_mut() {
            ui.add(egui::DragValue::new(c).range(0..=255));
        }
    });
}

/// `XCLL` — the lighting: ambient, directional, fog, and the fog distances.
///
/// The fog is only shown when the cell has any: a `fog_far` of zero is a cell
/// with no fog at all, and drawing a row of zeroes for it would suggest the
/// fields were broken rather than unused.
fn lighting_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    if !state.has_lighting {
        return;
    }
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Lighting (XCLL)");
        ui.add_space(4.0);
        let Some(l) = state.lighting.as_mut() else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "this cell's XCLL is shorter than the 40 bytes the layout needs",
            );
            return;
        };
        color_row(ui, "Ambient:", &mut l.ambient);
        color_row(ui, "Directional:", &mut l.directional);
        color_row(ui, "Fog:", &mut l.fog);
        ui.horizontal(|ui| {
            ui.label("Fog near:");
            ui.add(egui::DragValue::new(&mut l.fog_near).speed(1.0));
            ui.label("far:");
            ui.add(egui::DragValue::new(&mut l.fog_far).speed(1.0));
        });
        ui.horizontal(|ui| {
            ui.label("Sun rotation XY:");
            ui.add(egui::DragValue::new(&mut l.rotation_xy).range(-180..=180));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut l.rotation_z).range(-90..=90));
        });
        ui.horizontal(|ui| {
            ui.label("Directional fade:");
            ui.add(egui::DragValue::new(&mut l.directional_fade).speed(0.1));
            ui.label("Fog clip:");
            ui.add(egui::DragValue::new(&mut l.fog_clip_dist).speed(1.0));
        });
        ui.horizontal(|ui| {
            ui.label("Fog power:");
            ui.add(egui::DragValue::new(&mut l.fog_power).speed(0.1));
        });
        // The six-axis directional ambient cube has its own tab, the way
        // GECK's dialog separates it: it is a thing you set once, not part of
        // the cell's own lighting.
    });
}

/// `LTMP` / `LNAM` — the light template and what the cell inherits from it.
fn template_group(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut CellEditorState,
) {
    if !state.raw_subs.contains_key(b"LTMP") && !state.raw_subs.contains_key(b"LNAM") {
        return;
    }
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Light Template (LTMP / LNAM)");
        ui.add_space(4.0);
        let templates = session.records_of(*b"LGTM");
        record_field(
            ui,
            "template",
            "Template:",
            &templates,
            &mut state.light_template,
        );
        ui.separator();
        ui.label("Inherit from template:");
        ui.horizontal_wrapped(|ui| {
            for &(bit, name) in CELL_LNAM_FLAGS {
                let mut on = state.lnam_flags & bit != 0;
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        state.lnam_flags |= bit;
                    } else {
                        state.lnam_flags &= !bit;
                    }
                }
            }
        });
    });
}

/// `XCLW` / `XNAM` / `XCWT` — the cell's water noise texture.
///
/// The height and the water type moved to Common Data, where GECK puts them,
/// so what is left is the noise texture — the one water field that is not a
/// number or a reference.
fn water_noise_group(ui: &mut egui::Ui, state: &mut CellEditorState) {
    if !state.raw_subs.contains_key(b"XNAM") {
        return;
    }
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Water Noise Texture (XNAM)");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Noise texture:");
            ui.add(
                egui::TextEdit::singleline(&mut state.water_noise)
                    .desired_width(ui.available_width() - 20.0),
            );
        });
    });
}

/// `XCLR` — the regions the cell is in.
///
/// A list, so it is edited as one: a row per region, and the whole list is
/// rewritten on apply. `XCLR` is the only repeating subrecord a `CELL` record
/// has, and 4,363 of FalloutNV.esm's cells carry at least one.
fn references_group(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut CellEditorState,
) {
    if !state.raw_subs.contains_key(b"XCLR") && state.regions.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Regions (XCLR)");
        ui.add_space(4.0);
        let regions = session.records_of(*b"REGN");
        let mut remove: Option<usize> = None;
        let mut add: Option<u32> = None;
        for (i, r) in state.regions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                // A region row is never empty: the list is FormIDs, and an
                // empty entry would write a null reference into it. So the
                // field is an `Option` only for the widget's NONE case, which
                // the row turns back into the first region.
                let mut chosen = Some(*r);
                record_field(ui, &format!("region_{i}"), "Region:", &regions, &mut chosen);
                if let Some(f) = chosen {
                    *r = f;
                }
                if ui.button("−").clicked() {
                    remove = Some(i);
                }
            });
        }
        if ui.button("+ Add region").clicked() {
            // The first region in the load order is the natural default for a
            // new row: the list is sorted by Editor ID, and a row has to open
            // on something.
            add = regions.first().map(|(f, _)| *f);
        }
        if let Some(i) = remove {
            state.regions.remove(i);
        }
        if let Some(f) = add {
            state.regions.push(f);
        }
        if state.regions.is_empty() {
            ui.weak("no regions — the cell belongs to none of them");
        }
    });
}

/// `XEZN` and `XCCM` — the cell's remaining references.
///
/// The acoustic space, the image space and the music type moved to Common
/// Data, where GECK puts them.
fn ownership_group(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    state: &mut CellEditorState,
) {
    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("References");
        ui.add_space(4.0);
        let eczn = session.records_of(*b"ECZN");
        record_field(
            ui,
            "encounter_zone",
            "Encounter zone:",
            &eczn,
            &mut state.encounter_zone,
        );
        let climate = session.records_of(*b"CLMT");
        record_field(ui, "climate", "Climate:", &climate, &mut state.climate);
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.strong("Ownership (XOWN / XRNK)");
        ui.add_space(4.0);
        // The owner can be any of four record types, so the dropdown is the
        // union of them rather than one signature's list.
        let mut owners: Vec<(u32, String)> = session.records_of(*b"FACT").to_vec();
        owners.extend(session.records_of(*b"NPC_").iter().cloned());
        owners.extend(session.records_of(*b"CREA").iter().cloned());
        owners.extend(session.records_of(*b"ACHR").iter().cloned());
        owners.sort_by_key(|a| a.1.to_lowercase());
        record_field(ui, "owner", "Owner:", &owners, &mut state.owner);
        if state.owner.is_some() {
            ui.horizontal(|ui| {
                ui.label("Faction rank:");
                ui.add(egui::DragValue::new(&mut state.owner_rank).range(-1..=255));
            });
            ui.weak("the rank applies when the owner is a FACT");
        }
    });
}

/// One optional record reference: a labelled row, a dropdown of that type's
/// records, and a hex field beside it for a FormID the list does not name.
///
/// The label is GECK's own name for the field — "Default Acoustic", "Music
/// type", "ImageSpace" — rather than the subrecord's signature: a row with no
/// text on it is a row the user has to guess at, and the signature is not what
/// they are looking for.
///
/// The hex field beside the dropdown is what keeps an out-of-range value
/// editable rather than silently clamped: a cell that points at a record from
/// a plugin the session has hidden is still the cell's real value.
fn record_field(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    items: &[(u32, String)],
    opt: &mut Option<u32>,
) {
    let cur = match opt {
        Some(fid) => items
            .iter()
            .find(|(id, _)| id == fid)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| format!("Custom {fid:08X}")),
        None => "NONE".to_string(),
    };
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(cur)
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(opt.is_none(), "NONE").clicked() {
                    *opt = None;
                }
                for (fid, name) in items {
                    if ui
                        .selectable_label(opt == &Some(*fid), format!("{name} ({fid:08X})"))
                        .clicked()
                    {
                        *opt = Some(*fid);
                    }
                }
            });
        // The hex field, so a FormID the list does not hold can still be typed.
        let mut text = opt.map(|f| format!("{f:08X}")).unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id_salt(format!("{id}_fid"))
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
    });
}
