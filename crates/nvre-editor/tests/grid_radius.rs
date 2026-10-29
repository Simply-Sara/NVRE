//! Tests for S7-5 and S7-6: Reference resolution failure accounting and grid radius formula.

use nvre_editor::session::Session;

#[test]
fn s7_6_grid_radius_draws_exact_square_of_cells() {
    for r in 0..=10 {
        let count = Session::grid_cell_count(r);
        let side = (2 * r + 1) as usize;
        assert_eq!(count, side * side, "radius {r} must cover exactly (2r+1)^2 cells");
    }
    assert_eq!(Session::grid_cell_count(0), 1);
    assert_eq!(Session::grid_cell_count(1), 9);
    assert_eq!(Session::grid_cell_count(2), 25);
    assert_eq!(Session::grid_cell_count(3), 49);
}
