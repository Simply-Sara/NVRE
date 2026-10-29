//! Writing a NIF's blocks back out.
//!
//! The encoder is the inverse of the decoder in [`crate::file`]: a block's
//! decoded value is written out again, byte for byte, when the schema
//! describes the layout exactly. What the schema does not describe is
//! written as it was read, so a block that decodes but does not re-encode
//! is a block the schema read with the wrong layout.

use crate::codec::f32_to_half;
use crate::codec::CodecError;
use crate::header::{Header, Version, V20_1_0_3};
use crate::schema::{Prim, Schema, TypeRef};
use crate::value::{Struct, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The state a field's length and condition are read against.
///
/// A stack of frames, innermost last: the file's own state at the bottom,
/// then each enclosing struct's fields, and the struct being written's own
/// fields on top. A name is looked up from the top down, so an inner field
/// shadows an outer one of the same name — which is what a cloned
/// `HashMap` per struct used to do, at the cost of copying every value in it
/// once per struct and once per array element.
struct Frames<'a> {
    /// The enclosing scopes, outermost first.
    base: Vec<&'a HashMap<String, Value>>,
    /// The struct fields of each frame, outermost first, beside its base.
    own: Vec<&'a [(String, Value)]>,
    /// The enclosing type's argument, when there is one: read as `arg` by a
    /// struct type's own fields. It is computed rather than borrowed, because
    /// it comes from an expression over the enclosing scope.
    arg: Option<Value>,
    /// The scope a struct's own field conditions read, flattened once.
    ///
    /// A condition reads a count or a flag from the block *around* the struct —
    /// `arg` for a packed vertex, `num_vertices` for a geometry array — and
    /// that scope is the same for every element of an array. Resolving it once
    /// and sharing it is what keeps an array of structs linear in its length:
    /// resolving it per element copied the enclosing arrays once per element,
    /// which is quadratic.
    flat: OnceLock<HashMap<String, Value>>,
}

impl<'a> Frames<'a> {
    /// The value of `name`, innermost frame first.
    fn get(&self, name: &str) -> Option<&Value> {
        if name == "arg" {
            if let Some(v) = &self.arg {
                return Some(v);
            }
        }
        for own in self.own.iter().rev() {
            if let Some((_, v)) = own.iter().find(|(n, _)| n == name) {
                return Some(v);
            }
        }
        self.base.iter().rev().find_map(|s| s.get(name))
    }

