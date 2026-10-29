//! Port of `src/image-preparation.mjs`: document background composition and
//! the native EnhanceImage, median and despeckle preprocessing filters.

use crate::image_classification::premultiply_classifier_pixels;
use crate::image_resize::OwnedImage;

const ENHANCE_WEIGHTS: [u32; 25] = [5, 8, 10, 8, 5, 8, 20, 40, 20, 8, 10, 40, 80, 40, 10, 8, 20, 40, 20, 8, 5, 8, 10, 8, 5];
const BYTE_RECIPROCAL: f64 = 1.0 / 255.0;

/// ImageMagick's median size precondition reason.
pub const SMALLER_THAN_KERNEL: &str = "ImageSmallerThanKernelRadius";
pub const INVALID_MODE: &str = "Expected an image preprocessing mode from 0 through 3.";

/// JavaScript `Uint8Array` store of an integral double (ToUint8).
#[inline]
fn to_uint8(value: f64) -> u8 {
    if value.is_finite() { (value.trunc() as i64 & 255) as u8 } else { 0 }
}

/// Document composition over the original alpha-prepared backup. Background
/// components follow engine BGRA order and are quantized before blending; the
/// background RGB is not premultiplied by its own alpha.
pub fn composite_source_image(width: usize, height: usize, bgra: &[u8], enabled: bool, color: &[f64; 4]) -> OwnedImage {
    if !enabled { return OwnedImage { width, height, bgra: bgra.to_vec() }; }
    let scale = BYTE_RECIPROCAL as f32;
    let background_units = color.map(|value| ((((value as f32 as f64) * 255.0 + 0.5).trunc() as i64 & 255) as f32) * scale);
    let mut composed = vec![0u8; bgra.len()];
    for (target, source) in composed.chunks_exact_mut(4).zip(bgra.chunks_exact(4)) {
        let remaining = 1.0f32 - source[3] as f32 * scale;
        for channel in 0..4 {
            let source_unit = source[channel] as f32 * scale;
            let value = background_units[channel] * remaining + source_unit;
            target[channel] = ((value as f64 * 255.0 + 0.5).trunc() as i64 & 255) as u8;
        }
    }
    OwnedImage { width, height, bgra: composed }
}

/// EnhanceImage kernel over validated, already prepared Q8 BGRA. The
/// opacity-dependent blue distance is evaluated in ImageMagick opacity form.
/// Dimensions below 5 produce an empty image.
pub fn enhance_prepared_image(width: usize, height: usize, bgra: &[u8]) -> OwnedImage {
    if width < 5 || height < 5 { return OwnedImage::default(); }
    let mut enhanced = vec![0u8; bgra.len()];
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let center = (y as usize * width + x as usize) * 4;
            let blue = bgra[center] as f64;
            let green = bgra[center + 1] as f64;
            let red = bgra[center + 2] as f64;
            let opacity = (255 - bgra[center + 3]) as f64;
            let mut sums = [0.0f32; 4];
            let (mut count, mut tap) = (0u32, 0usize);
            for dy in -2..=2i64 {
                let row = (height as i64 - 1).min((y + dy).max(0)) as usize;
                for dx in -2..=2i64 {
                    let column = (width as i64 - 1).min((x + dx).max(0)) as usize;
                    let neighbor = (row * width + column) * 4;
                    let weight = ENHANCE_WEIGHTS[tap];
                    tap += 1;
                    let neighbor_blue = bgra[neighbor] as f64;
                    let neighbor_green = bgra[neighbor + 1] as f64;
                    let neighbor_red = bgra[neighbor + 2] as f64;
                    let neighbor_opacity = (255 - bgra[neighbor + 3]) as f64;
                    let dr = red - neighbor_red;
                    let dg = green - neighbor_green;
                    let db = blue - neighbor_blue;
                    let dopacity = opacity - neighbor_opacity;
                    // Native x87 double operation order at the strict cutoff.
                    let mut distance = (((red + neighbor_red) * 0.5 + 512.0) * dr) * dr * BYTE_RECIPROCAL;
                    distance += dg * dg * 4.0;
                    distance += ((767.0 - (blue + neighbor_blue) * 0.5) * db) * db * BYTE_RECIPROCAL;
                    distance += ((767.0 - (opacity + neighbor_opacity) * 0.5) * dopacity) * dopacity * BYTE_RECIPROCAL;
                    if distance < 2601.0 {
                        let w = weight as f64;
                        sums[0] = (sums[0] as f64 + neighbor_blue * w) as f32;
                        sums[1] = (sums[1] as f64 + neighbor_green * w) as f32;
                        sums[2] = (sums[2] as f64 + neighbor_red * w) as f32;
                        sums[3] = (sums[3] as f64 + neighbor_opacity * w) as f32;
                        count += weight;
                    }
                }
            }
            // The original biased integer normalization, not rounding.
            let half_count = (count as f64 * 0.5) as f32 as f64;
            let reciprocal = 1.0 / count as f64;
            for channel in 0..4 {
                let value = ((sums[channel] as f64 + half_count - 1.0) * reciprocal).trunc();
                enhanced[center + channel] = if channel == 3 { to_uint8(255.0 - value) } else { to_uint8(value) };
            }
        }
    }
    OwnedImage { width, height, bgra: enhanced }
}

/// Mode-one preparation for decoded BGRA and an explicit amount 0..3. Alpha
/// multiplication occurs once; each pass consumes the previous owned output.
pub fn prepare_enhanced_image(width: usize, height: usize, bgra: &[u8], amount: u32) -> OwnedImage {
    let prepared = premultiply_classifier_pixels(bgra);
    filter_prepared_image(OwnedImage { width, height, bgra: prepared }, 1, amount)
        .expect("mode one never fails")
}

