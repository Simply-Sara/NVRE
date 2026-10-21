//! One plugin file: its header, its groups, and the masters it declares.

use crate::raw::{self, EsmError, Group, PluginHeader, Record};
use std::path::Path;

/// A plugin file, read.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// The file's `TES4` header.
    pub header: PluginHeader,
    /// The file's top-level groups.
    pub groups: Vec<Group>,
    /// The path it was read from.
    pub path: std::path::PathBuf,
}

impl Plugin {
    /// Read a plugin from memory.
    pub fn from_bytes(path: &Path, data: &[u8]) -> Result<Plugin, EsmError> {
        let (header, groups) = raw::parse(data)?;
        Ok(Plugin {
            header,
            groups,
            path: path.to_path_buf(),
        })
    }

    /// Read a plugin from a file.
    pub fn open(path: &Path) -> Result<Plugin, EsmError> {
        let data = std::fs::read(path)?;
        Plugin::from_bytes(path, &data)
    }

    /// Every record in the file, in order.
    pub fn records(&self) -> Vec<&Record> {
        let mut out = Vec::new();
        for g in &self.groups {
            collect_records(g, &mut out);
        }
        out
    }

    /// The file's masters: the FormIDs the header lists, in the order it
    /// lists them.
    pub fn masters(&self) -> Vec<u32> {
        let Some(data) = self.header.get("MAST") else {
            return Vec::new();
        };
        data.chunks(8)
            .filter(|c| c.len() == 8)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// The FormID the file's own header carries.
    ///
    /// It is the master's own FormID, which is what another plugin's `MAST`
    /// header names.
    pub fn header_form_id(&self) -> u32 {
        self.header.form_id
    }

    /// Whether the file is a master, rather than a plugin.
    pub fn is_master(&self) -> bool {
        self.header.flags & crate::raw::record_flags::MASTER != 0
    }
}

/// Collect a group's records, and its groups' records.
fn collect_records<'a>(g: &'a Group, out: &mut Vec<&'a Record>) {
    out.extend(g.records.iter());
    for sub in &g.groups {
        collect_records(sub, out);
    }
}
