//! Schema-driven decoding of subrecords into JSON values, plus coverage
//! accounting. Unknown or mis-sized data is preserved (hex) and reported, never
//! dropped (spec section 7).

use crate::raw::{Record, Subrecord};
use crate::schema::{Prim, Schema, TypeDef};
use fnv_util::{cp1252, sha256::to_hex, Cursor};
use rayon::prelude::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubStatus {
    /// Decoded and consumed exactly.
    Ok,
    /// No definition for this signature in this record type.
    Unknown,
    /// Decoded, but bytes were left over (layout probably incomplete).
    Trailing,
    /// Data too short or otherwise inconsistent with the definition.
    Error,
}

impl SubStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubStatus::Ok => "ok",
            SubStatus::Unknown => "unknown",
            SubStatus::Trailing => "trailing",
            SubStatus::Error => "error",
        }
    }
}

pub struct DecodeCtx<'a> {
    pub schema: &'a Schema,
    /// Called with every decoded FormID value (file-local), for reference checks.
    pub on_formid: Option<&'a mut dyn FnMut(u32)>,
    /// Values a later subrecord may switch on: `(subrecord signature, field
    /// name) -> integer value`. A struct field marked `"expose": true` is
    /// recorded here, so a sibling subrecord's union can select its variant by
    /// that value (xEdit's `wbUnion` deciders that read another subrecord of the
    /// same record, e.g. `wbPerkDATADecider` reading PRKE's Type, or
    /// `wbEPFDDecider` reading EPFT). Populated in subrecord order, so the
    /// switch target must be decoded before the subrecord that switches on it.
    pub exposed: HashMap<([u8; 4], String), SwitchValue>,
    /// The record currently being decoded, for `exposed` lookups.
    pub rec_sig: [u8; 4],
    /// The subrecord currently being decoded, for `exposed` lookups.
    ///
    /// A field of a subrecord's own struct is exposed under its own name,
    /// but a field of a *struct* that is the whole of a subrecord is
    /// exposed under the subrecord's signature and the field's name —
    /// "PRKE.Type" — so that a later subrecord can switch on it
    /// (xEdit's `wbPerkDATADecider` reading the type in the perk's PRKE).
    pub sub_sig: Option<[u8; 4]>,
    /// Field values decoded earlier in the *same* struct, for an in-struct
    /// `switch_on` (e.g. a nested `wbUnion` that reads a sibling field of its
    /// own struct rather than another subrecord).
    pub switch_on: Option<HashMap<String, SwitchValue>>,
    /// The sequence block the decoder is currently inside, when the record has
    /// a `sequence`.
    ///
    /// A record whose subrecords are an ordered list of blocks gives the same
    /// signature different meanings in different blocks — `ANAM` is a formid in
    /// `ASPC` and a string in `PACK`'s activity list — so the definition used
    /// depends on which block the subrecord is in. This is the block's index
    /// into the record's `sequence.blocks`.
    pub block: Option<usize>,
    /// The decoded value of the block's opener, when the block chooses its
    /// layout by that value (`PACK`'s `ANAM` names the activity's type).
    ///
    /// The walker sets it while it decodes the opener, and reads it to pick
    /// the block's variant.
    pub block_value: Option<String>,
    /// The subrecord definitions of the current block's chosen variant, when
    /// the block has variants keyed by the opener's value.
    pub block_variant: Option<HashMap<[u8; 4], crate::schema::SubDef>>,
    /// The value each counted subrecord's element count came from.
    ///
    /// A subrecord can take its element count from a sibling: Skyrim's `KSIZ`
    /// holds the number of keywords and `KWDA` holds them all. The count is
    /// keyed by the subrecord that holds the count.
    pub counts: HashMap<[u8; 4], u64>,
    /// The sibling subrecord that holds the element count for the subrecord
    /// being decoded, when it has one.
    pub count_from: Option<[u8; 4]>,
    /// Whether the file the record came from is *localized*.
    ///
    /// A localized file holds its translatable strings as 4-byte string IDs
    /// rather than as text, and the flag is a property of the file — the TES4
    /// header's bit 7 — not of the record. Fallout 4's shipped masters all set
    /// it [documented: xEdit `wbDefinitionsFO4.pas`, `wbRecord(TES4, ...)`
    /// names bit 7 "Localized"; observed: all nine shipped masters and ESLs
    /// carry it].
    ///
    /// The [`Prim::Localized`] type reads one or the other according to this
    /// flag, so the same schema describes a localized master and an ordinary
    /// plugin.
    pub localized: bool,
}

impl<'a> DecodeCtx<'a> {
    /// A fresh per-record context: no FormID callback, nothing exposed yet.
    pub fn new(schema: &'a Schema, rec_sig: [u8; 4]) -> Self {
        DecodeCtx {
            schema,
            on_formid: None,
            exposed: HashMap::new(),
            rec_sig,
            sub_sig: None,
            switch_on: None,
            block: None,
            block_value: None,
            block_variant: None,
            counts: HashMap::new(),
            count_from: None,
            localized: false,
        }
    }
}

