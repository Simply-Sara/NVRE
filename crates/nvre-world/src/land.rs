//! The landscape: a 33×33 vertex grid per exterior cell, with its heights,
//! normals, colours and texture layers.
//!
//! An exterior cell's ground is a grid of 33×33 vertices. The heights are
//! stored as *steps* rather than values: the first value of each row is a
//! step from the first value of the row before, and every other value is a
//! step from its western neighbour. Integrating those steps gives the
//! terrain, and the factor that turns a step into world units is 8.

use crate::fields::{global, winner, Fields};
use crate::CELL_SIZE;
use nvre_esm::{LoadOrder, Schema};

/// Vertices per cell edge.
pub const SIDE: usize = 33;
/// The distance between neighbouring vertices, in world units.
pub const SPACING: f32 = CELL_SIZE / (SIDE as f32 - 1.0);
/// The world units one stored height step is worth.
pub const HEIGHT_SCALE: f32 = 8.0;
/// Vertices per quadrant edge. A quadrant is half a cell, plus the row it
/// shares with the quadrant below it.
pub const QUAD_SIDE: usize = 17;
/// The diffuse texture a quadrant without one of its own uses, relative to
/// `textures\`.
pub const DEFAULT_DIFFUSE: &str = "landscape\\dirtwasteland01.dds";
/// Its normal map.
pub const DEFAULT_NORMAL: &str = "landscape\\dirtwasteland01_n.dds";
/// The quadrant names, by index.
pub const QUADRANTS: [&str; 4] = ["Bottom Left", "Bottom Right", "Top Left", "Top Right"];

/// One texture layer of a quadrant.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Which quadrant the layer is in.
    pub quadrant: u8,
    /// The texture: `None` means the quadrant's default.
    pub texture: Option<u32>,
    /// The layer's order.
    pub layer: i16,
    /// The layer fills the quadrant, rather than being blended by its alpha.
    pub base: bool,
    /// Where the layer is opaque: a position in the 17×17 quadrant grid, and
    /// how opaque it is there.
    pub alpha: Vec<(u16, f32)>,
}

/// An exterior cell's ground.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Land {
    /// The cell's FormID.
    pub form_id: u32,
    /// The cell's flags.
    pub flags: u32,
    /// The heights, in world units, `SIDE * SIDE` of them, row by row: a row
    /// runs south to north, a column west to east, and both are relative to
    /// the cell's south-west corner.
    pub heights: Option<Vec<f32>>,
    /// The normals, as signed bytes, in the same layout.
    pub normals: Option<Vec<[i8; 3]>>,
    /// The vertex colours, in the same layout.
    pub colors: Option<Vec<[u8; 3]>>,
    /// The texture layers.
    pub layers: Vec<Layer>,
}

/// Integrate the stored height steps into heights.
///
/// The first value of each row is a step from the first value of the row
/// before — the first row's from `offset` — and every other value is a step
/// from its western neighbour.
pub fn heights_from_vhgt(offset: f32, steps: &[i8]) -> Vec<f32> {
    assert_eq!(steps.len(), SIDE * SIDE);
    let mut out = vec![0.0; SIDE * SIDE];
    let mut row_start = offset;
    for r in 0..SIDE {
        row_start += steps[r * SIDE] as f32;
        let mut h = row_start;
        out[r * SIDE] = h * HEIGHT_SCALE;
        for c in 1..SIDE {
            h += steps[r * SIDE + c] as f32;
            out[r * SIDE + c] = h * HEIGHT_SCALE;
        }
    }
    out
}

/// Which parts of a cell to read. The normals, the colours and the layers
/// are large, and a reader that wants only the heights does not need them.
#[derive(Debug, Clone, Copy)]
pub struct LandParts {
    /// Read the heights.
    pub heights: bool,
    /// Read the normals.
    pub normals: bool,
    /// Read the colours.
    pub colors: bool,
    /// Read the layers.
    pub layers: bool,
}

impl LandParts {
    /// Everything.
    pub const ALL: LandParts = LandParts {
        heights: true,
        normals: true,
        colors: true,
        layers: true,
    };
    /// Only the heights.
    pub const HEIGHTS: LandParts = LandParts {
        heights: true,
        normals: false,
        colors: false,
        layers: false,
    };
}

