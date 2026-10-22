//! One plugin file: its header, its groups, and the masters it declares.

pub use crate::raw::MasterRef;
use crate::raw::{self, EsmError, Group, Node, PluginHeader, Record};
use std::path::Path;

/// A plugin file, read.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// The file's name.
    pub name: String,
    /// The file's `TES4` header.
    pub header: PluginHeader,
    /// The file's top-level groups.
    pub groups: Vec<Group>,
    /// The file's contents, as one list: the groups and the records at the
    /// top level, in the order they are in the file.
    pub nodes: Vec<Node>,
    /// The path it was read from.
    pub path: std::path::PathBuf,
}

impl Plugin {
    /// Read a plugin from memory.
    pub fn from_bytes(path: &Path, data: &[u8]) -> Result<Plugin, EsmError> {
        let (header, groups) = raw::parse(data)?;
        // The top level's contents, as one list.
        let mut nodes = Vec::new();
        for g in &groups {
            for c in &g.children {
                nodes.push(c.clone());
            }
        }
        Ok(Plugin {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            header,
            groups,
            nodes,
            path: path.to_path_buf(),
        })
    }

    /// Read a plugin from a file.
    pub fn open(path: impl AsRef<Path>) -> Result<Plugin, EsmError> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;
        Plugin::from_bytes(path, &data)
    }

    /// Read a plugin from a copy of its bytes.
    pub fn parse(data: &[u8], name: String) -> Result<Plugin, EsmError> {
        Plugin::from_bytes(Path::new(&name), data)
    }

    /// Read a plugin from bytes that are already shared.
    pub fn parse_bytes(data: bytes::Bytes, name: String) -> Result<Plugin, EsmError> {
        Plugin::from_bytes(Path::new(&name), &data)
    }

    /// Every record in the file, in order.
    pub fn records(&self) -> Vec<&Record> {
        let mut out = Vec::new();
        for g in &self.groups {
            collect_records(g, &mut out);
        }
        out
    }

    /// The file's masters: their names, in the order the header lists them.
    ///
    /// A `MAST` subrecord holds the master's file name.
    pub fn masters(&self) -> Vec<String> {
        self.header
            .masters
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    /// The FormID the file's own header carries.
    ///
    /// It is the master's own FormID, which is what another plugin's `MAST`
    /// header names.
    pub fn header_form_id(&self) -> u32 {
        self.header.form_id
    }

    /// The file's own index in its master list.
    ///
    /// A plugin's own records use the master index one past the last
    /// master it names, so the index is the number of masters.
    pub fn self_index(&self) -> u8 {
        self.header.masters.len().min(255) as u8
    }

    /// Whether the file is a master, rather than a plugin.
    pub fn is_master(&self) -> bool {
        self.header.flags & crate::raw::record_flags::MASTER != 0
    }
}

/// Collect a group's records, and its groups' records.
fn collect_records<'a>(g: &'a Group, out: &mut Vec<&'a Record>) {
    out.extend(g.records());
    for sub in g.groups() {
        collect_records(sub, out);
    }
}
