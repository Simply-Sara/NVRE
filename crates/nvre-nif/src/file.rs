//! A whole NIF file: the header, the blocks, and the string table.
//!
//! The container is split into blocks before anything is decoded, so a reader
//! can say which block is which type, and where it is, without knowing any
//! layouts. The schema is then used to decode each block's fields.

use crate::codec::{CodecError, Reader};
use crate::header::{Header, NifError};
use crate::schema::{Field, Length, Prim, Schema, TypeRef, Width};
use crate::value::{Struct, Value};
use std::collections::HashMap;
use std::ops::Range;

/// A block, as it appears in the file: its type, and where it is.
#[derive(Debug, Clone)]
pub struct RawBlock {
    /// The block's type, as the schema names it.
    pub type_name: String,
    /// Where the block is in the file.
    pub range: Range<usize>,
}

/// How well the schema explains one block.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockStatus {
    /// Decoded, and used exactly the bytes the block holds.
    Ok,
    /// The schema has no definition for this block type.
    Unknown,
    /// Decoded, but left bytes over.
    Short {
        /// How many bytes the reading used.
        consumed: usize,
        /// How many bytes the block holds.
        size: usize,
    },
    /// The reading failed.
    Error(CodecError),
}

impl BlockStatus {
    /// A short name for the status.
    pub fn code(&self) -> &'static str {
        match self {
            BlockStatus::Ok => "ok",
            BlockStatus::Unknown => "unknown",
            BlockStatus::Short { .. } => "short",
            BlockStatus::Error(_) => "error",
        }
    }
}

/// A whole NIF file.
#[derive(Debug, Clone)]
pub struct NifFile {
    /// The file's header.
    pub header: Header,
    /// The blocks, in the order they appear.
    pub blocks: Vec<RawBlock>,
    /// The bytes after the last block.
    pub trailing: usize,
}

impl NifFile {
    /// Split the file into its blocks.
    ///
    /// From 20.2.0.5 the file records each block's size, so the blocks can
    /// be split without a schema. Before that, the blocks have to be decoded
    /// to find their ends, which needs a schema.
    pub fn parse(data: &[u8], schema: Option<&Schema>) -> Result<NifFile, NifError> {
        let header = Header::parse(data)?;
        let mut blocks = Vec::with_capacity(header.num_blocks as usize);
        let mut at = header.header_len;

        match &header.block_sizes {
            Some(sizes) => {
                for (i, size) in sizes.iter().enumerate() {
                    let end = at + *size as usize;
                    let type_name = header.block_type_name(i).unwrap_or_default();
                    blocks.push(RawBlock {
                        type_name,
                        range: at..end,
                    });
                    at = end;
                }
            }
            None => {
                // No sizes: the blocks have to be decoded to find their ends.
                let schema = schema.ok_or_else(|| {
                    NifError::Invalid(
                        "this version does not record block sizes, so a schema is needed to split the file"
                            .into(),
                    )
                })?;
                for i in 0..header.num_blocks as usize {
                    let type_name = header.block_type_name(i).unwrap_or_default();
                    let mut r = Reader::new(&data[at..]);
                    let used = decode_block_bytes(schema, &header, &type_name, &mut r)
                        .map_err(|e| NifError::Invalid(format!("block {i} ({type_name}): {e}")))?;
                    blocks.push(RawBlock {
                        type_name,
                        range: at..at + used,
                    });
                    at += used;
                }
            }
        }
        let trailing = data.len().saturating_sub(at);
        Ok(NifFile {
            header,
            blocks,
            trailing,
        })
    }

    /// Decode one block.
    pub fn decode_block(
        &self,
        index: usize,
        schema: &Schema,
        data: &[u8],
    ) -> Result<Struct, CodecError> {
        let b = &self.blocks[index];
        let mut r = Reader::new(&data[b.range.start..b.range.end]);
        decode_block(schema, &self.header, &b.type_name, &mut r)
    }

    /// Decode a block, and say how well the schema explained it.
    pub fn decode_block_status(
        &self,
        index: usize,
        schema: &Schema,
        data: &[u8],
    ) -> (BlockStatus, Option<Struct>) {
        let b = &self.blocks[index];
        if !schema.blocks.contains_key(&b.type_name) {
            return (BlockStatus::Unknown, None);
        }
        let mut r = Reader::new(&data[b.range.start..b.range.end]);
        match decode_block(schema, &self.header, &b.type_name, &mut r) {
            Ok(s) => {
                let consumed = r.pos;
                let size = b.range.end - b.range.start;
                let st = if consumed == size {
                    BlockStatus::Ok
                } else {
                    BlockStatus::Short { consumed, size }
                };
                (st, Some(s))
            }
            Err(e) => (BlockStatus::Error(e), None),
        }
    }

