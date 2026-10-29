//! The schema: a NIF block's layout, as data.
//!
//! A NIF is a list of blocks, and each block's fields are laid out in a fixed
//! order that depends on the block's type and on the file's version. The
//! schema says what that layout is, so the reader does not have to know it.
//!
//! # The file format
//!
//! Every `*.json` file in a directory is merged. Each holds:
//!
//! ```json
//! {
//!   "format": "nvre.nif.schema/1",
//!   "enums":   { "AlphaFunction": {"storage": "u16", "values": {"0": "ALWAYS"}} },
//!   "structs": { "TexDesc": {"fields": [{"name": "count", "type": "u32"}]} },
//!   "blocks":  { "NiNode": {"inherit": "NiAVObject", "fields": [...] }}
//! }
//! ```
//!
//! A field is `{"name", "type"}`, plus optionally:
//!
//! - `cond` — an expression over the fields read before it; the field is
//!   read only when the expression holds. The expression may use `version`,
//!   `user_version`, `bs_version` and `arg`.
//! - `length` — how many elements an array has: a number, an expression over
//!   the fields before it, or the name of an earlier integer field.
//! - `inherit` — a block or struct type this one is based on, whose fields
//!   come first.
//! - `type` — a primitive, a struct name, or `T` for "whatever the
//!   enclosing type's argument says".
//!
//! # Layers
//!
//! A schema is loaded from a base directory and any number of layers over it.
//! A layer may replace a block or a struct outright, or add to one. That is
//! how a game's own layouts sit on top of a shared base: the base holds what
//! every game has, and a game's layer holds what is different about it.

use crate::expr::Expr;
use crate::header::Version;
use crate::value::Value;
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The struct types the schema language has built in.
///
/// A `sized_string` is a length and that many bytes of text, which the
/// schema writes as a type of its own rather than as a field with a length.
pub const BUILTIN_STRUCTS: [&str; 1] = ["sized_string"];

/// The format tag a schema file carries.
pub const FORMAT: &str = "nvre.nif.schema/1";

/// The format tag the schemas in `schemas/nif/` carry.
///
/// Those files were written for the reader this crate replaces, and they are
/// NVRE's own data, so this reader reads them as they are.
pub const LEGACY_FORMAT: &str = "fnv.nif.schema/1";

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
    /// A 32-bit float.
    F32,
    /// A byte holding a flag.
    Bool,
    /// A signed 32-bit block index, -1 for none.
    Ref,
    /// A signed 32-bit block index, -1 for none, pointing up.
    Ptr,
    /// A 16-bit float.
    HFloat,
    /// Two 16-bit floats.
    HalfVec2,
    /// Three 16-bit floats.
    HalfVec3,
    /// Four 16-bit floats.
    HalfVec4,
    /// Two 32-bit floats: a texture coordinate.
    TexCoord,
    /// Two 32-bit floats.
    Vec2,
    /// Three 32-bit floats.
    Vec3,
    /// Four 32-bit floats.
    Vec4,
    /// A quaternion, stored w, x, y, z.
    Quat,
    /// Three 32-bit floats: a colour.
    Color3,
    /// Four 32-bit floats: a colour with an alpha.
    Color4,
    /// Four bytes: a colour.
    ByteColor4,
    /// Three bytes: a normal or a tangent.
    ByteVec3,
    /// Three 16-bit numbers: a vertex's position in a compressed mesh
    /// shape, which stores its triangle vertices this way.
    U16Vec3,
    /// A byte holding a value in the range -1 to 1.
    NormByte,
    /// A 2×2 matrix, row-major.
    Mat22,
    /// A 3×3 matrix, row-major.
    Mat33,
    /// A 3×4 matrix, row-major.
    Mat34,
    /// A 4×4 matrix, row-major.
    Mat44,
    /// Three 16-bit vertex indices.
    Triangle,
    /// A string: NUL-terminated, or an index into the string table.
    String,
    /// A 32-bit length and then that many bytes of Windows-1252.
    SizedString,
    /// A 16-bit length and then that many bytes of Windows-1252.
    SizedString16,
    /// Opaque bytes; needs a length.
    Bytes,
    /// Whatever the rest of the block holds, up to the next multiple of the
    /// alignment.
    Align,
}