pub fn formid_str(v: u32) -> String {
    format!("0x{v:08X}")
}

fn read_prim(p: Prim, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => Value::from(c.u8().map_err(e)?),
        Prim::U16 => Value::from(c.u16().map_err(e)?),
        Prim::U32 => Value::from(c.u32().map_err(e)?),
        Prim::U64 => Value::from(c.u64().map_err(e)?),
        Prim::I8 => Value::from(c.i8().map_err(e)?),
        Prim::I16 => Value::from(c.i16().map_err(e)?),
        Prim::I32 => Value::from(c.i32().map_err(e)?),
        Prim::I64 => Value::from(c.i64().map_err(e)?),
        Prim::F32 => Value::from(c.f32().map_err(e)? as f64),
        Prim::F64 => Value::from(c.f64().map_err(e)?),
        Prim::PeekU32 => {
            // The first four bytes, read without consuming them, so a union
            // can pick the field's shape before the field is read.
            if c.remaining() < 4 {
                return Err(format!(
                    "peek_u32: read of 4 bytes at offset {} exceeds length {}",
                    c.pos,
                    c.data.len()
                ));
            }
            let b = &c.data[c.pos..c.pos + 4];
            Value::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64)
        }
        Prim::FormId => {
            let v = c.u32().map_err(e)?;
            if let Some(f) = ctx.on_formid.as_mut() {
                f(v);
            }
            Value::from(formid_str(v))
        }
        Prim::ZString => {
            let rest = c.bytes(c.remaining()).unwrap();
            if rest.last() != Some(&0) && !rest.is_empty() {
                // Not terminated: keep the text but flag it via an error so the
                // definition gets reviewed.
                return Err(format!(
                    "zstring not NUL-terminated: {:?}",
                    cp1252::decode(rest)
                ));
            }
            Value::from(cp1252::decode_z(rest))
        }
        Prim::ZStringArray => {
            let rest = c.bytes(c.remaining()).unwrap();
            if rest.is_empty() {
                return Ok(Value::Array(Vec::new()));
            }
            let Some(body) = rest.strip_suffix(&[0]) else {
                return Err(format!(
                    "zstring array not NUL-terminated: {:?}",
                    cp1252::decode(rest)
                ));
            };
            // Drop the extra NUL that closes the list, when present.
            let body = body.strip_suffix(&[0]).unwrap_or(body);
            if body.is_empty() {
                return Ok(Value::Array(Vec::new()));
            }
            Value::Array(
                body.split(|&b| b == 0)
                    .map(|s| Value::from(cp1252::decode(s)))
                    .collect(),
            )
        }
        Prim::LenString => {
            // xEdit's `wbLenString`: a little-endian u32 byte count, then exactly
            // that many bytes of text with no terminator. A length that runs past
            // the end of the field is an error, never a truncation.
            let n = c.u32().map_err(e)? as usize;
            let s = c
                .bytes(n)
                .map_err(|oob| format!("len_string length {n} exceeds the field: {oob}"))?;
            Value::from(cp1252::decode(s))
        }
        Prim::LenStringU16 => {
            // Fallout 4's `wbLenString`: a little-endian u16 byte count, then
            // exactly that many bytes of text with no terminator.
            let n = c.u16().map_err(e)? as usize;
            let s = c
                .bytes(n)
                .map_err(|oob| format!("len_string length {n} exceeds the field: {oob}"))?;
            Value::from(cp1252::decode(s))
        }
        Prim::String => Value::from(cp1252::decode(c.bytes(c.remaining()).unwrap())),
        Prim::CString => {
            // xEdit's `wbString` inside a struct: text up to the first NUL, with the
            // cursor left just past that NUL so later fields still decode. A missing
            // NUL means the string ran to the end of the field, which is an error
            // rather than a silent truncation.
            // `bytes(n)` already moves the cursor to the end, so rewind it to just
            // past the NUL after finding the terminator.
            let rest = c.bytes(c.remaining()).unwrap();
            let Some(end) = rest.iter().position(|&b| b == 0) else {
                return Err(format!(
                    "cstring not NUL-terminated: {:?}",
                    cp1252::decode(rest)
                ));
            };
            c.pos -= rest.len() - (end + 1);
            Value::from(cp1252::decode(&rest[..end]))
        }
        Prim::Bytes => Value::from(to_hex(c.bytes(c.remaining()).unwrap())),
        Prim::Localized => {
            // A localized file holds this field as a 4-byte string ID; an
            // ordinary file holds text. The file decides, not the schema.
            if ctx.localized {
                let id = c.u32().map_err(e)?;
                Value::from(id)
            } else {
                let rest = c.bytes(c.remaining()).unwrap();
                if rest.last() != Some(&0) && !rest.is_empty() {
                    return Err(format!(
                        "localized string not NUL-terminated: {:?}",
                        cp1252::decode(rest)
                    ));
                }
                Value::from(cp1252::decode_z(rest))
            }
        }
    })
}

