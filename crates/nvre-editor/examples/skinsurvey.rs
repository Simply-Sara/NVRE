//! The skinning census: how many shapes are skinned, with what.
//!
//! S12-1. `NiSkinInstance` is read nowhere in the render path — the schema
//! declares it (`schemas/nif/base/core.json`) and the scene reader walks past
//! it. This is the count that says how much of a game is affected: every
//! actor, every creature and every piece of armour is a skinned mesh, and a
//! game whose corpus holds none of them does not need the skinning path at
//! all.
//!
//! For each shape with a skin instance it records the bone count, the vertex
//! weights' count, and whether the skin's bones resolve to nodes that exist
//! in the same file. A skin whose `skeleton_root` is missing, or whose bones
//! name blocks that are not there, is a mesh that cannot be posed — and
//! counting it is what turns "the actor is a T-pose" into a number.
//!
//! Usage: `skinsurvey <game id> [limit files]`

use nvre_editor::game::ALL;
use nvre_nif::scene::{scene_objects, Decoded, SHAPE_TYPES};
use std::collections::BTreeMap;

#[derive(Default)]
struct Counts {
    files: u64,
    unparsed: u64,
    /// Shapes seen.
    shapes: u64,
    /// Shapes carrying a `skin_instance`.
    skinned: u64,
    /// Skinned shapes whose skin block failed to decode.
    skin_undecoded: u64,
    /// Skinned shapes with a `NiSkinData` block.
    with_skin_data: u64,
    /// Skinned shapes whose skin data failed to decode.
    skin_data_undecoded: u64,
    /// Skinned shapes with a `skin_partition`.
    with_partition: u64,
    /// Skinned shapes with no skeleton root.
    no_skeleton_root: u64,
    /// Skinned shapes whose skeleton root is not a node in the file.
    skeleton_root_missing: u64,
    /// Skinned shapes whose bone list does not resolve.
    bones_unresolved: u64,
    /// Skinned shapes whose bone count disagrees with the skin data's.
    bone_count_differs: u64,
    /// Total bones over every skinned shape.
    bones: u64,
    /// Bone counts, as a histogram.
    bone_hist: BTreeMap<u64, u64>,
    /// The types that carry a skin instance, by count.
    by_type: BTreeMap<String, u64>,
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

    let mut c = Counts::default();
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if c.files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            c.unparsed += 1;
            continue;
        };
        c.files += 1;
        let dec = Decoded::new(&f, &bytes, &data.schema);
        let objects = scene_objects(&f, &dec);
        let n = f.blocks.len();
        // Which blocks are nodes, so a bone list that names a non-node is
        // visible.
        let is_node: Vec<bool> = (0..n)
            .map(|i| {
                dec.lineage
                    .get(i)
                    .is_some_and(|l| l.iter().any(|t| t == "NiNode"))
            })
            .collect();

        for o in &objects {
            if o.is_node || !SHAPE_TYPES.contains(&o.type_name.as_str()) {
                continue;
            }
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
                continue;
            };
            c.shapes += 1;
            // The skin instance hangs off the shape, under one of two names:
            // `skin_instance` on the original edition's `NiGeometry`, and
            // `skin` on the Special Edition's and Fallout 4's `BSTriShape`.
            let skin_ref = shape
                .reference("skin_instance")
                .or_else(|| shape.reference("skin"));
            let Some(si) = skin_ref
                .and_then(|r| usize::try_from(r).ok())
                .filter(|&i| i < n)
            else {
                continue;
            };
            c.skinned += 1;
            *c.by_type.entry(o.type_name.clone()).or_insert(0) += 1;
            let Some(skin) = dec.get(si as i32) else {
                c.skin_undecoded += 1;
                continue;
            };
            // The skeleton root: a pointer to the node the skin is bound to.
            match skin.reference("skeleton_root") {
                None => c.no_skeleton_root += 1,
                Some(r) => match usize::try_from(r).ok().filter(|&i| i < n) {
                    None => c.skeleton_root_missing += 1,
                    Some(root) => {
                        if !is_node[root] {
                            c.skeleton_root_missing += 1;
                        }
                    }
                },
            }
            // The bones: a list of pointers. A bone that is not a node in
            // the file is a bone that cannot move.
            let bones = skin.refs("bones").unwrap_or_default();
            if bones.iter().any(|&b| {
                usize::try_from(b)
                    .map(|i| i >= n || !is_node[i])
                    .unwrap_or(true)
            }) {
                c.bones_unresolved += 1;
            }
            c.bones += bones.len() as u64;
            *c.bone_hist.entry(bones.len() as u64).or_insert(0) += 1;

            // The skin data: the bone transforms and the vertex weights.
            match skin.reference("data").and_then(|r| dec.get(r)) {
                None => c.skin_data_undecoded += 1,
                Some(sd) => {
                    c.with_skin_data += 1;
                    if let (Some(a), Some(b)) = (sd.int("num_bones"), skin.int("num_bones")) {
                        if a != b {
                            c.bone_count_differs += 1;
                        }
                    }
                }
            }
            // The partition: the Special Edition's and Fallout 4's skin
            // data, one per body part.
            if skin
                .reference("skin_partition")
                .and_then(|r| dec.get(r))
                .is_some()
            {
                c.with_partition += 1;
            }
        }
    }

    println!(
        "{game_id}: {} files, {} unparsed, {} shapes",
        c.files, c.unparsed, c.shapes
    );
    println!("S12-1 skin instances");
    println!("  skinned shapes: {}", c.skinned);
    println!("  with skin data: {}", c.with_skin_data);
    println!("  with a partition: {}", c.with_partition);
    println!("  skin block undecoded: {}", c.skin_undecoded);
    println!("  skin data undecoded: {}", c.skin_data_undecoded);
    println!("  no skeleton root: {}", c.no_skeleton_root);
    println!("  skeleton root not a node: {}", c.skeleton_root_missing);
    println!("  bone list unresolved: {}", c.bones_unresolved);
    println!(
        "  bone count differs from skin data: {}",
        c.bone_count_differs
    );
    println!("  total bones: {}", c.bones);
    if !c.bone_hist.is_empty() {
        println!("  bone count histogram (top 12):");
        let mut v: Vec<_> = c.bone_hist.into_iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (bones, count) in v.iter().take(12) {
            println!("    {count:8} shapes with {bones} bones");
        }
    }
    if !c.by_type.is_empty() {
        println!("  by shape type:");
        for (k, v) in c.by_type.iter() {
            println!("    {v:8} {k}");
        }
    }
}
