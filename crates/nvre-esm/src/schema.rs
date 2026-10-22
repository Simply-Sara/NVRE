//! The record layouts, loaded from JSON at runtime.
//!
//! A record's layout says what its subrecords are and what each one holds.
//! The schema is that, as data, so a wrong layout is fixed by editing a
//! file rather than by editing code — and so several games' layouts can sit
//! side by side.
//!
//! # The shape of a schema file
//!
//! ```json
//! {
//!   "format": "nvre.esm.schema/1",
//!   "records": {
//!     "ACHR": {
//!       "name": "Placed NPC",
//!       "subrecords": {
//!         "NAME": {"type": "formid"},
//!         "DATA": {"type": "PosRot"}
//!       }
//!     }
//!   },
//!   "types": {
//!     "PosRot": {"struct": [
//!       {"name": "x", "type": "f32"},
//!       {"name": "y", "type": "f32"},
//!       {"name": "z", "type": "f32"}
//!     ]}
//!   },
//!   "common": {
//!     "Empty": {"type": "u8"}
//!   }
//! }
//! ```
//!
//! A subrecord's `type` is either the name of a type, or one of:
//!
//! - a **primitive**: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
//!   `f32`, `bool`, `formid`, `zstring`, `string`, `bytes`;
//! - an **object**, written inline: `{"enum": "u32", "values": {...}}` for a
//!   set of names, `{"bits": {...}}` for a set of bit fields, or
//!   `{"struct": [...]}` for a layout.
//!
//! A field may also be:
//!
//! - `optional` — the subrecord may be absent, and its absence is not an
//!   error;
//! - `count_from` — the name of a subrecord whose value is how many
//!   elements this one holds;
//! - `expose` — the field is shown in the editor's inspector.
//!
//! # Layers
//!
//! A schema is loaded from a base and any number of layers over it. A layer
//! may replace a record or a type outright, or add to one. That is how a
//! game's own layouts sit on top of a shared base.

use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The format tag a schema file carries.
pub const FORMAT: &str = "nvre.esm.schema/1";

/// The format tag the schemas in `schemas/` carry.
///
/// Those files were written for the reader this crate replaces, and they are
/// NVRE's own data, so this reader reads them as they are.
pub const LEGACY_FORMAT: &str = "fnv.schema/1";

/// A primitive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prim {
    /// An unsigned byte.
    U8,
    /// A signed byte.
    I8,
    /// An unsigned 16-bit number.
    U16,
    /// A signed 16-bit number.
    I16,
    /// An unsigned 32-bit number.
    U32,
    /// A signed 32-bit number.
    I32,
    /// An unsigned 64-bit number.
    U64,
    /// A signed 64-bit number.
    I64,
    /// A 32-bit float.
    F32,
    /// A byte holding a flag.
    Bool,
    /// A FormID: a 32-bit reference to a record.
    FormId,
    /// A NUL-terminated string, in Windows-1252.
    ZString,
    /// A string of a length byte and that many bytes.
    String,
    /// Bytes of a length the field gives.
    Bytes,
    /// A NUL-terminated string, in Windows-1252.
    CString,
    /// A string of a one-byte length and that many bytes.
    LenString,
    /// An array of NUL-terminated strings, each with a one-byte length.
    ZStringArray,
}

impl Prim {
    /// The primitive a name names, if it names one.
    pub fn parse(s: &str) -> Option<Prim> {
        Some(match s {
            "u8" => Prim::U8,
            "i8" => Prim::I8,
            "u16" => Prim::U16,
            "i16" => Prim::I16,
            "u32" => Prim::U32,
            "i32" => Prim::I32,
            "u64" => Prim::U64,
            "i64" => Prim::I64,
            "f32" => Prim::F32,
            "bool" => Prim::Bool,
            "formid" => Prim::FormId,
            "zstring" => Prim::ZString,
            "string" => Prim::String,
            "bytes" => Prim::Bytes,
            "cstring" => Prim::CString,
            "len_string" => Prim::LenString,
            "zstring_array" => Prim::ZStringArray,
            _ => return None,
        })
    }

    /// The primitive's name, as the schema writes it.
    pub fn name(self) -> &'static str {
        match self {
            Prim::U8 => "u8",
            Prim::I8 => "i8",
            Prim::U16 => "u16",
            Prim::I16 => "i16",
            Prim::U32 => "u32",
            Prim::I32 => "i32",
            Prim::U64 => "u64",
            Prim::I64 => "i64",
            Prim::F32 => "f32",
            Prim::Bool => "bool",
            Prim::FormId => "formid",
            Prim::ZString => "zstring",
            Prim::String => "string",
            Prim::Bytes => "bytes",
            Prim::CString => "cstring",
            Prim::LenString => "len_string",
            Prim::ZStringArray => "zstring_array",
        }
    }

    /// How many bytes the primitive holds, when it is a fixed size.
    pub fn size(self) -> Option<usize> {
        Some(match self {
            Prim::U8 | Prim::I8 | Prim::Bool => 1,
            Prim::U16 | Prim::I16 => 2,
            Prim::U32 | Prim::I32 | Prim::F32 | Prim::FormId => 4,
            Prim::U64 | Prim::I64 => 8,
            Prim::ZString
            | Prim::String
            | Prim::Bytes
            | Prim::CString
            | Prim::LenString
            | Prim::ZStringArray => return None,
        })
    }
}

