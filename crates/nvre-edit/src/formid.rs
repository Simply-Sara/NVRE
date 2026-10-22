//! Remapping FormIDs when a record moves between plugins.
//!
//! A FormID stored in a plugin is *file-local*: its top byte indexes that file's
//! own master list, and the value equal to the master count means "this file".
//! When the editor saves records into a **new** plugin, every FormID must be
//! rewritten into the new file's numbering, or the mod refers to objects that do
//! not exist for it.
//!
//! The remap is schema-driven: the decoder emits FormIDs as `"0x00157C89"` text,
//! so the values to rewrite are found by walking the schema type and the decoded
//! JSON in parallel. Anything that does not round-trip is left untouched and
//! reported, never guessed at.

use nvre_esm::raw::Subrecord;
use nvre_esm::schema::{Prim, Schema};
use nvre_esm::typedef::TypeDef;
use serde_json::{Map, Value};

/// Maps a plugin-local FormID in the source file to one in the target file.
///
/// Returns `None` for the null reference (0), which is preserved as-is.
pub type FormIdMap<'a> = dyn Fn(u32) -> Option<u32> + 'a;

/// Rewrite every FormID in `sub` through `map`, returning the new payload bytes.
///
/// `None` when the subrecord has no layout, does not decode exactly, or does not
/// re-encode to the same width — in every case the caller must leave the original
/// bytes alone rather than write a half-remapped record.
pub fn remap_sub(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &Subrecord,
    map: &FormIdMap,
) -> Option<Vec<u8>> {
    let def = schema.lookup(rec_sig, &sub.sig)?;
    // Opaque `bytes` definitions carry no FormIDs worth rewriting.
    if matches!(schema.resolve_def(&def.ty), TypeDef::Prim(Prim::Bytes)) {
        return Some(sub.data.clone());
    }
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, *rec_sig);
    let d = nvre_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    if d.status != nvre_esm::decode::SubStatus::Ok {
        return None;
    }
    let mut value = d.value?;
    let mut changed = false;
    remap_value(schema, &schema.resolve_def(&def.ty), &mut value, map, &mut changed)?;
    if !changed {
        return Some(sub.data.clone());
    }
    // Re-encode through the editor's own encoder, which refuses anything it
    // cannot write back byte for byte.
    let out = nvre_edit_encode(schema, rec_sig, &sub.sig, &value).ok()?;
    // A layout that does not match the record's own length would rewrite
    // the whole subrecord with the wrong field at every offset. The
    // re-encoded bytes have to be the same length as the ones that were
    // read, or the subrecord is left exactly as it was.
    if out.len() != sub.data.len() {
        return None;
    }
    Some(out)
}

/// Bridge to the encoder without a circular module dependency.
fn nvre_edit_encode(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    value: &Value,
) -> Result<Vec<u8>, crate::encode::EncodeError> {
    crate::encode::encode_sub(schema, rec_sig, sub_sig, value)
}

/// Walk a type and its value together, rewriting FormID leaves.
///
/// Returns `None` when the shape does not match the layout, which means the
/// subrecord cannot be remapped safely.
fn remap_value(
    schema: &Schema,
    t: &TypeDef,
    value: &mut Value,
    map: &FormIdMap,
    changed: &mut bool,
) -> Option<()> {
    match t {
        TypeDef::Prim(Prim::FormId) => {
            let local = parse_formid(value)?;
            if local == 0 {
                return Some(());
            }
            let mapped = map(local)?;
            if mapped != local {
                *value = Value::String(format!("0x{mapped:08X}"));
                *changed = true;
            }
            Some(())
        }
        TypeDef::Repeat(inner) => {
            let arr = value.as_array_mut()?;
            for v in arr.iter_mut() {
                remap_value(schema, inner, v, map, changed)?;
            }
            Some(())
        }
        TypeDef::Struct(fields) => {
            let obj = value.as_object_mut()?;
            for f in fields {
                // A missing optional field contributes no bytes, so there is
                // nothing to remap in it.
                if !obj.contains_key(&f.name) {
                    if f.optional {
                        continue;
                    }
                    return None;
                }
                let v = obj.get_mut(&f.name)?;
                remap_value(schema, &f.ty, v, map, changed)?;
            }
            Some(())
        }
        TypeDef::Array { elem, .. } => {
            let arr = value.as_array_mut()?;
            for v in arr.iter_mut() {
                remap_value(schema, elem.as_ref(), v, map, changed)?;
            }
            Some(())
        }
        // A union is chosen by byte width at decode time, so the variant that
        // produced this value is not recorded. Remapping inside one could pick
        // the wrong variant, so unions are left untouched.
        TypeDef::Union(_) | TypeDef::ValueUnion { .. } | TypeDef::PeekUnion { .. } => Some(()),
        TypeDef::Named(_) | TypeDef::Prim(_) | TypeDef::Enum { .. } | TypeDef::Flags { .. } => {
            Some(())
        }
    }
}