impl Prim {
    /// The primitive a name names, if it names one.
    ///
    /// A name may be written with underscores or without them: the schemas
    /// write `byte_color4` and `half_vec2`, and the names are the same
    /// either way.
    pub fn parse(s: &str) -> Option<Prim> {
        // A name may be written with underscores or without them: the
        // schemas write `byte_color4` and `half_vec2`, and the names are
        // the same either way.
        let name = s.replace('_', "");
        Some(match name.as_str() {
            "u8" => Prim::U8,
            "i8" => Prim::I8,
            "u16" => Prim::U16,
            "i16" => Prim::I16,
            "u32" => Prim::U32,
            "i32" => Prim::I32,
            "u64" => Prim::U64,
            "f32" => Prim::F32,
            "bool" => Prim::Bool,
            "ref" => Prim::Ref,
            "ptr" => Prim::Ptr,
            "hfloat" => Prim::HFloat,
            "halfvec2" => Prim::HalfVec2,
            "halfvec3" => Prim::HalfVec3,
            "halfvec4" => Prim::HalfVec4,
            "texcoord" => Prim::TexCoord,
            "vec2" => Prim::Vec2,
            "vec3" => Prim::Vec3,
            "vec4" => Prim::Vec4,
            "quat" => Prim::Quat,
            "color3" => Prim::Color3,
            "color4" => Prim::Color4,
            "bytecolor4" => Prim::ByteColor4,
            "bytevec3" => Prim::ByteVec3,
            "u16vec3" => Prim::U16Vec3,
            "normbyte" => Prim::NormByte,
            "mat22" => Prim::Mat22,
            "mat33" => Prim::Mat33,
            "mat34" => Prim::Mat34,
            "mat44" => Prim::Mat44,
            "triangle" => Prim::Triangle,
            "string" => Prim::String,
            "sizedstring" => Prim::SizedString,
            "sizedstring16" => Prim::SizedString16,
            "bytes" => Prim::Bytes,
            "align" => Prim::Align,
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
            Prim::F32 => "f32",
            Prim::Bool => "bool",
            Prim::Ref => "ref",
            Prim::Ptr => "ptr",
            Prim::HFloat => "hfloat",
            Prim::HalfVec2 => "halfvec2",
            Prim::HalfVec3 => "halfvec3",
            Prim::HalfVec4 => "halfvec4",
            Prim::TexCoord => "texcoord",
            Prim::Vec2 => "vec2",
            Prim::Vec3 => "vec3",
            Prim::Vec4 => "vec4",
            Prim::Quat => "quat",
            Prim::Color3 => "color3",
            Prim::Color4 => "color4",
            Prim::ByteColor4 => "bytecolor4",
            Prim::ByteVec3 => "bytevec3",
            Prim::U16Vec3 => "u16vec3",
            Prim::NormByte => "normbyte",
            Prim::Mat22 => "mat22",
            Prim::Mat33 => "mat33",
            Prim::Mat34 => "mat34",
            Prim::Mat44 => "mat44",
            Prim::Triangle => "triangle",
            Prim::String => "string",
            Prim::SizedString => "sizedstring",
            Prim::SizedString16 => "sizedstring16",
            Prim::Bytes => "bytes",
            Prim::Align => "align",
        }
    }
}

/// What a field's type is.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    /// A primitive.
    Prim(Prim),
    /// A struct type.
    Named(String),
    /// An enum: read as its storage type, and named by the schema.
    Enum(String),
    /// The enclosing type's argument.
    Template,
}

/// A field of a block or struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field's name.
    pub name: String,
    /// What the field holds.
    pub ty: TypeRef,
    /// When the field is read.
    pub cond: Option<Expr>,
    /// How many elements an array holds.
    pub length: Option<Length>,
    /// The argument passed to a struct type.
    pub arg: Option<Expr>,
    /// The block types the field belongs to: it is read only for these, and
    /// for any type based on one of them.
    pub only: Vec<String>,
    /// The block types the field does not belong to.
    pub exclude: Vec<String>,
    /// The file version the field appears from, inclusive.
    pub since: Option<Version>,
    /// The file version the field appears up to, inclusive.
    pub until: Option<Version>,
    /// What the type parameter `T` stands for inside a struct type.
    pub template: Option<String>,
    /// The width of a two-dimensional array: how many elements each row has.
    pub width: Option<Width>,
}

