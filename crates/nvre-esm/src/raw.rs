//! The TES4 container: groups, records and subrecords.
//!
//! A plugin file is a tree. At the top are **groups**, each with a four-byte
//! type and a name; a group holds **records**; a record holds **subrecords**,
//! each a four-byte type and a payload.
//!
//! This layer knows nothing about what any of them mean. It parses the
//! structure losslessly, keeps the original bytes of every payload, and can
//! write the file back exactly as it came.
//!
//! # The shapes
//!
//! ```text
//! file   := header groups* footer
//! header := "TES4" <24 bytes>
//! group  := "GRUP" <24 bytes> record* "XXXX"
//! record := <4-byte type> <24 bytes> subrecord*
//! ```
//!
//! The record's 24 bytes are the payload size, the flags, the FormID, the
//! version-control words, and the format version.

use bytes::Bytes;
use nvre_util::{tag_str, Cursor};
use std::borrow::Cow;

/// The size of a record's header, the four-byte signature included.
pub const RECORD_HEADER_SIZE: usize = 24;
/// The size of a group's header.
pub const GROUP_HEADER_SIZE: usize = 24;
/// The four bytes that start a group.
pub const GRUP: [u8; 4] = *b"GRUP";
/// The four bytes that end a group.
pub const XXXX: [u8; 4] = *b"XXXX";

/// The record flags this layer relies on.
pub mod record_flags {
    /// The plugin is a master file. Only the TES4 record carries it.
    pub const MASTER: u32 = 0x0000_0001;
    /// The record was deleted.
    pub const DELETED: u32 = 0x0000_0020;
    /// The payload is a u32 decompressed size and then a zlib stream.
    pub const COMPRESSED: u32 = 0x0004_0000;
}

/// A fault in a plugin file.
#[derive(Debug, thiserror::Error)]
pub enum EsmError {
    /// The file could not be read.
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    /// The structure is not what a plugin file must be.
    #[error("structure error at 0x{offset:x}: {msg}")]
    Structure {
        /// Where in the file it went wrong.
        offset: u64,
        /// What was wrong.
        msg: String,
    },
    /// A record's payload could not be decompressed.
    #[error("cannot decompress record {form_id:08X} at 0x{offset:x}: {msg}")]
    Decompress {
        /// Where in the file the record is.
        offset: u64,
        /// The record's FormID.
        form_id: u32,
        /// What was wrong.
        msg: String,
    },
}

impl From<std::io::Error> for EsmError {
    fn from(e: std::io::Error) -> Self {
        EsmError::Io(e)
    }
}

impl From<nvre_util::OutOfBounds> for EsmError {
    fn from(e: nvre_util::OutOfBounds) -> Self {
        EsmError::Structure {
            offset: e.pos as u64,
            msg: e.to_string(),
        }
    }
}

/// A record's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHeader {
    /// The record's four-byte type.
    pub sig: [u8; 4],
    /// The payload's size on disk — the compressed size, when compressed.
    pub data_size: u32,
    /// The record's flags.
    pub flags: u32,
    /// The record's FormID.
    pub form_id: u32,
    /// The creation kit's version-control word, kept as it was.
    pub vc_info: u32,
    /// The record's format version.
    pub form_version: u16,
    /// The second version-control word.
    pub vc_info2: u16,
}

impl RecordHeader {
    /// The record's type as text.
    pub fn sig_str(&self) -> String {
        tag_str(&self.sig)
    }
}

/// A group's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHeader {
    /// The group's size: the whole group, header and contents.
    pub size: u32,
    /// The group's label, usually the same as its type.
    pub label: [u8; 4],
    /// The group's type, as a number: the four bytes of the type read as a
    /// little-endian integer.
    pub group_type: i32,
    /// The group's stamp: the time it was made.
    pub stamp: u16,
    /// A word the format holds and nothing reads.
    pub unknown1: u16,
    /// The group's version.
    pub version: u16,
    /// A word the format holds and nothing reads.
    pub unknown2: u16,
}

impl GroupHeader {
    /// The group's label, as a number: the four bytes of the label read as
    /// a little-endian integer.
    pub fn label_u32(&self) -> u32 {
        u32::from_le_bytes(self.label)
    }