fn read_int(p: Prim, c: &mut Cursor) -> Result<i64, String> {
    let e = |e: fnv_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => c.u8().map_err(e)? as i64,
        Prim::U16 => c.u16().map_err(e)? as i64,
        Prim::U32 => c.u32().map_err(e)? as i64,
        Prim::U64 => c.u64().map_err(e)? as i64,
        Prim::I8 => c.i8().map_err(e)? as i64,
        Prim::I16 => c.i16().map_err(e)? as i64,
        Prim::I32 => c.i32().map_err(e)? as i64,
        Prim::I64 => c.i64().map_err(e)?,
        _ => return Err("non-integer base".into()),
    })
}

/// The integer value of a decoded field, whether it is a JSON number or an
/// enum/flags object carrying the raw value in `"raw"` or `"value"`.
fn obj_int(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.get("raw").and_then(Value::as_u64))
        .or_else(|| v.get("value").and_then(Value::as_u64))
}

/// The integer a decoded value carries, for element counts.
///
/// Only plain integers count here: a count is how many elements follow, so
/// an enum or a name is not one.
fn switch_int(v: &Value) -> Option<u64> {
    obj_int(v)
}

/// A value a union can switch on.
///
/// The text is what a case's key is matched against: an integer's decimal
/// form, an enum's number, or a string as it stands. The number is what a
/// bit test needs, and is absent for a value that is not a number.
#[derive(Debug, Clone)]
pub struct SwitchValue {
    pub text: String,
    pub number: Option<i64>,
}

impl SwitchValue {
    fn text_only(text: String) -> Self {
        SwitchValue { text, number: None }
    }
}

/// The value a decoded value carries for union-switch purposes.
///
/// A union's case keys are text, so the value is rendered as text: an
/// integer as its decimal form, and a string as itself. An enum field
/// decodes to its name string, so the name is looked back up in the
/// schema's value table to recover the number — the decoded JSON is left
/// alone.
fn switch_value(schema: &Schema, ty: &TypeDef, v: &Value) -> Option<SwitchValue> {
    if let Some(n) = obj_int(v) {
        return Some(SwitchValue {
            text: n.to_string(),
            number: Some(n as i64),
        });
    }
    let name = v.as_str()?;
    match schema.resolve(ty) {
        TypeDef::Enum { values, .. } => values
            .iter()
            .find(|(_, n)| n == name)
            .map(|(k, _)| SwitchValue {
                text: k.to_string(),
                number: Some(*k as i64),
            }),
        _ => Some(SwitchValue::text_only(name.to_string())),
    }
}