/// How wide a row of a two-dimensional array is.
#[derive(Debug, Clone, PartialEq)]
pub enum Width {
    /// A fixed number of elements.
    Fixed(usize),
    /// An expression over the fields read before.
    Expr(Expr),
    /// The name of an earlier integer field, or of an earlier integer array
    /// (one row width per element).
    Field(String),
}

/// How many elements an array holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    /// A fixed count.
    Fixed(usize),
    /// An expression over the fields read before.
    Expr(Expr),
    /// The name of an earlier integer field, or of an earlier integer array
    /// (one row width per element).
    Field(String),
}

/// An enum: a set of names for the integers a field can hold.
///
/// An enum is either a set of values, or a set of **bit fields**: a range of
/// bits that together hold one value, each with its own name.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    /// The type the value is stored as.
    pub storage: Prim,
    /// The names, by value.
    pub values: BTreeMap<i64, String>,
    /// The bit fields: a name, the first bit and the last bit.
    pub bits: Vec<BitField>,
}

/// A bit field of an enum: a range of bits that hold one value.
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
    /// The value of the field in a stored value.
    pub fn value(&self, stored: i64) -> i64 {
        let width = self.last - self.first + 1;
        if width >= 64 {
            return stored;
        }
        (stored >> self.first) & ((1i64 << width) - 1)
    }
}

/// A struct type: a layout that is not a block.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    /// The type this one is based on, whose fields come first.
    pub inherit: Option<String>,
    /// The fields.
    pub fields: Vec<Field>,
    /// What the type parameter `T` stands for inside this struct.
    pub template: Option<String>,
}

/// A block type: a layout that is a block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    /// The type this one is based on, whose fields come first.
    pub inherit: Option<String>,
    /// The fields.
    pub fields: Vec<Field>,
}

/// A fault in a schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SchemaError(pub String);

/// The whole schema: every block and struct type.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// The block types, by name.
    pub blocks: BTreeMap<String, BlockDef>,
    /// The struct types, by name.
    pub structs: BTreeMap<String, StructDef>,
    /// The enums, by name.
    pub enums: BTreeMap<String, EnumDef>,
    /// The block types each block type is based on.
    pub block_bases: HashMap<String, String>,
    /// The struct types each struct type is based on.
    pub struct_bases: HashMap<String, String>,
    /// The blocks that are abstract: they are never read on their own, only
    /// through a block that is based on them.
    pub abstract_blocks: HashSet<String>,
    /// The resolved field lists, by block type name: the type's own fields
    /// with its bases' first, in read order.
    ///
    /// A block's fields are the same every time it is read, so the chain walk
    /// and the clone it costs are done once per type rather than once per
    /// block. Over a whole game's meshes that is the difference between
    /// 335,863 builds and one per type: measured on Fallout: New Vegas's
    /// 2,721 clutter meshes, the schema work drops from 62s to a few seconds.
    ///
    /// Filled on first use rather than at load, because a schema is loaded
    /// before anything is decoded and the layers are merged one at a time.
    /// It is behind a `OnceLock` rather than a `Mutex` because a schema is
    /// read-only once loaded: the value is computed from the maps above, which
    /// nothing writes afterwards, so there is nothing to lock against.
    block_fields_cache: OnceLock<HashMap<String, Vec<Field>>>,
    /// The resolved field lists, by struct type name.
    struct_fields_cache: OnceLock<HashMap<String, Vec<Field>>>,
    /// Every block type's inheritance chain, nearest base first, by name.
    ///
    /// A field's `only` and `exclude` lists ask "is this block of that type?"
    /// once per field of every block, and answering it by walking the chain
    /// each time allocates a `Vec<String>` and clones every name in it.
    chains_cache: OnceLock<HashMap<String, Vec<String>>>,
}

impl Schema {
    /// A schema with nothing in it.
    pub fn new() -> Schema {
        Schema::default()
    }

    /// Read a schema from a directory of JSON files.
    pub fn load_dir(dir: &Path) -> Result<Schema, SchemaError> {
        let mut s = Schema::new();
        s.load_layer(dir)?;
        s.check()?;
        Ok(s)
    }