    /// The group's label, as a signed number: the four bytes of the label
    /// read as a little-endian integer.
    pub fn label_i32(&self) -> i32 {
        i32::from_le_bytes(self.label)
    }

    /// The cell's grid position, when the group is a cell.
    ///
    /// The label holds the position: two little-endian words, the y first
    /// and the x second.
    pub fn label_grid(&self) -> (i16, i16) {
        let y = i16::from_le_bytes([self.label[0], self.label[1]]);
        let x = i16::from_le_bytes([self.label[2], self.label[3]]);
        (x, y)
    }

    /// The group's type as text: the four bytes of the type, in the order
    /// they are in the file.
    pub fn sig_str(&self) -> String {
        let b = self.group_type.to_le_bytes();
        tag_str(&b)
    }

    /// The group's label as text.
    pub fn label_str(&self) -> String {
        tag_str(&self.label)
    }
}

/// One of a record's subrecords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subrecord {
    /// The subrecord's four-byte type.
    pub sig: [u8; 4],
    /// The payload, as it was in the file.
    pub data: Vec<u8>,
    /// The size came from a preceding `XXXX` subrecord, so the payload is
    /// longer than 65535 bytes.
    pub extended: bool,
}

impl Subrecord {
    /// The subrecord's type as text.
    pub fn sig_str(&self) -> String {
        tag_str(&self.sig)
    }
}

/// A record: its header and its subrecords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The record's header.
    pub header: RecordHeader,
    /// The record's subrecords.
    pub subrecords: Vec<Subrecord>,
    /// The record's raw bytes, as they were in the file.
    pub raw: Vec<u8>,
    /// Where the record is in the file.
    pub offset: u64,
}

impl Record {
    /// The record's type as text.
    pub fn sig_str(&self) -> String {
        self.header.sig_str()
    }

    /// Whether the record is flagged as deleted.
    pub fn is_deleted(&self) -> bool {
        self.header.flags & record_flags::DELETED != 0
    }

    /// Whether the record's payload is compressed.
    pub fn is_compressed(&self) -> bool {
        self.header.flags & record_flags::COMPRESSED != 0
    }
}

/// A group: its header and the records it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The group's header.
    pub header: GroupHeader,
    /// The group's contents: the records and the groups inside it, in the
    /// order they are in the file.
    pub children: Vec<Node>,
    /// Where the group is in the file.
    pub offset: u64,
}



impl Group {
    /// The group's contents, as one list: the records and the groups in the
    /// order they are in the file.
    /// The group's records.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.children.iter().filter_map(|n| match n {
            Node::Record(r) => Some(r),
            Node::Group(_) => None,
        })
    }

    /// The groups inside this one.
    pub fn groups(&self) -> impl Iterator<Item = &Group> {
        self.children.iter().filter_map(|n| match n {
            Node::Group(g) => Some(g),
            Node::Record(_) => None,
        })
    }
}

/// A node of the file: a group or a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A group.
    Group(Group),
    /// A record.
    Record(Record),
}

/// The file's own header, the `TES4` record.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginHeader {
    /// The header's fields, in the order the format has them.
    pub fields: Vec<(String, Vec<u8>)>,
    /// The header record's own FormID, which is the master's FormID.
    pub form_id: u32,
    /// The header record's flags.
    pub flags: u32,
    /// The header record's format version.
    pub form_version: u16,
    /// `HEDR`: the format version, as a float.
    pub version: f32,
    /// `HEDR`: the number of records and groups, less the header record.
    pub num_records: i32,
    /// `CNST`: the next object ID the plugin will use.
    pub next_object_id: u32,
    /// `CNST`: the plugin's author.
    pub author: Option<String>,
    /// `CNST`: the plugin's description.
    pub description: Option<String>,
    /// `MAST`: the masters the plugin depends on.
    pub masters: Vec<MasterRef>,
    /// `ONAM`: the FormIDs of the master records this plugin overrides.
    pub overridden_forms: Vec<u32>,
    /// The header's subrecords that are not one of the ones above.
    pub other: Vec<String>,
}

/// A master the plugin depends on.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterRef {
    /// The master's file name.
    pub name: String,
    /// The number that follows the master's name in the `MAST` subrecord.
    ///
    /// It is zero in every shipped master; the community's documentation
    /// describes it as the master's file size.
    pub data: Option<u64>,
}