    /// Whether a block re-encodes to the bytes it was read from.
    ///
    /// The block is decoded and written out again, and the two are
    /// compared. A block that decodes but does not re-encode is a block the
    /// schema read with the wrong layout — which is what a `bs_version`
    /// condition written for another game does.
    pub fn reencode_block(
        &self,
        schema: &Schema,
        data: &[u8],
        index: usize,
    ) -> Result<bool, CodecError> {
        let st = self.decode_block(index, schema, data)?;
        self.reencode_decoded(schema, data, index, &st)
    }

    /// Whether an already-decoded block re-encodes to the bytes it was read
    /// from.
    ///
    /// The same check [`NifFile::reencode_block`] makes, on a block the caller
    /// has decoded for another reason: a corpus pass asks both "did it decode
    /// cleanly?" and "does it re-encode?", and decoding twice doubles the cost
    /// of the more expensive half.
    pub fn reencode_decoded(
        &self,
        schema: &Schema,
        data: &[u8],
        index: usize,
        st: &Struct,
    ) -> Result<bool, CodecError> {
        let b = &self.blocks[index];
        let Some(fields) = schema.block_fields_ref(&b.type_name) else {
            return Err(CodecError::Layout(format!(
                "the unknown block type {:?}",
                b.type_name
            )));
        };
        // The lineage is the block's own type and the types it is based on.
        let mut lineage: Vec<String> = vec![b.type_name.clone()];
        lineage.extend(
            schema
                .block_chain(&b.type_name)
                .into_iter()
                .map(str::to_string),
        );
        let mut out = Vec::with_capacity(b.range.len());
        crate::encode::encode_fields(schema, &self.header, &lineage, &fields, st, None, &mut out)?;
        Ok(out == data[b.range.clone()])
    }
}

/// Decode a block's fields, and return how many bytes it used.
pub fn decode_block_bytes(
    schema: &Schema,
    header: &Header,
    type_name: &str,
    r: &mut Reader<'_>,
) -> Result<usize, CodecError> {
    let _ = decode_block(schema, header, type_name, r)?;
    Ok(r.pos)
}

/// Decode a block's fields.
pub fn decode_block(
    schema: &Schema,
    header: &Header,
    type_name: &str,
    r: &mut Reader<'_>,
) -> Result<Struct, CodecError> {
    let Some(fields) = schema.block_fields_ref(type_name) else {
        return Err(CodecError::Layout(format!(
            "no definition for the block type {type_name:?}"
        )));
    };
    let mut scope = file_scope(header);
    let mut out = Struct::new();
    let ctx = BlockCtx {
        type_name: type_name.to_string(),
        version: header.version,
    };
    read_fields(schema, header, &fields, &mut scope, &mut out, r, None, &ctx)?;
    Ok(out)
}

/// What the reading knows about the block it is reading.
#[derive(Debug, Clone)]
pub struct BlockCtx {
    /// The block's type, as the schema names it.
    pub type_name: String,
    /// The file's version, for the fields that appeared in one.
    pub version: crate::header::Version,
}

impl BlockCtx {
    /// Whether the block is of type `name`, or of a type based on it.
    fn is(&self, schema: &Schema, name: &str) -> bool {
        schema.is_a(&self.type_name, name)
    }
}

/// The scope every block starts with: the file's own state.
///
/// The version, the user version and the Bethesda version, which the
/// schema's conditions are written against.
pub fn file_scope(header: &Header) -> HashMap<String, Value> {
    let mut scope = HashMap::new();
    scope.insert("version".into(), Value::Num(header.version as f64));
    scope.insert(
        "user_version".into(),
        Value::Num(header.user_version as f64),
    );
    scope.insert("bs_version".into(), Value::Num(header.bs_version() as f64));
    scope
}

