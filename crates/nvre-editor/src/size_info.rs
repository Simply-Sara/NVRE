//! The Size Info window: how much of the file each record type occupies.
//!
//! GECK's File Details has a Size Info button for the same question — a mod's
//! `.esm` is a quarter of a gigabyte and the useful thing to know is which
//! record types are eating it. `FalloutNV.esm`'s answer is not what one would
//! guess: `LAND` is 29,363 records and 164 MB of its 245 MB, while `REFR` is
//! 307,710 records and 15 MB [measured].
//!
//! # The accounting
//!
//! Every byte of a plugin is attributable, and the identity is exact:
//!
//! ```text
//! file = TES4 header (24 + its payload) + Σ group headers + Σ (24 + record payload)
//! ```
//!
//! A group's header is attributed to the record type the group is labelled with,
//! but only when that label *is* a type: a top-level group is `STAT`, while a
//! cell's group is labelled with its grid coordinates and a worldspace's with its
//! FormID. Those belong to no type and are counted separately.
//!
//! A record's cost is its 24-byte header plus its payload — the payload is the
//! header's `data_size`, which for a compressed record is the *compressed* size,
//! so this is what the record costs on disk rather than what it expands to. A
//! group's cost is its own 24-byte header, attributed to the type it is labelled
//! with, because that label is what the group is for.
//!
//! Because every byte is attributed, the column sums to the file size exactly.
//! That is a property worth keeping: a breakdown whose total does not match the
//! file is telling the reader something is missing, and [`SizeBreakdown::total`]
//! is what a test can check it against.

use crate::file_details::{FileDetails, RowKind};

/// The 24 bytes of a record's or a group's header.
const HEADER_BYTES: u64 = 24;

/// One record type's share of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSize {
    /// The four-character type, or a group's label.
    pub kind: String,
    /// How many records of this type the file holds. Zero for a type that only
    /// appears as a group label.
    pub records: usize,
    /// The records' payload bytes, as the file stores them.
    ///
    /// A compressed record counts its compressed size, because that is what it
    /// costs on disk.
    pub data: u64,
    /// Every byte this type is responsible for: the payloads, the 24-byte record
    /// headers, and the 24-byte headers of the groups labelled with it.
    pub total: u64,
}

impl TypeSize {
    /// What fraction of the file this type occupies, 0.0 to 1.0.
    pub fn fraction(&self, file_size: u64) -> f64 {
        if file_size == 0 {
            return 0.0;
        }
        self.total as f64 / file_size as f64
    }
}

/// Every record type's share of a file, largest first.
#[derive(Debug, Clone, Default)]
pub struct SizeBreakdown {
    /// The types, by total bytes descending.
    pub types: Vec<TypeSize>,
    /// The file's size on disk.
    pub file_size: u64,
    /// The `TES4` header's own bytes: its 24-byte header and its payload.
    ///
    /// Kept apart rather than attributed to a type, because it is not one.
    pub header_bytes: u64,
    /// The headers of groups that are not labelled with a type.
    ///
    /// A cell's group is labelled with its grid coordinates and a worldspace's
    /// with its FormID, so those 24-byte headers belong to no record type. They
    /// are counted here so that every byte is still accounted for.
    pub other_bytes: u64,
}

impl SizeBreakdown {
    /// Every byte accounted for: the types' totals plus the file header.
    ///
    /// This equals [`SizeBreakdown::file_size`] for a plugin the scan read whole.
    /// A caller showing a breakdown can compare the two rather than trusting it.
    pub fn total(&self) -> u64 {
        self.types.iter().map(|t| t.total).sum::<u64>() + self.header_bytes + self.other_bytes
    }
}

/// Work out the breakdown from a scanned file.
///
/// The scan already holds every field this needs — a row's kind, its payload
/// size, and its type — so nothing is re-read. `file_size` is passed rather than
/// derived from the rows, because the point of the last column is to compare
/// against the real file.
pub fn breakdown(details: &FileDetails, file_size: u64) -> SizeBreakdown {
    use std::collections::HashMap;

    let mut by_type: HashMap<String, TypeSize> = HashMap::new();
    let mut header_bytes = 0u64;
    // Group headers whose label is not a type: a cell's or worldspace's own
    // group. They are real bytes, so they are counted, just not attributed.
    let mut other_bytes = 0u64;

    for row in 0..details.len() {
        let length = details.length.get(row).unwrap_or(0) as u64;
        let kind = details
            .kind
            .get(row)
            .map(|k| String::from_utf8_lossy(&k).into_owned())
            .unwrap_or_default();

        match details.row_kind.get(row).unwrap_or_default() {
            RowKind::Header => {
                // The file's own header: a record, but not a type.
                header_bytes += HEADER_BYTES + length;
            }
            RowKind::Group => {
                // A group costs its header. It belongs to the type it is labelled
                // with — but only a top-level group is labelled with a type;
                // nested ones carry grid coordinates or FormIDs, which are not
                // types and must not become rows [measured: FalloutNV.esm has
                // 77,000 groups but only ~110 of them are top-level].
                match details.group_label.get(row).flatten() {
                    Some(label) => {
                        let kind = String::from_utf8_lossy(&label).into_owned();
                        let entry = by_type.entry(kind.clone()).or_insert_with(|| TypeSize {
                            kind,
                            records: 0,
                            data: 0,
                            total: 0,
                        });
                        entry.total += HEADER_BYTES;
                    }
                    // A nested group's header is still bytes in the file, so it
                    // cannot be dropped: it is counted against the file rather
                    // than against a type, which keeps the total exact.
                    None => other_bytes += HEADER_BYTES,
                }
            }
            RowKind::Record => {
                let entry = by_type.entry(kind.clone()).or_insert_with(|| TypeSize {
                    kind,
                    records: 0,
                    data: 0,
                    total: 0,
                });
                entry.records += 1;
                entry.data += length;
                entry.total += HEADER_BYTES + length;
            }
        }
    }

    let mut types: Vec<TypeSize> = by_type.into_values().collect();
    // Largest first: the question is "what is taking the space", and the answer
    // is at the top. Ties break by name so the order is stable between runs.
    types.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.kind.cmp(&b.kind)));

    SizeBreakdown {
        types,
        file_size,
        header_bytes,
        other_bytes,
    }
}

/// Bytes as a readable size: `164.8 MB`, `15.1 MB`, `512 B`.
///
/// Powers of 1024, with one decimal above a kilobyte — a table of raw byte counts
/// is unreadable at this scale, and the exact number is still in the column
/// beside it.
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}
