//! Typed views over decoded blocks: the scene graph (nodes and shapes with
//! their local transforms) and triangle geometry.
//!
//! Field names are those of `schemas/nif`; the object model is described in
//! `docs/spec-notes/nif.md` §3.

use crate::file::{BlockStatus, NifFile};
use crate::schema::Schema;
use crate::value::{Struct, Value};

/// Every block decoded; `Err` keeps the reason a block could not be decoded.
pub struct Decoded {
    /// The blocks, in block order.
    pub blocks: Vec<Result<Struct, BlockStatus>>,
    /// Lineage (type, then ancestors) per block; empty for unknown types.
    pub lineage: Vec<Vec<String>>,
}

impl Decoded {
    /// Decode every block of `file`.
    ///
    /// From 20.1.0.3 on a NIF's strings are not written inline: each one is
    /// a `u32` index into the file's string table, so the decoder has to
    /// resolve them to their text. That is done here, once, so that every
    /// reader downstream sees the text rather than the index — Fallout 4
    /// and the later Skyrim editions all use the table, and a block's
    /// `name` is what tells the scene graph and the shape list what a
    /// node is.
    pub fn new(file: &NifFile, data: &[u8], schema: &Schema) -> Decoded {
        let mut blocks = Vec::with_capacity(file.blocks.len());
        let mut lineage = Vec::with_capacity(file.blocks.len());
        for (i, b) in file.blocks.iter().enumerate() {
            let (status, decoded) = file.decode_block_status(i, schema, data);
            blocks.push(decoded.ok_or(status).map(|mut s| {
                let names = string_fields(schema, &b.type_name);
                resolve_strings(&mut s, file, &names);
                s
            }));
            // The lineage is the type itself, then the types it derives
            // from, nearest first.
            lineage.push(
                std::iter::once(b.type_name.clone())
                    .chain(
                        schema
                            .block_chain(&b.type_name)
                            .into_iter()
                            .map(str::to_string),
                    )
                    .collect(),
            );
        }
        Decoded { blocks, lineage }
    }

    /// Whether the block at `i` is of type `ty`, or of a type that derives
    /// from it.
    pub fn is_a(&self, i: usize, ty: &str) -> bool {
        self.lineage
            .get(i)
            .is_some_and(|l| l.iter().any(|t| t == ty))
    }
    /// The block at `i`, if it decoded.
    pub fn get(&self, i: i32) -> Option<&Struct> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.blocks.get(i))
            .and_then(|b| b.as_ref().ok())
    }
}

/// The names of the fields a block type declares as `string`.
///
/// The string table is indexed by fields the schema says are strings, and by
/// nothing else: a `f32` that happens to hold 1 is a scale of one, not the
/// first string in the table. The names are collected over the whole type
/// chain, and over the structs the block holds, so a texture set's file names
/// are found too.
fn string_fields(schema: &Schema, type_name: &str) -> Vec<String> {
    fn walk(schema: &Schema, ty: &str, out: &mut Vec<String>, seen: &mut Vec<String>) {
        if seen.iter().any(|s| s == ty) {
            return;
        }
        seen.push(ty.to_string());
        // A block's own fields, and the fields it inherits.
        let fields = schema
            .block_fields(ty)
            .or_else(|| schema.struct_fields(ty))
            .unwrap_or_default();
        for f in &fields {
            if matches!(
                f.ty,
                crate::schema::TypeRef::Prim(crate::schema::Prim::String)
            ) {
                out.push(f.name.clone());
            }
            // A field whose type is a struct: the names inside it are
            // strings of this block too.
            if let crate::schema::TypeRef::Named(n) = &f.ty {
                if schema.structs.contains_key(n) {
                    walk(schema, n, out, seen);
                }
            }
        }
        for parent in schema.block_chain(ty) {
            if parent != ty {
                walk(schema, parent, out, seen);
            }
        }
    }
    let mut out = Vec::new();
    walk(schema, type_name, &mut out, &mut Vec::new());
    out
}

/// Resolve a block's string fields from the file's string table.
///
/// A `string` field that the file version writes as an index arrives as a
/// `Value::Num`; the table's text at that index replaces it. A field that
/// is already text (an older file, or a type the schema reads inline) is
/// left alone, and an index past the end of the table is left as it was
/// rather than guessed at.
///
/// Only a field the schema declares as a `string` is resolved. Any numeric
/// field can hold a number that is a valid index into the table — a scale of
/// one, a glossiness of one, a count of nine — and resolving those replaced
/// them with unrelated text.
fn resolve_strings(s: &mut Struct, file: &NifFile, names: &[String]) {
    let Some(table) = file.header.strings.as_ref() else {
        return;
    };
    for (name, v) in s.fields.iter_mut() {
        if names.iter().any(|n| n == name) {
            if let Value::Num(n) = v {
                let i = *n as usize;
                if let Some(text) = table.get(i) {
                    *v = Value::Str(text.text());
                }
            }
        }
        // A struct that holds strings of its own: a texture set's file names,
        // a material's shader name, and the like.
        if let Value::Struct(inner) = v {
            resolve_strings(inner, file, names);
        }
    }
}

