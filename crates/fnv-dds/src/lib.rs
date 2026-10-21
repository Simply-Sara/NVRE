//! NVRE's DDS texture reader.
//!
//! DDS is a DirectDraw Surface file: a 32-byte `DDS ` magic, then a
//! 128-byte header describing the surface, then the pixel data.
//!
//! What is here:
//!
//! - **The header** — the size, the pixel format, the mip count and the
//!   cube-map flags.
//! - **The layout** — where each mip level of each face sits in the
//!   payload, so a reader can find a level without decoding it.
//! - **`to_rgba8`** — the three uncompressed layouts, converted to RGBA8.
//! - **`decode_blocks`** — DXT1, DXT3 and DXT5 decoded on the CPU, for the
//!   few places that need the pixels themselves rather than the compressed
//!   blocks.
//!
//! The block-compressed formats are meant to reach the GPU as they are; this
//! crate does not re-encode them.

/// A fault in a DDS file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DdsError {
    /// The file does not start with `DDS `.
    #[error("not a DDS file (magic {0:02x?})")]
    BadMagic([u8; 4]),
    /// The file is shorter than a header.
    #[error("file too short for a DDS header ({0} bytes)")]
    Truncated(usize),
    /// The pixel format is one this reader does not know.
    #[error("unsupported pixel format {0}")]
    Unsupported(String),
    /// The payload is not the size the layout calls for.
    #[error("pixel data is {actual} bytes, the layout needs {expected}")]
    SizeMismatch {
        /// The size the layout needs.
        expected: usize,
        /// The size the payload is.
        actual: usize,
    },
}

/// The pixel format as the header describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    /// The format flags.
    pub flags: u32,
    /// A four-character code, for the block-compressed formats.
    pub fourcc: [u8; 4],
    /// The bits per pixel.
    pub rgb_bits: u32,
    /// The red, green, blue and alpha masks.
    pub masks: [u32; 4],
}

/// A pixel format this reader understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// DXT1: 4 bits of alpha in the colour block, one bit of transparency.
    Dxt1,
    /// DXT3: 4 bits of explicit alpha, then DXT1's colours.
    Dxt3,
    /// DXT5: 8 bits of interpolated alpha, then DXT1's colours.
    Dxt5,
    /// 32 bits: 8 alpha, 8 red, 8 green, 8 blue.
    A8R8G8B8,
    /// 24 bits: 8 red, 8 green, 8 blue.
    R8G8B8,
    /// 16 bits: 4 alpha, 4 red, 4 green, 4 blue.
    A4R4G4B4,
}

impl Format {
    /// The format's name.
    pub fn name(self) -> &'static str {
        match self {
            Format::Dxt1 => "DXT1",
            Format::Dxt3 => "DXT3",
            Format::Dxt5 => "DXT5",
            Format::A8R8G8B8 => "A8R8G8B8",
            Format::R8G8B8 => "R8G8B8",
            Format::A4R4G4B4 => "A4R4G4B4",
        }
    }

    /// The bytes a 4×4 block takes, for the block formats.
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Format::Dxt1 => Some(8),
            Format::Dxt3 | Format::Dxt5 => Some(16),
            _ => None,
        }
    }

    /// The bytes a level of `w`×`h` takes.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w as usize, h as usize);
        match self.block_bytes() {
            Some(b) => w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * b,
            None => {
                let bpp = match self {
                    Format::A8R8G8B8 => 4,
                    Format::R8G8B8 => 3,
                    _ => 2,
                };
                w * h * bpp
            }
        }
    }
}

/// The pixel format is carried by a four-character code.
const PF_FOURCC: u32 = 0x4;
/// The pixel format is carried by masks.
const PF_RGB: u32 = 0x40;
/// The format has an alpha channel.
const PF_ALPHA_PIXELS: u32 = 0x1;
/// The surface is a cube map.
const CAPS2_CUBEMAP: u32 = 0x200;
/// The cube map has all six faces.
const CAPS2_ALL_FACES: u32 = 0xFC00;

impl PixelFormat {
    /// The format, or a description of one this reader does not know.
    pub fn format(&self) -> Result<Format, String> {
        if self.flags == PF_FOURCC {
            return match &self.fourcc {
                b"DXT1" => Ok(Format::Dxt1),
                b"DXT3" => Ok(Format::Dxt3),
                b"DXT5" => Ok(Format::Dxt5),
                _ => Err(self.describe()),
            };
        }
        if self.flags == PF_RGB || self.flags == PF_RGB | PF_ALPHA_PIXELS {
            let alpha = self.flags & PF_ALPHA_PIXELS != 0;
            return match (self.rgb_bits, self.masks, alpha) {
                (32, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000], true) => {
                    Ok(Format::A8R8G8B8)
                }
                (24, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, _], false) => Ok(Format::R8G8B8),
                (16, [0x0F00, 0x00F0, 0x000F, 0xF000], true) => Ok(Format::A4R4G4B4),
                _ => Err(self.describe()),
            };
        }
        Err(self.describe())
    }

    /// A description of an unknown format.
    pub fn describe(&self) -> String {
        format!(
            "unknown(flags=0x{:x},fourcc={},bits={},masks={:08x}/{:08x}/{:08x}/{:08x})",
            self.flags,
            String::from_utf8_lossy(&self.fourcc).replace('\0', ""),
            self.rgb_bits,
            self.masks[0],
            self.masks[1],
            self.masks[2],
            self.masks[3]
        )
    }
}