/// What a field holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// A primitive.
    Prim(Prim),
    /// A named type: a struct, an enum, or a common type.
    Named(String),
    /// A set of names for the integers a field can hold.
    Enum {
        /// The type the value is stored as.
        storage: Prim,
        /// The names, by value.
        values: BTreeMap<i64, String>,
    },
    /// A set of bit fields.
    Bits {
        /// The type the value is stored as.
        storage: Prim,
        /// The fields: a name, the first bit and the last bit.
        bits: Vec<BitField>,
    },
    /// A layout, written inline.
    Inline(Vec<Field>),
    /// A set of variants, one of which is chosen by its size.
    Union(Vec<Variant>),
    /// A set of variants, one of which is chosen by another field's value.
    ValueUnion(ValueUnion),
    /// An array: a count, and what the elements are.
    Array {
        /// What the elements are.
        of: Box<Type>,
        /// How many elements there are.
        count: ArrayCount,
    },
}

/// How many elements an array has.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayCount {
    /// A fixed count.
    Fixed(usize),
    /// The count is a field of the same struct, read before this one.
    FromField(String),
    /// The count is in the data: a length before the elements, in the type
    /// the schema names.
    Prefix(Prim),
    /// There is no count: the elements fill what is left.
    Rest,
}

/// One variant of a union: a type, and the size it occupies.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    /// The size the variant occupies, when the schema says.
    pub size: Option<usize>,
    /// The variant's type.
    pub ty: Type,
}

/// A bit field of an enum.
#[derive(Debug, Clone, PartialEq)]
pub struct BitField {
    /// The field's name.
    pub name: String,
    /// The first bit.
    pub first: u32,
    /// The last bit, inclusive.
    pub last: u32,
}

impl BitField {
    /// The field's value in a stored value.
    pub fn value(&self, stored: i64) -> i64 {
        let width = (self.last - self.first + 1) as u32;
        if width >= 64 {
            return stored;
        }
        (stored >> self.first) & ((1i64 << width) - 1)
    }
}

/// One field of a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field's name.
    pub name: String,
    /// What the field holds.
    pub ty: Type,
    /// The field is optional: it may be absent.
    pub optional: bool,
    /// The field holds as many elements as another field's value says.
    pub count_from: Option<String>,
    /// The field is shown in the editor's inspector.
    pub expose: bool,
    /// The field's value is published, so a union in the same struct may
    /// be read against it.
    ///
    /// The `PERK` record's `PRKE` holds the type of the effect, and the
    /// `DATA` beside it is one thing or another depending on it.
    pub publish: bool,
}

/// A named type: a struct, or a set of names.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    /// The type's layout, when it is a struct.
    pub fields: Vec<Field>,
    /// The type's names, when it is an enum.
    pub values: BTreeMap<i64, String>,
    /// The type's bit fields, when it is one.
    pub bits: Vec<BitField>,
    /// The type the enum's values are stored as.
    pub storage: Prim,
    /// The type's variants, when it is a union chosen by the size of what
    /// is left.
    pub union: Vec<Variant>,
    /// The union chosen by the value of another field, when it is one.
    pub value_union: Option<ValueUnion>,
    /// The union chosen by the field's first word, when it is one.
    ///
    /// The word is read without consuming it: it belongs to the field either
    /// way, so the whole field is the union.
    pub peek_union: Option<PeekUnion>,
}

/// A union whose variant is chosen by the field's first word.
#[derive(Debug, Clone, PartialEq)]
pub struct PeekUnion {
    /// The word each variant is chosen by.
    pub cases: BTreeMap<i64, usize>,
    /// The variant to use when no case matches.
    pub default: usize,
    /// The variants.
    pub variants: Vec<Variant>,
}

/// A union whose variant is chosen by another field's value.
///
/// The decider is a field of the same struct, read before the union: the
/// `PERK` record's `DATA` is one thing or another depending on the `PRKE`
/// beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueUnion {
    /// The field whose value chooses the variant.
    pub switch_on: String,
    /// The variant each value chooses, by the value.
    pub cases: BTreeMap<i64, usize>,
    /// The variant to use when no case matches.
    pub default: usize,
    /// The variants.
    pub variants: Vec<Variant>,
}

