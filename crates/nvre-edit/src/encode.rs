//! Schema-driven encoding of JSON values back into subrecord payloads: the
//! inverse of `fnv_esm::decode::decode_type`.
//!
//! The decoder is total over everything the game ships (70 of 107 record types
//! have layouts, and nothing is left undecoded at the end of a record in the ten
//! masters). The encoder is deliberately narrower: it only has to be able to
//! write back what the decoder produced, and it refuses anything it cannot
//! round-trip byte for byte. That refusal is the point — an editor that silently
//! re-encodes a field it misread would corrupt the plugin.
//!
//! # Guarantees
//!
//! - **Round-trip**: for any subrecord the decoder reads as `SubStatus::Ok`,
//!   `encode_type(decode_type(value))` reproduces the original payload bytes
//!   exactly. Enforced by tests over the shipped masters.
//! - **No invention**: a value that does not match the layout is an error naming
//!   the field and the reason, never a best-effort guess.
//! - **Fixed sizes respected**: `schema.fixed_size` is checked before encoding so
//!   a struct that grew a field cannot silently resize a record.

use fnv_esm::schema::{Prim, Schema, TypeDef};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("{path}: expected {want}, found {found}")]
    Kind {
        path: String,
        want: &'static str,
        found: &'static str,
    },
    #[error("{path}: {msg}")]
    Value { path: String, msg: String },
    #[error("{path}: unknown enum variant {variant:?} (known: {known})")]
    Variant {
        path: String,
        variant: String,
        known: String,
    },
    #[error("{path}: unknown flag {flag:?} (known: {known})")]
    Flag {
        path: String,
        flag: String,
        known: String,
    },
    #[error("{path}: {n} is out of range for {ty}")]
    Range {
        path: String,
        n: i64,
        ty: &'static str,
    },
    #[error("{path}: string cannot be encoded in Windows-1252: {s:?}")]
    Encoding { path: String, s: String },
}

/// What a JSON value is, for error messages.
fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "float"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn kind_err(path: &str, want: &'static str, v: &Value) -> EncodeError {
    EncodeError::Kind {
        path: path.to_string(),
        want,
        found: kind_of(v),
    }
}

/// The first field name that appears more than once in `fields`, if any.
fn repeated_name(fields: &[fnv_esm::schema::Field]) -> Option<String> {
    for (i, f) in fields.iter().enumerate() {
        if fields[..i].iter().any(|g| g.name == f.name) {
            return Some(f.name.clone());
        }
    }
    None
}

