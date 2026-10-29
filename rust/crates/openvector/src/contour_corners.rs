//! Port of `src/contour-corners.mjs`.
use crate::contours::Geometry;
use crate::pixel_input::{fail, InputError};

/// Original 0x4adfb0 decision tree over six fields of the 21-double vector.
/// Negated comparisons preserve unordered branches when a value is NaN.
pub fn classify_corner_features(f: &[f64]) -> bool {
    if !(f[6] < 0.611194) {
        if !(f[15] < 0.5) {
            if !(f[16] < 0.097537) { return false; }
            return !(f[6] < 1.42496) || f[2] < 0.122841 || f[0] < 0.445642 || f[8] < 0.0046405;
        }
        return !(f[6] < 1.72002);
    }
    if f[6] < 0.282594 || !(f[0] < 0.322933) { return false; }
    f[0] < 0.0286571 || !(f[6] < 0.483023) || f[2] < 0.00209524
}

fn read_corner_geometry(geometry: &Geometry) -> Result<Geometry, InputError> {
    if geometry.nodes.is_empty() || geometry.regions.is_empty() {
        return fail("Corner puncturing needs shared nodes and region contours.");
    }
    if geometry.nodes.iter().any(|n| !n.x.is_finite() || !n.y.is_finite()) {
        return fail("Corner puncturing needs finite coordinates, byte kinds and byte flags.");
    }
    for region in &geometry.regions {
        if region.contour.len() < 3 || region.boundary.len() != region.contour.len() {
            return fail("Corner puncturing needs matching contour nodes and boundary records.");
        }
        for (sample, &node) in region.contour.iter().enumerate() {
            let b = region.boundary[sample][0];
            if node >= geometry.nodes.len() || !(0..=255).contains(&b) {
                return fail("Corner puncturing needs existing node identifiers and byte boundary flags.");
            }
        }
    }
    Ok(geometry.clone())
}

fn corner_turns(geometry: &Geometry, contour: &[usize]) -> Vec<f64> {
    let n = contour.len();
    let nodes = &geometry.nodes;
    let directions: Vec<(f64, f64)> = (0..n).map(|edge| {
        let (a, b) = (&nodes[contour[edge]], &nodes[contour[(edge + 1) % n]]);
        let (mut x, mut y) = (b.x - a.x, b.y - a.y);
        let length = (y * y + (x * x + 0.0)).sqrt();
        if length > 0.0 { let inv = 1.0 / length; x *= inv; y *= inv; }
        (x, y)
    }).collect();
    (0..n).map(|s| {
        let (first, last) = (directions[(s + n - 1) % n], directions[s]);
        let dot = last.1 * first.1 + (last.0 * first.0 + 0.0);
        2.0 - (dot + dot)
    }).collect()
}

/// Puncture corners in region order while retaining shared-node markers.
pub fn puncture_contour_corners(input: &Geometry) -> Result<Geometry, InputError> {
    let mut geometry = read_corner_geometry(input)?;
    let mut features = [0.0f64; 21];
    let mut turns = [0.0f64; 5];
    for r in 0..geometry.regions.len() {
        let n = geometry.regions[r].contour.len();
        if n <= 6 { continue; }
        let region_turns = corner_turns(&geometry, &geometry.regions[r].contour);
        for sample in 0..n {
            for t in 0..5 { turns[t] = region_turns[(sample + t + n - 2) % n]; }
            let mut maximum = turns[0];
            for t in 1..5 { if maximum < turns[t] { maximum = turns[t]; } }
            let peak = turns[2] == maximum;
            if peak { geometry.regions[r].boundary[sample][0] |= 8; }
            let left_weaker = !(turns[1] > turns[3]);
            let weak_outer = turns[if left_weaker { 0 } else { 4 }];
            let weak_inner = turns[if left_weaker { 1 } else { 3 }];
            let strong_outer = turns[if left_weaker { 4 } else { 0 }];
            let mut minimum = turns[0];
            for t in [1, 3, 4] { if turns[t] < minimum { minimum = turns[t]; } }
            features[0] = (turns[1] + turns[3]) + ((turns[0] + turns[4]) + 0.0);
            features[2] = strong_outer + (weak_inner + (weak_outer + 0.0));
            features[6] = turns[2];
            features[8] = strong_outer;
            features[15] = peak as i32 as f64;
            features[16] = minimum;
            if classify_corner_features(&features) {
                let node = geometry.regions[r].contour[sample];
                geometry.nodes[node].kind = 3;
                geometry.regions[r].boundary[sample][0] |= 1;
            }
        }
    }
    Ok(geometry)
}
