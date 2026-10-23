//! What a record is called.
//!
//! A record's editor ID is the name the game and the mods use it by, and
//! its full name is the name the player sees.
//!
//! In a localized file — every shipped Skyrim and Fallout 4 master — the
//! full name is not in the record at all: `FULL` holds a 4-byte string ID
//! into the plugin's string tables ([`crate::strings`]). Reading those four
//! bytes as text is what once spelled a creature's name `x&\x01`; a caller
//! whose plugin is localized passes its table, and the ID is resolved
//! through it instead.
use crate::raw::Record;
use crate::strings::{string_id, StringTable};
use nvre_util::cp1252;

/// The record's editor ID and its full name.
///
/// `strings` is the string table of the record's own plugin when that plugin
/// is localized, and `None` when it is not — see
/// [`crate::strings::StringTables::for_plugin`]. A localized `FULL` that the
/// table does not hold resolves to no name rather than to decoded ID bytes:
/// the lists this feeds fall back to the Editor ID, which every record has.
pub fn names(
    r: &Record,
    strings: Option<&StringTable>,
) -> Result<(Option<String>, Option<String>), String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let mut edid = None;
    let mut full = None;
    for s in &subs {
        match &s.sig {
            // EDID is a modder-facing identifier and is never localized, so
            // it is text even in a localized file.
            b"EDID" if edid.is_none() => edid = Some(cp1252::decode_z(&s.data)),
            b"FULL" if full.is_none() => full = full_name(&s.data, strings),
            _ => {}
        }
    }
    Ok((edid, full))
}

/// A record's full name from its `FULL` payload: through the string table
/// when the payload is a string ID, as text when it is text.
fn full_name(data: &[u8], strings: Option<&StringTable>) -> Option<String> {
    match strings {
        Some(table) => string_id(data)
            .and_then(|id| table.get(id))
            .map(str::to_string),
        None => Some(cp1252::decode_z(data)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{Record, RecordHeader};
    use crate::strings::{StringTable, TableKind};

    /// A record whose payload is an `EDID` and a `FULL` of `full`.
    fn record(full: &[u8]) -> Record {
        let mut raw = Vec::new();
        for (sig, data) in [
            (*b"EDID", b"EncDremoraWarlock06\0".as_slice()),
            (*b"FULL", full),
        ] {
            raw.extend_from_slice(&sig);
            raw.extend_from_slice(&(data.len() as u16).to_le_bytes());
            raw.extend_from_slice(data);
        }
        Record {
            header: RecordHeader {
                sig: *b"NPC_",
                data_size: raw.len() as u32,
                flags: 0,
                form_id: 0x16FFA,
                vc_info: 0,
                form_version: 0,
                vc_info2: 0,
            },
            subrecords: Vec::new(),
            raw,
            offset: 0,
        }
    }

    /// A table holding the one name these tests resolve: ID `0x12678` is
    /// "Dremora Valkynaz" in `Skyrim_English.STRINGS`.
    fn table() -> StringTable {
        // count 1, data size 19, then (id 0x12678, offset 0), then
        // "Dremora Valkynaz" NUL-terminated.
        let mut file = vec![1, 0, 0, 0, 19, 0, 0, 0, 0x78, 0x26, 0x01, 0x00, 0, 0, 0, 0];
        file.extend_from_slice(b"Dremora Valkynaz\0");
        StringTable::parse(&file, TableKind::Strings).expect("parses")
    }

    /// The same table with no entries: a localized plugin whose files are
    /// missing.
    fn empty_table() -> StringTable {
        StringTable::parse(&[0, 0, 0, 0, 0, 0, 0, 0], TableKind::Strings).expect("parses")
    }

    #[test]
    fn a_localized_full_is_read_through_the_table() {
        let r = record(&0x12678u32.to_le_bytes());
        let (edid, full) = names(&r, Some(&table())).expect("reads");
        assert_eq!(edid.as_deref(), Some("EncDremoraWarlock06"));
        assert_eq!(full.as_deref(), Some("Dremora Valkynaz"));
    }

    #[test]
    fn an_id_the_table_lacks_yields_no_name_rather_than_noise() {
        let r = record(&0x12678u32.to_le_bytes());
        let (_, full) = names(&r, Some(&empty_table())).expect("reads");
        assert_eq!(
            full, None,
            "an unresolvable ID is no name, not decoded bytes"
        );
    }

    #[test]
    fn an_unlocalized_full_is_still_text() {
        let r = record(b"Dremora Valkynaz\0");
        let (_, full) = names(&r, None).expect("reads");
        assert_eq!(full.as_deref(), Some("Dremora Valkynaz"));
    }

    #[test]
    fn four_bytes_without_a_table_are_still_decoded_as_text() {
        // The old behaviour, kept for unlocalized files: cp1252 to the NUL.
        let r = record(&[0x41, 0x42, 0x43, 0x00]);
        let (_, full) = names(&r, None).expect("reads");
        assert_eq!(full.as_deref(), Some("ABC"));
    }
}
