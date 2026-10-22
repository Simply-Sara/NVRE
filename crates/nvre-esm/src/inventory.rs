//! What a record is called.
//!
//! A record's editor ID is the name the game and the mods use it by, and
//! its full name is the name the player sees.
use crate::raw::Record;
use nvre_util::cp1252;

/// The record's editor ID and its full name.
pub fn names(r: &Record) -> Result<(Option<String>, Option<String>), String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let mut edid = None;
    let mut full = None;
    for s in &subs {
        match &s.sig {
            b"EDID" if edid.is_none() => edid = Some(cp1252::decode_z(&s.data)),
            b"FULL" if full.is_none() => full = Some(cp1252::decode_z(&s.data)),
            _ => {}
        }
    }
    Ok((edid, full))
}