pub fn decode_type(t: &TypeDef, c: &mut Cursor, ctx: &mut DecodeCtx) -> Result<Value, String> {
    let schema = ctx.schema;
    match schema.resolve(t) {
        TypeDef::Prim(p) => read_prim(*p, c, ctx),
        TypeDef::Named(_) => Err("unresolved type".into()),
        TypeDef::Union(variants) => {
            let left = c.remaining();
            let chosen = variants
                .iter()
                .find(|(n, _)| *n == Some(left))
                .or_else(|| variants.iter().find(|(n, _)| n.is_none()))
                .ok_or_else(|| format!("no union variant for {left} bytes"))?;
            decode_type(&chosen.1, c, ctx)
        }
        TypeDef::PeekUnion {
            cases,
            default,
            variants,
        } => {
            // The variant is chosen by the field's first word, read without
            // consuming it: the word belongs to the field either way, so the
            // whole field is the union.
            if c.remaining() < 4 {
                return Err(format!(
                    "peek union: read of 4 bytes at offset {} exceeds length {}",
                    c.pos,
                    c.data.len()
                ));
            }
            let b = &c.data[c.pos..c.pos + 4];
            let word = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64;
            let idx = cases
                .iter()
                .find(|(k, _)| *k == word)
                .map(|(_, i)| *i)
                .unwrap_or(*default);
            decode_type(&variants[idx], c, ctx)
        }
        TypeDef::ValueUnion {
            name,
            cases,
            bits,
            default,
            variants,
        } => {
            // The variant is chosen by a sibling value, not by the bytes left.
            // Two lookups, in order: a field decoded earlier in the *same* struct
            // (already in `ctx.exposed` by a sibling subrecord, or the enclosing
            // struct's own field), else a field exposed by an earlier subrecord
            // of the same record (xEdit's `RecordBySignature[PRKE]`).
            let switch = ctx
                .switch_on
                .as_ref()
                .and_then(|s| s.get(name).cloned())
                .or_else(|| ctx.exposed.get(&(ctx.rec_sig, name.clone())).cloned());
            // A missing switch selects the default, mirroring xEdit's deciders
            // (e.g. wbPerkDATADecider returns 0 when the sibling PRKE is absent,
            // which is exactly the record-level case the union must also cover).
            let idx = match &switch {
                // A bit test: the first mask xEdit tests that is set picks
                // the variant, and none being set leaves the default.
                Some(sv) if !bits.is_empty() => {
                    let n = sv.number.unwrap_or(0) as u64;
                    bits.iter()
                        .find(|(mask, _)| n & mask != 0)
                        .map(|(_, i)| *i)
                        .unwrap_or(*default)
                }
                Some(sv) => cases
                    .iter()
                    .find(|(k, _)| *k == sv.text)
                    .map(|(_, i)| *i)
                    .unwrap_or(*default),
                None => *default,
            };
            decode_type(&variants[idx], c, ctx)
        }
        TypeDef::Struct(fields) => {
            let mut obj = Map::new();
            // A struct's own field values are kept so a union inside it can
            // switch on one of them. The map is saved and restored, so a
            // nested struct does not see its parent's fields — except that
            // a union may switch on a field of the struct *around* it
            // (xEdit's `wbScriptObjFormatDecider` reads the "Object Format"
            // of the virtual machine adapter that holds the property), so
            // the parent's fields are kept too and the inner ones win.
            let saved_switch = ctx.switch_on.take();
            ctx.switch_on = Some(match &saved_switch {
                Some(parent) => parent.clone(),
                None => HashMap::new(),
            });
            let outer = ctx.switch_on.clone().unwrap_or_default();
            for f in fields {
                if f.optional_if_empty && c.remaining() == 0 {
                    // The field is absent, which the data says by having
                    // nothing left for it.
                    obj.insert(f.name.clone(), Value::Null);
                    continue;
                }
                if f.optional {
                    // The field is skipped when the bytes left are too few
                    // for it, so a struct may be shorter than its fields.
                    // A field whose size is not known is read, since there
                    // is no telling that it will not fit.
                    // The field is still named in the output, with a null
                    // value, so that a re-encode writes exactly the bytes
                    // that were read.
                    if let Some(n) = schema.fixed_size(&f.ty) {
                        if c.remaining() < n {
                            obj.insert(f.name.clone(), Value::Null);
                            break;
                        }
                    }
                }
                // A count-field array reads its element count from a sibling field
                // that was decoded earlier in this same struct (xEdit's
                // `SetCountPath`). An integer field is kept as a raw number for that
                // lookup without disturbing its JSON representation.
                if let TypeDef::Array {
                    count_field: Some(name),
                    ..
                } = &schema.resolve(&f.ty)
                {
                    let n = obj.get(name).and_then(Value::as_u64).ok_or_else(|| {
                        format!(
                            "{}: count_field {name:?} has no earlier integer value",
                            f.name
                        )
                    })?;
                    let TypeDef::Array { elem, .. } = schema.resolve(&f.ty) else {
                        unreachable!()
                    };
                    let mut out = Vec::new();
                    for i in 0..n {
                        let before = c.pos;
                        out.push(
                            decode_type(elem, c, ctx)
                                .map_err(|e| format!("{}: [{i}]: {e}", f.name))?,
                        );
                        if c.pos == before {
                            return Err(format!("{}: array element consumed no bytes", f.name));
                        }
                    }
                    obj.insert(f.name.clone(), Value::Array(out));
                    continue;
                }
                // A count-bits array reads its element count from the set
                // bits of a sibling field (xEdit's
                // `wbScriptFragmentsInfoCounter`, which counts a script's
                // fragments by how many of the first three of its flags are
                // set).
                if let TypeDef::Array {
                    count_bits: Some((name, bits)),
                    ..
                } = &schema.resolve(&f.ty)
                {
                    let v = obj.get(name).and_then(switch_int).ok_or_else(|| {
                        format!(
                            "{}: count_bits {name:?} has no earlier integer value",
                            f.name
                        )
                    })?;
                    let n = (0..*bits)
                        .filter(|b| v & (1u64 << *b) != 0)
                        .count();
                    let TypeDef::Array { elem, .. } = schema.resolve(&f.ty) else {
                        unreachable!()
                    };
                    let mut out = Vec::new();
                    for i in 0..n {
                        let before = c.pos;
                        out.push(
                            decode_type(elem, c, ctx)
                                .map_err(|e| format!("{}: [{i}]: {e}", f.name))?,
                        );
                        if c.pos == before {
                            return Err(format!("{}: array element consumed no bytes", f.name));
                        }
                    }
                    obj.insert(f.name.clone(), Value::Array(out));
                    continue;
                }
                let v = decode_type(&f.ty, c, ctx).map_err(|e| format!("{}: {e}", f.name))?;
                // `expose` records an integer field so a later subrecord can switch
                // a union on it. Enum/flag values stay their JSON form, so read the
                // raw number out of the object when present.
                if f.expose {
                    if let Some(n) = switch_value(schema, &f.ty, &v) {
                        ctx.exposed.insert((ctx.rec_sig, f.name.clone()), n.clone());
                        // A field of a subrecord's own struct is exposed
                        // under the subrecord's name and the field's, so
                        // that a later subrecord can switch on it.
                        if let Some(sig) = ctx.sub_sig {
                            let key = format!(
                                "{}.{}",
                                String::from_utf8_lossy(&sig),
                                f.name
                            );
                            ctx.exposed.insert((ctx.rec_sig, key), n);
                        }
                    }
                    // A struct that both exposes a field and switches on one needs
                    // its own field values, so keep them for in-struct lookups too.
                    if let Some(m) = ctx.switch_on.as_mut() {
                        if let Some(n) = switch_value(schema, &f.ty, &v) {
                            m.insert(f.name.clone(), n);
                        }
                    }
                }
                obj.insert(f.name.clone(), v);
            }
            // The struct's own fields are what a union inside it reads, and
            // they take the place of the enclosing struct's fields of the
            // same name.
            if let Some(m) = ctx.switch_on.as_mut() {
                for (k, v) in &outer {
                    m.entry(k.clone()).or_insert(v.clone());
                }
            }
            ctx.switch_on = saved_switch;
            Ok(Value::Object(obj))
        }
        TypeDef::Array {
            elem,
            count,
            count_prefix,
            ..
        } => {
            let mut out = Vec::new();
            match count {
                Some(n) => {
                    for i in 0..*n {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            // Zero-width elements would let a schema demand unbounded
                            // output from a few bytes of data.
                            return Err("array element consumed no bytes".into());
                        }
                    }
                }
                // The count is in the data. A declared count larger than the field
                // can hold is an error, never a short array: silently reading fewer
                // elements than the data says is how a layout bug hides.
                None => {
                    // A count given by a sibling subrecord takes the place of
                    // a prefix: the count was read from an earlier subrecord,
                    // not from the bytes in front of the elements.
                    if let Some(from) = ctx.count_from {
                        let n = ctx.counts.get(&from).copied().unwrap_or(0) as usize;
                        for i in 0..n {
                            let before = c.pos;
                            out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                            if c.pos == before {
                                return Err("array element consumed no bytes".into());
                            }
                        }
                        return Ok(Value::Array(out));
                    }
                    let n = match count_prefix {
                        Some(p) => read_int(*p, c)? as usize,
                        None => {
                            let mut i = 0;
                            while !c.is_empty() {
                                let before = c.pos;
                                out.push(
                                    decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?,
                                );
                                if c.pos == before {
                                    return Err("array element consumed no bytes".into());
                                }
                                i += 1;
                            }
                            return Ok(Value::Array(out));
                        }
                    };
                    for i in 0..n {
                        let before = c.pos;
                        out.push(decode_type(elem, c, ctx).map_err(|e| format!("[{i}]: {e}"))?);
                        if c.pos == before {
                            return Err("array element consumed no bytes".into());
                        }
                    }
                }
            }
            Ok(Value::Array(out))
        }
        TypeDef::Enum { base, values } => {
            let v = read_int(*base, c)?;
            match values.iter().find(|(k, _)| *k == v) {
                Some((_, name)) => Ok(Value::from(name.as_str())),
                None => Ok(Value::from(v)),
            }
        }
        TypeDef::Flags { base, bits } => {
            let v = read_int(*base, c)? as u64;
            let mut names = Vec::new();
            let mut rest = v;
            for (b, n) in bits {
                if v & (1u64 << b) != 0 {
                    names.push(Value::from(n.as_str()));
                    rest &= !(1u64 << b);
                }
            }
            let mut o = json!({"value": v, "set": names});
            if rest != 0 {
                o["unknown_bits"] = json!(format!("0x{rest:x}"));
            }
            Ok(o)
        }
    }
}