    /// The frames with `st`'s own fields pushed on top.
    ///
    /// The child inherits the parent's resolved scope, because that scope is
    /// what the child's own field conditions read and it does not change from
    /// one array element to the next. A fresh cache per element would resolve
    /// it again for each, which is quadratic in the array's length.
    fn push(&self, st: &'a Struct) -> Frames<'a> {
        let mut out = Frames {
            base: self.base.clone(),
            own: self.own.clone(),
            arg: self.arg.clone(),
            flat: OnceLock::new(),
        };
        out.own.push(&st.fields);
        out
    }

    /// An expression's value as a number, or `None` when a name it reads is
    /// not in scope — which is how the decoder's own scope answers, and what
    /// the callers already treat as "not there".
    ///
    /// `scratch` is a cleared map the caller owns, filled here rather than
    /// allocated: this runs once per field of a struct, and a struct written
    /// once per array element runs it once per element. The packed vertices of
    /// a skin partition are 991 elements of a twelve-field struct whose every
    /// field is conditional on `arg`, so a full scope rebuild per condition is
    /// what turned one block's re-encode from milliseconds into ten seconds.
    ///
    /// The names an expression reads are resolved from the frames and the
    /// result is cached on the frames, so the second and later conditions of
    /// the same struct cost a lookup rather than a rebuild.
    fn number(&self, e: &crate::expr::Expr, scratch: &mut HashMap<String, Value>) -> Option<f64> {
        let flat = self.flat.get_or_init(|| self.flatten());
        scratch.clear();
        scratch.extend(flat.iter().map(|(k, v)| (k.clone(), v.clone())));
        e.number_in(scratch)
    }

    /// Whether an expression evaluates to truthy against the frames.
    fn truthy(&self, e: &crate::expr::Expr, scratch: &mut HashMap<String, Value>) -> bool {
        let flat = self.flat.get_or_init(|| self.flatten());
        scratch.clear();
        scratch.extend(flat.iter().map(|(k, v)| (k.clone(), v.clone())));
        e.evaluate_in(scratch)
    }

    /// Every name in scope, innermost frame first.
    ///
    /// A frame's own fields shadow the enclosing scope's, and `arg` shadows
    /// both, which is the order the decoder reads them in.
    ///
    /// Only a scalar is put in. A condition is an expression over a count or a
    /// flag — `arg`, `num_vertices`, `flags` — and an evaluator that hands back
    /// a value by clone would otherwise copy an entire array on every name it
    /// reads: a skin partition's 991 packed vertices, re-cloned once per
    /// condition per vertex, is what turned one block's re-encode from
    /// milliseconds into fifteen seconds. A *width* still reads its array from
    /// the frames directly, through [`Frames::get`], so nothing that needs the
    /// array is denied it.
    fn flatten(&self) -> HashMap<String, Value> {
        let mut out: HashMap<String, Value> = HashMap::new();
        for base in self.base.iter().rev() {
            for (n, v) in base.iter() {
                if let Some(s) = scalar(v) {
                    out.insert(n.clone(), s);
                }
            }
        }
        for frame in &self.own {
            for (n, v) in frame.iter() {
                if let Some(s) = scalar(v) {
                    out.insert(n.clone(), s);
                }
            }
        }
        if let Some(v) = &self.arg {
            out.insert("arg".into(), v.clone());
        }
        out
    }

    /// The frames a struct type's fields are read against, with the field's
    /// own argument added as the innermost frame.
    fn with_arg(
        &self,
        f: &crate::schema::Field,
        scratch: &mut HashMap<String, Value>,
    ) -> Frames<'_> {
        Frames {
            base: self.base.clone(),
            own: self.own.clone(),
            arg: f
                .arg
                .as_ref()
                .and_then(|e| self.number(e, scratch))
                .map(Value::Num),
            flat: OnceLock::new(),
        }
    }
}

/// The scalar a value is, or `None` when it is an array, a struct or bytes.
///
/// A condition reads a count or a flag and never the contents of an array, so
/// this is what a scope holds. See [`Frames::flatten`] for why it matters.
fn scalar(v: &Value) -> Option<Value> {
    match v {
        Value::List(_) | Value::Bytes(_) | Value::Struct(_) => None,
        other => Some(other.clone()),
    }
}

/// Write a block's fields back out.
///
/// `scope` holds the values of the fields read so far, which the lengths
/// and conditions of the later fields are read against.
pub fn encode_fields(
    schema: &Schema,
    header: &Header,
    lineage: &[String],
    fields: &[crate::schema::Field],
    st: &Struct,
    template: Option<&str>,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let mut scope: HashMap<String, Value> = crate::file::file_scope(header);
    for (name, v) in &st.fields {
        scope.insert(name.clone(), v.clone());
    }
    // One map, reused for every condition and length the encoder evaluates:
    // the alternative is one allocation per field, and the values it holds can
    // be a whole vertex array.
    let mut scratch: HashMap<String, Value> = HashMap::new();
    let frames = Frames {
        base: vec![&scope],
        own: Vec::new(),
        arg: None,
        flat: OnceLock::new(),
    };
    encode_fields_in(
        schema,
        header,
        lineage,
        fields,
        st,
        template,
        &frames,
        &mut scratch,
        out,
    )
}