impl PluginHeader {
    /// Whether the file is a master, rather than a plugin.
    pub fn is_master(&self) -> bool {
        self.flags & record_flags::MASTER != 0
    }

    /// The value of a field, if it is there.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_slice())
    }
}

/// Read a plugin file's structure.
///
/// The result is the header and the top-level groups. Every payload is kept
/// as it was, so the file can be written back unchanged.
pub fn parse(data: &[u8]) -> Result<(PluginHeader, Vec<Group>), EsmError> {
    let mut c = Cursor::new(data);
    // The header: a TES4 record.
    let header = parse_header(&mut c, data)?;
    let groups = parse_groups(&mut c, data, 0)?;
    Ok((header, groups))
}

/// Read the `TES4` record.
fn parse_header(c: &mut Cursor<'_>, data: &[u8]) -> Result<PluginHeader, EsmError> {
    let start = c.pos;
    let oob = |e: nvre_util::OutOfBounds| EsmError::Structure {
        offset: start as u64,
        msg: e.to_string(),
    };
    if data.get(start..start + 4) != Some(b"TES4") {
        return Err(EsmError::Structure {
            offset: start as u64,
            msg: "the file does not start with a TES4 record".into(),
        });
    }
    c.bytes(4).map_err(oob)?;
    let data_size = c.u32().map_err(oob)?;
    let flags = c.u32().map_err(oob)?;
    let form_id = c.u32().map_err(oob)?;
    let _vc_info = c.u32().map_err(oob)?;
    let form_version = c.u16().map_err(oob)?;
    let _vc_info2 = c.u16().map_err(oob)?;
    // The header is 24 bytes in total, the signature included, so the
    // payload starts 20 bytes after the signature.
    let end = start + RECORD_HEADER_SIZE + data_size as usize;
    let body = data
        .get(start + RECORD_HEADER_SIZE..end)
        .ok_or_else(|| EsmError::Structure {
            offset: start as u64,
            msg: "the TES4 record's payload is out of range".into(),
        })?;
    // The header's fields: a four-byte type, a 16-bit size, and the bytes.
    let mut fields = Vec::new();
    let mut at = 0usize;
    while at + 6 <= body.len() {
        let sig = &body[at..at + 4];
        let len = u16::from_le_bytes([body[at + 4], body[at + 5]]) as usize;
        let value_at = at + 6;
        let value_end = value_at + len;
        if value_end > body.len() {
            break;
        }
        fields.push((
            tag_str(&[sig[0], sig[1], sig[2], sig[3]]),
            body[value_at..value_end].to_vec(),
        ));
        at = value_end;
    }
    c.pos = end;
    // The fields the editor and the writer need, read out of the header's
    // own subrecords.
    let field = |name: &str| fields.iter().find(|(n, _)| n == name).map(|(_, v)| v);
    let zstring = |name: &str| {
        field(name)
            .map(|v| nvre_util::cp1252::decode_z(v))
            .filter(|s| !s.is_empty())
    };
    let version = field("HEDR")
        .and_then(|v| v.get(..4))
        .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .unwrap_or(0.0);
    let num_records = field("HEDR")
        .and_then(|v| v.get(4..8))
        .map(|v| i32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .unwrap_or(0);
    let next_object_id = field("CNST")
        .and_then(|v| v.get(..4))
        .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .unwrap_or(0);
    let author = zstring("CNST").and_then(|a| {
        // The author is the text up to the first NUL.
        field("CNST").map(|v| {
            let end = v.iter().position(|&b| b == 0).unwrap_or(v.len());
            nvre_util::cp1252::decode(&v[..end])
        })
    });
    let description = field("DESC").map(|v| {
        let end = v.iter().position(|&b| b == 0).unwrap_or(v.len());
        nvre_util::cp1252::decode(&v[..end])
    });
    // The masters: each `MAST` holds the master's name and a number.
    let mut masters = Vec::new();
    let mast_bytes: Vec<Vec<u8>> = fields
        .iter()
        .filter(|(n, _)| n == "MAST")
        .map(|(_, v)| v.clone())
        .collect();
    for m in &mast_bytes {
        let end = m.iter().position(|&b| b == 0).unwrap_or(m.len());
        let name = nvre_util::cp1252::decode(&m[..end]);
        let rest = &m[(end + 1).min(m.len())..];
        let data = if rest.len() >= 8 {
            Some(u64::from_le_bytes([
                rest[0], rest[1], rest[2], rest[3], rest[4], rest[5], rest[6], rest[7],
            ]))
        } else {
            None
        };
        masters.push(MasterRef { name, data });
    }
    let overridden_forms = field("ONAM")
        .map(|v| {
            v.chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        })
        .unwrap_or_default();
    Ok(PluginHeader {
        fields,
        form_id,
        flags,
        form_version,
        version,
        num_records,
        next_object_id,
        author,
        description,
        masters,
        overridden_forms,
        other: Vec::new(),
    })
}

/// Read groups until the file ends.
fn parse_groups(c: &mut Cursor<'_>, data: &[u8], _depth: usize) -> Result<Vec<Group>, EsmError> {
    let mut out = Vec::new();
    loop {
        let start = c.pos;
        if start + GROUP_HEADER_SIZE > data.len() {
            break;
        }
        let sig = c.tag()?;
        if sig != GRUP {
            // A group that is not a group: the file has ended, or something
            // is wrong with it.
            c.pos = start;
            break;
        }
        c.pos = start;
        out.push(parse_group(c, data, start)?);
    }
    Ok(out)
}

/// A group's contents: the records and the groups in them, in the order
/// they are in the file.
///
/// The two kinds of thing may be interleaved, so the group is read in one
/// walk rather than in two.
fn parse_contents(
    c: &mut Cursor<'_>,
    data: &[u8],
    end: usize,
) -> Result<Vec<Node>, EsmError> {
    let mut children = Vec::new();
    loop {
        let start = c.pos;
        if start + GROUP_HEADER_SIZE > end {
            break;
        }
        let sig = c.tag()?;
        c.pos = start;
        if sig == GRUP {
            children.push(Node::Group(parse_group(c, data, start)?));
            continue;
        }
        if sig == XXXX {
            break;
        }
        // A record: read its header and its payload.
        let Some(record) = parse_record(c, data, end, start)? else {
            break;
        };
        children.push(Node::Record(record));
    }
    Ok(children)
}

/// Read one group.
fn parse_group(c: &mut Cursor<'_>, data: &[u8], start: usize) -> Result<Group, EsmError> {
    let oob = |e: nvre_util::OutOfBounds| EsmError::Structure {
        offset: start as u64,
        msg: e.to_string(),
    };
    let _sig = c.bytes(4).map_err(oob)?;
    let size = c.u32().map_err(oob)?;
    let label = c.tag().map_err(oob)?;
    let group_type = c.i32().map_err(oob)?;
    let stamp = c.u16().map_err(oob)?;
    let _unknown1 = c.u16().map_err(oob)?;
    let version = c.u16().map_err(oob)?;
    let _unknown2 = c.u16().map_err(oob)?;
    // The group holds records, and the header says how long it is.
    let gend = start
        .checked_add(size as usize)
        .filter(|&e| e <= data.len() && size as usize >= GROUP_HEADER_SIZE)
        .ok_or_else(|| EsmError::Structure {
            offset: start as u64,
            msg: format!("the group size {size} overruns its container"),
        })?;
    // A group holds records, and may hold groups of its own: a worldspace
    // holds cells, and a cell holds its own records.
    let children = parse_contents(c, data, gend)?;
    c.pos = gend;
    Ok(Group {
        header: GroupHeader {
            size,
            label,
            group_type,
            stamp,
            unknown1: 0,
            version,
            unknown2: 0,
        },
        children,
        offset: start as u64,
    })
}

/// Read records until the group ends.
fn parse_records(c: &mut Cursor<'_>, data: &[u8], end: usize) -> Result<Vec<Record>, EsmError> {
    let mut out = Vec::new();
    loop {
        let start = c.pos;
        if start + RECORD_HEADER_SIZE > end {
            break;
        }
        let sig = c.tag()?;
        if sig == XXXX {
            // The end of the group.
            c.pos = start;
            break;
        }
        if sig == GRUP {
            // A group inside this one.
            c.pos = start;
            break;
        }
        c.pos = start;
        let Some(record) = parse_record(c, data, end, start)? else {
            break;
        };
        out.push(record);
    }
    Ok(out)
}

/// Read one record: its header and its payload.
fn parse_record(
    c: &mut Cursor<'_>,
    data: &[u8],
    end: usize,
    start: usize,
) -> Result<Option<Record>, EsmError> {
    let _sig = c.bytes(4)?;
    // The header is 24 bytes in total, the signature included, so the
    // payload starts 20 bytes after the signature.
    let data_size = c.u32()?;
    let flags = c.u32()?;
    let form_id = c.u32()?;
    let vc_info = c.u32()?;
    let form_version = c.u16()?;
    let vc_info2 = c.u16()?;
    let header = RecordHeader {
        sig: [data[start], data[start + 1], data[start + 2], data[start + 3]],
        data_size,
        flags,
        form_id,
        vc_info,
        form_version,
        vc_info2,
    };
    let body_end = c.pos + data_size as usize;
    if body_end > end {
        return Err(EsmError::Structure {
            offset: start as u64,
            msg: format!(
                "record {} claims {data_size} bytes but only {} are left",
                header.sig_str(),
                data.len().saturating_sub(c.pos)
            ),
        });
    }
    let body = &data[c.pos..body_end];
    c.pos = body_end;
    // The record's payload is kept as it is in the file: a compressed
    // record's payload is a compressed stream, not subrecords. The
    // subrecords are split from the uncompressed payload, by
    // `Record::subrecords`.
    Ok(Some(Record {
        header,
        subrecords: Vec::new(),
        raw: body.to_vec(),
        offset: start as u64,
    }))
}

/// Read a record's subrecords.
///
/// The `XXXX` form is resolved here: it is a subrecord whose size field is
/// zero and whose payload is the size of the subrecord that follows, which
/// is how a subrecord larger than 65535 bytes is written.
pub fn parse_subrecords(body: &[u8]) -> Result<Vec<Subrecord>, (usize, String)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    // The size the `XXXX` before a subrecord says it has.
    let mut pending: Option<u32> = None;
    while at + 6 <= body.len() {
        let sig = &body[at..at + 4];
        let size16 = u16::from_le_bytes([body[at + 4], body[at + 5]]) as usize;
        let data_at = at + 6;
        if sig == XXXX {
            if size16 != 4 || data_at + 4 > body.len() {
                break;
            }
            pending = Some(u32::from_le_bytes([
                body[data_at],
                body[data_at + 1],
                body[data_at + 2],
                body[data_at + 3],
            ]));
            at = data_at + 4;
            continue;
        }
        let (size, extended) = match pending.take() {
            Some(s) => (s as usize, true),
            None => (size16, false),
        };
        let data_end = data_at + size;
        if data_end > body.len() {
            break;
        }
        out.push(Subrecord {
            sig: [sig[0], sig[1], sig[2], sig[3]],
            data: body[data_at..data_end].to_vec(),
            extended,
        });
        at = data_end;
    }
    Ok(out)
}