pub struct DecodedSub {
    pub status: SubStatus,
    /// The whole subrecord is defined only as opaque `bytes` (layout not understood).
    pub opaque: bool,
    pub name: Option<String>,
    pub value: Option<Value>,
    pub error: Option<String>,
}

pub fn decode_sub(rec_sig: &[u8; 4], s: &Subrecord, ctx: &mut DecodeCtx) -> DecodedSub {
    // A block's chosen variant wins over the block's own map: the opener's
    // value says which fields the block holds.
    // The definition is cloned out of the context: `decode_type` needs the
    // context mutably, and the definition may live in it.
    let found = ctx
        .block_variant
        .as_ref()
        .and_then(|v| v.get(&s.sig))
        .cloned()
        .or_else(|| {
            ctx.schema
                .lookup_in_block(rec_sig, &s.sig, ctx.block)
                .cloned()
        });
    let def = match found {
        Some(d) => d,
        None => {
            return DecodedSub {
                status: SubStatus::Unknown,
                opaque: false,
                name: None,
                value: None,
                error: None,
            }
        }
    };
    let opaque = matches!(ctx.schema.resolve(&def.ty), TypeDef::Prim(Prim::Bytes));
    let mut c = Cursor::new(&s.data);
    // The subrecord's own element count may come from a sibling, which the
    // array decoder needs to know before it reads the elements.
    let saved_from = ctx.count_from;
    ctx.count_from = def.count_from;
    // While the subrecord is being read, a field of its own struct is
    // exposed under the subrecord's name as well as its own.
    let saved_sub = ctx.sub_sig;
    ctx.sub_sig = Some(s.sig);
    let decoded = decode_type(&def.ty, &mut c, ctx);
    ctx.sub_sig = saved_sub;
    ctx.count_from = saved_from;
    match decoded {
        Ok(v) => {
            // A bare (non-struct) subrecord may itself be exposed so a sibling
            // subrecord's union can switch on it (xEdit's `wbEPFDDecider`
            // reading EPFT, which is `wbInteger(EPFT, 'Type', itU8, ...)`).
            // Keyed by the subrecord signature, not a field name, so it cannot
            // collide with a struct field of the same name (PERK's PRKE.Type).
            if def.expose {
                if let Some(i) = switch_value(ctx.schema, &def.ty, &v) {
                    ctx.exposed.insert((ctx.rec_sig, s.sig_str()), i);
                }
            }
            // A subrecord's value is remembered under its own signature, so a
            // later subrecord can take its element count from it (Skyrim's
            // KSIZ holding the keyword count and KWDA holding the keywords).
            if let Some(i) = switch_int(&v) {
                ctx.counts.insert(s.sig, i);
            }
            if c.is_empty() {
                DecodedSub {
                    status: SubStatus::Ok,
                    opaque,
                    name: Some(def.name.clone()),
                    value: Some(v),
                    error: None,
                }
            } else {
                let rest = to_hex(&s.data[c.pos..]);
                DecodedSub {
                    status: SubStatus::Trailing,
                    opaque,
                    name: Some(def.name.clone()),
                    value: Some(v),
                    error: Some(format!("{} trailing bytes: {rest}", s.data.len() - c.pos)),
                }
            }
        }
        Err(e) => DecodedSub {
            status: SubStatus::Error,
            opaque,
            name: Some(def.name.clone()),
            value: None,
            error: Some(e),
        },
    }
}