    /// Read a schema from a base directory and any number of layers over it.
    ///
    /// A layer may replace a block or a struct outright, or add to one. The
    /// base is read first, then each layer in turn.
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

    /// Read a schema from a JSON document.
    pub fn merge_str(&mut self, text: &str) -> Result<(), String> {
        let j: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
        self.merge(&j)
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
        if let Some(blocks) = obj.get("blocks").and_then(|v| v.as_object()) {
            for (name, def) in blocks {
                let d = self.parse_block(def)?;
                self.blocks.insert(
                    name.clone(),
                    BlockDef {
                        inherit: d.inherit.clone(),
                        fields: d.fields.clone(),
                    },
                );
                match d.inherit {
                    Some(base) => {
                        self.block_bases.insert(name.clone(), base);
                    }
                    None => {
                        self.block_bases.remove(name);
                    }
                }
                if d.abstract_ {
                    self.abstract_blocks.insert(name.clone());
                } else {
                    self.abstract_blocks.remove(name);
                }
            }
            // A layer changes what the resolved lists and chains hold, so the
            // caches built from the layers below are stale. They are dropped
            // rather than updated, because the next lookup rebuilds them from
            // the schema as it now stands.
            self.block_fields_cache = OnceLock::new();
            self.chains_cache = OnceLock::new();
        }
        if let Some(structs) = obj.get("structs").and_then(|v| v.as_object()) {
            for (name, def) in structs {
                let d = self.parse_struct(def)?;
                self.structs.insert(
                    name.clone(),
                    StructDef {
                        inherit: d.inherit.clone(),
                        fields: d.fields.clone(),
                        template: d.template.clone(),
                    },
                );
                match d.inherit {
                    Some(base) => {
                        self.struct_bases.insert(name.clone(), base);
                    }
                    None => {
                        self.struct_bases.remove(name);
                    }
                }
            }
            // As for the blocks: a layer changes the resolved struct lists.
            self.struct_fields_cache = OnceLock::new();
        }
        if let Some(enums) = obj.get("enums").and_then(|v| v.as_object()) {
            for (name, def) in enums {
                let Some(o) = def.as_object() else {
                    return Err(format!("enum {name:?} is not an object"));
                };
                let storage = o
                    .get("storage")
                    .and_then(|v| v.as_str())
                    .and_then(Prim::parse)
                    .unwrap_or(Prim::U32);
                let mut values = BTreeMap::new();
                if let Some(vs) = o.get("values").and_then(|v| v.as_object()) {
                    for (k, v) in vs {
                        let key: i64 = k
                            .parse()
                            .map_err(|_| format!("enum {name:?} has the non-numeric key {k:?}"))?;
                        values.insert(key, v.as_str().unwrap_or_default().to_string());
                    }
                }
                // The bit fields, if the enum has them: a name and a range
                // of bits, written as "3" or "1-4".
                // The bit fields: the bits are the keys, written as "3" or
                // "1-4", and the name of the field is the value.
                let mut bits = Vec::new();
                if let Some(bs) = o.get("bits").and_then(|v| v.as_object()) {
                    for (spec, bit_name) in bs {
                        let (first, last) = match spec.split_once('-') {
                            Some((a, b)) => {
                                let a: u32 = a.trim().parse().map_err(|_| {
                                    format!("enum {name:?} has the bit range {spec:?}")
                                })?;
                                let b: u32 = b.trim().parse().map_err(|_| {
                                    format!("enum {name:?} has the bit range {spec:?}")
                                })?;
                                (a, b)
                            }
                            None => {
                                let a: u32 = spec
                                    .trim()
                                    .parse()
                                    .map_err(|_| format!("enum {name:?} has the bit {spec:?}"))?;
                                (a, a)
                            }
                        };
                        bits.push(BitField {
                            name: bit_name.as_str().unwrap_or_default().to_string(),
                            first,
                            last,
                        });
                    }
                }
                bits.sort_by_key(|b| b.first);
                self.enums.insert(
                    name.clone(),
                    EnumDef {
                        storage,
                        values,
                        bits,
                    },
                );
            }
        }
        Ok(())
    }

