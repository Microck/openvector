//! Port of `src/image-resize.mjs`: Q8 sampling and triangle resizing, native
//! geometry fitting and working-size reduction.

/// Owned Q8 BGRA raster produced by the image preparation stages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedImage {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

/// `Math.min(255, Math.max(0, Math.trunc(value)))` stored into a Uint8Array.
#[inline]
fn clamp_byte(value: f64) -> u8 {
    if value.is_nan() { 0 } else { value.trunc().max(0.0).min(255.0) as u8 }
}

fn triangle_weights(source_size: usize, target_size: usize) -> Vec<Vec<(usize, f64)>> {
    let factor = f32r(target_size as f64 / source_size as f64);
    let inverse = f32r(1.0 / factor);
    let support = inverse.max(1.0);
    let scale = f32r(1.0 / support);
    (0..target_size).map(|output| {
        let center_double = (output as f64 + 0.5) * inverse;
        let center = f32r(center_double);
        // The lower bound uses the retained double center, while the upper
        // bound and tap distances read its float32 store.
        let start = ((center_double - support - 1e-6).max(0.0) + 0.5).trunc() as i64;
        let end = ((center + support).min(source_size as f64) + 0.5).trunc() as i64;
        let mut taps = Vec::new();
        let mut sum = 0.0f64;
        for pixel in start..end {
            let distance = f32r(((pixel as f64 - center) + 0.5) * scale);
            let weight = f32r((1.0 - distance.abs()).max(0.0));
            taps.push((pixel as usize, weight));
            sum = f32r(sum + weight);
        }
        if sum != 0.0 && sum != 1.0 {
            let reciprocal = f32r(1.0 / sum);
            for tap in &mut taps { tap.1 = f32r(tap.1 * reciprocal); }
        }
        taps
    }).collect()
}

fn triangle_pass(width: usize, height: usize, bgra: &[u8], target_size: usize, horizontal: bool, matte: bool) -> OwnedImage {
    let output_width = if horizontal { target_size } else { width };
    let output_height = if horizontal { height } else { target_size };
    let mut resized = vec![0u8; output_width * output_height * 4];
    let weights = triangle_weights(if horizontal { width } else { height }, target_size);
    for y in 0..output_height {
        for x in 0..output_width {
            let mut sums = [0.0f64; 3];
            let taps = &weights[if horizontal { x } else { y }];
            let grouped_taps = taps.len() - taps.len() % 4;
            let (mut alpha, mut first_alpha, mut opacity_sum) = (0.0f64, 0.0f64, 0.0f64);
            for (tap, &(coordinate, weight)) in taps.iter().enumerate() {
                let pixel = (if horizontal { y * width + coordinate } else { coordinate * width + x }) * 4;
                let opacity = (255 - bgra[pixel + 3]) as f64;
                let effective_weight = if matte { f32r((255.0 - opacity) * weight * (1.0 / 255.0)) } else { weight };
                if matte {
                    for channel in 0..3 {
                        sums[channel] = f32r(sums[channel] + bgra[pixel + channel] as f64 * effective_weight);
                    }
                } else {
                    for channel in 0..3 { sums[channel] += bgra[pixel + channel] as f64 * weight; }
                    // Red remains in x87 across the fourth and trailing taps;
                    // green spills after the third and fourth taps.
                    if tap < grouped_taps && (tap % 4 == 1 || tap % 4 == 2) { sums[2] = f32r(sums[2]); }
                    if tap >= grouped_taps || tap % 4 == 2 || tap % 4 == 3 { sums[1] = f32r(sums[1]); }
                    if tap >= grouped_taps || tap % 4 != 1 { sums[0] = f32r(sums[0]); }
                }
                opacity_sum += opacity * weight;
                if !matte || tap >= grouped_taps || tap % 4 == 0 || tap % 4 == 3 { opacity_sum = f32r(opacity_sum); }
                if tap < grouped_taps {
                    match tap % 4 {
                        0 => first_alpha = effective_weight,
                        1 => alpha = f32r(if horizontal { alpha + first_alpha + effective_weight } else { effective_weight + alpha + first_alpha }),
                        2 => alpha = f32r(alpha + effective_weight),
                        _ => alpha += effective_weight,
                    }
                } else {
                    alpha += effective_weight;
                }
            }
            let reciprocal = if matte && alpha.abs() > 1e-6 { 1.0 / alpha } else { 1.0 };
            let target = (y * output_width + x) * 4;
            for channel in 0..4 {
                let value = if channel == 3 { opacity_sum } else { sums[channel] * reciprocal };
                let byte = clamp_byte(value + 0.5);
                resized[target + channel] = if channel == 3 { 255 - byte } else { byte };
            }
        }
    }
    OwnedImage { width: output_width, height: output_height, bgra: resized }
}

fn sample_image(width: usize, height: usize, bgra: &[u8], target_width: usize, target_height: usize) -> OwnedImage {
    let mut sampled = vec![0u8; target_width * target_height * 4];
    for y in 0..target_height {
        let row = (((y as f64 + 0.5) * height as f64) / target_height as f64).trunc() as usize;
        for x in 0..target_width {
            let column = (((x as f64 + 0.5) * width as f64) / target_width as f64).trunc() as usize;
            let source = (row * width + column) * 4;
            let target = (y * target_width + x) * 4;
            sampled[target..target + 4].copy_from_slice(&bgra[source..source + 4]);
        }
    }
    OwnedImage { width: target_width, height: target_height, bgra: sampled }
}

