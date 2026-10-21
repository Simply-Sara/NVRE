//! The load order: which plugin's definition of a FormID wins.
//!
//! A FormID stored in a plugin is **file-local**: its top byte indexes that
//! file's own master list — the `MAST` header, in the order the header
//! lists them — and the value equal to the master count means "this file
//! itself".
//!
//! At runtime the top byte is replaced by the load-order index of the file
//! it refers to, and a later file overrides an earlier one's definition of
//! the same global FormID.
//!
//! So there are two mappings, and they are different:
//!
//! - **file-local to global**: the plugin's own master list, then the load
//!   order.
//! - **global to the winning definition**: the last plugin in the load
//!   order that defines it.

use crate::plugin::Plugin;
use crate::raw::Record;
use std::collections::HashMap;

/// A fault in a load order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadOrderError {
    /// More plugins than the FormID's top byte can index.
    #[error("{0} plugins; at most 255 can be loaded")]
    TooManyPlugins(usize),
    /// A plugin names a master that is not in the load order.
    #[error("{plugin} requires master {master}, which is not in the load order")]
    MissingMaster {
        /// The plugin that needs the master.
        plugin: String,
        /// The master it needs.
        master: String,
    },
    /// A plugin names a master that loads after it.
    #[error("{plugin} requires master {master}, which loads after it")]
    MasterAfterDependent {
        /// The plugin that needs the master.
        plugin: String,
        /// The master it needs.
        master: String,
    },
    /// A plugin appears twice.
    #[error("{0} appears twice in the load order")]
    Duplicate(String),
}

/// Where one definition of a record is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefRef {
    /// The plugin's index in the load order.
    pub plugin: u8,
    /// The record's ordinal in that plugin.
    pub ordinal: u32,
}

/// The plugins, in load order, and the records they hold.
pub struct LoadOrder<'a> {
    /// The plugins, in load order.
    pub plugins: &'a [Plugin],
    /// For each plugin: its master-list index mapped to the load-order index.
    master_map: Vec<Vec<u8>>,
    /// Each plugin's records, in file order.
    pub records: Vec<Vec<&'a Record>>,
    /// Every global FormID, and every definition of it in load order. The
    /// last one wins.
    pub index: HashMap<u32, Vec<DefRef>>,
    /// The records whose own FormID could not be mapped.
    pub unmappable: Vec<(u8, u32)>,
}

impl<'a> LoadOrder<'a> {
    /// Build a load order from plugins, already in order.
    pub fn new(plugins: &'a [Plugin]) -> Result<LoadOrder<'a>, LoadOrderError> {
        if plugins.len() > 255 {
            return Err(LoadOrderError::TooManyPlugins(plugins.len()));
        }
        // Each plugin's master list: the FormIDs of the masters it names.
        let mut master_map = Vec::with_capacity(plugins.len());
        for (i, p) in plugins.iter().enumerate() {
            let mut map = Vec::new();
            for (j, m) in p.masters().iter().enumerate() {
                // A master is identified by its FormID, and the FormID's
                // local index is the master list's own index.
                let found = plugins
                    .iter()
                    .position(|q| q.header_form_id() == *m && !std::ptr::eq(q, p));
                match found {
                    Some(k) => map.push(k as u8),
                    None => {
                        // A master that is not in the load order: the plugin
                        // cannot be loaded without it.
                        return Err(LoadOrderError::MissingMaster {
                            plugin: p.path.display().to_string(),
                            master: format!("{m:08X}"),
                        });
                    }
                }
                let _ = (i, j);
            }
            master_map.push(map);
        }
        // Each plugin's records.
        let mut records = Vec::with_capacity(plugins.len());
        for p in plugins {
            records.push(p.records());
        }
        // The global index: every definition of every FormID, in load order.
        let mut index: HashMap<u32, Vec<DefRef>> = HashMap::new();
        let mut unmappable = Vec::new();
        for (pi, recs) in records.iter().enumerate() {
            for (ordinal, r) in recs.iter().enumerate() {
                match map_form_id(pi as u8, r.header.form_id, &master_map[pi]) {
                    Some(global) => {
                        index.entry(global).or_default().push(DefRef {
                            plugin: pi as u8,
                            ordinal: ordinal as u32,
                        });
                    }
                    None => unmappable.push((pi as u8, r.header.form_id)),
                }
            }
        }
        Ok(LoadOrder {
            plugins,
            master_map,
            records,
            index,
            unmappable,
        })
    }

    /// The global FormID of a record in a plugin.
    ///
    /// `None` when the record's local index names a master that is not in
    /// the load order.
    pub fn global_form_id(&self, plugin: u8, form_id: u32) -> Option<u32> {
        map_form_id(plugin, form_id, &self.master_map[plugin as usize])
    }

    /// The definition of a global FormID that wins: the last one in load
    /// order.
    pub fn winner(&self, global: u32) -> Option<DefRef> {
        self.index.get(&global).and_then(|v| v.last()).copied()
    }

    /// Every definition of a global FormID, in load order.
    pub fn definitions(&self, global: u32) -> &[DefRef] {
        self.index.get(&global).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Map a file-local FormID to a global one.
///
/// The top byte is the index into the plugin's own master list; the value
/// equal to the master count means the plugin itself.
fn map_form_id(plugin: u8, form_id: u32, master_map: &[u8]) -> Option<u32> {
    let local = (form_id >> 24) as usize;
    let lower = form_id & 0x00FF_FFFF;
    if local < master_map.len() {
        // A master: the load-order index replaces the local one.
        Some(((master_map[local] as u32) << 24) | lower)
    } else if local == master_map.len() {
        // The plugin itself.
        Some(((plugin as u32) << 24) | lower)
    } else {
        None
    }
}
