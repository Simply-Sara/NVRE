//! One plugin file: its header, its groups, and the masters it declares.

pub use crate::raw::MasterRef;
use crate::raw::{self, EsmError, Group, Node, PluginHeader, Record};
use std::path::Path;

/// What a plugin's `TES4` header says, read without parsing the plugin.
///
/// See [`Plugin::header_summary`]. The masters, the author and the description
/// are all subrecords of the header record, so one walk of its payload answers
/// all three.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderSummary {
    /// `MAST`: the masters the plugin depends on, in the header's order.
    pub masters: Vec<String>,
    /// `CNAM`: the plugin's author. `None` when the header has none, or has an
    /// empty one.
    pub author: Option<String>,
    /// `SNAM`: the plugin's description, under the same rule as `author`.
    pub description: Option<String>,
}

impl HeaderSummary {
    /// The masters, in the header's order.
    ///
    /// The same list [`Plugin::masters`] gives for a plugin read whole.
    pub fn masters(&self) -> &[String] {
        &self.masters
    }
}

/// A plugin file, read.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// The file's name.
    pub name: String,
    /// The file's `TES4` header.
    pub header: PluginHeader,
    /// The file's top-level groups.
    pub groups: Vec<Group>,
    /// The file's top level, as one list: the `TES4` header record and then the
    /// groups, in the order they are in the file.
    ///
    /// This is the writer's view. A plugin's top level is *not* flat — a
    /// top-level group holds records and sub-groups, and `raw::parse` reports
    /// that nesting as a flat list of every record and sub-group, which is what
    /// reading a record needs and what writing a file must not be handed. The
    /// groups themselves are therefore re-wrapped here, so `nodes[0]` is the
    /// header and `nodes[1..]` is the file's top level exactly as it was.
    pub nodes: Vec<Node>,
    /// The path it was read from.
    pub path: std::path::PathBuf,
}

impl Plugin {
    /// Read a plugin from memory.
    pub fn from_bytes(path: &Path, data: &[u8]) -> Result<Plugin, EsmError> {
        let (header, groups, header_record) = raw::parse(data)?;
        // The top level's contents, as one list: the header record first, then
        // the groups, each whole. `nodes` is what the writer walks, and a plugin
        // whose first node is not its own `TES4` is not a plugin — the game
        // reads the header before anything else.
        let mut nodes = Vec::with_capacity(groups.len() + 1);
        nodes.push(Node::Record(header_record));
        nodes.extend(groups.iter().cloned().map(Node::Group));
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
        self.header.masters.iter().map(|m| m.name.clone()).collect()
    }

    /// The masters a plugin file's header names, without reading the plugin.
    ///
    /// The same answer as [`Plugin::open`]'s `masters()`, for a file that is
    /// only being asked about its dependencies. `Plugin::open` reads the whole
    /// file and materializes every record and group in it, which answers this
    /// question for 10× the cost [measured over the FNV Data folder: all 19
    /// masters 80 ms this way, 619 ms through `open`; all 16 Fallout 4 masters
    /// 44 ms against 1646 ms]. That difference is visible wherever the question
    /// is asked inside a frame — the launcher's game pick does exactly that.
    ///
    /// `None` when the file cannot be read or does not start with a `TES4`
    /// header, so a caller can let an unreadable file constrain nothing rather
    /// than fail a whole folder for one bad plugin.
    pub fn masters_in_header(path: impl AsRef<Path>) -> Option<Vec<String>> {
        Self::header_summary(path).map(|h| h.masters)
    }

    /// What a plugin's `TES4` header says, without reading the plugin.
    ///
    /// The masters, the author and the description are all subrecords of the
    /// header record, so one walk of its payload answers all three. A caller
    /// that wants more than one of them — the launcher's file list, which shows
    /// the masters and the header's own text — should not read the file twice.
    ///
    /// `None` when the file cannot be read or does not start with a `TES4`
    /// header. The header may carry `COMPRESSED`; a plugin's `TES4` is not
    /// compressed in any shipped file, and the flag is honoured anyway because a
    /// header that says it is compressed and is then read as plain bytes would
    /// yield nonsense names.
    pub fn header_summary(path: impl AsRef<Path>) -> Option<HeaderSummary> {
        use std::io::{Read, Seek, SeekFrom};

        /// The record header: four bytes of signature, then twenty more.
        const HEADER: usize = 24;
        /// The flag saying the payload is a u32 size and then a zlib stream.
        const COMPRESSED: u32 = crate::raw::record_flags::COMPRESSED;

        let mut f = std::fs::File::open(path.as_ref()).ok()?;
        let mut header = [0u8; HEADER];
        f.read_exact(&mut header).ok()?;
        if &header[..4] != b"TES4" {
            return None;
        }
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let flags = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let mut payload = vec![0u8; size];
        f.seek(SeekFrom::Start(HEADER as u64)).ok()?;
        f.read_exact(&mut payload).ok()?;

        if flags & COMPRESSED != 0 {
            // A u32 decompressed size, then the stream.
            let size = u32::from_le_bytes(payload.get(..4)?.try_into().unwrap()) as usize;
            payload = nvre_inflate::zlib_decompress(&payload[4..], Some(size)).ok()?;
        }

        // A header string is a null-terminated name, in Windows-1252 like every
        // other name in these formats.
        let text = |data: &[u8]| {
            let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
            nvre_util::cp1252::decode(&data[..end])
        };

        let mut summary = HeaderSummary::default();
        let mut off = 0usize;
        while off + 6 <= payload.len() {
            let sig = &payload[off..off + 4];
            let len = u16::from_le_bytes(payload[off + 4..off + 6].try_into().unwrap()) as usize;
            let data = payload.get(off + 6..off + 6 + len)?;
            match sig {
                b"MAST" => summary.masters.push(text(data)),
                // `CNAM` is the author and `SNAM` the description [documented:
                // fopdoc's TES4 page]. An empty string is treated as absent: a
                // zero-length subrecord is what GECK writes for a field nobody
                // filled in, and showing "" is the same as showing nothing.
                b"CNAM" => summary.author = Some(text(data)).filter(|s| !s.is_empty()),
                b"SNAM" => summary.description = Some(text(data)).filter(|s| !s.is_empty()),
                _ => {}
            }
            off += 6 + len;
        }
        Some(summary)
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