/// Walk a record's subrecords, tracking which sequence block each is in.
///
/// A record with a `sequence` is an ordered list of blocks. A block's `order`
/// says which signature is expected next, and a signature that matches it
/// continues the block; one that does not, and is a known opener, starts a new
/// block. That is what tells `SCEN`'s `ALID` — which both opens an actor and
/// appears inside an action — apart, since the signature alone cannot.
///
/// The two callers differ only in what they do with each decoded subrecord, so
/// the walk lives here and both use it.
pub fn walk_sequence(
    rec_sig: &[u8; 4],
    subs: &[crate::raw::Subrecord],
    schema: &Schema,
    ctx: &mut DecodeCtx,
    mut f: impl FnMut(&crate::raw::Subrecord, &mut DecodeCtx),
) {
    let seq = schema
        .records
        .get(rec_sig)
        .and_then(|rd| rd.sequence.as_ref());
    let mut block: Option<usize> = None;
    for s in subs {
        // Whether the current block expects this signature, which is what
        // tells a block's own field from a new block's opener.
        let mut expected_here = false;
        if let Some(seq) = seq {
            // The signature opens a block only when the current block does not
            // expect it. That is what tells `SCEN`'s `ALID` — which both opens
            // an actor and appears inside an action — apart: inside an action it
            // is where the action's order says it should be, so it continues
            // that block; at the head of the actor list it is not expected, so
            // it opens one.
            expected_here = block
                .and_then(|b| seq.blocks.get(b))
                .map(|bd| bd.order.contains(&s.sig))
                .unwrap_or(false);
            if !expected_here {
                if let Some(i) = seq.blocks.iter().position(|b| b.opener == s.sig) {
                    block = Some(i);
                    // A new block starts with no variant chosen; the opener's
                    // value picks one below.
                    ctx.block_variant = None;
                }
            }
        }
        // A block that switches on its opener's value keeps that choice for
        // the whole block, not just for the opener itself: `PACK`'s `ANAM`
        // names the activity's type, and the field *after* it is decoded
        // against that type's definition.
        let opens_a_block = block
            .and_then(|b| seq.and_then(|q| q.blocks.get(b)))
            .map(|bd| bd.opener == s.sig)
            .unwrap_or(false)
            && !expected_here;
        if opens_a_block {
            // The context's block must already be the block this subrecord
            // opens, so the opener's own definition is found in it.
            ctx.block = block;
            let opener_value = opener_value_of(rec_sig, s, schema, ctx);
            if let (Some(bd), Some(ov)) = (
                block.and_then(|b| seq.and_then(|q| q.blocks.get(b))),
                opener_value,
            ) {
                if let Some((_, vars)) = bd.variants.iter().find(|(v, _)| *v == ov) {
                    ctx.block_variant = Some(vars.clone());
                }
            }
        }
        ctx.block = block;
        f(s, ctx);
    }
}

/// The value of a block's opener, read from its own bytes.
///
/// A block that switches on the opener's value — `PACK`'s `ANAM`, whose value
/// names the activity's type — needs that value before the block's fields can
/// be decoded, so it is read straight from the opener's bytes rather than
/// from a decoded result.
fn opener_value_of(
    rec_sig: &[u8; 4],
    s: &crate::raw::Subrecord,
    schema: &Schema,
    ctx: &mut DecodeCtx,
) -> Option<String> {
    let def = schema
        .lookup_in_block(rec_sig, &s.sig, ctx.block)
        .or_else(|| schema.lookup(rec_sig, &s.sig))?;
    // Only a string opener can name a variant.
    match schema.resolve(&def.ty) {
        TypeDef::Prim(Prim::ZString) => {
            let bytes: &[u8] = &s.data;
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
        }
        _ => None,
    }
}