/// Local transform of a scene object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// The translation.
    pub translation: [f32; 3],
    /// The rotation, row-major 3×3.
    pub rotation: [f32; 9],
    /// The scale.
    pub scale: f32,
}

impl Transform {
    /// The transform that changes nothing.
    pub const IDENTITY: Transform = Transform {
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
    };

    /// The transform a block's fields hold.
    pub fn from_struct(s: &Struct) -> Option<Transform> {
        let t = floats(s, "translation")?;
        let r = floats(s, "rotation")?;
        Some(Transform {
            translation: [t[0], t[1], t[2]],
            rotation: r.try_into().ok()?,
            scale: s.num("scale")? as f32,
        })
    }
}

/// A node or a triangle shape in the scene graph.
#[derive(Debug, Clone)]
pub struct SceneObject {
    /// The block the object is.
    pub block: usize,
    /// The block's type.
    pub type_name: String,
    /// The object's name.
    pub name: Option<String>,
    /// The node the object hangs under.
    pub parent: Option<usize>,
    /// The object's transform, relative to its parent.
    pub transform: Transform,
    /// The object is a node, and may have children.
    pub is_node: bool,
    /// The object's children.
    pub children: Vec<usize>,
}

/// The blocks that are a shape.
pub const SHAPE_TYPES: [&str; 6] = [
    "NiTriShape",
    "NiTriStrips",
    "BSSegmentedTriShape",
    "BSLODTriShape",
    // The Special Edition's shape block: it holds its vertices in the shape
    // itself, in a packed format, where the original edition keeps them in
    // a separate block.
    "BSTriShape",
    // Fallout 4's shape block: it is a `BSTriShape` with its triangles
    // split into segments, one per body part or LOD level.
    "BSSubIndexTriShape",
];

/// The blocks that carry a shape's geometry.
///
/// The Special Edition's shape carries its own geometry, in the packed
/// vertex format, where the original edition keeps it in a separate block.
pub const SHAPE_DATA_TYPES: [&str; 3] = ["NiTriShapeData", "NiTriStripsData", "BSTriShape"];