/// The 128-byte DDS header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsHeader {
    /// The header flags.
    pub flags: u32,
    /// The height in pixels.
    pub height: u32,
    /// The width in pixels.
    pub width: u32,
    /// The pitch, or the total size for a compressed surface.
    pub pitch_or_linear_size: u32,
    /// The depth, for a volume texture.
    pub depth: u32,
    /// The number of mip levels.
    pub mip_count: u32,
    /// The pixel format.
    pub pixel_format: PixelFormat,
    /// The four capability words.
    pub caps: [u32; 4],
}

/// The length of the header, after the four-byte magic.
pub const HEADER_LEN: usize = 128;

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl DdsHeader {
    /// Read the header.
    pub fn parse(data: &[u8]) -> Result<DdsHeader, DdsError> {
        if data.len() >= 4 && &data[..4] != b"DDS " {
            return Err(DdsError::BadMagic(data[..4].try_into().unwrap()));
        }
        if data.len() < HEADER_LEN {
            return Err(DdsError::Truncated(data.len()));
        }
        // The fields follow the magic, so field `i` is at 4 + 4*i.
        let h = |i: usize| u32_at(data, 4 + 4 * i);
        Ok(DdsHeader {
            flags: h(1),
            height: h(2),
            width: h(3),
            pitch_or_linear_size: h(4),
            depth: h(5),
            mip_count: h(6),
            pixel_format: PixelFormat {
                flags: u32_at(data, 80),
                fourcc: data[84..88].try_into().unwrap(),
                rgb_bits: u32_at(data, 88),
                masks: [
                    u32_at(data, 92),
                    u32_at(data, 96),
                    u32_at(data, 100),
                    u32_at(data, 104),
                ],
            },
            caps: [
                u32_at(data, 108),
                u32_at(data, 112),
                u32_at(data, 116),
                u32_at(data, 120),
            ],
        })
    }

    /// The mip levels in the file: the stored count, or one when it is zero.
    pub fn levels(&self) -> u32 {
        self.mip_count.max(1)
    }

    /// Whether the surface is a cube map.
    pub fn is_cubemap(&self) -> bool {
        self.caps[1] & CAPS2_CUBEMAP != 0
    }

    /// The number of faces.
    pub fn faces(&self) -> u32 {
        if self.is_cubemap() {
            (self.caps[1] & CAPS2_ALL_FACES).count_ones().max(1)
        } else {
            1
        }
    }

    /// The width and height of mip level `i`.
    pub fn level_dims(&self, i: u32) -> (u32, u32) {
        (
            self.width.checked_shr(i).unwrap_or(0).max(1),
            self.height.checked_shr(i).unwrap_or(0).max(1),
        )
    }

    /// The payload bytes the layout needs: every face, every level.
    pub fn expected_size(&self, f: Format) -> usize {
        let per_face = (0..self.levels().min(32)).fold(0usize, |acc, i| {
            let (w, h) = self.level_dims(i);
            acc.saturating_add(f.level_size(w, h))
        });
        per_face.saturating_mul(self.faces() as usize)
    }
}

/// A parsed texture, with its level slices.
#[derive(Debug, Clone)]
pub struct Dds<'a> {
    /// The header.
    pub header: DdsHeader,
    /// The pixel format.
    pub format: Format,
    /// `levels[face][mip]`, each a slice of the payload.
    pub levels: Vec<Vec<&'a [u8]>>,
}

