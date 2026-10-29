//! Port of `src/bitmap-codecs.mjs`: native BMP, TIFF, PNG and JPEG bytes for
//! supplied BGRA pixels. PNG compression uses the pinned zlib 1.2.3 and JPEG
//! the pinned IJG jpeg-6b, both compiled from `vendor/` by `build.rs`.

use crate::pixel_input::{image_size, validate_bgra};
use std::fmt;

/// Encoding failure carrying the reference error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapError(pub String);

impl fmt::Display for BitmapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for BitmapError {}

fn fail<T>(message: impl Into<String>) -> Result<T, BitmapError> { Err(BitmapError(message.into())) }

unsafe extern "C" {
    fn ov_deflate(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
    fn ov_jpeg_encode(bgra: *const u8, width: u32, height: u32, encoded: *mut *mut u8, length: *mut usize) -> i32;
    fn ov_jpeg_free(bytes: *mut u8);
}

/// `readBitmapPixels`: dimensions, channel layout and one pixel per position.
fn read_bitmap_pixels(width: i64, height: i64, bgra: &[u8]) -> Result<usize, BitmapError> {
    let pixels = image_size(width, height).map_err(|error| BitmapError(error.0))?;
    validate_bgra(bgra).map_err(|error| BitmapError(error.0))?;
    if bgra.len() != pixels * 4 {
        return fail("Bitmap encoding needs one BGRA pixel for each image position.");
    }
    Ok(pixels)
}

/// Default native JPEG profile: quality 75, baseline RGB, 96-DPI JFIF.
/// Alpha is ignored; document storage/conversion policy belongs to bitmap-export.
pub fn encode_jpeg(width: i64, height: i64, bgra: &[u8]) -> Result<Vec<u8>, BitmapError> {
    read_bitmap_pixels(width, height, bgra)?;
    if width > 65500 || height > 65500 { return fail("JPEG dimensions cannot exceed 65500 pixels."); }
    let mut encoded: *mut u8 = std::ptr::null_mut();
    let mut length = 0usize;
    // SAFETY: `bgra` holds width*height*4 bytes; the compressor returns an
    // owned malloc buffer of `length` bytes, released below after copying.
    let status = unsafe { ov_jpeg_encode(bgra.as_ptr(), width as u32, height as u32, &mut encoded, &mut length) };
    if status != 0 { return fail(format!("JPEG compression failed with IJG status {status}.")); }
    let bytes = unsafe { std::slice::from_raw_parts(encoded, length) }.to_vec();
    unsafe { ov_jpeg_free(encoded) };
    Ok(bytes)
}

/// Store supplied BGR channels and discard alpha; bitmap-export owns storage policy.
pub fn encode_bmp(width: i64, height: i64, bgra: &[u8]) -> Result<Vec<u8>, BitmapError> {
    read_bitmap_pixels(width, height, bgra)?;
    let (width, height) = (width as usize, height as usize);
    let stride = (width * 3).div_ceil(4) * 4;
    let pixel_bytes = stride as u64 * height as u64;
    let length = 54 + pixel_bytes;
    if length > 0xffffffff { return fail("BMP output exceeds its 32-bit file size."); }
    let mut encoded = vec![0u8; length as usize];
    encoded[0..2].copy_from_slice(b"BM");
    put_u32_le(&mut encoded, 2, length as u32);
    put_u32_le(&mut encoded, 10, 54);
    put_u32_le(&mut encoded, 14, 40);
    put_u32_le(&mut encoded, 18, width as u32);
    put_u32_le(&mut encoded, 22, height as u32);
    put_u16_le(&mut encoded, 26, 1);
    put_u16_le(&mut encoded, 28, 24);
    put_u32_le(&mut encoded, 34, pixel_bytes as u32);
    put_u32_le(&mut encoded, 38, 3780);
    put_u32_le(&mut encoded, 42, 3780);
    for y in 0..height {
        let row = 54 + (height - 1 - y) * stride;
        for (x, source) in bgra[y * width * 4..(y + 1) * width * 4].chunks_exact(4).enumerate() {
            encoded[row + x * 3..row + x * 3 + 3].copy_from_slice(&source[..3]);
        }
    }
    Ok(encoded)
}

/// Encode supplied channel bytes; scene composition belongs to the renderer.
pub fn encode_tiff(width: i64, height: i64, bgra: &[u8]) -> Result<Vec<u8>, BitmapError> {
    let pixels = read_bitmap_pixels(width, height, bgra)?;
    let pixel_bytes = pixels as u64 * 4;
    // Nine entries, the entry count and the zero next-directory pointer.
    let directory_bytes = 2 + 9 * 12 + 4;
    let directory = 8 + pixel_bytes;
    let bits_offset = directory + directory_bytes;
    let length = bits_offset + 8;
    if length > 0xffffffff { return fail("TIFF output exceeds its classic 32-bit offsets."); }
    let (directory, bits_offset) = (directory as usize, bits_offset as usize);
    let mut encoded = vec![0u8; length as usize];
    encoded[0..2].copy_from_slice(b"II");
    put_u16_le(&mut encoded, 2, 42);
    put_u32_le(&mut encoded, 4, directory as u32);
    for (source, target) in bgra.chunks_exact(4).zip(encoded[8..directory].chunks_exact_mut(4)) {
        target.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }
    let size_type = |value: i64| if value <= 65535 { 3 } else { 4 };
    let entries: [(u16, u16, u32, u32); 9] = [
        (256, size_type(width), 1, width as u32), (257, size_type(height), 1, height as u32),
        (258, 3, 4, bits_offset as u32), (259, 3, 1, 1), (262, 3, 1, 2), (273, 4, 1, 8),
        (277, 3, 1, 4), (279, 4, 1, pixel_bytes as u32), (284, 3, 1, 1),
    ];
    put_u16_le(&mut encoded, directory, entries.len() as u16);
    for (index, (tag, kind, count, value)) in entries.into_iter().enumerate() {
        let at = directory + 2 + index * 12;
        put_u16_le(&mut encoded, at, tag);
        put_u16_le(&mut encoded, at + 2, kind);
        put_u32_le(&mut encoded, at + 4, count);
        put_u32_le(&mut encoded, at + 8, value);
    }
    for channel in 0..4 { put_u16_le(&mut encoded, bits_offset + channel * 2, 8); }
    Ok(encoded)
}

fn put_u16_le(bytes: &mut [u8], at: usize, value: u16) { bytes[at..at + 2].copy_from_slice(&value.to_le_bytes()); }
fn put_u32_le(bytes: &mut [u8], at: usize, value: u32) { bytes[at..at + 4].copy_from_slice(&value.to_le_bytes()); }

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 { 0xedb88320 ^ (value >> 1) } else { value >> 1 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &byte| CRC_TABLE[((crc ^ byte as u32) & 255) as usize] ^ (crc >> 8))
}