/// Nodes (NiNode and derived types) and triangle shapes, in block order,
/// with each object's parent = the lowest-index node listing it as a child.
pub fn scene_objects(file: &NifFile, dec: &Decoded) -> Vec<SceneObject> {
    let n = file.blocks.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        if !dec.is_a(i, "NiNode") {
            continue;
        }
        if let Some(children) = dec.blocks[i].as_ref().ok().and_then(|s| s.refs("children")) {
            for c in children {
                if let Ok(c) = usize::try_from(c) {
                    if c < n && parent[c].is_none() {
                        parent[c] = Some(i);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        let is_node = dec.is_a(i, "NiNode");
        if !is_node && !SHAPE_TYPES.contains(&b.type_name.as_str()) {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let children = if is_node {
            s.refs("children")
                .unwrap_or_default()
                .iter()
                .filter_map(|&c| usize::try_from(c).ok())
                .filter(|&c| c < n)
                .collect()
        } else {
            Vec::new()
        };
        out.push(SceneObject {
            block: i,
            type_name: b.type_name.clone(),
            name: s.text("name").map(str::to_string),
            parent: parent[i],
            transform: Transform::from_struct(s).unwrap_or(Transform::IDENTITY),
            is_node,
            children,
        });
    }
    out
}

/// Summary of one geometry-data block (NiTriShapeData / NiTriStripsData).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSummary {
    /// The block.
    pub block: usize,
    /// The block's type.
    pub type_name: String,
    /// How many vertices it holds.
    pub vertices: usize,
    /// How many triangles it holds.
    pub triangles: usize,
    /// Whether it holds normals.
    pub has_normals: bool,
    /// Whether it holds tangents.
    pub has_tangents: bool,
    /// How many UV sets it holds.
    pub uv_sets: usize,
    /// Whether it holds vertex colours.
    pub has_vertex_colors: bool,
    /// The centre of its bounding sphere.
    pub bound_center: [f32; 3],
    /// The radius of its bounding sphere.
    pub bound_radius: f32,
    /// Its bounding box, if it has vertices.
    pub aabb: Option<([f32; 3], [f32; 3])>,
}

/// The numbers a field holds.
///
/// A field holds its numbers either as a flat list, or as a list of lists —
/// one per vertex, say. Both are read the same way.
fn floats(s: &Struct, name: &str) -> Option<Vec<f32>> {
    let l = s.list(name)?;
    // A list of lists: each entry holds several numbers.
    if l.iter().any(|v| v.as_list().is_some()) {
        let mut out: Vec<f32> = Vec::with_capacity(l.len());
        for v in l {
            if let Some(inner) = v.as_list() {
                out.extend(inner.iter().filter_map(|n| n.as_number()).map(|n| n as f32));
            } else if let Some(n) = v.as_number() {
                out.push(n as f32);
            }
        }
        return Some(out);
    }
    Some(
        l.iter()
            .filter_map(|v| v.as_number())
            .map(|n| n as f32)
            .collect(),
    )
}

/// The numbers of a field, in groups of three.
fn vec3s(s: &Struct, name: &str) -> Vec<[f32; 3]> {
    floats(s, name)
        .map(|f| {
            f.as_chunks::<3>()
                .0
                .iter()
                .map(|c| [c[0], c[1], c[2]])
                .collect()
        })
        .unwrap_or_default()
}

/// A summary of a geometry-data block.
pub fn shape_summary(block: usize, type_name: &str, s: &Struct) -> ShapeSummary {
    let verts = vec3s(s, "vertices");
    let aabb = if verts.is_empty() {
        None
    } else {
        let mut lo = verts[0];
        let mut hi = verts[0];
        for v in &verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        Some((lo, hi))
    };
    let sphere = s.struct_at("bounding_sphere");
    let c = sphere
        .and_then(|b| floats(b, "center"))
        .filter(|c| c.len() == 3)
        .unwrap_or_else(|| vec![0.0, 0.0, 0.0]);
    ShapeSummary {
        block,
        type_name: type_name.to_string(),
        vertices: s.int("num_vertices").map_or(verts.len(), |n| n as usize),
        triangles: triangle_count(s),
        has_normals: s.get("normals").is_some(),
        has_tangents: s.get("tangents").is_some(),
        uv_sets: s.list("uv_sets").map_or(0, |l| l.len()),
        has_vertex_colors: s.get("vertex_colors").is_some(),
        bound_center: [c[0], c[1], c[2]],
        bound_radius: sphere.and_then(|b| b.num("radius")).unwrap_or(0.0) as f32,
        aabb,
    }
}

/// Triangles described by the index data (strips count n − 2 per strip,
/// including degenerate joins).
pub fn triangle_count(s: &Struct) -> usize {
    if let Some(t) = s.list("triangles") {
        if t.iter().any(|v| v.as_list().is_some()) {
            return t.len();
        }
        return t.len() / 3;
    }
    if let Some(rows) = s.list("points") {
        return rows
            .iter()
            .map(|r| r.as_list().map_or(0, |l| l.len().saturating_sub(2)))
            .sum();
    }
    0
}

/// Triangle list (vertex indices) from a shape-data block. Strips are
/// unrolled with every second triangle's winding reversed; degenerate
/// triangles are dropped.
pub fn triangle_indices(s: &Struct) -> Vec<[u32; 3]> {
    if let Some(t) = s.list("triangles") {
        if t.iter().any(|v| v.as_list().is_some()) {
            return t
                .iter()
                .filter_map(|v| {
                    let inner = v.as_list()?;
                    if inner.len() < 3 {
                        return None;
                    }
                    Some([
                        inner[0].as_int().unwrap_or(0) as u32,
                        inner[1].as_int().unwrap_or(0) as u32,
                        inner[2].as_int().unwrap_or(0) as u32,
                    ])
                })
                .collect();
        }
        return t
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                [
                    c[0].as_int().unwrap_or(0) as u32,
                    c[1].as_int().unwrap_or(0) as u32,
                    c[2].as_int().unwrap_or(0) as u32,
                ]
            })
            .collect();
    }
    let mut out = Vec::new();
    if let Some(rows) = s.list("points") {
        for r in rows {
            let Some(p) = r.as_list() else { continue };
            for k in 2..p.len() {
                let (a, b, c) = (
                    p[k - 2].as_int().unwrap_or(0) as u32,
                    p[k - 1].as_int().unwrap_or(0) as u32,
                    p[k].as_int().unwrap_or(0) as u32,
                );
                if a == b || b == c || a == c {
                    continue;
                }
                out.push(if k % 2 == 0 { [a, b, c] } else { [a, c, b] });
            }
        }
    }
    out
}

/// Vertex streams of a shape-data block, ready for upload.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    /// The positions.
    pub positions: Vec<[f32; 3]>,
    /// The normals.
    pub normals: Vec<[f32; 3]>,
    /// The tangents.
    pub tangents: Vec<[f32; 3]>,
    /// The bitangents.
    pub bitangents: Vec<[f32; 3]>,
    /// The vertex colours.
    pub colors: Vec<[f32; 4]>,
    /// One vec per UV set.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    /// The triangles.
    pub triangles: Vec<[u32; 3]>,
}

