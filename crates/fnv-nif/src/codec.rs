//! Schema interpreter: decode block bytes into a [`Struct`] and encode it back.
//!
//! Encoding exists to prove the decoder: a block that decodes, consumes exactly its
//! stored size and re-encodes to the same bytes is understood field by field.

use crate::expr::Scope;
use crate::header::{RawString, V20_1_0_3};
use crate::schema::{Field, Prim, Schema, TypeRef};
use crate::value::{FieldScope, Struct, Value};
use fnv_util::{cp1252, Cursor};

/// Error while decoding or encoding, with the field path and (for decoding) the offset
/// within the block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{path}: {msg}{}", .offset.map(|o| format!(" (block offset {o})")).unwrap_or_default())]
pub struct CodecError {
    pub path: String,
    pub msg: String,
    pub offset: Option<usize>,
}

/// Arrays longer than this are rejected as corrupt counts.
const MAX_ELEMENTS: i64 = 1 << 26;

/// Everything about the file that decoding depends on.
#[derive(Clone, Copy)]
pub struct FileCtx<'a> {
    pub version: u32,
    pub user_version: u32,
    pub bs_version: u32,
    pub strings: Option<&'a [RawString]>,
}

pub struct Codec<'a> {
    pub schema: &'a Schema,
    pub file: FileCtx<'a>,
    /// Concrete block type and its ancestors (for `only` / `exclude`).
    pub lineage: &'a [String],
}

