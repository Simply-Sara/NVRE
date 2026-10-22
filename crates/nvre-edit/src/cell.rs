//! Creating cells and worldspaces: the records a new one needs, and the group
//! tree they have to sit in.
//!
//! A cell is not just a `CELL` record. In a Bethesda plugin it lives in a
//! *group tree* whose shape the engine walks, and a record in the wrong place
//! is silently ignored:
//!
//! - **Interior cell** — `TOP: CELL` (type 0) -> `INTERIOR_CELL_BLOCK` (type 2)
//!   -> `INTERIOR_CELL_SUBBLOCK` (type 3) -> the `CELL` record -> `CELL_CHILDREN`
//!   (type 6) -> `CELL_TEMPORARY_CHILDREN` (type 9) / `CELL_PERSISTENT_CHILDREN`
//!   (type 8).
//! - **Exterior cell** — `TOP: WRLD` (type 0) -> `WORLD_CHILDREN` (type 1,
//!   labelled with the worldspace's FormID) -> `EXTERIOR_CELL_BLOCK` (type 4)
//!   -> `EXTERIOR_CELL_SUBBLOCK` (type 5) -> the `CELL` record -> `CELL_CHILDREN`
//!   (type 6) -> the temporary/persistent children group.
//!
//! In every case the record comes **first** and its own group **second**, as
//! siblings: a world-children group holds `REC WRLD, GRUP 1`, and a
//! sub-block group holds `REC CELL, GRUP 6`.
//!
//! The block and sub-block labels are derived from the cell's own FormID and
//! grid, and the rules were measured against the shipped masters rather than
//! assumed:
//!
//! - **Interior**: `block = (FormID & 0xFFFFFF) % 10`,
//!   `sub = ((FormID & 0xFFFFFF) / 10) % 10`. Verified on all 388 interior
//!   cells of `FalloutNV.esm` and all 27 of `DeadMoney.esm` — zero mismatches.
//!   Only the *low 24 bits* of the FormID count: a DLC plugin's own cell
//!   `0x0100CF6C` has block 0, not 6.
//! - **Exterior**: `block = (grid_y >> 5, grid_x >> 5)` and
//!   `sub = (grid_y >> 3, grid_x >> 3)`, each label a pair of `i16` written
//!   *y first, then x*. Verified on all 30,096 exterior cells of
//!   `FalloutNV.esm` — zero mismatches.
//!
//! A new cell also needs a `LAND` record for its terrain, and a `CELL` record
//! whose own subrecords follow fopdoc's `CELL` page.

use nvre_esm::raw::{group_type, Group, GroupHeader, Node, Record, RecordHeader, Subrecord};

/// The vertices along one edge of a cell's height map: 33 x 33.
pub const LAND_SIDE: usize = 33;

/// A new cell, and the worldspace it belongs to.
#[derive(Debug, Clone)]
pub struct NewCell {
    /// The Editor ID (`EDID`).
    pub edid: String,
    /// The display name (`FULL`).
    pub full: Option<String>,
    /// The worldspace the cell belongs to. `None` makes an interior cell.
    pub world: Option<u32>,
    /// The cell's grid position (`XCLC`), for an exterior cell.
    pub grid: Option<(i32, i32)>,
    /// Give the cell terrain: a `LAND` record in its temporary-children group.
    ///
    /// The game draws an exterior cell's landscape from this record, and an
    /// interior cell has none. A new exterior cell without terrain renders as
    /// a hole, so the default is on for exterior cells.
    pub with_land: bool,
    /// The cell's `DATA` flags, apart from the "is interior" bit, which is set
    /// from the cell's own kind.
    pub data_flags: u8,
}

impl NewCell {
    /// An interior cell: no worldspace, no grid.
    pub fn interior(edid: impl Into<String>) -> NewCell {
        NewCell {
            edid: edid.into(),
            full: None,
            world: None,
            grid: None,
            with_land: false,
            data_flags: 0,
        }
    }

    /// An exterior cell in a worldspace at a grid position.
    pub fn exterior(edid: impl Into<String>, world: u32, grid: (i32, i32)) -> NewCell {
        NewCell {
            edid: edid.into(),
            full: None,
            world: Some(world),
            grid: Some(grid),
            with_land: true,
            data_flags: 0,
        }
    }
}

/// A new worldspace.
#[derive(Debug, Clone)]
pub struct NewWorldspace {
    /// The Editor ID (`EDID`).
    pub edid: String,
    /// The display name (`FULL`).
    pub full: Option<String>,
    /// The default land height (`DNAM`), in game units.
    pub default_land_height: f32,
    /// The default water height (`DNAM`), in game units.
    pub default_water_height: f32,
    /// The climate record (`CNAM`).
    pub climate: Option<u32>,
    /// The water type (`NAM2`).
    pub water: Option<u32>,
}