/// One UV set: one vec2 per vertex.
///
/// A UV set is a list of vertices, and each vertex is a list of two numbers;
/// a set that is a flat list of numbers is read in pairs.
fn uv_set(set: &Value) -> Vec<[f32; 2]> {
    let Some(l) = set.as_list() else {
        return Vec::new();
    };
    // A list of vertices, each a list of two numbers.
    if l.iter().any(|v| v.as_list().is_some()) {
        return l
            .iter()
            .map(|v| {
                let c = v.as_list().unwrap_or(&[]);
                [
                    c.first().and_then(|n| n.as_number()).unwrap_or(0.0) as f32,
                    c.get(1).and_then(|n| n.as_number()).unwrap_or(0.0) as f32,
                ]
            })
            .collect();
    }
    l.as_chunks::<2>()
        .0
        .iter()
        .map(|c| {
            [
                c[0].as_number().unwrap_or(0.0) as f32,
                c[1].as_number().unwrap_or(0.0) as f32,
            ]
        })
        .collect()
}

/// The vertex streams of a geometry-data block.
///
/// `bs_version` decides which packed vertex format the shape uses: the
/// Special Edition (bs_version 100) writes `BSVertexDataSSE`, and Fallout 4
/// (bs_version 130 and up) writes `BSVertexData` — the same fields, but the
/// position is a half-precision vector unless the description's
/// `FULL_PRECISION` flag says otherwise.
pub fn mesh_data(s: &Struct, bs_version: u32) -> MeshData {
    // A shape may carry its geometry in a packed format: a vertex
    // description (flags) followed by the packed bytes. Its layout is a
    // function of the description, so it is unpacked here.
    if let (Some(desc), Some(data)) = (
        s.int("vertex_desc"),
        s.get("vertex_data").and_then(|v| v.as_bytes()),
    ) {
        return unpack_sse(desc as u64, data, s, bs_version);
    }
    let uv_sets = s
        .list("uv_sets")
        .map(|sets| sets.iter().map(uv_set).collect())
        .unwrap_or_default();
    MeshData {
        positions: vec3s(s, "vertices"),
        normals: vec3s(s, "normals"),
        tangents: vec3s(s, "tangents"),
        bitangents: vec3s(s, "bitangents"),
        colors: floats(s, "vertex_colors")
            .map(|f| {
                f.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect()
            })
            .unwrap_or_default(),
        uv_sets,
        triangles: triangle_indices(s),
    }
}

