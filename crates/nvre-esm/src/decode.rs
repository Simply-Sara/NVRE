//! Decoding a record's fields against a schema, and measuring how much of
//! the data the schema explains.
//!
//! Two things happen here, and they are worth separating:
//!
//! - **Decoding** a record turns its subrecords' bytes into values, using
//!   the schema's layouts.
//! - **Measuring** a record says how much of its data the schema accounts
//!   for: every byte explained, or a byte the schema does not mention, or a
//!   subrecord the schema has no layout for.
//!
//! The measure is what makes a schema trustworthy. A layout that is wrong
//! shows up as a number that is not 100%, not as a program that crashes.

use crate::raw::{Record, Subrecord};
use crate::schema::PeekUnion;
use crate::schema::ValueUnion;
use crate::schema::{ArrayCount, BlockDef, Prim, Schema, SequenceDef, SubDef, Type, TypeDef};
use nvre_util::{cp1252, Cursor};
use serde_json::{Map, Value as Json};
use std::collections::BTreeMap;
use std::collections::HashMap;

/// How well the schema explains one subrecord.
#[derive(Debug, Clone, PartialEq)]
pub enum SubStatus {
    /// Every byte was accounted for.
    Ok,
    /// The schema has no layout for this subrecord.
    Unknown,
    /// The subrecord was read, but some bytes are unaccounted for.
    Trailing {
        /// How many bytes were read.
        used: usize,
        /// How many bytes the subrecord holds.
        total: usize,
    },
    /// The subrecord could not be read.
    Error(String),
}

impl SubStatus {
    /// A short name for the status.
    pub fn as_str(&self) -> &'static str {
        match self {
            SubStatus::Ok => "ok",
            SubStatus::Unknown => "unknown",
            SubStatus::Trailing { .. } => "trailing",
            SubStatus::Error(_) => "error",
        }
    }
}

/// The state a record's decoding carries: what has been read so far, and
/// what the schema says about the record.
pub struct DecodeCtx<'a> {
    /// The schema.
    pub schema: &'a Schema,
    /// The record's four-byte type.
    pub rec_sig: [u8; 4],
    /// The values read so far, by the subrecord they came from.
    pub values: HashMap<String, Json>,
    /// The element counts of the subrecords of this record, by the
    /// subrecord's name.
    ///
    /// A subrecord may take its element count from a sibling: Skyrim's
    /// `KWDA` takes the keyword count from `KSIZ`.
    pub counts: HashMap<String, u64>,
    /// The switch values of the subrecords of this record, by the record's
    /// type and the subrecord's name.
    ///
    /// A subrecord may be read against a value another subrecord of the
    /// same record holds: the `PERK` record's `DATA` is one thing or
    /// another depending on the `PRKE` beside it.
    pub exposed: HashMap<(String, String), i64>,
    /// The subrecords of the block that is open, when the block's opener
    /// names a variant.
    ///
    /// A block may switch on its opener's value: `PACK`'s `ANAM` names the
    /// activity's type, and the field after it is decoded against that
    /// type's definition.
    pub block_variant: Option<BTreeMap<String, SubDef>>,
    /// The values of the struct being read, for a union inside it to be
    /// read against.
    ///
    /// The map is saved and restored around a nested struct, so a nested
    /// struct does not see its parent's fields.
    pub switch_on: Option<HashMap<String, i64>>,
}

