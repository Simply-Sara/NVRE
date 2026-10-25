//! The File Details window: every record in a plugin, one row each.
//!
//! GECK and the Creation Kit both have this dialog — a flat list of every record
//! in the loaded file with its type, flags, Editor ID, FormID and where it sits
//! in the file. It is the tool for finding a record by something other than its
//! name, and for spotting the things a plugin carries that nobody meant to ship.
//!
//! # Why this is a scan and not a parse
//!
//! `FalloutNV.esm` holds 465,017 records. Reading it through
//! [`nvre_esm::Plugin::open`] materializes every record's payload and subrecords
//! — hundreds of megabytes and several hundred milliseconds — to answer a
//! question that is entirely in the 24-byte record headers plus one small
//! subrecord each. So the scan walks the file's group tree by hand, reads only
//! what each column needs, and never touches a payload it does not have to.
//!
//! The Editor ID is the one thing outside the header, and it is the *first*
//! subrecord of almost every record, so the scan reads a small window at each
//! record's start and stops at the first `EDID`. A record with no `EDID` in that
//! window is left unnamed rather than seeking through its payload.
//!
//! # Why the rows are a fixed array of columns
//!
//! A `Vec<DetailRow>` of 465,000 structs is ~40 MB and copies badly; the table
//! draws only the rows on screen, so what it wants is cheap random access to a
//! few dozen fields. Each column is stored as its own packed array
//! ([`Column`]), which is a quarter of the memory and lets the table read a row
//! without touching the others.

use std::path::{Path, PathBuf};

/// One column of the File Details table.
///
/// Stored column-wise rather than row-wise: see the module comment.
#[derive(Debug, Clone, Default)]
pub struct Column<T> {
    values: Vec<T>,
}

impl<T: Copy> Column<T> {
    /// Append a value, as the next row's.
    ///
    /// Public so a table can be built by hand — a test with three rows does not
    /// need a plugin file, and a caller assembling rows from elsewhere should not
    /// have to route them through a scan.
    pub fn push(&mut self, value: T) {
        self.values.push(value);
    }

    /// The value at `row`, or `None` when there is no such row.
    pub fn get(&self, row: usize) -> Option<T> {
        self.values.get(row).copied()
    }

    /// How many rows the column holds.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the column holds no rows.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Every value, in row order.
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
}

/// The Editor ID column, which is the one variable-length column.
///
/// Held as one byte arena and an offset per row rather than a `String` per row:
/// 465,000 `String`s is 465,000 allocations and 11 MB of headers for text that is
/// usually under 30 bytes.
#[derive(Debug, Clone, Default)]
pub struct Names {
    /// Every name, concatenated.
    bytes: Vec<u8>,
    /// Where each row's name starts in `bytes`.
    starts: Vec<u32>,
    /// How many rows hold no name at all.
    empty: Vec<bool>,
}

impl Names {
    /// Append a row's name, `None` for an unnamed row.
    pub fn push(&mut self, name: Option<&str>) {
        self.starts.push(self.bytes.len() as u32);
        match name {
            Some(n) => {
                self.empty.push(false);
                self.bytes.extend_from_slice(n.as_bytes());
            }
            None => self.empty.push(true),
        }
    }

    /// The name at `row`, or `None` when the row is unnamed.
    pub fn get(&self, row: usize) -> Option<&str> {
        if *self.empty.get(row)? {
            return None;
        }
        let start = *self.starts.get(row)? as usize;
        let end = self
            .starts
            .get(row + 1)
            .map(|e| *e as usize)
            .unwrap_or(self.bytes.len());
        // The bytes came from a `cp1252` decode, so this is always valid UTF-8.
        std::str::from_utf8(&self.bytes[start..end]).ok()
    }

    /// How many rows the column holds.
    pub fn len(&self) -> usize {
        self.empty.len()
    }

    /// Whether the column holds no rows.
    pub fn is_empty(&self) -> bool {
        self.empty.is_empty()
    }
}

/// What a row is: the file's header, a group, or a record.
///
/// The table lists all three, as xEdit's does: a plugin is a tree of groups, and
/// a flat list of records with the groups left out hides the structure that says
/// where a record lives. GECK's own dialog lists records only, which is why this
/// distinction is worth stating rather than assuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowKind {
    /// The `TES4` record: the file's own header.
    Header,
    /// A `GRUP`: a group of records, or of other groups.
    Group,
    /// A record.
    #[default]
    Record,
}

