//! Reading a NIF's bytes into values.
//!
//! The reader is driven by the schema: a block's layout says what its fields
//! are, and this module reads them. Everything is little-endian, and every
//! read is checked against what is left, so a wrong layout is an error
//! rather than a wrong number.

use crate::header::V20_1_0_3;
use crate::value::Value;

/// A fault while reading.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// A read went past the end of the data.
    #[error("read of {wanted} bytes at offset {pos} exceeds length {len}")]
    OutOfBounds {
        /// Where the read started.
        pos: usize,
        /// How many bytes were wanted.
        wanted: usize,
        /// How many bytes there are.
        len: usize,
    },
    /// A half float was not a number.
    #[error("invalid half float {0:#06x}")]
    BadHalf(u16),
    /// A string was not Windows-1252.
    #[error("string is not Windows-1252")]
    BadString,
    /// The data did not hold what the layout promised.
    #[error("{0}")]
    Layout(String),
}

/// A reader over a NIF's bytes.
pub struct Reader<'a> {
    /// The bytes.
    pub data: &'a [u8],
    /// How far into them the reader is.
    pub pos: usize,
}

impl<'a> Reader<'a> {
    /// A reader over `data`, at its start.
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    /// How many bytes are left.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Read `n` bytes.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len());
        let Some(end) = end else {
            return Err(CodecError::OutOfBounds {
                pos: self.pos,
                wanted: n,
                len: self.data.len(),
            });
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// Read a number of the given width and signedness.
    pub fn int(&mut self, bytes: usize, signed: bool) -> Result<i64, CodecError> {
        let b = self.bytes(bytes)?;
        Ok(match (bytes, signed) {
            (1, false) => b[0] as i64,
            (1, true) => b[0] as i8 as i64,
            (2, false) => u16::from_le_bytes([b[0], b[1]]) as i64,
            (2, true) => i16::from_le_bytes([b[0], b[1]]) as i64,
            (4, false) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
            (4, true) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
            (8, false) => u64::from_le_bytes(b.try_into().unwrap()) as i64,
            (8, true) => i64::from_le_bytes(b.try_into().unwrap()),
            _ => return Err(CodecError::Layout(format!("no {bytes}-byte integer"))),
        })
    }

    /// Read a 32-bit float.
    pub fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    /// Read a 16-bit float — the half precision format, which is how a NIF
    /// stores most of its colours and normals.
    pub fn f16(&mut self) -> Result<f32, CodecError> {
        let b = self.bytes(2)?;
        Ok(half_to_f32(u16::from_le_bytes([b[0], b[1]])))
    }

    /// Read a block index: a signed 32-bit number, where -1 means none.
    pub fn reference(&mut self) -> Result<i32, CodecError> {
        Ok(self.int(4, true)? as i32)
    }

    /// Read a NUL-terminated string, in Windows-1252.
    pub fn string_z(&mut self) -> Result<String, CodecError> {
        let end = self.data[self.pos..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| self.pos + p)
            .ok_or(CodecError::BadString)?;
        let s = nvre_util::cp1252::decode(&self.data[self.pos..end]);
        self.pos = end + 1;
        Ok(s)
    }

    /// Read a string of `len` bytes, in Windows-1252.
    pub fn string_n(&mut self, len: usize) -> Result<String, CodecError> {
        Ok(nvre_util::cp1252::decode(self.bytes(len)?))
    }
}

/// A 16-bit float, widened.
///
/// The format is a sign bit, a five-bit exponent and a ten-bit mantissa. The
/// exponent's bias is 15, and the two exponents either side of it are the
/// subnormals and the infinities.
///
/// A NaN keeps its payload, so that the half it was read from can be
/// written back as the same half: a NIF's vertex data holds NaNs, and a
/// NaN that comes back as a different NaN is a file that no longer
/// re-encodes to the same bytes.
pub fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as i32;
    let mant = (bits & 0x03FF) as i32;

    let value = if exp == 0 {
        // A subnormal: no implicit leading one, and the exponent is the
        // smallest one, 1 - bias, which is -14.
        if mant == 0 {
            0.0
        } else {
            (mant as f32) * (2.0f32).powi(-24)
        }
    } else if exp == 0x1F {
        // An infinity, or a NaN.
        if mant == 0 {
            if sign == 1 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            }
        } else {
            // A NaN, with its payload carried into the wider format's
            // payload, so that it survives the trip back.
            f32::from_bits((sign << 31) | 0x7F80_0000 | ((mant as u32) << 13))
        }
    } else {
        (1.0 + (mant as f32) / 1024.0) * (2.0f32).powi(exp - 15)
    };
    if sign == 1 && exp != 0x1F {
        -value
    } else {
        value
    }
}