/// Unpack a `BSTriShape`'s packed vertices.
///
/// The description (nifgen's `BSVertexDesc`) is a bitfield of word offsets
/// and flags: its low forty-four bits are word offsets (bits 0-3 the
/// vertex's size in four-byte words, then the UV, normal, tangent, colour,
/// skinning, land and eye offsets), and bits 44 and up are
/// `vertex_attributes` — the `VertexAttribute` flags that say which of
/// those fields are present at all.
///
/// So the offsets say where a field sits, and the flags say whether it is
/// there; a field whose flag is off is not in the vertex at all, whatever
/// its offset says. Both halves are read here: the flags decide what to
/// unpack, and the offsets decide where each one starts, because they do
/// not always agree with a fixed field order. Skyrim SE's static
/// architecture packs them in nifgen's order (position, uv, normal,
/// tangent, colour), which is also the order the offsets describe, but
/// Fallout 4 and other meshes use a compact layout — a vertex with UV,
/// normals and tangents but no colours and no position word of its own
/// packs them at words 2, 3 and 4, with the vertex's three floats ending
/// at word 2, so the total is five words rather than the six a fixed
/// order would need.
///
/// The bitangent travels packed as two Normbytes (y, z) beside the normal
/// and the tangent; a Normbyte maps an unsigned byte to -1..1 as
/// `b / 127.5 - 1`.
fn unpack_sse(desc: u64, data: &[u8], s: &Struct, bs_version: u32) -> MeshData {
    let stride = (desc & 0x0F) as usize * 4;
    if stride == 0 {
        return MeshData::default();
    }
    let n = s
        .int("num_vertices")
        .map(|n| n as usize)
        .unwrap_or_else(|| data.len() / stride);
    let mut md = MeshData::default();
    // One UV set for the Special Edition and for Fallout 4.
    md.uv_sets.push(Vec::with_capacity(n));
    // The attribute flags live in the description's high bits, not its low
    // ones: the low bits are the word offsets the flags describe.
    let attrs = desc >> 44;
    let has_pos = attrs & 1 != 0;
    let has_bitangent_x = attrs & 17 == 17;
    let has_unused_w = attrs & 17 == 1;
    let has_uv = attrs & 2 != 0;
    let has_normal = attrs & 8 != 0;
    let has_tangent = attrs & 24 == 24;
    let has_colors = attrs & 32 != 0;
    let has_bones = attrs & 64 != 0;
    let _has_eye = attrs & 256 != 0;
    // Where each field sits, in four-byte words from the vertex's start.
    let word = |shift: u32| ((desc >> shift) & 0xF) as usize;
    let (uv, normal, tangent, color, skin, eye) =
        (word(8), word(16), word(20), word(24), word(28), word(36));
    for i in 0..n {
        let base = i * stride;
        if base + stride > data.len() {
            break;
        }
        let v = &data[base..base + stride];
        // A field is read only when its flag is on and it fits inside the
        // vertex, so a description that disagrees with itself cannot read
        // past the end.
        let bytes = |at: usize, len: usize| v.get(at..at + len);
        // position: a Vector3 (3 floats) at word 0, or — in Fallout 4's
        // `BSVertexData` — a HalfVector3 of three half floats unless the
        // description's FULL_PRECISION flag asks for the wide form.
        //
        // The version is what decides which of the two structures is in
        // play: Skyrim SE (bs_version 100) writes `BSVertexDataSSE`, whose
        // position is always a Vector3, and Fallout 4 (bs_version 130 and
        // up) writes `BSVertexData`, whose position is a HalfVector3 unless
        // FULL_PRECISION is set. Measured: every desc in Skyrim SE's archives
        // has FULL_PRECISION clear, and its positions are still four-byte
        // floats — so the flag alone cannot be the test.
        let half_position = bs_version >= 130 && attrs & 1024 == 0;
        if has_pos {
            if half_position {
                if let Some(b) = bytes(0, 6) {
                    let x = half_to_f32(u16::from_le_bytes([b[0], b[1]]));
                    let y = half_to_f32(u16::from_le_bytes([b[2], b[3]]));
                    let z = half_to_f32(u16::from_le_bytes([b[4], b[5]]));
                    md.positions.push([x, y, z]);
                } else {
                    md.positions.push([0.0, 0.0, 0.0]);
                }
            } else if let Some(b) = bytes(0, 12) {
                let x = f32::from_le_bytes(b[0..4].try_into().unwrap());
                let y = f32::from_le_bytes(b[4..8].try_into().unwrap());
                let z = f32::from_le_bytes(b[8..12].try_into().unwrap());
                md.positions.push([x, y, z]);
            } else {
                md.positions.push([0.0, 0.0, 0.0]);
            }
        } else {
            md.positions.push([0.0, 0.0, 0.0]);
        }
        // uv: HalfTexCoord (2 half floats).
        if has_uv {
            let at = uv * 4;
            if let Some(b) = bytes(at, 4) {
                let u = half_to_f32(u16::from_le_bytes([b[0], b[1]]));
                let w = half_to_f32(u16::from_le_bytes([b[2], b[3]]));
                md.uv_sets[0].push([u, w]);
            } else {
                md.uv_sets[0].push([0.0, 0.0]);
            }
        } else {
            md.uv_sets[0].push([0.0, 0.0]);
        }
        // normal: ByteVector3 (3 Normbytes) + bitangent_y Normbyte.
        if has_normal {
            let at = normal * 4;
            if let Some(b) = bytes(at, 4) {
                md.normals
                    .push([normbyte(b[0]), normbyte(b[1]), normbyte(b[2])]);
            } else {
                md.normals.push([0.0, 0.0, 1.0]);
            }
        } else {
            md.normals.push([0.0, 0.0, 1.0]);
        }
        // tangent: ByteVector3 (3 Normbytes) + bitangent_z Normbyte.
        if has_tangent {
            let at = tangent * 4;
            if let Some(b) = bytes(at, 4) {
                md.tangents
                    .push([normbyte(b[0]), normbyte(b[1]), normbyte(b[2])]);
            }
        }
        // vertex colours: ByteColor4.
        if has_colors {
            let at = color * 4;
            if let Some(b) = bytes(at, 4) {
                md.colors.push([
                    b[0] as f32 / 255.0,
                    b[1] as f32 / 255.0,
                    b[2] as f32 / 255.0,
                    b[3] as f32 / 255.0,
                ]);
            }
        }
        // The skinning and eye data are read by the skinning code, not
        // here; the offsets are named only so the reader that needs them
        // has a place to start from.
        let _ = (skin, eye, has_bones, has_bitangent_x, has_unused_w);
    }
    md.triangles = triangle_indices(s);
    md
}

