//! What does NVRE's own parser make of a Fallout 4 NIF?
//!
//! Reads a NIF off disk (extracted from a BA2 by hand) and reports the
//! header, every block's type and decode status, and what `scene` makes of
//! it. The question this answers is which of FO4's blockers are real: the
//! header, the schema, the geometry, or only the archive reader.
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_dir = args.next().unwrap();
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };
    // The files to read, kept so the totals at the end can say how many
    // there were.
    let files: Vec<String> = args.collect();
    let args_len = files.len();
    let (mut outside, mut bad_normals, mut bad_uvs, mut bad_tris) = (0u64, 0u64, 0u64, 0u64);
    let mut outside_examples: Vec<String> = Vec::new();
    let mut zero_radius = 0u64;
    for path in files {
        let Ok(data) = std::fs::read(&path) else {
            println!("{path}: cannot read");
            continue;
        };
        println!("=== {path} ({} bytes)", data.len());
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            println!("  PARSE FAILED");
            continue;
        };
        let h = &f.header;
        println!(
            "  version {:#010x}, user_version {}, bs_version {}, {} blocks, string table {}",
            h.version,
            h.user_version,
            h.bs_version(),
            f.blocks.len(),
            h.strings.is_some()
        );
        let (mut f_out, mut f_badn, mut f_baduv, mut f_badtri, mut f_zero) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        let dec = nvre_nif::scene::Decoded::new(&f, &data, &schema);
        let objects = nvre_nif::scene::scene_objects(&f, &dec);
        let (mut shapes, mut built) = (0usize, 0usize);
        println!(
            "  {} scene objects ({} nodes)",
            objects.len(),
            objects.iter().filter(|o| o.is_node).count()
        );
        for o in &objects {
            if o.is_node {
                continue;
            }
            shapes += 1;
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
                println!("  shape {}: block failed to decode", o.type_name);
                continue;
            };
            println!(
                "  shape {}: data_ref={:?} shader_property={:?} alpha_property={:?}",
                o.type_name,
                shape.ref_at("data"),
                shape.ref_at("shader_property"),
                shape.ref_at("alpha_property")
            );
            let data_block = if let Some(r) = shape.ref_at("data") {
                dec.get(r)
            } else if o.type_name == "BSTriShape" || dec.is_a(o.block, "BSTriShape") {
                Some(shape)
            } else {
                None
            };
            let Some(db) = data_block else {
                println!("    no geometry block");
                continue;
            };
            let md = nvre_nif::scene::mesh_data(db, f.header.bs_version());
            println!(
                "    {} positions, {} triangles, normals {}, uv {}",
                md.positions.len(),
                md.triangles.len(),
                md.normals.len() == md.positions.len(),
                md.uv_sets
                    .first()
                    .is_some_and(|u| u.len() == md.positions.len())
            );
            if !md.positions.is_empty() && !md.triangles.is_empty() {
                built += 1;
                // Sanity: positions inside the shape's own bounding sphere,
                // normals roughly unit, triangles naming vertices that exist.
                let sphere = shape.struct_at("bounding_sphere");
                let c = sphere
                    .and_then(|b| b.list("center"))
                    .map(|l| {
                        l.iter()
                            .filter_map(|n| n.as_number())
                            .map(|n| n as f32)
                            .collect::<Vec<f32>>()
                    })
                    .filter(|c| c.len() == 3);
                let rad = sphere.and_then(|b| b.num("radius")).unwrap_or(0.0) as f32;
                if rad == 0.0 {
                    f_zero += 1;
                    continue;
                }
                if let Some(c) = c {
                    let far = md
                        .positions
                        .iter()
                        .filter(|p| {
                            let dx = p[0] - c[0];
                            let dy = p[1] - c[1];
                            let dz = p[2] - c[2];
                            (dx * dx + dy * dy + dz * dz).sqrt() > rad * 1.05
                        })
                        .count();
                    if far > 0 {
                        f_out += far as u64;
                        if outside_examples.len() < 4 {
                            let worst = md
                                .positions
                                .iter()
                                .map(|p| {
                                    let dx = p[0] - c[0];
                                    let dy = p[1] - c[1];
                                    let dz = p[2] - c[2];
                                    ((dx * dx + dy * dy + dz * dz).sqrt(), *p)
                                })
                                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                                .unwrap();
                            outside_examples.push(format!(
                                "{path}: n={} far={far} radius={rad} worst_dist={:.3} worst_pos={:?} center={:?}",
                                md.positions.len(), worst.0, worst.1, c
                            ));
                        }
                    }
                }
                let bad_n = md
                    .normals
                    .iter()
                    .filter(|n| {
                        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                        !(0.9..1.1).contains(&l)
                    })
                    .count();
                f_badn += bad_n as u64;
                let bad_uv = md
                    .uv_sets
                    .first()
                    .map(|u| {
                        u.iter()
                            .filter(|v| !v[0].is_finite() || !v[1].is_finite())
                            .count()
                    })
                    .unwrap_or(0);
                f_baduv += bad_uv as u64;
                let bad_tri = md
                    .triangles
                    .iter()
                    .filter(|t| t.iter().any(|&i| i as usize >= md.positions.len()))
                    .count();
                f_badtri += bad_tri as u64;
            }
        }
        println!("  {shapes} shapes, {built} drawable");
        println!("  shapes with a zero bounding radius: {f_zero}");
        println!("  positions outside bounding sphere: {f_out}");
        for e in &outside_examples {
            println!("    {e}");
        }
        println!("  non-unit normals: {f_badn}");
        println!("  non-finite UVs: {f_baduv}");
        println!("  triangles naming a missing vertex: {f_badtri}");
        outside += f_out;
        bad_normals += f_badn;
        bad_uvs += f_baduv;
        bad_tris += f_badtri;
        zero_radius += f_zero;
        outside_examples.clear();
        println!("  header_len {}", f.header.header_len);
        for (i, b) in f.blocks.iter().enumerate() {
            println!(
                "  raw block {i} {} {:?} ({} bytes)",
                b.type_name,
                b.range,
                b.range.end - b.range.start
            );
        }
        // Decode status of every block, so a failure shows up.
        for i in 0..f.blocks.len() {
            let tn = f.header.block_type_name(i).unwrap_or_default();
            let (st, dec2) = f.decode_block_status(i, &schema, &data);
            if let Some(b) = dec2 {
                println!(
                    "      fields: {:?}",
                    b.fields
                        .iter()
                        .map(|(k, v)| (k.clone(), format!("{v:?}")))
                        .collect::<Vec<_>>()
                );
            }
            println!("    block {i} {tn}: {st:?}");
        }
        // Lineage: is NiNode recognised as a node, BSTriShape as a shape?
        for i in 0..f.blocks.len() {
            let tn = f.header.block_type_name(i).unwrap_or_default();
            if tn.contains("TriShape") || tn.contains("NiNode") {
                println!(
                    "    block {i} {tn}: is_node={} is_shape={} lineage={:?}",
                    dec.is_a(i, "NiNode"),
                    nvre_nif::scene::SHAPE_TYPES.contains(&tn.as_str()),
                    dec.lineage[i]
                );
            }
        }
        // Block types, so an unknown one shows up.
        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for b in &f.blocks {
            *kinds.entry(b.type_name.clone()).or_insert(0) += 1;
        }
        println!("  block types:");
        for (k, v) in &kinds {
            println!("    {k}: {v}");
        }
    }
    // The totals over every file, which is what the per-file counts are for.
    println!("== totals over {args_len} files ==");
    println!("  positions outside bounding sphere: {outside}");
    println!("  non-unit normals: {bad_normals}");
    println!("  non-finite UVs: {bad_uvs}");
    println!("  triangles naming a missing vertex: {bad_tris}");
    println!("  shapes with a zero radius: {zero_radius}");
}