impl<'a> DecodeCtx<'a> {
    /// A context for a record of the given type.
    pub fn new(schema: &'a Schema, rec_sig: [u8; 4]) -> DecodeCtx<'a> {
        DecodeCtx {
            schema,
            rec_sig,
            block_variant: None,
            values: HashMap::new(),
            counts: HashMap::new(),
            exposed: HashMap::new(),
            switch_on: None,
        }
    }
}

/// A FormID as text, with its master's index.
pub fn formid_str(v: u32) -> String {
    format!("{v:08X}")
}

/// A decoded subrecord.
#[derive(Debug, Clone)]
pub struct DecodedSub {
    /// The subrecord's own error, when it could not be read.
    pub error: Option<String>,
    /// The subrecord's four-byte type.
    pub sig: [u8; 4],
    /// The value the subrecord decoded to, when it decoded.
    pub value: Option<Json>,
    /// How well the schema explained it.
    pub status: SubStatus,
    /// The field's name, when the schema has one.
    pub name: Option<String>,
}

impl DecodedSub {
    /// The subrecord's type as text.
    pub fn sig_str(&self) -> String {
        String::from_utf8_lossy(&self.sig).into_owned()
    }
}

/// Decode a subrecord against a block's own definition.
fn decode_sub_in(
    b: &BlockDef,
    sig: &str,
    s: &Subrecord,
    ctx: &mut DecodeCtx<'_>,
    _scope: &Map<String, Json>,
) -> DecodedSub {
    // A block's chosen variant wins over the block's own map: the opener's
    // value says which fields the block holds.
    let def = ctx
        .block_variant
        .as_ref()
        .and_then(|v| v.get(sig))
        .cloned()
        .or_else(|| b.subrecords.get(sig).cloned());
    let Some(def) = &def else {
        return DecodedSub {
            sig: s.sig,
            error: None,
            value: None,
            status: SubStatus::Unknown,
            name: None,
        };
    };
    let mut c = Cursor::new(&s.data);
    match decode_subdef(def, &mut c, ctx) {
        Ok(v) => {
            let used = c.pos;
            let total = s.data.len();
            let status = if used == total {
                SubStatus::Ok
            } else {
                SubStatus::Trailing { used, total }
            };
            ctx.values.insert(sig.to_string(), v.clone());
            DecodedSub {
                sig: s.sig,
                error: None,
                value: Some(v),
                status,
                name: Some(def.name.clone()),
            }
        }
        Err(e) => DecodedSub {
            sig: s.sig,
            value: None,
            status: SubStatus::Error(e.clone()),
            name: Some(def.name.clone()),
            error: Some(e),
        },
    }
}

/// Decode one subrecord.
pub fn decode_sub(rec_sig: &[u8; 4], s: &Subrecord, ctx: &mut DecodeCtx<'_>) -> DecodedSub {
    let sig = String::from_utf8_lossy(&s.sig).into_owned();
    let rec_key = String::from_utf8_lossy(rec_sig).into_owned();
    let def = ctx
        .schema
        .records
        .get(&rec_key)
        .and_then(|r| {
            r.subrecords.get(&sig).or_else(|| {
                // A signature that is only partly known: the weather
                // record's texture layers are `00TX`, `10TX`, `20TX` and
                // on, so the schema gives the suffix and the signature is
                // matched against its trailing bytes.
                r.wildcards.iter().find(|(suffix, _)| {
                    suffix.len() <= sig.len()
                        && sig.as_bytes()[sig.len() - suffix.len()..] == suffix[..]
                }).map(|(_, d)| d)
            })
        })
        // A subrecord any record may carry: the editor ID and its like.
        .or_else(|| ctx.schema.common_subrecords.get(&sig));
    let Some(def) = def else {
        return DecodedSub {
            sig: s.sig,
            error: None,
            value: None,
            status: SubStatus::Unknown,
            name: None,
        };
    };
    let mut c = Cursor::new(&s.data);
    match decode_subdef(def, &mut c, ctx) {
        Ok(v) => {
            let used = c.pos;
            let total = s.data.len();
            let status = if used == total {
                SubStatus::Ok
            } else {
                SubStatus::Trailing { used, total }
            };
            ctx.values.insert(sig.clone(), v.clone());
            DecodedSub {
                sig: s.sig,
                error: None,
                value: Some(v),
                status,
                name: Some(def.name.clone()),
            }
        }
        Err(e) => DecodedSub {
            sig: s.sig,
            value: None,
            status: SubStatus::Error(e.clone()),
            name: Some(def.name.clone()),
            error: Some(e),
        },
    }
}

/// Decode a subrecord against its definition.
fn decode_subdef(
    def: &SubDef,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
) -> Result<Json, String> {
    // A subrecord that holds a count of elements: the count comes from
    // another subrecord's value.
    if let Some(count_from) = &def.count_from {
        let n = ctx
            .counts
            .get(count_from)
            .copied()
            .or_else(|| ctx.values.get(count_from).and_then(Json::as_u64))
            .ok_or_else(|| format!("the count of {count_from:?} is not known yet"))?
            as usize;
        return decode_counted(&def.ty, n, c, ctx, &Map::new());
    }
    decode_type_value(&def.ty, c, ctx, &Map::new())
}

/// Decode a type's value.
fn decode_type_value(
    ty: &Type,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
    scope: &Map<String, Json>,
) -> Result<Json, String> {
    match ty {
        Type::Prim(p) => decode_prim(*p, c),
        Type::Named(n) => {
            // An enum: the value, and the name it has.
            if let Some(values) = ctx.schema.enum_values(n) {
                let v = decode_prim(ctx.schema.enum_storage(n), c)?;
                let i = v.as_i64().unwrap_or(0);
                let name = values.get(&i).cloned();
                let mut m = Map::new();
                m.insert("value".into(), v);
                if let Some(name) = name {
                    m.insert("name".into(), Json::String(name));
                }
                return Ok(Json::Object(m));
            }
            // A struct, or a union.
            let Some(td) = ctx.schema.type_def(n) else {
                return Err(format!("the unknown type {n:?}"));
            };
            // A named type that is a set of flags: the flags are stored as
            // one number, and each flag is a bit of it.
            if !td.bits.is_empty() {
                let v = decode_prim(td.storage, c)?;
                let i = v.as_i64().unwrap_or(0);
                let mut m = Map::new();
                m.insert("value".into(), v);
                let mut fields = Map::new();
                for b in &td.bits {
                    fields.insert(b.name.clone(), Json::from(b.value(i)));
                }
                m.insert("fields".into(), Json::Object(fields));
                return Ok(Json::Object(m));
            }
            // A named type that is a set of names: the value is stored as
            // one number, and the name it has is read from it.
            if !td.values.is_empty() {
                let v = decode_prim(td.storage, c)?;
                let i = v.as_i64().unwrap_or(0);
                let mut m = Map::new();
                m.insert("value".into(), v);
                if let Some(name) = td.values.get(&i) {
                    m.insert("name".into(), Json::String(name.clone()));
                }
                return Ok(Json::Object(m));
            }
            // A union whose variant is chosen by the field's first word:
            // Skyrim's `MODT` has a header or has not.
            if let Some(pu) = &td.peek_union {
                return decode_peek_union(pu, c, ctx, scope);
            }
            // A union whose variant is chosen by another field's value:
            // the `PERK` record's `DATA` is one thing or another depending
            // on the `PRKE` beside it.
            if let Some(vu) = &td.value_union {
                return decode_value_union(vu, c, ctx, scope);
            }
            if !td.union.is_empty() {
                return decode_type_value(&Type::Union(td.union.clone()), c, ctx, scope);
            }
            decode_struct(td, c, ctx, scope)
        }
        Type::Enum { storage, values } => {
            let v = decode_prim(*storage, c)?;
            let i = v.as_i64().unwrap_or(0);
            let mut m = Map::new();
            m.insert("value".into(), v);
            if let Some(name) = values.get(&i) {
                m.insert("name".into(), Json::String(name.clone()));
            }
            Ok(Json::Object(m))
        }
        Type::Bits { storage, bits } => {
            let v = decode_prim(*storage, c)?;
            let i = v.as_i64().unwrap_or(0);
            let mut m = Map::new();
            m.insert("value".into(), v);
            let mut fields = Map::new();
            for b in bits {
                fields.insert(b.name.clone(), Json::from(b.value(i)));
            }
            m.insert("fields".into(), Json::Object(fields));
            Ok(Json::Object(m))
        }
        Type::Inline(fields) => {
            let td = TypeDef {
                peek_union: None,
                fields: fields.clone(),
                values: Default::default(),
                bits: Vec::new(),
                storage: Prim::U32,
                union: Vec::new(),
                value_union: None,
            };
            decode_struct(&td, c, ctx, scope)
        }
        Type::Array { of, count } => {
            // How many elements: a fixed count, a field's value, or a
            // length in the data itself.
            let n = match count {
                ArrayCount::Fixed(n) => *n,
                ArrayCount::FromField(f) => {
                    // The count is a sibling field: in the struct being
                    // read, or in the subrecord this array is part of.
                    scope
                        .get(f)
                        .and_then(Json::as_u64)
                        .or_else(|| ctx.values.get(f).and_then(Json::as_u64))
                        .ok_or_else(|| format!("the count of {f:?} is not known yet"))?
                        as usize
                }
                ArrayCount::Rest => {
                    // The elements fill what is left. When the element has
                    // a fixed size the count is known; when it does not,
                    // the elements are read until the data runs out.
                    let size = ctx.schema.fixed_size(of);
                    let left = c.remaining();
                    match size {
                        Some(n) if n > 0 => left / n,
                        _ => {
                            // The count is not known, so the elements are
                            // read until there are no bytes left.
                            let mut elems = Vec::new();
                            let mut i = 0usize;
                            while c.remaining() > 0 {
                                let before = c.pos;
                                elems.push(decode_type_value(of, c, ctx, scope).map_err(
                                    |e| format!("[{i}]: {e}"),
                                )?);
                                if c.pos == before {
                                    // An element that reads no bytes would
                                    // let the loop run for ever.
                                    return Err("an array element read no bytes".into());
                                }
                                i += 1;
                            }
                            return Ok(Json::Array(elems));
                        }
                    }
                }
                ArrayCount::Prefix(p) => {
                    // The count is in the data, in the type the schema names.
                    let len: u64 = match p {
                        Prim::U8 => {
                            c.u8().map_err(|e: nvre_util::OutOfBounds| e.to_string())? as u64
                        }
                        Prim::U16 => {
                            c.u16().map_err(|e: nvre_util::OutOfBounds| e.to_string())? as u64
                        }
                        Prim::U32 => {
                            c.u32().map_err(|e: nvre_util::OutOfBounds| e.to_string())? as u64
                        }
                        other => {
                            return Err(format!("a count prefix of {other:?} is not supported"))
                        }
                    };
                    len as usize
                }
            };
            decode_counted(of, n, c, ctx, scope)
        }
        Type::ValueUnion(vu) => decode_value_union(vu, c, ctx, scope),
        Type::Union(variants) => {
            // A union's variant is chosen by its size: the one whose size
            // matches what is left in the subrecord.
            let left = c.remaining();
            let chosen = variants
                .iter()
                .find(|v| v.size == Some(left))
                .or_else(|| variants.iter().find(|v| v.size.is_none()))
                .or_else(|| variants.first());
            match chosen {
                Some(v) => decode_type_value(&v.ty, c, ctx, scope),
                None => Err("a union has no variants".into()),
            }
        }
    }
}

/// Decode a struct's fields.
fn decode_struct(
    td: &TypeDef,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
    outer: &Map<String, Json>,
) -> Result<Json, String> {
    let mut m = Map::new();
    // A struct's own field values are kept so a union inside it can switch
    // on one of them. The map is saved and restored, so a nested struct does
    // not see its parent's fields.
    let saved_switch = ctx.switch_on.take();
    ctx.switch_on = Some(HashMap::new());
    for f in &td.fields {
        // An optional field may be absent: the data runs out before it, and
        // that is not an error.
        if f.optional && c.remaining() == 0 {
            continue;
        }
        let v = if let Some(count_from) = &f.count_from {
            // The count comes from another field of the same struct.
            let n = m
                .get(count_from)
                .and_then(Json::as_u64)
                .or_else(|| outer.get(count_from).and_then(Json::as_u64))
                .or_else(|| ctx.counts.get(count_from).copied())
                .ok_or_else(|| format!("the count of {count_from:?} is not known yet"))?
                as usize;
            decode_counted(&f.ty, n, c, ctx, &m)?
        } else {
            decode_type_value(&f.ty, c, ctx, &m)?
        };
        // A field that publishes its value is read by a union in the same
        // struct: the `PERK` record's `PRKE` holds the type of the effect,
        // and the `DATA` beside it is one thing or another depending on it.
        if f.publish || f.expose {
            if let Some(n) = switch_value(&v) {
                // The value is published record-wide, so a union in a
                // subrecord that comes later is read against it.
                ctx.exposed.insert(
                    (
                        String::from_utf8_lossy(&ctx.rec_sig).into_owned(),
                        f.name.clone(),
                    ),
                    n,
                );
                // And in the struct's own map, for a union beside it.
                if let Some(sw) = ctx.switch_on.as_mut() {
                    sw.insert(f.name.clone(), n);
                }
            }
        }
        m.insert(f.name.clone(), v);
    }

    ctx.switch_on = saved_switch;
    Ok(Json::Object(m))
}

/// Decode `n` elements of a type.
fn decode_counted(
    ty: &Type,
    n: usize,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
    scope: &Map<String, Json>,
) -> Result<Json, String> {
    // A counted array of a primitive is a list of them.
    if let Type::Prim(p) = ty {
        if *p == Prim::Bytes {
            let b = c.bytes(n).map_err(|e| e.to_string())?;
            return Ok(Json::from(hex_of(b)));
        }
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            out.push(decode_prim(*p, c)?);
        }
        return Ok(Json::Array(out));
    }
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        out.push(decode_type_value(ty, c, ctx, scope)?);
    }
    Ok(Json::Array(out))
}

