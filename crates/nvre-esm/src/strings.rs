//! A localized plugin's string tables.
//!
//! A localized file — every shipped Skyrim and Fallout 4 master — holds its
//! translatable text not in the records but in three side files named after
//! the plugin (`Skyrim_English.STRINGS`, `.DLSTRINGS`, `.ILSTRINGS`). In such
//! a file a record's `FULL` is a 4-byte string ID into those tables rather
//! than text [documented: UESP "String Table File Format"; measured: every
//! `FULL` in Skyrim.esm is 4 bytes, and ID `0x12678` reads "Dremora Valkynaz"
//! out of `Skyrim_English.STRINGS`]. Reading those four bytes as text is what
//! once put `x&\x01` in the object window's Name column where a creature's
//! name belongs.
//!
//! The three tables of one plugin share a single ID space [measured: no ID
//! appears in more than one of Skyrim.esm's three files], so they are merged
//! into one map here and a lookup never asks which file an ID came from.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Which of a plugin's three table files a blob is.
///
/// The header and the ID/offset table are the same in all three; only the
/// entry's text differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    /// `.strings`: the short UI text — names, messages. An entry's text runs
    /// to the first NUL.
    Strings,
    /// `.dlstrings` and `.ilstrings`: book text and in-game messages. An
    /// entry is a 4-byte length followed by that many bytes of text, the
    /// length counting a trailing NUL [measured: every entry of
    /// `Skyrim_English.DLSTRINGS` and `.ILSTRINGS` parses this way].
    Long,
}

/// One plugin's strings, by string ID.
#[derive(Debug, Default, Clone)]
pub struct StringTable {
    map: HashMap<u32, String>,
}

impl StringTable {
    /// Parse one `.strings`/`.dlstrings`/`.ilstrings` file.
    ///
    /// The layout [documented: UESP "String Table File Format"; measured
    /// against all three of `Skyrim_English`]: a 4-byte entry count, a
    /// 4-byte byte count of the string data, then one `(id, offset)` pair per
    /// entry, then the string data itself. An offset is relative to the end
    /// of that pair table — to `8 + 8 * count`, not to the file's start.
    /// Anything truncated is an error rather than a partial table, so a
    /// mis-read is reported where it happens.
    pub fn parse(data: &[u8], kind: TableKind) -> Result<StringTable, String> {
        let word = |at: usize| -> Result<u32, String> {
            let bytes: [u8; 4] = data
                .get(at..at + 4)
                .ok_or_else(|| format!("a 4-byte word at {at} is past the file's end"))?
                .try_into()
                .expect("four bytes were just read");
            Ok(u32::from_le_bytes(bytes))
        };
        let count = word(0)? as usize;
        let table_end = count
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| format!("{count} entries overflow the table's size"))?;
        if table_end > data.len() {
            return Err(format!(
                "the header says {count} entries, which is {} bytes past the file's end",
                table_end - data.len()
            ));
        }
        let mut map = HashMap::with_capacity(count);
        for i in 0..count {
            let at = 8 + i * 8;
            let id = word(at)?;
            let offset = word(at + 4)? as usize;
            let start = table_end.checked_add(offset).ok_or_else(|| {
                format!("entry {i} (id {id:#x}) has an offset past the file's end")
            })?;
            let text = match kind {
                TableKind::Strings => {
                    let rest = data.get(start..).ok_or_else(|| {
                        format!("entry {i} (id {id:#x}) has an offset past the file's end")
                    })?;
                    let nul = rest.iter().position(|&b| b == 0).ok_or_else(|| {
                        format!("entry {i} (id {id:#x}) is not NUL-terminated: the file is cut")
                    })?;
                    &data[start..start + nul]
                }
                TableKind::Long => {
                    let len = word(start)? as usize;
                    let end = start
                        .checked_add(4)
                        .and_then(|n| n.checked_add(len))
                        .filter(|end| *end <= data.len())
                        .ok_or_else(|| {
                            format!("entry {i} (id {id:#x}) runs {len} bytes past the file's end")
                        })?;
                    let text = &data[start + 4..end];
                    // The length counts a trailing NUL; the text does not
                    // want it.
                    let trimmed = text
                        .iter()
                        .rposition(|&b| b != 0)
                        .map(|n| &text[..n + 1])
                        .unwrap_or_default();
                    trimmed
                }
            };
            map.insert(id, String::from_utf8_lossy(text).into_owned());
        }
        Ok(StringTable { map })
    }

    /// The text this ID names, when the table has it.
    pub fn get(&self, id: u32) -> Option<&str> {
        self.map.get(&id).map(String::as_str)
    }

    /// Add another table's entries, for the two tables whose IDs share one
    /// space.
    pub fn merge(&mut self, other: StringTable) {
        self.map.extend(other.map);
    }

    /// Whether the table holds no strings — a localized plugin whose table
    /// files are not installed.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The string ID a localized field holds, when its bytes are one: exactly