impl<'a> Dds<'a> {
    /// Parse a whole texture.
    pub fn parse(data: &'a [u8]) -> Result<Dds<'a>, DdsError> {
        let header = DdsHeader::parse(data)?;
        let format = header
            .pixel_format
            .format()
            .map_err(DdsError::Unsupported)?;
        let payload = &data[HEADER_LEN..];
        let expected = header.expected_size(format);
        // More than 32 levels is not a real surface, and the size check
        // would be meaningless.
        if payload.len() != expected || header.levels() > 32 {
            return Err(DdsError::SizeMismatch {
                expected,
                actual: payload.len(),
            });
        }
        let mut levels = Vec::new();
        let mut off = 0;
        for _ in 0..header.faces() {
            let mut face = Vec::new();
            for i in 0..header.levels() {
                let (w, h) = header.level_dims(i);
                let n = format.level_size(w, h);
                face.push(&payload[off..off + n]);
                off += n;
            }
            levels.push(face);
        }
        Ok(Dds {
            header,
            format,
            levels,
        })
    }
}

/// Convert one level of an uncompressed format to RGBA8.
///
/// `None` for the block-compressed formats, which are not converted.
pub fn to_rgba8(format: Format, src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        Format::A8R8G8B8 => {
            out.reserve(src.len());
            for p in src.as_chunks::<4>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Format::R8G8B8 => {
            out.reserve(src.len() / 3 * 4);
            for p in src.as_chunks::<3>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        Format::A4R4G4B4 => {
            out.reserve(src.len() * 2);
            for p in src.as_chunks::<2>().0 {
                let v = u16::from_le_bytes([p[0], p[1]]);
                // Four bits per channel, widened to eight by the 0..15 -> 0..255
                // mapping that repeats the nibble: 0xF becomes 0xFF.
                let ch = |shift: u16| (((v >> shift) & 0xF) as u8) * 17;
                out.extend_from_slice(&[ch(8), ch(4), ch(0), ch(12)]);
            }
        }
        _ => return None,
    }
    Some(out)
}

/// A colour as RGB565, widened to eight bits a channel.
fn rgb565(v: u16) -> [u8; 3] {
    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}

/// The 16 colours of a DXT colour block.
///
/// The two endpoint colours are in the first four bytes, as RGB565, and the
/// two-bit indices of the 16 texels are in the next four. DXT1 has a
/// three-colour mode, used when the first endpoint is not the larger, in
/// which the fourth colour is transparent; DXT3 and DXT5 always use the
/// four-colour mode, because their alpha comes from elsewhere.
fn color_block(b: &[u8], four_always: bool) -> [[u8; 4]; 16] {
    let (c0, c1) = (
        u16::from_le_bytes([b[0], b[1]]),
        u16::from_le_bytes([b[2], b[3]]),
    );
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| {
        ((u16::from(a) * wa + u16::from(b) * wb) / (wa + wb)) as u8
    };
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || four_always {
        // Two-thirds and one-third of the way from each endpoint.
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 2, 1);
            pal[3][k] = mix(p0[k], p1[k], 1, 2);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        // The three-colour mode: one colour halfway, and a transparent one.
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 1, 1);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|i| pal[((bits >> (2 * i)) & 3) as usize])
}

