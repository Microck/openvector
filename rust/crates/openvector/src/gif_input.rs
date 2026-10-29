//! GIF first-frame decoder, preserving the reference's transparency sentinel
//! and single-step interlace progression.
use crate::pixel_input::{fail, DecodedImage, InputError};

struct Reader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], InputError> {
        let end = self.offset.checked_add(count).filter(|&n| n <= self.bytes.len())
            .ok_or_else(|| InputError::new("GIF data is incomplete."))?;
        let out = &self.bytes[self.offset..end]; self.offset = end; Ok(out)
    }
    fn blocks(&mut self) -> Result<Vec<u8>, InputError> {
        let mut out = Vec::new();
        loop { let n = self.take(1)?[0] as usize; if n == 0 { return Ok(out); } out.extend_from_slice(self.take(n)?); }
    }
}

fn decode_lzw(data: &[u8], minimum: u8, count: usize) -> Result<Vec<u8>, InputError> {
    if !(2..=8).contains(&minimum) { return fail("GIF LZW minimum code size must be from 2 through 8."); }
    let clear = 1usize << minimum;
    let mut prefix = [0usize; 4096]; let mut suffix = [0u8; 4096];
    for (i, item) in suffix.iter_mut().enumerate().take(clear) { *item = i as u8; }
    let mut stack = [0u8; 4096];
    let (mut bits, mut next, mut bit) = (minimum as usize + 1, clear + 2, 0usize);
    let mut previous = None;
    let mut output = Vec::with_capacity(count);
    while output.len() < count {
        if bit + bits > data.len() * 8 { return fail("GIF LZW pixels are incomplete."); }
        let mut code = 0usize;
        for shift in 0..bits { code |= (((data[bit / 8] >> (bit % 8)) & 1) as usize) << shift; bit += 1; }
        if code == clear { bits = minimum as usize + 1; next = clear + 2; previous = None; continue; }
        if code == clear + 1 { return fail("GIF LZW stream ended before all pixels were decoded."); }
        let special = code == next && previous.is_some();
        if !(code < next || special) { return fail("GIF LZW code is outside the current dictionary."); }
        let mut cursor = if special { previous.unwrap() } else { code };
        let mut size = 0;
        while cursor >= clear + 2 { stack[size] = suffix[cursor]; size += 1; cursor = prefix[cursor]; }
        if cursor >= clear { return fail("GIF LZW code is outside the current dictionary."); }
        stack[size] = suffix[cursor]; size += 1;
        let first = stack[size - 1];
        if size + usize::from(special) > count - output.len() { return fail("GIF LZW code exceeds the image pixel count."); }
        output.extend(stack[..size].iter().rev()); if special { output.push(first); }
        if let Some(prev) = previous { if next < 4096 {
            prefix[next] = prev; suffix[next] = first; next += 1;
            if next == 1 << bits && bits < 12 { bits += 1; }
        } }
        previous = Some(code);
    }
    Ok(output)
}

pub fn decode_gif(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    let mut reader = Reader { bytes: encoded, offset: 6 };
    let screen = reader.take(7)?;
    let mut palette = if screen[4] & 128 != 0 { Some(reader.take(3 * (2 << (screen[4] & 7)))?) } else { None };
    let mut transparency: i32 = -1;
    while reader.offset < encoded.len() {
        let marker = reader.take(1)?[0];
        if marker == 0x3b { break; }
        if marker == 0x21 {
            let label = reader.take(1)?[0]; let extension = reader.blocks()?;
            if label == 0xf9 {
                if extension.len() != 4 { return fail("GIF graphic control requires four bytes."); }
                if extension[0] & 1 != 0 { transparency = extension[3] as i32; }
            }
            continue;
        }
        if marker != 0x2c { return fail("GIF image descriptor is missing."); }
        let d = reader.take(9)?;
        let width = u16::from_le_bytes([d[4], d[5]]) as usize;
        let height = u16::from_le_bytes([d[6], d[7]]) as usize;
        if width == 0 || height == 0 || width * height > 2147483647 { return fail("GIF dimensions require a positive int32 pixel count."); }
        let flags = d[8];
        if flags & 128 != 0 { palette = Some(reader.take(3 * (2 << (flags & 7)))?); }
        let palette = palette.ok_or_else(|| InputError::new("GIF requires an active color table."))?;
        if transparency >= (palette.len() / 3) as i32 { transparency = -1; }
        let minimum = reader.take(1)?[0]; let compressed = reader.blocks()?;
        let indices = decode_lzw(&compressed, minimum, width * height)?;
        let mut bgra = vec![0; width * height * 4];
        let (mut y, mut pass) = (0usize, 0usize);
        for row in 0..height {
            if y >= height { return fail("GIF interlace selects a row outside the image."); }
            for x in 0..width {
                let index = indices[row * width + x]; let color = index as usize * 3; let pixel = (y * width + x) * 4;
                if color >= palette.len() { return fail("GIF palette index is outside its color table."); }
                bgra[pixel..pixel + 4].copy_from_slice(&[palette[color + 2], palette[color + 1], palette[color], if index == transparency as u8 { 0 } else { 255 }]);
            }
            if flags & 64 != 0 { y += [8, 8, 4, 2][pass]; if y >= height && pass < 3 { pass += 1; y = [0, 4, 2, 1][pass]; } } else { y += 1; }
        }
        return Ok(DecodedImage { width, height, bgra, has_alpha: transparency >= 0, format: "gif" });
    }
    fail("GIF contains no image frame.")
}
