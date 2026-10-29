//! Port of `src/blend-solver.mjs`: per-pixel color blend fitting over the
//! native small-matrix routines (column-major Cholesky, reverse-order forward
//! substitution and an active-set nonnegativity loop).
const BYTE_SCALE: f32 = 1.0 / 255.0;

pub const EIGHT_DIRECTIONS: [(i64, i64); 8] = [(1, 0), (0, 1), (-1, 0), (0, -1), (-1, 1), (1, -1), (-1, -1), (1, 1)];
pub const TWENTY_FOUR_DIRECTIONS: [(i64, i64); 24] = [(1, 0), (0, 1), (-1, 0), (0, -1), (-1, 1), (1, -1), (-1, -1), (1, 1),
    (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2), (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
    (-1, -2), (0, -2), (1, -2), (-1, 2), (0, 2), (1, 2)];

/// A sample whose first two channels stay double and last two are float32,
/// matching the native register/storage mix. Values are held as f64.
pub fn write_blend_sample(bgra: &[u8], offset: usize, sample: &mut [f64; 4]) {
    for channel in 0..4 {
        let value = bgra[offset + channel] as f64 * BYTE_SCALE as f64;
        sample[channel] = if channel < 2 { value } else { value as f32 as f64 };
    }
}

pub fn collect_blend_indices(indices: &[i32], width: usize, height: usize, pixel: usize, directions: &[(i64, i64)],
    source: i32, target: i32) -> Vec<i32> {
    let remap = |r: i32| if r == source { target } else { r };
    let mut selected = vec![remap(indices[pixel])];
    let (x, y) = ((pixel % width) as i64, (pixel / width) as i64);
    for (dx, dy) in directions {
        let (column, row) = (x + dx, y + dy);
        if column < 0 || column >= width as i64 || row < 0 || row >= height as i64 { continue; }
        let color = remap(indices[(row * width as i64 + column) as usize]);
        if !selected.contains(&color) {
            selected.push(color);
            if selected.len() == 9 { break; }
        }
    }
    selected
}

pub fn dot_float(first: &[f32], second: &[f32]) -> f32 {
    let mut value = first[0] * second[0];
    for channel in 1..4 { value += first[channel] * second[channel]; }
    value
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlendFit { pub color: Vec<f32>, pub weights: Vec<f64>, pub self_weight: f64 }

fn fit_two_colors(pixel: &[f64; 4], colors: &[f32]) -> BlendFit {
    let difference: Vec<f32> = (0..4).map(|c| colors[c + 4] - colors[c]).collect();
    let norm = dot_float(&difference, &difference);
    if norm <= 0.0 { return BlendFit { color: colors[..4].to_vec(), weights: vec![0.5, 0.5], self_weight: 0.5 }; }
    let relative: Vec<f32> = (0..4).map(|c| (pixel[c] as f32) - colors[c]).collect();
    let norm64 = norm as f64;
    let mut second = dot_float(&difference, &relative) as f64 / norm64;
    second = (1.0 - (second + second)) * (0.01 / (norm64 + 0.02)) + second;
    second = second.clamp(0.0, 1.0);
    let first = 1.0 - second;
    let rounded = second as f32;
    let color = (0..4).map(|c| colors[c] + rounded * difference[c]).collect();
    BlendFit { color, weights: vec![first, second], self_weight: first }
}

// Column-major storage and loop order follow the native small-matrix routines.
fn factor_positive_matrix(matrix: &[f64], size: usize) -> Result<Vec<f64>, &'static str> {
    let mut factor = matrix.to_vec();
    for row in 0..size {
        for column in 0..row {
            let offset = column * size + row;
            for previous in 0..column {
                factor[offset] -= factor[previous * size + row] * factor[previous * size + column];
            }
            factor[offset] /= factor[column * size + column];
        }
        let diagonal = row * size + row;
        for column in 0..row {
            let value = factor[column * size + row];
            factor[diagonal] -= value * value;
        }
        if factor[diagonal] <= 0.0 { return Err("Color-blend matrix is not positive definite."); }
        factor[diagonal] = factor[diagonal].sqrt();
    }
    Ok(factor)
}

fn solve_factored(factor: &[f64], right: &[f64], size: usize) -> Vec<f64> {
    let mut solved = right.to_vec();
    for row in 0..size {
        // Native forward substitution subtracts previous terms in reverse order.
        let mut value = solved[row];
        for column in (0..row).rev() { value -= solved[column] * factor[column * size + row]; }
        solved[row] = value / factor[row * size + row];
    }
    for row in (0..size).rev() {
        let mut value = solved[row];
        for column in row + 1..size { value -= solved[column] * factor[row * size + column]; }
        solved[row] = value / factor[row * size + row];
    }
    solved
}

fn fit_multiple_colors(pixel: &[f64; 4], colors: &[f32]) -> Result<BlendFit, &'static str> {
    let count = colors.len() / 4;
    let mut gram = vec![0f64; count * count];
    let mut projection = vec![0f64; count];
    for column in 0..count {
        for row in 0..count {
            let mut value = 0f64;
            for channel in 0..3 { value += colors[row * 4 + channel] as f64 * colors[column * 4 + channel] as f64; }
            gram[column * count + row] = value + if row == column { 0.01 } else { 0.0 };
        }
        for channel in 0..3 { projection[column] += colors[column * 4 + channel] as f64 * pixel[channel]; }
    }
    let factor = factor_positive_matrix(&gram, count)?;
    let mut constrained = vec![false; count];
    let mut constraints = vec![vec![1f64; count]];
    // The factor and existing constraints stay fixed as new bounds are added.
    let mut solved_constraints = vec![solve_factored(&factor, &constraints[0], count)];
    let weights = loop {
        let n = constraints.len();
        let mut schur = vec![0f64; n * n];
        let mut right = vec![0f64; n];
        for column in 0..n {
            for row in 0..n {
                for color in 0..count { schur[column * n + row] += constraints[row][color] * solved_constraints[column][color]; }
            }
            for color in 0..count { right[column] += solved_constraints[column][color] * projection[color]; }
        }
        right[0] -= 1.0;
        let multipliers = solve_factored(&factor_positive_matrix(&schur, n)?, &right, n);
        let mut adjusted = projection.clone();
        for constraint in 0..n {
            let multiplier = -multipliers[constraint];
            for color in 0..count { adjusted[color] += multiplier * constraints[constraint][color]; }
        }
        let weights = solve_factored(&factor, &adjusted, count);
        let (mut most_negative, mut minimum) = (usize::MAX, 0f64);
        for color in 0..count {
            if !constrained[color] && weights[color] < minimum { minimum = weights[color]; most_negative = color; }
        }
        if most_negative == usize::MAX || weights[most_negative] >= -0.0001 { break weights; }
        constrained[most_negative] = true;
        let mut constraint = vec![0f64; count];
        constraint[most_negative] = 1.0;
        solved_constraints.push(solve_factored(&factor, &constraint, count));
        constraints.push(constraint);
    };
    let mut color: Vec<f32> = pixel.iter().map(|v| *v as f32).collect();
    for channel in 0..3 {
        let mut value = 0f64;
        for entry in 0..count { value += weights[entry] * colors[entry * 4 + channel] as f64; }
        color[channel] = value as f32;
    }
    let self_weight = weights[0];
    Ok(BlendFit { color, weights, self_weight })
}

/// Fit a pixel against 1, 2 or more prepared float32 BGRA colors.
pub fn fit_prepared_blend(pixel: &[f64; 4], colors: &[f32]) -> Result<BlendFit, &'static str> {
    match colors.len() {
        4 => Ok(BlendFit { color: colors.to_vec(), weights: vec![1.0], self_weight: 1.0 }),
        8 => Ok(fit_two_colors(pixel, colors)),
        _ => fit_multiple_colors(pixel, colors),
    }
}