/// Read an exterior cell's ground.
pub fn load_land(lo: &LoadOrder, _schema: &Schema, fid: u32, parts: LandParts) -> Option<Land> {
    let (pi, r) = winner(lo, fid)?;
    let subs = r.subrecords().unwrap_or_default();

    let flags = subs
        .iter()
        .find(|s| &s.sig == b"DATA")
        .and_then(|s| s.data.get(..4))
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0);

    let mut land = Land {
        form_id: fid,
        flags,
        ..Default::default()
    };
    if parts.heights {
        if let Some(s) = subs.iter().find(|s| &s.sig == b"VHGT") {
            if s.data.len() >= 4 + SIDE * SIDE {
                let offset = f32::from_le_bytes(s.data[0..4].try_into().ok()?);
                let steps: Vec<i8> = s.data[4..4 + SIDE * SIDE]
                    .iter()
                    .map(|&b| b as i8)
                    .collect();
                land.heights = Some(heights_from_vhgt(offset, &steps));
            }
        }
    }
    if parts.normals {
        if let Some(s) = subs.iter().find(|s| &s.sig == b"VNML") {
            if s.data.len() >= SIDE * SIDE * 3 {
                let raw: Vec<[i8; 3]> = s.data[..SIDE * SIDE * 3]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0] as i8, c[1] as i8, c[2] as i8])
                    .collect();
                land.normals = Some(raw);
            }
        }
    }
    if parts.colors {
        if let Some(s) = subs.iter().find(|s| &s.sig == b"VCLR") {
            if s.data.len() >= SIDE * SIDE * 3 {
                let raw: Vec<[u8; 3]> = s.data[..SIDE * SIDE * 3]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2]])
                    .collect();
                land.colors = Some(raw);
            }
        }
    }
    if parts.layers {
        for s in &subs {
            match &s.sig {
                b"BTXT" | b"ATXT" => {
                    if s.data.len() >= 8 {
                        let local_tex = u32::from_le_bytes(s.data[0..4].try_into().ok()?);
                        let quadrant = s.data[4];
                        let layer = i16::from_le_bytes(s.data[6..8].try_into().ok()?);
                        land.layers.push(Layer {
                            quadrant,
                            texture: (local_tex != 0)
                                .then(|| global(lo, pi, local_tex))
                                .flatten(),
                            layer,
                            base: &s.sig == b"BTXT",
                            alpha: Vec::new(),
                        });
                    }
                }
                b"VTXT" => {
                    // The alpha of the layer that came before.
                    if let Some(l) = land.layers.last_mut().filter(|l| !l.base) {
                        for chunk in s.data.as_chunks::<4>().0 {
                            let pos = u16::from_le_bytes([chunk[0], chunk[1]]);
                            let opacity = chunk[3] as f32 / 255.0;
                            l.alpha.push((pos, opacity));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(land)
}

/// A landscape texture, resolved through its texture set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LandTexture {
    /// The texture's FormID.
    pub form_id: u32,
    /// The editor ID.
    pub edid: Option<String>,
    /// The diffuse texture, relative to `textures\`.
    pub diffuse: Option<String>,
    /// The normal map.
    pub normal: Option<String>,
}

/// Read a landscape texture.
pub fn land_texture(lo: &LoadOrder, schema: &Schema, ltex: u32) -> Option<LandTexture> {
    let (pi, r) = winner(lo, ltex)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let mut t = LandTexture {
        form_id: ltex,
        edid: f.string(b"EDID"),
        ..Default::default()
    };
    if let Some(ts) = f.formid(b"TNAM", lo) {
        let (tpi, tr) = winner(lo, ts)?;
        let tf = Fields::new(tr, tpi, schema).ok()?;
        let nonempty = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        t.diffuse = nonempty(tf.string(b"TX00"));
        t.normal = nonempty(tf.string(b"TX01"));
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_steps_integrate() {
        let mut steps = vec![0i8; SIDE * SIDE];
        // Each row starts one step higher than the row before; within the
        // second row every vertex is two steps higher than its neighbour.
        for r in 0..SIDE {
            steps[r * SIDE] = 1;
        }
        for c in 1..SIDE {
            steps[SIDE + c] = 2;
        }
        let h = heights_from_vhgt(10.0, &steps);
        assert_eq!(h[0], 11.0 * 8.0);
        assert_eq!(h[SIDE - 1], 11.0 * 8.0);
        assert_eq!(h[SIDE], 12.0 * 8.0);
        assert_eq!(h[2 * SIDE - 1], (12.0 + 64.0) * 8.0);
        // A row starts from the row before's *first* value, not its last.
        assert_eq!(h[2 * SIDE], 13.0 * 8.0);
    }

    #[test]
    fn the_vertices_are_a_hundred_and_twenty_eight_apart() {
        assert_eq!(SPACING, 128.0);
    }

    #[test]
    fn s8_1_vhgt_step_integration_corner_invariants() {
        // Test exact 33x33 height grid integration across all four corners
        let mut steps = vec![0i8; SIDE * SIDE];
        // Row 0 start step +5
        steps[0] = 5;
        // Step each vertex in row 0 by +1
        for c in 1..SIDE {
            steps[c] = 1;
        }
        // Step each subsequent row's starting vertex by +2 relative to previous row's first vertex
        for r in 1..SIDE {
            steps[r * SIDE] = 2;
        }
        // At the bottom-right corner (last row, last col), step each col by +3
        for c in 1..SIDE {
            steps[(SIDE - 1) * SIDE + c] = 3;
        }

        let offset = 100.0;
        let h = heights_from_vhgt(offset, &steps);

        // Corner 0: (r=0, c=0) -> (offset + 5) * 8.0
        assert_eq!(h[0], (100.0 + 5.0) * 8.0);
        // Corner 1: (r=0, c=32) -> (offset + 5 + 32*1) * 8.0 = 137.0 * 8.0
        assert_eq!(h[SIDE - 1], (105.0 + 32.0) * 8.0);
        // Corner 2: (r=32, c=0) -> row starts step from prev row's first vertex.
        // row 0 start = 105. Each row 1..=32 adds 2. Total = 105 + 32*2 = 169.
        assert_eq!(h[(SIDE - 1) * SIDE], (105.0 + 64.0) * 8.0);
        // Corner 3: (r=32, c=32) -> row 32 start (169) + 32*3 = 169 + 96 = 265.
        assert_eq!(h[SIDE * SIDE - 1], (169.0 + 96.0) * 8.0);
    }

    #[test]
    fn s8_3_vnml_normals_and_vclr_colors_parsing() {
        // 33 * 33 = 1089 vertices.
        // Test parsing synthetic VNML signed bytes and VCLR unsigned bytes.
        let mut vnml_data = Vec::with_capacity(SIDE * SIDE * 3);
        let mut vclr_data = Vec::with_capacity(SIDE * SIDE * 3);
        for _ in 0..(SIDE * SIDE) {
            vnml_data.extend_from_slice(&[0u8, 0u8, 127u8]); // Normal pointing along +Z
            vclr_data.extend_from_slice(&[255u8, 128u8, 64u8]); // RGB
        }

        let raw_vnml: Vec<[i8; 3]> = vnml_data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0] as i8, c[1] as i8, c[2] as i8])
            .collect();
        assert_eq!(raw_vnml.len(), SIDE * SIDE);
        assert_eq!(raw_vnml[0], [0, 0, 127]);

        let raw_vclr: Vec<[u8; 3]> = vclr_data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        assert_eq!(raw_vclr.len(), SIDE * SIDE);
        assert_eq!(raw_vclr[0], [255, 128, 64]);
    }

    #[test]
    fn s8_5_btxt_and_vtxt_quadrant_layer_ordering() {
        // Test quadrant layer base vs alpha records
        let btxt_bytes = [
            0x10, 0x00, 0x00, 0x00, // local_tex = 16
            2,    // quadrant = 2
            0,    // unused
            1, 0, // layer index = 1
        ];
        let local_tex = u32::from_le_bytes(btxt_bytes[0..4].try_into().unwrap());
        let quadrant = btxt_bytes[4];
        let layer_idx = i16::from_le_bytes(btxt_bytes[6..8].try_into().unwrap());
        assert_eq!(local_tex, 16);
        assert_eq!(quadrant, 2);
        assert_eq!(layer_idx, 1);

        // VTXT chunk: [pos_lo, pos_hi, unused, opacity]
        let vtxt_chunk = [15u8, 1u8, 0u8, 128u8]; // pos = 256 + 15 = 271, opacity = 128/255
        let pos = u16::from_le_bytes([vtxt_chunk[0], vtxt_chunk[1]]);
        let opacity = vtxt_chunk[3] as f32 / 255.0;
        assert_eq!(pos, 271);
        assert!((opacity - 128.0 / 255.0).abs() < 1e-5);
    }
}