    /// The block types a block type is based on, nearest first.
    ///
    /// The chain is resolved once per type and cached: a field's `only` and
    /// `exclude` lists are checked against it once per field of every block, so
    /// rebuilding the chain each time allocates a `Vec<String>` and clones
    /// every name in it — which over a corpus of meshes is most of the decode
    /// and re-encode time. See [`Schema::is_a`] for the cached form.
    pub fn block_chain(&self, name: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut cur = name;
        while let Some(base) = self.block_bases.get(cur).or_else(|| {
            if cur.contains("::") {
                self.block_bases.get(&cur.replace("::", "__"))
            } else {
                None
            }
        }) {
            out.push(base.as_str());
            cur = base;
        }
        out
    }

    /// Whether the schema knows about a block type.
    pub fn contains_block(&self, name: &str) -> bool {
        self.blocks.contains_key(name)
            || (name.contains("::") && self.blocks.contains_key(&name.replace("::", "__")))
    }

    /// Whether `name` is of type `ty`, or of a type based on it.
    ///
    /// The cached form of [`Schema::block_chain`] plus a search: it answers the
    /// question a field's `only` and `exclude` actually ask, without building
    /// the chain.
    pub fn is_a(&self, name: &str, ty: &str) -> bool {
        if name == ty {
            return true;
        }
        let cache = self.chains_cache.get_or_init(|| self.resolve_chains());
        if let Some(chain) = cache.get(name) {
            return chain.iter().any(|b| b == ty);
        }
        if name.contains("::") {
            let alt = name.replace("::", "__");
            if let Some(chain) = cache.get(&alt) {
                return chain.iter().any(|b| b == ty);
            }
        }
        false
    }

    /// Every block type's inheritance chain, nearest base first, built once.
    fn resolve_chains(&self) -> HashMap<String, Vec<String>> {
        let mut out = HashMap::with_capacity(self.blocks.len());
        for name in self.blocks.keys() {
            let mut chain = Vec::new();
            let mut cur = name.clone();
            while let Some(base) = self.block_bases.get(&cur) {
                chain.push(base.clone());
                cur = base.clone();
            }
            out.insert(name.clone(), chain.clone());
            if name.contains("__") {
                out.insert(name.replace("__", "::"), chain);
            }
        }
        out
    }

    /// The struct types a struct type is based on, nearest first.
    pub fn struct_chain(&self, name: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut cur = name;
        while let Some(base) = self.struct_bases.get(cur) {
            out.push(base.as_str());
            cur = base.as_str();
        }
        out
    }

    /// The fields of a block type, its bases' fields first.
    ///
    /// The list is resolved once and cached: it is the same every time the
    /// type is read, and the chain walk plus the clone it costs is what made a
    /// corpus-wide pass slow. See the cache field's note for the measurement.
    pub fn block_fields(&self, name: &str) -> Option<Vec<Field>> {
        self.block_fields_ref(name).map(<[Field]>::to_vec)
    }

    /// The fields of a block type, borrowed from the cache.
    ///
    /// The same list [`Schema::block_fields`] returns, without the copy: a
    /// reader that only looks at the fields takes this one, so a whole corpus
    /// costs one build per type instead of one per block.
    pub fn block_fields_ref(&self, name: &str) -> Option<&[Field]> {
        let cache = self
            .block_fields_cache
            .get_or_init(|| self.resolve_block_fields());
        if let Some(f) = cache.get(name) {
            return Some(f.as_slice());
        }
        if name.contains("::") {
            let alt = name.replace("::", "__");
            return cache.get(&alt).map(Vec::as_slice);
        }
        None
    }

    /// The fields of every block type, its bases' first, built once.
    fn resolve_block_fields(&self) -> HashMap<String, Vec<Field>> {
        let mut out = HashMap::with_capacity(self.blocks.len());
        for name in self.blocks.keys() {
            let mut fields = Vec::new();
            for base in self.block_chain(name).into_iter().rev() {
                if let Some(bd) = self.blocks.get(base) {
                    fields.extend(bd.fields.iter().cloned());
                }
            }
            if let Some(d) = self.blocks.get(name) {
                fields.extend(d.fields.iter().cloned());
            }
            out.insert(name.clone(), fields.clone());
            if name.contains("__") {
                out.insert(name.replace("__", "::"), fields);
            }
        }
        out
    }

