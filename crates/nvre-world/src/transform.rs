//! The rotation convention the game stores.
//!
//! A reference stores three Euler angles in radians — x, y, z — and the
//! matrix they make is `M = Rx(-x) Ry(-y) Rz(-z)`: each angle is clockwise,
//! and the matrices are multiplied left to right, so the z rotation is
//! applied to the vector first.
//!
//! That is not a guess. Static collections ship their combined mesh and the
//! placement of each of their parts, and under this convention the two agree
//! for 286 of the 308 parts that carry a rotation. The other conventions fit
//! far fewer, and the ones that do not fit are six collections that are
//! known to be laid out by hand.
//!
//! The matrices here are row-major 3×3, rotating column vectors
//! (`v' = M v`), which is the convention a NIF's own transforms use.

/// A row-major 3×3 rotation matrix.
pub type Mat3 = [f32; 9];

/// The matrix that changes nothing.
pub const IDENTITY: Mat3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Multiply two matrices.
pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut m = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    m
}

/// The transpose of a matrix.
pub fn transpose(a: &Mat3) -> Mat3 {
    [a[0], a[3], a[6], a[1], a[4], a[7], a[2], a[5], a[8]]
}

/// A rotation of `a` radians about an axis, looking down the axis towards
/// the origin.
pub fn axis(i: usize, a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    match i {
        0 => [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c],
        1 => [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c],
        _ => [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
    }
}

/// The rotation a reference's three angles make.
///
/// The convention is `M = Rx(-x) Ry(-y) Rz(-z)`: each angle is negated, and
/// the matrices are multiplied left to right, so the z rotation is applied to
/// the vector first.
pub fn ref_rotation(angles: [f32; 3]) -> Mat3 {
    // Each angle belongs to its axis, and each is negated because the
    // rotation is clockwise.
    let m = |i: usize| axis(i, -angles[i]);
    mul(&mul(&m(0), &m(1)), &m(2))
}

/// The angles a rotation matrix is made of.
///
/// The matrix is `M = Rx(a) Ry(b) Rz(c)` with `(a, b, c) = -(x, y, z)`, so
/// `m[2] = sin b`, `m[5] = -sin a cos b` and `m[1] = -cos b sin c`. When
/// `cos b` is zero only `a + c` is defined, and the z angle is set to zero.
///
/// The angles come out in [-π, π], with the y angle in [-π/2, π/2], so they
/// need not equal the stored ones — only give the same matrix. They are the
/// *stored* angles, the ones a reference holds, so the signs are the other
/// way round from the matrix's own.
pub fn ref_angles(m: &Mat3) -> [f32; 3] {
    let sb = m[2].clamp(-1.0, 1.0);
    let b = sb.asin();
    let (a, c) = if sb.abs() < 1.0 - 1e-6 {
        ((-m[5]).atan2(m[8]), (-m[1]).atan2(m[0]))
    } else {
        ((-m[7]).atan2(m[6]), 0.0)
    };
    // The matrix's own angles are the negated ones, so the stored angles are
    // the other way round.
    [-a, -b, -c]
}

/// The largest difference between two matrices, element by element.
pub fn max_diff(a: &Mat3, b: &Mat3) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_changes_nothing() {
        let m = ref_rotation([0.0, 0.0, 0.0]);
        for (a, b) in m.iter().zip(IDENTITY.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn a_quarter_turn_about_z_is_the_matrix_it_should_be() {
        // A quarter turn about z, clockwise: the matrix is
        // [c, -s, 0, s, c, 0, 0, 0, 1] with the angle negated.
        let m = ref_rotation([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        let (s, c) = (-std::f32::consts::FRAC_PI_2).sin_cos();
        assert!((m[0] - c).abs() < 1e-6);
        assert!((m[1] + s).abs() < 1e-6);
        assert!((m[3] - s).abs() < 1e-6);
    }

    #[test]
    fn the_angles_a_matrix_makes_come_back() {
        // The angles that come back need not equal the ones that went in —
        // only give the same matrix.
        for angles in [
            [0.3, -0.7, 1.1],
            [0.0, 0.0, 0.0],
            [1.0, 0.5, -0.25],
            [0.0, 1.2, 0.0],
        ] {
            let m = ref_rotation(angles);
            let back = ref_angles(&m);
            assert!(
                max_diff(&m, &ref_rotation(back)) < 1e-5,
                "{angles:?} came back as {back:?}"
            );
        }
    }
}