/// How many bytes a type holds, when it is a fixed size.
#[allow(dead_code)]
fn type_size(schema: &Schema, ty: &Type) -> Option<usize> {
    match ty {
        Type::Prim(p) => p.size(),
        // A named type: a struct of fixed-size fields has a fixed size.
        Type::Named(n) => {
            let td = schema.type_def(n)?;
            let mut total = 0usize;
            for f in &td.fields {
                total += type_size(schema, &f.ty)?;
            }
            Some(total)
        }
        // An array of a fixed size: the element's size times the count.
        Type::Array { of, count } => {
            let one = type_size(schema, of)?;
            match count {
                ArrayCount::Fixed(n) => Some(one * n),
                ArrayCount::Prefix(p) => Some(one + p.size()?),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Decode a primitive.
fn decode_prim(p: Prim, c: &mut Cursor<'_>) -> Result<Json, String> {
    let e = |e: nvre_util::OutOfBounds| e.to_string();
    Ok(match p {
        Prim::U8 => Json::from(c.u8().map_err(e)?),
        Prim::I8 => Json::from(c.i8().map_err(e)?),
        Prim::U16 => Json::from(c.u16().map_err(e)?),
        Prim::I16 => Json::from(c.i16().map_err(e)?),
        Prim::U32 => Json::from(c.u32().map_err(e)?),
        Prim::I32 => Json::from(c.i32().map_err(e)?),
        Prim::U64 => Json::from(c.u64().map_err(e)?),
        Prim::I64 => Json::from(c.i64().map_err(e)?),
        Prim::F32 => Json::from(c.f32().map_err(e)?),
        Prim::F64 => Json::from(c.f64().map_err(e)?),
        // The peek reads the number without consuming it: it belongs to
        // the field either way.
        Prim::PeekU32 => Json::from(
            c.peek_u32()
                .map_err(|oob: nvre_util::OutOfBounds| oob.to_string())?,
        ),
        Prim::Bool => Json::from(c.u8().map_err(e)? != 0),
        Prim::FormId => Json::from(c.u32().map_err(e)?),
        Prim::ZString => {
            let bytes = c.bytes(c.remaining()).map_err(e)?;
            Json::String(cp1252::decode_z(bytes))
        }
        Prim::String => {
            // The rest of the data is the text.
            let bytes = c.bytes(c.remaining()).map_err(e)?;
            Json::String(cp1252::decode(bytes))
        }
        Prim::Bytes => {
            let b = c.bytes(c.remaining()).map_err(e)?;
            Json::from(hex_of(b))
        }
        Prim::CString => {
            // xEdit's `wbString` inside a struct: text up to the first NUL,
            // with the cursor left just past that NUL so the fields that
            // follow still decode. A missing NUL means the string ran to
            // the end of the field, which is an error rather than a silent
            // truncation.
            let rest = c.bytes(c.remaining()).map_err(e)?;
            let Some(end) = rest.iter().position(|&b| b == 0) else {
                return Err(format!(
                    "the string is not NUL-terminated: {:?}",
                    cp1252::decode(rest)
                ));
            };
            // `bytes(n)` already moved the cursor to the end, so it is
            // rewound to just past the NUL.
            c.pos -= rest.len() - (end + 1);
            Json::String(cp1252::decode(&rest[..end]))
        }
        Prim::LenString => {
            // xEdit's `wbLenString`: a little-endian u32 byte count, then
            // exactly that many bytes of text with no terminator. A length
            // that runs past the end of the field is an error, never a
            // truncation.
            let n = c.u32().map_err(e)? as usize;
            let bytes = c
                .bytes(n)
                .map_err(|oob| format!("the length {n} exceeds the field: {oob}"))?;
            Json::String(cp1252::decode(bytes))
        }
        Prim::ZStringArray => {
            // A list of NUL-separated strings, closed by a NUL.
            let rest = c.bytes(c.remaining()).map_err(e)?;
            // The list is closed by a NUL, and one more NUL may close it.
            let body = rest.strip_suffix(&[0]).unwrap_or(rest);
            let body = body.strip_suffix(&[0]).unwrap_or(body);
            Json::Array(
                body.split(|&b| b == 0)
                    .map(|s| Json::String(cp1252::decode(s)))
                    .collect(),
            )
        }
    })
}

/// Bytes as lower-case hex.
fn hex_of(b: &[u8]) -> String {
    nvre_util::sha256::to_hex(b)
}

/// Decode a record, and measure how much of it the schema explains.
pub fn decode_record(r: &Record, schema: &Schema) -> (Json, RecordCoverage) {
    let mut ctx = DecodeCtx::new(schema, r.header.sig);
    let mut cov = RecordCoverage {
        sig: r.header.sig,
        form_id: r.header.form_id,
        ..Default::default()
    };
    let mut m = Map::new();
    // A record that is a sequence is read as one: its subrecords come in
    // blocks, and a subrecord's meaning depends on the block it is in.
    let rec_key = String::from_utf8_lossy(&r.header.sig).into_owned();
    let rec_sig = r.header.sig;
    let sequence = schema
        .records
        .get(&rec_key)
        .and_then(|d| d.sequence.clone());
    // The record's subrecords are split from its uncompressed payload: a
    // compressed record's payload is a compressed stream, so it has to be
    // decompressed before it can be split.
    // The subrecords of this record that are exposed: their values are
    // published for the unions beside them.
    let exposed_subs: Vec<String> = schema
        .records
        .get(&rec_key)
        .map(|d| {
            d.subrecords
                .iter()
                .filter(|(_, s)| s.expose)
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    let subs = match r.subrecords() {
        Ok(s) => s,
        Err(e) => {
            cov.error_count += 1;
            cov.error_count += 1;
            return (Json::Object(m), cov);
        }
    };
    let decoded = match sequence {
        Some(seq) => decode_sequence(&seq, &subs, rec_sig, &mut ctx, &mut cov),
        None => {
            for s in &subs {
                let d = decode_sub(&r.header.sig, s, &mut ctx);
                count(&mut cov, &d, s.data.len());
                if let Some(v) = d.value.clone() {
                    m.insert(d.sig_str(), v.clone());
                    // The value is exposed to the subrecords that come
                    // after it: the `PERK` record's `DATA` is read against
                    // the `PRKE` beside it.
                    if let Some(n) = switch_value(&v) {
                        ctx.exposed.insert((rec_key.clone(), d.sig_str()), n);
                    }
                    // A subrecord that is exposed is published under its
                    // own name, so a union beside it can switch on it: the
                    // `EPFD` record's `DATA` is read against the `EPFT`
                    // beside it.
                    if exposed_subs.contains(&d.sig_str()) {
                        if let Some(n) = switch_value(&v) {
                            ctx.exposed.insert((rec_key.clone(), d.sig_str()), n);
                        }
                    }
                }
            }
            m
        }
    };
    (Json::Object(decoded), cov)
}

/// Add a decoded subrecord to the coverage.
fn count(cov: &mut RecordCoverage, d: &DecodedSub, size: usize) {
    match d.status {
        SubStatus::Ok => cov.ok_count += 1,
        SubStatus::Unknown => cov.unknown_count += 1,
        SubStatus::Trailing { .. } => cov.trailing_count += 1,
        SubStatus::Error(_) => cov.error_count += 1,
    }
    cov.subrecords.push(SubCoverage {
        sig: d.sig,
        status: d.status.clone(),
        size,
    });
}

/// The number a value holds, when it is one: the switch value a union's
/// variant is chosen by.
fn switch_value(v: &Json) -> Option<i64> {
    match v {
        Json::Number(n) => n.as_i64(),
        Json::Object(m) => m.get("value").and_then(|v| v.as_i64()),
        _ => None,
    }
}

/// Decode a union whose variant is chosen by the field's first word.
///
/// The word is read without consuming it: it belongs to the field either
/// way, so the whole field is the union.
fn decode_peek_union(
    pu: &PeekUnion,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
    scope: &Map<String, Json>,
) -> Result<Json, String> {
    if c.remaining() < 4 {
        return Err(format!(
            "the union reads a word of 4 bytes at offset {} but only {} are left",
            c.pos,
            c.remaining()
        ));
    }
    let word = c.peek_u32().map_err(|e: nvre_util::OutOfBounds| e.to_string())? as i64;
    let idx = pu.cases.get(&word).copied().unwrap_or(pu.default);
    let Some(variant) = pu.variants.get(idx) else {
        return Err(format!("the union has no variant {idx}"));
    };
    decode_type_value(&variant.ty, c, ctx, scope)
}

/// Decode a union whose variant is chosen by another field's value.
///
/// The decider is a field of the same struct, or of an earlier subrecord of
/// the same record — the `PERK` record's `DATA` is one thing or another
/// depending on the `PRKE` beside it. A missing decider selects the
/// default.
fn decode_value_union(
    vu: &ValueUnion,
    c: &mut Cursor<'_>,
    ctx: &mut DecodeCtx<'_>,
    scope: &Map<String, Json>,
) -> Result<Json, String> {
    // The decider is looked for in three places, in order: the struct being
    // read, the record's own published values, and the record's published
    // subrecords.
    let switch = scope
        .get(&vu.switch_on)
        .and_then(|v| v.as_i64())
        .or_else(|| {
            ctx.switch_on
                .as_ref()
                .and_then(|m| m.get(&vu.switch_on).copied())
        })
        .or_else(|| {
            // The decider may be a subrecord of the same record that was
            // decoded earlier.
            ctx.exposed
                .get(&(
                    String::from_utf8_lossy(&ctx.rec_sig).into_owned(),
                    vu.switch_on.clone(),
                ))
                .copied()
        });
    let idx = match switch {
        Some(v) => vu.cases.get(&v).copied().unwrap_or(vu.default),
        None => vu.default,
    };
    let Some(variant) = vu.variants.get(idx) else {
        return Err(format!("the union has no variant {idx}"));
    };
    decode_type_value(&variant.ty, c, ctx, scope)
}

/// Decode a record that is a sequence of blocks.
///
/// The subrecords before the first block are the record's own; then each
/// block is opened by its signature, and the subrecords that follow belong to
/// it until the next block opens.
fn decode_sequence(
    seq: &SequenceDef,
    subs: &[Subrecord],
    rec_sig: [u8; 4],
    ctx: &mut DecodeCtx<'_>,
    cov: &mut RecordCoverage,
) -> Map<String, Json> {
    let mut m = Map::new();
    // The block that is open, and where in it we are.
    let mut open: Option<&BlockDef> = None;
    // The subrecords of the block that is open.
    let mut block_subs: Vec<(String, Json)> = Vec::new();
    for s in subs {
        let sig = String::from_utf8_lossy(&s.sig).into_owned();
        // A signature that opens a block starts one — unless the block that
        // is open expects it, in which case it is that block's own
        // subrecord. The order is what tells the two apart: a signature may
        // both open a block and be a subrecord of another.
        let expected_here = open.map(|b| b.order.contains(&sig)).unwrap_or(false);
        let opener = if expected_here {
            None
        } else {
            seq.blocks.values().find(|b| b.opener == sig)
        };
        if let Some(b) = opener {
            // The block that was open ends.
            if let Some(prev) = open.take() {
                store_block(&mut m, prev, &mut block_subs);
            }
            open = Some(b);
            block_subs.clear();
            // A new block starts with no variant chosen.
            ctx.block_variant = None;
            // The opener itself is a subrecord of the block.
            let d = decode_sub_in(b, &sig, s, ctx, &m);
            count(cov, &d, s.data.len());
            if let Some(v) = d.value.clone() {
                block_subs.push((sig, v.clone()));
                // A block that switches on its opener's value keeps that
                // choice for the whole block, not just for the opener: the
                // field after it is decoded against that type's definition.
                if !b.variants.is_empty() {
                    if let Some(name) = variant_name(&v) {
                        if let Some(var) = b.variants.get(&name) {
                            ctx.block_variant = Some(var.subrecords.clone());
                        }
                    }
                }
            }
            continue;
        }
        // A subrecord of the block that is open, or of the record. The
        // block's chosen variant says which fields the block holds, so it
        // is asked first.
        let in_block = open.is_some()
            && (ctx.block_variant.as_ref().map(|v| v.contains_key(&sig)).unwrap_or(false)
                || open.unwrap().subrecords.contains_key(&sig));
        let d = match (open, in_block) {
            (Some(b), true) => decode_sub_in(b, &sig, s, ctx, &m),
            _ => decode_sub(&rec_sig, s, ctx),
        };
        count(cov, &d, s.data.len());
        match d.value.clone() {
            Some(v) => {
                if open.is_some() {
                    block_subs.push((sig, v));
                } else {
                    m.insert(sig, v);
                }
            }
            None => {}
        }
    }
    // The last block ends with the record.
    if let Some(prev) = open {
        store_block(&mut m, prev, &mut block_subs);
    }
    m
}

/// The name a variant is chosen by, when the value names one.
fn variant_name(v: &Json) -> Option<String> {
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Object(m) => m.get("value").and_then(|v| v.as_str()).map(str::to_string),
        _ => None,
    }
}

/// Walk a record's subrecords, giving each one to the caller.
///
/// A record that is a sequence is walked a block at a time: the context
/// says which block is open, so a subrecord's meaning is known before it
/// is read.
pub fn walk_sequence(
    rec_sig: &[u8; 4],
    subs: &[Subrecord],
    schema: &Schema,
    ctx: &mut DecodeCtx<'_>,
    mut f: impl FnMut(&Subrecord, &mut DecodeCtx<'_>),
) {
    let seq = schema
        .records
        .get(String::from_utf8_lossy(rec_sig).as_ref())
        .and_then(|rd| rd.sequence.as_ref());
    let Some(seq) = seq else {
        for s in subs {
            f(s, ctx);
        }
        return;
    };
    // The block that is open, and the variant its opener chose.
    let mut open: Option<&BlockDef> = None;
    for s in subs {
        let sig = String::from_utf8_lossy(&s.sig).into_owned();
        // A signature that opens a block starts one, unless the block that
        // is open expects it.
        let expected_here = open.map(|b| b.order.contains(&sig)).unwrap_or(false);
        if !expected_here {
            if let Some(b) = seq.blocks.values().find(|b| b.opener == sig) {
                if let Some(prev) = open {
                    ctx.block_variant = None;
                    let _ = prev;
                }
                open = Some(b);
                ctx.block_variant = None;
            }
        }
        f(s, ctx);
        // The opener's value picks the block's variant.
        if let Some(b) = open {
            if b.opener == sig && !b.variants.is_empty() {
                let value = ctx.values.get(&sig).and_then(variant_name);
                if let Some(name) = value {
                    if let Some(var) = b.variants.get(&name) {
                        ctx.block_variant = Some(var.subrecords.clone());
                    }
                }
            }
        }
    }
}

/// Store a block's subrecords under the block's name.
fn store_block(m: &mut Map<String, Json>, b: &BlockDef, subs: &mut Vec<(String, Json)>) {
    if subs.is_empty() {
        return;
    }
    let mut inner = Map::new();
    for (k, v) in subs.drain(..) {
        inner.insert(k, v);
    }
    let name = b.opener.clone();
    m.insert(name, Json::Object(inner));
}

/// How well one subrecord was explained.
#[derive(Debug, Clone)]
pub struct SubCoverage {
    /// The subrecord's four-byte type.
    pub sig: [u8; 4],
    /// How well it was explained.
    pub status: SubStatus,
    /// How many bytes the subrecord holds.
    pub size: usize,
}

/// How well one record was explained.
#[derive(Debug, Clone, Default)]
pub struct RecordCoverage {
    /// The record's four-byte type.
    pub sig: [u8; 4],
    /// The record's FormID.
    pub form_id: u32,
    /// Each subrecord's coverage.
    pub subrecords: Vec<SubCoverage>,
    /// How many subrecords were fully explained.
    pub ok_count: u64,
    /// How many the schema does not know.
    pub unknown_count: u64,
    /// How many had bytes left over.
    pub trailing_count: u64,
    /// How many could not be read.
    pub error_count: u64,
}

impl RecordCoverage {
    /// How many of the record's subrecords were fully explained.
    pub fn ok(&self) -> u64 {
        self.ok_count
    }

    /// How many of the record's subrecords the schema does not know.
    pub fn unknown(&self) -> u64 {
        self.unknown_count
    }

    /// How many of the record's subrecords had bytes left over.
    pub fn trailing(&self) -> u64 {
        self.trailing_count
    }

    /// How many of the record's subrecords could not be read.
    pub fn error(&self) -> u64 {
        self.error_count
    }
}

/// The coverage of a whole file.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    /// The records, by their four-byte type.
    pub by_type: HashMap<String, RecordCoverage>,
    /// How many subrecords were fully explained.
    pub ok: u64,
    /// How many the schema does not know.
    pub unknown: u64,
    /// How many had bytes left over.
    pub trailing: u64,
    /// How many could not be read.
    pub error: u64,
    /// How many subrecords there were.
    pub total: u64,
}

/// How well one subrecord signature was explained.
#[derive(Debug, Clone, Default)]
pub struct SignatureCoverage {
    /// How many there were.
    pub total: u64,
    /// How many were fully explained.
    pub ok: u64,
    /// How many the schema does not know.
    pub unknown: u64,
    /// How many had bytes left over.
    pub trailing: u64,
    /// How many could not be read.
    pub error: u64,
    /// The sizes the subrecords were, by how many of each.
    pub sizes: BTreeMap<usize, u64>,
    /// The first error, for reporting.
    pub first_error: Option<String>,
}

impl Coverage {
    /// The coverage of each subrecord signature, by the record that holds
    /// it and the subrecord's own signature.
    pub fn by_sub(&self) -> BTreeMap<(String, String), SignatureCoverage> {
        let mut out: BTreeMap<(String, String), SignatureCoverage> = BTreeMap::new();
        for (rec, rc) in &self.by_type {
            for sc in &rc.subrecords {
                let sig = String::from_utf8_lossy(&sc.sig).into_owned();
                let e = out.entry((rec.clone(), sig)).or_default();
                e.total += 1;
                *e.sizes.entry(sc.size).or_insert(0) += 1;
                match &sc.status {
                    SubStatus::Ok => e.ok += 1,
                    SubStatus::Unknown => e.unknown += 1,
                    SubStatus::Trailing { used, total } => {
                        e.trailing += 1;
                        if e.first_error.is_none() {
                            e.first_error = Some(format!(
                                "{total} bytes, {used} read: {} bytes left over",
                                total - used
                            ));
                        }
                    }
                    SubStatus::Error(m) => {
                        e.error += 1;
                        if e.first_error.is_none() {
                            e.first_error = Some(m.clone());
                        }
                    }
                }
            }
        }
        out
    }

    /// The totals: total, ok, unknown, trailing, error.
    pub fn totals(&self) -> (u64, u64, u64, u64, u64) {
        (self.total, self.ok, self.unknown, self.trailing, self.error)
    }

    /// Measure a record, and add it to the totals.
    /// Measure a list of records and fold them into the totals.
    pub fn add_records(&mut self, recs: &[&Record], schema: &Schema) {
        for r in recs {
            let (_value, rc) = decode_record(r, schema);
            self.add(rc);
        }
    }

    /// The totals, as a JSON document.
    pub fn to_json(&self, max_rows: usize) -> Json {
        let (total, ok, unknown, trailing, error) = self.totals();
        let pct = |n: u64| {
            if total == 0 {
                0.0
            } else {
                (n as f64) * 100.0 / total as f64
            }
        };
        let mut summary = Map::new();
        summary.insert("subrecords".into(), Json::from(total));
        summary.insert("ok".into(), Json::from(ok));
        summary.insert("unknown".into(), Json::from(unknown));
        summary.insert("trailing".into(), Json::from(trailing));
        summary.insert("error".into(), Json::from(error));
        summary.insert("percent".into(), Json::from(pct(ok)));
        let mut by_sub = Map::new();
        let subs = self.by_sub();
        let mut rows: Vec<_> = subs.iter().collect();
        rows.sort_by(|a, b| {
            b.1.total
                .saturating_sub(b.1.ok)
                .cmp(&a.1.total.saturating_sub(a.1.ok))
                .then(a.0.cmp(b.0))
        });
        for ((rec, sig), c) in rows.into_iter().take(max_rows) {
            let mut o = Map::new();
            o.insert("record".into(), Json::String(rec.clone()));
            o.insert("subrecord".into(), Json::String(sig.clone()));
            o.insert("total".into(), Json::from(c.total));
            o.insert("ok".into(), Json::from(c.ok));
            o.insert("unknown".into(), Json::from(c.unknown));
            o.insert("trailing".into(), Json::from(c.trailing));
            o.insert("error".into(), Json::from(c.error));
            by_sub.insert(format!("{rec}.{sig}"), Json::Object(o));
        }
        let mut out = Map::new();
        out.insert("summary".into(), Json::Object(summary));
        out.insert("by_subrecord".into(), Json::Object(by_sub));
        Json::Object(out)
    }

    pub fn add(&mut self, rc: RecordCoverage) {
        self.ok += rc.ok();
        self.unknown += rc.unknown();
        self.trailing += rc.trailing();
        self.error += rc.error();
        self.total += rc.subrecords.len() as u64;
        let key = String::from_utf8_lossy(&rc.sig).into_owned();
        match self.by_type.get_mut(&key) {
            Some(e) => {
                e.ok_count += rc.ok();
                e.unknown_count += rc.unknown();
                e.trailing_count += rc.trailing();
                e.error_count += rc.error();
            }
            None => {
                self.by_type.insert(key, rc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{RecordHeader, Subrecord};

    fn schema() -> Schema {
        let j: Json = serde_json::from_str(
            r#"{"format":"nvre.esm.schema/1",
                "records":{"TEST":{"subrecords":{
                  "NAME":{"type":"formid"},
                  "DATA":{"type":"PosRot"}}}},
                "types":{"PosRot":{"struct":[
                  {"name":"x","type":"f32"},
                  {"name":"y","type":"f32"},
                  {"name":"z","type":"f32"}]}}}"#,
        )
        .unwrap();
        let mut s = Schema::new();
        s.merge(&j).unwrap();
        s
    }

    fn record(subs: Vec<(&[u8; 4], Vec<u8>)>) -> Record {
        Record {
            header: RecordHeader {
                sig: *b"TEST",
                data_size: 0,
                flags: 0,
                form_id: 1,
                vc_info: 0,
                form_version: 0,
                vc_info2: 0,
            },
            subrecords: subs
                .into_iter()
                .map(|(sig, data)| Subrecord {
                    sig: *sig,
                    data: bytes::Bytes::from(data),
                })
                .collect(),
            payload: bytes::Bytes::new(),
            offset: 0,
        }
    }

    #[test]
    fn a_record_decodes() {
        let s = schema();
        let mut data = Vec::new();
        data.extend_from_slice(&42u32.to_le_bytes());
        let r = record(vec![(b"NAME", data)]);
        let (v, cov) = decode_record(&r, &s);
        assert_eq!(v["NAME"], Json::from(42));
        assert_eq!(cov.ok(), 1);
        assert_eq!(cov.error(), 0);
    }

    #[test]
    fn a_subrecord_the_schema_does_not_know_is_unknown() {
        let s = schema();
        let r = record(vec![(b"ZZZZ", vec![1, 2, 3, 4])]);
        let (_, cov) = decode_record(&r, &s);
        assert_eq!(cov.unknown(), 1);
    }

    #[test]
    fn a_subrecord_with_bytes_left_over_is_trailing() {
        let s = schema();
        let mut data = Vec::new();
        data.extend_from_slice(&42u32.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        let r = record(vec![(b"NAME", data)]);
        let (_, cov) = decode_record(&r, &s);
        assert_eq!(cov.trailing(), 1);
    }
}