    /// The fields of a struct type, its bases' fields first.
    pub fn struct_fields(&self, name: &str) -> Option<Vec<Field>> {
        self.struct_fields_ref(name).map(<[Field]>::to_vec)
    }

    /// The fields of a struct type, borrowed from the cache.
    pub fn struct_fields_ref(&self, name: &str) -> Option<&[Field]> {
        let cache = self
            .struct_fields_cache
            .get_or_init(|| self.resolve_struct_fields());
        cache.get(name).map(Vec::as_slice)
    }

    /// The fields of every struct type, its bases' first, built once.
    fn resolve_struct_fields(&self) -> HashMap<String, Vec<Field>> {
        let mut out = HashMap::with_capacity(self.structs.len());
        for name in self.structs.keys() {
            let mut fields = Vec::new();
            for base in self.struct_chain(name).into_iter().rev() {
                if let Some(sd) = self.structs.get(base) {
                    fields.extend(sd.fields.iter().cloned());
                }
            }
            if let Some(d) = self.structs.get(name) {
                fields.extend(d.fields.iter().cloned());
            }
            out.insert(name.clone(), fields);
        }
        out
    }

    /// The name of an enum's value, if it has one.
    pub fn enum_name(&self, enum_name: &str, value: i64) -> Option<&str> {
        self.enums
            .get(enum_name)?
            .values
            .get(&value)
            .map(|s| s.as_str())
    }

    /// Check that the schema refers only to types it defines.
    pub fn check(&self) -> Result<(), SchemaError> {
        let mut errs = Vec::new();
        for name in self.blocks.keys() {
            for base in self.block_chain(name) {
                if !self.blocks.contains_key(base) {
                    errs.push(format!("block {name:?} is based on the unknown {base:?}"));
                }
            }
            // The fields of a block are its own and its bases', so a length
            // may come from any of them.
            let all = self.block_fields(name).unwrap_or_default();
            self.check_fields(name, &all, &mut errs);
        }
        for name in self.structs.keys() {
            for base in self.struct_chain(name) {
                if !self.structs.contains_key(base) {
                    errs.push(format!("struct {name:?} is based on the unknown {base:?}"));
                }
            }
            let all = self.struct_fields(name).unwrap_or_default();
            self.check_fields(name, &all, &mut errs);
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(SchemaError(errs.join("; ")))
        }
    }

    /// Check a list of fields.
    fn check_fields(&self, owner: &str, fields: &[Field], errs: &mut Vec<String>) {
        for f in fields {
            match &f.ty {
                TypeRef::Named(n) => {
                    // A name is a struct type, or an enum, or one of the
                    // built-in struct types the schema language has.
                    if !self.structs.contains_key(n)
                        && !self.enums.contains_key(n)
                        && !BUILTIN_STRUCTS.contains(&n.as_str())
                    {
                        errs.push(format!(
                            "{owner}.{} refers to the unknown type {n:?}",
                            f.name
                        ));
                    }
                }
                TypeRef::Enum(_) => {}
                _ => {}
            }
            if let Some(Length::Field(n)) = &f.length {
                // The length comes from an earlier field, or from the
                // argument the enclosing type was given.
                if n != "arg" && !fields.iter().any(|g| &g.name == n) {
                    errs.push(format!(
                        "{owner}.{} takes its length from {n:?}, which it does not have",
                        f.name
                    ));
                }
            }
        }
    }

