//! Port of `src/shadow-classification.mjs`: shadow region decision trees,
//! pre-refinement shadow statistics and shadow pixel reassignment.
use crate::blend_solver::dot_float;
use crate::blending::boundary_mask;

const BYTE_SCALE: f32 = 1.0 / 255.0;
fn byte_to_unit(byte: u8) -> f32 { byte as f32 * BYTE_SCALE }

/// Tree node: feature test (`features[i] < t`), control test, or leaf.
enum Model { Leaf(bool), Test(usize, f64, &'static Model, &'static Model), Control(i32, &'static Model, &'static Model) }
use Model::{Control, Leaf, Test};

const T: Model = Leaf(true);
const F: Model = Leaf(false);

static REGION_MODEL: Model = Test(23, 0.00402429,
    &Test(18, 1.5,
        &Test(14, 1.5, &F, &Test(5, 0.0491163, &Test(20, 1.5, &T, &F), &Test(22, 0.196468, &F, &T))),
        &Test(20, 0.5, &F,
            &Test(19, 0.5, &Test(1, 0.0112894, &T, &F),
                &Test(41, 0.449972,
                    &Test(21, 0.118876, &Test(0, 41.535, &T, &F), &Test(11, 5.5, &T, &F)), &F)))),
    &Test(23, 0.0241458,
        &Test(5, 0.0473801, &Test(22, 0.0138127, &Test(39, 0.0102382, &F, &T), &T), &F), &F));

static ACCEPTANCE_MODEL: Model = Test(37, 3.57554,
    &Test(28, 0.0442212,
        &Test(39, 0.00627504,
            &Control(1, &T, &Control(2, &T, &Test(4, 0.934367, &F, &T))), &T),
        &Control(0, &T, &Test(34, 0.694472, &Test(0, 3.09081, &F, &T), &F))),
    &Test(35, 0.00258463, &T, &F));

fn evaluate(model: &Model, features: &[f64], control: Option<i32>) -> bool {
    let mut node = model;
    loop {
        node = match node {
            Leaf(value) => return *value,
            Test(i, t, a, b) => if features[*i] < *t { a } else { b },
            Control(c, a, b) => if control == Some(*c) { a } else { b },
        };
    }
}

/// Internal possible-shadow decision over the producer's 42 float64 values.
pub fn classify_shadow_features(features: &[f64]) -> bool { evaluate(&REGION_MODEL, features, None) }

/// Internal refinement acceptance decision with the original int32 control.
pub fn accept_shadow_features(features: &[f64], control: i32) -> bool { evaluate(&ACCEPTANCE_MODEL, features, Some(control)) }