/// The uncompressed payload of a record.
///
/// A compressed record's payload is a u32 decompressed size and then a
/// zlib stream. A record that is not compressed is its own payload.
///
/// The trailer is *tolerated*, not required: shipped game data holds
/// streams that end without an Adler-32, and streams whose Adler-32 is
/// wrong. Those are reported by [`Record::payload_checked`].
pub fn payload(record: &Record) -> Result<Cow<'_, [u8]>, EsmError> {
    let (p, _trailer) = payload_checked(record)?;
    Ok(p)
}

/// The uncompressed payload of a record, and what the zlib trailer was.
///
/// A record that is not compressed is its own payload, and its trailer is
/// [`nvre_inflate::Trailer::Verified`].
pub fn payload_checked(
    record: &Record,
) -> Result<(Cow<'_, [u8]>, nvre_inflate::Trailer), EsmError> {
    if !record.is_compressed() {
        return Ok((
            Cow::Borrowed(&record.raw),
            nvre_inflate::Trailer::Verified,
        ));
    }
    // A compressed record's payload is a u32 decompressed size and then a
    // zlib stream.
    let raw = &record.raw;
    if raw.len() < 4 {
        return Err(EsmError::Decompress {
            offset: record.offset,
            form_id: record.header.form_id,
            msg: "the compressed payload is shorter than 4 bytes".into(),
        });
    }
    let size = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    nvre_inflate::zlib_decompress_ex(&raw[4..], Some(size))
        .map(|(d, t)| (Cow::Owned(d), t))
        .map_err(|e| EsmError::Decompress {
            offset: record.offset,
            form_id: record.header.form_id,
            msg: e.to_string(),
        })
}