    /// Read a block definition.
    fn parse_block(&self, j: &Json) -> Result<ParsedBlock, String> {
        let Some(o) = j.as_object() else {
            return Err("a block is not an object".into());
        };
        Ok(ParsedBlock {
            inherit: o
                .get("inherit")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            fields: self.parse_fields(o.get("fields"))?,
            abstract_: o.get("abstract").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }

    /// Read a struct definition.
    fn parse_struct(&self, j: &Json) -> Result<ParsedStruct, String> {
        let Some(o) = j.as_object() else {
            return Err("a struct is not an object".into());
        };
        Ok(ParsedStruct {
            inherit: o
                .get("inherit")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            fields: self.parse_fields(o.get("fields"))?,
            template: o
                .get("template")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        })
    }

    /// Read a list of fields.
    fn parse_fields(&self, j: Option<&Json>) -> Result<Vec<Field>, String> {
        let Some(j) = j else {
            return Ok(Vec::new());
        };
        let Some(list) = j.as_array() else {
            return Err("a field list is not an array".into());
        };
        let mut out = Vec::with_capacity(list.len());
        for f in list {
            let Some(o) = f.as_object() else {
                return Err("a field is not an object".into());
            };
            let name = o
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("a field has no name")?
                .to_string();
            let ty = o
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("field {name:?} has no type"))?;
            let ty = if ty == "T" {
                TypeRef::Template
            } else if let Some(p) = Prim::parse(ty) {
                TypeRef::Prim(p)
            } else {
                TypeRef::Named(ty.to_string())
            };
            let cond = match o.get("cond").and_then(|v| v.as_str()) {
                Some(t) => Some(Expr::parse(t)?),
                None => None,
            };
            let length = match o.get("length") {
                Some(Json::Number(n)) => Some(Length::Fixed(n.as_u64().unwrap_or(0) as usize)),
                Some(Json::String(s)) => {
                    // A plain name is the name of an earlier field: a length
                    // is a count, so it comes from a field rather than from
                    // a computation.
                    if is_identifier(s) {
                        Some(Length::Field(s.clone()))
                    } else {
                        Some(Length::Expr(Expr::parse(s)?))
                    }
                }
                _ => None,
            };
            let arg = match o.get("arg").and_then(|v| v.as_str()) {
                Some(t) => Some(Expr::parse(t)?),
                None => None,
            };
            let since = match o.get("since").and_then(|v| v.as_str()) {
                Some(t) => Some(
                    crate::header::parse_version(t)
                        .ok_or_else(|| format!("field {name:?} has the version {t:?}"))?,
                ),
                None => None,
            };
            let until = match o.get("until").and_then(|v| v.as_str()) {
                Some(t) => Some(
                    crate::header::parse_version(t)
                        .ok_or_else(|| format!("field {name:?} has the version {t:?}"))?,
                ),
                None => None,
            };
            let template = o
                .get("template")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let only = string_list(o.get("only"));
            let exclude = string_list(o.get("exclude"));
            let width = match o.get("width") {
                Some(Json::Number(n)) => Some(Width::Fixed(n.as_u64().unwrap_or(0) as usize)),
                Some(Json::String(s)) => {
                    if is_identifier(s) {
                        Some(Width::Field(s.clone()))
                    } else {
                        Some(Width::Expr(Expr::parse(s)?))
                    }
                }
                _ => None,
            };
            out.push(Field {
                name,
                ty,
                cond,
                length,
                arg,
                only,
                exclude,
                width,
                since,
                until,
                template,
            });
        }
        Ok(out)
    }
}

/// A list of strings, whether it is one string or many.
fn string_list(j: Option<&Json>) -> Vec<String> {
    match j {
        Some(Json::String(s)) => vec![s.clone()],
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a string is a plain name: letters, digits and underscores, and
/// nothing else.
fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !s.chars().next().unwrap().is_ascii_digit()
}

/// A block definition, as it was read.
struct ParsedBlock {
    inherit: Option<String>,
    fields: Vec<Field>,
    abstract_: bool,
}

/// A struct definition, as it was read.
struct ParsedStruct {
    inherit: Option<String>,
    fields: Vec<Field>,
    template: Option<String>,
}

/// The scope an expression is evaluated in, built from the fields read so far.
pub type Scope = HashMap<String, Value>;

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
    fn a_block_is_read() {
        let s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{"NiNode":{"fields":[{"name":"name","type":"string"}]}}}"#,
        );
        assert!(s.blocks.contains_key("NiNode"));
        assert_eq!(s.block_fields("NiNode").unwrap().len(), 1);
    }

    #[test]
    fn a_base_blocks_fields_come_first() {
        let s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{
                  "NiAVObject":{"fields":[{"name":"flags","type":"u32"}]},
                  "NiNode":{"inherit":"NiAVObject","fields":[{"name":"children","type":"ref"}]}}}"#,
        );
        let f = s.block_fields("NiNode").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].name, "flags");
        assert_eq!(f[1].name, "children");
    }

    #[test]
    fn a_layer_replaces_a_block() {
        let mut s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{"NiNode":{"fields":[{"name":"name","type":"string"}]}}}"#,
        );
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "blocks":{"NiNode":{"fields":[{"name":"other","type":"u32"}]}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let f = s.block_fields("NiNode").unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].name, "other");
    }

    #[test]
    fn an_unknown_type_is_reported() {
        let mut s = Schema::new();
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "blocks":{"NiNode":{"fields":[{"name":"x","type":"NoSuchType"}]}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(s.check().is_err());
    }

    #[test]
    fn a_length_from_a_field_that_is_not_there_is_reported() {
        let mut s = Schema::new();
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "blocks":{"NiNode":{"fields":[{"name":"x","type":"u32","length":"count"}]}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(s.check().is_err());
    }

    #[test]
    fn an_enum_with_a_non_numeric_key_is_reported() {
        let mut s = Schema::new();
        let e = s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "enums":{"E":{"storage":"u16","values":{"0x10":"A"}}}}"#,
            )
            .unwrap(),
        );
        assert!(e.is_err());
    }

    /// A layer merged after a lookup has already been made must still apply.
    ///
    /// The resolved field lists and chains are cached, so a cache built while
    /// only the base was loaded would answer every later question from the base
    /// alone: a block the version layer defines reads as having no fields at
    /// all, and the file fails to decode rather than failing on a wrong layout.
    /// Skyrim SE's `BSTriShape` is the case that found it.
    #[test]
    fn a_layer_merged_after_a_lookup_still_applies() {
        let mut s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{"NiNode":{"fields":[{"name":"a","type":"u32"}]}}}"#,
        );
        // The lookup that fills the cache, before the layer is merged.
        assert_eq!(s.block_fields("NiNode").map(|f| f.len()), Some(1));
        assert!(!s.is_a("NiNode", "NiAVObject"));

        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "blocks":{
                      "NiNode":{"inherit":"NiAVObject","fields":[{"name":"b","type":"u32"}]},
                      "NiAVObject":{"fields":[{"name":"name","type":"string"}]}}}"#,
            )
            .unwrap(),
        )
        .unwrap();

        // Both the new field and the inherited one, and the new ancestry.
        let f = s.block_fields("NiNode").unwrap();
        let names: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["name", "b"],
            "the layer's fields and its base's"
        );
        assert!(s.is_a("NiNode", "NiAVObject"), "the layer's inheritance");
    }

    /// A struct layer merged after a lookup applies, as a block's does.
    #[test]
    fn a_struct_layer_merged_after_a_lookup_still_applies() {
        let mut s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "structs":{"S":{"fields":[{"name":"a","type":"u32"}]}}}"#,
        );
        assert_eq!(s.struct_fields("S").map(|f| f.len()), Some(1));
        s.merge(
            &serde_json::from_str(
                r#"{"format":"nvre.nif.schema/1",
                    "structs":{"S":{"inherit":"T","fields":[{"name":"b","type":"u32"}]},
                               "T":{"fields":[{"name":"c","type":"u32"}]}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let fields = s.struct_fields("S").unwrap();
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["c", "b"]);
    }

    #[test]
    fn a_condition_is_parsed() {
        let s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{"NiNode":{"fields":[
                  {"name":"a","type":"u32"},
                  {"name":"b","type":"u32","cond":"bs_version >= 83"}]}}}"#,
        );
        let f = s.block_fields("NiNode").unwrap();
        assert!(f[0].cond.is_none());
        assert!(f[1].cond.is_some());
    }

    #[test]
    fn s1_6_lineage_trace_through_inheritance_hierarchy() {
        let s = schema_from(
            r#"{"format":"nvre.nif.schema/1",
                "blocks":{
                  "NiObject":{"fields":[]},
                  "NiProperty":{"inherit":"NiObject","fields":[]},
                  "BSShaderProperty":{"inherit":"NiProperty","fields":[]},
                  "BSLightingShaderProperty":{"inherit":"BSShaderProperty","fields":[]}
                }}"#,
        );
        assert!(s.is_a("BSLightingShaderProperty", "BSLightingShaderProperty"));
        assert!(s.is_a("BSLightingShaderProperty", "BSShaderProperty"));
        assert!(s.is_a("BSLightingShaderProperty", "NiProperty"));
        assert!(s.is_a("BSLightingShaderProperty", "NiObject"));
        assert!(!s.is_a("BSLightingShaderProperty", "NiNode"));
    }
}
