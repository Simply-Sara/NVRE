//! Schema-as-data: record/subrecord layouts loaded from JSON files at runtime, so
//! definitions can be corrected without recompiling.
//!
//! # Two directories, not one
//!
//! A schema is loaded from a **base** directory and zero or more **game**
//! directories, and the split is the point:
//!
//! ```text
//! schemas/base/    the container, the group tree, the FormID model, and every
//!                  layout the Gamebryo engines share. Loaded for every game.
//! schemas/<game>/  only what that game does differently, replacing whole
//!                  records in the base.
//! ```
//!
//! This is NVRE's own layout, not NVForever's. Upstream ships a single flat
//! directory per game with no notion of inheritance, so a second engine restated
//! the shared half and drifted from it on every edit. The base directory is
//! NVRE's answer to that, and it is also what made Fallout 3 four records instead
//! of a copy of 107.
//!
//! File format (`fnv.schema/1`), any number of files merged from a directory:
//!
//! ```json
//! {
//!   "format": "fnv.schema/1",
//!   "types":   { "Bounds": {"struct": [{"name": "x1", "type": "i16"}, ...]} },
//!   "common":  { "EDID": {"name": "Editor ID", "type": "zstring", "confidence": "observed"} },
//!   "records": { "GLOB": {"name": "Global", "subrecords": { "FNAM": {...}, "FLTV": {...} }} }
//! }
//! ```
//!
//! Type expressions: a primitive name (`u8 u16 u32 u64 i8 i16 i32 i64 f32 f64 formid
//! zstring zstring_array len_string string bytes`), a named type from `types`, or an object:
//! `{"struct": [fields]}`, `{"array": T, "count": N}` (no count = fill the rest),
//! `{"array": T, "count_prefix": "u32"}` (the count is read from the data as that
//! integer type first — xEdit's `wbArray`/`wbArrayS` with `Count = -1`),
//! `{"array": T, "count_field": "n"}` (the count is an earlier integer field of
//! the same struct — xEdit's `SetCountPath`; lets one pair of count fields serve two
//! different-length arrays),
//! `{"enum": prim, "values": {"0": "Name"}}`, `{"flags": prim, "bits": {"0": "Name"}}`,
//! `{"union": [{"size": N, "type": T}, ..., {"type": T}]}` (the first variant whose
//! `size` equals the bytes left, else the first variant without `size`; the value is
//! the chosen variant's value), and, when the union object also carries
//! `"switch_on": "field"` plus `"cases": {"v": i, ...}`, the variant chosen by an
//! earlier sibling field's *value* (xEdit's `wbUnion` with a decider function, e.g.
//! `wbPerkDATADecider` reading PRKE.Type). A variant without `size` is the default.
//! Struct fields may be `"optional": true` (allowed to be absent at the end).
//!
//! A record entry may also name bits of its 24-byte record header's flags field
//! (additive; older loaders ignore the key):
//!
//! ```json
//! "WEAP": {"name": "Weapon", "header_flags": {"bits": {"10": "Quest Item"},
//!          "confidence": "documented", "note": "source"}, "subrecords": {}}
//! ```
//!
//! Bits are numbered from 0 (least significant). The container-level flags every record
//! shares (deleted, compressed, master) stay in `raw::record_flags`: the reader needs them
//! before any schema is loaded.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    FormId,
    /// NUL-terminated Windows-1252 string occupying the rest of the field.
    ZString,
    /// NUL-terminated Windows-1252 strings filling the rest of the field, decoded to a
    /// list. One extra NUL after the last string closes the list and is not an element
    /// [observed: every CREA KFFZ/NIFZ in the ten masters ends that way].
    ZStringArray,
    /// Windows-1252 string preceded by its byte length as a little-endian `u32`:
    /// xEdit's `wbLenString` (AlternateTexture, MODS/MO2S/MO3S/MO4S model names).
    /// The length counts the bytes that follow, with no terminator of any kind.
    LenString,
    /// Windows-1252 string without terminator occupying the rest of the field.
    String,
    /// NUL-terminated Windows-1252 string that stops at the first NUL and leaves
    /// the cursor after it, so more fields can follow in the same struct. This is
    /// xEdit's `wbString` when it is not the last member of a struct (e.g. DEBR
    /// DATA: `[u8 Percentage][string Model FileName][u8 Has Collision]`).
    CString,
    /// A string that a *localized* file holds as a 4-byte string ID rather than
    /// as text.
    ///
    /// Fallout 4's shipped masters set the file's "Localized" flag (the TES4
    /// header's bit 7), and in such a file the fields xEdit declares as
    /// `wbLString` — `FULL`, `DESC` and their kin — carry a `u32` index into
    /// the file's string table instead of the text itself
    /// [documented: xEdit `wbDefinitionsFO4.pas`, `wbLStringKC`; observed:
    /// `FULL` is four bytes in all 43 record types of `Fallout4.esm` that carry
    /// it, and the master sets bit 7].
    ///
    /// The same field in a file that is *not* localized holds ordinary text, so
    /// this type reads one or the other according to the file the record came
    /// from: four bytes when the file is localized, a NUL-terminated
    /// Windows-1252 string when it is not. Which one it is comes from the
    /// decode context, not from the schema, so a schema written once describes
    /// both kinds of file.
    Localized,
    /// Opaque bytes occupying the rest of the field (hex in output).
    Bytes,
    /// The field's first four bytes read as a little-endian `u32`, without
    /// consuming them.
    ///
    /// A record's field can take more than one shape, and the shape is
    /// decided by the field's first word. Skyrim's `MODT` is a headered form
    /// whose first word is 2 and a bare form whose first word is a texture
    /// hash. The first word has to be looked at before the field is read, so
    /// a union can switch on it.
    PeekU32,
    /// A length-prefixed string whose length prefix is two bytes wide.
    ///
    /// Fallout 4's `wbLenString` always writes a two-byte length, where the
    /// other games' is four bytes wide [observed: every `wbLenString` in
    /// `wbDefinitionsFO4.pas` names a size of 2].
    LenStringU16,
}

