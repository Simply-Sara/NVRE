//! Survey Skyrim SE NIFs: does the packed-vertex unpacker read the right
//! attribute flags out of the vertex description?
use bsa105::Archive;
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().unwrap();
    let schema_dir = args.next().unwrap();
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };
    let mut names: Vec<String> = std::fs::read_dir(&data_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bsa"))
        .collect();
    names.sort();
    let (mut files, mut shapes, mut zero_pos, mut ok_pos) = (0u64, 0u64, 0u64, 0u64);
    let (mut empty_shapes, mut real_zero) = (0u64, 0u64);
    let (mut outside, mut bad_normals, mut bad_uvs, mut bad_tris) = (0u64, 0u64, 0u64, 0u64);
    let mut filler_normals = 0u64;
    let mut normal_examples: Vec<String> = Vec::new();
    let mut descs: std::collections::BTreeMap<u64, u64> = Default::default();
    let mut attr_variants: std::collections::BTreeMap<u64, u64> = Default::default();
    let mut examples: Vec<String> = Vec::new();
    'outer: for name in &names {
        if !name.contains("Mesh") {
            continue;
        }
        let Ok(mut a) = Archive::open(PathBuf::from(&data_dir).join(name)) else {
            continue;
        };
        let paths: Vec<String> = a
            .entries()
            .map(|m| m.path())
            .filter(|p| p.to_lowercase().ends_with(".nif"))
            .collect();
        for p in paths {
            let Ok(data) = a.extract_by_path(&p) else {
                continue;
            };
            let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
                continue;
            };
            files += 1;
            if files > limit as u64 {
                break 'outer;
            }
            let dec = nvre_nif::scene::Decoded::new(&f, &data, &schema);
            for (i, b) in f.blocks.iter().enumerate() {
                if b.type_name != "BSTriShape" {
                    continue;
                }
                let Some(Ok(s)) = dec.blocks.get(i) else {
                    continue;
                };
                shapes += 1;
                let Some(desc) = s.int("vertex_desc").map(|d| d as u64) else {
                    continue;
                };
                *descs.entry(desc).or_insert(0) += 1;
                *attr_variants.entry(desc >> 44).or_insert(0) += 1;
                let md = nvre_nif::scene::mesh_data(s, f.header.bs_version());
                // Sanity: positions must lie inside the shape's own bounding
                // sphere, and normals must be roughly unit length.
                let sphere = s.struct_at("bounding_sphere");
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
                if let (Some(c), true) = (c, !md.positions.is_empty()) {
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
                    outside += far as u64;
                }
                let default_normals = md
                    .normals
                    .iter()
                    .filter(|n| **n == [0.0f32, 0.0, 1.0])
                    .count();
                filler_normals += default_normals as u64;
                let bad_n = md
                    .normals
                    .iter()
                    .filter(|n| {
                        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                        !(0.9..1.1).contains(&l)
                    })
                    .count();
                if bad_n > 0 && normal_examples.len() < 10 {
                    let d2 = s.int("vertex_desc").unwrap_or(0) as u64;
                    let bad: Vec<[f32; 3]> = md
                        .normals
                        .iter()
                        .filter(|n| {
                            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                            !(0.9..1.1).contains(&l)
                        })
                        .take(3)
                        .copied()
                        .collect();
                    normal_examples.push(format!(
                        "{p} block {i} desc={d2:#x} attrs={:#05x} bad={bad:?}",
                        d2 >> 44
                    ));
                }
                bad_normals += bad_n as u64;
                let bad_uv = md
                    .uv_sets
                    .first()
                    .map(|u| {
                        u.iter()
                            .filter(|v| !v[0].is_finite() || !v[1].is_finite())
                            .count()
                    })
                    .unwrap_or(0);
                bad_uvs += bad_uv as u64;
                let bad_tri = md
                    .triangles
                    .iter()
                    .filter(|t| t.iter().any(|&i| i as usize >= md.positions.len()))
                    .count();
                bad_tris += bad_tri as u64;
                let any = md
                    .positions
                    .iter()
                    .any(|p| p[0] != 0.0 || p[1] != 0.0 || p[2] != 0.0);
                if md.positions.is_empty() {
                    empty_shapes += 1;
                } else if any {
                    ok_pos += 1;
                } else {
                    real_zero += 1;
                    if examples.len() < 5 {
                        examples.push(format!("{p} desc={desc:#x} n={}", md.positions.len()));
                    }
                }
                if false {
                    zero_pos += 1;
                    if examples.len() < 5 {
                        examples.push(format!("{p} desc={desc:#x} n={}", md.positions.len()));
                    }
                }
            }
        }
    }
    println!("files {files}, BSTriShape blocks {shapes}");
    println!("  with any non-zero position: {ok_pos}");
    println!("  all-zero positions:         {zero_pos}");
    println!("    of which 0-vertex shapes:  {empty_shapes}");
    println!("    of which real zero-geom:  {real_zero}");
    for e in &examples {
        println!("    {e}");
    }
    println!("  positions outside bounding sphere: {outside}");
    println!("  non-unit normals:                  {bad_normals}");
    for e in &normal_examples {
        println!("    {e}");
    }
    println!("  non-finite UVs:                    {bad_uvs}");
    println!("  triangles naming a missing vertex: {bad_tris}");
    println!("distinct vertex_desc values: {}", descs.len());
    for (d, n) in descs.iter().take(20) {
        println!(
            "  {d:#x} x{n}  (attrs={:#x}, stride_words={})",
            d >> 44,
            d & 0xF
        );
    }
    println!("distinct attribute nibbles:");
    for (a, n) in &attr_variants {
        let names = [
            ("VERTEX", 1u64),
            ("UV", 2),
            ("UV2", 4),
            ("NORMALS", 8),
            ("TANGENTS", 16),
            ("COLORS", 32),
            ("SKINNED", 64),
            ("LAND", 128),
            ("EYE", 256),
            ("INSTANCE", 512),
            ("FULL_PREC", 1024),
        ];
        let on: Vec<&str> = names
            .iter()
            .filter(|(_, b)| a & b != 0)
            .map(|(n, _)| *n)
            .collect();
        println!("  {a:#05x} x{n} -> {on:?}");
    }
}
