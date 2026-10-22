//! NVRE's world reader.
//!
//! A game world is a grid of **cells**, and a cell holds **placed
//! references**: the objects, actors and lights that stand in it. Around
//! the cells is the **landscape**, the terrain.
//!
//! The pieces:
//!
//! - [`lib`] — the cell grid: how a position maps to a cell.
//! - [`index`] — which cells belong to which worldspace, and which
//!   references to which cell, across a load order.
//! - [`fields`] — reading a record's fields, and resolving a FormID to the
//!   record that wins.
//! - [`placed`] — where a reference stands, and what it is.
//! - [`transform`] — the rotation maths, in the order the game stores it.
//! - [`land`] — the landscape's terrain.

pub mod fields;
pub mod index;

pub use index::{Cell, RefEntry, WorldIndex, Worldspace};
pub mod land;
pub mod placed;
pub mod transform;

/// The size of a cell, in world units. The grid is 4096 units square.
pub const CELL_SIZE: f32 = 4096.0;

/// The cell a world position is in.
///
/// The grid runs from negative to positive on both axes, so a position on a
/// boundary belongs to the cell that starts there.
pub fn cell_of(x: f32, y: f32) -> (i32, i32) {
    (
        (x / CELL_SIZE).floor() as i32,
        (y / CELL_SIZE).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_in_its_cell() {
        assert_eq!(cell_of(0.0, 0.0), (0, 0));
        assert_eq!(cell_of(4095.9, 4096.0), (0, 1));
        assert_eq!(cell_of(-0.1, -4096.0), (-1, -1));
        assert_eq!(cell_of(-4096.0, -4096.0), (-1, -1));
    }
}
