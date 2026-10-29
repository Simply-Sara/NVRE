//! Does every shape hang off exactly one root, and does every node have a
//! parent that is a node?
//!
//! S2-1, over the corpus. The scene reader resolves a shape's place in the
//! graph from the `children` lists the nodes carry, so three things can go
//! wrong and none of them shows up as an error:
//!
//! - A shape that no root reaches is never drawn, and nothing says so.
//! - A node whose parent is not a node has no transform to inherit, so its
//!   children sit at the origin.
//! - A block two parents list belongs to the first and is silently dropped
//!   from the second.
//!
//! Usage: `nifgraph <game id> [limit]`
//!
//! The limit is the number of files to walk; the default is every file.

use nvre_editor::game::ALL;
use nvre_nif::scene::{scene_objects, Decoded, SHAPE_TYPES};
use std::collections::BTreeMap;

/// What was found.
#[derive(Default)]
struct Counts {
    files: u64,
    /// Files that did not parse.
    unparsed: u64,
    /// Shapes with no path to a root.
    orphaned_shapes: u64,
    /// Nodes whose parent is not a node.
    nodes_with_a_non_node_parent: u64,
    /// A block two parents list.
    blocks_with_two_parents: u64,
    /// A child index that points outside the file.
    children_out_of_range: u64,
    /// Nodes with a name.
    named_nodes: u64,
    /// Shapes with a name.
    named_shapes: u64,
    /// Nodes whose name is still an index rather than text.
    unresolved_names: u64,
    /// Shapes.
    shapes: u64,
    /// Nodes.
    nodes: u64,
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

    let mut counts = Counts::default();
    // Which block types are orphaned, so a type that is always orphaned is
    // visible rather than a count.
    let mut orphaned: BTreeMap<String, u64> = BTreeMap::new();

    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if counts.files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(file) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            counts.unparsed += 1;
            continue;
        };
        counts.files += 1;
        let dec = Decoded::new(&file, &bytes, &data.schema);
        let objects = scene_objects(&file, &dec);
        if objects.is_empty() {
            continue;
        }

        let n = file.blocks.len();
        // Which blocks are listed as a child, and by how many.
        let mut listed_by: BTreeMap<usize, usize> = BTreeMap::new();
        let mut parent_of: BTreeMap<usize, usize> = BTreeMap::new();
        for o in &objects {
            if !o.is_node {
                continue;
            }
            counts.nodes += 1;
            for &c in &o.children {
                if c >= n {
                    counts.children_out_of_range += 1;
                    continue;
                }
                *listed_by.entry(c).or_insert(0) += 1;
                // The first parent wins, which is what the reader does.
                parent_of.entry(c).or_insert(o.block);
            }
        }

        for (_block, times) in &listed_by {
            if *times > 1 {
                counts.blocks_with_two_parents += 1;
            }
        }

        // A node's parent must itself be a node.
        let is_node: BTreeMap<usize, bool> = objects.iter().map(|o| (o.block, o.is_node)).collect();
        for o in &objects {
            // S2-5: a name that survived the string table is text, and a
            // name that did not is still the index it was written as.
            if o.is_node {
                if o.name.is_some() {
                    counts.named_nodes += 1;
                }
                if let Some(s) = dec.get(o.block as i32) {
                    if s.get("name").is_some_and(|v| v.as_str().is_none()) {
                        counts.unresolved_names += 1;
                    }
                }
            } else if o.name.is_some() {
                counts.named_shapes += 1;
            }
            if let Some(p) = o.parent {
                if !is_node.get(&p).copied().unwrap_or(false) {
                    counts.nodes_with_a_non_node_parent += 1;
                }
            }
        }

        // Every shape must reach a root: walking up the parents must end at
        // a node with no parent.
        for o in &objects {
            if o.is_node {
                continue;
            }
            if !SHAPE_TYPES.contains(&o.type_name.as_str()) {
                continue;
            }
            counts.shapes += 1;
            // Walk up the chain of parents. A shape reaches a root when the
            // chain ends at a node that has no parent of its own.
            let mut cur = o.parent;
            let mut reached = false;
            let mut steps = 0;
            while let Some(p) = cur {
                steps += 1;
                if steps > n {
                    // A cycle: the shape is in a loop and reaches nothing.
                    break;
                }
                let Some(node_obj) = objects.iter().find(|obj| obj.block == p) else {
                    // Parent block not in objects or broken.
                    break;
                };
                if !node_obj.is_node {
                    // The parent is not a node, so the chain is broken.
                    break;
                }
                match node_obj.parent {
                    None => {
                        reached = true;
                        break;
                    }
                    Some(up) => cur = Some(up),
                }
            }
            if !reached {
                counts.orphaned_shapes += 1;
                *orphaned.entry(o.type_name.clone()).or_insert(0) += 1;
            }
        }
    }

    println!(
        "{game_id}: {} files, {} unparsed",
        counts.files, counts.unparsed
    );
    println!("  {} nodes, {} shapes", counts.nodes, counts.shapes);
    println!(
        "  shapes with no path to a root: {}",
        counts.orphaned_shapes
    );
    println!(
        "  nodes whose parent is not a node: {}",
        counts.nodes_with_a_non_node_parent
    );
    println!(
        "  blocks two parents list: {}",
        counts.blocks_with_two_parents
    );
    println!(
        "  children pointing outside the file: {}",
        counts.children_out_of_range
    );
    println!(
        "  nodes with a name: {}, shapes with a name: {}",
        counts.named_nodes, counts.named_shapes
    );
    println!(
        "  nodes whose name is still an index: {}",
        counts.unresolved_names
    );
    if !orphaned.is_empty() {
        println!("  orphaned by type:");
        for (k, v) in orphaned.iter().rev().take(15) {
            println!("    {v:8} {k}");
        }
    }
}