fn push_png_chunk(output: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let start = output.len();
    output.extend_from_slice(kind);
    output.extend_from_slice(payload);
    let crc = crc32(&output[start..]);
    output.extend_from_slice(&crc.to_be_bytes());
}

fn paeth(left: i32, above: i32, corner: i32) -> i32 {
    let estimate = left + above - corner;
    let (a, b, c) = ((estimate - left).abs(), (estimate - above).abs(), (estimate - corner).abs());
    if a <= b && a <= c { left } else if b <= c { above } else { corner }
}

fn compress_png(filtered: &[u8]) -> Result<Vec<u8>, BitmapError> {
    // The captured zlib 1.2.3 stream differs from current zlib releases on
    // larger images. One pinned compressor owns PNG output on every host.
    let length = filtered.len();
    let capacity = length + length / 4096 + length / 16384 + 11;
    if length > u32::MAX as usize || capacity > u32::MAX as usize {
        return fail("PNG compression exceeds the available WebAssembly memory.");
    }
    let mut output = vec![0u8; capacity];
    // SAFETY: both buffers are valid for the lengths passed.
    let written = unsafe { ov_deflate(filtered.as_ptr(), length as u32, output.as_mut_ptr(), capacity as u32) };
    if written <= 0 { return fail(format!("PNG compression failed with zlib status {written}.")); }
    output.truncate(written as usize);
    Ok(output)
}