impl Prim {
    pub fn parse(s: &str) -> Option<Prim> {
        Some(match s {
            "u8" => Prim::U8,
            "u16" => Prim::U16,
            "u32" => Prim::U32,
            "u64" => Prim::U64,
            "i8" => Prim::I8,
            "i16" => Prim::I16,
            "i32" => Prim::I32,
            "i64" => Prim::I64,
            "f32" => Prim::F32,
            "f64" => Prim::F64,
            "formid" => Prim::FormId,
            "zstring" => Prim::ZString,
            "zstring_array" => Prim::ZStringArray,
            "len_string" => Prim::LenString,
            "len_string_u16" => Prim::LenStringU16,
            "string" => Prim::String,
            "cstring" => Prim::CString,
            "localized" => Prim::Localized,
            "bytes" => Prim::Bytes,
            "peek_u32" => Prim::PeekU32,
            _ => return None,
        })
    }
    /// Fixed size in bytes, or None for rest-of-field types.
    pub fn size(self) -> Option<usize> {
        match self {
            Prim::U8 | Prim::I8 => Some(1),
            Prim::U16 | Prim::I16 => Some(2),
            Prim::U32 | Prim::I32 | Prim::F32 | Prim::FormId | Prim::PeekU32 => Some(4),
            Prim::U64 | Prim::I64 | Prim::F64 => Some(8),
            Prim::ZString
            | Prim::ZStringArray
            | Prim::LenString
            | Prim::LenStringU16
            | Prim::String
            | Prim::CString
            | Prim::Localized
            | Prim::Bytes => None,
        }
    }
    pub fn is_integer(self) -> bool {
        matches!(
            self,
            Prim::U8
                | Prim::U16
                | Prim::U32
                | Prim::U64
                | Prim::I8
                | Prim::I16
                | Prim::I32
                | Prim::I64
        )
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeDef,
    pub optional: bool,
    /// Record this field's integer value in `DecodeCtx::exposed` after decoding, so a
    /// later subrecord of the same record can switch a union on it (xEdit's
    /// `RecordBySignature[PRKE]` / `ElementNativeValues['EPFT']` deciders).
    pub expose: bool,
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Prim(Prim),
    Named(String),
    Struct(Vec<Field>),
    Array {
        elem: Box<TypeDef>,
        count: Option<usize>,
        count_prefix: Option<Prim>,
        count_field: Option<String>,
    },
    Enum {
        base: Prim,
        values: Vec<(i64, String)>,
    },
    Flags {
        base: Prim,
        bits: Vec<(u32, String)>,
    },
    /// Layout chosen by the number of bytes left: `(Some(n), T)` applies when exactly `n`
    /// bytes remain, `(None, T)` is the fallback. Expresses xEdit's size-decided unions
    /// (e.g. WATR DATA: 2-byte damage, or the older 186-byte visual data).
    Union(Vec<(Option<usize>, TypeDef)>),
    /// Union whose variant is chosen by an earlier sibling field's *value* rather
    /// than by the bytes left (xEdit's `wbUnion` with a decider function, e.g.
    /// `wbPerkDATADecider` reading PRKE.Type, or `wbEPFDDecider` reading EPFT).
    /// `cases` maps that value to a variant index; `default` is used when no case
    /// matches. `name` is the sibling field to read, which must decode to an
    /// integer (enums and flags included) earlier in the same struct.
    ///
    /// The value the switch reads may also be *text*. xEdit's
    /// `wbPubPackCNAMDecider` reads a package's `ANAM`, which names the type
    /// of the value that follows — "Bool", "Integer", "Float" — and the
    /// variant is chosen by that name. A case's key is then the name itself,
    /// and a case whose key is a number is the integer's decimal form, so
    /// both kinds of switch are expressed the same way.
    ValueUnion {
        name: String,
        cases: Vec<(String, usize)>,
        default: usize,
        variants: Vec<TypeDef>,
    },
    /// A union whose variant is chosen by the field's first word, read
    /// without consuming it.
    ///
    /// Some fields take more than one shape and the first word says which.
    /// Skyrim's `MODT` is a headered form whose first word is 2 and a bare
    /// form whose first word is a texture hash, and the first word belongs to
    /// the field either way — so the whole field is the union, not a field
    /// inside it.
    PeekUnion {
        cases: Vec<(u64, usize)>,
        default: usize,
        variants: Vec<TypeDef>,
    },
}

#[derive(Debug, Clone)]
pub struct SubDef {
    pub name: String,
    pub ty: TypeDef,
    pub confidence: String,
    pub note: Option<String>,
    /// Record this subrecord's integer value in `DecodeCtx::exposed` after
    /// decoding, so a later subrecord of the same record can switch a union on
    /// it (xEdit's `Container.ElementNativeValues['EPFT']` decider).
    pub expose: bool,
    /// The sibling subrecord whose value is this subrecord's element count.
    ///
    /// Some records carry a count in one subrecord and the elements in
    /// another: Skyrim's `KSIZ` holds the number of keywords and `KWDA` holds
    /// them all in one subrecord. The count is in a different subrecord from
    /// the elements, so it cannot be a struct's `count_field`.
    ///
    /// The named subrecord must come *before* this one in the record, since
    /// the count has to have been read by the time the elements are.
    pub count_from: Option<[u8; 4]>,
}

#[derive(Debug, Clone)]
pub struct RecordDef {
    pub name: String,
    pub confidence: String,
    pub subrecords: HashMap<[u8; 4], SubDef>,
    /// Definitions for signatures that are not fixed, keyed by a suffix.
    ///
    /// Some records name their fields by position rather than by a fixed
    /// signature. Skyrim's `WTHR` has one cloud texture per layer, and layer
    /// *n* uses the signature `chr(ord('0') + n)` followed by `0TX`, so the
    /// layers run `00TX`, `10TX`, `20TX` and on past the digits into the
    /// punctuation and the letters. A key of `"0TX"` matches any signature
    /// that ends with it.
    pub wildcards: Vec<(Vec<u8>, SubDef)>,
    /// Named bits of the record header's flags field, sorted by bit (`header_flags`).
    pub header_flags: Vec<(u32, String)>,
    pub header_flags_confidence: Option<String>,
    /// Ordered-list structure, when the record's meaning is positional rather than
    /// per-subrecord. `None` for every ordinary record.
    pub sequence: Option<SequenceDef>,
}

/// A record whose subrecords are an ordered list of blocks.
///
/// Expresses what a per-subrecord decoder cannot: UESP's Skyrim `SCEN` says its
/// three lists "have no terminator and simply repeat until a field designates
/// that the next block is starting", `PACK`'s fields "occur multiple times and
/// have different meanings depending upon where they occur", and `QUST`'s "order
/// in which the fields appear provides important information".
///
/// The shape is: a run of subrecords, in which a **block opener** starts a
/// block and every subrecord after it belongs to that block until the next
/// opener. Each block kind carries its own subrecord definitions, because the
/// same signature means different things in different blocks — `ANAM` is a
/// formid in `ASPC` and a string in `PACK`'s activity list, and within one
/// record a signature can change meaning from block to block.
///
/// A record's own `subrecords` map holds the definitions for the parts of the
/// record that are *not* in a block; a block's own map holds the rest.
#[derive(Debug, Clone, Default)]
pub struct SequenceDef {
    /// Subrecords that appear before any block opens, in order.
    pub start: Vec<[u8; 4]>,
    /// The blocks, in the order their openers first appear.
    pub blocks: Vec<BlockDef>,
    /// Subrecords that appear after the blocks end, in order.
    pub end: Vec<[u8; 4]>,
}

/// One block of a sequence: the signature that opens it, and what its
/// subrecords mean.
#[derive(Debug, Clone)]
pub struct BlockDef {
    /// The block's name, for a schema note and for a decode error.
    pub name: String,
    /// The signature that opens a block of this kind.
    pub opener: [u8; 4],
    /// The subrecords this block contains, and their layouts. A signature may
    /// appear in several blocks with a different layout in each.
    pub subrecords: HashMap<[u8; 4], SubDef>,
    /// The order the block's subrecords appear in.
    ///
    /// This is what disambiguates a signature that both opens a block and
    /// appears inside one. `ALID` opens `SCEN`'s actor list and is also a field
    /// of an action, so the two cases cannot be told apart by the signature
    /// alone: a signature that matches the block's *next expected* slot
    /// continues the block, and one that does not — and is a known opener —
    /// starts a new block.
    ///
    /// A signature may repeat within a block (a phase's `CTDA` appears twice),
    /// so the order is a cycle rather than a single pass: the next expected slot
    /// after the last one is the first again.
    pub order: Vec<[u8; 4]>,
    /// The block's own subrecords, chosen by the opener's *value*.
    ///
    /// Some records have a block whose meaning depends on what the opener
    /// says, not just which signature it is. Skyrim's `PACK` opens a public
    /// activity with `ANAM`, whose value is a string naming the activity's
    /// type — "Bool", "Float", "Location", "Topic" — and the field that
    /// follows is a `CNAM` for the first three and a `PLDT` or `PDTO` for the
    /// rest. The opener's value selects which fields the block holds.
    ///
    /// `variants` maps the opener's value to that variant's subrecords. A
    /// value not listed falls back to `subrecords`, the block's default.
    pub variants: Vec<(String, HashMap<[u8; 4], SubDef>)>,
}

impl SequenceDef {
    /// The block whose opener is `sig`, if any.
    pub fn block_for(&self, sig: &[u8; 4]) -> Option<&BlockDef> {
        self.blocks.iter().find(|b| &b.opener == sig)
    }
    /// Whether `sig` opens some block.
    pub fn is_opener(&self, sig: &[u8; 4]) -> bool {
        self.blocks.iter().any(|b| &b.opener == sig)
    }
    /// Whether `sig` is named as a start or end signature.
    pub fn is_boundary(&self, sig: &[u8; 4]) -> bool {
        self.start.contains(sig) || self.end.contains(sig)
    }
    /// Every signature the sequence names, for validation.
    pub fn all_sigs(&self) -> Vec<[u8; 4]> {
        let mut out: Vec<[u8; 4]> = Vec::new();
        out.extend(self.start.iter().copied());
        out.extend(self.end.iter().copied());
        for b in &self.blocks {
            out.push(b.opener);
            out.extend(b.subrecords.keys().copied());
        }
        out
    }
}

impl RecordDef {
    /// The mask of the header flag with this name (case-insensitive), if the schema
    /// names it for this record type.
    pub fn header_flag(&self, name: &str) -> Option<u32> {
        self.header_flags
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(b, _)| 1u32 << b)
    }