/// Explicit resize request (`resizePreparedImage` input).
#[derive(Debug, Clone, Copy)]
pub struct ResizeRequest<'a> {
    pub width: usize,
    pub height: usize,
    pub bgra: &'a [u8],
    pub target_width: usize,
    pub target_height: usize,
    pub filtered: bool,
    pub matte: bool,
}

/// Private Q8 resize over validated prepared BGRA and positive dimensions.
/// Native filtered resizing quantizes between passes, including same-size
/// resizes. Both paths own their output bytes.
pub fn resize_prepared_image(input: &ResizeRequest) -> OwnedImage {
    let ResizeRequest { width, height, bgra, target_width, target_height, filtered, matte } = *input;
    if !filtered { return sample_image(width, height, bgra, target_width, target_height); }
    if (width + target_width) as u64 * (target_height as u64) < (height + target_height) as u64 * target_width as u64 {
        let horizontal = triangle_pass(width, height, bgra, target_width, true, matte);
        return triangle_pass(horizontal.width, horizontal.height, &horizontal.bgra, target_height, false, matte);
    }
    let vertical = triangle_pass(width, height, bgra, target_height, false, matte);
    triangle_pass(vertical.width, vertical.height, &vertical.bgra, target_width, true, matte)
}

/// Private native Geometry fit. Zero geometry dimensions are omitted; one-axis
/// requests convert the integer to float32 before division, while two-axis
/// requests retain a double width ratio and float32 height ratio.
pub fn fit_resize_geometry(width: i64, height: i64, geometry_width: i64, geometry_height: i64) -> [i64; 2] {
    if (geometry_width == 0 || geometry_width == width) && (geometry_height == 0 || geometry_height == height) {
        return [width, height];
    }
    let (w, h) = (width as f64, height as f64);
    let scale = if geometry_width != 0 && geometry_height != 0 {
        (geometry_width as f64 / w).min(f32r(geometry_height as f64 / h))
    } else if geometry_width != 0 {
        f32r(geometry_width as f64) / w
    } else {
        f32r(geometry_height as f64) / h
    };
    [1i64.max((w * scale + 0.5).trunc() as i64), 1i64.max((h * scale + 0.5).trunc() as i64)]
}

#[inline]
fn signed_area(width: usize, height: usize) -> i32 {
    (width as i32).wrapping_mul(height as i32)
}

/// Private working-size calculation. Preserves the original signed int32 area
/// comparison, including its overflow behavior.
pub fn working_image_dimensions(width: usize, height: usize) -> [usize; 2] {
    let area = signed_area(width, height);
    if area <= 200000 { return [width, height]; }
    let scale = (150000.0 / area as f64).sqrt();
    let fitted = fit_resize_geometry(width as i64, height as i64,
        (width as f64 * scale + 0.5).trunc() as i64, (height as f64 * scale + 0.5).trunc() as i64);
    [fitted[0] as usize, fitted[1] as usize]
}

/// Private reduction of an already preprocessed raster, including empty
/// preprocessing outcomes. Produces only the owned working raster.
pub fn reduce_working_image(width: usize, height: usize, bgra: &[u8], anti_alias: bool, matte: bool) -> OwnedImage {
    let [target_width, target_height] = working_image_dimensions(width, height);
    if signed_area(width, height) <= 200000 {
        return OwnedImage { width, height, bgra: bgra.to_vec() };
    }
    resize_prepared_image(&ResizeRequest { width, height, bgra, target_width, target_height, filtered: anti_alias, matte })
}

fn source_shrink_geometry(width: usize, height: usize) -> Option<(usize, usize, usize)> {
    let area = signed_area(width, height);
    if area <= 1_600_000 { return None; }
    let divisor = (area as f64 / 1_600_000.0).sqrt().ceil();
    let reciprocal = 1.0 / divisor;
    Some(((width as f64 * reciprocal).ceil() as usize,
        (height as f64 * reciprocal).ceil() as usize, divisor as usize))
}

/// Recommended source dimensions without allocating or reading pixels.
pub fn source_shrink_dimensions(width: usize, height: usize) -> [usize; 2] {
    source_shrink_geometry(width, height).map_or([width, height], |(width, height, _)| [width, height])
}

/// Optional native source reduction, distinct from working-image filtering.
/// No reduction means no allocation; the caller owns source replacement.
pub fn shrink_source_image(source: &OwnedImage) -> Option<OwnedImage> {
    let (width, height, step) = source_shrink_geometry(source.width, source.height)?;
    let normalization = f32::from_bits(0x3b808081);
    let mut bgra = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let first_y = y * step;
        let rows = step.min(source.height - first_y);
        for x in 0..width {
            let first_x = x * step;
            let columns = step.min(source.width - first_x);
            let weight = ((1.0 / rows as f64) * (1.0 / columns as f64)) as f32;
            let mut channels = [0.0f32; 4];
            for row in first_y..first_y + rows {
                for column in first_x..first_x + columns {
                    let pixel = (row * source.width + column) * 4;
                    for channel in 0..4 {
                        // Preserve both native float32 multiplications and each sum spill.
                        channels[channel] += (source.bgra[pixel + channel] as f32 * normalization) * weight;
                    }
                }
            }
            bgra.extend(channels.map(|sum| (sum as f64 * 255.0 + 0.5).trunc() as u8));
        }
    }
    Some(OwnedImage { width, height, bgra })
}