/// Write a list of fields, against a scope.
///
/// The scope is the fields read so far, which the later fields' conditions
/// and lengths are read against.
#[allow(clippy::too_many_arguments)]
fn encode_fields_in(
    schema: &Schema,
    header: &Header,
    lineage: &[String],
    fields: &[crate::schema::Field],
    st: &Struct,
    template: Option<&str>,
    scope: &Frames<'_>,
    scratch: &mut HashMap<String, Value>,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    // The struct's own fields shadow the enclosing scope's, which is what
    // the decoder's scope does and what a clone per struct used to buy.
    let own = scope.push(st);
    // The values are matched to the fields in order, not by name: a schema
    // may name two fields the same and tell them apart by their
    // conditions — `HavokFilter`'s two `layer` fields, and `NiPSysData`'s
    // `num_vertices` and `bs_max_vertices` — and the value holds only the
    // one that was read.
    //
    // So the fields that are present are counted, and the values are
    // taken in the same order, which is the order the decoder read them
    // in.
    let present_fields: Vec<&crate::schema::Field> = fields
        .iter()
        .filter(|f| present(schema, header, lineage, f, &own, scratch).unwrap_or(false))
        .collect();
    for f in &present_fields {
        let Some(v) = st.get(&f.name) else {
            // A field the schema says is present, but which the value does
            // not hold: the value is incomplete, and there is nothing to
            // write.
            continue;
        };
        let f = *f;
        let at = out.len();
        write_field(schema, header, lineage, f, v, template, &own, scratch, out)?;
        if std::env::var("NVRE_NIF_TRACE").is_ok() {
            eprintln!("  {}+{}..{} = {:?}", f.name, at, out.len(), v);
        }
    }
    Ok(())
}

/// Whether a field is in the file.
///
/// A field is in the file when the version allows it, the block's type
/// allows it, and its condition holds.
fn present(
    _schema: &Schema,
    header: &Header,
    lineage: &[String],
    f: &crate::schema::Field,
    scope: &Frames<'_>,
    scratch: &mut HashMap<String, Value>,
) -> Result<bool, CodecError> {
    let version = header.version;
    if let Some(since) = f.since {
        if version < since {
            return Ok(false);
        }
    }
    if let Some(until) = f.until {
        if version > until {
            return Ok(false);
        }
    }
    // The block's own type is part of what a field's `only` and `exclude`
    // are read against, as it is for the decoder.
    let is = |t: &String| lineage.iter().any(|l| l == t);
    if !f.only.is_empty() && !f.only.iter().any(is) {
        return Ok(false);
    }
    if f.exclude.iter().any(is) {
        return Ok(false);
    }
    match &f.cond {
        None => Ok(true),
        Some(e) => Ok(scope.truthy(e, scratch)),
    }
}

