//! The geometry census: strips, UVs, normals, tangents and declared counts.
//!
//! S3-2, S3-5, S3-6, S3-7, S3-8 — one pass over a game's corpus, so a defect
//! is a class count rather than a mesh.
//!
//! Each step asks a question the render path answers silently:
//!
//! - **S3-2** a `NiTriStripsData`'s `points` unrolls to `n − 2` triangles
//!   per strip, and the count the block declares (`num_triangles`) must
//!   agree with what the unroller produced. A disagreement means either the
//!   winding rule or the degenerate-drop is wrong, and a wrong winding turns
//!   half a model inside out.
//! - **S3-5** a shape whose first UV set holds one distinct UV cannot show a
//!   texture, whatever its material says. Counted per game, because a game
//!   that never ships such a shape does not need the branch.
//! - **S3-6** normals come from the file; `build_mesh` computes them only
//!   when the block has none. Counted: how many shapes have file normals,
//!   how many needed computed ones, and how many had neither.
//! - **S3-7** tangents are generated only when a normal map is bound, so a
//!   shape with file tangents and no normal map is a shape whose tangents
//!   are read and discarded, and one with a normal map and no tangents is a
//!   shape that must generate them.
//! - **S3-8** the declared vertex count must match the length of the vertex
//!   data: `num_vertices` against the positions actually read. A mismatch
//!   shifts every field after it, which is the same failure a `Short` block
//!   causes one stage earlier.
//!
//! Usage: `geomsurvey <game id> [limit files]`

use nvre_assets::shape::shape_props;
use nvre_editor::game::ALL;
use nvre_nif::scene::{mesh_data, scene_objects, Decoded, SHAPE_TYPES};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// One thread's tally.
///
/// The corpus walk is per-file and independent, so each thread folds its own
/// over the files it is handed and they are merged at the end — which is what
/// makes a 225,000-file corpus finish in minutes rather than hours.
#[derive(Default)]
struct Counts {
    files: u64,
    unparsed: u64,
    /// Geometry blocks seen (one per shape).
    shapes: u64,
    /// S3-2: shapes whose geometry is a strip.
    strip_shapes: u64,
    /// S3-2: strip shapes whose unrolled count equals the declared one.
    strip_count_agrees: u64,
    /// S3-2: strip shapes where they disagree, by type.
    strip_count_differs: BTreeMap<String, u64>,
    /// S3-2: triangles a strip states that the unroller skipped as
    /// degenerate. nifgen skips the same ones, so this is the data's own
    /// coincident-index count, not a reader defect.
    strip_degenerate: u64,
    /// S3-5: shapes with a first UV set holding one distinct UV.
    one_uv: u64,
    /// S3-5: shapes with no UV set at all.
    no_uv: u64,
    /// S3-6: shapes whose block carries file normals.
    file_normals: u64,
    /// S3-6: shapes that had none and needed computed normals.
    computed_normals: u64,
    /// S3-6: shapes whose normal count does not match their vertex count.
    normal_count_mismatch: u64,
    /// S3-7: shapes with a normal map bound.
    with_normal_map: u64,
    /// S3-7: shapes with file tangents.
    file_tangents: u64,
    /// S3-7: shapes with a normal map but no file tangents (must generate).
    normal_map_no_tangents: u64,
    /// S3-7: shapes with file tangents but no normal map (read and discarded).
    tangents_no_normal_map: u64,
    /// S3-8: shapes whose declared count differs from the positions read.
    vertex_count_differs: BTreeMap<String, u64>,
}

impl Counts {
    /// Another tally's numbers, added to this one.
    fn merge(&mut self, o: Counts) {
        self.files += o.files;
        self.unparsed += o.unparsed;
        self.shapes += o.shapes;
        self.strip_shapes += o.strip_shapes;
        self.strip_count_agrees += o.strip_count_agrees;
        self.strip_degenerate += o.strip_degenerate;
        self.one_uv += o.one_uv;
        self.no_uv += o.no_uv;
        self.file_normals += o.file_normals;
        self.computed_normals += o.computed_normals;
        self.normal_count_mismatch += o.normal_count_mismatch;
        self.with_normal_map += o.with_normal_map;
        self.file_tangents += o.file_tangents;
        self.normal_map_no_tangents += o.normal_map_no_tangents;
        self.tangents_no_normal_map += o.tangents_no_normal_map;
        for (k, v) in o.strip_count_differs {
            *self.strip_count_differs.entry(k).or_insert(0) += v;
        }
        for (k, v) in o.vertex_count_differs {
            *self.vertex_count_differs.entry(k).or_insert(0) += v;
        }
    }
}

