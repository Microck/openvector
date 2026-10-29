//! Port of `src/bmp-input.mjs`: first BMP raster with original Q8 behavior.

use crate::pixel_input::{fail, image_size, DecodedImage, InputError};

fn u16le(b: &[u8], o: usize) -> u32 {
    b[o] as u32 | (b[o + 1] as u32) << 8
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u16le(b, o) | u16le(b, o + 2) << 16
}

/// Node `ascii` decoding clears each byte's high bit.
pub(crate) fn ascii_pair(b: &[u8], o: usize) -> [u8; 2] {
    [b[o] & 0x7f, b[o + 1] & 0x7f]
}

/// Original bitfield expansion promotes to 16 bits before Q8 rounding.
#[derive(Clone, Copy)]
struct MaskQuantum {
    mask: u32,
    shift: u32,
    width: u32,
    bits: u32,
    channel: usize,
}

impl MaskQuantum {
    fn new(mask: u32, bits: u32, channel: usize) -> Result<Self, InputError> {
        let shift = if mask != 0 { mask.leading_zeros() } else { 0 };
        let width = (!(mask.wrapping_shl(shift))).leading_zeros();
        if width == 32 {
            return fail("Unsupported BMP full-word mask.");
        }
        Ok(MaskQuantum { mask, shift, width, bits, channel })
    }
    fn apply(&self, pixel: u32) -> u8 {
        let mut word = (pixel & self.mask).wrapping_shl(self.shift) >> 16;
        let width = self.width;
        if self.bits == 16 {
            if width == 5 && self.channel != 3 {
                word |= (word & 0xe000) >> 5;
            }
            if width == 6 && self.channel == 1 {
                word |= (word & 0xc000) >> 6;
            }
            if width <= 8 {
                word |= (word & 0xff00) >> 8;
            }
        } else if width == 8 {
            word |= word >> 8;
        }
        ((word + 128) / 257) as u8
    }
}

/// RLE stores one palette byte per output pixel in a tight raster.
fn decode_rle(encoded: &[u8], mut offset: usize, width: usize, height: usize, bits: u32) -> Result<Vec<u8>, InputError> {
    let total = width * height;
    let mut indices = vec![0u8; total];
    let mut read = || -> Result<u8, InputError> {
        if offset >= encoded.len() {
            return fail("BMP RLE command is incomplete.");
        }
        offset += 1;
        Ok(encoded[offset - 1])
    };
    let (mut x, mut y, mut position) = (0usize, 0usize, 0usize);
    while y < height && position < total {
        let count = read()? as usize;
        let value = read()?;
        if count != 0 {
            let pixels = count.min(total - position);
            for index in 0..pixels {
                indices[position] = if bits == 8 { value } else if index & 1 != 0 { value & 15 } else { value >> 4 };
                position += 1;
            }
            x += pixels;
        } else if value == 1 {
            break;
        } else if value == 0 {
            x = 0;
            y += 1;
            position = y * width;
        } else if value == 2 {
            x += read()? as usize;
            y += read()? as usize;
            position = y * width + x;
        } else {
            let pixels = (value as usize).min(total - position);
            let mut byte = 0u8;
            for index in 0..pixels {
                if bits == 8 || index & 1 == 0 {
                    byte = read()?;
                }
                indices[position] = if bits == 8 { byte } else if index & 1 != 0 { byte & 15 } else { byte >> 4 };
                position += 1;
            }
            x += pixels;
            if (if bits == 8 { pixels } else { pixels.div_ceil(2) }) & 1 != 0 {
                read()?;
            }
        }
    }
    Ok(indices)
}

