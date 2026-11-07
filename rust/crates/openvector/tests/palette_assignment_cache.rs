//! Exact equivalence against the original scalar assignment/refinement loops.
//! Do not replace the reference with the production assignment implementation.
use openvector::palette::{assign_palette, byte_to_unit, measure_anti_alias, measurement_weights, CoreSession, GeneratedPalette};

fn scalar_assign(bgra: &[u8], colors: &[f32]) -> Vec<i32> {
    bgra.chunks_exact(4).map(|pixel| {
        let sample: [f32; 4] = std::array::from_fn(|c| byte_to_unit(pixel[c]));
        let distance = |offset: usize| {
            let b = sample[0] - colors[offset]; let g = sample[1] - colors[offset + 1];
            let r = sample[2] - colors[offset + 2]; let a = sample[3] - colors[offset + 3];
            let mut cost = b * b + g * g; cost += r * r; cost + a * a
        };
        let mut best = 0; let mut cost = distance(0);
        for color in 1..colors.len() / 4 {
            let candidate = distance(color * 4);
            if candidate < cost { best = color as i32; cost = candidate; }
        }
        best
    }).collect()
}

fn random_colors(seed: &CoreSession, output: &mut [f32]) {
    output[2] = seed.next_random() as f32; output[1] = seed.next_random() as f32;
    output[0] = seed.next_random() as f32; output[3] = 1.0;
}

fn scalar_refine(bgra: &[u8], mut colors: Vec<f32>, measurement: Option<&[u8]>, pixel_weights: Option<&[f64]>) -> (GeneratedPalette, u32) {
    let session = CoreSession::default(); let mut indices = Vec::new();
    let mut means = vec![0.0f32; colors.len()]; let mut counts = vec![0.0f32; colors.len() / 4];
    let mut weights = vec![0.0; counts.len()]; let mut converged = false; let mut passes = 0;
    for iteration in 0..10 {
        indices = scalar_assign(bgra, &colors); passes += 1;
        if iteration == 9 || converged {
            weights.fill(0.0);
            for (pixel, &index) in indices.iter().enumerate() {
                weights[index as usize] += measurement.map_or(1.0, |m| m[pixel] as f64 * (1.0 / 255.0));
            }
        }
        if converged { break; }
        means.fill(0.0); counts.fill(0.0);
        for (pixel, &index) in indices.iter().enumerate() {
            let color = index as usize; let weight = pixel_weights.map_or(1.0, |w| w[pixel] as f32);
            for c in 0..4 { means[color * 4 + c] += byte_to_unit(bgra[pixel * 4 + c]) * weight; }
            counts[color] += weight;
        }
        for (color, &count) in counts.iter().enumerate() {
            let mean = &mut means[color * 4..color * 4 + 4];
            if count > 0.0 { let reciprocal = (1.0 / count as f64) as f32; for value in mean { *value *= reciprocal; } }
            else { random_colors(&session, mean); }
        }
        let mut change = 0.0f64;
        for (a, b) in colors.iter().zip(&means) { change = change.max((*a - *b).abs() as f64); }
        colors.copy_from_slice(&means); converged = change < 0.01f32 as f64;
    }
    for (color, &count) in counts.iter().enumerate() { if count == 0.0 { colors[color * 4..color * 4 + 4].fill(0.0); } }
    (GeneratedPalette { palette: colors, indices, weights, passes }, session.random_state())
}

fn assert_bits(actual: &GeneratedPalette, expected: &GeneratedPalette) {
    assert_eq!(actual.palette.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), expected.palette.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "palette bits");
    assert_eq!(actual.weights.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), expected.weights.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "weight bits");
    assert_eq!(actual.indices, expected.indices, "assignment map"); assert_eq!(actual.passes, expected.passes, "passes");
}

fn images() -> Vec<Vec<u8>> {
    let mut colors = Vec::new();
    for byte in 0..=255u8 { colors.extend_from_slice(&[byte, 255 - byte, byte.wrapping_mul(37), byte.wrapping_mul(71)]); }
    let repeated: Vec<u8> = (0..48 * 32).flat_map(|i| { let p = i % 256; colors[p * 4..p * 4 + 4].to_vec() }).collect();
    let runs: Vec<u8> = (0..48 * 32).flat_map(|i| { let p = i / 64 % 256; colors[p * 4..p * 4 + 4].to_vec() }).collect();
    let mut state = 0x1836_aba1_u32; let mut noise = Vec::new();
    for _ in 0..48 * 32 { state ^= state << 13; state ^= state >> 17; state ^= state << 5; noise.extend_from_slice(&state.to_le_bytes()); }
    vec![repeated, runs, noise, vec![0u8; 48 * 32 * 4]]
}

#[test]
fn assignments_match_scalar_for_repeats_unique_colors_and_ties() {
    let session = CoreSession::default();
    for count in [1, 2, 3, 8, 12, 32] {
        let mut palette = vec![0.0f32; count * 4];
        for color in palette.chunks_exact_mut(4) { random_colors(&session, color); }
        for image in images() {
            let colors = palette.iter().map(|&v| v as f64).collect::<Vec<_>>();
            assert_eq!(assign_palette(&image, &colors).unwrap(), scalar_assign(&image, &palette));
        }
    }
    // Equal-distance and duplicate colors must keep the earliest palette entry,
    // including signed zero and transparent BGRA colors.
    for palette in [vec![0.0, 0.0, 0.0, 0.0, -0.0, 0.0, 0.0, 0.0],
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0],
        vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]] {
        for image in images() {
            let f32_colors = palette.iter().map(|&v| v as f32).collect::<Vec<_>>();
            assert_eq!(assign_palette(&image, &palette).unwrap(), scalar_assign(&image, &f32_colors));
        }
    }
}

#[test]
fn weighted_and_unweighted_refinement_preserve_every_bit_and_random_draw() {
    // Duplicated initial colors force empty clusters and random replacement.
    let colors = vec![0.0f32, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 1.0,
        1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0, 0.5, 0.5];
    let palette = colors.iter().map(|&v| v as f64).collect::<Vec<_>>();
    for image in images() {
        for weighted in [false, true] {
            let measurement = weighted.then(|| measure_anti_alias(48, 32, &image).unwrap());
            let weights = measurement.as_ref().map(|m| measurement_weights(m, 50.0, 0.01));
            let (expected, state) = scalar_refine(&image, colors.clone(), measurement.as_deref(), weights.as_deref());
            let session = CoreSession::default();
            let actual = if weighted { session.refine_weighted(48, 32, &image, &palette) }
                else { session.refine_unweighted(&image, &palette) }.unwrap();
            assert_bits(&actual, &expected); assert_eq!(session.random_state(), state, "random state");
        }
    }
}

#[test]
fn colliding_transparent_colors_and_changed_palettes_do_not_reuse_wrong_indices() {
    // These different full BGRA keys land in the same direct-mapped cache slot.
    // The all-zero key also exercises the cache's invalid-entry sentinel.
    let pair = [[0, 0, 0, 0], [98, 2, 0, 0]];
    let pixels: Vec<u8> = (0..128).flat_map(|i| pair[i % 2]).collect();
    let mut palette: Vec<f64> = pair.iter().flatten().map(|&v| byte_to_unit(v) as f64).collect();
    for reversed in [false, true] {
        if reversed { palette.rotate_left(4); }
        let expected: Vec<i32> = (0..128).map(|i| (i % 2) ^ i32::from(reversed)).collect();
        assert_eq!(assign_palette(&pixels, &palette).unwrap(), expected);
    }
}