/// Write one field's value.
#[allow(clippy::too_many_arguments)]
fn write_field(
    schema: &Schema,
    header: &Header,
    lineage: &[String],
    f: &crate::schema::Field,
    v: &Value,
    template: Option<&str>,
    scope: &Frames<'_>,
    scratch: &mut HashMap<String, Value>,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    // The length of an array, if it has one.
    let length = match &f.length {
        None => None,
        Some(crate::schema::Length::Fixed(n)) => Some(*n),
        Some(crate::schema::Length::Expr(e)) => {
            Some(scope.number(e, scratch).unwrap_or(0.0) as usize)
        }
        Some(crate::schema::Length::Field(name)) => {
            let Some(v) = scope.get(name) else {
                return Ok(());
            };
            Some(v.as_number().unwrap_or(0.0) as usize)
        }
    };

    // A named type: a struct, an enum, or a built-in struct type.
    if let TypeRef::Named(n) = &f.ty {
        // An enum is written as its storage type. A field with a length is
        // an array of them, so one is written per element.
        if let Some(e) = schema.enums.get(n) {
            if let Some(count) = length {
                let items = v.as_list().unwrap_or(&[]);
                for item in items.iter().take(count) {
                    write_prim(e.storage, item, header.version, out)?;
                }
                return Ok(());
            }
            write_prim(e.storage, v, header.version, out)?;
            return Ok(());
        }
        // A sized string: a length and that many bytes.
        if n == "sized_string" {
            let count = length.unwrap_or(1);
            for i in 0..count {
                let text = match v {
                    Value::List(items) => items.get(i).and_then(|x| x.as_str()).unwrap_or(""),
                    other => other.as_str().unwrap_or(""),
                };
                let bytes = nvre_util::cp1252::encode(text).unwrap_or_default();
                out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                out.extend_from_slice(&bytes);
            }
            return Ok(());
        }
        let fields = schema
            .struct_fields(n)
            .ok_or_else(|| CodecError::Layout(format!("the unknown struct type {n:?}")))?;
        // The template the struct's own fields are read against: the
        // field's own, or the enclosing one.
        let sub_template = sub_template(schema, f, template);
        // The argument the struct's own fields are read against.
        let sub_scope = scope.with_arg(f, scratch);
        // An array of structs: the struct is written once per element.
        if let Some(count) = length {
            let items = v.as_list().unwrap_or(&[]);
            for (i, item) in items.iter().take(count).enumerate() {
                let Value::Struct(sub) = item else {
                    return Err(CodecError::Layout(format!(
                        "the element {i} of {:?} is not a struct",
                        f.name
                    )));
                };
                encode_fields_in(
                    schema,
                    header,
                    lineage,
                    &fields,
                    sub,
                    sub_template.as_deref(),
                    &sub_scope,
                    scratch,
                    out,
                )?;
            }
            return Ok(());
        }
        let Value::Struct(sub) = v else {
            return Err(CodecError::Layout(format!(
                "the value of {:?} is not a struct",
                f.name
            )));
        };
        return encode_fields_in(
            schema,
            header,
            lineage,
            &fields,
            sub,
            sub_template.as_deref(),
            &sub_scope,
            scratch,
            out,
        );
    }

    // The enclosing type's argument: the type parameter `T`.
    if matches!(f.ty, TypeRef::Template) {
        let Some(t) = template else {
            return Err(CodecError::Layout(format!(
                "{:?} has no enclosing type to take its type from",
                f.name
            )));
        };
        if let Some(p) = Prim::parse(t) {
            return write_prim(p, v, header.version, out);
        }
        let fields = schema
            .struct_fields(t)
            .ok_or_else(|| CodecError::Layout(format!("the unknown struct type {t:?}")))?;
        let Value::Struct(sub) = v else {
            return Err(CodecError::Layout(format!(
                "the value of {:?} is not a struct",
                f.name
            )));
        };
        return encode_fields_in(
            schema,
            header,
            lineage,
            &fields,
            sub,
            Some(t),
            scope,
            scratch,
            out,
        );
    }

    let TypeRef::Prim(prim) = f.ty else {
        unreachable!()
    };

    // An array.
    if let Some(n) = length {
        // A byte array of a known length is raw bytes.
        if prim == Prim::Bytes {
            if let Value::Bytes(b) = v {
                out.extend_from_slice(b);
            }
            return Ok(());
        }
        // A two-dimensional array: each row is as wide as the width says.
        if let Some(w) = &f.width {
            let widths = widths(schema, w, n, scope, scratch);
            let Value::List(rows) = v else {
                return Err(CodecError::Layout(format!(
                    "the value of {:?} is not a list",
                    f.name
                )));
            };
            for (i, row) in rows.iter().take(n).enumerate() {
                let width = widths.get(i).copied().unwrap_or(0);
                let Value::List(cells) = row else {
                    return Err(CodecError::Layout(format!(
                        "the row {i} of {:?} is not a list",
                        f.name
                    )));
                };
                for cell in cells.iter().take(width) {
                    write_prim(prim, cell, header.version, out)?;
                }
            }
            return Ok(());
        }
        // A plain array: one element per count.
        let items = v.as_list().unwrap_or(&[]);
        for item in items.iter().take(n) {
            write_prim(prim, item, header.version, out)?;
        }
        return Ok(());
    }

    write_prim(prim, v, header.version, out)
}

/// The widths of a two-dimensional array's rows.
fn widths(
    _schema: &Schema,
    w: &crate::schema::Width,
    n: usize,
    scope: &Frames<'_>,
    scratch: &mut HashMap<String, Value>,
) -> Vec<usize> {
    match w {
        crate::schema::Width::Fixed(k) => vec![*k; n],
        crate::schema::Width::Expr(e) => vec![scope.number(e, scratch).unwrap_or(0.0) as usize; n],
        crate::schema::Width::Field(name) => {
            // The field the width comes from. It is usually an array whose
            // elements are the row widths — `NiTriStripsData.points` is as
            // wide as `strip_lengths` says, per row — and sometimes a single
            // count, which is then the width of every row.
            //
            // The array is read from the frames, which hold it by reference;
            // a scope carries only an array's length, because that is all a
            // condition or a length reads from one.
            let Some(v) = scope.get(name) else {
                return vec![0; n];
            };
            match v {
                Value::List(l) => l
                    .iter()
                    .map(|x| x.as_number().unwrap_or(0.0) as usize)
                    .collect(),
                // A single count: the same width for every row.
                other => vec![other.as_number().unwrap_or(0.0) as usize; n],
            }
        }
    }
}