/// Nonpeak median over validated prepared bytes and amount 0..3. A zero
/// radius selects the same 3x3 kernel as radius one. Selection avoids a
/// unique-value minimum or maximum when another value exists.
pub fn reduce_prepared_noise(width: usize, height: usize, bgra: &[u8], amount: u32) -> Result<OwnedImage, String> {
    let radius = amount.max(1) as i64;
    let kernel_width = (radius * 2 + 1) as usize;
    if width < kernel_width || height < kernel_width { return Err(SMALLER_THAN_KERNEL.to_string()); }
    let sample_count = kernel_width * kernel_width;
    let median_index = sample_count / 2;
    let mut reduced = vec![0u8; bgra.len()];
    let mut channel_samples = vec![vec![0u8; sample_count]; 4];
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let mut sample = 0;
            for dy in -radius..=radius {
                let row = (height as i64 - 1).min((y + dy).max(0)) as usize;
                for dx in -radius..=radius {
                    let column = (width as i64 - 1).min((x + dx).max(0)) as usize;
                    let neighbor = (row * width + column) * 4;
                    for channel in 0..4 { channel_samples[channel][sample] = bgra[neighbor + channel]; }
                    sample += 1;
                }
            }
            for channel in 0..4 {
                let samples = &mut channel_samples[channel];
                samples.sort_unstable();
                let mut selected = samples[median_index];
                if selected == samples[0] {
                    let mut next = median_index + 1;
                    while next < samples.len() && samples[next] == selected { next += 1; }
                    if next < samples.len() { selected = samples[next]; }
                } else if selected == samples[samples.len() - 1] {
                    let mut previous = median_index - 1;
                    while samples[previous] == selected { previous -= 1; }
                    selected = samples[previous];
                }
                reduced[(y as usize * width + x as usize) * 4 + channel] = selected;
            }
        }
    }
    Ok(OwnedImage { width, height, bgra: reduced })
}

fn hull_plane(plane: &mut [u8], scratch: &mut [u8], width: usize, height: usize, delta: isize, positive: bool) {
    let stride = width + 2;
    for y in 0..height {
        for x in 0..width {
            let pixel = (y + 1) * stride + x + 1;
            let center = plane[pixel] as i32;
            let neighbor = plane[(pixel as isize + delta) as usize] as i32;
            let change = if positive { neighbor >= center + 2 } else { neighbor <= center - 2 };
            scratch[pixel] = if change { (center + if positive { 1 } else { -1 }) as u8 } else { center as u8 };
        }
    }
    // Both neighbors come from the completed scratch pass; the one-byte
    // exterior remains zero in both planes.
    for y in 0..height {
        for x in 0..width {
            let pixel = (y + 1) * stride + x + 1;
            let center = scratch[pixel] as i32;
            let backward = scratch[(pixel as isize - delta) as usize] as i32;
            let forward = scratch[(pixel as isize + delta) as usize] as i32;
            let change = if positive { backward >= center + 2 && forward > center } else { backward <= center - 2 && forward < center };
            plane[pixel] = if change { (center + if positive { 1 } else { -1 }) as u8 } else { center as u8 };
        }
    }
}

/// Hull despeckle over validated Q8 BGRA, including one-dimensional rasters.
pub fn despeckle_prepared_image(width: usize, height: usize, bgra: &[u8]) -> OwnedImage {
    let stride = width + 2;
    let mut plane = vec![0u8; stride * (height + 2)];
    let mut scratch = vec![0u8; plane.len()];
    let mut despeckled = vec![0u8; bgra.len()];
    for channel in [2usize, 1, 0, 3] {
        for y in 0..height {
            for x in 0..width {
                let value = bgra[(y * width + x) * 4 + channel];
                plane[(y + 1) * stride + x + 1] = if channel == 3 { 255 - value } else { value };
            }
        }
        let s = stride as isize;
        for delta in [s, 1, s + 1, s - 1] {
            hull_plane(&mut plane, &mut scratch, width, height, delta, true);
            hull_plane(&mut plane, &mut scratch, width, height, -delta, true);
            hull_plane(&mut plane, &mut scratch, width, height, -delta, false);
            hull_plane(&mut plane, &mut scratch, width, height, delta, false);
        }
        for y in 0..height {
            for x in 0..width {
                let value = plane[(y + 1) * stride + x + 1];
                despeckled[(y * width + x) * 4 + channel] = if channel == 3 { 255 - value } else { value };
            }
        }
    }
    OwnedImage { width, height, bgra: despeckled }
}

/// Decoded-pixel preprocessing with explicit native mode/amount. One initial
/// alpha path; each filter keeps its own empty/error outcome.
pub fn prepare_image_pixels(width: usize, height: usize, bgra: &[u8], mode: u32, amount: u32) -> Result<OwnedImage, String> {
    let prepared = premultiply_classifier_pixels(bgra);
    filter_prepared_image(OwnedImage { width, height, bgra: prepared }, mode, amount)
}

/// Filter an owned, already alpha-prepared source. Document composition
/// enters here so alpha is never multiplied a second time.
pub fn filter_prepared_image(prepared: OwnedImage, mode: u32, amount: u32) -> Result<OwnedImage, String> {
    match mode {
        0 => Ok(prepared),
        1 => {
            let mut prepared = prepared;
            for _ in 0..amount { prepared = enhance_prepared_image(prepared.width, prepared.height, &prepared.bgra); }
            Ok(prepared)
        }
        2 => reduce_prepared_noise(prepared.width, prepared.height, &prepared.bgra, amount),
        3 => Ok(despeckle_prepared_image(prepared.width, prepared.height, &prepared.bgra)),
        _ => Err(INVALID_MODE.to_string()),
    }
}