/// One record, as the table shows it.
///
/// The columns are GECK's own, in its own order: the record's type, whether it
/// is flagged deleted or ignored, its Editor ID and FormID, where it sits in the
/// file, and the Creation Kit's revision word.
#[derive(Debug, Clone, Default)]
pub struct FileDetails {
    /// The file the rows came from.
    pub path: PathBuf,
    /// What the row is: the header, a group, or a record.
    pub row_kind: Column<RowKind>,
    /// How deep in the group tree the row sits.
    ///
    /// The top level is 0, a record in a top-level group is 1, and so on. The
    /// table indents by this, which is what makes a list holding `GRUP` rows
    /// readable rather than a jumble.
    ///
    /// It also decides whether a group's label is a *type*: a top-level group is
    /// labelled with a record signature (`STAT`), while a deeper one is labelled
    /// with whatever identifies it — a cell's grid coordinates, a worldspace's
    /// FormID — which is not a type at all.
    pub depth: Column<u8>,
    /// The row's four-character type: `TES4`, `GRUP`, or a record's signature.
    pub kind: Column<[u8; 4]>,
    /// The `Deleted` record flag (`0x20`).
    pub deleted: Column<bool>,
    /// The `Ignored` record flag (`0x1000`).
    pub ignored: Column<bool>,
    /// `EDID`: the Editor ID, when the record has one.
    pub editor_id: Names,
    /// The record's own FormID, as the file stores it.
    pub form_id: Column<u32>,
    /// The record header's offset in the file.
    pub offset: Column<u64>,
    /// The record's payload size on disk, the header's `data_size`.
    pub length: Column<u32>,
    /// The Creation Kit's revision word, the header's `vc_info`.
    ///
    /// The header calls it "used for revision control by the Creation Kit, if
    /// enabled" [documented: fopdoc's Record Format table]; it is the `VC` column
    /// GECK shows.
    pub vc: Column<u32>,
    /// The `Compressed` record flag (`0x40000`).
    ///
    /// Not a column GECK shows, but the one flag that changes what `length`
    /// means: a compressed record's `data_size` is its *compressed* size, so a
    /// reader comparing lengths needs to know.
    pub compressed: Column<bool>,
    /// A group's own label, as a type name when it is one.
    ///
    /// `None` for a record (whose name is its `EDID`), and for a group labelled
    /// with something that is not text — a cell's grid coordinates or a
    /// worldspace's FormID, which are four bytes of numbers and would be garbage
    /// if shown as characters.
    pub group_label: Column<Option<[u8; 4]>>,
}

impl FileDetails {
    /// How many rows the table holds.
    pub fn len(&self) -> usize {
        self.row_kind.len()
    }

    /// Whether the table holds no rows.
    pub fn is_empty(&self) -> bool {
        self.row_kind.is_empty()
    }

    /// How many of the rows are records, rather than the header or a group.
    pub fn record_count(&self) -> usize {
        self.row_kind
            .as_slice()
            .iter()
            .filter(|k| **k == RowKind::Record)
            .count()
    }
}

/// What went wrong reading a file's details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailsError {
    /// The file could not be read.
    Io(String),
    /// The file is not a plugin.
    NotAPlugin,
    /// The group tree is malformed, at this offset.
    Structure(u64, String),
}

impl std::fmt::Display for DetailsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetailsError::Io(e) => write!(f, "cannot read the file: {e}"),
            DetailsError::NotAPlugin => {
                write!(f, "not a plugin: the file does not start with TES4")
            }
            DetailsError::Structure(at, msg) => write!(f, "malformed at 0x{at:x}: {msg}"),
        }
    }
}

/// The record header's flags this window reads.
///
/// [documented: fopdoc's Record Format "Flags" table — `0x20` Deleted, `0x1000`
/// Ignored, `0x40000` Compressed]. The rest of the word is contextual, its meaning
/// changing with the record type, so nothing else is named here.
mod flags {
    /// The record was deleted.
    pub const DELETED: u32 = 0x0000_0020;
    /// The record is ignored.
    pub const IGNORED: u32 = 0x0000_1000;
    /// The payload is a u32 decompressed size and then a zlib stream.
    pub const COMPRESSED: u32 = 0x0004_0000;
}

/// How many bytes of a record the scan reads looking for its `EDID`.
///
/// The Editor ID is the first subrecord of nearly every record, so a small
/// window finds it; a record whose first subrecords are long and not `EDID` is
/// left unnamed rather than having its payload walked. 512 bytes covers a
/// `MAST`-first header and every ordinary record's leading subrecords.
const EDID_WINDOW: usize = 512;