/// A record's layout: its subrecords, in the order they appear.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecordDef {
    /// The record's name, for the editor.
    pub name: String,
    /// The subrecords, by their four-byte type.
    pub subrecords: BTreeMap<String, SubDef>,
    /// The subrecords whose signature is only partly known, by the suffix
    /// their signature ends with.
    ///
    /// A weather record's texture layers are named `00TX`, `10TX`, `20TX`
    /// and on, so the schema gives the suffix `0TX` and the signature is
    /// matched against its trailing bytes.
    pub wildcards: Vec<(Vec<u8>, SubDef)>,
    /// The order the subrecords appear in, when the schema says.
    pub order: Vec<String>,
    /// The record's sequence, when it is one.
    pub sequence: Option<SequenceDef>,
}

/// A record that is an ordered list of blocks.
///
/// A sequence record's subrecords come in groups. Each group is opened by a
/// signature, and the signatures that follow it are the group's contents, in
/// the order the schema gives.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SequenceDef {
    /// The subrecords that come before the first block.
    pub start: Vec<String>,
    /// The subrecords that come after the last block.
    pub end: Vec<String>,
    /// The blocks, by the name the schema gives them.
    pub blocks: BTreeMap<String, BlockDef>,
}

/// One block of a sequence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDef {
    /// The signature that opens the block.
    pub opener: String,
    /// The signatures the block holds, in order.
    pub order: Vec<String>,
    /// The block's subrecords.
    pub subrecords: BTreeMap<String, SubDef>,
    /// The block's variants: a subrecord's layout may depend on a value.
    pub variants: BTreeMap<String, VariantDef>,
}

/// A variant of a subrecord: a layout chosen by a value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VariantDef {
    /// The subrecords the variant holds.
    pub subrecords: BTreeMap<String, SubDef>,
}

/// One subrecord of a record.
#[derive(Debug, Clone, PartialEq)]
pub struct SubDef {
    /// The subrecord's name, for the editor.
    pub name: String,
    /// What the subrecord holds.
    pub ty: Type,
    /// The subrecord may be absent.
    pub optional: bool,
    /// The subrecord holds as many elements as another subrecord's value says.
    pub count_from: Option<String>,
    /// The subrecord is shown in the editor's inspector.
    pub expose: bool,
    /// The subrecord's value is published, so a union beside it may be read
    /// against it.
    pub publish: bool,
}

/// A fault in a schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SchemaError(pub String);

/// The whole schema: every record and every type.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// The records, by their four-byte type.
    pub records: BTreeMap<String, RecordDef>,
    /// The types, by name.
    pub types: BTreeMap<String, TypeDef>,
    /// The common types: the ones a subrecord can use without naming a
    /// record.
    pub common: BTreeMap<String, TypeDef>,
    /// The common subrecords: the ones any record may carry, such as the
    /// editor ID.
    pub common_subrecords: BTreeMap<String, SubDef>,
}

impl Schema {
    /// A schema with nothing in it.
    pub fn new() -> Schema {
        Schema::default()
    }