/// Read a list of fields into a struct.
#[allow(clippy::too_many_arguments)]
fn read_fields(
    schema: &Schema,
    header: &Header,
    fields: &[Field],
    scope: &mut HashMap<String, Value>,
    out: &mut Struct,
    r: &mut Reader<'_>,
    template: Option<&str>,
    ctx: &BlockCtx,
) -> Result<(), CodecError> {
    for f in fields {
        // The file version the field appeared in.
        if let Some(since) = f.since {
            if ctx.version < since {
                continue;
            }
        }
        if let Some(until) = f.until {
            if ctx.version > until {
                continue;
            }
        }
        // A condition: the field is read only when it holds.
        if let Some(cond) = &f.cond {
            if !cond.evaluate(scope) {
                continue;
            }
        }
        // The field belongs to some block types and not to others.
        if !f.only.is_empty() && !f.only.iter().any(|n| ctx.is(schema, n)) {
            continue;
        }
        if f.exclude.iter().any(|n| ctx.is(schema, n)) {
            continue;
        }
        let at = r.pos;
        let value = read_field(schema, header, f, scope, r, template, ctx)?;
        let Some(value) = value else { continue };
        if std::env::var("NVRE_NIF_TRACE").is_ok() {
            eprintln!("  {}+{}..{} = {:?}", f.name, at, r.pos, value);
        }
        // The scope holds the fields read so far, so a later field's
        // condition or length can use them.
        //
        // The value goes in as it is, including an array: a *width* is the
        // name of an earlier field and reads that field's elements as the row
        // widths — `NiTriStripsData.points` is as wide as `strip_lengths`
        // says, per row — so an array has to stay reachable by name, and
        // holding its length instead would read one row where the file has
        // many. The clone is the price of that.
        scope.insert(f.name.clone(), value.clone());
        out.set(&f.name, value);
    }
    Ok(())
}

