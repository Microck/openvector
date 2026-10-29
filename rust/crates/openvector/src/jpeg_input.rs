//! IJG jpeg-6b integer inverse transform and original Q8 JPEG import.
//! Numerical code copyright 1991-1998 Thomas G. Lane; see src/jpeg-licenses.txt.
use crate::jpeg_coefficients::{read_jpeg_coefficients, Adobe, Jfif, JpegComponent};
use crate::pixel_input::{fail, normalize_cmyk_pixels, DecodedImage, InputError};

fn imul(a: i64, b: i64) -> i64 { (a as i32).wrapping_mul(b as i32) as i64 }
fn inverse_transform(v: &[i64; 8]) -> [i64; 8] {
    let (mut z2, mut z3) = (v[2], v[6]); let z1 = imul(z2 + z3, 4433);
    let (even2, even3) = (z1 - imul(z3, 15137), z1 + imul(z2, 6270));
    let even0 = ((v[0] + v[4]) as i32).wrapping_shl(13) as i64;
    let even1 = ((v[0] - v[4]) as i32).wrapping_shl(13) as i64;
    let (t10, t11, t12, t13) = (even0 + even3, even1 + even2, even1 - even2, even0 - even3);
    let (mut a, mut b, mut c, mut d) = (v[7], v[5], v[3], v[1]);
    let (p, q) = (a + d, b + c); z2 = a + c; z3 = b + d;
    let z5 = imul(z2 + z3, 9633); let (odd_p, odd_q) = (-imul(p, 7373), -imul(q, 20995));
    z2 = -imul(z2, 16069) + z5; z3 = -imul(z3, 3196) + z5;
    a = imul(a, 2446) + odd_p + z2; b = imul(b, 16819) + odd_q + z3;
    c = imul(c, 25172) + odd_q + z2; d = imul(d, 12299) + odd_p + z3;
    [t10+d,t11+c,t12+b,t13+a,t13-a,t12-b,t11-c,t10-d]
}
fn component_samples(c: &JpegComponent, width: usize, height: usize, max_h: usize, max_v: usize) -> Result<Vec<u8>, InputError> {
    let stride = c.blocks_per_line * 8;
    let mut pixels = vec![0u8; stride * c.blocks_per_column * 8]; let mut workspace = [0i64;64]; let mut values = [0i64;8];
    for y in 0..c.blocks_per_column { for x in 0..c.blocks_per_line {
        let coeff = &c.blocks[y * c.block_stride + x];
        for col in 0..8 {
            for row in 0..8 { values[row] = coeff[col + row * 8] as i64 * c.quantization_table[col + row * 8] as i64; }
            let transformed = inverse_transform(&values);
            for row in 0..8 { workspace[col + row * 8] = ((transformed[row] + 1024) as i32 >> 11) as i64; }
        }
        for row in 0..8 {
            values.copy_from_slice(&workspace[row * 8..row * 8 + 8]); let transformed = inverse_transform(&values);
            for col in 0..8 {
                let index = (((transformed[col] + 131072) as i32 >> 18) & 1023) as usize;
                pixels[(y * 8 + row) * stride + x * 8 + col] = if index < 128 { (index + 128) as u8 } else if index < 512 { 255 } else if index < 896 { 0 } else { (index - 896) as u8 };
            }
        }
    } }
    if max_h % c.h != 0 || max_v % c.v != 0 { return fail("Fractional JPEG sampling is unsupported."); }
    let (horizontal, vertical) = (max_h / c.h, max_v / c.v);
    let (actual_width, actual_height) = (width.div_ceil(horizontal), height.div_ceil(vertical));
    let sample = |x: isize, y: isize| -> i32 { pixels[y.clamp(0, actual_height as isize - 1) as usize * stride + x.clamp(0, actual_width as isize - 1) as usize] as i32 };
    let mut output = vec![0; width * height];
    for y in 0..height { for x in 0..width {
        let (col, row) = ((x / horizontal) as isize, (y / vertical) as isize); let mut value = sample(col, row);
        if horizontal == 2 && actual_width > 2 && vertical == 1 { value = (value * 3 + sample(col + if x % 2 != 0 { 1 } else { -1 }, row) + if x % 2 != 0 { 2 } else { 1 }) >> 2; }
        else if horizontal == 2 && actual_width > 2 && vertical == 2 {
            let near = row + if y % 2 != 0 { 1 } else { -1 }; let next = col + if x % 2 != 0 { 1 } else { -1 };
            let current = sample(col, row) * 3 + sample(col, near); let neighbor = sample(next, row) * 3 + sample(next, near);
            value = (current * 3 + neighbor + if x % 2 != 0 { 7 } else { 8 }) >> 4;
        }
        output[y * width + x] = value as u8;
    } }
    Ok(output)
}
#[derive(Debug, Clone)]
pub struct JpegSamples { pub width: usize, pub height: usize, pub planes: Vec<Vec<u8>>, pub jfif: Option<Jfif>, pub adobe: Option<Adobe>, pub component_ids: Vec<u8> }
pub fn read_jpeg_samples(encoded: &[u8]) -> Result<JpegSamples, InputError> {
    let f = read_jpeg_coefficients(encoded)?;
    let planes = f.components.iter().map(|c| component_samples(c, f.width, f.height, f.max_h, f.max_v)).collect::<Result<Vec<_>, _>>()?;
    Ok(JpegSamples { width: f.width, height: f.height, planes, jfif: f.jfif, adobe: f.adobe, component_ids: f.components.iter().map(|c| c.id).collect() })
}
pub fn convert_jpeg_ycbcr(planes: &mut [Vec<u8>]) {
    for pixel in 0..planes[0].len() {
        let (y, cb, cr) = (planes[0][pixel] as i32, planes[1][pixel] as i32 - 128, planes[2][pixel] as i32 - 128);
        planes[0][pixel] = (y + ((91881 * cr + 32768) >> 16)).clamp(0, 255) as u8;
        planes[1][pixel] = (y + ((-22554 * cb - 46802 * cr + 32768) >> 16)).clamp(0, 255) as u8;
        planes[2][pixel] = (y + ((116130 * cb + 32768) >> 16)).clamp(0, 255) as u8;
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegPixels { pub width: usize, pub height: usize, pub bgra: Vec<u8>, pub color_space: u8, pub black: Option<Vec<u8>> }
pub fn decode_jpeg_pixels(encoded: &[u8]) -> Result<JpegPixels, InputError> {
    let mut frame = read_jpeg_samples(encoded)?;
    if ![1,3,4].contains(&frame.planes.len()) { return fail("Unsupported JPEG component count."); }
    let cmyk = frame.planes.len() == 4;
    let direct_rgb = frame.jfif.is_none() && frame.adobe.as_ref().map_or(frame.component_ids == [82,71,66], |a| a.transform_code == 0);
    let ycc = frame.planes.len() == 3 && !direct_rgb || cmyk && frame.adobe.as_ref().is_some_and(|a| a.transform_code == 2);
    if ycc { convert_jpeg_ycbcr(&mut frame.planes); }
    let mut bgra = vec![0; frame.width * frame.height * 4];
    for pixel in 0..frame.width * frame.height {
        let red = frame.planes[0][pixel]; let green = if frame.planes.len() == 1 { red } else { frame.planes[1][pixel] }; let blue = if frame.planes.len() == 1 { red } else { frame.planes[2][pixel] };
        let out = &mut bgra[pixel * 4..pixel * 4 + 4];
        out.copy_from_slice(&[blue,green,red,255]);
        if cmyk { if !ycc { for c in &mut out[..3] { *c = 255 - *c; } } frame.planes[3][pixel] = 255 - frame.planes[3][pixel]; }
    }
    Ok(JpegPixels { width: frame.width, height: frame.height, bgra, color_space: if cmyk { 12 } else { 1 }, black: if cmyk { frame.planes.pop() } else { None } })
}
pub fn decode_jpeg(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    let mut p = decode_jpeg_pixels(encoded)?; if let Some(black) = &p.black { normalize_cmyk_pixels(&mut p.bgra, black); }
    Ok(DecodedImage { width: p.width, height: p.height, bgra: p.bgra, has_alpha: false, format: "jpeg" })
}