/// Scan `path` and return every record it holds, in file order.
///
/// This reads the whole file into memory — a plugin is one allocation, and the
/// alternative is a syscall per record — but walks only the headers, so the cost
/// is the read plus a pointer bump per record rather than a parse.
pub fn scan(path: &Path) -> Result<FileDetails, DetailsError> {
    let data = std::fs::read(path).map_err(|e| DetailsError::Io(e.to_string()))?;
    scan_bytes(&data, path.to_path_buf())
}

/// The same scan, over bytes already in memory.
///
/// Split out so the walk is testable without a file, and so a caller holding a
/// plugin's bytes does not read them again.
pub fn scan_bytes(data: &[u8], path: PathBuf) -> Result<FileDetails, DetailsError> {
    if data.get(..4) != Some(b"TES4") {
        return Err(DetailsError::NotAPlugin);
    }
    let mut out = FileDetails {
        path,
        ..Default::default()
    };
    // The `TES4` row first: it is the file's own header, and xEdit lists it as
    // the tree's root. Its FormID, VC and flags are real header fields, so they
    // are shown rather than blanked.
    let size = u32::from_le_bytes(
        data.get(4..8)
            .ok_or(DetailsError::NotAPlugin)?
            .try_into()
            .unwrap(),
    );
    let flags = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let vc = u32::from_le_bytes(data[16..20].try_into().unwrap());
    out.row_kind.push(RowKind::Header);
    out.depth.push(0);
    out.kind.push(*b"TES4");
    out.deleted.push(flags & flags::DELETED != 0);
    out.ignored.push(flags & flags::IGNORED != 0);
    out.compressed.push(flags & flags::COMPRESSED != 0);
    // The header record's own FormID is 0 in every plugin [documented: fopdoc's
    // Record Format table, "TES4 records have a FormID of 0"].
    out.form_id
        .push(u32::from_le_bytes(data[12..16].try_into().unwrap()));
    out.offset.push(0);
    out.length.push(size);
    out.vc.push(vc);
    out.editor_id.push(None);
    out.group_label.push(None);

    let first = 24usize + size as usize;
    walk(data, first, data.len(), 1, &mut out)?;
    Ok(out)
}

/// Walk a run of the file, adding each record it finds.
///
/// Groups nest: a top-level group holds records, and a record's child group
/// (a worldspace's cells, a cell's references) holds more. The walk recurses
/// through both rather than flattening, because the group boundaries are what
/// say where a record ends.
fn walk(
    data: &[u8],
    mut pos: usize,
    end: usize,
    depth: usize,
    out: &mut FileDetails,
) -> Result<(), DetailsError> {
    // A plugin's group nesting is shallow — a worldspace group holding cell
    // groups is the deepest real case — so this bound only catches a cycle in a
    // corrupt file, which would otherwise recurse until the stack dies.
    const MAX_DEPTH: usize = 16;
    if depth > MAX_DEPTH {
        return Ok(());
    }
    while pos + 24 <= end {
        let sig = data.get(pos..pos + 4).ok_or_else(|| {
            DetailsError::Structure(pos as u64, "the record header is cut short".into())
        })?;
        if sig == b"GRUP" {
            // A group: its size counts the whole group, header included.
            let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            if size < 24 {
                return Err(DetailsError::Structure(
                    pos as u64,
                    format!("a group claims {size} bytes, less than its own header"),
                ));
            }
            // The group is a row of its own, as xEdit shows it: its label and
            // type say what the records under it are.
            out.row_kind.push(RowKind::Group);
            out.depth.push(depth as u8);
            out.kind.push(*b"GRUP");
            // A group has no flags, FormID, VC or length of its own — its header
            // is a size, a label and a type — so those cells are zero and the
            // table shows them blank.
            out.deleted.push(false);
            out.ignored.push(false);
            out.compressed.push(false);
            out.form_id.push(0);
            out.offset.push(pos as u64);
            out.length.push(size as u32);
            out.vc.push(0);
            // A group's label is the only name it has. For a **top-level** group
            // it is the record type the group holds (`STAT`); for a nested one it
            // is a cell's grid coordinates or a worldspace's FormID, which are
            // four bytes of numbers and would be garbage shown as characters
            // [measured: FalloutNV.esm has 46,296 distinct group labels and
            // 46,195 of them hold non-printable bytes].
            let label: [u8; 4] = data
                .get(pos + 8..pos + 12)
                .and_then(|l| l.try_into().ok())
                .unwrap_or(*b"????");
            let is_type = label.iter().all(|b| b.is_ascii_graphic()) && depth == 1;
            out.group_label
                .push(if is_type { Some(label) } else { None });
            if is_type {
                out.editor_id.push(Some(&String::from_utf8_lossy(&label)));
            } else {
                out.editor_id.push(None);
            }

            let inner_end = (pos + size).min(end);
            walk(data, pos + 24, inner_end, depth + 1, out)?;
            pos += size;
            continue;
        }

        let data_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());
        let flags = u32::from_le_bytes(data[pos + 8..pos + 12].try_into().unwrap());
        let form_id = u32::from_le_bytes(data[pos + 12..pos + 16].try_into().unwrap());
        let vc_info = u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap());

        out.row_kind.push(RowKind::Record);
        out.depth.push(depth as u8);
        out.kind.push([sig[0], sig[1], sig[2], sig[3]]);
        out.deleted.push(flags & flags::DELETED != 0);
        out.ignored.push(flags & flags::IGNORED != 0);
        out.compressed.push(flags & flags::COMPRESSED != 0);
        out.form_id.push(form_id);
        out.offset.push(pos as u64);
        out.length.push(data_size);
        out.vc.push(vc_info);
        out.group_label.push(None);

        // The Editor ID: the first `EDID` in the record's opening bytes.
        //
        // A compressed record is the exception: its payload is a `u32`
        // decompressed size and then a zlib stream, not a run of subrecords, so
        // the `EDID` is inside the stream. 7.1% of `FalloutNV.esm`'s records are
        // compressed [measured: 33,179 of 465,016], so skipping them would leave
        // that share of the table unnamed.
        let payload = pos + 24;
        let payload_end = (payload + data_size as usize).min(end);
        let compressed = flags & flags::COMPRESSED != 0;
        let editor_id = if compressed {
            inflated_edid(&data[payload..payload_end])
        } else {
            let window_end = (payload + EDID_WINDOW).min(payload_end);
            edid_in(&data[payload..window_end])
        };
        match editor_id {
            Some(id) => {
                out.editor_id.empty.push(false);
                out.editor_id.starts.push(out.editor_id.bytes.len() as u32);
                out.editor_id.bytes.extend_from_slice(id.as_bytes());
            }
            None => {
                out.editor_id.empty.push(true);
                out.editor_id.starts.push(out.editor_id.bytes.len() as u32);
            }
        }

        // Advance past the payload. A record whose size runs past the group's
        // end is the file's last record cut short: stop rather than read past.
        pos = payload + data_size as usize;
        if pos > end {
            break;
        }
    }
    Ok(())
}

