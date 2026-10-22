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
    pub fn new(file: &NifFile, data: &[u8], schema: &Schema) -> Decoded {
        let mut blocks = Vec::with_capacity(file.blocks.len());
        let mut lineage = Vec::with_capacity(file.blocks.len());
        for (i, b) in file.blocks.iter().enumerate() {
            let (status, decoded) = file.decode_block_status(i, schema, data);
            blocks.push(decoded.ok_or(status));
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
        rotation: [
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0,
        ],
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
pub const SHAPE_TYPES: [&str; 4] = [
    "NiTriShape",
    "NiTriStrips",
    "BSSegmentedTriShape",
    // The Special Edition's shape block: it holds its vertices in the shape
    // itself, in a packed format, where the original edition keeps them in
    // a separate block.
    "BSTriShape",
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
        .map(|f| f.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
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
        return t
            .chunks_exact(3)
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
    l.chunks_exact(2)
        .map(|c| {
            [
                c[0].as_number().unwrap_or(0.0) as f32,
                c[1].as_number().unwrap_or(0.0) as f32,
            ]
        })
        .collect()
}

/// The vertex streams of a geometry-data block.
pub fn mesh_data(s: &Struct) -> MeshData {
    let uv_sets = s
        .list("uv_sets")
        .map(|sets| sets.iter().map(|set| uv_set(set)).collect())
        .unwrap_or_default();
    MeshData {
        positions: vec3s(s, "vertices"),
        normals: vec3s(s, "normals"),
        tangents: vec3s(s, "tangents"),
        bitangents: vec3s(s, "bitangents"),
        colors: floats(s, "vertex_colors")
            .map(|f| f.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
            .unwrap_or_default(),
        uv_sets,
        triangles: triangle_indices(s),
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
}
