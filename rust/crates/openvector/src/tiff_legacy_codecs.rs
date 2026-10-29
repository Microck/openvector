//! NeXT and ThunderScan row codecs; notices retained in src/tiff-licenses.txt.
use crate::pixel_input::{fail, InputError};

pub fn decode_next(encoded: &[u8], width: usize, rows: usize) -> Result<Vec<u8>, InputError> {
    let stride = width.div_ceil(4); let mut output = vec![255u8; stride * rows]; let mut offset = 0;
    let mut take = |count: usize| -> Result<&[u8], InputError> {
        if count > encoded.len().saturating_sub(offset) { return fail("TIFF NeXT row is incomplete."); }
        let bytes = &encoded[offset..offset + count]; offset += count; Ok(bytes)
    };
    for row in 0..rows {
        let mut command = take(1)?[0];
        if command == 0 { output[row * stride..(row + 1) * stride].copy_from_slice(take(stride)?); }
        else if command == 64 {
            let header = take(4)?; let start = u16::from_be_bytes([header[0],header[1]]) as usize; let count = u16::from_be_bytes([header[2],header[3]]) as usize;
            if start + count > stride { return fail("TIFF NeXT span exceeds its row."); }
            output[row * stride + start..row * stride + start + count].copy_from_slice(take(count)?);
        } else {
            let mut x = 0;
            while x < width {
                let gray = command >> 6; let count = (command as usize & 63).min(width - x);
                for _ in 0..count { let target = row * stride + x / 4; let shift = 6 - (x & 3) * 2; output[target] = if x & 3 == 0 { gray << 6 } else { output[target] | gray << shift }; x += 1; }
                if x < width { command = take(1)?[0]; }
            }
        }
    }
    Ok(output)
}

pub fn decode_thunder(encoded: &[u8], width: usize, rows: usize, scanline: &mut [u8]) -> Result<Vec<u8>, InputError> {
    let stride = width.div_ceil(2); let mut output = vec![0; stride * rows]; let mut offset = 0;
    for row in 0..rows {
        let (mut x, mut last, mut cursor) = (0usize, 0i32, 0usize);
        while x < width {
            let command = *encoded.get(offset).ok_or_else(|| InputError::new("TIFF ThunderScan row is incomplete."))?; offset += 1;
            let kind = command >> 6;
            if kind == 0 {
                let mut count = (command & 63) as i32;
                if x + count as usize > width { return fail("TIFF ThunderScan run exceeds its row."); }
                if x & 1 != 0 { scanline[cursor] |= last as u8; last = scanline[cursor] as i32; cursor += 1; x += 1; count -= 1; } else { last |= last << 4; }
                x = (x as i64 + count as i64) as usize;
                if x < width { while count > 0 { scanline[cursor] = last as u8; cursor += 1; count -= 2; } }
                if count == -1 { cursor -= 1; scanline[cursor] &= 240; }
                last &= 15;
            } else {
                let mut value = last;
                let mut emit = |value: i32| -> Result<(), InputError> {
                    if x >= width { return fail("TIFF ThunderScan command exceeds its row."); }
                    last = value & 15;
                    if x & 1 != 0 { scanline[cursor] |= last as u8; cursor += 1; } else { scanline[cursor] = (last << 4) as u8; }
                    x += 1; Ok(())
                };
                if kind == 3 { emit(command as i32)?; }
                else {
                    // Each delta depends on the preceding emitted nibble.
                    let bits = if kind == 1 { 2 } else { 3 }; let skip = if kind == 1 { 2 } else { 4 };
                    for shift in (0..=6-bits).rev().step_by(bits) {
                        let code = (command as usize >> shift) & ((1 << bits) - 1);
                        if code != skip { value = (value + if kind == 1 { [0,1,0,-1][code] } else { [0,1,2,3,0,-3,-2,-1][code] }) & 15; emit(value)?; }
                    }
                }
            }
        }
        output[row * stride..(row + 1) * stride].copy_from_slice(scanline);
    }
    Ok(output)
}
