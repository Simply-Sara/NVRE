//! Reading a record's fields, and resolving a FormID to the record that wins.
//!
//! A record's subrecords are decoded against the schema, and the decoded
//! values are picked out by subrecord signature and field name.
//!
//! A FormID in a record is *file-local*: its top byte indexes the file's own
//! master list. So a FormID read out of a record has to be mapped through
//! the load order before it means anything, and that is what [`Fields`]
//! remembers the plugin for.

use nvre_esm::decode::{decode_sub, DecodeCtx, SubStatus};
use nvre_esm::{EsmError, LoadOrder, Record, Schema, Subrecord};
use serde_json::Value;

/// One record's subrecords, and the plugin that holds it.
pub struct Fields<'a> {
    /// The record's type.
    pub sig: [u8; 4],
    /// The plugin's index in the load order.
    pub plugin: usize,
    /// The record's flags.
    pub flags: u32,
    /// The record's subrecords.
    pub subs: Vec<Subrecord>,
    schema: &'a Schema,
}

impl<'a> Fields<'a> {
    /// Read a record's subrecords.
    pub fn new(r: &Record, plugin: usize, schema: &'a Schema) -> Result<Fields<'a>, EsmError> {
        Ok(Fields {
            sig: r.header.sig,
            plugin,
            flags: r.header.flags,
            subs: r.subrecords().unwrap_or_default(),
            schema,
        })
    }

    /// The first subrecord with this signature.
    pub fn raw(&self, sig: &[u8; 4]) -> Option<&Subrecord> {
        self.subs.iter().find(|s| &s.sig == sig)
    }

    /// Decode one subrecord; `None` unless it decodes exactly.
    pub fn decode(&self, s: &Subrecord) -> Option<Value> {
        let mut ctx = DecodeCtx::new(self.schema, self.sig);
        let d = decode_sub(&self.sig, s, &mut ctx);
        match d.status {
            SubStatus::Ok => d.value,
            _ => None,
        }
    }

    /// The first subrecord with this signature, decoded.
    pub fn get(&self, sig: &[u8; 4]) -> Option<Value> {
        self.raw(sig).and_then(|s| self.decode(s))
    }

    /// The first subrecord with this signature, as text.
    pub fn string(&self, sig: &[u8; 4]) -> Option<String> {
        match self.get(sig)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// A FormID-valued subrecord, mapped to the load order.
    pub fn formid(&self, sig: &[u8; 4], lo: &LoadOrder) -> Option<u32> {
        global(lo, self.plugin, formid_of(&self.get(sig)?)?)
    }
}

/// The FormID a decoded value holds.
pub fn formid_of(v: &Value) -> Option<u32> {
    let s = v.as_str()?;
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// Map a file-local FormID to a global one. The null reference has no
/// global.
pub fn global(lo: &LoadOrder, plugin: usize, local: u32) -> Option<u32> {
    if local == 0 {
        return None;
    }
    lo.to_global(plugin, local)
}

/// A number a decoded value holds.
pub fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64()
}

/// A three-component vector a decoded value holds.
pub fn vec3(v: &Value) -> Option<[f32; 3]> {
    Some([
        num(v, "x")? as f32,
        num(v, "y")? as f32,
        num(v, "z")? as f32,
    ])
}

/// The winning definition of a global FormID, and the plugin that holds it.
pub fn winner<'a>(lo: &LoadOrder<'a>, global: u32) -> Option<(usize, &'a Record)> {
    let (plugin, record) = lo.winner_record(global)?;
    let pi = lo
        .plugins
        .iter()
        .position(|p| std::ptr::eq(p, plugin))
        .unwrap_or(0);
    Some((pi, record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_formid_is_read_from_its_text() {
        assert_eq!(formid_of(&json!("0x0001F4A2")), Some(0x1F4A2));
        assert_eq!(formid_of(&json!("1F4A2")), None);
        assert_eq!(formid_of(&json!(5)), None);
    }

    #[test]
    fn a_vector_is_read_from_its_parts() {
        assert_eq!(
            vec3(&json!({"x": 1.0, "y": -2, "z": 3.5})),
            Some([1.0, -2.0, 3.5])
        );
        assert_eq!(vec3(&json!({"x": 1.0})), None);
    }
}