/// PNG preserves the renderer's RGB/RGBA storage choice, even for opaque RGBA.
pub fn encode_png(width: i64, height: i64, bgra: &[u8], has_alpha: bool) -> Result<Vec<u8>, BitmapError> {
    read_bitmap_pixels(width, height, bgra)?;
    let (width, height) = (width as usize, height as usize);
    let channels = if has_alpha { 4 } else { 3 };
    let row_bytes = width * channels;
    let mut filtered = vec![0u8; (row_bytes + 1) * height];
    let mut previous = vec![0u8; row_bytes];
    let mut row = vec![0u8; row_bytes];
    let mut candidates = vec![vec![0u8; row_bytes]; 5];
    for y in 0..height {
        for x in 0..width {
            let (source, target) = ((y * width + x) * 4, x * channels);
            row[target] = bgra[source + 2];
            row[target + 1] = bgra[source + 1];
            row[target + 2] = bgra[source];
            if has_alpha { row[target + 3] = bgra[source + 3]; }
        }
        // Choose the first minimum signed-byte sum in PNG's filter order.
        let (mut best_score, mut best_filter) = (u64::MAX, 0usize);
        for (filter, candidate) in candidates.iter_mut().enumerate() {
            let mut score = 0u64;
            for column in 0..row_bytes {
                let left = if column < channels { 0 } else { row[column - channels] as i32 };
                let above = previous[column] as i32;
                let corner = if column < channels { 0 } else { previous[column - channels] as i32 };
                let predictor = match filter {
                    1 => left,
                    2 => above,
                    3 => (left + above) / 2,
                    4 => paeth(left, above, corner),
                    _ => 0,
                };
                let residual = ((row[column] as i32 - predictor) & 255) as u8;
                candidate[column] = residual;
                score += residual.min((256 - residual as u32) as u8) as u64;
            }
            if score < best_score { best_score = score; best_filter = filter; }
        }
        let target = y * (row_bytes + 1);
        filtered[target] = best_filter as u8;
        filtered[target + 1..target + 1 + row_bytes].copy_from_slice(&candidates[best_filter]);
        std::mem::swap(&mut previous, &mut row);
    }
    let mut compressed = compress_png(&filtered)?;
    // PNG advertises the smallest window covering its whole filtered stream.
    // This changes CMF/FCHECK only; the compressor still uses its 32 KiB window.
    let window_bits = (filtered.len().next_power_of_two().trailing_zeros() as u32).clamp(8, 15);
    compressed[0] = (((window_bits - 8) << 4) | 8) as u8;
    compressed[1] &= 0xe0;
    let check = (31 - (compressed[0] as u32 * 256 + compressed[1] as u32) % 31) % 31;
    compressed[1] += check as u8;
    let mut header = [0u8; 13];
    header[0..4].copy_from_slice(&(width as u32).to_be_bytes());
    header[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    header[8] = 8;
    header[9] = if has_alpha { 6 } else { 2 };
    let mut resolution = [0u8; 9];
    resolution[0..4].copy_from_slice(&3780u32.to_be_bytes());
    resolution[4..8].copy_from_slice(&3780u32.to_be_bytes());
    resolution[8] = 1;
    let mut output = Vec::with_capacity(compressed.len() + compressed.len() / 8192 * 12 + 80);
    output.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    push_png_chunk(&mut output, b"IHDR", &header);
    push_png_chunk(&mut output, b"pHYs", &resolution);
    for part in compressed.chunks(8192) { push_png_chunk(&mut output, b"IDAT", part); }
    push_png_chunk(&mut output, b"IEND", &[]);
    Ok(output)
}
