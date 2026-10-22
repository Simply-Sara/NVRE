//! A placed reference, and the base object it places.
//!
//! A **reference** is an object standing in the world: a rock, a crate, an
//! NPC. It has a position, a rotation, a scale, and a **base object** — the
//! record that says what it is.

use crate::fields::{formid_of, global, num, vec3, winner, Fields};
use fnv_esm::{LoadOrder, Schema};

/// The record header flags a reference carries.
pub mod ref_flags {
    /// The reference was deleted.
    pub const DELETED: u32 = 0x20;
    /// The reference is persistent: it is saved with the game.
    pub const PERSISTENT: u32 = 0x400;
    /// The reference starts disabled.
    pub const INITIALLY_DISABLED: u32 = 0x800;
    /// The reference is drawn even when it is far away.
    pub const VISIBLE_WHEN_DISTANT: u32 = 0x8000;
}

/// The "Imposter" bit of a reference's special rendering flags. The other
/// bits of the stored value vary from reference to reference with no
/// pattern, so only this one is named.
const IMPOSTER: u32 = 0x2;

/// Where a reference stands, and what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The reference's FormID.
    pub form_id: u32,
    /// The reference's record type.
    pub sig: [u8; 4],
    /// The record's flags.
    pub flags: u32,
    /// The base object: what the reference places.
    pub base: Option<u32>,
    /// Where it stands, in world units.
    pub position: [f32; 3],
    /// Its rotation, in radians, as stored.
    pub rotation: [f32; 3],
    /// Its scale.
    pub scale: f32,
    /// The reference whose enabled state this one follows, and whether it is
    /// the opposite of the parent's.
    pub enable_parent: Option<(u32, bool)>,
    /// The door opens by default.
    pub open_by_default: bool,
    /// The door leads elsewhere, rather than opening in place.
    pub teleport: bool,
    /// The region or light whose colour drives the reference's own light.
    pub emittance: Option<u32>,
    /// The reference is drawn as an imposter while the player is far away.
    pub imposter: bool,
}

impl Placement {
    /// Whether the reference was deleted.
    pub fn deleted(&self) -> bool {
        self.flags & ref_flags::DELETED != 0
    }

    /// Whether the reference starts disabled.
    pub fn initially_disabled(&self) -> bool {
        self.flags & ref_flags::INITIALLY_DISABLED != 0
    }
}

/// The winning definition of a placed reference.
pub fn placement(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Placement> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let base = f.formid(b"NAME", lo);
    let (position, rotation) = match f.get(b"DATA") {
        Some(d) => (
            d.get("position").and_then(vec3).unwrap_or_default(),
            d.get("rotation_radians").and_then(vec3).unwrap_or_default(),
        ),
        None => ([0.0; 3], [0.0; 3]),
    };
    let scale = f
        .get(b"XSCL")
        .and_then(|v| v.as_f64())
        .map_or(1.0, |v| v as f32);
    let enable_parent = f.get(b"XESP").and_then(|v| {
        let parent = global(lo, pi, formid_of(v.get("reference")?)?)?;
        let opposite = v
            .get("flags")
            .and_then(|f| num(f, "value"))
            .is_some_and(|b| b as u32 & 1 != 0);
        Some((parent, opposite))
    });
    let open_by_default = f.get(b"ONAM").is_some();
    let teleport = f.raw(b"XTEL").is_some();
    let emittance = f.formid(b"XEMI", lo).filter(|&e| e != 0);
    let imposter = f
        .get(b"XSRF")
        .and_then(|v| num(&v, "value"))
        .is_some_and(|b| b as u32 & IMPOSTER != 0);
    Some(Placement {
        form_id: fid,
        sig: r.header.sig,
        flags: r.header.flags,
        base,
        position,
        rotation,
        scale,
        enable_parent,
        open_by_default,
        teleport,
        emittance,
        imposter,
    })
}

/// What a base object is and what it looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseObject {
    /// The object's FormID.
    pub form_id: u32,
    /// The object's record type.
    pub sig: [u8; 4],
    /// The editor ID.
    pub edid: Option<String>,
    /// The model, as stored: relative to `meshes\`.
    pub model: Option<String>,
    /// The object's *world* model, for the record types whose `MODL` is
    /// something else: armour's `MODL` is the worn model and `MOD2` the one
    /// it shows on the ground; a weapon's `MODL` is the one in the hand and
    /// `MOD4` the one on the ground.
    pub world_model: Option<String>,
}

impl BaseObject {
    /// Archive path of MODL ([`archive_path`]).
    pub fn model_path(&self) -> Option<String> {
        archive_path(self.model.as_deref()?)
    }

    /// Archive path of the model the object shows (and collides with) in the
    /// world, placed or dropped: an item's world model when its record names
    /// one ([`world_model_sub`]), else MODL.
    pub fn world_model_path(&self) -> Option<String> {
        self.world_model
            .as_deref()
            .and_then(archive_path)
            .or_else(|| self.model_path())
    }

    /// Whether the model is a SpeedTree model.
    pub fn is_speedtree(&self) -> bool {
        self.model_path().is_some_and(|m| m.ends_with(".spt"))
    }
}

/// Archive path of a model stored relative to `meshes\`, lower case:
/// `meshes\...`, except SpeedTree models (`.spt`, used by TREE), which live
/// under `trees\` [observed: all ten `.spt` files in the archives are in
/// `trees\`, and TREE MODL values name them without a folder].
fn archive_path(stored: &str) -> Option<String> {
    let m = stored.trim();
    if m.is_empty() {
        return None;
    }
    // TREE models are stored with a leading separator, e.g.
    // "\WastelandShrub01.spt" [observed]; the file is
    // trees\wastelandshrub01.spt.
    let m = m.replace('/', "\\").to_ascii_lowercase();
    let m = m.trim_start_matches('\\');
    let root = if m.ends_with(".spt") { "trees\\" } else { "meshes\\" };
    Some(if m.starts_with(root) {
        m.to_string()
    } else {
        format!("{root}{m}")
    })
}

/// The subrecord naming the model an item shows lying in the world, for the
/// record types whose `MODL` is something else.
///
/// - Armour: `MODL` is the worn model, `MOD2` the "male world model", which
///   is usually a `...GO.NIF` ground object — armour on the ground shows
///   `MOD2`.
/// - A weapon: `MODL` is the model in the hand, `MOD4` the "world model".
///   Where `MOD4` differs from `MODL` it is a `...GO.NIF` or a
///   `...WorldObject.NIF`, and eight fist weapons have it and no `MODL`.
///
/// Every other item type has `MODL` only.
pub fn world_model_sub(sig: &[u8; 4]) -> Option<&'static [u8; 4]> {
    match sig {
        b"ARMO" => Some(b"MOD2"),
        b"WEAP" => Some(b"MOD4"),
        _ => None,
    }
}

/// The winning definition of a base object.
pub fn base_object(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<BaseObject> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let edid = f.string(b"EDID");
    let model = f.string(b"MODL");
    let world_model = world_model_sub(&r.header.sig).and_then(|s| f.string(s));
    Some(BaseObject {
        form_id: fid,
        sig: r.header.sig,
        edid,
        model,
        world_model,
    })
}