    /// Read a schema from a directory of JSON files.
    ///
    /// A directory that holds a `base` entry which resolves outside itself —
    /// the symlink a game directory carries — is a game directory: the base
    /// is read first, and the directory's own files are laid over it.
    pub fn load_dir(dir: &Path) -> Result<Schema, SchemaError> {
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
            None => Schema::new(),
        };
        s.load_layer(dir)?;
        s.check()?;
        Ok(s)
    }

    /// Read a schema from a base directory and any number of layers over it.
    pub fn load_game(base: &Path, layers: &[String]) -> Result<Schema, SchemaError> {
        let mut s = Schema::load_dir(base)?;
        for l in layers {
            s.load_layer(Path::new(l))?;
        }
        s.check()?;
        Ok(s)
    }

    /// Read one layer of JSON files into this schema.
    pub fn load_layer(&mut self, dir: &Path) -> Result<(), SchemaError> {
        let mut files: Vec<PathBuf> = Vec::new();
        let rd =
            std::fs::read_dir(dir).map_err(|e| SchemaError(format!("{}: {e}", dir.display())))?;
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                files.push(p);
            }
        }
        files.sort();
        for f in files {
            let text = std::fs::read_to_string(&f)
                .map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
            let j: Json = serde_json::from_str(&text)
                .map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
            self.merge(&j)
                .map_err(|e| SchemaError(format!("{}: {e}", f.display())))?;
        }
        Ok(())
    }

    /// Merge one schema document into this schema.
    pub fn merge(&mut self, j: &Json) -> Result<(), String> {
        let Some(obj) = j.as_object() else {
            return Err("a schema file is not an object".into());
        };
        if let Some(f) = obj.get("format").and_then(|v| v.as_str()) {
            if f != FORMAT && f != LEGACY_FORMAT {
                return Err(format!("unknown schema format {f:?}"));
            }
        }
        if let Some(records) = obj.get("records").and_then(|v| v.as_object()) {
            for (name, def) in records {
                let mut r = self.parse_record(name, def)?;
                // A record that says "merge" *adds to* the one it shadows
                // rather than replacing it: its own subrecords are added or
                // replace their counterparts, and the shadowed record's
                // other subrecords stay. That is what a game needs when
                // only some of a record's subrecords differ.
                let merges = def.get("merge").and_then(|v| v.as_bool()).unwrap_or(false);
                if merges {
                    if let Some(prev) = self.records.get(name) {
                        for (sig, sd) in &prev.subrecords {
                            r.subrecords
                                .entry(sig.clone())
                                .or_insert_with(|| sd.clone());
                        }
                        for sig in &prev.order {
                            if !r.order.contains(sig) {
                                r.order.push(sig.clone());
                            }
                        }
                    }
                }
                self.records.insert(name.clone(), r);
            }
        }
        if let Some(types) = obj.get("types").and_then(|v| v.as_object()) {
            for (name, def) in types {
                self.types.insert(name.clone(), self.parse_type(name, def)?);
            }
        }
        if let Some(common) = obj.get("common").and_then(|v| v.as_object()) {
            for (name, def) in common {
                // A common entry is either a subrecord any record may carry,
                // or a type a subrecord may use. A subrecord's name is four
                // bytes of uppercase text, which may hold digits: `MOD2` is
                // a subrecord, and so is `MODL`.
                if name.len() == 4
                    && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                {
                    if let Some(d) = def.as_object() {
                        if d.get("type").is_some() && d.get("struct").is_none() {
                            let (ty, optional, count_from, expose, publish) =
                                self.parse_sub(&Json::Object(d.clone()))?;
                            self.common_subrecords.insert(
                                name.to_string(),
                                SubDef {
                                    name: d
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                    ty,
                                    optional,
                                    count_from,
                                    expose,
                                    publish,
                                },
                            );
                            continue;
                        }
                    }
                }
                // A common type is either a type definition, or a direct
                // statement of what it holds.
                let td = if def.get("type").is_some() && def.get("struct").is_none() {
                    let ty = self.parse_type_value(def.get("type").expect("the type is there"))?;
                    TypeDef {
                        fields: match &ty {
                            Type::Inline(f) => f.clone(),
                            _ => Vec::new(),
                        },
                        values: match &ty {
                            Type::Enum { values, .. } => values.clone(),
                            _ => BTreeMap::new(),
                        },
                        peek_union: None,
                        bits: match &ty {
                            Type::Bits { bits, .. } => bits.clone(),
                            _ => Vec::new(),
                        },
                        storage: Prim::U32,
                        union: Vec::new(),
                value_union: None,
                    }
                } else {
                    self.parse_type(name, def)?
                };
                self.common.insert(name.clone(), td);
            }
        }
        Ok(())
    }

    /// Read a record's definition.
    fn parse_record(&self, name: &str, def: &Json) -> Result<RecordDef, String> {
        let Some(o) = def.as_object() else {
            return Err(format!("the record {name:?} is not an object"));
        };
        let mut out = RecordDef {
            name: o
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            ..Default::default()
        };
        // The subrecords: a map, and sometimes an order.
        if let Some(subs) = o.get("subrecords").and_then(|v| v.as_object()) {
            for (sig, sdef) in subs {
                // A key that starts with an asterisk names a suffix rather
                // than a whole signature: `0TX` matches `00TX`, `10TX` and
                // so on.
                if let Some(suffix) = sig.strip_prefix('*') {
                    let b = suffix.as_bytes();
                    if b.is_empty() || b.len() > 4 {
                        return Err(format!(
                            "the wildcard suffix {suffix:?} of {name:?} must be 1 to 4 bytes"
                        ));
                    }
                    let (ty, optional, count_from, expose, publish) = self.parse_sub(sdef)?;
                    out.wildcards.push((
                        b.to_vec(),
                        SubDef {
                            name: sdef
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            ty,
                            optional,
                            count_from,
                            expose,
                            publish,
                        },
                    ));
                    continue;
                }
                let (ty, optional, count_from, expose, publish) = self.parse_sub(sdef)?;
                out.subrecords.insert(
                    sig.clone(),
                    SubDef {
                        name: sdef
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        ty,
                        optional,
                        count_from,
                        expose,
                        publish,
                    },
                );
            }
        }
        if let Some(order) = o.get("order").and_then(|v| v.as_array()) {
            out.order = order
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
        }
        // A record that is a sequence: an ordered list of blocks.
        if let Some(seq) = o.get("sequence") {
            out.sequence = Some(self.parse_sequence(seq)?);
        }
        Ok(out)
    }

    /// Read a record's sequence.
    fn parse_sequence(&self, seq: &Json) -> Result<SequenceDef, String> {
        let Some(o) = seq.as_object() else {
            return Err("a sequence is not an object".into());
        };
        let list = |k: &str| -> Vec<String> {
            o.get(k)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut blocks = BTreeMap::new();
        if let Some(bs) = o.get("blocks").and_then(|v| v.as_object()) {
            for (name, def) in bs {
                blocks.insert(name.clone(), self.parse_block(def)?);
            }
        }
        Ok(SequenceDef {
            start: list("start"),
            end: list("end"),
            blocks,
        })
    }

    /// Read one block of a sequence.
    fn parse_block(&self, def: &Json) -> Result<BlockDef, String> {
        let Some(o) = def.as_object() else {
            return Err("a block is not an object".into());
        };
        let mut subrecords = BTreeMap::new();
        if let Some(subs) = o.get("subrecords").and_then(|v| v.as_object()) {
            for (sig, sdef) in subs {
                let (ty, optional, count_from, expose, publish) = self.parse_sub(sdef)?;
                subrecords.insert(
                    sig.to_string(),
                    SubDef {
                        name: sdef
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        ty,
                        optional,
                        count_from,
                        expose,
                        publish,
                    },
                );
            }
        }
        let mut variants = BTreeMap::new();
        if let Some(vs) = o.get("variants").and_then(|v| v.as_object()) {
            for (vname, vdef) in vs {
                let mut vsub = BTreeMap::new();
                if let Some(subs) = vdef.get("subrecords").and_then(|v| v.as_object()) {
                    for (sig, sdef) in subs {
                        let (ty, optional, count_from, expose, publish) = self.parse_sub(sdef)?;
                        vsub.insert(
                            sig.to_string(),
                            SubDef {
                                name: sdef
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string(),
                                ty,
                                optional,
                                count_from,
                                expose,
                                publish,
                            },
                        );
                    }
                }
                variants.insert(vname.clone(), VariantDef { subrecords: vsub });
            }
        }
        Ok(BlockDef {
            opener: o
                .get("opener")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            order: o
                .get("order")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            subrecords,
            variants,
        })
    }

    /// Read a subrecord's type and its flags.
    fn parse_sub(
        &self,
        def: &Json,
    ) -> Result<(Type, bool, Option<String>, bool, bool), String> {
        let t = def.get("type").ok_or("a subrecord has no type")?;
        let ty = self.parse_type_value(t)?;
        let optional = def
            .get("optional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let count_from = def
            .get("count_from")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let expose = def.get("expose").and_then(|v| v.as_bool()).unwrap_or(false);
        // A subrecord that is published holds the value a union beside it is
        // read against.
        let publish = def.get("publish").and_then(|v| v.as_bool()).unwrap_or(false);
        Ok((ty, optional, count_from, expose, publish))
    }

    /// Read a type's value: a name, a primitive, or an object.
    fn parse_type_value(&self, t: &Json) -> Result<Type, String> {
        match t {
            Json::String(s) => Ok(match Prim::parse(s) {
                Some(p) => Type::Prim(p),
                None => Type::Named(s.clone()),
            }),
            Json::Object(o) => {
                if let Some(st) = o.get("struct").and_then(|v| v.as_array()) {
                    let mut fields = Vec::with_capacity(st.len());
                    for f in st {
                        fields.push(self.parse_field(f)?);
                    }
                    return Ok(Type::Inline(fields));
                }
                if let Some(us) = o.get("union").and_then(|v| v.as_array()) {
                    let mut variants = Vec::with_capacity(us.len());
                    for u in us {
                        let size = u.get("size").and_then(|v| v.as_u64()).map(|v| v as usize);
                        let ty = self.parse_type_value(
                            u.get("type").ok_or("a union variant has no type")?,
                        )?;
                        variants.push(Variant { size, ty });
                    }
                    // A union whose variant is chosen by another field's
                    // value has the cases and the decider beside the
                    // variants.
                    if let (Some(switch_on), Some(cases)) = (
                        o.get("switch_on").and_then(|v| v.as_str()),
                        o.get("cases").and_then(|v| v.as_object()),
                    ) {
                        let mut map = BTreeMap::new();
                        for (k, v) in cases {
                            if let Some(n) = v.as_u64() {
                                map.insert(k.parse::<i64>().unwrap_or(-1), n as usize);
                            }
                        }
                        return Ok(Type::ValueUnion(ValueUnion {
                            switch_on: switch_on.to_string(),
                            cases: map,
                            default: o.get("default").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                            variants,
                        }));
                    }
                    return Ok(Type::Union(variants));
                }
                if let Some(elem) = o.get("array") {
                    // An array: what the elements are, and how many.
                    let of = self.parse_type_value(elem)?;
                    let count = if let Some(n) = o.get("count").and_then(|v| v.as_u64()) {
                        ArrayCount::Fixed(n as usize)
                    } else if let Some(f) = o.get("count_field").and_then(|v| v.as_str()) {
                        ArrayCount::FromField(f.to_string())
                    } else if let Some(p) = o.get("count_prefix").and_then(|v| v.as_str()) {
                        // The count is in the data, in the type the schema
                        // names: a u32 length, or a u16 one.
                        let p =
                            Prim::parse(p).ok_or_else(|| format!("unknown count type {p:?}"))?;
                        ArrayCount::Prefix(p)
                    } else {
                        // No count: the elements fill what is left.
                        ArrayCount::Rest
                    };
                    return Ok(Type::Array {
                        of: Box::new(of),
                        count,
                    });
                }
                if let Some(bits) = o.get("bits").and_then(|v| v.as_object()) {
                    // The storage type is under "storage", or under "flags"
                    // when the object is written inline.
                    let storage = o
                        .get("storage")
                        .or_else(|| o.get("flags"))
                        .and_then(|v| v.as_str())
                        .and_then(Prim::parse)
                        .unwrap_or(Prim::U32);
                    return Ok(Type::Bits {
                        storage,
                        bits: self.parse_bits(bits)?,
                    });
                }
                if let Some(values) = o.get("values").and_then(|v| v.as_object()) {
                    // The storage type is under "storage", or under "enum"
                    // when the object is written inline.
                    let storage = o
                        .get("storage")
                        .or_else(|| o.get("enum"))
                        .and_then(|v| v.as_str())
                        .and_then(Prim::parse)
                        .unwrap_or(Prim::U32);
                    let mut v = BTreeMap::new();
                    for (k, name) in values {
                        let key: i64 = k
                            .parse()
                            .map_err(|_| format!("an enum has the non-numeric key {k:?}"))?;
                        v.insert(key, name.as_str().unwrap_or_default().to_string());
                    }
                    return Ok(Type::Enum { storage, values: v });
                }
                // An object with only a type is a primitive with a name.
                if let Some(p) = o.get("enum").and_then(|v| v.as_str()) {
                    return Ok(Type::Prim(
                        Prim::parse(p).ok_or_else(|| format!("unknown type {p:?}"))?,
                    ));
                }
                Err(format!(
                    "a type object says nothing about the type: {}",
                    serde_json::to_string(o).unwrap_or_default()
                ))
            }
            _ => Err("a type is neither a name nor an object".into()),
        }
    }

    /// Read a field of a struct.
    fn parse_field(&self, f: &Json) -> Result<Field, String> {
        let Some(o) = f.as_object() else {
            return Err("a field is not an object".into());
        };
        let name = o
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("a field has no name")?
            .to_string();
        let ty = self.parse_type_value(
            o.get("type")
                .ok_or_else(|| format!("the field {name:?} has no type"))?,
        )?;
        Ok(Field {
            name,
            ty,
            optional: o.get("optional").and_then(|v| v.as_bool()).unwrap_or(false),
            count_from: o
                .get("count_from")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            expose: o.get("expose").and_then(|v| v.as_bool()).unwrap_or(false),
            publish: o.get("publish").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }

    /// Read a named type's definition.
    fn parse_type(&self, name: &str, def: &Json) -> Result<TypeDef, String> {
        let Some(o) = def.as_object() else {
            return Err(format!("the type {name:?} is not an object"));
        };
        let storage = o
            .get("storage")
            .or_else(|| o.get("flags"))
            .or_else(|| o.get("enum"))
            .and_then(|v| v.as_str())
            .and_then(Prim::parse)
            .unwrap_or(Prim::U32);
        // A type is a struct, a set of names, a set of bit fields, or a
        // union.
        if let Some(us) = o.get("union").and_then(|v| v.as_array()) {
            let mut union = Vec::with_capacity(us.len());
            for u in us {
                let size = u.get("size").and_then(|v| v.as_u64()).map(|v| v as usize);
                let ty =
                    self.parse_type_value(u.get("type").ok_or("a union variant has no type")?)?;
                union.push(Variant { size, ty });
            }
            // A union whose variant is chosen by the field's first word:
            // the word is read without consuming it.
            let peek_union = if o.get("peek").and_then(|v| v.as_bool()).unwrap_or(false) {
                let mut map = BTreeMap::new();
                if let Some(cases) = o.get("cases").and_then(|v| v.as_object()) {
                    for (k, v) in cases {
                        if let Some(n) = v.as_u64() {
                            map.insert(k.parse::<i64>().unwrap_or(-1), n as usize);
                        }
                    }
                }
                Some(PeekUnion {
                    cases: map,
                    default: o.get("default").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                    variants: union.clone(),
                })
            } else {
                None
            };
            // A union whose variant is chosen by another field's value has
            // the cases and the decider beside the variants.
            let value_union = match (
                o.get("switch_on").and_then(|v| v.as_str()),
                o.get("cases").and_then(|v| v.as_object()),
            ) {
                (Some(switch_on), Some(cases)) => {
                    let mut map = BTreeMap::new();
                    for (k, v) in cases {
                        if let Some(n) = v.as_u64() {
                            map.insert(k.parse::<i64>().unwrap_or(-1), n as usize);
                        }
                    }
                    Some(ValueUnion {
                        switch_on: switch_on.to_string(),
                        cases: map,
                        default: o.get("default").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        variants: union.clone(),
                    })
                }
                _ => None,
            };
            return Ok(TypeDef {
                fields: Vec::new(),
                values: BTreeMap::new(),
                bits: Vec::new(),
                storage,
                union,
                value_union,
                peek_union,
            });
        }
        if let Some(st) = o.get("struct").and_then(|v| v.as_array()) {
            let mut fields = Vec::with_capacity(st.len());
            for f in st {
                fields.push(self.parse_field(f)?);
            }
            return Ok(TypeDef {
                fields,
                values: BTreeMap::new(),
                bits: Vec::new(),
                storage: Prim::U32,
                union: Vec::new(),
                value_union: None,
                peek_union: None,
            });
        }
        if let Some(bits) = o.get("bits").and_then(|v| v.as_object()) {
            return Ok(TypeDef {
                fields: Vec::new(),
                values: BTreeMap::new(),
                bits: self.parse_bits(bits)?,
                storage,
                union: Vec::new(),
                value_union: None,
                peek_union: None,
            });
        }
        let mut values = BTreeMap::new();
        if let Some(vs) = o.get("values").and_then(|v| v.as_object()) {
            for (k, n) in vs {
                let key: i64 = k
                    .parse()
                    .map_err(|_| format!("the type {name:?} has the non-numeric key {k:?}"))?;
                values.insert(key, n.as_str().unwrap_or_default().to_string());
            }
        }
        Ok(TypeDef {
            fields: Vec::new(),
            values,
            bits: Vec::new(),
            storage,
            union: Vec::new(),
            value_union: None,
            peek_union: None,
        })
    }

    /// Read a set of bit fields.
    fn parse_bits(&self, bits: &serde_json::Map<String, Json>) -> Result<Vec<BitField>, String> {
        let mut out = Vec::with_capacity(bits.len());
        for (spec, name) in bits {
            let (first, last) = match spec.split_once('-') {
                Some((a, b)) => {
                    let a: u32 = a
                        .trim()
                        .parse()
                        .map_err(|_| format!("a bit range {spec:?} is not a number"))?;
                    let b: u32 = b
                        .trim()
                        .parse()
                        .map_err(|_| format!("a bit range {spec:?} is not a number"))?;
                    (a, b)
                }
                None => {
                    let a: u32 = spec
                        .trim()
                        .parse()
                        .map_err(|_| format!("a bit {spec:?} is not a number"))?;
                    (a, a)
                }
            };
            out.push(BitField {
                name: name.as_str().unwrap_or_default().to_string(),
                first,
                last,
            });
        }
        out.sort_by_key(|b| b.first);
        Ok(out)
    }

    /// The fields of a named type, if it is a struct.
    pub fn type_fields(&self, name: &str) -> Option<&[Field]> {
        self.types
            .get(name)
            .or_else(|| self.common.get(name))
            .map(|t| t.fields.as_slice())
    }

    /// A named type's definition, if it has one.
    pub fn type_def(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(name).or_else(|| self.common.get(name))
    }

    /// The type an enum's values are stored as.
    pub fn enum_storage(&self, name: &str) -> Prim {
        self.type_def(name).map(|t| t.storage).unwrap_or(Prim::U32)
    }

    /// The names of an enum's values.
    ///
    /// `None` when the name is not an enum — a struct has no values, and a
    /// struct with no values is not an empty enum.
    /// The size a type takes, when it takes one.
    ///
    /// `None` for a type whose size depends on the data: a field that may
    /// be absent, an array whose count is not known, or a union whose
    /// variants differ.
    pub fn fixed_size(&self, ty: &Type) -> Option<usize> {
        match ty {
            Type::Prim(p) => p.size(),
            Type::Enum { storage, .. } | Type::Bits { storage, .. } => storage.size(),
            Type::Named(n) => {
                let td = self.type_def(n)?;
                self.type_def_size(td)
            }
            Type::Inline(fields) => {
                let mut n = 0;
                for f in fields {
                    if f.optional {
                        return None;
                    }
                    n += self.fixed_size(&f.ty)?;
                }
                Some(n)
            }
            Type::Union(variants) | Type::ValueUnion(ValueUnion { variants, .. }) => {
                // The variants have different shapes, so the union has no
                // fixed size unless they all agree.
                let mut n = None;
                for v in variants {
                    let s = self.fixed_size(&v.ty)?;
                    match n {
                        None => n = Some(s),
                        Some(prev) if prev == s => {}
                        Some(_) => return None,
                    }
                }
                n
            }
            Type::Array { of, count } => match count {
                ArrayCount::Fixed(c) => self.fixed_size(of).map(|s| s * c),
                _ => None,
            },
        }
    }

    /// The size a named type takes, when it takes one.
    fn type_def_size(&self, td: &TypeDef) -> Option<usize> {
        if !td.union.is_empty() {
            let mut n = None;
            for v in &td.union {
                let s = self.fixed_size(&v.ty)?;
                match n {
                    None => n = Some(s),
                    Some(prev) if prev == s => {}
                    Some(_) => return None,
                }
            }
            return n;
        }
        if td.value_union.is_some() {
            return None;
        }
        if !td.bits.is_empty() || !td.values.is_empty() {
            return td.storage.size();
        }
        let mut n = 0;
        for f in &td.fields {
            if f.optional {
                return None;
            }
            n += self.fixed_size(&f.ty)?;
        }
        Some(n)
    }

    pub fn enum_values(&self, name: &str) -> Option<&BTreeMap<i64, String>> {
        let t = self.types.get(name).or_else(|| self.common.get(name))?;
        if t.values.is_empty() {
            return None;
        }
        Some(&t.values)
    }

    /// Check that the schema refers only to types it defines.
    pub fn check(&self) -> Result<(), SchemaError> {
        let mut errs = Vec::new();
        for (name, r) in &self.records {
            for (sig, sub) in &r.subrecords {
                self.check_type(&format!("{name}.{sig}"), &sub.ty, &mut errs);
            }
        }
        for (name, t) in self.types.iter().chain(self.common.iter()) {
            for f in &t.fields {
                self.check_type(&format!("{name}.{}", f.name), &f.ty, &mut errs);
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(SchemaError(errs.join("; ")))
        }
    }

    /// Check one type.
    fn check_type(&self, owner: &str, ty: &Type, errs: &mut Vec<String>) {
        match ty {
            Type::Named(n) => {
                if self.type_fields(n).is_none() && self.enum_values(n).is_none() {
                    errs.push(format!("{owner} refers to the unknown type {n:?}"));
                }
            }
            Type::Inline(fields) => {
                for f in fields {
                    self.check_type(&format!("{owner}.{}", f.name), &f.ty, errs);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_from(text: &str) -> Schema {
        let j: Json = serde_json::from_str(text).unwrap();
        let mut s = Schema::new();
        s.merge(&j).unwrap();
        s
    }

    #[test]
    fn a_record_is_read() {
        let s = schema_from(
            r#"{"format":"nvre.esm.schema/1",
                "records":{"ACHR":{"name":"Placed NPC","subrecords":{
                  "NAME":{"type":"formid"},
                  "DATA":{"type":"PosRot"}}}}}"#,
        );
        let r = s.records.get("ACHR").unwrap();
        assert_eq!(r.name, "Placed NPC");
        assert_eq!(r.subrecords.len(), 2);
        assert_eq!(r.subrecords["NAME"].ty, Type::Prim(Prim::FormId));
    }

    #[test]
    fn a_type_is_read() {
        let s = schema_from(
            r#"{"format":"nvre.esm.schema/1",
                "types":{"PosRot":{"struct":[
                  {"name":"x","type":"f32"},
                  {"name":"y","type":"f32"}]}}}"#,
        );
        let f = s.type_fields("PosRot").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].ty, Type::Prim(Prim::F32));
    }

    #[test]
    fn an_inline_enum_is_read() {
        let s = schema_from(
            r#"{"format":"nvre.esm.schema/1",
                "types":{"E":{"struct":[
                  {"name":"t","type":{"enum":"u32","values":{"0":"A","1":"B"}}}]}}}"#,
        );
        let f = s.type_fields("E").unwrap();
        match &f[0].ty {
            Type::Enum { storage, values } => {
                assert_eq!(*storage, Prim::U32);
                assert_eq!(values.get(&1).map(String::as_str), Some("B"));
            }
            other => panic!("expected an enum, got {other:?}"),
        }
    }

    #[test]
    fn a_bit_field_is_read() {
        let s = schema_from(
            r#"{"format":"nvre.esm.schema/1",
                "types":{"E":{"struct":[
                  {"name":"t","type":{"bits":{"0":"A","1-3":"B"}}}]}}}"#,
        );
        let f = s.type_fields("E").unwrap();
        match &f[0].ty {
            Type::Bits { bits, .. } => {
                assert_eq!(bits.len(), 2);
                assert_eq!(bits[0].first, 0);
                assert_eq!(bits[1].first, 1);
                assert_eq!(bits[1].last, 3);
            }
            other => panic!("expected bits, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_type_is_reported() {
        let mut s = Schema::new();
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.esm.schema/1",
                    "records":{"ACHR":{"subrecords":{"NAME":{"type":"NoSuchType"}}}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(s.check().is_err());
    }

    #[test]
    fn a_layer_replaces_a_record() {
        let mut s = schema_from(
            r#"{"format":"nvre.esm.schema/1",
                "records":{"ACHR":{"subrecords":{"NAME":{"type":"formid"}}}}}"#,
        );
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.esm.schema/1",
                    "records":{"ACHR":{"subrecords":{"OTHER":{"type":"u32"}}}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let r = s.records.get("ACHR").unwrap();
        assert_eq!(r.subrecords.len(), 1);
        assert!(r.subrecords.contains_key("OTHER"));
    }
}