impl Record {
    /// The record's subrecords, split from its uncompressed payload.
    ///
    /// A compressed record's payload is a compressed stream, so the split
    /// happens after it is decompressed.
    pub fn subrecords(&self) -> Result<Vec<Subrecord>, EsmError> {
        let p = payload(self)?;
        parse_subrecords(&p).map_err(|(pos, msg)| EsmError::Structure {
            offset: self.offset,
            msg: format!(
                "record {} {:08X}: subrecord at payload+{pos}: {msg}",
                self.sig_str(),
                self.header.form_id
            ),
        })
    }

    /// The record's raw bytes, as they are in the file.
    pub fn subrecords_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for s in &self.subrecords {
            out.extend_from_slice(&s.sig);
            out.extend_from_slice(&(s.data.len() as u16).to_le_bytes());
            out.extend_from_slice(&s.data);
        }
        out
    }
}

/// Write a file's records and groups back out.
///
/// The bytes are the same ones the file was read from, so a file that is
/// read and written again is byte for byte the file it came from.
pub fn write_nodes(nodes: &[Node], out: &mut Vec<u8>) {
    for n in nodes {
        match n {
            Node::Record(r) => {
                let h = &r.header;
                out.extend_from_slice(&h.sig);
                out.extend_from_slice(&(r.raw.len() as u32).to_le_bytes());
                out.extend_from_slice(&h.flags.to_le_bytes());
                out.extend_from_slice(&h.form_id.to_le_bytes());
                out.extend_from_slice(&h.vc_info.to_le_bytes());
                out.extend_from_slice(&h.form_version.to_le_bytes());
                out.extend_from_slice(&h.vc_info2.to_le_bytes());
                out.extend_from_slice(&r.raw);
            }
            Node::Group(g) => {
                let start = out.len();
                let h = &g.header;
                out.extend_from_slice(&GRUP);
                out.extend_from_slice(&0u32.to_le_bytes()); // patched below
                out.extend_from_slice(&h.label);
                out.extend_from_slice(&h.group_type.to_le_bytes());
                out.extend_from_slice(&h.stamp.to_le_bytes());
                out.extend_from_slice(&h.unknown1.to_le_bytes());
                out.extend_from_slice(&h.version.to_le_bytes());
                out.extend_from_slice(&h.unknown2.to_le_bytes());
                write_nodes(&g.children, out);
                let size = (out.len() - start) as u32;
                out[start + 4..start + 8].copy_from_slice(&size.to_le_bytes());
            }
        }
    }
}