/// Decode the first BMP raster (`decodeBmp`).
pub fn decode_bmp(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    let len = encoded.len();
    let mut file_header = 0usize;
    while len >= file_header + 2 && ascii_pair(encoded, file_header) == *b"BA" {
        if file_header + 14 > len {
            return fail("BMP array header is incomplete.");
        }
        file_header += 14;
    }
    if len < file_header + 26 {
        return fail("BMP header is incomplete.");
    }
    let id = ascii_pair(encoded, file_header);
    if id != *b"BM" && id != *b"CI" {
        return fail("BMP bitmap identifier is missing.");
    }
    let fh = file_header;
    let pixel_offset = u32le(encoded, fh + 10) as usize;
    let header_size = u32le(encoded, fh + 14) as usize;
    let core = header_size == 12;
    if !core && header_size < 40 {
        return fail("This BMP header version is not implemented.");
    }
    if fh + 14 + header_size > len {
        return fail("BMP information header is incomplete.");
    }
    let width = if core { u16le(encoded, fh + 18) as i16 as i64 } else { u32le(encoded, fh + 18) as i32 as i64 };
    let signed_height = if core { u16le(encoded, fh + 20) as i16 as i64 } else { u32le(encoded, fh + 22) as i32 as i64 };
    let height = signed_height.abs();
    let pixels = image_size(width, height)?;
    let (width, height) = (width as usize, height as usize);
    let planes = u16le(encoded, fh + if core { 22 } else { 26 });
    let bits = u16le(encoded, fh + if core { 24 } else { 28 });
    let compression = if core { 0 } else { u32le(encoded, fh + 30) };
    let used = if core { 0 } else { u32le(encoded, fh + 46) as usize };
    if planes != 1 || ![1, 4, 8, 16, 24, 32].contains(&bits) || compression > 3 || compression == 3 && bits < 16
        || compression == 1 && bits != 8 || compression == 2 && bits != 4
    {
        return fail("This BMP pixel encoding is not implemented.");
    }
    let mut masks: [u32; 4] = if bits == 16 { [0x7c00, 0x3e0, 0x1f, 0] } else { [0xff0000, 0xff00, 0xff, 0] };
    if header_size > 40 && len < fh + 70 {
        return fail("BMP extended mask fields are incomplete.");
    }
    let alpha_mask = if header_size > 40 { u32le(encoded, fh + 66) } else { 0 };
    let has_alpha = alpha_mask != 0;
    if compression == 3 {
        if len < fh + 66 {
            return fail("BMP bitfield masks are incomplete.");
        }
        for (index, mask) in masks.iter_mut().take(3).enumerate() {
            *mask = u32le(encoded, fh + 54 + index * 4);
        }
        masks[3] = alpha_mask;
    }
    let palette_colors = if bits < 16 { if used != 0 { used } else { 1usize << bits } } else { 0 };
    let entry_bytes = if core || header_size == 64 { 3 } else { 4 };
    let palette_start = 14 + header_size;
    if bits < 16 && palette_colors > 1usize << bits || palette_start + palette_colors * entry_bytes > len {
        return fail("BMP color table is incomplete.");
    }
    let stride = (width * bits as usize).div_ceil(32) * 4;
    let rle = compression == 1 || compression == 2;
    if pixel_offset < fh + 14 + header_size || pixel_offset > len || !rle && pixel_offset + stride * height > len {
        return fail("BMP pixel data is incomplete.");
    }
    let indices = if rle { Some(decode_rle(encoded, pixel_offset, width, height, bits as u32)?) } else { None };
    let mut bgra = vec![0u8; pixels * 4];
    let quantums = if bits == 16 || bits == 32 {
        Some([
            MaskQuantum::new(masks[0], bits, 0)?,
            MaskQuantum::new(masks[1], bits, 1)?,
            MaskQuantum::new(masks[2], bits, 2)?,
            MaskQuantum::new(masks[3], bits, 3)?,
        ])
    } else {
        None
    };
    let bits = bits as usize;
    for y in 0..height {
        let file_row = if signed_height < 0 { y } else { height - y - 1 };
        let row = pixel_offset + file_row * stride;
        for x in 0..width {
            let target = (y * width + x) * 4;
            if bits <= 8 {
                let bit = x * bits;
                let color = match &indices {
                    Some(i) => i[file_row * width + x] as usize,
                    None => ((encoded[row + (bit >> 3)] >> (8 - bits - bit % 8)) as usize) & ((1 << bits) - 1),
                };
                if color >= palette_colors {
                    return fail("BMP palette index is outside its color table.");
                }
                let source = palette_start + color * entry_bytes;
                bgra[target..target + 3].copy_from_slice(&encoded[source..source + 3]);
                bgra[target + 3] = 255;
            } else if bits == 24 {
                let source = row + x * 3;
                bgra[target..target + 3].copy_from_slice(&encoded[source..source + 3]);
                bgra[target + 3] = 255;
            } else {
                let q = quantums.as_ref().unwrap();
                let pixel = if bits == 16 { u16le(encoded, row + x * 2) } else { u32le(encoded, row + x * 4) };
                bgra[target] = q[2].apply(pixel);
                bgra[target + 1] = q[1].apply(pixel);
                bgra[target + 2] = q[0].apply(pixel);
                bgra[target + 3] = if has_alpha { 255 - q[3].apply(pixel) } else { 255 };
            }
        }
    }
    Ok(DecodedImage { width, height, bgra, has_alpha, format: "bmp" })
}