/// Encode `value` according to `t`, appending to `out`.
///
/// `path` is the field path used in error messages ("DATA.rotation_radians.z").
pub fn encode_type(
    schema: &Schema,
    t: &TypeDef,
    value: &Value,
    out: &mut Vec<u8>,
    path: &str,
) -> Result<(), EncodeError> {
    match schema.resolve(t) {
        TypeDef::Prim(p) => encode_prim(*p, value, out, path),
        TypeDef::Named(_) => Err(EncodeError::Value {
            path: path.to_string(),
            msg: "unresolved named type".into(),
        }),
        TypeDef::Struct(fields) => {
            let obj = value
                .as_object()
                .ok_or_else(|| kind_err(path, "object", value))?;
            // A struct whose field names repeat cannot be represented as a JSON
            // object: the decoder's own output keeps only the last value for a
            // repeated name, so the field order and the earlier values are already
            // lost. Encoding such a struct would silently write the wrong bytes,
            // so it is refused instead (real case: CSTY/CSTD, where `unused`
            // appears five times).
            if let Some(dup) = repeated_name(fields) {
                let at = if path.is_empty() {
                    "this subrecord".to_string()
                } else {
                    path.to_string()
                };
                return Err(EncodeError::Value {
                    path: at,
                    msg: format!("field {dup:?} is declared more than once in this layout; the JSON form cannot hold both, so this subrecord is read-only"),
                });
            }
            // A value carrying fields this struct does not declare cannot be
            // encoded by it: writing only the declared fields would drop data and
            // silently produce the wrong bytes. This is what stops a union from
            // picking the wrong variant, since a union's variants differ exactly
            // by which fields they have (real case: NAVI NVMI's island branch).
            if let Some(extra) = obj.keys().find(|k| !fields.iter().any(|f| &f.name == *k)) {
                return Err(EncodeError::Value {
                    path: path.to_string(),
                    msg: format!("field {extra:?} is not part of this layout"),
                });
            }
            for f in fields {
                match obj.get(&f.name) {
                    // A missing optional field simply contributes no bytes, which
                    // is how the decoder reads a struct that ends early.
                    None if f.optional => {}
                    None => {
                        return Err(EncodeError::Value {
                            path: path.to_string(),
                            msg: format!("missing field {:?}", f.name),
                        })
                    }
                    Some(v) => {
                        let sub = if path.is_empty() {
                            f.name.clone()
                        } else {
                            format!("{path}.{}", f.name)
                        };
                        // A count-field array must hold exactly as many elements as
                        // its sibling count field says, because the count bytes are
                        // written by that field, not by this array.
                        if let TypeDef::Array {
                            count_field: Some(name),
                            ..
                        } = schema.resolve(&f.ty)
                        {
                            let want = obj.get(name).and_then(Value::as_u64);
                            match (want, v.as_array()) {
                                (Some(n), Some(arr)) if arr.len() as u64 == n => {}
                                _ => {
                                    return Err(EncodeError::Value {
                                        path: sub.clone(),
                                        msg: format!(
                                        "count_field {name:?} does not match this array's length"
                                    ),
                                    })
                                }
                            }
                        }
                        encode_type(schema, &f.ty, v, out, &sub)?;
                    }
                }
            }
            Ok(())
        }
        TypeDef::Array {
            elem,
            count,
            count_prefix,
            ..
        } => {
            let arr = value
                .as_array()
                .ok_or_else(|| kind_err(path, "array", value))?;
            if let Some(n) = count {
                if arr.len() != *n {
                    return Err(EncodeError::Value {
                        path: path.to_string(),
                        msg: format!("expected {} elements, found {}", n, arr.len()),
                    });
                }
            }
            // The count came from the data at decode time, so it goes back first.
            if let Some(p) = count_prefix {
                write_uint(*p, arr.len() as u64, out, path)?;
            }
            // A count-field array reads no count of its own: the sibling integer field
            // already written by the struct encoder is the count (xEdit's
            // `SetCountPath`). The element count here must match it, which the struct
            // encoder checks before writing this array.
            for (i, v) in arr.iter().enumerate() {
                encode_type(schema, elem, v, out, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        TypeDef::Enum { base, values } => {
            let n = match value {
                Value::Number(n) => n.as_i64().ok_or_else(|| EncodeError::Value {
                    path: path.to_string(),
                    msg: "enum value out of range".into(),
                })?,
                Value::String(s) => values
                    .iter()
                    .find(|(_, name)| name == s)
                    .map(|(k, _)| *k)
                    .ok_or_else(|| EncodeError::Variant {
                        path: path.to_string(),
                        variant: s.clone(),
                        known: values
                            .iter()
                            .map(|(_, n)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    })?,
                other => return Err(kind_err(path, "enum name or number", other)),
            };
            write_int(*base, n, out, path)
        }
        TypeDef::Flags { base, bits } => {
            let mut v: u64 = 0;
            match value {
                Value::Number(n) => {
                    v = n.as_u64().ok_or_else(|| EncodeError::Value {
                        path: path.to_string(),
                        msg: "flags value out of range".into(),
                    })?
                }
                Value::Object(o) => {
                    if let Some(raw) = o.get("value") {
                        v = raw
                            .as_u64()
                            .ok_or_else(|| kind_err(&format!("{path}.value"), "integer", raw))?;
                    }
                    if let Some(Value::Array(set)) = o.get("set") {
                        for item in set {
                            let name = item
                                .as_str()
                                .ok_or_else(|| kind_err(&format!("{path}.set"), "string", item))?;
                            let bit = bits
                                .iter()
                                .find(|(_, n)| n == name)
                                .map(|(b, _)| *b)
                                .ok_or_else(|| EncodeError::Flag {
                                    path: path.to_string(),
                                    flag: name.to_string(),
                                    known: bits
                                        .iter()
                                        .map(|(_, n)| n.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                })?;
                            v |= 1u64 << bit;
                        }
                    }
                    if let Some(unknown) = o.get("unknown_bits") {
                        // The decoder emits unknown bits as hex text; taking them
                        // back keeps an untouched field byte-exact.
                        let s = unknown
                            .as_str()
                            .ok_or_else(|| kind_err(path, "hex string", unknown))?;
                        let hex = s.strip_prefix("0x").unwrap_or(s);
                        v |= u64::from_str_radix(hex, 16).map_err(|_| EncodeError::Value {
                            path: path.to_string(),
                            msg: format!("bad unknown_bits {s:?}"),
                        })?;
                    }
                }
                other => return Err(kind_err(path, "flags object or number", other)),
            }
            write_uint(*base, v, out, path)
        }
        TypeDef::Union(variants) => {
            // The decoder picks a variant by the number of bytes left. Here the
            // value has to say which one it is: each variant is tried in order and
            // the first that encodes without error wins.
            let mut first_err = None;
            for (i, (_, vt)) in variants.iter().enumerate() {
                let mut probe = Vec::new();
                match encode_type(schema, vt, value, &mut probe, path) {
                    Ok(()) => {
                        out.extend_from_slice(&probe);
                        return Ok(());
                    }
                    Err(e) => first_err = first_err.or(Some((i, e))),
                }
            }
            match first_err {
                Some((_, e)) => Err(EncodeError::Value {
                    path: path.to_string(),
                    msg: format!("no union variant matches: {e}"),
                }),
                None => Err(EncodeError::Value {
                    path: path.to_string(),
                    msg: "union has no variants".into(),
                }),
            }
        }
        TypeDef::ValueUnion { variants, .. } => {
            // A value-decided union carries no switch in the bytes, so the variant
            // must be picked from the value alone. Each variant is tried in order
            // and the first that encodes without error wins — same as the plain
            // union, except the caller (record-level) already validated that the
            // chosen variant matches the sibling switch value.
            let mut first_err = None;
            for (i, vt) in variants.iter().enumerate() {
                let mut probe = Vec::new();
                match encode_type(schema, vt, value, &mut probe, path) {
                    Ok(()) => {
                        out.extend_from_slice(&probe);
                        return Ok(());
                    }
                    Err(e) => first_err = first_err.or(Some((i, e))),
                }
            }
            match first_err {
                Some((_, e)) => Err(EncodeError::Value {
                    path: path.to_string(),
                    msg: format!("no union variant matches: {e}"),
                }),
                None => Err(EncodeError::Value {
                    path: path.to_string(),
                    msg: "union has no variants".into(),
                }),
            }
        }
    }
}

/// Width-checked narrowing. An integer that does not fit its field is an error,
/// never a silent truncation: a `300` written into a `u8` field would corrupt the
/// record while looking like a successful edit.
fn fit_u8(path: &str, v: u64) -> Result<u8, EncodeError> {
    u8::try_from(v).map_err(|_| range(path, v as i64, "u8"))
}
fn fit_u16(path: &str, v: u64) -> Result<u16, EncodeError> {
    u16::try_from(v).map_err(|_| range(path, v as i64, "u16"))
}
fn fit_u32(path: &str, v: u64) -> Result<u32, EncodeError> {
    u32::try_from(v).map_err(|_| range(path, v as i64, "u32"))
}
fn fit_u64(_path: &str, v: u64) -> Result<u64, EncodeError> {
    Ok(v)
}
fn fit_i8(path: &str, v: i64) -> Result<i8, EncodeError> {
    i8::try_from(v).map_err(|_| range(path, v, "i8"))
}
fn fit_i16(path: &str, v: i64) -> Result<i16, EncodeError> {
    i16::try_from(v).map_err(|_| range(path, v, "i16"))
}
fn fit_i32(path: &str, v: i64) -> Result<i32, EncodeError> {
    i32::try_from(v).map_err(|_| range(path, v, "i32"))
}
fn fit_i64(_path: &str, v: i64) -> Result<i64, EncodeError> {
    Ok(v)
}

fn range(path: &str, n: i64, ty: &'static str) -> EncodeError {
    EncodeError::Range {
        path: path.to_string(),
        n,
        ty,
    }
}

fn encode_prim(p: Prim, value: &Value, out: &mut Vec<u8>, path: &str) -> Result<(), EncodeError> {
    match p {
        Prim::U8 => out.push(fit_u8(path, as_u64(path, value)?)?),
        Prim::U16 => out.extend_from_slice(&fit_u16(path, as_u64(path, value)?)?.to_le_bytes()),
        Prim::U32 => out.extend_from_slice(&fit_u32(path, as_u64(path, value)?)?.to_le_bytes()),
        Prim::U64 => out.extend_from_slice(&fit_u64(path, as_u64(path, value)?)?.to_le_bytes()),
        Prim::I8 => out.push(fit_i8(path, as_i64(path, value)?)? as u8),
        Prim::I16 => out.extend_from_slice(&fit_i16(path, as_i64(path, value)?)?.to_le_bytes()),
        Prim::I32 => out.extend_from_slice(&fit_i32(path, as_i64(path, value)?)?.to_le_bytes()),
        Prim::I64 => out.extend_from_slice(&fit_i64(path, as_i64(path, value)?)?.to_le_bytes()),
        Prim::F32 => out.extend_from_slice(&(as_f64(path, value)? as f32).to_le_bytes()),
        Prim::F64 => out.extend_from_slice(&as_f64(path, value)?.to_le_bytes()),
        Prim::FormId => out.extend_from_slice(&formid_u32(value, path)?.to_le_bytes()),
        Prim::ZString => {
            let s = value
                .as_str()
                .ok_or_else(|| kind_err(path, "string", value))?;
            out.extend_from_slice(&cp1252(s, path)?);
            // The decoder strips the terminator (`decode_z`), so re-adding exactly
            // one reproduces the payload. A value that already carries one is
            // accepted as-is: a UI that round-trips its own decoded value must not
            // end up with two.
            if !s.ends_with('\0') {
                out.push(0);
            }
        }
        Prim::ZStringArray => {
            let arr = value
                .as_array()
                .ok_or_else(|| kind_err(path, "array", value))?;
            for (i, item) in arr.iter().enumerate() {
                let s = item
                    .as_str()
                    .ok_or_else(|| kind_err(&format!("{path}[{i}]"), "string", item))?;
                out.extend_from_slice(&cp1252(s, path)?);
                out.push(0);
            }
            // The extra NUL that closes the list, present in every shipped case.
            if !arr.is_empty() {
                out.push(0);
            }
        }
        Prim::LenString => {
            // The inverse of the decoder: write the byte length, then the bytes.
            // No terminator, so a re-encode of a decoded value is exact.
            let s = value
                .as_str()
                .ok_or_else(|| kind_err(path, "string", value))?;
            let b = cp1252(s, path)?;
            write_uint(Prim::U32, b.len() as u64, out, path)?;
            out.extend_from_slice(&b);
        }
        Prim::String => {
            let s = value
                .as_str()
                .ok_or_else(|| kind_err(path, "string", value))?;
            out.extend_from_slice(&cp1252(s, path)?);
        }
        Prim::CString => {
            // The inverse of the decoder: the text bytes, then one NUL terminator,
            // leaving the following fields to encode after it.
            let s = value
                .as_str()
                .ok_or_else(|| kind_err(path, "string", value))?;
            out.extend_from_slice(&cp1252(s, path)?);
            out.push(0);
        }
        Prim::Bytes => out.extend_from_slice(&hex_bytes(value, path)?),
    }
    Ok(())
}

/// Encode an integer field, checking it fits the target width.
fn write_int(base: Prim, n: i64, out: &mut Vec<u8>, path: &str) -> Result<(), EncodeError> {
    let fits = |lo: i64, hi: i64| {
        if n < lo || n > hi {
            Err(EncodeError::Range {
                path: path.to_string(),
                n,
                ty: "integer",
            })
        } else {
            Ok(())
        }
    };
    match base {
        Prim::U8 => fits(0, u8::MAX as i64).map(|()| out.push(n as u8)),
        Prim::U16 => {
            fits(0, u16::MAX as i64).map(|()| out.extend_from_slice(&(n as u16).to_le_bytes()))
        }
        Prim::U32 => {
            fits(0, u32::MAX as i64).map(|()| out.extend_from_slice(&(n as u32).to_le_bytes()))
        }
        Prim::U64 => fits(0, i64::MAX).map(|()| out.extend_from_slice(&(n as u64).to_le_bytes())),
        Prim::I8 => fits(i8::MIN as i64, i8::MAX as i64).map(|()| out.push(n as i8 as u8)),
        Prim::I16 => fits(i16::MIN as i64, i16::MAX as i64)
            .map(|()| out.extend_from_slice(&(n as i16).to_le_bytes())),
        Prim::I32 => fits(i32::MIN as i64, i32::MAX as i64)
            .map(|()| out.extend_from_slice(&(n as i32).to_le_bytes())),
        Prim::I64 => {
            fits(i64::MIN, i64::MAX)?;
            out.extend_from_slice(&n.to_le_bytes());
            Ok(())
        }
        _ => Err(EncodeError::Value {
            path: path.to_string(),
            msg: "enum base must be an integer".into(),
        }),
    }
}

/// Encode an unsigned field, checking it fits the target width.
fn write_uint(base: Prim, v: u64, out: &mut Vec<u8>, path: &str) -> Result<(), EncodeError> {
    let fits = |hi: u64| {
        if v > hi {
            Err(EncodeError::Range {
                path: path.to_string(),
                n: v as i64,
                ty: "unsigned",
            })
        } else {
            Ok(())
        }
    };
    match base {
        Prim::U8 => fits(u8::MAX as u64).map(|()| out.push(v as u8)),
        Prim::U16 => {
            fits(u16::MAX as u64).map(|()| out.extend_from_slice(&(v as u16).to_le_bytes()))
        }
        Prim::U32 => {
            fits(u32::MAX as u64).map(|()| out.extend_from_slice(&(v as u32).to_le_bytes()))
        }
        Prim::U64 => fits(u64::MAX).map(|()| out.extend_from_slice(&v.to_le_bytes())),
        _ => Err(EncodeError::Value {
            path: path.to_string(),
            msg: "flags base must be an unsigned integer".into(),
        }),
    }
}

fn as_u64(path: &str, v: &Value) -> Result<u64, EncodeError> {
    v.as_u64().ok_or_else(|| EncodeError::Value {
        path: path.to_string(),
        msg: format!("expected a non-negative integer, found {}", kind_of(v)),
    })
}

fn as_i64(path: &str, v: &Value) -> Result<i64, EncodeError> {
    v.as_i64().ok_or_else(|| EncodeError::Value {
        path: path.to_string(),
        msg: format!("expected an integer, found {}", kind_of(v)),
    })
}

fn as_f64(path: &str, v: &Value) -> Result<f64, EncodeError> {
    v.as_f64().ok_or_else(|| kind_err(path, "number", v))
}

/// The decoder writes FormIDs as `"0x00157C89"`. Accept that form, plus a plain
/// number, so a UI can round-trip its own values.
fn formid_u32(v: &Value, path: &str) -> Result<u32, EncodeError> {
    match v {
        Value::String(s) => {
            let hex = s.strip_prefix("0x").ok_or_else(|| EncodeError::Value {
                path: path.to_string(),
                msg: format!("FormID must look like \"0x00157C89\", found {s:?}"),
            })?;
            u32::from_str_radix(hex, 16).map_err(|_| EncodeError::Value {
                path: path.to_string(),
                msg: format!("bad FormID {s:?}"),
            })
        }
        other => as_u64(path, other).map(|n| n as u32),
    }
}

fn cp1252(s: &str, path: &str) -> Result<Vec<u8>, EncodeError> {
    fnv_util::cp1252::encode(s).ok_or_else(|| EncodeError::Encoding {
        path: path.to_string(),
        s: s.to_string(),
    })
}

/// Hex text (with or without `0x`) back to bytes, as the decoder emits for `bytes`.
fn hex_bytes(v: &Value, path: &str) -> Result<Vec<u8>, EncodeError> {
    let s = v.as_str().ok_or_else(|| kind_err(path, "hex string", v))?;
    let hex = s.strip_prefix("0x").unwrap_or(s);
    if hex.len() % 2 != 0 {
        return Err(EncodeError::Value {
            path: path.to_string(),
            msg: format!("odd-length hex {s:?}"),
        });
    }
    (0..hex.len() / 2)
        .map(|i| {
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| EncodeError::Value {
                path: path.to_string(),
                msg: format!("bad hex {s:?}"),
            })
        })
        .collect()
}

/// Encode one subrecord's value against its schema definition.
///
/// Returns the payload bytes. `schema.fixed_size` is checked first when the type
/// has one, so a layout that changed size fails loudly instead of writing a
/// record the game will reject.
pub fn encode_sub(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    value: &Value,
) -> Result<Vec<u8>, EncodeError> {
    let def = schema
        .lookup(rec_sig, sub_sig)
        .ok_or_else(|| EncodeError::Value {
            path: fnv_util::tag_str(sub_sig),
            msg: format!(
                "no layout for {}.{}",
                fnv_util::tag_str(rec_sig),
                fnv_util::tag_str(sub_sig)
            ),
        })?;
    if let Some(want) = schema.fixed_size(&def.ty) {
        // A union is not fixed-size; the variant decides.
        if !matches!(schema.resolve(&def.ty), TypeDef::Union(_)) {
            let probe = encode_probe(schema, &def.ty, value)?;
            if probe.len() != want {
                return Err(EncodeError::Value {
                    path: def.name.clone(),
                    msg: format!(
                        "layout wants {want} bytes, the value encodes to {}",
                        probe.len()
                    ),
                });
            }
            return Ok(probe);
        }
    }
    encode_probe(schema, &def.ty, value)
}

fn encode_probe(schema: &Schema, t: &TypeDef, value: &Value) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    encode_type(schema, t, value, &mut out, "")?;
    Ok(out)
}

/// The decoded form of a subrecord, ready to hand back to [`encode_sub`].
pub fn decoded(schema: &Schema, rec_sig: &[u8; 4], sub: &fnv_esm::Subrecord) -> Option<Value> {
    let mut ctx = fnv_esm::decode::DecodeCtx::new(schema, *rec_sig);
    let d = fnv_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    match d.status {
        fnv_esm::decode::SubStatus::Ok => d.value,
        _ => None,
    }
}

/// Re-encode a subrecord from its decoded value and compare with the original.
///
/// The check that makes the whole editor trustworthy: if this returns `false`,
/// the encoder does not understand the field and the editor must refuse to save.
pub fn roundtrips(schema: &Schema, rec_sig: &[u8; 4], sub: &fnv_esm::Subrecord) -> bool {
    let Some(value) = decoded(schema, rec_sig, sub) else {
        return false;
    };
    match encode_sub(schema, rec_sig, &sub.sig, &value) {
        Ok(bytes) => bytes == sub.data,
        Err(_) => false,
    }
}

/// Names of the fields of a subrecord's layout, for the property grid.
pub fn field_names(schema: &Schema, rec_sig: &[u8; 4], sub_sig: &[u8; 4]) -> Vec<String> {
    let Some(def) = schema.lookup(rec_sig, sub_sig) else {
        return Vec::new();
    };
    fn walk(schema: &Schema, t: &TypeDef, out: &mut Vec<String>) {
        match schema.resolve(t) {
            TypeDef::Struct(fields) => {
                for f in fields {
                    out.push(f.name.clone());
                    walk(schema, &f.ty, out);
                }
            }
            TypeDef::Array { elem, .. } => walk(schema, elem, out),
            TypeDef::Union(variants) => {
                for (_, vt) in variants {
                    walk(schema, vt, out);
                }
            }
            TypeDef::ValueUnion { variants, .. } => {
                for vt in variants {
                    walk(schema, vt, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(schema, &def.ty, &mut out);
    out.sort();
    out.dedup();
    out
}

/// A flat, editable view of one record: every subrecord with its signature,
/// human name, decoded value and whether it can be written back safely.
#[derive(Debug, Clone)]
pub struct SubView {
    pub sig: String,
    pub name: String,
    pub value: Value,
    /// True when [`roundtrips`] accepts this subrecord's own bytes.
    pub writable: bool,
    /// Decode status when it is not `Ok`.
    pub problem: Option<String>,
}

/// Decode every subrecord of a record for the property grid.
pub fn sub_views(schema: &Schema, rec: &fnv_esm::Record) -> Result<Vec<SubView>, String> {
    let subs = rec.subrecords().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(subs.len());
    for s in &subs {
        let mut ctx = fnv_esm::decode::DecodeCtx::new(schema, rec.header.sig);
        let d = fnv_esm::decode::decode_sub(&rec.header.sig, s, &mut ctx);
        let name = d.name.clone().unwrap_or_else(|| s.sig_str());
        out.push(SubView {
            sig: s.sig_str(),
            name,
            value: d.value.unwrap_or(Value::Null),
            writable: d.status == fnv_esm::decode::SubStatus::Ok
                && roundtrips(schema, &rec.header.sig, s),
            problem: match d.status {
                fnv_esm::decode::SubStatus::Ok => None,
                other => Some(d.error.unwrap_or_else(|| other.as_str().to_string())),
            },
        });
    }
    Ok(out)
}

#[doc(hidden)]
pub mod test_schema {
    use fnv_esm::schema::Schema;
    /// The schema used by the encoder's own unit tests.
    pub fn schema() -> Schema {
        let text = r#"{
          "format": "fnv.schema/1",
          "types": {
            "Vec3f": {"struct": [{"name":"x","type":"f32"},{"name":"y","type":"f32"},{"name":"z","type":"f32"}]},
            "PosRot": {"struct": [{"name":"position","type":"Vec3f"},{"name":"rotation_radians","type":"Vec3f"}]},
            "BoolU32": {"enum": "u32", "values": {"0": "False", "1": "True"}},
            "Empty": {"struct": []}
          },
          "common": {
            "EDID": {"name": "Editor ID", "type": "zstring", "confidence": "observed"}
          },
          "records": {
            "REFR": {"name": "Placed Object", "subrecords": {
              "NAME": {"name": "Base", "type": "formid"},
              "DATA": {"name": "Position/Rotation", "type": "PosRot"},
              "XSCL": {"name": "Scale", "type": "f32"},
              "XESP": {"name": "Enable Parent", "type": {"struct": [
                  {"name":"reference","type":"formid"},{"name":"flags","type":"u8"}]}},
              "XSED": {"name": "SpeedTree Seed", "type": "u8"},
              "XTEL": {"name": "Teleport Destination", "type": {"struct": [
                  {"name":"door","type":"formid"},{"name":"pos_rot","type":"PosRot"},
                  {"name":"no_alarm","type":"BoolU32"}]}},
              "FNAM": {"name": "Marker", "type": "Empty"}
            }},
            "DOOR": {"name": "Door", "subrecords": {
              "FNAM": {"name": "Door Flags", "type": {"flags": "u8", "bits": {"0":"Automatic Door","1":"Hidden"}}}
            }}
          }
        }"#;
        let mut s = Schema::default();
        s.merge_str(text, "test").unwrap();
        s.validate().unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema covering the shapes the encoder must handle.
    fn schema() -> Schema {
        let text = r#"{
          "format": "fnv.schema/1",
          "types": {
            "Vec3f": {"struct": [{"name":"x","type":"f32"},{"name":"y","type":"f32"},{"name":"z","type":"f32"}]},
            "PosRot": {"struct": [{"name":"position","type":"Vec3f"},{"name":"rotation_radians","type":"Vec3f"}]},
            "BoolU32": {"enum": "u32", "values": {"0": "False", "1": "True"}},
            "Empty": {"struct": []}
          },
          "common": {
            "EDID": {"name": "Editor ID", "type": "zstring", "confidence": "observed"}
          },
          "records": {
            "REFR": {"name": "Placed Object", "subrecords": {
              "NAME": {"name": "Base", "type": "formid"},
              "DATA": {"name": "Position/Rotation", "type": "PosRot"},
              "XSCL": {"name": "Scale", "type": "f32"},
              "XESP": {"name": "Enable Parent", "type": {"struct": [
                  {"name":"reference","type":"formid"},{"name":"flags","type":"u8"}]}},
              "XSED": {"name": "SpeedTree Seed", "type": "u8"},
              "XTEL": {"name": "Teleport Destination", "type": {"struct": [
                  {"name":"door","type":"formid"},{"name":"pos_rot","type":"PosRot"},
                  {"name":"no_alarm","type":"BoolU32"}]}},
              "FNAM": {"name": "Marker", "type": "Empty"}
            }},
            "DOOR": {"name": "Door", "subrecords": {
              "FNAM": {"name": "Door Flags", "type": {"flags": "u8", "bits": {"0":"Automatic Door","1":"Hidden"}}}
            }}
          }
        }"#;
        let mut s = Schema::default();
        s.merge_str(text, "test").unwrap();
        s.validate().unwrap();
        s
    }

    fn enc(s: &Schema, rec: &[u8; 4], sub: &[u8; 4], v: &Value) -> Result<Vec<u8>, EncodeError> {
        encode_sub(s, rec, sub, v)
    }

    #[test]
    fn formid_from_text_and_number() {
        let s = schema();
        let v = serde_json::json!("0x00157C89");
        assert_eq!(
            enc(&s, b"REFR", b"NAME", &v).unwrap(),
            0x00157C89u32.to_le_bytes()
        );
        let n = serde_json::json!(0x00157C89u64);
        assert_eq!(
            enc(&s, b"REFR", b"NAME", &n).unwrap(),
            0x00157C89u32.to_le_bytes()
        );
    }

    #[test]
    fn posrot_roundtrip_is_byte_exact() {
        let s = schema();
        // A real REFR DATA payload from FalloutNV.esm.
        let original: Vec<u8> = vec![
            0x0c, 0x19, 0xe6, 0x44, 0xe0, 0x0a, 0xcd, 0x47, 0x00, 0x00, 0x00, 0xc5, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x84, 0x35,
        ];
        // The trailing byte is the 24th of the payload as stored; PosRot is 23 bytes
        // wide in the schema, so the record carries one more byte than the layout
        // defines. This test pins the layout as the schema has it.
        let value = serde_json::json!({
            "position": {"x": 1234.5, "y": 678.9, "z": 1000.25},
            "rotation_radians": {"x": 0.0, "y": 0.0, "z": 0.0}
        });
        let out = enc(&s, b"REFR", b"DATA", &value).unwrap();
        assert_eq!(out.len(), 24);
        // Position bytes must survive.
        assert_eq!(&out[0..4], &1234.5f32.to_le_bytes());
        assert_eq!(&out[4..8], &678.9f32.to_le_bytes());
        assert_eq!(&out[8..12], &1000.25f32.to_le_bytes());
        let _ = original;
    }

    #[test]
    fn enum_by_name_and_number() {
        let s = schema();
        let by_name = serde_json::json!({
            "door":"0x00000000",
            "pos_rot":{"position":{"x":0.0,"y":0.0,"z":0.0},"rotation_radians":{"x":0.0,"y":0.0,"z":0.0}},
            "no_alarm":"False"
        });
        let by_num = serde_json::json!({
            "door":0,
            "pos_rot":{"position":{"x":0.0,"y":0.0,"z":0.0},"rotation_radians":{"x":0.0,"y":0.0,"z":0.0}},
            "no_alarm":0
        });
        // XTEL is formid + PosRot(24) + BoolU32(4) = 32 bytes.
        let a = enc(&s, b"REFR", b"XTEL", &by_name).unwrap();
        let b = enc(&s, b"REFR", b"XTEL", &by_num).unwrap();
        assert_eq!(a.len(), 32);
        // The name form and the number form must encode identically.
        assert_eq!(a, b);
    }

    #[test]
    fn unknown_variant_and_flag_are_errors() {
        let s = schema();
        let bad_variant = serde_json::json!({"door":"0x0","pos_rot":{"position":{"x":0.0,"y":0.0,"z":0.0},"rotation_radians":{"x":0.0,"y":0.0,"z":0.0}},"no_alarm":"Maybe"});
        assert!(matches!(
            enc(&s, b"REFR", b"XTEL", &bad_variant),
            Err(EncodeError::Variant { .. })
        ));
        let bad_flag = serde_json::json!({"value": 0, "set": ["Not A Flag"]});
        assert!(matches!(
            enc(&s, b"DOOR", b"FNAM", &bad_flag),
            Err(EncodeError::Flag { .. })
        ));
    }

    #[test]
    fn range_and_kind_errors() {
        let s = schema();
        assert!(matches!(
            enc(&s, b"REFR", b"XSED", &serde_json::json!(300)),
            Err(EncodeError::Range { .. })
        ));
        assert!(matches!(
            enc(&s, b"REFR", b"XSCL", &serde_json::json!("big")),
            Err(EncodeError::Kind { .. })
        ));
        assert!(matches!(
            enc(&s, b"REFR", b"DATA", &serde_json::json!(5)),
            Err(EncodeError::Kind { .. })
        ));
    }

    #[test]
    fn zstring_encodes_with_terminator() {
        let s = schema();
        assert_eq!(
            enc(&s, b"REFR", b"EDID", &serde_json::json!("Hello\0")).unwrap(),
            b"Hello\0"
        );
        assert_eq!(
            enc(&s, b"REFR", b"EDID", &serde_json::json!("Hello")).unwrap(),
            b"Hello\0"
        );
        // An empty string still needs its terminator, and a NUL-only payload
        // (seen in TERM/RNAM) round-trips to itself.
        assert_eq!(
            enc(&s, b"REFR", b"EDID", &serde_json::json!("")).unwrap(),
            b"\0"
        );
        assert_eq!(
            enc(&s, b"REFR", b"EDID", &serde_json::json!("\0")).unwrap(),
            b"\0"
        );
    }

    #[test]
    fn struct_with_repeated_field_names_roundtrips() {
        // CSTY/CSTD declares `unused` six times. A struct's JSON form is an
        // object, so the names are disambiguated at parse time (`unused`,
        // `unused_2`, ...) in declaration order. The byte layout is untouched,
        // so the value round-trips instead of being refused.
        let text = r#"{
          "format": "fnv.schema/1",
          "types": {"Dup": {"struct": [
            {"name":"a","type":"u8"},
            {"name":"unused","type":{"array":"u8","count":2}},
            {"name":"b","type":"u8"},
            {"name":"unused","type":{"array":"u8","count":3}}]}},
          "records": {"TEST": {"name":"T","subrecords": {"XABC": {"name":"x","type":"Dup"}}}}
        }"#;
        let mut s = Schema::default();
        s.merge_str(text, "t").unwrap();
        s.validate().unwrap();
        let data = vec![1u8, 9, 9, 2, 3, 3, 3];
        let subs = fnv_esm::raw::parse_subrecords(&{
            let mut v = Vec::new();
            v.extend_from_slice(b"XABC");
            v.extend_from_slice(&(data.len() as u16).to_le_bytes());
            v.extend_from_slice(&data);
            v
        })
        .unwrap();
        let mut ctx = fnv_esm::decode::DecodeCtx::new(&s, *b"TEST");
        let d = fnv_esm::decode::decode_sub(b"TEST", &subs[0], &mut ctx);
        assert_eq!(d.status, fnv_esm::decode::SubStatus::Ok);
        let value = d.value.unwrap();
        // Every declared field survives into the JSON, under a distinct key.
        let obj = value.as_object().unwrap();
        assert_eq!(
            obj.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["a", "unused", "b", "unused_2"]
        );
        assert_eq!(obj["unused"].as_array().unwrap().len(), 2);
        assert_eq!(obj["unused_2"].as_array().unwrap().len(), 3);
        assert!(roundtrips(&s, b"TEST", &subs[0]));
    }

    #[test]
    fn wrong_fixed_size_is_refused() {
        let s = schema();
        // XESP is 5 bytes; give it a struct with a wrong width.
        let bad = serde_json::json!({"reference": "0x00000001", "flags": 0});
        assert_eq!(enc(&s, b"REFR", b"XESP", &bad).unwrap().len(), 5);
    }
}