    /// Names of the set bits of `flags` that the schema names, in bit order.
    pub fn header_flag_names(&self, flags: u32) -> Vec<&str> {
        self.header_flags
            .iter()
            .filter(|(b, _)| flags & (1u32 << b) != 0)
            .map(|(_, n)| n.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub types: HashMap<String, TypeDef>,
    pub common: HashMap<[u8; 4], SubDef>,
    pub records: HashMap<[u8; 4], RecordDef>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("schema error: {0}")]
pub struct SchemaError(pub String);

fn sig4(s: &str, ctx: &str) -> Result<[u8; 4], SchemaError> {
    let b = s.as_bytes();
    if b.len() != 4 {
        return Err(SchemaError(format!(
            "{ctx}: signature {s:?} is not 4 bytes"
        )));
    }
    Ok([b[0], b[1], b[2], b[3]])
}

fn parse_type(v: &Value, ctx: &str) -> Result<TypeDef, SchemaError> {
    match v {
        Value::String(s) => Ok(match Prim::parse(s) {
            Some(p) => TypeDef::Prim(p),
            None => TypeDef::Named(s.clone()),
        }),
        Value::Object(_) => {
            if let Some(fields) = v.get("struct") {
                let arr = fields
                    .as_array()
                    .ok_or_else(|| SchemaError(format!("{ctx}: struct must be an array")))?;
                let mut out = Vec::new();
                for (i, f) in arr.iter().enumerate() {
                    let name = f
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| SchemaError(format!("{ctx}: field #{i} needs a name")))?;
                    let ty = parse_type(
                        f.get("type")
                            .ok_or_else(|| SchemaError(format!("{ctx}.{name}: missing type")))?,
                        &format!("{ctx}.{name}"),
                    )?;
                    let optional = f.get("optional").and_then(Value::as_bool).unwrap_or(false);
                    let expose = f.get("expose").and_then(Value::as_bool).unwrap_or(false);
                    out.push(Field {
                        name: name.to_string(),
                        ty,
                        optional,
                        expose,
                    });
                }
                // A struct's JSON form is an object, so two fields may not share a
                // name: the second would silently overwrite the first and the value
                // could not be re-encoded. xEdit does declare the same padding name
                // repeatedly (CSTY CSTD has six `wbUnused` fields), so the duplicates
                // are disambiguated here in declaration order rather than rejected.
                // The byte layout is untouched; only the JSON keys differ.
                let mut seen: HashMap<String, usize> = HashMap::new();
                for f in out.iter_mut() {
                    let n = seen.entry(f.name.clone()).or_insert(0);
                    *n += 1;
                    let suffix = *n;
                    if suffix > 1 {
                        f.name = format!("{}_{}", f.name, suffix);
                    }
                }
                // Only a trailing run of fields may be optional.
                let first_opt = out.iter().position(|f| f.optional).unwrap_or(out.len());
                if out[first_opt..].iter().any(|f| !f.optional) {
                    return Err(SchemaError(format!(
                        "{ctx}: optional fields must all be at the end"
                    )));
                }
                Ok(TypeDef::Struct(out))
            } else if let Some(elem) = v.get("array") {
                let count = match v.get("count") {
                    None => None,
                    Some(c) => Some(
                        c.as_u64()
                            .ok_or_else(|| SchemaError(format!("{ctx}: bad count")))?
                            as usize,
                    ),
                };
                // A count read from the data instead of written in the schema:
                // `{"array": T, "count_prefix": "u32"}`. xEdit's `wbArrayS`/`wbArray`
                // with `Count = -1` (arcU32), used by AlternateTexture, NVCI's
                // reference lists and NAVGD's per-cell triangle arrays. Mutually
                // exclusive with `count`.
                let count_prefix = match v.get("count_prefix") {
                    None => None,
                    Some(c) => Some(
                        c.as_str()
                            .and_then(Prim::parse)
                            .filter(|p| matches!(p, Prim::U8 | Prim::U16 | Prim::U32))
                            .ok_or_else(|| {
                                SchemaError(format!(
                                    "{ctx}: count_prefix must be \"u8\", \"u16\" or \"u32\""
                                ))
                            })?,
                    ),
                };
                if count.is_some() && count_prefix.is_some() {
                    return Err(SchemaError(format!(
                        "{ctx}: array cannot have both \"count\" and \"count_prefix\""
                    )));
                }
                // A count stored in a sibling field of the same struct: xEdit's
                // `SetCountPath('Vertex Count', True)`. The count bytes are read once as
                // an ordinary field and every array naming that field uses it, which is
                // how a struct can hold two different-length arrays behind one pair of
                // count fields. Mutually exclusive with `count` and `count_prefix`.
                let count_field = match v.get("count_field") {
                    None => None,
                    Some(c) => Some(c.as_str().map(str::to_string).ok_or_else(|| {
                        SchemaError(format!("{ctx}: count_field must be a field name string"))
                    })?),
                };
                if count_field.is_some() && (count.is_some() || count_prefix.is_some()) {
                    return Err(SchemaError(format!(
                        "{ctx}: count_field cannot be combined with \"count\" or \"count_prefix\""
                    )));
                }
                Ok(TypeDef::Array {
                    elem: Box::new(parse_type(elem, ctx)?),
                    count,
                    count_prefix,
                    count_field,
                })
            } else if let Some(base) = v.get("enum") {
                let base = base
                    .as_str()
                    .and_then(Prim::parse)
                    .filter(|p| p.is_integer())
                    .ok_or_else(|| {
                        SchemaError(format!("{ctx}: enum base must be an integer type"))
                    })?;
                let mut values = Vec::new();
                if let Some(Value::Object(m)) = v.get("values") {
                    for (k, n) in m {
                        let key: i64 = k
                            .parse()
                            .map_err(|_| SchemaError(format!("{ctx}: enum key {k:?}")))?;
                        values.push((key, n.as_str().unwrap_or("").to_string()));
                    }
                }
                Ok(TypeDef::Enum { base, values })
            } else if let Some(base) = v.get("flags") {
                let base = base
                    .as_str()
                    .and_then(Prim::parse)
                    .filter(|p| p.is_integer())
                    .ok_or_else(|| {
                        SchemaError(format!("{ctx}: flags base must be an integer type"))
                    })?;
                let mut bits = Vec::new();
                if let Some(Value::Object(m)) = v.get("bits") {
                    for (k, n) in m {
                        let bit: u32 = k
                            .parse()
                            .map_err(|_| SchemaError(format!("{ctx}: bit {k:?}")))?;
                        if bit > 63 {
                            return Err(SchemaError(format!("{ctx}: bit {bit} out of range")));
                        }
                        bits.push((bit, n.as_str().unwrap_or("").to_string()));
                    }
                }
                Ok(TypeDef::Flags { base, bits })
            } else if let Some(vs) = v.get("union") {
                let arr = vs
                    .as_array()
                    .ok_or_else(|| SchemaError(format!("{ctx}: union must be an array")))?;
                let mut out = Vec::new();
                for (i, var) in arr.iter().enumerate() {
                    let size = match var.get("size") {
                        None => None,
                        Some(n) => Some(
                            n.as_u64()
                                .ok_or_else(|| SchemaError(format!("{ctx}: union #{i} bad size")))?
                                as usize,
                        ),
                    };
                    let ty = parse_type(
                        var.get("type").ok_or_else(|| {
                            SchemaError(format!("{ctx}: union #{i} missing type"))
                        })?,
                        &format!("{ctx}[{i}]"),
                    )?;
                    out.push((size, ty));
                }
                if out.is_empty() {
                    return Err(SchemaError(format!(
                        "{ctx}: union needs at least one variant"
                    )));
                }
                // `{"union": [...], "switch_on": "field", "cases": {"v": i}}` turns the
                // union into a *value*-decided one: the variant index comes from an
                // earlier sibling field's value rather than from the bytes left
                // (xEdit's `wbUnion` with a decider function). Without `switch_on`
                // this stays the plain size-decided union above.
                // `{"union": [...], "peek": true}` makes the union choose
                // its variant by the field's first word, read without
                // consuming it. The word belongs to the field either way, so
                // the whole field is the union.
                if v.get("peek").and_then(Value::as_bool) == Some(true) {
                    let mut cases = Vec::new();
                    if let Some(Value::Object(m)) = v.get("cases") {
                        for (k, idx) in m {
                            let key: u64 = k
                                .parse()
                                .map_err(|_| SchemaError(format!("{ctx}: case key {k:?}")))?;
                            let i = idx.as_u64().ok_or_else(|| {
                                SchemaError(format!("{ctx}: case {k:?} needs a variant index"))
                            })? as usize;
                            if i >= out.len() {
                                return Err(SchemaError(format!(
                                    "{ctx}: case {k:?} names variant {i}, but there are only {}",
                                    out.len()
                                )));
                            }
                            cases.push((key, i));
                        }
                    }
                    cases.sort_by_key(|(k, _)| *k);
                    let default = v
                        .get("default")
                        .and_then(Value::as_u64)
                        .map(|d| d as usize)
                        .unwrap_or(0);
                    if default >= out.len() {
                        return Err(SchemaError(format!(
                            "{ctx}: default names variant {default}, but there are only {}",
                            out.len()
                        )));
                    }
                    return Ok(TypeDef::PeekUnion {
                        cases,
                        default,
                        variants: out.into_iter().map(|(_, t)| t).collect(),
                    });
                }
                if let Some(field) = v.get("switch_on") {
                    let name = field.as_str().map(str::to_string).ok_or_else(|| {
                        SchemaError(format!("{ctx}: switch_on must be a field name string"))
                    })?;
                    let mut cases = Vec::new();
                    if let Some(Value::Object(m)) = v.get("cases") {
                        for (k, idx) in m {
                            // The key is the value the switch reads, as text:
                            // an integer's decimal form, or the name itself
                            // when the switch reads text.
                            let key: String = k.clone();
                            let i = idx.as_u64().ok_or_else(|| {
                                SchemaError(format!("{ctx}: case {k:?} needs a variant index"))
                            })? as usize;
                            if i >= out.len() {
                                return Err(SchemaError(format!(
                                    "{ctx}: case {k:?} names variant {i}, but there are only {}",
                                    out.len()
                                )));
                            }
                            cases.push((key, i));
                        }
                    }
                    cases.sort_by_key(|(k, _)| k.clone());
                    let default = v
                        .get("default")
                        .and_then(Value::as_u64)
                        .map(|d| d as usize)
                        .unwrap_or(0);
                    if default >= out.len() {
                        return Err(SchemaError(format!(
                            "{ctx}: default names variant {default}, but there are only {}",
                            out.len()
                        )));
                    }
                    return Ok(TypeDef::ValueUnion {
                        name,
                        cases,
                        default,
                        variants: out.into_iter().map(|(_, t)| t).collect(),
                    });
                }
                Ok(TypeDef::Union(out))
            } else {
                Err(SchemaError(format!("{ctx}: unrecognised type object")))
            }
        }
        _ => Err(SchemaError(format!(
            "{ctx}: type must be a string or object"
        ))),
    }
}

fn parse_subdef(v: &Value, ctx: &str) -> Result<SubDef, SchemaError> {
    let count_from = match v.get("count_from").and_then(Value::as_str) {
        Some(sig) => Some(sig4(sig, ctx)?),
        None => None,
    };
    Ok(SubDef {
        count_from,
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ty: parse_type(
            v.get("type")
                .ok_or_else(|| SchemaError(format!("{ctx}: missing type")))?,
            ctx,
        )?,
        confidence: v
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("hypothesis")
            .to_string(),
        note: v.get("note").and_then(Value::as_str).map(str::to_string),
        expose: v.get("expose").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Named header flag bits (sorted) and their confidence.
type HeaderFlags = (Vec<(u32, String)>, Option<String>);

/// The definitions for a record's wildcard signatures.
///
/// A key that ends in `*` names a suffix rather than a whole signature:
/// Skyrim's `WTHR` has one cloud texture per layer, and layer *n* uses the
/// signature `chr(ord('0') + n)` followed by `0TX`, so the layers run
/// `00TX`, `10TX`, `20TX` and on past the digits into the punctuation and
/// the letters.
fn parse_wildcards(
    subs: Option<&Value>,
    label: &str,
    rec: &str,
) -> Result<Vec<(Vec<u8>, SubDef)>, SchemaError> {
    let mut out = Vec::new();
    if let Some(Value::Object(sm)) = subs {
        for (ss, sd) in sm {
            let Some(suffix) = ss.strip_prefix('*') else {
                continue;
            };
            // The suffix is right-aligned in the signature, so it is compared
            // against the signature's trailing bytes.
            let b = suffix.as_bytes();
            if b.is_empty() || b.len() > 4 {
                return Err(SchemaError(format!(
                    "{label}:{rec}.{ss}: a wildcard suffix must be 1 to 4 bytes"
                )));
            }
            out.push((
                b.to_vec(),
                parse_subdef(sd, &format!("{label}:{rec}.{ss}"))?,
            ));
        }
    }
    Ok(out)
}

fn parse_header_flags(v: Option<&Value>, ctx: &str) -> Result<HeaderFlags, SchemaError> {
    let Some(v) = v else {
        return Ok((Vec::new(), None));
    };
    let bits = v
        .get("bits")
        .and_then(Value::as_object)
        .ok_or_else(|| SchemaError(format!("{ctx}: needs a \"bits\" object")))?;
    let mut out = Vec::new();
    for (k, n) in bits {
        let bit: u32 = k
            .parse()
            .map_err(|_| SchemaError(format!("{ctx}: bit {k:?}")))?;
        if bit > 31 {
            return Err(SchemaError(format!(
                "{ctx}: bit {bit} out of range (the field is a u32)"
            )));
        }
        let name = n
            .as_str()
            .filter(|n| !n.is_empty())
            .ok_or_else(|| SchemaError(format!("{ctx}: bit {bit} needs a name")))?;
        out.push((bit, name.to_string()));
    }
    out.sort();
    Ok((
        out,
        v.get("confidence")
            .and_then(Value::as_str)
            .map(str::to_string),
    ))
}

/// A record entry's `sequence`: `{"start": [...], "blocks": {"name": "SIG"}, "end": [...]}`.
///
/// `blocks` maps a block's *name* to the signature that opens it, which is the
/// only place a block's identity lives: the subrecords inside it are decoded by
/// the ordinary `subrecords` map, and the same signature may appear in several
/// blocks. Two blocks may not share an opener signature, and an opener may not
/// also be a start or end signature, because then it would be ambiguous whether
/// a block had opened.
fn parse_sequence(v: Option<&Value>, ctx: &str) -> Result<Option<SequenceDef>, SchemaError> {
    let Some(v) = v else { return Ok(None) };
    let sigs = |key: &str| -> Result<Vec<[u8; 4]>, SchemaError> {
        let Some(a) = v.get(key) else {
            return Ok(Vec::new());
        };
        let a = a
            .as_array()
            .ok_or_else(|| SchemaError(format!("{ctx}: {key} must be an array")))?;
        let mut out = Vec::new();
        for s in a {
            let s = s
                .as_str()
                .ok_or_else(|| SchemaError(format!("{ctx}: {key} entries must be strings")))?;
            out.push(sig4(s, ctx)?);
        }
        Ok(out)
    };
    let start = sigs("start")?;
    let end = sigs("end")?;
    // Each block is an object: `{"opener": "SIG", "order": [...], "subrecords": {...}}`.
    // The order of the blocks in the file is the order their openers first
    // appear, which is what the decoder walks.
    let mut blocks = Vec::new();
    if let Some(Value::Object(m)) = v.get("blocks") {
        for (name, b) in m {
            let bctx = format!("{ctx}: block {name}");
            let opener = b
                .get("opener")
                .and_then(Value::as_str)
                .ok_or_else(|| SchemaError(format!("{bctx}: needs an \"opener\" signature")))?;
            let mut subs = HashMap::new();
            if let Some(Value::Object(sm)) = b.get("subrecords") {
                for (ss, sd) in sm {
                    subs.insert(sig4(ss, &bctx)?, parse_subdef(sd, &format!("{bctx}.{ss}"))?);
                }
            }
            // `variants` chooses the block's subrecords by the opener's value.
            let mut variants = Vec::new();
            if let Some(Value::Object(vm)) = b.get("variants") {
                for (v, vd) in vm {
                    let vctx = format!("{bctx}.variants.{v}");
                    let mut vsubs = HashMap::new();
                    if let Some(Value::Object(sm)) = vd.get("subrecords") {
                        for (ss, sd) in sm {
                            vsubs.insert(
                                sig4(ss, &vctx)?,
                                parse_subdef(sd, &format!("{vctx}.{ss}"))?,
                            );
                        }
                    }
                    variants.push((v.clone(), vsubs));
                }
            }
            blocks.push(BlockDef {
                name: name.clone(),
                opener: sig4(opener, &bctx)?,
                subrecords: subs,
                order: sigs_of(b, "order", &bctx)?,
                variants,
            });
        }
    }
    let seq = SequenceDef { start, blocks, end };
    for b in &seq.blocks {
        if seq.start.contains(&b.opener) || seq.end.contains(&b.opener) {
            return Err(SchemaError(format!(
                "{ctx}: block opener {:?} may not also be a start or end signature",
                b.opener
            )));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for b in &seq.blocks {
        if !seen.insert(b.opener) {
            return Err(SchemaError(format!(
                "{ctx}: block opener {:?} names two blocks",
                b.opener
            )));
        }
    }
    Ok(Some(seq))
}

/// The signatures in `obj`'s `key` array, as 4-byte tags.
fn sigs_of(obj: &Value, key: &str, ctx: &str) -> Result<Vec<[u8; 4]>, SchemaError> {
    let Some(a) = obj.get(key) else {
        return Ok(Vec::new());
    };
    let a = a
        .as_array()
        .ok_or_else(|| SchemaError(format!("{ctx}: {key} must be an array")))?;
    let mut out = Vec::new();
    for s in a {
        let s = s
            .as_str()
            .ok_or_else(|| SchemaError(format!("{ctx}: {key} entries must be strings")))?;
        out.push(sig4(s, ctx)?);
    }
    Ok(out)
}

impl Schema {
    /// The mask of the named record-header flag for record type `rec`, if the schema
    /// names it (`header_flags`).
    pub fn header_flag(&self, rec: &[u8; 4], name: &str) -> Option<u32> {
        self.records.get(rec)?.header_flag(name)
    }

    /// Load and merge every `*.json` file under `dir` (sorted by path).
    ///
    /// Kept for a single flat directory — `schemas/base` on its own, a test
    /// fixture, or a tool that takes one schema dir. A *game* is loaded with
    /// [`Schema::load_game`], which layers the game's differences over the base.
    ///
    /// A game directory carries a symlink to the base (`schemas/fnv/base ->
    /// ../base`) so it can be read on its own as well as layered. Reading one
    /// therefore has to load the base's files **first** and then let the
    /// directory's own files replace the records they name — the same overlay
    /// rule [`Schema::load_game`] uses. Doing it the other way round leaves a
    /// game directory unreadable on its own, because its own files redefine
    /// records the base already defines and `merge_str` rejects a duplicate.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Schema, SchemaError> {
        let dir = dir.as_ref();
        // Is `dir` a game directory? It is when it holds a `base` entry that
        // resolves outside itself — the symlink. Then the base loads first and
        // this directory's own files overlay it.
        let linked_base = dir.join("base");
        let base = if linked_base.is_dir()
            && linked_base
                .canonicalize()
                .map(|c| c != dir.canonicalize().unwrap_or_default().join("base"))
                .unwrap_or(false)
        {
            Some(linked_base.canonicalize().unwrap_or(linked_base))
        } else {
            None
        };
        let mut s = match &base {
            Some(b) => Schema::load_dir(b)?,
            None => Schema::default(),
        };
        let mut files = Vec::new();
        collect_json(dir, &mut files)
            .map_err(|e| SchemaError(format!("{}: {e}", dir.display())))?;
        files.sort();
        if files.is_empty() {
            return Err(SchemaError(format!("no schema files in {}", dir.display())));
        }
        let base_canon = base.as_deref();
        for f in &files {
            // A file reached through the base symlink is already loaded.
            if let Some(b) = base_canon {
                if f.canonicalize().map(|c| c.starts_with(b)).unwrap_or(false) {
                    continue;
                }
            }
            let text = std::fs::read_to_string(f)
                .map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
            if base_canon.is_some() {
                s.merge_overlay_str(&text, &f.display().to_string())?;
            } else {
                s.merge_str(&text, &f.display().to_string())?;
            }
        }
        s.validate()?;
        Ok(s)
    }

    /// Load the shared `base` directory, then every game directory over it.
    ///
    /// This is how a game's schema is assembled, and the split is the point: the
    /// base holds the container, the group tree, the FormID model and every
    /// layout the Gamebryo engines share, and a game directory holds only what
    /// that game does differently. A game directory that is missing or empty is
    /// **not** an error — it means the game shares every layout it uses — so the
    /// result is simply the base. Only a missing *base* is an error, because
    /// without it there is nothing to overlay onto.
    ///
    /// `games` are applied in order, so a later one wins over an earlier one.
    /// Each one may redefine records the base already holds, and may redefine
    /// records an earlier game directory defined; within one directory the rules
    /// of [`Schema::merge_str`] still apply, so a record defined twice in the
    /// same directory is still an error.
    pub fn load_game(
        base: impl AsRef<Path>,
        games: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Schema, SchemaError> {
        let base = base.as_ref();
        let mut s = Schema::load_dir(base)?;
        for g in games {
            let g = g.as_ref();
            if !g.is_dir() {
                // A game with no directory shares every layout the base has.
                continue;
            }
            s.merge_over_dir(g, base, false)?;
        }
        s.validate()?;
        Ok(s)
    }

    /// Merge every `*.json` file under `dir` (sorted by path).
    ///
    /// `strict` selects the duplicate-record rule: `true` rejects a record the
    /// schema already holds (one directory's own files must not repeat), `false`
    /// replaces it (an overlay directory may redefine).
    ///
    /// A game directory commonly holds a symlink to the base so its files are
    /// reachable through both paths. Those are already loaded, and merging them
    /// twice would fail on the first duplicate type — so a file whose canonical
    /// path is *inside the base* is skipped. The comparison must be against the
    /// base, not against `dir` itself: comparing against `dir` marks every file
    /// of the overlay as "already loaded" and silently merges nothing.
    fn merge_over_dir(&mut self, dir: &Path, base: &Path, strict: bool) -> Result<(), SchemaError> {
        let mut files = Vec::new();
        collect_json(dir, &mut files)
            .map_err(|e| SchemaError(format!("{}: {e}", dir.display())))?;
        files.sort();
        let base_canon = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        for f in &files {
            let is_base = f
                .canonicalize()
                .map(|c| c.starts_with(&base_canon))
                .unwrap_or(false);
            if is_base {
                continue;
            }
            let text = std::fs::read_to_string(f)
                .map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
            if strict {
                self.merge_str(&text, &f.display().to_string())?;
            } else {
                self.merge_overlay_str(&text, &f.display().to_string())?;
            }
        }
        Ok(())
    }

    /// Load `base`, then let `overlay`'s files merge **over** it.
    ///
    /// [`Schema::load_dir`] rejects a record defined twice, which is right for a
    /// single game's files — a layout in two places is a bug waiting to happen.
    /// A second game is a different situation: Fallout 3's container is Fallout:
    /// New Vegas's container plus a subrecord, and restating the other 103
    /// records to change four would drift the moment FNV's own schema moves.
    ///
    /// So an overlay *replaces* the records it names and leaves the rest of the
    /// base's alone. A record named here is defined in full, because a
    /// replacement is not a merge: the base's subrecords for that record are
    /// dropped, which is why an overlay file carries a complete record.
    ///
    /// Both directories are read recursively, so `schemas/fo3` holds its own
    /// files plus a symlink to `schemas/fnv` and the two load as one.
    ///
    /// [`Schema::load_game`] is the multi-game form; this is the two-directory
    /// convenience wrapper the `coverage` and `rtcheck` tools take.
    pub fn load_overlay(
        base: impl AsRef<Path>,
        overlay: impl AsRef<Path>,
    ) -> Result<Schema, SchemaError> {
        let base = base.as_ref();
        let mut s = Schema::load_dir(base)?;
        let overlay = overlay.as_ref();
        if overlay.is_dir() {
            s.merge_over_dir(overlay, base, false)?;
        }
        s.validate()?;
        Ok(s)
    }

    pub fn merge_str(&mut self, text: &str, label: &str) -> Result<(), SchemaError> {
        let v = parse_strict(text).map_err(|e| SchemaError(format!("{label}: {e}")))?;
        if v.get("format").and_then(Value::as_str) != Some("fnv.schema/1") {
            return Err(SchemaError(format!(
                "{label}: missing \"format\": \"fnv.schema/1\""
            )));
        }
        if let Some(Value::Object(types)) = v.get("types") {
            for (name, t) in types {
                // A type in an overlay *replaces* the one it shadows. A layout
                // that is not the same in every game cannot live in the base,
                // so the game that differs supplies its own and the base's
                // copy is overwritten.
                self.types.insert(
                    name.clone(),
                    parse_type(t, &format!("{label}:types.{name}"))?,
                );
            }
        }
        if let Some(Value::Object(common)) = v.get("common") {
            for (sig, d) in common {
                let k = sig4(sig, label)?;
                // A common subrecord in an overlay replaces the one it shadows,
                // for the same reason a type does: a subrecord whose layout is
                // not the same in every game belongs to the game, not the base.
                self.common
                    .insert(k, parse_subdef(d, &format!("{label}:common.{sig}"))?);
            }
        }
        if let Some(Value::Object(recs)) = v.get("records") {
            for (sig, d) in recs {
                let k = sig4(sig, label)?;
                // A record in an overlay *replaces* the one it shadows, so the
                // overlay's copy has to be complete. A record that says
                // `"merge": true` instead adds to the one it shadows: its own
                // fields are added or replace their counterparts, and the
                // shadowed record's other fields stay. That is what a game
                // needs when only some of a record's fields differ.
                let mut subs = HashMap::new();
                if let Some(Value::Object(sm)) = d.get("subrecords") {
                    for (ss, sd) in sm {
                        // A wildcard key names a suffix, not a whole
                        // signature, so it does not go in the fixed map.
                        if ss.starts_with('*') {
                            continue;
                        }
                        subs.insert(
                            sig4(ss, label)?,
                            parse_subdef(sd, &format!("{label}:{sig}.{ss}"))?,
                        );
                    }
                }
                let wildcards = parse_wildcards(d.get("subrecords"), label, sig)?;
                let (header_flags, header_flags_confidence) = parse_header_flags(
                    d.get("header_flags"),
                    &format!("{label}:{sig}.header_flags"),
                )?;
                let rd = RecordDef {
                    name: d
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    confidence: d
                        .get("confidence")
                        .and_then(Value::as_str)
                        .unwrap_or("hypothesis")
                        .to_string(),
                    subrecords: subs,
                    wildcards,
                    header_flags,
                    header_flags_confidence,
                    sequence: parse_sequence(
                        d.get("sequence"),
                        &format!("{label}:{sig}.sequence"),
                    )?,
                };
                if self.records.insert(k, rd).is_some() {
                    return Err(SchemaError(format!("{label}: record {sig} defined twice")));
                }
            }
        }
        self.files.push(label.to_string());
        Ok(())
    }

    /// [`Schema::merge_str`] for a file that may redefine records already loaded.
    ///
    /// The only difference is the last check: a record the schema already holds
    /// is **replaced** rather than rejected. Everything else — the format tag,
    /// the types, the common subrecords — stays strict, so a typo in an overlay
    /// is still an error rather than a silent second definition.
    ///
    /// A replacement is whole, not a merge: the base's subrecords for that
    /// record are dropped, so an overlay file names every subrecord its record
    /// has. That is the point — an overlay that only listed the differences
    /// would silently delete the subrecords it did not repeat.
    pub fn merge_overlay_str(&mut self, text: &str, label: &str) -> Result<(), SchemaError> {
        let v = parse_strict(text).map_err(|e| SchemaError(format!("{label}: {e}")))?;
        if v.get("format").and_then(Value::as_str) != Some("fnv.schema/1") {
            return Err(SchemaError(format!(
                "{label}: missing \"format\": \"fnv.schema/1\""
            )));
        }
        if let Some(Value::Object(types)) = v.get("types") {
            for (name, t) in types {
                // A type in an overlay *replaces* the one it shadows. A layout
                // that is not the same in every game cannot live in the base,
                // so the game that differs supplies its own and the base's
                // copy is overwritten.
                self.types.insert(
                    name.clone(),
                    parse_type(t, &format!("{label}:types.{name}"))?,
                );
            }
        }
        if let Some(Value::Object(common)) = v.get("common") {
            for (sig, d) in common {
                let k = sig4(sig, label)?;
                // A common subrecord in an overlay replaces the one it shadows,
                // for the same reason a type does: a subrecord whose layout is
                // not the same in every game belongs to the game, not the base.
                self.common
                    .insert(k, parse_subdef(d, &format!("{label}:common.{sig}"))?);
            }
        }
        if let Some(Value::Object(recs)) = v.get("records") {
            for (sig, d) in recs {
                let k = sig4(sig, label)?;
                // A record that says `"merge": true` adds to the record it
                // shadows rather than replacing it: its own fields are added
                // or replace their counterparts, and the shadowed record's
                // other fields stay.
                let merge = d.get("merge").and_then(Value::as_bool) == Some(true);
                let mut subs = HashMap::new();
                if let Some(Value::Object(sm)) = d.get("subrecords") {
                    for (ss, sd) in sm {
                        // A wildcard key names a suffix, not a whole
                        // signature, so it does not go in the fixed map.
                        if ss.starts_with('*') {
                            continue;
                        }
                        subs.insert(
                            sig4(ss, label)?,
                            parse_subdef(sd, &format!("{label}:{sig}.{ss}"))?,
                        );
                    }
                }
                let wildcards = parse_wildcards(d.get("subrecords"), label, sig)?;
                let (header_flags, header_flags_confidence) = parse_header_flags(
                    d.get("header_flags"),
                    &format!("{label}:{sig}.header_flags"),
                )?;
                let rd = RecordDef {
                    name: d
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    confidence: d
                        .get("confidence")
                        .and_then(Value::as_str)
                        .unwrap_or("hypothesis")
                        .to_string(),
                    subrecords: subs,
                    wildcards,
                    header_flags,
                    header_flags_confidence,
                    sequence: parse_sequence(
                        d.get("sequence"),
                        &format!("{label}:{sig}.sequence"),
                    )?,
                };
                if merge {
                    // The shadowed record's other fields stay, and this one's
                    // own fields replace their counterparts.
                    match self.records.get_mut(&k) {
                        Some(prev) => {
                            prev.subrecords.extend(rd.subrecords);
                            prev.wildcards.extend(rd.wildcards);
                            if rd.header_flags.is_empty() {
                                // keep the shadowed record's header flags
                            } else {
                                prev.header_flags = rd.header_flags;
                                prev.header_flags_confidence = rd.header_flags_confidence;
                            }
                            if rd.sequence.is_some() {
                                prev.sequence = rd.sequence;
                            }
                            if !rd.name.is_empty() {
                                prev.name = rd.name;
                            }
                            if !rd.confidence.is_empty() {
                                prev.confidence = rd.confidence;
                            }
                        }
                        None => {
                            self.records.insert(k, rd);
                        }
                    }
                } else {
                    self.records.insert(k, rd);
                }
            }
        }
        self.files.push(label.to_string());
        Ok(())
    }

    /// Every named type reference must resolve, without cycles.
    pub fn validate(&self) -> Result<(), SchemaError> {
        fn check(s: &Schema, t: &TypeDef, depth: usize, ctx: &str) -> Result<(), SchemaError> {
            if depth > 32 {
                return Err(SchemaError(format!(
                    "{ctx}: type nesting too deep (cycle?)"
                )));
            }
            match t {
                TypeDef::Prim(_) | TypeDef::Enum { .. } | TypeDef::Flags { .. } => Ok(()),
                TypeDef::PeekUnion { variants, .. } => {
                    for v in variants {
                        check(s, v, depth + 1, ctx)?;
                    }
                    Ok(())
                }
                TypeDef::Named(n) => match s.types.get(n) {
                    Some(t2) => check(s, t2, depth + 1, ctx),
                    None => Err(SchemaError(format!("{ctx}: unknown type {n:?}"))),
                },
                TypeDef::Struct(fs) => {
                    for f in fs {
                        check(s, &f.ty, depth + 1, ctx)?;
                        // A count-field array names a sibling integer field decoded
                        // earlier in the same struct. A dangling or self-referential
                        // name is a schema bug, not a data problem.
                        if let TypeDef::Array {
                            count_field: Some(name),
                            ..
                        } = &f.ty
                        {
                            if name == &f.name {
                                return Err(SchemaError(format!(
                                    "{ctx}.{}: count_field cannot name itself",
                                    f.name
                                )));
                            }
                            let declared_earlier = fs
                                .iter()
                                .take_while(|p| p.name != f.name)
                                .any(|p| p.name == *name);
                            if !declared_earlier {
                                return Err(SchemaError(format!(
                                    "{ctx}.{}: count_field {name:?} must name an earlier field of the same struct",
                                    f.name
                                )));
                            }
                        }
                    }
                    Ok(())
                }
                TypeDef::Array { elem, .. } => check(s, elem, depth + 1, ctx),
                TypeDef::Union(vs) => vs.iter().try_for_each(|(_, t)| check(s, t, depth + 1, ctx)),
                TypeDef::ValueUnion { variants, .. } => variants
                    .iter()
                    .try_for_each(|t| check(s, t, depth + 1, ctx)),
            }
        }
        for (n, t) in &self.types {
            check(self, t, 0, &format!("types.{n}"))?;
        }
        for (k, d) in &self.common {
            check(self, &d.ty, 0, &format!("common.{}", fnv_util::tag_str(k)))?;
        }
        for (rk, r) in &self.records {
            for (sk, d) in &r.subrecords {
                check(
                    self,
                    &d.ty,
                    0,
                    &format!("{}.{}", fnv_util::tag_str(rk), fnv_util::tag_str(sk)),
                )?;
            }
        }
        Ok(())
    }

    /// The definition that applies to `sub` inside `rec` record.
    pub fn lookup(&self, rec: &[u8; 4], sub: &[u8; 4]) -> Option<&SubDef> {
        if let Some(r) = self.records.get(rec) {
            if let Some(d) = r.subrecords.get(sub) {
                return Some(d);
            }
            if let Some(d) = r.wildcards.iter().find(|(suffix, _)| {
                // The suffix is right-aligned in the signature, so it is
                // compared against the signature's trailing bytes.
                suffix.len() <= sub.len() && sub[sub.len() - suffix.len()..] == suffix[..]
            }) {
                return Some(&d.1);
            }
        }
        self.common.get(sub)
    }

    /// The definition that applies to `sub` inside `rec`, given the sequence
    /// block the decoder is in.
    ///
    /// A block's own definitions win over the record's: a signature listed in a
    /// block means that block's version of it, which is the whole point of a
    /// sequence. A signature the block does not define falls back to the
    /// record's own map, and then to `common`.
    pub fn lookup_in_block(
        &self,
        rec: &[u8; 4],
        sub: &[u8; 4],
        block: Option<usize>,
    ) -> Option<&SubDef> {
        if let Some(b) = block {
            if let Some(seq) = self.records.get(rec).and_then(|r| r.sequence.as_ref()) {
                if let Some(bd) = seq.blocks.get(b) {
                    if let Some(d) = bd.subrecords.get(sub) {
                        return Some(d);
                    }
                }
            }
        }
        self.lookup(rec, sub)
    }

    pub fn resolve<'a>(&'a self, t: &'a TypeDef) -> &'a TypeDef {
        let mut t = t;
        let mut n = 0;
        while let TypeDef::Named(name) = t {
            t = &self.types[name];
            n += 1;
            if n > 32 {
                break;
            }
        }
        t
    }

    /// Fixed byte size of a type, or None if variable.
    pub fn fixed_size(&self, t: &TypeDef) -> Option<usize> {
        match self.resolve(t) {
            TypeDef::Prim(p) => p.size(),
            TypeDef::Enum { base, .. } | TypeDef::Flags { base, .. } => base.size(),
            TypeDef::PeekUnion { variants, .. } => {
                // The variants have different shapes, so the union has no
                // fixed size unless they all agree.
                let mut n = None;
                for v in variants {
                    let s = self.fixed_size(v)?;
                    match n {
                        None => n = Some(s),
                        Some(prev) if prev == s => {}
                        Some(_) => return None,
                    }
                }
                n
            }
            TypeDef::ValueUnion { variants, .. } => {
                // The variant is chosen by a sibling's value, so the union
                // has a fixed size only when its variants all agree.
                let mut n = None;
                for v in variants {
                    let s = self.fixed_size(v)?;
                    match n {
                        None => n = Some(s),
                        Some(prev) if prev == s => {}
                        Some(_) => return None,
                    }
                }
                n
            }
            TypeDef::Struct(fs) => {
                let mut n = 0;
                for f in fs {
                    if f.optional {
                        return None;
                    }
                    n += self.fixed_size(&f.ty)?;
                }
                Some(n)
            }
            TypeDef::Array {
                elem,
                count: Some(c),
                count_prefix: None,
                count_field: None,
            } => Some(self.fixed_size(elem)? * c),
            // A fill-the-rest array, a count-prefixed array, a count-field array and a
            // malformed array carrying both `count` and `count_prefix` (rejected at
            // parse time) all have no size known before reading the data.
            TypeDef::Array { .. } => None,
            TypeDef::Named(_) | TypeDef::Union(_) => None,
        }
    }
}

/// Parse JSON like `serde_json::from_str`, but reject duplicate object keys: in a
/// schema file a repeated key would otherwise silently replace a definition.
pub fn parse_strict(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<Strict>(text).map(|s| s.0)
}

struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(StrictVisitor).map(Strict)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(Strict(v)) = seq.next_element()? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            let Strict(v) = map.next_value()?;
            if out.insert(k.clone(), v).is_some() {
                return Err(de::Error::custom(format!("duplicate key {k:?}")));
            }
        }
        Ok(Value::Object(out))
    }
}

fn collect_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect_json(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = r#"{"format":"fnv.schema/1",
      "types":{"Bounds":{"struct":[{"name":"x1","type":"i16"},{"name":"y1","type":"i16"}]}},
      "common":{"EDID":{"name":"Editor ID","type":"zstring","confidence":"observed"}},
      "records":{"GLOB":{"name":"Global","subrecords":{"FLTV":{"name":"Value","type":"f32"},
         "OBND":{"name":"Bounds","type":"Bounds"}}}}}"#;

    #[test]
    fn load_and_lookup() {
        let mut s = Schema::default();
        s.merge_str(S, "t").unwrap();
        s.validate().unwrap();
        assert_eq!(s.lookup(b"GLOB", b"EDID").unwrap().name, "Editor ID");
        assert_eq!(s.lookup(b"GLOB", b"FLTV").unwrap().name, "Value");
        assert!(s.lookup(b"WEAP", b"FLTV").is_none());
        assert_eq!(
            s.fixed_size(&s.lookup(b"GLOB", b"OBND").unwrap().ty),
            Some(4)
        );
    }

    #[test]
    fn rejects_bad_schemas() {
        let mut s = Schema::default();
        assert!(s.merge_str(r#"{"records":{}}"#, "nofmt").is_err());
        let mut s = Schema::default();
        s.merge_str(
            r#"{"format":"fnv.schema/1","common":{"EDID":{"type":"Nope"}}}"#,
            "x",
        )
        .unwrap();
        assert!(s.validate().is_err());
        let mut s = Schema::default();
        let opt_mid = r#"{"format":"fnv.schema/1","types":{"T":{"struct":[{"name":"a","type":"u8","optional":true},{"name":"b","type":"u8"}]}}}"#;
        assert!(s.merge_str(opt_mid, "x").is_err());
        let mut s = Schema::default();
        assert!(s
            .merge_str(
                r#"{"format":"fnv.schema/1","common":{"TOOLONG":{"type":"u8"}}}"#,
                "x"
            )
            .is_err());
        let mut s = Schema::default();
        let dup =
            r#"{"format":"fnv.schema/1","common":{"EDID":{"type":"u8"},"EDID":{"type":"u16"}}}"#;
        assert!(s
            .merge_str(dup, "x")
            .unwrap_err()
            .0
            .contains("duplicate key"));
    }

    #[test]
    fn header_flags() {
        let mut s = Schema::default();
        let t = r#"{"format":"fnv.schema/1","records":{"MISC":{"name":"Misc",
          "header_flags":{"bits":{"10":"Quest Item","5":"Deleted"},"confidence":"documented"},"subrecords":{}}}}"#;
        s.merge_str(t, "t").unwrap();
        assert_eq!(s.header_flag(b"MISC", "quest item"), Some(0x400));
        assert_eq!(s.header_flag(b"MISC", "Nope"), None);
        assert_eq!(s.header_flag(b"KEYM", "Quest Item"), None);
        let r = &s.records[b"MISC"];
        assert_eq!(r.header_flag_names(0x420), vec!["Deleted", "Quest Item"]);
        assert_eq!(r.header_flags_confidence.as_deref(), Some("documented"));
        for bad in [
            r#"{"bits":{"32":"X"}}"#,
            r#"{"bits":{"x":"X"}}"#,
            r#"{"bits":{"1":""}}"#,
            r#"{}"#,
        ] {
            let t = format!(
                r#"{{"format":"fnv.schema/1","records":{{"MISC":{{"header_flags":{bad}}}}}}}"#
            );
            assert!(Schema::default().merge_str(&t, "x").is_err(), "{bad}");
        }
    }
}