/// A Normbyte: an unsigned byte standing for a signed normal, as nifgen's
/// `Normbyte.from_function` reads it. The Special Edition packs its normals
/// and tangents in these.
fn normbyte(b: u8) -> f32 {
    b as f32 / 127.5 - 1.0
}

fn half_to_f32(h: u16) -> f32 {
    // IEEE 754 half
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1F;
    let mant = h & 0x3FF;
    if exp == 0 {
        let f = mant as f32 / 1024.0 * (1.0 / 16384.0);
        if sign == 1 {
            -f
        } else {
            f
        }
    } else if exp == 31 {
        if mant != 0 {
            f32::NAN
        } else if sign == 1 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        }
    } else {
        f32::from_bits(
            ((sign as u32) << 31) | (((exp as i32 + 112) as u32) << 23) | ((mant as u32) << 13),
        )
    }
}

/// The triangles of each segment of a BSSegmentedTriShape, as ranges into
/// its triangle list of `triangles` triangles: `start_index` counts indices,
/// three per triangle, and `num_primitives` triangles [documented:
/// nif.xml BSGeometrySegmentData]. `None` when the shape has no segments or
/// they do not tile the triangle list in order (each non-empty segment
/// starting where the one before ends, all of them together covering it), as
/// they do in all 355 object LOD block shapes of `Fallout - Meshes.bsa`
/// [observed: `examples/lod_segment_survey.rs`].
pub fn segment_triangles(shape: &Struct, triangles: usize) -> Option<Vec<std::ops::Range<usize>>> {
    let mut out = Vec::new();
    let mut next = 0;
    for seg in shape.list("segment")? {
        let Value::Struct(seg) = seg else {
            return None;
        };
        let (start, n) = (
            usize::try_from(seg.int("start_index")?).ok()?,
            usize::try_from(seg.int("num_primitives")?).ok()?,
        );
        if n > 0 && start != 3 * next {
            return None;
        }
        out.push(next..next + n);
        next += n;
    }
    (!out.is_empty() && next == triangles).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(segments: &[(i64, i64)]) -> Struct {
        let seg = |&(start, n): &(i64, i64)| {
            Value::Struct(Struct {
                fields: vec![
                    ("flags".into(), Value::Num(0.0)),
                    ("start_index".into(), Value::Num(start as f64)),
                    ("num_primitives".into(), Value::Num(n as f64)),
                ],
            })
        };
        Struct {
            fields: vec![(
                "segment".into(),
                Value::List(segments.iter().map(seg).collect()),
            )],
        }
    }

    /// A shape-data block with one triangle strip, holding `points`.
    fn strip(points: &[u16]) -> Struct {
        Struct {
            fields: vec![(
                "points".into(),
                Value::List(vec![Value::List(
                    points.iter().map(|&p| Value::Num(p as f64)).collect(),
                )]),
            )],
        }
    }

    #[test]
    fn a_triangle_strip_unrolls_to_a_triangle_list() {
        // A strip of four points is two triangles: (0,1,2) and (1,2,3).
        let tris = triangle_indices(&strip(&[0, 1, 2, 3]));
        assert_eq!(tris.len(), 2);
        assert_eq!(tris[0], [0, 1, 2]);
        // The second triangle's winding is reversed, because the strip
        // alternates: without the reversal the two triangles face opposite
        // ways and half of the model is invisible.
        assert_eq!(tris[1], [1, 3, 2]);
    }

    #[test]
    fn a_degenerate_join_is_dropped_from_a_strip() {
        // A strip that repeats a vertex to join two runs: the repeated
        // vertex makes a triangle with two of the same index, which is not
        // a triangle at all.
        let tris = triangle_indices(&strip(&[0, 1, 2, 2, 3, 4]));
        // Every triangle that came out must name three different vertices.
        for t in &tris {
            assert!(
                t[0] != t[1] && t[1] != t[2] && t[0] != t[2],
                "{t:?} names the same vertex twice"
            );
        }
    }

    #[test]
    fn a_strip_of_two_points_is_no_triangle() {
        assert!(triangle_indices(&strip(&[0, 1])).is_empty());
        assert!(triangle_indices(&strip(&[])).is_empty());
    }

    #[test]
    fn a_triangle_list_is_read_as_it_is() {
        // A shape that has a triangle list rather than a strip: the three
        // indices of each triangle, in order.
        let s = Struct {
            fields: vec![(
                "triangles".into(),
                Value::List(vec![
                    Value::List(vec![Value::Num(0.0), Value::Num(1.0), Value::Num(2.0)]),
                    Value::List(vec![Value::Num(2.0), Value::Num(1.0), Value::Num(3.0)]),
                ]),
            )],
        };
        assert_eq!(
            triangle_indices(&s),
            vec![[0, 1, 2], [2, 1, 3]],
            "a triangle list must not have its winding reversed"
        );
    }

    #[test]
    fn segments_tile_the_triangle_list() {
        // Start indices count indices; an empty segment's start is not looked
        // at.
        assert_eq!(
            segment_triangles(&shape(&[(0, 2), (0, 0), (6, 3)]), 5),
            Some(vec![0..2, 2..2, 2..5])
        );
        // Not a segmented shape, a gap, an overlap, triangles left over.
        assert_eq!(segment_triangles(&Struct::default(), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (9, 2)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (3, 3)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (6, 2)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[]), 0), None);
    }

    /// A packed vertex in the Special Edition's layout, as `desc` describes.
    ///
    /// Built the way the game writes them: position (3 floats), then the
    /// fields the description's `vertex_attributes` flags name, in order,
    /// with the normals and tangents as Normbytes.
    fn packed(desc: u64, vertex: [f32; 3], normal: [u8; 3], tangent: [u8; 3]) -> Vec<u8> {
        let mut v = Vec::new();
        for c in vertex {
            v.extend_from_slice(&c.to_le_bytes());
        }
        // The description's flags say which fields follow; the test's
        // description names UV, NORMALS, TANGENTS and COLORS, so the UV
        // comes before the normal.
        v.extend_from_slice(&[0, 0, 0, 0]); // uv, half floats
        v.extend_from_slice(&normal);
        v.push(127); // bitangent_y
        v.extend_from_slice(&tangent);
        v.push(127); // bitangent_z
        v.extend_from_slice(&[255, 255, 255, 255]); // vertex_colors
                                                    // The stride is the vertex's size in words, so pad to it.
        let stride = (desc & 0xF) as usize * 4;
        while v.len() < stride {
            v.push(0);
        }
        v
    }

    fn shape_with_packed_vertices(desc: u64, verts: &[Vec<u8>], n: usize) -> Struct {
        Struct {
            fields: vec![
                ("num_vertices".into(), Value::Num(n as f64)),
                ("vertex_desc".into(), Value::Num(desc as f64)),
                (
                    "vertex_data".into(),
                    Value::Bytes(verts.iter().flatten().copied().collect()),
                ),
            ],
        }
    }

    #[test]
    fn packed_vertices_read_the_attribute_flags_from_the_high_bits() {
        // The layout the common static architecture uses: VERTEX, UV,
        // NORMALS, TANGENTS, COLORS, stride eight words. Its low bits are the
        // word offsets (uv at 4, normal at 5, tangent at 6, colour at 7);
        // its high bits are the attribute flags. Reading the flags out of the
        // low bits instead reads the offsets, and every field comes out
        // missing: the shape is drawn at the origin, or not at all.
        let desc = 0x3b00007650408;
        let s = shape_with_packed_vertices(
            desc,
            &[packed(
                desc,
                [1.0, 2.0, 3.0],
                [127, 127, 255], // +x, +y, +z, roughly
                [127, 127, 255],
            )],
            1,
        );
        let md = mesh_data(&s, 100);
        assert_eq!(md.positions, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(md.normals.len(), 1);
        // Normbytes map an unsigned byte to -1..1: 127 is (almost) 0 and 255
        // is 1, so this is a normal pointing up and to the right.
        let n = md.normals[0];
        assert!((n[0] - 0.0).abs() < 0.01, "{n:?}");
        assert!((n[1] - 0.0).abs() < 0.01, "{n:?}");
        assert!((n[2] - 1.0).abs() < 0.01, "{n:?}");
    }

    #[test]
    fn a_normbyte_is_an_unsigned_byte_scaled_to_signed() {
        // 0 is -1 and 255 is 1: the mapping is unsigned, so a normal of
        // exactly -1 is a byte of 0, not 0x80.
        assert_eq!(normbyte(0), -1.0);
        assert!((normbyte(127) - 0.0).abs() < 0.01);
        assert_eq!(normbyte(255), 1.0);
    }

    #[test]
    fn packed_vertices_honour_the_stride_and_count() {
        // A shape of two vertices: the stride decides where each begins, so
        // a vertex that is not a whole number of strides from the start is
        // never read.
        let desc = 0x3b00007650408;
        let verts = vec![
            packed(desc, [10.0, 0.0, 0.0], [127, 127, 255], [127, 127, 255]),
            packed(desc, [20.0, 0.0, 0.0], [127, 127, 255], [127, 127, 255]),
        ];
        let s = shape_with_packed_vertices(desc, &verts, 2);
        let md = mesh_data(&s, 100);
        assert_eq!(md.positions, vec![[10.0, 0.0, 0.0], [20.0, 0.0, 0.0]]);
        assert_eq!(md.uv_sets[0].len(), 2);
    }

    #[test]
    fn a_description_with_no_size_yields_no_geometry() {
        // A stride of zero says the vertices have no words at all, so there
        // is nothing to unpack rather than a zero-length vertex.
        let s = shape_with_packed_vertices(0, &[vec![0u8; 4]], 1);
        assert!(mesh_data(&s, 100).positions.is_empty());
    }

    #[test]
    fn s3_3_and_s3_4_packed_vertex_precision_and_game_version() {
        // Fallout 4 BSVertexData (bs_version >= 130) without FULL_PRECISION (bit 10 of attrs = 1024):
        // Position is HalfVector3 (3 x 16-bit half floats, 6 bytes).
        // 1.0 in f16 is 0x3C00, 2.0 is 0x4000, -3.0 is 0xC200.
        let desc_fo4_half = (1u64 << 44) | 4; // attrs = 1 (has_pos), stride = 4 words (16 bytes)
        let mut vert_fo4_half = vec![0u8; 16];
        vert_fo4_half[0..2].copy_from_slice(&0x3C00u16.to_le_bytes()); // 1.0
        vert_fo4_half[2..4].copy_from_slice(&0x4000u16.to_le_bytes()); // 2.0
        vert_fo4_half[4..6].copy_from_slice(&0xC200u16.to_le_bytes()); // -3.0

        let s_fo4_half = shape_with_packed_vertices(desc_fo4_half, &[vert_fo4_half], 1);
        let md_fo4_half = mesh_data(&s_fo4_half, 130);
        assert_eq!(md_fo4_half.positions, vec![[1.0, 2.0, -3.0]]);

        // Fallout 4 BSVertexData with FULL_PRECISION (bit 10 set, 1024):
        // attrs = 1 | 1024 = 1025 (0x401 in hex)
        let desc_fo4_full = (1025u64 << 44) | 4; // stride 4 words (16 bytes)
        let mut vert_fo4_full = vec![0u8; 16];
        vert_fo4_full[0..4].copy_from_slice(&1.5f32.to_le_bytes());
        vert_fo4_full[4..8].copy_from_slice(&2.5f32.to_le_bytes());
        vert_fo4_full[8..12].copy_from_slice(&(-3.5f32).to_le_bytes());

        let s_fo4_full = shape_with_packed_vertices(desc_fo4_full, &[vert_fo4_full], 1);
        let md_fo4_full = mesh_data(&s_fo4_full, 130);
        assert_eq!(md_fo4_full.positions, vec![[1.5, 2.5, -3.5]]);

        // Skyrim SE (bs_version 100): even with FULL_PRECISION bit clear (attrs = 1),
        // Skyrim SE BSVertexDataSSE ALWAYS uses full f32 floats (Vector3).
        let desc_skyrimse = (1u64 << 44) | 4;
        let mut vert_sse = vec![0u8; 16];
        vert_sse[0..4].copy_from_slice(&1.5f32.to_le_bytes());
        vert_sse[4..8].copy_from_slice(&2.5f32.to_le_bytes());
        vert_sse[8..12].copy_from_slice(&(-3.5f32).to_le_bytes());

        let s_sse = shape_with_packed_vertices(desc_skyrimse, &[vert_sse], 1);
        let md_sse = mesh_data(&s_sse, 100);
        assert_eq!(md_sse.positions, vec![[1.5, 2.5, -3.5]]);
    }

    #[test]
    fn s1_5_resolve_strings_only_resolves_declared_string_fields() {
        let mut st = Struct::new();
        st.set("name", Value::Num(0.0));
        st.set("scale", Value::Num(1.0));

        let header = crate::header::Header {
            header_string: Vec::new(),
            version: 0,
            endian: 1,
            user_version: 0,
            num_blocks: 0,
            bs: None,
            block_types: Vec::new(),
            block_type_index: Vec::new(),
            block_sizes: None,
            max_string_length: None,
            strings: Some(vec![
                crate::header::RawString(b"TargetString0".to_vec()),
                crate::header::RawString(b"TargetString1".to_vec()),
            ]),
            groups: Vec::new(),
            header_len: 0,
        };
        let file = NifFile {
            header,
            blocks: Vec::new(),
            trailing: 0,
        };

        // Only "name" is declared as a string field:
        let string_names = vec!["name".to_string()];
        resolve_strings(&mut st, &file, &string_names);

        // 'name' must be resolved to "TargetString0"
        assert_eq!(st.text("name"), Some("TargetString0"));
        // 'scale' must REMAIN a number (1.0) and NOT be replaced by "TargetString1"
        assert_eq!(st.num("scale"), Some(1.0));
    }
}