/// Decode a whole record to JSON.
pub fn decode_record(r: &Record, schema: &Schema) -> Value {
    decode_record_localized(r, schema, false)
}

/// [`decode_record`] for a record from a file that may be *localized*.
///
/// A localized file holds its translatable strings as 4-byte string IDs
/// rather than as text, so the same schema reads one or the other according
/// to the file the record came from.
pub fn decode_record_localized(r: &Record, schema: &Schema, localized: bool) -> Value {
    let mut out = json!({
        "formid": formid_str(r.header.form_id),
        "type": r.sig_str(),
        "flags": r.header.flags,
        "form_version": r.header.form_version,
    });
    if let Some(rd) = schema.records.get(&r.header.sig) {
        out["type_name"] = json!(rd.name);
        // Header flag bits the schema names for this record type (`header_flags`).
        let named = rd.header_flag_names(r.header.flags);
        if !named.is_empty() {
            out["flag_names"] = json!(named);
        }
    }
    match r.subrecords() {
        Err(e) => {
            out["error"] = json!(e.to_string());
        }
        Ok(subs) => {
            let mut arr = Vec::with_capacity(subs.len());
            let mut ctx = DecodeCtx::new(schema, r.header.sig);
            ctx.localized = localized;
            walk_sequence(&r.header.sig, &subs, schema, &mut ctx, |s, ctx| {
                let d = decode_sub(&r.header.sig, s, ctx);
                let mut o = Map::new();
                o.insert("sig".into(), json!(s.sig_str()));
                if let Some(n) = d.name {
                    o.insert("name".into(), json!(n));
                }
                o.insert("status".into(), json!(d.status.as_str()));
                match d.value {
                    Some(v) => o.insert("value".into(), v),
                    None => o.insert("raw".into(), json!(to_hex(&s.data))),
                };
                if let Some(e) = d.error {
                    o.insert("error".into(), json!(e));
                }
                arr.push(Value::Object(o));
            });
            out["subrecords"] = Value::Array(arr);
        }
    }
    out
}

/// Per (record type, subrecord signature) coverage counters.
#[derive(Debug, Clone, Default)]
pub struct SubCoverage {
    pub total: u64,
    pub ok: u64,
    /// Subset of `ok` whose definition is opaque `bytes`.
    pub opaque: u64,
    pub unknown: u64,
    pub trailing: u64,
    pub error: u64,
    pub sizes: BTreeMap<usize, u64>,
    pub first_error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Coverage {
    pub by_sub: BTreeMap<(String, String), SubCoverage>,
    pub records: BTreeMap<String, u64>,
    pub record_errors: Vec<String>,
}

/// Coverage outcome of one record, computed independently of every other record.
pub struct RecordCoverage {
    sig: String,
    form_id: u32,
    /// Err if the payload cannot be split into subrecords.
    subs: Result<Vec<SubOutcome>, String>,
}

struct SubOutcome {
    sig: String,
    size: usize,
    status: SubStatus,
    opaque: bool,
    error: Option<String>,
}

impl Coverage {
    /// Decode one record's subrecords against the schema (no shared state).
    pub fn measure<'a>(
        r: &Record,
        schema: &'a Schema,
        on_formid: Option<&'a mut dyn FnMut(u32)>,
    ) -> RecordCoverage {
        Self::measure_localized(r, schema, on_formid, false)
    }

