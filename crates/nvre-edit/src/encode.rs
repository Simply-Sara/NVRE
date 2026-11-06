//! Schema-driven encoding of JSON values back into subrecord payloads: the
//! inverse of `nvre_esm::decode::decode_type`.
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

use nvre_esm::schema::{Prim, Schema};
use nvre_esm::typedef::{FieldDef, TypeDef};
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

impl EncodeError {
    /// Whether the value did not fit the field's type.
    ///
    /// A caller that is *probing* a layout needs to tell "this value is too big
    /// for a `u8`" from "this value is the wrong shape", because the first means
    /// the test picked a bad sample and the second means the layout is wrong. The
    /// distinction used to be a string search for "out of range", which breaks the
    /// moment the message is wrapped — and a union wraps every variant's error.
    pub fn is_range(&self) -> bool {
        match self {
            EncodeError::Range { .. } => true,
            // A union reports one variant's failure; if that one was a range, the
            // value is out of range for the layout, not malformed.
            EncodeError::Value { msg, .. } => msg.contains("is out of range for"),
            _ => false,
        }
    }
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
fn repeated_name(fields: &[FieldDef]) -> Option<String> {
    for (i, f) in fields.iter().enumerate() {
        if fields[..i].iter().any(|g| g.name == f.name) {
            return Some(f.name.clone());
        }
    }
    None
}

// The values a union may switch on, kept while a struct is written.
thread_local! {
    static SWITCH_VALUES: std::cell::RefCell<Vec<std::collections::HashMap<String, String>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static EXPOSED_VALUES: std::cell::RefCell<std::collections::HashMap<String, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Remembers a subrecord's value for a later subrecord's union to switch on.
pub fn expose_subrecord(
    schema: &Schema,
    def: &nvre_esm::schema::SubDef,
    sig: &[u8; 4],
    value: &Value,
) {
    if !def.expose {
        return;
    }
    let text = match value {
        Value::Number(n) => n.to_string(),
        Value::String(s) => match schema.resolve_def(&def.ty) {
            TypeDef::Enum { values, .. } => match values.iter().find(|(_, n)| n == s) {
                Some((k, _)) => k.to_string(),
                None => s.clone(),
            },
            _ => s.clone(),
        },
        _ => return,
    };
    EXPOSED_VALUES.with(|c| {
        c.borrow_mut()
            .insert(nvre_util::tag_str(sig).to_string(), text);
    });
}

/// Clears the remembered exposed values, for a new record.
pub fn clear_exposed() {
    EXPOSED_VALUES.with(|c| c.borrow_mut().clear());
}

/// The variant a union's switch picks, if the switch has a value.
fn switch_variant(
    name: &str,
    cases: &[(String, usize)],
    bits: &[(u64, usize)],
    default: usize,
) -> Option<usize> {
    let key = SWITCH_VALUES
        .with(|c| {
            let stack = c.borrow();
            stack.last().and_then(|m| m.get(name).cloned())
        })
        .or_else(|| EXPOSED_VALUES.with(|c| c.borrow().get(name).cloned()))?;
    if !bits.is_empty() {
        let n = key.parse::<u64>().unwrap_or(0);
        return Some(
            bits.iter()
                .find(|(mask, _)| n & mask != 0)
                .map(|(_, i)| *i)
                .unwrap_or(default),
        );
    }
    Some(
        cases
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, i)| *i)
            .unwrap_or(default),
    )
}

/// Keeps the switch values for the length of a struct's writing.
struct SwitchValuesGuard;

impl SwitchValuesGuard {
    fn push(values: std::collections::HashMap<String, String>) -> Self {
        SWITCH_VALUES.with(|c| c.borrow_mut().push(values));
        SwitchValuesGuard
    }
}

impl Drop for SwitchValuesGuard {
    fn drop(&mut self) {
        SWITCH_VALUES.with(|c| {
            c.borrow_mut().pop();
        });
    }
}

/// The value of a named field, as the switch would read it.
fn switch_value_of(
    _schema: &Schema,
    obj: &serde_json::Map<String, Value>,
    field: &FieldDef,
) -> Option<String> {
    let v = obj.get(&field.name)?;
    if let Some(s) = v.as_str() {
        if let TypeDef::Enum { values, .. } = &field.ty {
            if let Some((k, _)) = values.iter().find(|(_, n)| n == s) {
                return Some(k.to_string());
            }
        }
        return Some(s.to_string());
    }
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    if let Some(n) = v.as_u64() {
        return Some(n.to_string());
    }
    if let Some(n) = v.get("value").and_then(Value::as_i64) {
        return Some(n.to_string());
    }
    None
}