impl NewWorldspace {
    /// A worldspace with the given Editor ID and the heights the Mojave uses.
    pub fn new(edid: impl Into<String>) -> NewWorldspace {
        NewWorldspace {
            edid: edid.into(),
            full: None,
            default_land_height: -2500.0,
            default_water_height: -2300.0,
            climate: None,
            water: None,
        }
    }
}

/// A `CELL` record for a new cell.
///
/// The subrecords are the ones the game reads: `EDID`, `FULL`, `DATA` (the
/// flags), `XCLC` (the grid) for an exterior cell, and `XCLW` (the water
/// height), which the engine reads for interior lighting.
pub fn make_cell(cell: &NewCell) -> Record {
    let mut subs: Vec<Subrecord> = Vec::new();
    subs.push(Subrecord {
        sig: *b"EDID",
        data: crate::document::encode_zstring(&cell.edid),
        extended: false,
    });
    if let Some(full) = &cell.full {
        subs.push(Subrecord {
            sig: *b"FULL",
            data: crate::document::encode_zstring(full),
            extended: false,
        });
    }
    // Bit 0 is "is interior cell"; the game uses it to tell an interior from
    // an exterior, so it is set from the cell's own kind rather than left to
    // the caller.
    let mut flags = cell.data_flags;
    if cell.world.is_none() {
        flags |= 0x01;
    }
    subs.push(Subrecord {
        sig: *b"DATA",
        data: vec![flags],
        extended: false,
    });
    if let Some((x, y)) = cell.grid {
        // XCLC: x (i32), y (i32), the land flags, and three unused bytes. The
        // shipped cells carry the 12-byte form; the 8-byte form omits the
        // flags and the padding.
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data.push(0); // land flags: no quadrant hidden
        data.extend_from_slice(&[0u8; 3]);
        subs.push(Subrecord {
            sig: *b"XCLC",
            data,
            extended: false,
        });
    }
    // The water height. The game stores -2147483648.0 for "no water" in an
    // interior cell, which is what the shipped interiors carry.
    subs.push(Subrecord {
        sig: *b"XCLW",
        data: (-2147483648.0f32).to_le_bytes().to_vec(),
        extended: false,
    });
    make_record(*b"CELL", subs)
}

/// A `LAND` record: a flat 33 x 33 height map, normals and colours.
///
/// The height map is a `f32` offset followed by one byte per vertex, each a
/// step from the previous vertex; a flat cell is all zero steps, so every
/// vertex sits at the offset. The normals point straight up and the colours
/// are white, which is what an untextured cell shows.
pub fn make_land(height: f32) -> Record {
    let mut subs: Vec<Subrecord> = Vec::new();
    // DATA: the cell's own flags, a u32. Bit 0 says the record has vertex
    // normals and a height map, bit 1 that it has vertex colours, and bit 4
    // that the normals are calculated. A new cell has all three, so the flags
    // are 0x1F — the value the shipped cells carry.
    subs.push(Subrecord {
        sig: *b"DATA",
        data: 0x1Fu32.to_le_bytes().to_vec(),
        extended: false,
    });
    // VNML: 1,089 vertex normals, three bytes each. A flat surface points up,
    // so the normal is (0, 0, 127) in the signed-byte form the game stores.
    let mut vnml = Vec::with_capacity(LAND_SIDE * LAND_SIDE * 3);
    for _ in 0..LAND_SIDE * LAND_SIDE {
        vnml.extend_from_slice(&[0, 0, 127]);
    }
    subs.push(Subrecord {
        sig: *b"VNML",
        data: vnml,
        extended: false,
    });
    // VHGT: the height offset, one byte per vertex, and three unused bytes.
    let mut vhgt = Vec::with_capacity(4 + LAND_SIDE * LAND_SIDE + 3);
    vhgt.extend_from_slice(&height.to_le_bytes());
    vhgt.resize(4 + LAND_SIDE * LAND_SIDE, 0);
    vhgt.resize(4 + LAND_SIDE * LAND_SIDE + 3, 0);
    subs.push(Subrecord {
        sig: *b"VHGT",
        data: vhgt,
        extended: false,
    });
    // VCLR: 1,089 vertex colours, three bytes each. White, so the cell is not
    // tinted by its own vertex colours.
    let mut vclr = Vec::with_capacity(LAND_SIDE * LAND_SIDE * 3);
    vclr.resize(LAND_SIDE * LAND_SIDE * 3, 255);
    subs.push(Subrecord {
        sig: *b"VCLR",
        data: vclr,
        extended: false,
    });
    make_record(*b"LAND", subs)
}