    /// [`Coverage::measure`] for a record from a file that may be *localized*.
    ///
    /// A localized file holds its translatable strings as 4-byte string IDs
    /// rather than as text, so the same schema reads one or the other
    /// according to the file the record came from.
    pub fn measure_localized<'a>(
        r: &Record,
        schema: &'a Schema,
        on_formid: Option<&'a mut dyn FnMut(u32)>,
        localized: bool,
    ) -> RecordCoverage {
        let subs = r.subrecords().map_err(|e| e.to_string()).map(|subs| {
            let mut ctx = DecodeCtx {
                on_formid,
                ..DecodeCtx::new(schema, r.header.sig)
            };
            ctx.localized = localized;
            // The same sequence walk as `decode_record`: the block decides
            // which definition a signature gets.
            let mut out = Vec::with_capacity(subs.len());
            walk_sequence(&r.header.sig, &subs, schema, &mut ctx, |s, ctx| {
                let d = decode_sub(&r.header.sig, s, ctx);
                out.push(SubOutcome {
                    sig: s.sig_str(),
                    size: s.data.len(),
                    status: d.status,
                    opaque: d.opaque,
                    error: d.error,
                });
            });
            out
        });
        RecordCoverage {
            sig: r.sig_str(),
            form_id: r.header.form_id,
            subs,
        }
    }

    /// Fold one measured record into the totals. Records must be added in file
    /// order: `first_error` and `record_errors` keep the earliest occurrences.
    pub fn add(&mut self, rc: RecordCoverage) {
        *self.records.entry(rc.sig.clone()).or_default() += 1;
        let subs = match rc.subs {
            Ok(s) => s,
            Err(e) => {
                if self.record_errors.len() < 100 {
                    self.record_errors.push(e);
                }
                return;
            }
        };
        for sub in subs {
            let e = self.by_sub.entry((rc.sig.clone(), sub.sig)).or_default();
            e.total += 1;
            *e.sizes.entry(sub.size).or_default() += 1;
            match sub.status {
                SubStatus::Ok => {
                    e.ok += 1;
                    if sub.opaque {
                        e.opaque += 1;
                    }
                }
                SubStatus::Unknown => e.unknown += 1,
                SubStatus::Trailing => e.trailing += 1,
                SubStatus::Error => e.error += 1,
            }
            if e.first_error.is_none() {
                if let Some(err) = sub.error {
                    e.first_error = Some(format!("{:08X}: {err}", rc.form_id));
                }
            }
        }
    }

    pub fn add_record<'a>(
        &mut self,
        r: &Record,
        schema: &'a Schema,
        on_formid: Option<&'a mut dyn FnMut(u32)>,
    ) {
        self.add(Self::measure(r, schema, on_formid));
    }

    /// [`Coverage::add_record`] for a record from a file that may be
    /// *localized*.
    pub fn add_record_localized<'a>(
        &mut self,
        r: &Record,
        schema: &'a Schema,
        on_formid: Option<&'a mut dyn FnMut(u32)>,
        localized: bool,
    ) {
        self.add(Self::measure_localized(r, schema, on_formid, localized));
    }

    /// [`Coverage::add_record`] for many records, decoded in parallel and folded in
    /// order, so the result is identical to adding them one by one.
    pub fn add_records_par(&mut self, recs: &[&Record], schema: &Schema) {
        for chunk in recs.chunks(4096) {
            let measured: Vec<RecordCoverage> = chunk
                .par_iter()
                .map(|r| Self::measure(r, schema, None))
                .collect();
            for rc in measured {
                self.add(rc);
            }
        }
    }

    /// [`Coverage::add_records_par`] for records from a file that may be
    /// *localized*.
    pub fn add_records_par_localized(
        &mut self,
        recs: &[&Record],
        schema: &Schema,
        localized: bool,
    ) {
        for chunk in recs.chunks(4096) {
            let measured: Vec<RecordCoverage> = chunk
                .par_iter()
                .map(|r| Self::measure_localized(r, schema, None, localized))
                .collect();
            for rc in measured {
                self.add(rc);
            }
        }
    }

    pub fn totals(&self) -> (u64, u64, u64, u64, u64) {
        let mut t = (0, 0, 0, 0, 0);
        for c in self.by_sub.values() {
            t.0 += c.total;
            t.1 += c.ok;
            t.2 += c.unknown;
            t.3 += c.trailing;
            t.4 += c.error;
        }
        t
    }

    pub fn to_json(&self, max_rows: usize) -> Value {
        let (total, ok, unknown, trailing, error) = self.totals();
        let pct = |n: u64| {
            if total == 0 {
                0.0
            } else {
                (n as f64) * 100.0 / total as f64
            }
        };
        let summary = json!({
            "subrecords": total,
            "ok": ok,
            "unknown": unknown,
            "trailing": trailing,
            "error": error,
            "ok_percent": (pct(ok) * 100.0).round() / 100.0,
            "opaque_bytes": self.by_sub.values().map(|c| c.opaque).sum::<u64>(),
            "distinct_pairs": self.by_sub.len(),
            "pairs_fully_decoded": self.by_sub.values().filter(|c| c.ok == c.total).count(),
        });
        // Work queue: pairs with the most undecoded instances first.
        let mut rows: Vec<(&(String, String), &SubCoverage)> =
            self.by_sub.iter().filter(|(_, c)| c.ok < c.total).collect();
        rows.sort_by(|a, b| {
            (b.1.total - b.1.ok)
                .cmp(&(a.1.total - a.1.ok))
                .then(a.0.cmp(b.0))
        });
        let mut q = Vec::new();
        for ((rt, st), c) in rows.into_iter().take(max_rows) {
            let sizes: Vec<Value> = {
                let mut sv: Vec<(&usize, &u64)> = c.sizes.iter().collect();
                sv.sort_by(|a, b| b.1.cmp(a.1));
                sv.into_iter()
                    .take(5)
                    .map(|(sz, n)| json!({"size": sz, "count": n}))
                    .collect()
            };
            let mut o = json!({
                "record": rt,
                "sub": st,
                "total": c.total,
                "ok": c.ok,
                "unknown": c.unknown,
                "trailing": c.trailing,
                "error": c.error,
                "top_sizes": sizes,
            });
            if let Some(e) = &c.first_error {
                o["first_error"] = json!(e);
            }
            q.push(o);
        }
        json!({"summary": summary, "work_queue": q, "record_errors": self.record_errors})
    }
}