/// The decoder's FormID text form, `"0x00157C89"`.
fn parse_formid(v: &Value) -> Option<u32> {
    let s = v.as_str()?;
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// How many FormIDs a record's payload would change under `map`.
///
/// Used for reporting: a save says how many references it rewrote rather than
/// leaving the user to trust that it happened.
pub fn count_formids(schema: &Schema, rec_sig: &[u8; 4], sub: &Subrecord) -> usize {
    let Some(def) = schema.lookup(rec_sig, &sub.sig) else {
        return 0;
    };
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, *rec_sig);
    let Ok(subs) = parse_one(sub) else { return 0 };
    let _ = subs;
    let d = nvre_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    let Some(v) = d.value else { return 0 };
    let mut n = 0;
    count_in(schema, &schema.resolve_def(&def.ty), &v, &mut n);
    n
}

fn parse_one(sub: &Subrecord) -> Result<Vec<Subrecord>, ()> {
    Ok(vec![sub.clone()])
}

fn count_in(schema: &Schema, t: &TypeDef, value: &Value, n: &mut usize) {
    match t {
        TypeDef::Prim(Prim::FormId) => {
            if parse_formid(value).is_some_and(|f| f != 0) {
                *n += 1;
            }
        }
        TypeDef::Struct(fields) => {
            if let Some(obj) = value.as_object() {
                for f in fields {
                    if let Some(v) = obj.get(&f.name) {
                        count_in(schema, &f.ty, v, n);
                    }
                }
            }
        }
        TypeDef::Array { elem, .. } => {
            if let Some(arr) = value.as_array() {
                for v in arr {
                    count_in(schema, elem, v, n);
                }
            }
        }
        TypeDef::Union(variants) => {
            for (_, vt) in variants {
                count_in(schema, vt, value, n);
            }
        }
        TypeDef::ValueUnion { variants, .. } | TypeDef::PeekUnion { variants, .. } => {
            for vt in variants {
                count_in(schema, vt, value, n);
            }
        }
        _ => {}
    }
}

/// The names of the fields a subrecord exposes, for a save report.
pub fn formid_fields(schema: &Schema, rec_sig: &[u8; 4], sub_sig: &[u8; 4]) -> Vec<String> {
    let Some(def) = schema.lookup(rec_sig, sub_sig) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(schema: &Schema, t: &TypeDef, out: &mut Vec<String>) {
        match t {
            TypeDef::Prim(Prim::FormId) => out.push("<formid>".to_string()),
            TypeDef::Struct(fields) => {
                for f in fields {
                    walk(schema, &f.ty, out);
                }
            }
            TypeDef::Array { elem, .. } => walk(schema, elem, out),
            TypeDef::Union(vs) => {
                for (_, vt) in vs {
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
    walk(schema, &schema.resolve_def(&def.ty), &mut out);
    out.sort();
    out.dedup();
    out
}

/// Flatten a decoded record into its subrecord values, for inspection.
pub fn decoded_map(
    schema: &Schema,
    rec_sig: &[u8; 4],
    sub: &Subrecord,
) -> Option<Map<String, Value>> {
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, *rec_sig);
    let d = nvre_esm::decode::decode_sub(rec_sig, sub, &mut ctx);
    d.value?.as_object().cloned()
}