/// The template a field's struct type is read against.
fn sub_template(
    schema: &Schema,
    f: &crate::schema::Field,
    template: Option<&str>,
) -> Option<String> {
    match f.template.as_deref() {
        Some("T") => template.map(str::to_string),
        Some(t) => Some(t.to_string()),
        None => {
            let named = match &f.ty {
                TypeRef::Named(n) => Some(n.as_str()),
                _ => None,
            };
            named
                .and_then(|n| schema.structs.get(n))
                .and_then(|s| s.template.clone())
                .or_else(|| template.map(str::to_string))
        }
    }
}

/// Write a primitive's value.
pub fn write_prim(
    prim: Prim,
    v: &Value,
    version: Version,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    use Prim::*;
    // The number a value holds, for the types that are numbers.
    let num = |v: &Value| -> Result<f64, CodecError> {
        v.as_number()
            .ok_or_else(|| CodecError::Layout(format!("the value {v:?} is not a number")))
    };
    match prim {
        U8 => out.push(num(v)? as u8),
        I8 => out.push(num(v)? as i8 as u8),
        U16 => out.extend_from_slice(&(num(v)? as u16).to_le_bytes()),
        I16 => out.extend_from_slice(&(num(v)? as i16).to_le_bytes()),
        U32 => out.extend_from_slice(&(num(v)? as u32).to_le_bytes()),
        I32 => out.extend_from_slice(&(num(v)? as i32).to_le_bytes()),
        U64 => match v {
            Value::Int64(n) => out.extend_from_slice(&n.to_le_bytes()),
            _ => out.extend_from_slice(&(num(v)? as u64).to_le_bytes()),
        },
        F32 => out.extend_from_slice(&(num(v)? as f32).to_le_bytes()),
        Bool => {
            // A flag is a byte, and a byte above one is not a flag: it is
            // the number it is, and it is written back as it was.
            match v {
                Value::Bool(b) => out.push(if *b { 1 } else { 0 }),
                other => out.push(other.as_number().unwrap_or(0.0) as u8),
            }
        }
        Ref | Ptr => {
            let r = v.as_ref().unwrap_or(-1);
            out.extend_from_slice(&r.to_le_bytes());
        }
        HFloat => out.extend_from_slice(&half_bits(v).to_le_bytes()),
        HalfVec2 => {
            for c in half_list(v, 2)? {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        HalfVec3 => {
            for c in half_list(v, 3)? {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        HalfVec4 => {
            for c in half_list(v, 4)? {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        TexCoord => write_floats(v, 2, out)?,
        Vec2 => write_floats(v, 2, out)?,
        Vec3 => write_floats(v, 3, out)?,
        Vec4 => write_floats(v, 4, out)?,
        Color3 => write_floats(v, 3, out)?,
        Color4 => write_floats(v, 4, out)?,
        Mat22 => write_floats(v, 4, out)?,
        Mat33 => write_floats(v, 9, out)?,
        Mat34 => write_floats(v, 12, out)?,
        Mat44 => write_floats(v, 16, out)?,
        Quat => {
            // A quaternion is stored w, x, y, z.
            let c = floats(v, 4)?;
            out.extend_from_slice(&c[0].to_le_bytes());
            out.extend_from_slice(&c[1].to_le_bytes());
            out.extend_from_slice(&c[2].to_le_bytes());
            out.extend_from_slice(&c[3].to_le_bytes());
        }
        ByteColor4 => {
            for c in bytes(v, 4)? {
                out.push(c);
            }
        }
        ByteVec3 => {
            // Three bytes, each a value in the range -1 to 1: a vertex's
            // normal or tangent, as the format stores them.
            for c in floats(v, 3)? {
                let b = ((c + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
                out.push(b);
            }
        }
        U16Vec3 => {
            for c in ints(v, 3)? {
                out.extend_from_slice(&(c as u16).to_le_bytes());
            }
        }
        NormByte => {
            // The byte is unsigned, and the scale is 127.5 so that the whole
            // range of the byte maps onto the whole range of -1 to 1. That
            // is what makes the round trip exact: a value written back from
            // `b / 127.5 - 1` is the byte it came from.
            let f = num(v)? as f32;
            let b = ((f + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
            out.push(b);
        }
        Triangle => {
            // Three 16-bit vertex indices.
            for c in ints(v, 3)? {
                out.extend_from_slice(&(c as u16).to_le_bytes());
            }
        }
        String => {
            if version >= V20_1_0_3 {
                // A string is an index into the file's string table.
                out.extend_from_slice(&(num(v)? as u32).to_le_bytes());
            } else {
                let text = v.as_str().unwrap_or("");
                out.extend_from_slice(
                    nvre_util::cp1252::encode(text)
                        .unwrap_or_default()
                        .as_slice(),
                );
                out.push(0);
            }
        }
        SizedString => {
            let text = v.as_str().unwrap_or("");
            let bytes = nvre_util::cp1252::encode(text).unwrap_or_default();
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
        }
        SizedString16 => {
            let text = v.as_str().unwrap_or("");
            let bytes = nvre_util::cp1252::encode(text).unwrap_or_default();
            out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&bytes);
        }
        Bytes => {
            if let Value::Bytes(b) = v {
                out.extend_from_slice(b);
            }
        }
        Align => {
            if let Value::Bytes(b) = v {
                out.extend_from_slice(b);
            }
        }
    }
    Ok(())
}

/// The floats a value holds.
fn floats(v: &Value, n: usize) -> Result<Vec<f32>, CodecError> {
    let mut out = Vec::with_capacity(n);
    if let Some(l) = v.as_list() {
        for x in l.iter().take(n) {
            out.push(x.as_number().unwrap_or(0.0) as f32);
        }
    }
    while out.len() < n {
        out.push(0.0);
    }
    Ok(out)
}

/// The half float a value is.
///
/// A NaN is written from the payload the reader kept, rather than from a
/// narrowing: a narrowing of a NaN loses the payload, and a NIF's vertex
/// data holds NaNs whose payload is the file's own.
fn half_bits(v: &Value) -> u16 {
    let Some(n) = v.as_number() else {
        return 0;
    };
    if n.is_nan() {
        return crate::codec::half_from_nan(n);
    }
    f32_to_half(n as f32)
}

/// The half floats a list holds.
fn half_list(v: &Value, n: usize) -> Result<Vec<u16>, CodecError> {
    let mut out = Vec::with_capacity(n);
    if let Some(l) = v.as_list() {
        for x in l.iter().take(n) {
            out.push(half_bits(x));
        }
    }
    while out.len() < n {
        out.push(0);
    }
    Ok(out)
}

/// The integers a value holds.
fn ints(v: &Value, n: usize) -> Result<Vec<i64>, CodecError> {
    let mut out = Vec::with_capacity(n);
    if let Some(l) = v.as_list() {
        for x in l.iter().take(n) {
            out.push(x.as_int().unwrap_or(0));
        }
    }
    while out.len() < n {
        out.push(0);
    }
    Ok(out)
}

/// The bytes a value holds.
fn bytes(v: &Value, n: usize) -> Result<Vec<u8>, CodecError> {
    if let Value::Bytes(b) = v {
        return Ok(b.iter().copied().take(n).collect());
    }
    let mut out = Vec::with_capacity(n);
    if let Some(l) = v.as_list() {
        for x in l.iter().take(n) {
            out.push(x.as_int().unwrap_or(0) as u8);
        }
    }
    while out.len() < n {
        out.push(0);
    }
    Ok(out)
}

/// Write a vector of floats.
fn write_floats(v: &Value, n: usize, out: &mut Vec<u8>) -> Result<(), CodecError> {
    for c in floats(v, n)? {
        out.extend_from_slice(&c.to_le_bytes());
    }
    Ok(())
}