impl<'a> Codec<'a> {
    fn scope<'s>(&self, fields: &'s [(std::sync::Arc<str>, Value)], arg: i64) -> FieldScope<'s> {
        FieldScope {
            fields,
            version: self.file.version,
            user_version: self.file.user_version,
            bs_version: self.file.bs_version,
            arg,
        }
    }

    fn present(&self, f: &Field, scope: &dyn Scope, path: &str) -> Result<bool, CodecError> {
        let v = self.file.version;
        if f.since.is_some_and(|s| v < s) || f.until.is_some_and(|u| v > u) {
            return Ok(false);
        }
        if !f.only.is_empty() && !f.only.iter().any(|t| self.lineage.contains(t)) {
            return Ok(false);
        }
        if f.exclude.iter().any(|t| self.lineage.contains(t)) {
            return Ok(false);
        }
        match &f.cond {
            None => Ok(true),
            Some(e) => e
                .eval(scope)
                .map(|x| x != 0)
                .map_err(|e| err(path, format!("cond: {e}"), None)),
        }
    }

    fn resolve<'t>(&self, ty: &'t TypeRef, template: Option<&'t TypeRef>, path: &str) -> Result<&'t TypeRef, CodecError> {
        match ty {
            TypeRef::Template => template.ok_or_else(|| err(path, "T used without a template".into(), None)),
            t => Ok(t),
        }
    }

    // ---------------------------------------------------------------- decoding

    pub fn decode_fields(
        &self,
        fields: &[Field],
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Struct, CodecError> {
        let mut st = Struct::default();
        self.decode_fields_into(fields, c, arg, template, path, &mut st)?;
        Ok(st)
    }

    /// Decode into a struct the caller owns, so a failure can leave the
    /// fields that *were* read in place.
    ///
    /// A schema author needs to see where a layout went wrong, and the
    /// fields read before the failure are what say so. The ordinary
    /// [`Codec::decode_fields`] throws them away.
    pub fn decode_fields_into(
        &self,
        fields: &[Field],
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        st: &mut Struct,
    ) -> Result<(), CodecError> {
        for f in fields {
            let fpath = format!("{path}.{}", f.name);
            let scope = self.scope(&st.fields, arg);
            if !self.present(f, &scope, &fpath)? {
                continue;
            }
            let ty = self.resolve(&f.ty, template, &fpath)?;
            let sub_template = match &f.template {
                Some(TypeRef::Template) => template,
                Some(t) => Some(t),
                None => None,
            };
            let sub_arg = match &f.arg {
                Some(e) => e.eval(&scope).map_err(|e| err(&fpath, format!("arg: {e}"), Some(c.pos)))?,
                None => 0,
            };
            let v = match &f.length {
                None => self.read_one(ty, c, sub_arg, sub_template, &fpath)?,
                Some(len) => {
                    let n = len.eval(&scope).map_err(|e| err(&fpath, format!("length: {e}"), Some(c.pos)))?;
                    if ty == &TypeRef::Prim(Prim::Align) {
                        // The alignment is the field's length. The padding is
                        // however many bytes reach the next multiple of it.
                        // The alignment is not clamped to what is left: a
                        // three-byte tail still aligns to four, and clamping
                        // would change the modulus and read nothing.
                        let align = n.max(1) as usize;
                        let want = (c.pos + align - 1) / align * align - c.pos;
                        let want = want.min(c.remaining());
                        let b = c.bytes(want).map_err(|e| err(&fpath, e.to_string(), Some(c.pos)))?;
                        Value::Bytes(b.to_vec())
                    } else {
                    match &f.width {
                        None => self.read_array(ty, n, c, sub_arg, sub_template, &fpath)?,
                        Some(w) => {
                            let widths = self.widths(w, n, &scope, &fpath, c.pos)?;
                            let mut rows = Vec::with_capacity(widths.len());
                            for (i, w) in widths.into_iter().enumerate() {
                                rows.push(self.read_array(ty, w, c, sub_arg, sub_template, &format!("{fpath}[{i}]"))?);
                            }
                            Value::List(rows)
                        }
                    }
                    }
                }
            };
            st.fields.push((f.name.clone(), v));
        }
        Ok(())
    }

    /// Row widths of a 2D array: one per element of a named integer array, or the same
    /// scalar for every row.
    fn widths(
        &self,
        w: &crate::expr::Expr,
        rows: i64,
        scope: &FieldScope<'_>,
        path: &str,
        pos: usize,
    ) -> Result<Vec<i64>, CodecError> {
        check_count(rows, path, pos)?;
        if let Some(name) = w.as_ident() {
            if let Some((_, Value::IntList(v))) = scope.fields.iter().rev().find(|(n, _)| &**n == name) {
                if v.len() as i64 != rows {
                    return Err(err(path, format!("{} row widths for {rows} rows", v.len()), Some(pos)));
                }
                return Ok(v.clone());
            }
        }
        let n = w.eval(scope).map_err(|e| err(path, format!("width: {e}"), Some(pos)))?;
        Ok(vec![n; rows as usize])
    }

    fn read_array(
        &self,
        ty: &TypeRef,
        n: i64,
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Value, CodecError> {
        check_count(n, path, c.pos)?;
        let n = n as usize;
        let min = match ty {
            TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. } => p.min_size(),
            _ => 0,
        };
        if min > 0 && n.saturating_mul(min) > c.remaining() {
            return Err(err(path, format!("{n} elements of at least {min} bytes exceed the {} bytes left", c.remaining()), Some(c.pos)));
        }
        let at = c.pos;
        let oob = |e: fnv_util::OutOfBounds| err(path, e.to_string(), Some(at));
        match ty {
            TypeRef::Prim(Prim::Bytes) => Ok(Value::Bytes(c.bytes(n).map_err(oob)?.to_vec())),
            TypeRef::Prim(p) if p.floats().is_some() => {
                let dim = p.floats().unwrap();
                let mut data = Vec::with_capacity(n * dim);
                for _ in 0..n * dim {
                    data.push(c.f32().map_err(oob)?);
                }
                Ok(Value::FloatList { dim: dim as u8, data })
            }
            TypeRef::Prim(Prim::Ref | Prim::Ptr) => {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(c.i32().map_err(oob)?);
                }
                Ok(Value::RefList(v))
            }
            TypeRef::Prim(p @ (Prim::Triangle | Prim::ByteColor4)) => {
                let mut v = Vec::with_capacity(n * 4);
                for _ in 0..n {
                    push_ints(*p, c, &mut v).map_err(oob)?;
                }
                Ok(Value::IntList(v))
            }
            TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. } if p.int_size().is_some() => {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(read_int(*p, c).map_err(oob)?);
                }
                Ok(Value::IntList(v))
            }
            _ => {
                let mut v = Vec::with_capacity(n.min(4096));
                for i in 0..n {
                    v.push(self.read_one(ty, c, arg, template, &format!("{path}[{i}]"))?);
                }
                Ok(Value::List(v))
            }
        }
    }

    fn read_one(
        &self,
        ty: &TypeRef,
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Value, CodecError> {
        let at = c.pos;
        let oob = |e: fnv_util::OutOfBounds| err(path, e.to_string(), Some(at));
        Ok(match ty {
            TypeRef::Struct(name) => {
                let def = self.schema.structs.get(name).ok_or_else(|| err(path, format!("unknown struct {name}"), None))?;
                Value::Struct(self.decode_fields(&def.fields, c, arg, template, path)?)
            }
            TypeRef::Template => return Err(err(path, "unresolved template".into(), None)),
            TypeRef::Enum { storage, .. } => Value::Int(read_int(*storage, c).map_err(oob)?),
            TypeRef::Prim(p) => match p {
                Prim::Bool => {
                    let b = c.u8().map_err(oob)?;
                    if b <= 1 {
                        Value::Bool(b == 1)
                    } else {
                        Value::Int(b as i64)
                    }
                }
                Prim::Ref => Value::Ref(c.i32().map_err(oob)?),
                Prim::Ptr => Value::Ptr(c.i32().map_err(oob)?),
                Prim::F32 => Value::Float(c.f32().map_err(oob)?),
                Prim::HFloat => Value::Float(half_to_f32(c.u16().map_err(oob)?)),
                Prim::NormByte => Value::Float((c.u8().map_err(oob)? as f32) / 127.0 - 1.0),
                Prim::ByteVec3 => {
                    let b = c.bytes(3).map_err(oob)?;
                    Value::IntList(b.iter().map(|x| *x as i64).collect())
                }
                Prim::HalfVec3 => {
                    let mut v = Vec::with_capacity(3);
                    for _ in 0..3 {
                        v.push(half_to_f32(c.u16().map_err(oob)?));
                    }
                    Value::Floats(v)
                }
                Prim::HalfVec2 => {
                    let mut v = Vec::with_capacity(2);
                    for _ in 0..2 {
                        v.push(half_to_f32(c.u16().map_err(oob)?));
                    }
                    Value::Floats(v)
                }
                Prim::String if self.file.version >= V20_1_0_3 => {
                    let idx = c.u32().map_err(oob)?;
                    let text = self.file.strings.and_then(|s| s.get(idx as usize)).map(RawString::text);
                    Value::Str { index: Some(idx), text }
                }
                Prim::String | Prim::SizedString => {
                    let n = c.u32().map_err(oob)? as usize;
                    let b = c.bytes(n).map_err(oob)?;
                    Value::Str { index: None, text: Some(cp1252::decode(b)) }
                }
                Prim::Bytes => return Err(err(path, "bytes without length".into(), None)),
                Prim::Triangle | Prim::ByteColor4 => {
                    let mut v = Vec::with_capacity(4);
                    push_ints(*p, c, &mut v).map_err(oob)?;
                    Value::IntList(v)
                }
                p if p.floats().is_some() => {
                    let mut v = Vec::with_capacity(p.floats().unwrap());
                    for _ in 0..p.floats().unwrap() {
                        v.push(c.f32().map_err(oob)?);
                    }
                    Value::Floats(v)
                }
                p => Value::Int(read_int(*p, c).map_err(oob)?),
            },
        })
    }

    // ---------------------------------------------------------------- encoding

    pub fn encode_fields(
        &self,
        fields: &[Field],
        st: &Struct,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        let mut idx = 0;
        for f in fields {
            let fpath = format!("{path}.{}", f.name);
            let scope = self.scope(&st.fields[..idx], arg);
            if !self.present(f, &scope, &fpath)? {
                continue;
            }
            let Some((name, v)) = st.fields.get(idx) else {
                return Err(err(&fpath, "missing from the value".into(), None));
            };
            if **name != *f.name {
                return Err(err(&fpath, format!("value has {name} here"), None));
            }
            let ty = self.resolve(&f.ty, template, &fpath)?;
            let sub_template = match &f.template {
                Some(TypeRef::Template) => template,
                Some(t) => Some(t),
                None => None,
            };
            let sub_arg = match &f.arg {
                Some(e) => e.eval(&scope).map_err(|e| err(&fpath, format!("arg: {e}"), None))?,
                None => 0,
            };
            match (&f.length, &f.width) {
                (None, _) => self.write_one(ty, v, sub_arg, sub_template, &fpath, out)?,
                (Some(len), None) => {
                    if ty == &TypeRef::Prim(Prim::Align) {
                        // The padding is whatever was read; write it back, and
                        // pad to the alignment if the value is short.
                        let align = len.eval(&scope).unwrap_or(4).max(1) as usize;
                        let Value::Bytes(b) = v else {
                            return Err(err(&fpath, "align field is not bytes".into(), None));
                        };
                        let want = (out.len() + align - 1) / align * align - out.len();
                        let mut b = b.clone();
                        b.resize(want.max(b.len()), 0);
                        out.extend_from_slice(&b);
                    } else {
                        self.write_array(ty, v, sub_arg, sub_template, &fpath, out)?;
                    }
                }
                (Some(_), Some(_)) => {
                    let Value::List(rows) = v else { return Err(err(&fpath, "2D array is not a list".into(), None)) };
                    for (i, r) in rows.iter().enumerate() {
                        self.write_array(ty, r, sub_arg, sub_template, &format!("{fpath}[{i}]"), out)?;
                    }
                }
            }
            idx += 1;
        }
        if idx != st.fields.len() {
            return Err(err(path, format!("{} extra fields in the value", st.fields.len() - idx), None));
        }
        Ok(())
    }

    fn write_array(
        &self,
        ty: &TypeRef,
        v: &Value,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        match (ty, v) {
            (TypeRef::Prim(Prim::Bytes), Value::Bytes(b)) => out.extend_from_slice(b),
            (TypeRef::Prim(p), Value::FloatList { data, .. }) if p.floats().is_some() => {
                for f in data {
                    out.extend_from_slice(&f.to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::Ref | Prim::Ptr), Value::RefList(r)) => {
                for x in r {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::Triangle), Value::IntList(xs)) => {
                for x in xs {
                    out.extend_from_slice(&(*x as u16).to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::ByteColor4), Value::IntList(xs)) => {
                for x in xs {
                    out.push(*x as u8);
                }
            }
            (TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. }, Value::IntList(xs)) if p.int_size().is_some() => {
                for x in xs {
                    write_int(*p, *x, out);
                }
            }
            (_, Value::List(items)) => {
                for (i, it) in items.iter().enumerate() {
                    self.write_one(ty, it, arg, template, &format!("{path}[{i}]"), out)?;
                }
            }
            _ => return Err(err(path, format!("array value {v:?} does not fit {ty:?}"), None)),
        }
        Ok(())
    }

    fn write_one(
        &self,
        ty: &TypeRef,
        v: &Value,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        let mismatch = || err(path, format!("value {v:?} does not fit {ty:?}"), None);
        match ty {
            TypeRef::Struct(name) => {
                let def = self.schema.structs.get(name).ok_or_else(|| err(path, format!("unknown struct {name}"), None))?;
                let Value::Struct(s) = v else { return Err(mismatch()) };
                self.encode_fields(&def.fields, s, arg, template, path, out)?;
            }
            TypeRef::Template => return Err(err(path, "unresolved template".into(), None)),
            TypeRef::Enum { storage, .. } => write_int(*storage, v.as_int().ok_or_else(mismatch)?, out),
            TypeRef::Prim(p) => match (p, v) {
                (Prim::F32, Value::Float(f)) => out.extend_from_slice(&f.to_le_bytes()),
                (Prim::HFloat, Value::Float(f)) => {
                    out.extend_from_slice(&f32_to_half(*f).to_le_bytes())
                }
                (Prim::NormByte, Value::Float(f)) => {
                    let v = ((*f + 1.0) * 127.0).round().clamp(0.0, 255.0) as u8;
                    out.push(v);
                }
                (Prim::ByteVec3, Value::IntList(xs)) => {
                    for x in xs.iter().take(3) {
                        out.push(*x as u8);
                    }
                }
                (Prim::HalfVec3, Value::Floats(xs)) => {
                    for x in xs.iter().take(3) {
                        out.extend_from_slice(&f32_to_half(*x).to_le_bytes());
                    }
                }
                (Prim::HalfVec2, Value::Floats(xs)) => {
                    for x in xs.iter().take(2) {
                        out.extend_from_slice(&f32_to_half(*x).to_le_bytes());
                    }
                }
                (Prim::Ref | Prim::Ptr, Value::Ref(r) | Value::Ptr(r)) => out.extend_from_slice(&r.to_le_bytes()),
                (Prim::String, Value::Str { index: Some(i), .. }) if self.file.version >= V20_1_0_3 => {
                    out.extend_from_slice(&i.to_le_bytes())
                }
                (Prim::String | Prim::SizedString, Value::Str { index: None, text: Some(t) }) => {
                    let b = cp1252::encode(t).ok_or_else(|| err(path, "text not encodable".into(), None))?;
                    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                    out.extend_from_slice(&b);
                }
                (Prim::Triangle, Value::IntList(xs)) => {
                    for x in xs {
                        out.extend_from_slice(&(*x as u16).to_le_bytes());
                    }
                }
                (Prim::ByteColor4, Value::IntList(xs)) => {
                    for x in xs {
                        out.push(*x as u8);
                    }
                }
                (p, Value::Floats(fs)) if p.floats().is_some() => {
                    for f in fs {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                }
                (p, v) if p.int_size().is_some() => write_int(*p, v.as_int().ok_or_else(mismatch)?, out),
                _ => return Err(mismatch()),
            },
        }
        Ok(())
    }
}

/// A 16-bit float, as the IEEE half format.
fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits = match exp {
        0 => {
            if mant == 0 {
                sign << 31
            } else {
                // A subnormal half: its value is `mant * 2^-24`, and the
                // leading one of the mantissa becomes the f32's implicit bit.
                // Normalising shifts that bit up; `shift` counts how far.
                let mut m = mant;
                let mut shift = 0u32;
                while m & 0x400 == 0 {
                    m <<= 1;
                    shift += 1;
                }
                m &= 0x3ff;
                // The leading bit was at 10 - shift, so the value is
                // 1.f * 2^(10 - shift - 24) and the f32's exponent field is
                // that plus 127.
                let exp32 = 10i32 - shift as i32 - 24 + 127;
                (sign << 31) | ((exp32 as u32) << 23) | (m << 13)
            }
        }
        0x1f => {
            // A NaN: keep the payload so a half that held one comes back as
            // the same half.
            (sign << 31) | (0xff << 23) | (mant << 13)
        }
        _ => (sign << 31) | ((exp + 127 - 15) << 23) | (mant << 13),
    };
    f32::from_bits(bits)
}

/// A 32-bit float, as the IEEE half format.
fn f32_to_half(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;
    if exp == 0xff {
        if mant == 0 {
            return sign | 0x7c00;
        }
        // A NaN: the half's payload is the f32's top ten mantissa bits, so a
        // NaN that came from a half comes back as the same half.
        return sign | 0x7c00 | ((mant >> 13) as u16);
    }
    // Round to nearest, ties to even, so a value that came from a half comes
    // back as the same half.
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        // A subnormal half. The f32's value is `M * 2^(exp - 150)`, and the
        // half's mantissa is that value times 2^24, so the shift is
        // `126 - exp`.
        let m = mant | 0x80_0000;
        let shift = 126 - exp;
        if shift > 31 {
            return sign;
        }
        let mut half = (m >> shift) as u16;
        let rest = m & ((1u32 << shift) - 1);
        let halfway = 1u32 << (shift - 1);
        if rest > halfway || (rest == halfway && half & 1 == 1) {
            half += 1;
        }
        return sign | half;
    }
    let mut half = sign | ((e as u16) << 10) | ((mant >> 13) as u16);
    // Round to nearest, ties to even, on the bits that are dropped.
    let rest = mant & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && half & 1 == 1) {
        half += 1;
    }
    half
}