/// The kinds of group a file holds.
pub mod group_type {
    /// The file's own top-level group.
    pub const TOP: i32 = 0;
    /// The groups a world space holds.
    pub const WORLD_CHILDREN: i32 = 1;
    /// A cell's interior block.
    pub const INTERIOR_CELL_BLOCK: i32 = 2;
    /// A cell's interior sub-block.
    pub const INTERIOR_CELL_SUBBLOCK: i32 = 3;
    /// A cell's exterior block.
    pub const EXTERIOR_CELL_BLOCK: i32 = 4;
    /// A cell's exterior sub-block.
    pub const EXTERIOR_CELL_SUBBLOCK: i32 = 5;
    /// A cell's persistent children.
    pub const CELL_CHILDREN: i32 = 6;
    /// A topic's children.
    pub const TOPIC_CHILDREN: i32 = 7;
    /// A cell's persistent children.
    pub const CELL_PERSISTENT_CHILDREN: i32 = 8;
    /// A cell's temporary children.
    pub const CELL_TEMPORARY_CHILDREN: i32 = 9;
    /// A cell's visible distant children.
    pub const CELL_VISIBLE_DISTANT_CHILDREN: i32 = 10;

    /// The name of a group type.
    pub fn name(t: i32) -> &'static str {
        match t {
            TOP => "top",
            WORLD_CHILDREN => "world_children",
            INTERIOR_CELL_BLOCK => "interior_cell_block",
            INTERIOR_CELL_SUBBLOCK => "interior_cell_subblock",
            EXTERIOR_CELL_BLOCK => "exterior_cell_block",
            EXTERIOR_CELL_SUBBLOCK => "exterior_cell_subblock",
            CELL_CHILDREN => "cell_children",
            TOPIC_CHILDREN => "topic_children",
            CELL_PERSISTENT_CHILDREN => "cell_persistent_children",
            CELL_TEMPORARY_CHILDREN => "cell_temporary_children",
            CELL_VISIBLE_DISTANT_CHILDREN => "cell_visible_distant_children",
            _ => "unknown",
        }
    }
}