/// The 16 alphas of a DXT5 alpha block.
fn alpha_block5(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(b[0]), u16::from(b[1]));
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    if a0 > a1 {
        // Six interpolated values between the two endpoints.
        for i in 1..7u16 {
            pal[i as usize + 1] = ((a0 * (7 - i) + a1 * i) / 7) as u8;
        }
    } else {
        // Four interpolated values, then a hard zero and a hard one.
        for i in 1..5u16 {
            pal[i as usize + 1] = ((a0 * (5 - i) + a1 * i) / 5) as u8;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    // The 48 bits of indices, most significant first.
    let bits = b[2..8]
        .iter()
        .rev()
        .fold(0u64, |acc, &x| (acc << 8) | u64::from(x));
    std::array::from_fn(|i| pal[((bits >> (3 * i)) & 7) as usize])
}

/// Decode one level of DXT1, DXT3 or DXT5 data of `w`×`h` texels to RGBA8,
/// row-major.
///
/// `None` for any other format, or when the data is too short.
pub fn decode_blocks(format: Format, w: u32, h: u32, src: &[u8]) -> Option<Vec<u8>> {
    let block = format
        .block_bytes()
        .filter(|_| matches!(format, Format::Dxt1 | Format::Dxt3 | Format::Dxt5))?;
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    if src.len() < bw * bh * block {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let b = &src[(by * bw + bx) * block..][..block];
            let texels: [[u8; 4]; 16] = match format {
                Format::Dxt1 => color_block(b, false),
                Format::Dxt3 => {
                    // Four bits of explicit alpha per texel, low nibble first.
                    let mut c = color_block(&b[8..], true);
                    for (i, t) in c.iter_mut().enumerate() {
                        let nib = (b[i / 2] >> (4 * (i % 2))) & 15;
                        t[3] = nib * 17;
                    }
                    c
                }
                _ => {
                    let a = alpha_block5(b);
                    let mut c = color_block(&b[8..], true);
                    for (t, a) in c.iter_mut().zip(a) {
                        t[3] = a;
                    }
                    c
                }
            };
            for (i, t) in texels.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                // The last block of an edge is clipped to the surface.
                if x < w && y < h {
                    out[(y * w + x) * 4..][..4].copy_from_slice(t);
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn dxt_blocks_decode_to_their_colours_and_alphas() {
        // DXT1: c0 white (0xFFFF) > c1 black, so the four-colour mode, with
        // the first row indexed 0, 1, 2, 3.
        let b = [0xFF, 0xFF, 0x00, 0x00, 0b11_10_01_00, 0, 0, 0];
        let px = decode_blocks(Format::Dxt1, 4, 4, &b).unwrap();
        assert_eq!(
            &px[0..16],
            &[255, 255, 255, 255, 0, 0, 0, 255, 170, 170, 170, 255, 85, 85, 85, 255]
        );
        // c0 <= c1: the three-colour mode, whose fourth colour is transparent.
        let b = [0x00, 0x00, 0xFF, 0xFF, 0b11, 0, 0, 0];
        assert_eq!(
            &decode_blocks(Format::Dxt1, 4, 4, &b).unwrap()[0..4],
            &[0, 0, 0, 0]
        );
        // DXT5: alpha 255 and 0, with index 1 (0) on the first texel and
        // index 0 (255) on the second.
        let mut b = [0u8; 16];
        b[0] = 255;
        b[1] = 0;
        b[2] = 1;
        b[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let px = decode_blocks(Format::Dxt5, 4, 4, &b).unwrap();
        assert_eq!((px[3], px[7]), (0, 255));
        // DXT3: explicit four-bit alphas.
        let mut b = [0u8; 16];
        b[0] = 0xF0;
        let px = decode_blocks(Format::Dxt3, 4, 4, &b).unwrap();
        assert_eq!((px[3], px[7]), (0, 255));
        // And the uncompressed formats are not block-decoded.
        assert!(decode_blocks(Format::A8R8G8B8, 4, 4, &b).is_none());
    }

    #[test]
    fn the_uncompressed_formats_convert_to_rgba8() {
        // A8R8G8B8 is stored as B, G, R, A in memory.
        assert_eq!(
            to_rgba8(Format::A8R8G8B8, &[1, 2, 3, 4]).unwrap(),
            vec![3, 2, 1, 4]
        );
        // R8G8B8 has no alpha.
        assert_eq!(
            to_rgba8(Format::R8G8B8, &[1, 2, 3]).unwrap(),
            vec![3, 2, 1, 255]
        );
        // A4R4G4B4 widens each nibble by 17. The channel order is A, R, G, B
        // from the high nibble down, so 0xF00F is blue 15, alpha 15, and
        // red and green zero — and the output is RGBA, so blue comes first.
        assert_eq!(
            to_rgba8(Format::A4R4G4B4, &[0x0F, 0xF0]).unwrap(),
            vec![0, 0, 255, 255]
        );
        // And 0x0F0F is red 15 and blue 15, with no alpha.
        assert_eq!(
            to_rgba8(Format::A4R4G4B4, &[0x0F, 0x0F]).unwrap(),
            vec![255, 0, 255, 0]
        );
    }

    #[test]
    fn a_file_without_the_magic_is_refused() {
        let e = DdsHeader::parse(b"XXXX\0\0\0\0").unwrap_err();
        assert_eq!(e, DdsError::BadMagic(*b"XXXX"));
    }

    #[test]
    fn a_short_file_is_refused() {
        assert_eq!(
            DdsHeader::parse(b"DDS ").unwrap_err(),
            DdsError::Truncated(4)
        );
    }

    proptest! {
        /// Whatever bytes go in, the layout says how big the payload is.
        #[test]
        fn the_layout_matches_the_payload(
            w in 1u32..256,
            h in 1u32..256,
            mips in 1u32..8,
        ) {
            let f = Format::Dxt5;
            let mut d = vec![0u8; HEADER_LEN];
            d[..4].copy_from_slice(b"DDS ");
            let put = |d: &mut Vec<u8>, o: usize, v: u32| {
                d[o..o + 4].copy_from_slice(&v.to_le_bytes())
            };
            put(&mut d, 4, 124);
            put(&mut d, 8, 0x1007 | 0x20000);
            put(&mut d, 12, h);
            put(&mut d, 16, w);
            put(&mut d, 28, mips);
            put(&mut d, 76, 32);
            put(&mut d, 80, PF_FOURCC);
            d[84..88].copy_from_slice(b"DXT5");
            let header = DdsHeader::parse(&d).unwrap();
            prop_assert_eq!(header.width, w);
            prop_assert_eq!(header.height, h);
            prop_assert_eq!(header.levels(), mips);
            let expected = header.expected_size(f);
            d.extend(std::iter::repeat(0u8).take(expected));
            let parsed = Dds::parse(&d);
            prop_assert!(parsed.is_ok());
        }
    }
}