fn multiply_color_matrix(matrix: &[f32; 16], vector: &[f32; 4], output: &mut [f32; 4]) {
    for channel in 0..4 {
        let mut sum = 0.0f32;
        for input in 0..4 { sum += matrix[input * 4 + channel] * vector[input]; }
        output[channel] = sum;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Principal { pub eigenvalue: f64, pub direction: [f32; 4] }

/// Internal principal-color iteration with explicit retained float32 state.
/// Native iteration limits are signed int32; nonpositive limits retain the initial state.
pub fn principal_color_direction(matrix: &[f32; 16], previous_eigenvalue: f64, iterations: i32, epsilon: f32) -> Principal {
    let mut direction = [1.0f32; 4];
    let mut working = [0.0f32; 4];
    multiply_color_matrix(matrix, &direction, &mut working);
    let mut eigenvalue = previous_eigenvalue;
    for _ in 0..iterations {
        let mut squared = 0.0f64;
        for c in working { squared += c as f64 * c as f64; }
        let length = squared.sqrt() as f32;
        if length > 0.0 {
            let reciprocal = (1.0 / length as f64) as f32;
            for c in working.iter_mut() { *c *= reciprocal; }
        }
        direction = working;
        multiply_color_matrix(matrix, &direction, &mut working);
        let mut rayleigh = 0.0f64;
        for c in 0..4 { rayleigh += working[c] as f64 * direction[c] as f64; }
        let estimate = rayleigh as f32 as f64;
        if (eigenvalue - estimate).abs() < epsilon as f64 { return Principal { eigenvalue, direction }; }
        eigenvalue = estimate;
    }
    Principal { eigenvalue, direction }
}

/// Region statistics consumed by shadow measurement (subpixel state).
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowRegion {
    pub count: u32,
    pub interior_count: u32,
    pub sums: [f32; 4],
    pub squares: [f32; 4],
    pub color: [f32; 4],
    /// Boundary pixels in insertion order.
    pub boundary: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowFeatures { pub features: Vec<[f64; 42]>, pub covariances: Vec<[f32; 16]>, pub directions: Vec<[f32; 4]>, pub eigenvalue: f64 }

/// Internal pre-refinement shadow statistics over a rebuilt subpixel state.
#[allow(clippy::too_many_arguments)]
pub fn measure_shadow_features(width: usize, height: usize, bgra: &[u8], indices: &[i32], regions: &[ShadowRegion],
    gradient: &[f64], blur_gradient: &[f64], initial_eigenvalue: f64) -> ShadowFeatures {
    let mut features = vec![[0.0f64; 42]; regions.len()];
    let mut covariances = vec![[0.0f32; 16]; regions.len()];
    let mut means = vec![[0.0f32; 4]; regions.len()];
    let mut directions = Vec::with_capacity(regions.len());
    let anchors: Vec<&ShadowRegion> = regions.iter().filter(|s| s.interior_count > 10).collect();
    let mask = boundary_mask(width, height, indices);
    let mut sample = [0.0f32; 4];
    for pixel in 0..indices.len() {
        let region = indices[pixel] as usize;
        let values = &mut features[region];
        values[0] += gradient[pixel]; values[1] += blur_gradient[pixel];
        values[6] += gradient[pixel].sqrt(); values[7] += blur_gradient[pixel];
        if mask[pixel] == 0 { values[10] += 1.0; }
        for c in 0..4 { sample[c] = byte_to_unit(bgra[pixel * 4 + c]); }
        let matrix = &mut covariances[region];
        for row in 0..4 { for column in 0..4 { matrix[row * 4 + column] += sample[row] * sample[column]; } }
    }
    let mut eigenvalue = initial_eigenvalue;
    for (region, statistic) in regions.iter().enumerate() {
        let reciprocal = (1.0 / (statistic.count as f32) as f64) as f32;
        let mean = &mut means[region];
        for c in 0..4 { mean[c] = statistic.sums[c] * reciprocal; }
        let matrix = &mut covariances[region];
        for row in 0..4 { for column in 0..4 {
            let o = row * 4 + column;
            matrix[o] = matrix[o] * reciprocal - mean[row] * mean[column];
        } }
        let principal = principal_color_direction(matrix, eigenvalue, 10, 1e-6f32);
        directions.push(principal.direction);
        eigenvalue = principal.eigenvalue;
    }
    let mut residual = [0.0f32; 4];
    for pixel in 0..indices.len() {
        let region = indices[pixel] as usize;
        let (mean, direction) = (means[region], directions[region]);
        for c in 0..4 { sample[c] = byte_to_unit(bgra[pixel * 4 + c]); residual[c] = sample[c] - mean[c]; }
        let projection = dot_float(&residual, &direction);
        let mut squared = 0.0f64;
        for c in 0..4 {
            residual[c] = sample[c] - (direction[c] * projection + mean[c]);
            squared += residual[c] as f64 * residual[c] as f64;
        }
        features[region][41] += squared;
    }
    for (region, statistic) in regions.iter().enumerate() {
        let mean = means[region];
        let color = statistic.color;
        let count = statistic.count as f64;
        let values = &mut features[region];
        values[2] = values[0].sqrt(); values[3] = values[1].sqrt();
        values[4] = (values[0] / count).sqrt(); values[5] = (values[1] / count).sqrt();
        values[8] = count; values[9] = statistic.interior_count as f64;
        for f in 0..8 { values[25 + f] = values[f] / count; }
        values[39] = 1e100;
        for candidate in &anchors {
            let mut distance = 0.0f64;
            for c in 0..4 { let d = candidate.color[c] - color[c]; distance += d as f64 * d as f64; }
            if distance < values[39] { values[39] = distance; }
        }
        values[21] = (color[0] as f64 * 0.11 + color[1] as f64 * 0.59) + color[2] as f64 * 0.3;
        let reciprocal = (1.0 / (statistic.count as f32) as f64) as f32;
        for c in 0..4 { values[22] += (statistic.squares[c] * reciprocal - mean[c] * mean[c]) as f64; }
        values[23] = statistic.interior_count as f64 / statistic.boundary.len() as f64;
        let mut neighbors: Vec<i32> = Vec::new();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0usize, 0usize);
        for &pixel in &statistic.boundary {
            if !neighbors.contains(&indices[pixel]) { neighbors.push(indices[pixel]); }
            let (x, y) = (pixel % width, pixel / width);
            min_x = min_x.min(x); min_y = min_y.min(y); max_x = max_x.max(x); max_y = max_y.max(y);
        }
        values[24] = (max_x as f64 - min_x as f64) / (max_y as f64 - min_y as f64);
        values[11] = neighbors.len() as f64 - 1.0;
        for &neighbor in &neighbors {
            if neighbor as usize == region { continue; }
            let candidate = &regions[neighbor as usize];
            if candidate.interior_count == 0 { values[12] += 1.0; }
            if candidate.count > 4 { values[13] += 1.0; }
            if candidate.count > 9 { values[14] += 1.0; }
            if candidate.count > 14 { values[15] += 1.0; }
            if candidate.interior_count > 4 { values[16] += 1.0; }
            if candidate.interior_count > 9 { values[17] += 1.0; }
            if candidate.interior_count > 14 { values[18] += 1.0; }
            let nc = candidate.color;
            let luminance = (nc[2] as f64 * 0.3 + nc[0] as f64 * 0.11) + nc[1] as f64 * 0.59;
            values[if luminance > values[21] { 19 } else { 20 }] += 1.0;
        }
    }
    ShadowFeatures { features, covariances, directions, eigenvalue }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Proposal { pub cost: f32, pub target: i32 }

const ASSIGNMENT_DIRECTIONS: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// Internal four-neighbor proposal. A rejected proposal leaves the target untouched.
pub fn write_shadow_proposal(width: usize, height: usize, bgra: &[u8], indices: &[i32], shadow_flags: &[u8], pixel: usize,
    proposal: &mut Proposal) -> bool {
    let (x, y) = ((pixel % width) as i64, (pixel / width) as i64);
    proposal.cost = 1e10f32;
    let mut accepted = false;
    for (dx, dy) in ASSIGNMENT_DIRECTIONS {
        let (column, row) = (x + dx, y + dy);
        if column < 0 || column >= width as i64 || row < 0 || row >= height as i64 { continue; }
        let neighbor = row as usize * width + column as usize;
        let target = indices[neighbor];
        if shadow_flags[target as usize] & 1 != 0 { continue; }
        let mut cost = 0.0f32;
        for c in 0..4 {
            let d = byte_to_unit(bgra[neighbor * 4 + c]) - byte_to_unit(bgra[pixel * 4 + c]);
            cost += d * d;
        }
        if cost < proposal.cost { proposal.cost = cost; proposal.target = target; accepted = true; }
    }
    accepted
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowAssignment { pub pixel: usize, pub target: i32, pub cost: f32 }

/// Fill shadow pixels before acceptance, observing each newly assigned frontier.
pub fn reassign_shadow_pixels(width: usize, height: usize, bgra: &[u8], original: &[i32], shadow_flags: &[u8])
    -> Result<(Vec<i32>, Vec<ShadowAssignment>), String> {
    let mut indices = original.to_vec();
    let mut assignments = Vec::new();
    let mut proposal = Proposal { cost: 0.0, target: -1 };
    for region in 0..shadow_flags.len() {
        if shadow_flags[region] & 1 == 0 { continue; }
        let mut pending: Vec<usize> = (0..indices.len()).filter(|&p| indices[p] == region as i32).collect();
        while !pending.is_empty() {
            let (mut selected, mut target, mut cost) = (usize::MAX, -1, f32::INFINITY);
            for position in 0..pending.len() {
                if write_shadow_proposal(width, height, bgra, &indices, shadow_flags, pending[position], &mut proposal) && proposal.cost < cost {
                    selected = position; target = proposal.target; cost = proposal.cost;
                }
            }
            if selected == usize::MAX { return Err("Shadow region has no non-shadow boundary.".into()); }
            let pixel = pending.remove(selected);
            indices[pixel] = target;
            assignments.push(ShadowAssignment { pixel, target, cost });
        }
    }
    Ok((indices, assignments))
}