/// A 16-bit float, widened to the 64-bit float the value type holds.
///
/// A NaN is the one case where widening loses something: a signalling
/// NaN is quieted on the way, and the quieting sets the bit that the
/// payload's top bit would have been. So a NaN is widened by hand, with
/// its payload kept where a narrowing will not lose it.
pub fn half_bits_to_f64(bits: u16) -> f64 {
    let exp = (bits >> 10) & 0x1F;
    let mant = bits & 0x03FF;
    if exp == 0x1F && mant != 0 {
        return nan_half(bits);
    }
    half_to_f32(bits) as f64
}

/// The 64-bit float a NaN half becomes.
///
/// The half's ten payload bits are kept in the wider format's mantissa,
/// at the top, and the quiet bit is set so that the value is never a
/// signalling NaN. The inverse is [`half_from_nan`].
pub fn nan_half(bits: u16) -> f64 {
    let sign = ((bits as u64 >> 15) & 1) << 63;
    let payload = (bits as u64) & 0x3FF;
    f64::from_bits(sign | 0x7FF8_0000_0000_0000 | (1 << 51) | (payload << 41))
}

/// The half a NaN came from.
///
/// The inverse of [`nan_half`]: the payload is taken from where
/// [`nan_half`] put it, and the quiet bit is dropped.
pub fn half_from_nan(v: f64) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let payload = ((bits >> 41) & 0x3FF) as u16;
    sign | 0x7C00 | payload
}

/// A 32-bit float, widened to the 64-bit float the value type holds.
///
/// A NaN is widened by hand, as a half's is: a signalling NaN is quieted
/// on the way, and the quieting takes the bit the payload's top bit would
/// have been.
pub fn f32_bits_to_f64(bits: u32) -> f64 {
    let exp = (bits >> 23) & 0xFF;
    let mant = bits & 0x7F_FFFF;
    if exp == 0xFF && mant != 0 {
        return f32_nan(bits);
    }
    f32::from_bits(bits) as f64
}

/// The 64-bit float a NaN float becomes.
///
/// The float's 23 payload bits are kept in the wider format's mantissa,
/// at the top, and the quiet bit is set so that the value is never a
/// signalling NaN. The inverse is [`f32_from_nan`].
pub fn f32_nan(bits: u32) -> f64 {
    let sign = ((bits as u64 >> 31) & 1) << 63;
    let payload = (bits as u64) & 0x7F_FFFF;
    f64::from_bits(sign | 0x7FF8_0000_0000_0000 | (1 << 51) | (payload << 28))
}

/// The float a NaN came from.
///
/// The inverse of [`f32_nan`]: the payload is taken from where
/// [`f32_nan`] put it, and the quiet bit is dropped.
pub fn f32_from_nan(v: f64) -> u32 {
    let bits = v.to_bits();
    let sign = ((bits >> 32) & 0x8000_0000) as u32;
    let payload = ((bits >> 28) & 0x7F_FFFF) as u32;
    sign | 0x7F80_0000 | payload
}

/// A 32-bit float, narrowed to a 16-bit float.
///
/// Rounds to nearest, ties to even — the way a conversion instruction does,
/// so a value that goes out and comes back is the value that went in.
pub fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = (bits & 0x007F_FFFF) as i32;

    if exp == 0xFF {
        // An infinity, or a NaN.
        if mant == 0 {
            return sign | 0x7C00;
        }
        // A NaN. Its payload is carried down to the half's payload, so
        // that the half it was read from comes back as the same half: a
        // NIF's vertex data holds NaNs, and a NaN that changes is a file
        // that no longer re-encodes to the same bytes.
        let payload = (mant >> 13).max(1);
        return sign | 0x7C00 | (payload as u16 & 0x03FF);
    }

    // The exponent in the half's terms, and how far it has to move.
    let half_exp = exp - 127 + 15;
    if half_exp >= 0x1F {
        // Too large: an infinity.
        return sign | 0x7C00;
    }
    if half_exp <= 0 {
        if half_exp < -9 {
            // Too small even for a subnormal: zero.
            return sign;
        }
        // A subnormal. The value is (2^23 + mant) * 2^(half_exp - 14), and a
        // half's subnormal is m * 2^-24, so m is that divided out. The
        // division is a shift, and the bits shifted off decide the rounding.
        let full = (mant | 0x0080_0000) as u32;
        let shift = (14 - half_exp) as u32;
        let mut m = (full >> shift) as i32;
        let rem = full & ((1u32 << shift) - 1);
        let half = 1u32 << (shift - 1);
        // Round to nearest, ties to even: the bit below the cut decides, and
        // a tie goes to the even value.
        if rem > half || (rem == half && (m & 1) == 1) {
            m += 1;
        }
        return sign | (m as u16 & 0x03FF);
    }

    // A normal number: round the mantissa to ten bits.
    let mut m = mant >> 13;
    let rem = mant & 0x1FFF;
    if rem > 0x1000 || (rem == 0x1000 && (m & 1) == 1) {
        m += 1;
        if m == 0x400 {
            // The mantissa carried into the exponent.
            let e = half_exp + 1;
            if e >= 0x1F {
                return sign | 0x7C00;
            }
            return sign | ((e as u16) << 10);
        }
    }
    sign | ((half_exp as u16) << 10) | (m as u16 & 0x03FF)
}