fn err(path: &str, msg: String, offset: Option<usize>) -> CodecError {
    CodecError { path: path.to_string(), msg, offset }
}

fn check_count(n: i64, path: &str, pos: usize) -> Result<(), CodecError> {
    if !(0..=MAX_ELEMENTS).contains(&n) {
        return Err(err(path, format!("implausible element count {n}"), Some(pos)));
    }
    Ok(())
}

fn read_int(p: Prim, c: &mut Cursor<'_>) -> Result<i64, fnv_util::OutOfBounds> {
    Ok(match p {
        Prim::U8 | Prim::Bool => c.u8()? as i64,
        Prim::I8 => c.i8()? as i64,
        Prim::U16 => c.u16()? as i64,
        Prim::I16 => c.i16()? as i64,
        Prim::U32 => c.u32()? as i64,
        Prim::I32 | Prim::Ref | Prim::Ptr => c.i32()? as i64,
        Prim::U64 => c.u64()? as i64,
        _ => unreachable!("not an integer primitive"),
    })
}

fn push_ints(p: Prim, c: &mut Cursor<'_>, v: &mut Vec<i64>) -> Result<(), fnv_util::OutOfBounds> {
    match p {
        Prim::Triangle => {
            for _ in 0..3 {
                v.push(c.u16()? as i64);
            }
        }
        Prim::ByteColor4 => {
            for _ in 0..4 {
                v.push(c.u8()? as i64);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn write_int(p: Prim, x: i64, out: &mut Vec<u8>) {
    match p {
        Prim::U8 | Prim::I8 | Prim::Bool => out.push(x as u8),
        Prim::U16 | Prim::I16 => out.extend_from_slice(&(x as u16).to_le_bytes()),
        Prim::U32 | Prim::I32 | Prim::Ref | Prim::Ptr => out.extend_from_slice(&(x as u32).to_le_bytes()),
        Prim::U64 => out.extend_from_slice(&(x as u64).to_le_bytes()),
        _ => unreachable!("not an integer primitive"),
    }
}

#[cfg(test)]
mod tests {
    use super::{f32_to_half, half_to_f32};

    /// Every half float must survive a round trip through an f32, or a block
    /// that holds one re-encodes to different bytes.
    #[test]
    fn every_half_float_round_trips() {
        for h in 0..=0xffffu16 {
            let back = f32_to_half(half_to_f32(h));
            assert_eq!(back, h, "half {h:#06x}");
        }
    }
}