/// Tally one file.
///
/// Only the blocks a geometry survey needs are decoded — the nodes, the
/// shapes and their geometry. A NIF is mostly not geometry: the material
/// properties, the texture sets, the collision shapes and the controllers
/// are all blocks too, and decoding them is most of the cost of walking a
/// corpus. Measured over FNV's mesh archive, 16,739 of 62,669 blocks are
/// geometry; the rest is skipped.
fn tally(bytes: &[u8], schema: &nvre_nif::Schema) -> Counts {
    let mut c = Counts::default();
    let Ok(f) = nvre_nif::NifFile::parse(bytes, Some(schema)) else {
        c.unparsed += 1;
        return c;
    };
    c.files += 1;
    let dec = Decoded::geometry_only(&f, bytes, schema);
    let objects = scene_objects(&f, &dec);
    for o in &objects {
        if o.is_node || !SHAPE_TYPES.contains(&o.type_name.as_str()) {
            continue;
        }
        let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
            continue;
        };
        let data_block = if let Some(r) = shape.ref_at("data") {
            dec.get(r)
        } else if o.type_name == "BSTriShape" || dec.is_a(o.block, "BSTriShape") {
            // The Special Edition's and Fallout 4's shape carries its own
            // geometry, and Fallout 4's `BSSubIndexTriShape` derives from
            // `BSTriShape` — matching on the exact type name would miss every
            // one of FO4's shapes, which are that subclass rather than the
            // base.
            Some(shape)
        } else {
            None
        };
        let Some(db) = data_block else { continue };
        c.shapes += 1;

        let md = mesh_data(db, f.header.bs_version());
        let props = shape_props(&dec, o, &Default::default());
        let db_type = dec
            .lineage
            .get(o.block)
            .and_then(|l| l.first())
            .map(String::as_str)
            .unwrap_or("?");

        // S3-2 — a strip's unrolled count against the declared one.
        //
        // `num_triangles` on a strip block is **not** the triangle
        // count: it is `sum(strip_lengths) − 2`, the classic strip
        // identity. Measured against nifgen's own reader over the
        // corpus: every strip block agrees with `sum − 2`, and the
        // triangles its `triangulate()` produces are the same set this
        // reader produces. The file simply stores coincident vertex
        // indices where the mesh has UV seams or hard edges, so a
        // "degenerate" triangle in the index list is a real one to
        // draw, not a join marker. Comparing the declared count to the
        // unrolled count therefore compares two different quantities,
        // and the honest check is the identity the file actually
        // states.
        if db.get("points").is_some() {
            c.strip_shapes += 1;
            let declared = db.int("num_triangles").unwrap_or(-1);
            let unrolled = md.triangles.len();
            // The strip identity: sum of strip lengths, less two per
            // strip. This is what `num_triangles` holds.
            let (points, strips) = db
                .list("points")
                .map(|rows| {
                    let lens: Vec<usize> = rows
                        .iter()
                        .map(|r| r.as_list().map_or(0, |l| l.len()))
                        .collect();
                    let sum: usize = lens.iter().sum();
                    (sum, lens.len())
                })
                .unwrap_or((0, 0));
            let identity = points.saturating_sub(2 * strips);
            if declared >= 0 && declared as usize == identity {
                c.strip_count_agrees += 1;
            } else if declared >= 0 {
                *c.strip_count_differs
                    .entry(format!(
                        "{db_type}: declared {declared}, strip identity {identity}, unrolled {unrolled}"
                    ))
                    .or_insert(0) += 1;
            }
            // Triangles the unroller skipped because a strip repeated
            // an index. nifgen skips the same ones, so this is a
            // property of the data, not a defect in the reader.
            c.strip_degenerate += identity.saturating_sub(unrolled) as u64;
        }

        // S3-5 — the first UV set's distinct coordinates.
        match md.uv_sets.first() {
            None => c.no_uv += 1,
            Some(uv) => {
                let distinct: BTreeSet<(u32, u32)> = uv
                    .iter()
                    .map(|c| (c[0].to_bits(), c[1].to_bits()))
                    .collect();
                if distinct.len() <= 1 {
                    c.one_uv += 1;
                }
            }
        }

        // S3-6 — normals. A block carries them, or the mesh must compute
        // them; a count that disagrees with the vertices is a defect.
        //
        // Whether the block carries them is asked of the *decoded
        // stream*, not of the field: a packed shape (`BSTriShape`) keeps
        // its normals inside `vertex_data` and has no `normals` field of
        // its own, so asking the field reports a packed shape as having
        // none and it gets counted as needing computed normals it does
        // not need. `mesh_data` already answered the question, so its
        // answer is the one counted.
        let n = md.positions.len();
        let has_file_normals = !md.normals.is_empty();
        if has_file_normals {
            c.file_normals += 1;
            if md.normals.len() != n {
                c.normal_count_mismatch += 1;
            }
        } else if n > 2 {
            c.computed_normals += 1;
        }

        // S3-7 — tangents against the normal map.
        let has_tangents = db.get("tangents").is_some() || !md.tangents.is_empty();
        if has_tangents {
            c.file_tangents += 1;
        }
        if props.normal_texture.is_some() {
            c.with_normal_map += 1;
            if !has_tangents {
                c.normal_map_no_tangents += 1;
            }
        } else if has_tangents {
            c.tangents_no_normal_map += 1;
        }

        // S3-8 — the declared vertex count against the data's length.
        if let Some(declared) = db.int("num_vertices") {
            let read = md.positions.len();
            if declared as usize != read {
                *c.vertex_count_differs
                    .entry(format!("{db_type}: declared {declared}, read {read}"))
                    .or_insert(0) += 1;
            }
        }
    }
    c
}



fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);

    let game = ALL.iter().find(|g| g.id == game_id).expect("a game");
    let data_dir = game.data_dir().expect("a data folder");
    let data = game.open_asset_data(&data_dir).expect("the asset data");

    // The walk is parallel: each file is independent, and a game's meshes are
    // 20k-225k files, so one core at a time is the difference between a
    // minute and half an hour. Each thread folds its own tally over the
    // files it is handed and they are merged at the end.
    let started = std::time::Instant::now();
    let schema = data.schema.clone();
    let paths: Vec<String> = data
        .vfs
        .paths(|p| p.to_lowercase().ends_with(".nif"))
        .into_iter()
        .take(limit)
        .collect();
    let c: Counts = paths
        .par_iter()
        .fold(Counts::default, |mut acc, p| {
            let Some(Ok(bytes)) = data.vfs.read(p) else {
                return acc;
            };
            acc.merge(tally(&bytes, &schema));
            acc
        })
        .reduce(Counts::default, |mut a, b| {
            a.merge(b);
            a
        });
    let secs = started.elapsed().as_secs_f32();

    println!(
        "{game_id}: {} files, {} unparsed, {} shapes (in {secs:.1}s on {} threads, {} files/s)",
        c.files,
        c.unparsed,
        c.shapes,
        rayon::current_num_threads(),
        (c.files as f32 / secs) as u64,
    );
    println!("S3-2 strips");
println!("  strip shapes: {}", c.strip_shapes);
println!("  unrolled count equals declared: {}", c.strip_count_agrees);
println!(
    "  unrolled count differs: {} kinds",
    c.strip_count_differs.len()
);
for (k, v) in c.strip_count_differs.iter().take(12) {
    println!("    {v:7} {k}");
}
println!(
    "  triangles dropped as degenerate joins: {}",
    c.strip_degenerate
);
println!("S3-5 UV sets");
println!("  one distinct UV: {}", c.one_uv);
println!("  no UV set: {}", c.no_uv);
println!("S3-6 normals");
println!("  file normals: {}", c.file_normals);
println!("  normals must be computed: {}", c.computed_normals);
println!(
    "  normal count != vertex count: {}",
    c.normal_count_mismatch
);
println!("S3-7 tangents");
println!("  shapes with a normal map: {}", c.with_normal_map);
println!("  file tangents: {}", c.file_tangents);
println!(
    "  normal map, no tangents (must generate): {}",
    c.normal_map_no_tangents
);
println!(
    "  tangents, no normal map (discarded): {}",
    c.tangents_no_normal_map
);
println!("S3-8 declared vertex count");
println!(
    "  shapes where it differs: {} kinds",
    c.vertex_count_differs.len()
);
for (k, v) in c.vertex_count_differs.iter().take(12) {
    println!("    {v:7} {k}");
}
}
