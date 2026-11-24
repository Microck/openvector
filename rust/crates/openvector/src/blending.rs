//! Port of `src/blending.mjs`.
use crate::blend_solver::{MAX_FIT_COLORS, collect_blend_indices_into, dot_float, fit_prepared_blend, fit_prepared_blend_into, write_blend_sample, BlendFit, EIGHT_DIRECTIONS};
use crate::pixel_input::{fail, read_palette_colors, read_palette_score_input, validate_bgra, InputError};

/// Fit ordered local colors using the original blend model's precision rules.
pub fn fit_color_blend(bgra: &[u8], colors: &[f64]) -> Result<BlendFit, InputError> {
    validate_bgra(bgra)?;
    if bgra.len() != 4 { return fail("Blend sample must contain one BGRA byte pixel."); }
    let normalized = read_palette_colors(colors)?;
    if normalized.len() > 36 { return fail("Blend colors must contain one through nine BGRA colors."); }
    let mut pixel = [0.0; 4];
    write_blend_sample(bgra, 0, &mut pixel);
    fit_prepared_blend(&pixel, &normalized).map_err(InputError::new)
}

/// Internal radius-two mask shared by palette scoring and image classification.
pub fn boundary_mask(width: usize, height: usize, indices: &[i32]) -> Vec<u8> {
    let mut mask = vec![0u8; indices.len()];
    for y in 0..height.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let p = y * width + x;
            if indices[p] != indices[p + 1] || indices[p] != indices[p + width] || indices[p] != indices[p + width + 1] {
                mask[p] = 2; mask[p + 1] = 2; mask[p + width] = 2; mask[p + width + 1] = 2;
            }
        }
    }
    for y in 0..height.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let p = y * width + x;
            let quad = [p, p + 1, p + width, p + width + 1];
            if quad.iter().any(|&q| mask[q] > 1) {
                for q in quad { if mask[q] == 0 { mask[q] = 1; } }
            }
        }
    }
    mask
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightedScore { pub total: f32, pub error: f32, pub colors: f32 }

/// Score explicit anti-alias assignments using the original local blend fit.
pub fn score_weighted_palette(width: f64, height: f64, bgra: &[u8], palette: &[f64], indices: &[i64], cost_per_color: f64)
    -> Result<WeightedScore, InputError> {
    let input = read_palette_score_input(bgra, palette, indices, cost_per_color)?;
    const MAX: f64 = 2147483647.0;
    if width.fract() != 0.0 || height.fract() != 0.0 || !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0
        || width > MAX || height > MAX || width * height > MAX || width * height * 4.0 != bgra.len() as f64 {
        return fail("Weighted score needs nonnegative int32 dimensions with exactly width * height BGRA pixels.");
    }
    Ok(score_prepared_weighted_palette(width as usize, height as usize, bgra, &input.colors, &input.indices, input.cost_per_color))
}

/// Internal scoring kernel shared with candidate generation.
pub fn score_prepared_weighted_palette(width: usize, height: usize, bgra: &[u8], colors: &[f32], indices: &[i32], cost_per_color: f64)
    -> WeightedScore {
    let mask = boundary_mask(width, height, indices);
    let mut sample = [0.0f64; 4];
    let mut differences = [0.0f32; 4];
    let mut sum = 0.0f32;
    // One answer buffer for the whole scan: the fit writes into it and the
    // prediction is read out of it before the next fit, so the loop allocates
    // nothing per boundary pixel. The values are the ones the owned fit
    // produced, bit for bit.
    let mut fitted = BlendFit::default();
    for pixel in 0..indices.len() {
        write_blend_sample(bgra, pixel * 4, &mut sample);
        let predicted: &[f32] = if mask[pixel] != 0 {
            let mut selected = [0i32; MAX_FIT_COLORS];
            let len = collect_blend_indices_into(indices, width, height, pixel, &EIGHT_DIRECTIONS, -1, -1, &mut selected);
            let selected = &selected[..len];
            // `selected` holds at most nine colors, so the local palette is at
            // most 36 `f32`. Building it in a fixed array instead of a `Vec`
            // removes one heap block per boundary pixel; the entries are the
            // same four channels of the same colors in the same order, and the
            // slice handed to the fit is the same length, so every fit sees
            // identical operands.
            let mut local = [0.0f32; 4 * MAX_FIT_COLORS];
            for (entry, &c) in selected.iter().enumerate() {
                local[entry * 4..entry * 4 + 4].copy_from_slice(&colors[c as usize * 4..c as usize * 4 + 4]);
            }
            fit_prepared_blend_into(&sample, &local[..selected.len() * 4], &mut fitted).expect("blend fit");
            &fitted.color
        } else {
            let o = indices[pixel] as usize * 4;
            &colors[o..o + 4]
        };
        for c in 0..4 { differences[c] = predicted[c] - sample[c] as f32; }
        sum += dot_float(&differences, &differences);
    }
    let error = (1.0 + sum as f64).log10() as f32;
    let color_cost = ((colors.len() / 4) as f32 - 2.0) * cost_per_color as f32;
    WeightedScore { total: error + color_cost, error, colors: color_cost }
}