/// Read one field.
fn read_field(
    schema: &Schema,
    header: &Header,
    f: &Field,
    scope: &mut HashMap<String, Value>,
    r: &mut Reader<'_>,
    template: Option<&str>,
    ctx: &BlockCtx,
) -> Result<Option<Value>, CodecError> {
    // The length of an array, if it has one.
    let length = match &f.length {
        None => None,
        Some(Length::Fixed(n)) => Some(*n),
        Some(Length::Expr(e)) => {
            // A field that was not read is zero, not an error: the schema
            // says "the UV sets are as many as the flags say", and only one
            // of the two flag fields is present in a given version.
            let n = e.evaluate_number(scope).unwrap_or(0.0);
            if n < 0.0 {
                return Err(CodecError::Layout(format!(
                    "the length of {:?} is negative",
                    f.name
                )));
            }
            Some(n as usize)
        }
        Some(Length::Field(name)) => {
            // A field that is not there is zero: the field may be
            // conditional, and whether it is there is the schema's business.
            let Some(v) = scope.get(name) else {
                return Ok(Some(Value::List(Vec::new())));
            };
            Some(v.as_number().ok_or_else(|| {
                CodecError::Layout(format!("the length of {:?} is not a number", f.name))
            })? as usize)
        }
    };

    // A named type: a struct type, an enum, or one of the built-in struct
    // types the schema language has.
    if let TypeRef::Named(n) = &f.ty {
        // An enum is read as its storage type.
        if let Some(e) = schema.enums.get(n) {
            let v = crate::codec::read_prim(e.storage, r, header.version)?;
            return Ok(Some(v));
        }
        // A sized string: a length and that many bytes. A field with a
        // length is an array of them — the texture set of a shape names
        // several files.
        if n == "sized_string" {
            let mut strings = Vec::new();
            for _ in 0..length.unwrap_or(1) {
                let len = r.int(4, false)? as usize;
                let b = r.bytes(len)?;
                strings.push(Value::Str(nvre_util::cp1252::decode(b)));
            }
            return Ok(Some(if strings.len() == 1 {
                strings
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Str(String::new()))
            } else {
                Value::List(strings)
            }));
        }
        let fields = schema
            .struct_fields_ref(n)
            .ok_or_else(|| CodecError::Layout(format!("the unknown struct type {n:?}")))?;
        // The template says what `T` stands for inside the struct. A
        // template of "T" means "the enclosing template", so it is passed
        // through; anything else names the type outright.
        let sub_template: Option<String> = match f.template.as_deref() {
            Some("T") => template.map(str::to_string),
            Some(t) => Some(t.to_string()),
            None => schema
                .structs
                .get(n)
                .and_then(|s| s.template.clone())
                .or_else(|| template.map(str::to_string)),
        };
        // An array of structs: the struct is read once per element.
        if let Some(count) = length {
            let mut out = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                let mut sub = Struct::new();
                let mut sub_scope = scope.clone();
                if let Some(arg) = f.arg.as_ref().and_then(|e| e.evaluate_number(scope).ok()) {
                    sub_scope.insert("arg".into(), Value::Num(arg));
                }
                read_fields(
                    schema,
                    header,
                    &fields,
                    &mut sub_scope,
                    &mut sub,
                    r,
                    sub_template.as_deref(),
                    ctx,
                )?;
                out.push(Value::Struct(sub));
            }
            return Ok(Some(Value::List(out)));
        }
        let mut sub = Struct::new();
        let mut sub_scope = scope.clone();
        if let Some(arg) = f.arg.as_ref().and_then(|e| e.evaluate_number(scope).ok()) {
            sub_scope.insert("arg".into(), Value::Num(arg));
        }
        read_fields(
            schema,
            header,
            &fields,
            &mut sub_scope,
            &mut sub,
            r,
            sub_template.as_deref(),
            ctx,
        )?;
        return Ok(Some(Value::Struct(sub)));
    }

    // The enclosing type's argument: the type parameter `T`.
    if matches!(f.ty, TypeRef::Template) {
        let t = template.ok_or_else(|| {
            CodecError::Layout(format!(
                "{:?} has no enclosing type to take its type from",
                f.name
            ))
        })?;
        // The template is a type name, and it may be a primitive.
        if let Some(p) = crate::schema::Prim::parse(t) {
            return Ok(Some(crate::codec::read_prim(p, r, header.version)?));
        }
        let fields = schema
            .struct_fields_ref(t)
            .ok_or_else(|| CodecError::Layout(format!("the unknown struct type {t:?}")))?;
        let mut sub = Struct::new();
        let mut sub_scope = scope.clone();
        read_fields(
            schema,
            header,
            &fields,
            &mut sub_scope,
            &mut sub,
            r,
            template,
            ctx,
        )?;
        return Ok(Some(Value::Struct(sub)));
    }

    let TypeRef::Prim(prim) = f.ty else {
        unreachable!()
    };

    // An array.
    if let Some(n) = length {
        // A byte array of a known length is raw bytes.
        if prim == Prim::Bytes {
            let b = r.bytes(n)?;
            return Ok(Some(Value::Bytes(b.to_vec())));
        }
        // An array of structs: the struct is read once per element.
        if let TypeRef::Named(name) = &f.ty {
            if name != "sized_string" && !schema.enums.contains_key(name) {
                let sub_fields = schema.struct_fields_ref(name).ok_or_else(|| {
                    CodecError::Layout(format!("the unknown struct type {name:?}"))
                })?;
                let sub_template: Option<String> = match f.template.as_deref() {
                    Some("T") => template.map(str::to_string),
                    Some(t) => Some(t.to_string()),
                    None => schema
                        .structs
                        .get(name)
                        .and_then(|s| s.template.clone())
                        .or_else(|| template.map(str::to_string)),
                };
                let mut out = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let mut sub = Struct::new();
                    let mut sub_scope = scope.clone();
                    if let Some(arg) = f.arg.as_ref().and_then(|e| e.evaluate_number(scope).ok()) {
                        sub_scope.insert("arg".into(), Value::Num(arg));
                    }
                    read_fields(
                        schema,
                        header,
                        &sub_fields,
                        &mut sub_scope,
                        &mut sub,
                        r,
                        sub_template.as_deref(),
                        ctx,
                    )?;
                    out.push(Value::Struct(sub));
                }
                return Ok(Some(Value::List(out)));
            }
        }
        // A two-dimensional array: each row is as wide as the width says.
        if let Some(w) = &f.width {
            // The widths of the rows: either one width for every row, or one
            // width per row, from an earlier array of counts.
            let widths: Vec<usize> = match w {
                Width::Fixed(k) => vec![*k; n],
                Width::Expr(e) => vec![e.evaluate_number(scope).unwrap_or(0.0) as usize; n],
                Width::Field(name) => {
                    // A field that is not there is zero: the field may be
                    // conditional, and whether it is there is the schema's
                    // business.
                    let Some(v) = scope.get(name) else {
                        return Ok(Some(Value::List(Vec::new())));
                    };
                    match v.as_list() {
                        // An array of counts: one width per row.
                        Some(l) => l
                            .iter()
                            .map(|x| x.as_number().unwrap_or(0.0) as usize)
                            .collect(),
                        // A single count: the same width for every row.
                        None => {
                            let w = v.as_number().ok_or_else(|| {
                                CodecError::Layout(format!(
                                    "the width of {:?} is not a number",
                                    f.name
                                ))
                            })? as usize;
                            vec![w; n]
                        }
                    }
                }
            };
            let mut out = Vec::with_capacity(n.min(4096));
            for i in 0..n {
                let width = widths.get(i).copied().unwrap_or(0);
                let mut row = Vec::with_capacity(width.min(4096));
                for _ in 0..width {
                    row.push(crate::codec::read_prim(prim, r, header.version)?);
                }
                out.push(Value::List(row));
            }
            return Ok(Some(Value::List(out)));
        }
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            out.push(crate::codec::read_prim(prim, r, header.version)?);
        }
        return Ok(Some(Value::List(out)));
    }

    Ok(Some(crate::codec::read_prim(prim, r, header.version)?))
}