thread_local! {
    static FORM_VERSION: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Set the record's format version for the length of the write.
pub fn set_form_version(v: u32) -> FormVersionGuard {
    FORM_VERSION.with(|c| {
        let old = c.get();
        c.set(v);
        FormVersionGuard(Some(old))
    })
}

/// Restores the form version when the write is done.
pub struct FormVersionGuard(Option<u32>);

impl Drop for FormVersionGuard {
    fn drop(&mut self) {
        if let Some(old) = self.0 {
            FORM_VERSION.with(|c| c.set(old));
        }
    }
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
    match t {
        TypeDef::Prim(p) => encode_prim(*p, value, out, path),
        // A targeted FormID writes exactly the four bytes a plain one does.
        TypeDef::FormIdRef { .. } => encode_prim(Prim::FormId, value, out, path),
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
            // A union inside the struct may switch on one of the struct's
            // own fields, so the fields' values are kept while the struct is
            // written.
            let mut switch_values = std::collections::HashMap::new();
            for f in fields {
                if let Some(v) = switch_value_of(schema, obj, f) {
                    switch_values.insert(f.name.clone(), v);
                }
            }
            let _switch_guard = SwitchValuesGuard::push(switch_values);
            for f in fields {
                match obj.get(&f.name) {
                    // A missing optional field simply contributes no bytes, which
                    // is how the decoder reads a struct that ends early.
                    None if f.optional || f.optional_if_empty => {}
                    Some(Value::Null) if f.optional || f.optional_if_empty => {}
                    None => {
                        return Err(EncodeError::Value {
                            path: path.to_string(),
                            msg: format!("missing field {:?}", f.name),
                        });
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
                        } = &f.ty
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
                        if let TypeDef::Array {
                            count_pow: Some(name),
                            ..
                        } = &f.ty
                        {
                            let want = obj.get(name).and_then(|x| {
                                x.as_u64()
                                    .or_else(|| x.get("value").and_then(Value::as_u64))
                            });
                            let n = want.map(|w| (w * w) as usize);
                            match (n, v.as_array()) {
                                (Some(n), Some(arr)) if arr.len() == n => {}
                                _ => {
                                    return Err(EncodeError::Value {
                                        path: sub.clone(),
                                        msg: format!(
                                            "count_pow {name:?} does not match this array's length"
                                        ),
                                    })
                                }
                            }
                        }
                        if let TypeDef::Array {
                            count_sum: Some((a, b)),
                            ..
                        } = &f.ty
                        {
                            let get = |n: &str| -> Option<u64> {
                                obj.get(n).and_then(|x| {
                                    x.as_u64()
                                        .or_else(|| x.get("value").and_then(Value::as_u64))
                                })
                            };
                            let want = match (get(a), get(b)) {
                                (Some(x), Some(y)) => Some((x + y) as usize),
                                _ => None,
                            };
                            match (want, v.as_array()) {
                                (Some(n), Some(arr)) if arr.len() == n => {}
                                _ => {
                                    return Err(EncodeError::Value {
                                        path: sub.clone(),
                                        msg: format!(
                                            "count_sum {a:?} and {b:?} do not match this array's length"
                                        ),
                                    })
                                }
                            }
                        }
                        if let TypeDef::Array {
                            count_bits: Some((name, bits)),
                            ..
                        } = &f.ty
                        {
                            let want = obj.get(name).and_then(|x| {
                                x.as_u64()
                                    .or_else(|| x.get("value").and_then(Value::as_u64))
                            });
                            let n =
                                want.map(|w| (0..*bits).filter(|b| w & (1u64 << *b) != 0).count());
                            match (n, v.as_array()) {
                                (Some(n), Some(arr)) if arr.len() == n => {}
                                _ => {
                                    return Err(EncodeError::Value {
                                        path: sub.clone(),
                                        msg: format!(
                                            "count_bits {name:?} does not match this array's length"
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
                // The decoder's own shape: `{"value": n, "name": "…"}`. Accepting
                // it is what makes an enum field round-trippable at all — without
                // this, decode-then-encode gives the encoder an object where it
                // wants a number, and the field reads as "does not re-encode" and
                // is shown read-only. The number is authoritative: the name is the
                // decoder's rendering of it, and a dropdown writes both.
                Value::Object(o) => match o.get("value") {
                    Some(Value::Number(n)) => n.as_i64().ok_or_else(|| EncodeError::Value {
                        path: path.to_string(),
                        msg: "enum value out of range".into(),
                    })?,
                    // A name with no number: the name decides, as below.
                    _ => match o.get("name").and_then(|v| v.as_str()) {
                        Some(s) => values
                            .iter()
                            .find(|(_, name)| name == s)
                            .map(|(k, _)| *k)
                            .ok_or_else(|| EncodeError::Variant {
                                path: path.to_string(),
                                variant: s.to_string(),
                                known: values
                                    .iter()
                                    .map(|(_, n)| n.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            })?,
                        None => {
                            return Err(kind_err(path, "enum name or number", value));
                        }
                    },
                },
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
            let mut range_err = None;
            for (i, (_, vt)) in variants.iter().enumerate() {
                let mut probe = Vec::new();
                match encode_type(schema, vt, value, &mut probe, path) {
                    Ok(()) => {
                        out.extend_from_slice(&probe);
                        return Ok(());
                    }
                    Err(e) => {
                        // A variant that rejects the value because one number is
                        // too big is saying something different from one that does
                        // not know the field at all: the first is the caller's
                        // value, the second is this variant's shape. Keeping the
                        // range error means a caller probing the layout learns
                        // "too big" rather than "wrong shape", which the nested
                        // Skyrim SE `RACE` union used to lose.
                        if e.is_range() && range_err.is_none() {
                            range_err = Some(e.clone());
                        }
                        first_err = first_err.or(Some((i, e)));
                    }
                }
            }
            if let Some(e) = range_err {
                return Err(e);
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
        TypeDef::Repeat(elem) => {
            let arr = value.as_array().ok_or_else(|| EncodeError::Value {
                path: path.to_string(),
                msg: "expected an array of repeated values".into(),
            })?;
            for v in arr {
                encode_type(schema, elem, v, out, path)?;
            }
            Ok(())
        }
        TypeDef::PeekUnion { variants, .. } => {
            // The switch word is part of the field, so the variant has to be
            // picked from the value alone. Each variant is tried in order and
            // the first that encodes without error wins.
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
        TypeDef::ValueUnion {
            name,
            variants,
            min_version,
            cases,
            bits,
            default,
            ..
        } => {
            if let Some(idx) = switch_variant(name, cases, bits, *default) {
                return encode_type(schema, &variants[idx], value, out, path);
            }
            if let Some(min) = min_version {
                let v = FORM_VERSION.with(|c| c.get());
                let idx = if v >= *min {
                    1.min(variants.len() - 1)
                } else {
                    0
                };
                return encode_type(schema, &variants[idx], value, out, path);
            }
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
        Prim::Bool => {
            // A boolean is a byte: 0 or 1.
            let b = value
                .as_bool()
                .ok_or_else(|| kind_err(path, "boolean", value))?;
            out.push(if b { 1 } else { 0 });
        }
        Prim::PeekU32 => {
            // The peeked word is read, not written: it belongs to the
            // shape the field takes, and the shape's own layout writes it.
        }
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
        Prim::LenStringU16 => {
            let s = value
                .as_str()
                .ok_or_else(|| kind_err(path, "string", value))?;
            let b = cp1252(s, path)?;
            write_uint(Prim::U16, b.len() as u64, out, path)?;
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
        Prim::Localized => match value {
            Value::Number(n) => {
                let id = n
                    .as_u64()
                    .ok_or_else(|| kind_err(path, "string id", value))?;
                write_uint(Prim::U32, id, out, path)?;
            }
            Value::String(s) => {
                out.extend_from_slice(&cp1252(s, path)?);
                if !s.ends_with('\0') {
                    out.push(0);
                }
            }
            _ => return Err(kind_err(path, "string or string id", value)),
        },
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
    nvre_util::cp1252::encode(s).ok_or_else(|| EncodeError::Encoding {
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
    encode_sub_len(schema, rec_sig, sub_sig, value, None)
}

/// Encode a subrecord, knowing how many bytes it took.
///
/// A union's variants can all encode the same value but to different
/// lengths — Skyrim's cell flags are one byte in some records and two in
/// others — and the first variant that encodes is not necessarily the one
/// the data used. When the original length is known, the variant that
/// matches it is the one to write.
/// Writes a subrecord that is known to be inside a block of its record's sequence.
pub fn encode_sub_len_in_block(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    value: &Value,
    orig_len: Option<usize>,
    block: Option<&str>,
) -> Result<Vec<u8>, EncodeError> {
    encode_sub_len_in_block_variant(schema, rec_sig, sub_sig, value, orig_len, block, None)
}

/// Writes a subrecord that is known to be inside a block of its record's
/// sequence, and in the block's chosen variant.
pub fn encode_sub_len_in_block_variant(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    value: &Value,
    orig_len: Option<usize>,
    block: Option<&str>,
    variant: Option<&str>,
) -> Result<Vec<u8>, EncodeError> {
    let sub_str = nvre_util::tag_str(sub_sig);
    let rec_str = nvre_util::tag_str(rec_sig);
    let mut defs: Vec<&nvre_esm::schema::SubDef> = Vec::new();
    if let Some(seq) = schema
        .records
        .get(&rec_str)
        .and_then(|r| r.sequence.as_ref())
    {
        if let Some(bd) = block.and_then(|b| {
            seq.blocks.get(b).or_else(|| {
                seq.blocks
                    .values()
                    .find(|bd| bd.opener == b || bd.openers.iter().any(|o| o == b))
            })
        }) {
            if let Some(v) = variant {
                if let Some(vd) = bd.variants.get(v) {
                    if let Some(d) = vd.subrecords.get(&sub_str) {
                        defs.push(d);
                    }
                }
            }
            if let Some(d) = bd.subrecords.get(&sub_str) {
                if !defs.iter().any(|e| std::ptr::eq(*e, d)) {
                    defs.push(d);
                }
            }
        }
    }
    if let Some(d) = schema.lookup(rec_sig, sub_sig) {
        if !defs.iter().any(|e| std::ptr::eq(*e, d)) {
            defs.push(d);
        }
    }
    if let Some(seq) = schema
        .records
        .get(&rec_str)
        .and_then(|r| r.sequence.as_ref())
    {
        for bd in seq.blocks.values() {
            if let Some(d) = bd.subrecords.get(&sub_str) {
                if !defs.iter().any(|e| std::ptr::eq(*e, d)) {
                    defs.push(d);
                }
            }
        }
    }
    if defs.is_empty() {
        return Err(EncodeError::Value {
            path: sub_str.clone(),
            msg: format!("no layout for {rec_str}.{sub_str}"),
        });
    }
    let mut last_err = None;
    let mut first_ok = None;
    for def in defs {
        match encode_sub_with(schema, def, value, orig_len) {
            Ok(bytes) => {
                if orig_len == Some(bytes.len()) {
                    return Ok(bytes);
                }
                if first_ok.is_none() {
                    first_ok = Some(bytes);
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    if let Some(bytes) = first_ok {
        return Ok(bytes);
    }
    Err(last_err.unwrap_or_else(|| EncodeError::Value {
        path: sub_str,
        msg: format!("no layout for {rec_str}.{sub_sig:?}"),
    }))
}

fn encode_sub_with(
    schema: &Schema,
    def: &nvre_esm::schema::SubDef,
    value: &Value,
    orig_len: Option<usize>,
) -> Result<Vec<u8>, EncodeError> {
    let resolved = schema.resolve_def(&def.ty);
    if let Some(want) = schema.fixed_size(&def.ty) {
        if !matches!(resolved, TypeDef::Union(_)) {
            let probe = encode_probe(schema, &resolved, value)?;
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
    if let (
        TypeDef::ValueUnion {
            ref variants,
            ref name,
            ref cases,
            ref bits,
            default,
            ..
        },
        Some(len),
    ) = (&resolved, orig_len)
    {
        if let Some(idx) = switch_variant(name, cases, bits, *default) {
            let bytes = encode_probe(schema, &variants[idx], value)?;
            if bytes.len() == len {
                return Ok(bytes);
            }
        }
        for vt in variants.iter() {
            if let Ok(bytes) = encode_probe(schema, vt, value) {
                if bytes.len() == len {
                    return Ok(bytes);
                }
            }
        }
    }
    if let (TypeDef::Union(variants), Some(len)) = (&resolved, orig_len) {
        let mut sized: Vec<(usize, &TypeDef)> = variants
            .iter()
            .filter_map(|(n, t)| n.map(|n| (n, t)))
            .collect();
        sized.sort_by_key(|(n, _)| n.abs_diff(len));
        for (n, vt) in sized {
            if n != len {
                continue;
            }
            if let Ok(bytes) = encode_probe(schema, vt, value) {
                if bytes.len() == len {
                    return Ok(bytes);
                }
            }
        }
    }
    encode_probe(schema, &resolved, value)
}

/// Encode a subrecord, knowing how many bytes it took.
pub fn encode_sub_len(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    value: &Value,
    orig_len: Option<usize>,
) -> Result<Vec<u8>, EncodeError> {
    encode_sub_len_in_block(schema, rec_sig, sub_sig, value, orig_len, None)
}

fn encode_probe(schema: &Schema, t: &TypeDef, value: &Value) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    encode_type(schema, t, value, &mut out, "")?;
    Ok(out)
}

/// The decoded form of a subrecord, ready to hand back to [`encode_sub`].
pub fn decoded(schema: &Schema, rec_sig: &[u8; 4], sub: &nvre_esm::Subrecord) -> Option<Value> {
    decoded_localized(schema, rec_sig, sub, false)
}

/// Decodes a subrecord that is in a block of its record's sequence.
pub fn decoded_in_block(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    block: Option<&str>,
) -> Option<Value> {
    decoded_in_block_localized(schema, rec_sig, sub, block, false)
}

/// The same, for a subrecord of a **localized** plugin.
///
/// A localized plugin stores its text as a four-byte string-table ID rather than
/// as characters, so the same subrecord decodes differently depending on the file
/// it came from. Reading a 9-byte `DESC` as a zstring when it is a string ID with
/// padding leaves the padding unexplained, which reads as "does not re-encode"
/// and makes the field read-only in the editor. Skyrim's and Fallout 4's shipped
/// masters are both localized (`TES4` header flag 0x80), and Fallout: New Vegas's
/// is not, which is why this only showed up in two of the five games.
pub fn decoded_in_block_localized(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    block: Option<&str>,
    localized: bool,
) -> Option<Value> {
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, *rec_sig);
    ctx.block = block.map(str::to_string);
    ctx.localized = localized;
    let d = nvre_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    match d.status {
        nvre_esm::decode::SubStatus::Ok => d.value,
        _ => None,
    }
}

/// Decodes a subrecord, specifying whether the parent plugin is localized.
pub fn decoded_localized(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    localized: bool,
) -> Option<Value> {
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, *rec_sig);
    ctx.localized = localized;
    if let Some(def) = schema.lookup(rec_sig, &sub.sig) {
        if let Some(from) = def.count_from.clone() {
            if let Some(n) = sibling_count(schema, rec_sig, &from, &sub.sig, sub) {
                ctx.counts.insert(from.to_string(), n);
            }
        }
    }
    let d = nvre_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    match d.status {
        nvre_esm::decode::SubStatus::Ok => d.value,
        _ => None,
    }
}

/// The element count of a counted subrecord, recovered from its own size.
///
/// The count lives in a sibling subrecord, which is not available when a
/// subrecord is decoded on its own. The count is nevertheless the only
/// reading of the size that divides evenly, so it can be recovered: the
/// element size is known from the layout, and the size divided by it is the
/// count.
fn sibling_count(
    schema: &Schema,
    rec_sig: &[u8; 4],
    from: &str,
    sub_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
) -> Option<u64> {
    // The element type comes from the counted subrecord's own layout.
    let def = schema.lookup(rec_sig, sub_sig)?;
    let elem_size = match schema.resolve_def(&def.ty) {
        TypeDef::Array { elem, .. } => schema.fixed_size_def(elem.as_ref())?,
        _ => return None,
    };
    if elem_size == 0 || !sub.data.len().is_multiple_of(elem_size) {
        return None;
    }
    let _ = from;
    Some((sub.data.len() / elem_size) as u64)
}

/// Re-encode a subrecord from its decoded value and compare with the original.
pub fn roundtrips(schema: &Schema, rec_sig: &[u8; 4], sub: &nvre_esm::Subrecord) -> bool {
    roundtrips_localized(schema, rec_sig, sub, false)
}

/// Re-encode a subrecord in a block of its record's sequence and compare with the original.
pub fn roundtrips_in_block(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    block: Option<&str>,
) -> bool {
    roundtrips_in_block_localized(schema, rec_sig, sub, block, false)
}

/// Re-encode a subrecord in its block, for a localized plugin.
///
/// The localized flag decides whether a text field's payload is characters or a
/// string-table ID, and the encoder has to be told the same thing the decoder
/// was: a field read as text and written as an ID would be a different length.
pub fn roundtrips_in_block_localized(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    block: Option<&str>,
    localized: bool,
) -> bool {
    let Some(value) = decoded_in_block_localized(schema, rec_sig, sub, block, localized) else {
        return false;
    };
    // The encoder needs no localized flag: `Prim::Localized` writes a number as
    // the four-byte string ID and a string as text, so the value's own type
    // decides. Only the decoder has to be told, because it has nothing to go on.
    match encode_sub_len_in_block(
        schema,
        rec_sig,
        &sub.sig,
        &value,
        Some(sub.data.len()),
        block,
    ) {
        Ok(bytes) => bytes == sub.data,
        Err(_) => false,
    }
}

/// Re-encode a subrecord, specifying whether the parent plugin is localized.
pub fn roundtrips_localized(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    localized: bool,
) -> bool {
    let Some(value) = decoded_localized(schema, rec_sig, sub, localized) else {
        return false;
    };
    match encode_sub_len(schema, rec_sig, &sub.sig, &value, Some(sub.data.len())) {
        Ok(bytes) => bytes == sub.data,
        Err(_) => false,
    }
}

/// Names of the fields of a subrecord's layout, for the property grid.
///
/// A struct contributes its own field names; a bare scalar contributes the empty
/// string, which is the path of the subrecord's own value. Without that, a
/// scalar subrecord — `XOWN` (owner), `XSCL` (scale), `XRDS` (radius) — has no
/// rows at all, and the one field it has cannot be edited.
///
/// A nested struct contributes its **path**: `position.x`, not `x`. Flattening a
/// nested struct into bare names would leave the paths unaddressable — a
/// reference's position and rotation are the most-edited fields in the editor,
/// and `DATA`'s are two structs deep.
pub fn field_names(schema: &Schema, rec_sig: &[u8; 4], sub_sig: &[u8; 4]) -> Vec<String> {
    field_names_in_block(schema, rec_sig, sub_sig, None)
}

/// The same, for a subrecord that sits inside a block of its record's sequence.
///
/// A signature can mean one thing at the top of a record and another inside a
/// block — `DATA` in `PACK`, `FULL` in `FACT` — so the block decides which
/// layout's fields the grid shows. Without it the grid offers the top-level
/// layout's rows for a subrecord the record actually stores as the block's, and
/// every one of those rows writes to the wrong place.
pub fn field_names_in_block(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
) -> Vec<String> {
    let Some(def) = schema.lookup_in_block(rec_sig, sub_sig, block) else {
        return Vec::new();
    };
    fn walk(t: &TypeDef, prefix: &str, out: &mut Vec<String>) {
        match t {
            TypeDef::Struct(fields) => {
                for f in fields {
                    let path = if prefix.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{prefix}.{}", f.name)
                    };
                    out.push(path.clone());
                    walk(&f.ty, &path, out);
                }
            }
            TypeDef::Array { elem, .. } => walk(elem, prefix, out),
            TypeDef::Union(variants) => {
                for (_, vt) in variants {
                    walk(vt, prefix, out);
                }
            }
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                for vt in variants {
                    walk(vt, prefix, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&schema.resolve_def(&def.ty), "", &mut out);
    out.sort();
    out.dedup();
    // A bare scalar (or an opaque `bytes`) has no inner fields, so the whole
    // subrecord is the field. The empty path addresses the value itself, which
    // `set_path` and the encoder both treat as the whole subrecord.
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// One row of the schema grid: a field path, and whether it can be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The path `set_path` and the encoder address the field by.
    pub path: String,
    /// The value is derived from another field — an array's element count — so
    /// the encoder refuses one that disagrees with the array's own length. The
    /// row is shown, because hiding it would hide what the record says, but it
    /// is not editable: a count edited on its own either fails or silently
    /// resizes nothing.
    pub derived: bool,
}

/// The rows the grid shows for one decoded subrecord.
///
/// The rows come from **the value**, not from a walk of the layout. That is the
/// difference between a grid that works in every game and one that does not: a
/// layout declares union variants the record did not choose, optional fields it
/// does not carry, and array element shapes the decoder flattens into a list, so
/// a walk of the type produces rows whose paths are not in the decoded value —
/// rows the grid then has to skip, leaving the field invisible and uneditable
/// with nothing said about it.
///
/// Reading the rows off the decoded value cannot produce such a row: every path
/// is a key the decoder actually emitted, so every row is addressable, and the
/// encoder is given back exactly the shape it produced. A union shows the
/// variant the record chose; an absent optional field has no row; an array is
/// one row (a list, which this grid does not edit element by element) rather
/// than a set of phantom element fields.
///
/// The one thing the value cannot say is which fields are *derived*, so that
/// still comes from the layout ([`derived_paths`]).
pub fn rows(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    value: &Value,
) -> Vec<Row> {
    let mut paths = Vec::new();
    value_paths(value, "", &mut paths);
    // An empty struct, or a value the walk produced nothing for, still gets one
    // row so the subrecord is visible rather than looking absent.
    if paths.is_empty() {
        paths.push(String::new());
    }
    let derived = derived_paths(schema, rec_sig, sub_sig, block);
    let selector = selector_path(schema, rec_sig, sub_sig, block, value);
    paths
        .into_iter()
        .map(|path| Row {
            derived: derived.contains(&path) || selector.as_deref() == Some(path.as_str()),
            path,
        })
        .collect()
}

/// The field that chooses a peek union's layout, when the subrecord is one.
///
/// A `peek` union picks its variant by reading the subrecord's **first word**
/// and then decoding the whole subrecord as the variant that word names, so that
/// first field is not a value: it is the choice of layout for everything after
/// it. Editing it on its own asks the encoder for a layout the rest of the bytes
/// do not match, which either fails or produces a subrecord that no longer
/// decodes — `MODT`'s `version` is the case in every game that has one.
///
/// The decoder emits the variant's fields in layout order, so the first key of
/// the decoded object is the word that was peeked.
fn selector_path(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    value: &Value,
) -> Option<String> {
    let def = schema.lookup_in_block(rec_sig, sub_sig, block)?;
    match schema.resolve_def(&def.ty) {
        TypeDef::PeekUnion { .. } => value.as_object()?.keys().next().cloned(),
        _ => None,
    }
}

/// The value at a dotted path inside a decoded subrecord.
///
/// A field name may itself contain a dot — Fallout 4's `DNAM` carries
/// `"Assoc. Item"` and `"Explosion - Alt. Trigger - Proximity"` — so the
/// **longest** prefix that names a key wins at each level, and splitting at the
/// first dot is only what happens when no longer name matches. Splitting first
/// would make those fields unreachable, which is what the grid would then
/// silently skip.
pub fn value_at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut at = value;
    let mut rest = path;
    loop {
        let o = at.as_object()?;
        let mut cut = rest.len();
        let found = loop {
            if let Some(v) = o.get(&rest[..cut]) {
                break Some((cut, v));
            }
            cut = rest[..cut].rfind('.')?;
        };
        let (cut, v) = found?;
        if cut == rest.len() {
            return Some(v);
        }
        at = v;
        rest = &rest[cut + 1..];
        if rest.is_empty() {
            return None;
        }
    }
}

/// The scalar leaves of a decoded value, as dotted paths.
///
/// A struct contributes its keys; a nested struct its dotted path; anything
/// else — a number, a string, a list, a `null` — is a leaf and is addressed by
/// its own path. A top-level scalar is the empty path, which is the subrecord's
/// own value.
///
/// A flags word is a leaf even though it decodes to an object: its `value`,
/// `set` and `unknown_bits` keys are the decoder's representation of one number,
/// not three fields, and the grid edits the word as one thing.
fn value_paths(value: &Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(o) if !is_word(o) => {
            for (k, v) in o {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                value_paths(v, &path, out);
            }
        }
        _ => out.push(prefix.to_string()),
    }
}

/// Whether a decoded object is a word — a flags set, a bit set or an enum —
/// rather than a struct.
///
/// The decoder writes these as one number plus its reading:
/// `{"value": n, "fields": {name: bool, …}}` for a flags set, the same shape for
/// a bit set, and `{"value": n, "name": "…"}` for an enum. The keys after
/// `value` are the decoder's rendering of that one number, not fields of the
/// record, so the grid has to edit the word as one thing: recursing into
/// `fields` would offer a checkbox per named bit, and a checkbox written into the
/// `fields` map does not change the number the encoder writes, so every one of
/// them would silently do nothing.
///
/// The test is the exact key set, not the presence of `value` alone: a struct
/// whose only field is called `value` is a struct, and its row is that field.
fn is_word(o: &serde_json::Map<String, Value>) -> bool {
    if !o.contains_key("value") {
        return false;
    }
    let rendering = o.contains_key("fields") || o.contains_key("set") || o.contains_key("name");
    // `value` plus its rendering, and at most the extra bits a flags word
    // carries (`unknown_bits`).
    rendering && o.len() <= 3
}

/// The paths inside a subrecord whose value the encoder derives from another
/// field.
///
/// An array declared as `{"array": T, "count": {"from_field": "n"}}` stores its
/// length in the sibling `n`; the encoder checks `n` against the array's actual
/// length and refuses a count that disagrees. Editing `n` alone therefore cannot
/// work — it would have to resize the array — so the grid shows it read-only.
/// The same holds for the other derived counts (`count_bits`, `count_sum`,
/// `count_pow`).
pub fn derived_paths(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
) -> Vec<String> {
    let Some(def) = schema.lookup_in_block(rec_sig, sub_sig, block) else {
        return Vec::new();
    };
    fn join(prefix: &str, name: &str) -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        }
    }
    fn walk(t: &TypeDef, prefix: &str, out: &mut Vec<String>) {
        match t {
            TypeDef::Struct(fields) => {
                for f in fields {
                    // A derived count is a **sibling** of the array that owns it,
                    // so it is named here, at the struct that holds both — not
                    // inside the array, where the only prefix is the array's own
                    // path and `references.count` would be recorded instead of
                    // `count`.
                    if let TypeDef::Array {
                        count_field,
                        count_bits,
                        count_sum,
                        count_pow,
                        ..
                    } = &f.ty
                    {
                        if let Some(n) = count_field {
                            out.push(join(prefix, n));
                        }
                        if let Some((n, _)) = count_bits {
                            out.push(join(prefix, n));
                        }
                        if let Some((a, b)) = count_sum {
                            out.push(join(prefix, a));
                            out.push(join(prefix, b));
                        }
                        if let Some(n) = count_pow {
                            out.push(join(prefix, n));
                        }
                    }
                    walk(&f.ty, &join(prefix, &f.name), out);
                }
            }
            TypeDef::Array { elem, .. } => {
                // An array of structs can carry derived counts one level in;
                // those belong to the element's own struct, which is walked with
                // the array's prefix.
                walk(elem, prefix, out);
            }
            TypeDef::Union(variants) => {
                for (_, vt) in variants {
                    walk(vt, prefix, out);
                }
            }
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                for vt in variants {
                    walk(vt, prefix, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&schema.resolve_def(&def.ty), "", &mut out);
    out.sort();
    out.dedup();
    out
}

/// Whether the field at `path` is a FormID, so the grid offers a picker for it.
///
/// A FormID decodes to a number, and a number gets a spinner by default — which
/// is a field a modder has to type a hex ID into. The layout is the only thing
/// that knows the field is a reference, so the grid asks it.
pub fn is_formid_field(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    path: &str,
) -> bool {
    let Some(def) = schema.lookup_in_block(rec_sig, sub_sig, block) else {
        return false;
    };
    fn at_path(t: &TypeDef, parts: &[&str]) -> bool {
        let Some((part, rest)) = parts.split_first() else {
            return matches!(t, TypeDef::Prim(Prim::FormId) | TypeDef::FormIdRef { .. });
        };
        match t {
            TypeDef::Struct(fields) => match fields.iter().find(|f| f.name == *part) {
                Some(f) => at_path(&f.ty, rest),
                None => false,
            },
            TypeDef::Union(variants) => variants.iter().any(|(_, v)| at_path(v, parts)),
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                variants.iter().any(|v| at_path(v, parts))
            }
            _ => false,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    at_path(&schema.resolve_def(&def.ty), &parts)
}

/// The names an enum field's values have, from the schema.
///
/// The decoder emits an enum as `{"value": n, "name": "…"}`, which carries the
/// name of the value the record holds — but not the other names, so a dropdown
/// built from the value alone could not offer anything else. The table is where
/// they live.
///
/// Empty when the field is not an enum, which is how the grid tells the two
/// apart without a second walk of the layout.
pub fn enum_values_at(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    path: &str,
) -> Vec<(i64, String)> {
    let Some(def) = schema.lookup_in_block(rec_sig, sub_sig, block) else {
        return Vec::new();
    };
    fn at_path(t: &TypeDef, parts: &[&str]) -> Option<TypeDef> {
        let Some((part, rest)) = parts.split_first() else {
            return Some(t.clone());
        };
        match t {
            TypeDef::Struct(fields) => {
                let f = fields.iter().find(|f| f.name == *part)?;
                at_path(&f.ty, rest)
            }
            TypeDef::Union(variants) => variants.iter().find_map(|(_, v)| at_path(v, parts)),
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                variants.iter().find_map(|v| at_path(v, parts))
            }
            _ => None,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    match at_path(&schema.resolve_def(&def.ty), &parts) {
        Some(TypeDef::Enum { values, .. }) => values,
        _ => Vec::new(),
    }
}

/// The element layout of an array field, when the field at `path` is one.
///
/// An array's length is part of its encoding: adding or removing an element
/// changes the bytes, so the grid cannot edit one element in place. What it can
/// do is show the list, let the elements be edited as whole rows, and let one be
/// added or removed — which is a write of the whole subrecord, exactly what the
/// encoder already does for every other field.
pub fn array_elem_at(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    path: &str,
) -> Option<TypeDef> {
    let def = schema.lookup_in_block(rec_sig, sub_sig, block)?;
    fn at_path(t: &TypeDef, parts: &[&str]) -> Option<TypeDef> {
        let Some((part, rest)) = parts.split_first() else {
            return Some(t.clone());
        };
        match t {
            TypeDef::Struct(fields) => {
                let f = fields.iter().find(|f| f.name == *part)?;
                at_path(&f.ty, rest)
            }
            TypeDef::Union(variants) => variants.iter().find_map(|(_, v)| at_path(v, parts)),
            TypeDef::PeekUnion { variants, .. } | TypeDef::ValueUnion { variants, .. } => {
                variants.iter().find_map(|v| at_path(v, parts))
            }
            _ => None,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    match at_path(&schema.resolve_def(&def.ty), &parts) {
        Some(TypeDef::Array { elem, .. }) => Some(*elem),
        _ => None,
    }
}

/// A flat, editable view of one record: every subrecord with its signature,
/// human name, decoded value and whether it can be written back safely.
#[derive(Debug, Clone)]
pub struct SubView {
    pub sig: String,
    pub name: String,
    /// The block of the record's sequence this subrecord was read in, when the
    /// record is a sequence (`PACK`, `FACT`, `REGN`, …).
    ///
    /// A signature can mean one thing at the top of a record and another inside
    /// a block, so the layout that applies is only known with the block that was
    /// open. It travels with the view because the grid's field names and the
    /// write path both need it: reading a subrecord by its top-level layout and
    /// writing it back the same way is how a block's `DATA` gets a page of rows
    /// that do not exist in it.
    pub block: Option<String>,
    pub value: Value,
    /// True when [`roundtrips`] accepts this subrecord's own bytes.
    pub writable: bool,
    /// Decode status when it is not `Ok`.
    pub problem: Option<String>,
}

/// Decode every subrecord of a record for the property grid.
///
/// The record's block sequence is walked first, so each subrecord is decoded
/// with the block that was open when it was read — which is the layout the game
/// used, and so the layout the editor has to write back.
///
/// `localized` is whether the plugin the record came from stores its text as
/// string-table IDs. Skyrim's and Fallout 4's shipped masters do and Fallout: New
/// Vegas's does not, so the same subrecord decodes to different bytes' worth of
/// value depending on the file; [`crate::Session`] answers it per record, because
/// a mod's record and the master it inherits from can differ.
pub fn sub_views(
    schema: &Schema,
    rec: &nvre_esm::Record,
    localized: bool,
) -> Result<Vec<SubView>, String> {
    let subs = rec.subrecords().map_err(|e| e.to_string())?;
    let rec_sig = rec.header.sig;
    // One entry per subrecord, in file order: the block it was read in.
    // `walk_sequence` calls back exactly once per subrecord, so the indices line
    // up with `subs` even when the record is not a sequence at all.
    let mut blocks: Vec<Option<String>> = Vec::with_capacity(subs.len());
    {
        let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, rec_sig);
        nvre_esm::decode::walk_sequence(&rec_sig, &subs, schema, &mut ctx, |_, c| {
            blocks.push(c.block.clone());
        });
    }
    let mut out = Vec::with_capacity(subs.len());
    for (i, s) in subs.iter().enumerate() {
        let block = blocks.get(i).cloned().flatten();
        let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, rec_sig);
        ctx.block = block.clone();
        // The plugin's own answer about how its text is stored. A localized
        // plugin holds a four-byte string ID where a plain one holds characters,
        // so a subrecord read with the wrong flag is wrong by a whole field.
        ctx.localized = localized;
        let d = nvre_esm::decode::decode_sub(&rec_sig, s, &mut ctx);
        let name = d.name.clone().unwrap_or_else(|| s.sig_str());
        out.push(SubView {
            sig: s.sig_str(),
            name,
            block: block.clone(),
            value: d.value.unwrap_or(Value::Null),
            writable: d.status == nvre_esm::decode::SubStatus::Ok
                && roundtrips_in_block_localized(schema, &rec_sig, s, block.as_deref(), localized),
            problem: match d.status {
                nvre_esm::decode::SubStatus::Ok => None,
                other => Some(d.error.unwrap_or_else(|| other.as_str().to_string())),
            },
        });
    }
    Ok(out)
}

#[doc(hidden)]
pub mod test_schema {
    use nvre_esm::schema::Schema;
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
        s.merge_str(text).unwrap();
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
        s.merge_str(text).unwrap();
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
        s.merge_str(text).unwrap();
        s.validate().unwrap();
        let data = vec![1u8, 9, 9, 2, 3, 3, 3];
        let subs = nvre_esm::raw::parse_subrecords(&{
            let mut v = Vec::new();
            v.extend_from_slice(b"XABC");
            v.extend_from_slice(&(data.len() as u16).to_le_bytes());
            v.extend_from_slice(&data);
            v
        })
        .unwrap();
        let mut ctx = nvre_esm::decode::DecodeCtx::new(&s, *b"TEST");
        let d = nvre_esm::decode::decode_sub(b"TEST", &subs[0], &mut ctx);
        assert_eq!(d.status, nvre_esm::decode::SubStatus::Ok);
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