/// A `WRLD` record for a new worldspace.
///
/// The subrecords are the ones a worldspace needs to load: the Editor ID and
/// name, the land data (the two default heights), the world map data (the
/// usable size and the corner cells), the world map offset, the flags, and the
/// object bounds.
pub fn make_worldspace(world: &NewWorldspace) -> Record {
    let mut subs: Vec<Subrecord> = Vec::new();
    subs.push(Subrecord {
        sig: *b"EDID",
        data: crate::document::encode_zstring(&world.edid),
        extended: false,
    });
    if let Some(full) = &world.full {
        subs.push(Subrecord {
            sig: *b"FULL",
            data: crate::document::encode_zstring(full),
            extended: false,
        });
    }
    if let Some(c) = world.climate {
        subs.push(Subrecord {
            sig: *b"CNAM",
            data: c.to_le_bytes().to_vec(),
            extended: false,
        });
    }
    if let Some(w) = world.water {
        subs.push(Subrecord {
            sig: *b"NAM2",
            data: w.to_le_bytes().to_vec(),
            extended: false,
        });
    }
    // DNAM: the default land and water heights.
    let mut dnam = Vec::with_capacity(8);
    dnam.extend_from_slice(&world.default_land_height.to_le_bytes());
    dnam.extend_from_slice(&world.default_water_height.to_le_bytes());
    subs.push(Subrecord {
        sig: *b"DNAM",
        data: dnam,
        extended: false,
    });
    // MNAM: the usable size and the corner cells. A new worldspace covers one
    // cell square, so the usable size is one cell and the corners are 0, 0.
    let mut mnam = Vec::with_capacity(16);
    mnam.extend_from_slice(&1i32.to_le_bytes());
    mnam.extend_from_slice(&1i32.to_le_bytes());
    mnam.extend_from_slice(&0i16.to_le_bytes());
    mnam.extend_from_slice(&0i16.to_le_bytes());
    mnam.extend_from_slice(&0i16.to_le_bytes());
    mnam.extend_from_slice(&0i16.to_le_bytes());
    subs.push(Subrecord {
        sig: *b"MNAM",
        data: mnam,
        extended: false,
    });
    // ONAM: the world map scale and the cell offsets.
    let mut onam = Vec::with_capacity(12);
    onam.extend_from_slice(&1.0f32.to_le_bytes());
    onam.extend_from_slice(&0.0f32.to_le_bytes());
    onam.extend_from_slice(&0.0f32.to_le_bytes());
    subs.push(Subrecord {
        sig: *b"ONAM",
        data: onam,
        extended: false,
    });
    // DATA: the worldspace flags. None set.
    subs.push(Subrecord {
        sig: *b"DATA",
        data: vec![0u8],
        extended: false,
    });
    // NAM0 / NAM9: the minimum and maximum object bounds. A new worldspace has
    // no objects, so both cover the same square.
    for sig in [*b"NAM0", *b"NAM9"] {
        let mut bounds = Vec::with_capacity(8);
        bounds.extend_from_slice(&0.0f32.to_le_bytes());
        bounds.extend_from_slice(&0.0f32.to_le_bytes());
        subs.push(Subrecord {
            sig,
            data: bounds,
            extended: false,
        });
    }
    make_record(*b"WRLD", subs)
}

/// Build a record from its subrecords.
fn make_record(sig: [u8; 4], subs: Vec<Subrecord>) -> Record {
    let payload = nvre_esm::raw::write_subrecords(&subs);
    Record {
        header: RecordHeader {
            sig,
            data_size: payload.len() as u32,
            flags: 0,
            form_id: 0,
            // The game's own bookkeeping; 0 is accepted for new records.
            vc_info: 0,
            form_version: 15,
            vc_info2: 0,
        },
        raw: payload,
        subrecords: subs,
        offset: 0,
    }
}

/// The interior block and sub-block a cell's FormID puts it in.
///
/// The label is computed from the cell's **low 24 bits**: a plugin's own
/// records have a master index in the top byte, and the shipped DLC masters
/// label their interior cells by the low bits only.
pub fn interior_block_labels(form_id: u32) -> (i32, i32) {
    let low = form_id & 0x00FF_FFFF;
    ((low % 10) as i32, ((low / 10) % 10) as i32)
}

/// The exterior block label a cell's grid puts it in, as (x, y).
///
/// The group header stores the pair **y first, then x**, which is what
/// [`exterior_block_label_bytes`] writes; this returns them the other way
/// round, so the pair reads as the grid coordinate it is.
pub fn exterior_block_labels(grid: (i32, i32)) -> (i16, i16) {
    let (x, y) = grid;
    ((x >> 5) as i16, (y >> 5) as i16)
}