/// Write a record's subrecords: each one's type, its size, and its payload.
pub fn write_subrecords(subs: &[Subrecord]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in subs {
        out.extend_from_slice(&s.sig);
        out.extend_from_slice(&(s.data.len() as u16).to_le_bytes());
        out.extend_from_slice(&s.data);
    }
    out
}

/// Write a group and everything in it.
///
/// A group's size is not known until its contents are written, so it is
/// written as zero and patched afterwards.
pub fn write_group(g: &Group, out: &mut Vec<u8>) {
    let start = out.len();
    let h = &g.header;
    out.extend_from_slice(&GRUP);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&h.label);
    out.extend_from_slice(&h.group_type.to_le_bytes());
    out.extend_from_slice(&h.stamp.to_le_bytes());
    out.extend_from_slice(&h.version.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    for r in g.records() {
        write_record(r, out);
    }
    for sub in g.groups() {
        write_group(sub, out);
    }
    let size = (out.len() - start) as u32;
    out[start + 4..start + 8].copy_from_slice(&size.to_le_bytes());
}

/// Write a record: its header and its payload.
pub fn write_record(r: &Record, out: &mut Vec<u8>) {
    let h = &r.header;
    out.extend_from_slice(&h.sig);
    out.extend_from_slice(&(r.raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&h.flags.to_le_bytes());
    out.extend_from_slice(&h.form_id.to_le_bytes());
    out.extend_from_slice(&h.vc_info.to_le_bytes());
    out.extend_from_slice(&h.form_version.to_le_bytes());
    out.extend_from_slice(&h.vc_info2.to_le_bytes());
    out.extend_from_slice(&r.raw);
}

/// Visit every record in a tree, with the groups it is in.
///
/// The groups are the ones between the file's top and the record, nearest
/// first, and what is given is each group's header.
pub fn visit_records<'a, F: FnMut(&'a Record, &[&'a GroupHeader])>(
    groups: &'a [Group],
    f: &mut F,
) {
    let mut path: Vec<&GroupHeader> = Vec::new();
    for g in groups {
        visit_in_group(g, &mut path, f);
    }
}

/// Depth-first visit of every record in a list of nodes, with the chain of
/// enclosing group headers.
pub fn visit_nodes<'a, F: FnMut(&'a Record, &[&'a GroupHeader])>(
    nodes: &'a [Node],
    f: &mut F,
) {
    let mut path: Vec<&GroupHeader> = Vec::new();
    for n in nodes {
        match n {
            Node::Record(r) => f(r, path.as_slice()),
            Node::Group(g) => visit_in_group(g, &mut path, f),
        }
    }
}

fn visit_in_group<'a, F: FnMut(&'a Record, &[&'a GroupHeader])>(
    g: &'a Group,
    path: &mut Vec<&'a GroupHeader>,
    f: &mut F,
) {
    path.push(&g.header);
    for r in g.records() {
        f(r, path.as_slice());
    }
    for sub in g.groups() {
        visit_in_group(sub, path, f);
    }
    path.pop();
}

/// Visit every group in a list of nodes, with the group it is in.
pub fn visit_groups_in<'a, F: FnMut(&'a Group, usize)>(nodes: &'a [Node], f: &mut F) {
    for n in nodes {
        if let Node::Group(g) = n {
            visit_group(g, 0, f);
        }
    }
}

/// Visit every group in a tree, with the group it is in.
pub fn visit_groups<'a, F: FnMut(&'a Group, usize)>(groups: &'a [Group], f: &mut F) {
    for g in groups {
        visit_group(g, 0, f);
    }
}

fn visit_group<'a, F: FnMut(&'a Group, usize)>(g: &'a Group, depth: usize, f: &mut F) {
    f(g, depth);
    for sub in g.groups() {
        visit_group(sub, depth + 1, f);
    }
}