/// Bytes as lower-case hex.
pub fn to_hex(b: &[u8]) -> String {
    nvre_util::sha256::to_hex(b)
}

/// Read a 32-bit float, as the 64-bit float the value type holds.
///
/// A NaN keeps its payload, so that the float it was read from can be
/// written back as the same float.
fn read_f32(r: &mut Reader<'_>) -> Result<f64, CodecError> {
    let bits = r.int(4, false)? as u32;
    Ok(crate::codec::f32_bits_to_f64(bits))
}

/// Read a value of the given shape.
///
/// This is the primitive level: the schema's `Prim` says what to read, and
/// this reads it.
pub fn read_prim(
    prim: crate::schema::Prim,
    r: &mut Reader<'_>,
    version: crate::header::Version,
) -> Result<Value, CodecError> {
    use crate::schema::Prim;
    Ok(match prim {
        Prim::U8 => Value::Num(r.int(1, false)? as f64),
        Prim::I8 => Value::Num(r.int(1, true)? as f64),
        Prim::U16 => Value::Num(r.int(2, false)? as f64),
        Prim::I16 => Value::Num(r.int(2, true)? as f64),
        Prim::U32 => Value::Num(r.int(4, false)? as f64),
        Prim::I32 => Value::Num(r.int(4, true)? as f64),
        Prim::U64 => {
            let b = r.bytes(8)?;
            Value::Int64(u64::from_le_bytes(b.try_into().unwrap()))
        }
        Prim::F32 => {
            let bits = r.int(4, false)? as u32;
            Value::Num(crate::codec::f32_bits_to_f64(bits))
        }
        Prim::Bool => {
            // A flag is a byte, and a byte above one is not a flag: the
            // Special Edition's `NiBoolInterpolator` stores 2 for "invalid",
            // and the value is kept as the number it is.
            let b = r.int(1, false)?;
            if b <= 1 {
                Value::Bool(b != 0)
            } else {
                Value::Num(b as f64)
            }
        }
        Prim::Ref => Value::Ref(r.reference()?),
        Prim::Ptr => Value::Ref(r.reference()?),
        Prim::Vec2 => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            Value::List(vec![Value::Num(a), Value::Num(b)])
        }
        Prim::Vec3 => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            let c = read_f32(r)?;
            Value::List(vec![Value::Num(a), Value::Num(b), Value::Num(c)])
        }
        Prim::Vec4 => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            let c = read_f32(r)?;
            let d = read_f32(r)?;
            Value::List(vec![
                Value::Num(a),
                Value::Num(b),
                Value::Num(c),
                Value::Num(d),
            ])
        }
        Prim::Mat33 => {
            let mut v = Vec::with_capacity(9);
            for _ in 0..9 {
                v.push(Value::Num(read_f32(r)?));
            }
            Value::List(v)
        }
        Prim::Color3 => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            let c = read_f32(r)?;
            Value::List(vec![Value::Num(a), Value::Num(b), Value::Num(c)])
        }
        Prim::Color4 => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            let c = read_f32(r)?;
            let d = read_f32(r)?;
            Value::List(vec![
                Value::Num(a),
                Value::Num(b),
                Value::Num(c),
                Value::Num(d),
            ])
        }
        Prim::ByteColor4 => {
            let b = r.bytes(4)?;
            Value::List(b.iter().map(|&x| Value::Num(x as f64)).collect())
        }
        Prim::String => {
            // A string is an index into the file's string table from
            // 20.1.0.3 on. Before that it is the text itself, written
            // with its length in front of it.
            if version >= V20_1_0_3 {
                Value::Num(r.int(4, false)? as f64)
            } else {
                let len = r.int(4, false)? as usize;
                Value::Str(r.string_n(len)?)
            }
        }
        Prim::SizedString => {
            let len = r.int(4, false)? as usize;
            Value::Str(r.string_n(len)?)
        }
        Prim::SizedString16 => {
            let len = r.int(2, false)? as usize;
            Value::Str(r.string_n(len)?)
        }
        Prim::Bytes => {
            let len = r.int(4, false)? as usize;
            Value::Bytes(r.bytes(len)?.to_vec())
        }
        Prim::HFloat => {
            let bits = r.int(2, false)? as u16;
            Value::Num(crate::codec::half_bits_to_f64(bits))
        }
        Prim::HalfVec2 => {
            let a = r.int(2, false)? as u16;
            let b = r.int(2, false)? as u16;
            Value::List(vec![
                Value::Num(crate::codec::half_bits_to_f64(a)),
                Value::Num(crate::codec::half_bits_to_f64(b)),
            ])
        }
        Prim::HalfVec3 => {
            let a = r.int(2, false)? as u16;
            let b = r.int(2, false)? as u16;
            let c = r.int(2, false)? as u16;
            Value::List(vec![
                Value::Num(crate::codec::half_bits_to_f64(a)),
                Value::Num(crate::codec::half_bits_to_f64(b)),
                Value::Num(crate::codec::half_bits_to_f64(c)),
            ])
        }
        Prim::HalfVec4 => {
            let a = r.int(2, false)? as u16;
            let b = r.int(2, false)? as u16;
            let c = r.int(2, false)? as u16;
            let d = r.int(2, false)? as u16;
            Value::List(vec![
                Value::Num(crate::codec::half_bits_to_f64(a)),
                Value::Num(crate::codec::half_bits_to_f64(b)),
                Value::Num(crate::codec::half_bits_to_f64(c)),
                Value::Num(crate::codec::half_bits_to_f64(d)),
            ])
        }
        Prim::TexCoord => {
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            Value::List(vec![Value::Num(a), Value::Num(b)])
        }
        Prim::Quat => {
            // A quaternion is stored w, x, y, z.
            let a = read_f32(r)?;
            let b = read_f32(r)?;
            let c = read_f32(r)?;
            let d = read_f32(r)?;
            Value::List(vec![
                Value::Num(a),
                Value::Num(b),
                Value::Num(c),
                Value::Num(d),
            ])
        }
        Prim::ByteVec3 => {
            // Three bytes, each a value in the range -1 to 1: a vertex's
            // normal or tangent, as the format stores them.
            let b = r.bytes(3)?;
            Value::List(
                b.iter()
                    .map(|&x| Value::Num(x as f64 / 127.5 - 1.0))
                    .collect(),
            )
        }
        Prim::U16Vec3 => {
            let a = r.int(2, false)?;
            let b = r.int(2, false)?;
            let c = r.int(2, false)?;
            Value::List(vec![
                Value::Num(a as f64),
                Value::Num(b as f64),
                Value::Num(c as f64),
            ])
        }
        Prim::NormByte => {
            // A byte holding a value in the range -1 to 1.
            //
            // The byte is unsigned: 0 is -1 and 255 is 1, and the scale is
            // 127.5 rather than 127, so that the whole range of the byte
            // maps onto the whole range of the float. The scale is what
            // makes the round trip exact — a value written back from
            // `b / 127.5 - 1` is the byte it came from.
            let b = r.bytes(1)?[0];
            Value::Num(b as f64 / 127.5 - 1.0)
        }
        Prim::Mat22 => {
            let mut v = Vec::with_capacity(4);
            for _ in 0..4 {
                v.push(Value::Num(read_f32(r)?));
            }
            Value::List(v)
        }
        Prim::Mat34 => {
            let mut v = Vec::with_capacity(12);
            for _ in 0..12 {
                v.push(Value::Num(read_f32(r)?));
            }
            Value::List(v)
        }
        Prim::Mat44 => {
            let mut v = Vec::with_capacity(16);
            for _ in 0..16 {
                v.push(Value::Num(read_f32(r)?));
            }
            Value::List(v)
        }
        Prim::Triangle => {
            // Three 16-bit vertex indices.
            let a = r.int(2, false)?;
            let b = r.int(2, false)?;
            let c = r.int(2, false)?;
            Value::List(vec![
                Value::Num(a as f64),
                Value::Num(b as f64),
                Value::Num(c as f64),
            ])
        }
        Prim::Align => {
            // Whatever is left, up to the next multiple of the alignment.
            // The alignment is the argument, which the caller passes in.
            Value::Bytes(Vec::new())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Prim;

    #[test]
    fn every_half_float_survives_the_round_trip() {
        // Every one of the 65,536 half floats, widened and narrowed.
        for bits in 0u16..=0xFFFF {
            let value = half_to_f32(bits);
            let back = f32_to_half(value);
            // The round trip is by the bits: a NaN is not equal to itself,
            // and a NaN's payload has to survive too, or a file that holds
            // one no longer re-encodes to the same bytes.
            assert_eq!(back, bits, "{bits:#06x} became {back:#06x}");
        }
    }

    #[test]
    fn a_half_float_nan_keeps_its_payload() {
        // A NaN with a payload, and one without: both come back as the
        // same half.
        for bits in [0x7E00u16, 0x7FFF, 0xFE00, 0xFFFF] {
            let value = half_to_f32(bits);
            assert!(value.is_nan(), "{bits:#06x} is not a NaN");
            assert_eq!(f32_to_half(value), bits, "{bits:#06x} changed");
        }
    }

    #[test]
    fn a_half_float_nan_survives_the_value_type() {
        // Every NaN half, through the 64-bit float the value type holds.
        //
        // A signalling NaN is quieted on the way from the half to the
        // 32-bit float, and the quieting takes the bit the payload's top
        // bit would have been. So the payload is kept by hand, and every
        // NaN half comes back as itself.
        for bits in 0x7C01u16..=0x7FFF {
            for bits in [bits, bits | 0x8000] {
                let value = half_bits_to_f64(bits);
                assert!(value.is_nan(), "{bits:#06x} is not a NaN");
                assert_eq!(half_from_nan(value), bits, "{bits:#06x} changed");
            }
        }
    }

    #[test]
    fn a_half_float_infinity_keeps_its_sign() {
        assert!(half_to_f32(0x7C00).is_infinite());
        assert!(half_to_f32(0x7C00).is_sign_positive());
        assert!(half_to_f32(0xFC00).is_infinite());
        assert!(half_to_f32(0xFC00).is_sign_negative());
        assert_eq!(f32_to_half(f32::INFINITY), 0x7C00);
        assert_eq!(f32_to_half(f32::NEG_INFINITY), 0xFC00);
    }

    #[test]
    fn a_half_float_reads_its_value() {
        // 1.0 in a half float is 0x3C00.
        let mut r = Reader::new(&[0x00, 0x3C]);
        assert_eq!(r.f16().unwrap(), 1.0);
        // -2.0 is 0xC000.
        let mut r = Reader::new(&[0x00, 0xC0]);
        assert_eq!(r.f16().unwrap(), -2.0);
        // 0.5 is 0x3800.
        let mut r = Reader::new(&[0x00, 0x38]);
        assert_eq!(r.f16().unwrap(), 0.5);
    }

    #[test]
    fn a_read_past_the_end_is_an_error() {
        let mut r = Reader::new(&[1, 2]);
        assert!(r.bytes(4).is_err());
        // And the reader did not move.
        assert_eq!(r.pos, 0);
    }

    #[test]
    fn a_nul_terminated_string_stops_at_the_nul() {
        let mut r = Reader::new(b"hello\0world");
        assert_eq!(r.string_z().unwrap(), "hello");
        assert_eq!(r.pos, 6);
    }

    #[test]
    fn a_string_before_the_string_table_carries_its_length() {
        // Before 20.1.0.3 a string is the text itself, with a four-byte
        // length in front of it, and not a nul at the end.
        let mut r = Reader::new(&[5, 0, 0, 0, b'h', b'e', b'l', b'l', b'o']);
        let v = read_prim(Prim::String, &mut r, 0x1400_0004).unwrap();
        assert_eq!(v.as_str().unwrap(), "hello");
        assert_eq!(r.pos, 9);
    }

    #[test]
    fn a_string_after_the_string_table_is_an_index() {
        // From 20.1.0.3 a string is an index into the file's table.
        let mut r = Reader::new(&[7, 0, 0, 0]);
        let v = read_prim(Prim::String, &mut r, 0x1401_0003).unwrap();
        assert_eq!(v.as_number().unwrap(), 7.0);
    }

    #[test]
    fn a_block_index_of_minus_one_is_none() {
        let mut r = Reader::new(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.reference().unwrap(), -1);
    }
}
