use nvre_world::land::{load_land, LandParts};
use nvf_assets::data::GameData;
use nvre_editor::session::Session;
use std::path::Path;
use std::time::Instant;

fn parse_formid(s: &str) -> Option<u32> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(hex, 16).ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let cell_query = args.get(1).map(|s| s.as_str()).unwrap_or("TheFortSummit");
    let radius: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2);

    let data = std::env::var("NVRE_DATA")?;
    let master = format!("{data}/FalloutNV.esm");
    let schema_dir = Path::new("schemas/fnv");

    println!("============================================================");
    println!(
        " PROFILING CELL LOAD: '{}' (Radius: {})",
        cell_query, radius
    );
    println!("============================================================");

    let t_init = Instant::now();
    let mut session = Session::open(
        Path::new(&master),
        schema_dir,
        vec!["FalloutNV.esm".to_string()],
    )?;
    println!("Session::open: {:?}", t_init.elapsed());

    let cell_entry = session
        .index
        .cells
        .values()
        .find(|c| {
            c.edid
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(cell_query))
                || c.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(cell_query))
                || format!("{:08X}", c.form_id).eq_ignore_ascii_case(cell_query)
        })
        .cloned();

    let Some(cell_info) = cell_entry else {
        eprintln!("Cell '{}' not found!", cell_query);
        return Ok(());
    };

    let cell_id = cell_info.form_id;
    let is_exterior = cell_info.grid.is_some();
    println!(
        "Found cell: {:08X} ({:?}) - {}",
        cell_id,
        cell_info.edid,
        if is_exterior { "Exterior" } else { "Interior" }
    );

    // 1. Session open_cell & ref building
    session.open_cell(cell_id);

    let t_refs = Instant::now();
    let cell_refs = session.cell_refs().to_vec();
    let dur_refs = t_refs.elapsed();
    println!(
        "Step 1a: session.cell_refs() ({} refs): {:?}",
        cell_refs.len(),
        dur_refs
    );

    let t_details = Instant::now();
    let _details = session.ref_details().clone();
    let dur_details = t_details.elapsed();
    println!(
        "Step 1b: session.ref_details() (owners/types): {:?}",
        dur_details
    );

    let (active_refs, dur_grid) = if is_exterior && radius > 0 {
        let t_grid = Instant::now();
        let grid_refs = session.exterior_grid_refs(radius).to_vec();
        (grid_refs, t_grid.elapsed())
    } else {
        (cell_refs.clone(), std::time::Duration::ZERO)
    };
    if is_exterior && radius > 0 {
        println!(
            "Step 1c: exterior_grid_refs(radius={}) ({} refs): {:?}",
            radius,
            active_refs.len(),
            dur_grid
        );
    }

    // 2. Terrain Loading (if exterior)
    let mut dur_terrain_records = std::time::Duration::ZERO;
    let mut terrain_cells_count = 0;

    let nif_schema = Path::new("schemas/nif");
    let game_data = GameData::open(Path::new(&data), nif_schema)?;

    if is_exterior {
        let lo = session.load_order()?;
        let (Some(w), Some(g)) = (cell_info.world, cell_info.grid) else {
            unreachable!()
        };
        let ws = session.index.worldspaces.get(&w).cloned().unwrap();

        let cells_to_load: Vec<((i32, i32), u32)> = if ws.grid.len() <= 1200 {
            ws.grid
                .iter()
                .filter_map(|(&coord, &cid)| Some((coord, session.index.cells.get(&cid)?.land?)))
                .collect()
        } else {
            let mut list = Vec::new();
            for dx in -8..=8 {
                for dy in -8..=8 {
                    let coord = (g.0 + dx, g.1 + dy);
                    if let Some(&cid) = ws.grid.get(&coord) {
                        if let Some(c) = session.index.cells.get(&cid) {
                            if let Some(lid) = c.land {
                                list.push((coord, lid));
                            }
                        }
                    }
                }
            }
            list
        };
        terrain_cells_count = cells_to_load.len();

        let t_land_read = Instant::now();
        for (_, land_id) in &cells_to_load {
            let _ = load_land(lo, &session.schema, *land_id, LandParts::ALL);
        }
        dur_terrain_records = t_land_read.elapsed();

        println!(
            "Step 2: Terrain records ({} cells): {:?}",
            terrain_cells_count, dur_terrain_records
        );
    }

    // 3. Model path resolution
    let t_paths = Instant::now();
    let mut missing_bases: Vec<u32> = active_refs
        .iter()
        .filter_map(|r| r.base.as_deref().and_then(parse_formid))
        .collect();
    missing_bases.sort_unstable();
    missing_bases.dedup();
    let unique_base_count = missing_bases.len();

    let paths = session.model_paths_for(&missing_bases);
    let dur_paths = t_paths.elapsed();
    println!(
        "Step 3: Resolve {} unique base objects -> NIF paths: {:?}",
        unique_base_count, dur_paths
    );

    let mut model_paths: Vec<String> = paths.into_iter().flatten().collect();
    model_paths.sort();
    model_paths.dedup();
    let unique_nif_count = model_paths.len();
    println!("        Unique NIF model files: {}", unique_nif_count);

    // 4. Archive Read & NIF Parsing breakdown
    let mut bsa_read_total = std::time::Duration::ZERO;
    let mut nif_parse_total = std::time::Duration::ZERO;
    let mut total_nif_bytes = 0usize;
    let mut model_timings: Vec<(String, std::time::Duration, std::time::Duration, usize)> =
        Vec::new();

    let schema = game_data.schema.clone();
    for p in &model_paths {
        let t_read = Instant::now();
        let bytes_opt = game_data.read(p);
        let read_time = t_read.elapsed();
        bsa_read_total += read_time;

        if let Some(bytes) = bytes_opt {
            total_nif_bytes += bytes.len();
            let t_parse = Instant::now();
            let _ = nvre_nif::NifFile::parse(&bytes, Some(&schema));
            let parse_time = t_parse.elapsed();
            nif_parse_total += parse_time;

            model_timings.push((p.clone(), read_time, parse_time, bytes.len()));
        }
    }
    println!(
        "Step 4a: BSA archive reads ({} KB): {:?}",
        total_nif_bytes / 1024,
        bsa_read_total
    );
    println!("Step 4b: NIF binary parsing: {:?}", nif_parse_total);

    // Sort models by total cost (read + parse)
    model_timings.sort_by_key(|m| std::cmp::Reverse(m.1 + m.2));
    println!("\n--- TOP 10 HEAVIEST MODELS ---");
    for (path, read_dur, parse_dur, size) in model_timings.iter().take(10) {
        println!(
            "  {:>6} KB | read: {:>8.2?} | parse: {:>8.2?} | total: {:>8.2?} | {}",
            size / 1024,
            read_dur,
            parse_dur,
            *read_dur + *parse_dur,
            path
        );
    }

    println!("\n============================================================");
    println!(" SUMMARY BREAKDOWN FOR '{}'", cell_query);
    println!("============================================================");
    println!("Total Placed References:  {}", active_refs.len());
    println!("Unique Base Objects:      {}", unique_base_count);
    println!("Unique NIF Models:        {}", unique_nif_count);
    if is_exterior {
        println!("Terrain Cells Loaded:     {}", terrain_cells_count);
    }
    println!("------------------------------------------------------------");
    println!(
        "Phase 1: Cell Reference & ESM indexing: {:?}",
        dur_refs + dur_details + dur_grid
    );
    println!(
        "Phase 2: Base Object -> NIF path resolution: {:?}",
        dur_paths
    );
    if is_exterior {
        println!(
            "Phase 3: LAND records extraction:       {:?}",
            dur_terrain_records
        );
    }
    println!(
        "Phase 4: BSA archive file decompression: {:?}",
        bsa_read_total
    );
    println!(
        "Phase 5: NIF geometry & block parsing:   {:?}",
        nif_parse_total
    );
    println!("============================================================");

    Ok(())
}