/// The `EDID` inside a compressed record's zlib stream.
///
/// The stream is a `u32` decompressed size and then the DEFLATE data. Only the
/// opening bytes are needed — the `EDID` is the first subrecord — so this inflates
/// with a **limit** and reads what came out before the limit was hit.
///
/// That limit is the difference between a usable window and an unusable one:
/// `FalloutNV.esm`'s 33,179 compressed records decompress to 253 MB, and
/// materializing all of it made the scan slower than parsing the whole plugin
/// (1313 ms against 602 ms). Stopping at the window's worth costs a few hundred
/// bytes per record instead.
///
/// A record whose `EDID` sits past the limit is left unnamed, which is the same
/// answer the uncompressed path gives for a record with no leading `EDID`. A
/// stream that fails to inflate at all is also left unnamed: a damaged record is
/// still a row, and one unnamed row is better than a scan that gives up.
fn inflated_edid(payload: &[u8]) -> Option<String> {
    // The `u32` size, then the zlib header (two bytes), then the DEFLATE data.
    // `inflate_prefix` is a raw-DEFLATE inflater, so the header is skipped here
    // rather than handed to it — a zlib header read as DEFLATE is "invalid stored
    // block lengths", which is what this produced until it was.
    let body = payload.get(6..)?;
    let prefix = nvre_inflate::inflate_prefix(body, EDID_WINDOW).ok()?;
    edid_in(&prefix)
}

/// The `EDID` in a record's opening bytes, if it is there.
///
/// Subrecords are `sig` then a `u16` length then the payload, and the payload of
/// an `EDID` is a null-terminated string in Windows-1252 like every other name in
/// these formats.
fn edid_in(bytes: &[u8]) -> Option<String> {
    let mut off = 0usize;
    while off + 6 <= bytes.len() {
        let sig = &bytes[off..off + 4];
        let len = u16::from_le_bytes(bytes[off + 4..off + 6].try_into().unwrap()) as usize;
        let data = bytes.get(off + 6..off + 6 + len)?;
        if sig == b"EDID" {
            let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
            return Some(nvre_util::cp1252::decode(&data[..end]));
        }
        // `XXXX` says the next subrecord's length is a `u32` rather than a
        // `u16`, which no `EDID` uses but which would misalign the walk if a
        // record led with one.
        if sig == b"XXXX" {
            return None;
        }
        off += 6 + len;
    }
    None
}