/// The exterior block label a cell's grid puts it in, as the four bytes a
/// group header carries: the y coordinate first, then the x.
pub fn exterior_block_label_bytes(grid: (i32, i32)) -> [u8; 4] {
    let (bx, by) = exterior_block_labels(grid);
    let mut l = [0u8; 4];
    l[0..2].copy_from_slice(&by.to_le_bytes());
    l[2..4].copy_from_slice(&bx.to_le_bytes());
    l
}

/// The exterior sub-block label a cell's grid puts it in, as (x, y).
pub fn exterior_subblock_labels(grid: (i32, i32)) -> (i16, i16) {
    let (x, y) = grid;
    ((x >> 3) as i16, (y >> 3) as i16)
}

/// The exterior sub-block label a cell's grid puts it in, as the four bytes a
/// group header carries: the y coordinate first, then the x.
pub fn exterior_subblock_label_bytes(grid: (i32, i32)) -> [u8; 4] {
    let (sx, sy) = exterior_subblock_labels(grid);
    let mut l = [0u8; 4];
    l[0..2].copy_from_slice(&sy.to_le_bytes());
    l[2..4].copy_from_slice(&sx.to_le_bytes());
    l
}

/// The `CELL_CHILDREN` group and its temporary-children group, for one cell.
///
/// The temporary-children group holds the cell's terrain and its placed
/// references.
fn cell_children_group(cell_form_id: u32) -> Group {
    let mut group = Group {
        header: group_header(group_type::CELL_CHILDREN, cell_form_id.to_le_bytes()),
        children: Vec::new(),
        offset: 0,
    };
    group.children.push(Node::Group(Group {
        header: group_header(
            group_type::CELL_TEMPORARY_CHILDREN,
            cell_form_id.to_le_bytes(),
        ),
        children: Vec::new(),
        offset: 0,
    }));
    group
}

/// The block group an interior cell lives in, ready to be put under
/// `TOP: CELL`.
///
/// The group holds the sub-block group, which holds the `CELL` record and the
/// cell's children group.
pub fn interior_block_group(cell_form_id: u32, cell_record: Record) -> Group {
    let (block, sub_block) = interior_block_labels(cell_form_id);
    let mut sub = Group {
        header: group_header(group_type::INTERIOR_CELL_SUBBLOCK, sub_block.to_le_bytes()),
        children: vec![Node::Record(cell_record)],
        offset: 0,
    };
    sub.children
        .push(Node::Group(cell_children_group(cell_form_id)));
    Group {
        header: group_header(group_type::INTERIOR_CELL_BLOCK, block.to_le_bytes()),
        children: vec![Node::Group(sub)],
        offset: 0,
    }
}

/// The block group an exterior cell lives in, ready to be put under a
/// world-children group.
///
/// The group holds the sub-block group, which holds the `CELL` record and the
/// cell's children group.
pub fn exterior_block_group(grid: (i32, i32), cell_form_id: u32, cell_record: Record) -> Group {
    let block_label = exterior_block_label_bytes(grid);
    let sub_label = exterior_subblock_label_bytes(grid);
    let mut sub = Group {
        header: group_header(group_type::EXTERIOR_CELL_SUBBLOCK, sub_label),
        children: vec![Node::Record(cell_record)],
        offset: 0,
    };
    sub.children
        .push(Node::Group(cell_children_group(cell_form_id)));
    Group {
        header: group_header(group_type::EXTERIOR_CELL_BLOCK, block_label),
        children: vec![Node::Group(sub)],
        offset: 0,
    }
}

/// The worldspace's own record and its world-children group, ready to be put
/// into a `TOP: WRLD` group.
///
/// A plugin has **one** `TOP: WRLD` group holding every worldspace, and each
/// worldspace's record is followed by its own world-children group. So a new
/// worldspace adds two children to that one top-level group, not a group of
/// its own.
pub fn worldspace_children(world_form_id: u32, world_record: Record) -> Vec<Node> {
    vec![
        Node::Record(world_record),
        Node::Group(Group {
            header: group_header(group_type::WORLD_CHILDREN, world_form_id.to_le_bytes()),
            children: Vec::new(),
            offset: 0,
        }),
    ]
}

/// A group header with the given type and label.
fn group_header(group_type: i32, label: [u8; 4]) -> GroupHeader {
    GroupHeader {
        size: 0,
        label,
        group_type,
        stamp: 0,
        unknown1: 0,
        version: 0,
        unknown2: 0,
    }
}