/// four bytes, read as a little-endian `u32`.
///
/// `None` for any other length, which is a field holding text rather than an
/// ID — a mis-declared schema or a file that is not localized after all.
pub fn string_id(data: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = data.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// The string tables of a load order's localized plugins, by plugin file
/// name (matched case-insensitively).
#[derive(Debug, Default)]
pub struct StringTables {
    by_plugin: HashMap<String, StringTable>,
}

impl StringTables {
    /// File a plugin's table under its name.
    pub fn insert(&mut self, plugin: &str, table: StringTable) {
        self.by_plugin.insert(plugin.to_ascii_lowercase(), table);
    }

    /// The table a record's localized fields resolve through.
    ///
    /// `None` when the plugin is not localized: its strings are text in the
    /// record itself. `Some` when it is — and an empty table when its table
    /// files are missing, so an unresolvable ID yields no name rather than
    /// four bytes of decoded noise.
    pub fn for_plugin(&self, plugin: &str, localized: bool) -> Option<&StringTable> {
        if !localized {
            return None;
        }
        match self.by_plugin.get(&plugin.to_ascii_lowercase()) {
            Some(table) => Some(table),
            None => Some(empty()),
        }
    }
}

/// The stand-in for a localized plugin whose tables are not installed.
fn empty() -> &'static StringTable {
    static EMPTY: OnceLock<StringTable> = OnceLock::new();
    EMPTY.get_or_init(StringTable::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.strings`-shaped file: header, ID/offset pairs, NUL-terminated
    /// entries.
    fn strings_file(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut table = Vec::new();
        let mut data = Vec::new();
        for &(id, text) in entries {
            table.extend_from_slice(&id.to_le_bytes());
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(text.as_bytes());
            data.push(0);
        }
        let mut out = (entries.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&data);
        out
    }

    /// The same file in the `.dlstrings` layout: each entry length-prefixed.
    fn long_file(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut table = Vec::new();
        let mut data = Vec::new();
        for &(id, text) in entries {
            let len = text.len() + 1; // the length counts the trailing NUL
            table.extend_from_slice(&id.to_le_bytes());
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(&(len as u32).to_le_bytes());
            data.extend_from_slice(text.as_bytes());
            data.push(0);
        }
        let mut out = (entries.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn a_strings_file_is_read_by_id() {
        let file = strings_file(&[(7, "Draugr"), (0x12678, "Dremora Valkynaz")]);
        let table = StringTable::parse(&file, TableKind::Strings).expect("parses");
        assert_eq!(table.get(7), Some("Draugr"));
        assert_eq!(table.get(0x12678), Some("Dremora Valkynaz"));
        assert_eq!(table.get(8), None);
    }

    #[test]
    fn a_dlstrings_file_is_read_by_id() {
        let file = long_file(&[(3, "A book's text."), (9, "")]);
        let table = StringTable::parse(&file, TableKind::Long).expect("parses");
        assert_eq!(table.get(3), Some("A book's text."));
        assert_eq!(table.get(9), Some(""));
    }

    #[test]
    fn the_offset_is_relative_to_the_table_not_the_file() {
        // Two entries, so a reader that took the offset from the file's start
        // would land inside the pair table rather than in the text.
        let file = strings_file(&[(1, "first"), (2, "second")]);
        let table = StringTable::parse(&file, TableKind::Strings).expect("parses");
        assert_eq!(table.get(1), Some("first"));
        assert_eq!(table.get(2), Some("second"));
    }

    #[test]
    fn a_truncated_file_is_an_error_rather_than_a_partial_table() {
        let mut file = strings_file(&[(1, "one"), (2, "two")]);
        file.truncate(file.len() - 3);
        assert!(
            StringTable::parse(&file, TableKind::Strings).is_err(),
            "a file cut inside its text is rejected"
        );
        let short = [0x40u8, 0, 0, 0, 0, 0, 0, 0];
        assert!(
            StringTable::parse(&short, TableKind::Strings).is_err(),
            "a header claiming 16 entries with no table is rejected"
        );
    }

    #[test]
    fn only_four_bytes_are_a_string_id() {
        assert_eq!(string_id(&[0x78, 0x26, 0x01, 0x00]), Some(0x12678));
        assert_eq!(string_id(&[0x41, 0x00]), None);
        assert_eq!(string_id(&[0x41, 0x00, 0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn an_uninstalled_table_yields_no_name_rather_than_noise() {
        let mut tables = StringTables::default();
        tables.insert("Skyrim.esm", StringTable::default());
        assert!(tables
            .for_plugin("SKYRIM.ESM", true)
            .expect("localized")
            .is_empty());
        assert!(tables
            .for_plugin("Skyrim.esm", true)
            .expect("localized")
            .get(0x12678)
            .is_none());
        assert!(tables.for_plugin("FalloutNV.esm", false).is_none());
    }
}
