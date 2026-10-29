//! Port of `src/fitting.mjs`.
use crate::extended_arithmetic::fitting_threshold_multiplier;
use serde::{Deserialize, Serialize};

/// Fitting node: coordinates, byte kind and assigned cubic slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitNode { pub x: f64, pub y: f64, pub kind: u8, pub cubic: i32 }

pub type Point = [f64; 2];

#[derive(Clone, Debug, PartialEq)]
pub struct FittingSamples { pub parameters: Vec<f64>, pub points: Vec<Point> }

#[derive(Clone, Debug, PartialEq)]
pub struct IntervalFit { pub cost: f64, pub curve: Vec<Point> }

#[derive(Clone, Debug, PartialEq)]
pub struct KeeperRecord { pub slot: i64, pub start: i64, pub count: i64, pub next: i64, pub cost: f64, pub dirty: bool }

#[derive(Clone, Debug, PartialEq)]
pub struct KeeperPartition { pub records: Vec<KeeperRecord>, pub threshold: f64 }

const QUADRATIC_DIFFERENCES: [&[f64]; 3] = [&[1.0], &[-1.0, 1.0], &[1.0, -2.0, 1.0]];
const CUBIC_BINOMIAL: [f64; 4] = [1.0, 3.0, 3.0, 1.0];
const CUBIC_DIFFERENCES: [&[f64]; 4] = [&[], &[-1.0], &[1.0, -2.0], &[-1.0, 3.0, -3.0]];

fn at(contour: &[usize], sample: i64) -> usize { contour[sample.rem_euclid(contour.len() as i64) as usize] }

/// Internal fitting kernel on the stage's owned geometry.
pub fn prepare_fitting_samples(nodes: &mut [FitNode], contour: &[usize], start: i64, mut end: i64, mark_interior: bool) -> FittingSamples {
    let cycle = contour.len() as i64;
    while end < start { end += cycle; }
    let count = end - start - 1;
    if count < 1 { return FittingSamples { parameters: Vec::new(), points: Vec::new() }; }
    let mut parameters = vec![0.0; count as usize];
    let mut points = Vec::with_capacity(count as usize);
    let first = &nodes[at(contour, start)];
    let (mut px, mut py) = (first.x, first.y);
    let mut length = 0.0;
    for sample in 0..count {
        let node = &mut nodes[at(contour, start + sample + 1)];
        let dx = node.x - px; let dy = node.y - py;
        length = (dy * dy + dx * dx).sqrt() + length;
        parameters[sample as usize] = length;
        points.push([node.x, node.y]);
        if mark_interior { node.kind = 2; }
        px = node.x; py = node.y;
    }
    let last = &nodes[at(contour, end)];
    let dx = last.x - px; let dy = last.y - py;
    let total = (dy * dy + dx * dx).sqrt() + length;
    let inverse = 1.0 / (if total == 0.0 { 1.0 } else { total });
    for p in parameters.iter_mut() { *p = inverse * *p; }
    FittingSamples { parameters, points }
}

fn solve_normal_equations(matrix: &mut [Vec<f64>], right: &mut [f64]) {
    let size = right.len();
    for column in 0..size {
        let mut pivot = column;
        for row in column + 1..size {
            if matrix[row][column].abs() > matrix[pivot][column].abs() { pivot = row; }
        }
        if pivot != column { matrix.swap(pivot, column); right.swap(pivot, column); }
        let inverse = 1.0 / matrix[column][column];
        for row in column + 1..size { matrix[row][column] *= inverse; }
        for next in column + 1..size {
            let multiplier = -matrix[column][next];
            for row in column + 1..size { let v = matrix[row][column] * multiplier; matrix[row][next] += v; }
        }
    }
    for column in 0..size {
        if right[column] != 0.0 {
            for row in column + 1..size { let v = right[column] * matrix[row][column]; right[row] -= v; }
        }
    }
    for column in (0..size).rev() {
        if right[column] != 0.0 {
            right[column] /= matrix[column][column];
            for row in 0..column { let v = right[column] * matrix[row][column]; right[row] -= v; }
        }
    }
}

fn fit_interior_controls(first: Point, last: Point, samples: &FittingSamples, quadratic: bool) -> Vec<Point> {
    let size = if quadratic { 2 } else { 4 };
    let mut design: Vec<[f64; 4]> = Vec::new();
    let mut residuals = Vec::new();
    for (sample, point) in samples.points.iter().enumerate() {
        let t = samples.parameters[sample]; let inverse = 1.0 - t;
        let first_weight = if quadratic { inverse * inverse } else { inverse * inverse * inverse };
        let last_weight = if quadratic { t * t } else { t * t * t };
        let left = if quadratic { inverse * t + inverse * t } else { inverse * inverse * t * 3.0 };
        let right = t * t * inverse * 3.0;
        for channel in 0..2 {
            let mut row = [0.0; 4];
            row[channel] = left;
            if !quadratic { row[channel + 2] = right; }
            design.push(row);
            residuals.push(point[channel] - (last_weight * last[channel] + first_weight * first[channel]));
        }
    }
    let mut matrix = vec![vec![0.0; size]; size];
    let mut right = vec![0.0; size];
    for column in 0..size {
        for row in 0..size {
            let mut sum = 0.0;
            for d in &design { sum = d[row] * d[column] + sum; }
            matrix[row][column] = sum + if row == column { 0.00001 } else { 0.0 };
        }
        let mut sum = 0.0;
        for (d, r) in design.iter().zip(&residuals) { sum = d[column] * r + sum; }
        right[column] = sum;
    }
    solve_normal_equations(&mut matrix, &mut right);
    if quadratic { vec![first, [right[0], right[1]], last] }
    else { vec![first, [right[0], right[1]], [right[2], right[3]], last] }
}

fn evaluate_curve(curve: &[Point], t: f64) -> Point {
    let inverse = 1.0 - t;
    if curve.len() == 3 {
        let f = |c: usize| (t * inverse + t * inverse) * curve[1][c] + inverse * curve[0][c] * inverse + t * t * curve[2][c];
        return [f(0), f(1)];
    }
    let w = [inverse * inverse * inverse, inverse * inverse * t * 3.0, t * t * inverse * 3.0, t * t * t];
    let f = |c: usize| ((w[0] * curve[0][c] + w[1] * curve[1][c]) + w[2] * curve[2][c]) + w[3] * curve[3][c];
    [f(0), f(1)]
}

fn elevate_quadratic(quadratic: &[Point]) -> Vec<Point> {
    let mut coefficients = [[0.0; 2]; 4];
    for power in 0..3 {
        for channel in 0..2 {
            let mut sum = 0.0;
            for term in 0..=power { sum += QUADRATIC_DIFFERENCES[power][term] * quadratic[term][channel]; }
            coefficients[power][channel] = sum * if power == 1 { 2.0 } else { 1.0 };
        }
    }
    let mut curve: Vec<Point> = Vec::with_capacity(4);
    for point in 0..4 {
        let scale = 1.0 / CUBIC_BINOMIAL[point];
        let mut value = [coefficients[point][0] * scale, coefficients[point][1] * scale];
        for term in 0..point {
            for channel in 0..2 { value[channel] -= CUBIC_DIFFERENCES[point][term] * curve[term][channel]; }
        }
        curve.push(value);
    }
    curve
}

pub fn fit_contour_interval(nodes: &mut [FitNode], contour: &[usize], start: i64, end: i64, mark_interior: bool) -> IntervalFit {
    let len = contour.len() as i64;
    let mut e = end;
    while e < start { e += len; }
    let pt = |nodes: &[FitNode], s: i64| { let n = &nodes[at(contour, s)]; [n.x, n.y] };
    let mut curve = vec![pt(nodes, start), pt(nodes, start + 1), pt(nodes, e + len - 1), pt(nodes, e)];
    let samples = prepare_fitting_samples(nodes, contour, start, end, mark_interior);
    let count = samples.points.len();
    if count == 0 { return IntervalFit { cost: 0.0, curve }; }
    if count <= 2 {
        let (first, last) = (curve[0], curve[3]);
        let interpolate = |t: f64| [t * last[0] + (1.0 - t) * first[0], (1.0 - t) * first[1] + t * last[1]];
        curve[1] = interpolate(0.25); curve[2] = interpolate(0.75);
        let point = interpolate(samples.parameters[0]);
        let dx = point[0] - samples.points[0][0]; let dy = point[1] - samples.points[0][1];
        return IntervalFit { cost: dy * dy + dx * dx, curve };
    }
    let fitted = fit_interior_controls(curve[0], curve[3], &samples, count == 3);
    let mut cost = 0.0;
    for sample in 0..count {
        let point = evaluate_curve(&fitted, samples.parameters[sample]);
        let dx = samples.points[sample][0] - point[0]; let dy = samples.points[sample][1] - point[1];
        cost = (dy * dy + (dx * dx + 0.0)) + cost;
    }
    let curve = if count == 3 { elevate_quadratic(&fitted) } else { fitted };
    IntervalFit { cost, curve }
}

/// Original keeper partition before shared-node marking and cubic allocation.
#[allow(clippy::too_many_arguments)]
pub fn partition_contour_interval(nodes: &mut [FitNode], contour: &[usize], start: i64, end: i64,
    initial_threshold: f64, final_threshold: f64, max_iterations: i64, fraction: f64) -> Result<KeeperPartition, String> {
    let mut end = end;
    while end < start { end += contour.len() as i64; }
    let slots = end - start + 1;
    let mut records: Vec<KeeperRecord> = (0..slots).map(|slot| KeeperRecord { slot, start: slot, count: 1,
        next: if slot + 1 < slots { slot + 1 } else { -1 }, cost: 0.0, dirty: true }).collect();
    let mut evaluate = |nodes: &mut [FitNode], r: &KeeperRecord, fd: i64, ld: i64| {
        fit_contour_interval(nodes, contour, start + r.start + fd, start + r.start + r.count + ld, false).cost
    };
    fn cached(nodes: &mut [FitNode], r: &mut KeeperRecord, ev: &mut dyn FnMut(&mut [FitNode], &KeeperRecord, i64, i64) -> f64) -> f64 {
        if r.dirty { r.cost = ev(nodes, r, 0, 0); r.dirty = false; }
        r.cost
    }
    fn redistributed(nodes: &mut [FitNode], r: &mut KeeperRecord, fd: i64, ld: i64, ev: &mut dyn FnMut(&mut [FitNode], &KeeperRecord, i64, i64) -> f64) -> f64 {
        let previous = cached(nodes, r, ev); let changed = ev(nodes, r, fd, ld);
        let improvement = previous - changed;
        if ld != fd && ld - fd >= 0 { -improvement } else { improvement }
    }
    let mut threshold = initial_threshold;
    if max_iterations > 0 {
        let multiplier = fitting_threshold_multiplier(initial_threshold, final_threshold, max_iterations as f64, fraction);
        for iteration in 0..max_iterations {
            let mut slot = 0usize;
            while records[slot].next != -1 && records[records[slot].next as usize].next != -1 {
                let si = records[slot].next as usize;
                let (a, b) = records.split_at_mut(si);
                let first = &mut a[slot]; let second = &mut b[0];
                let first_cost = cached(nodes, first, &mut evaluate);
                let second_cost = cached(nodes, second, &mut evaluate);
                let merged = evaluate(nodes, first, 0, second.count);
                if merged - (second_cost + first_cost) < threshold {
                    first.count += second.count; first.next = second.next; first.dirty = true;
                } else if redistributed(nodes, second, -1, 0, &mut evaluate) - redistributed(nodes, first, 0, -1, &mut evaluate) < 0.0 {
                    second.count += 1; first.count -= 1; second.start -= 1;
                    second.dirty = true; first.dirty = true;
                    if second.start <= first.start { return Err("Original keeper partition crossed its left boundary.".into()); }
                } else if redistributed(nodes, first, 0, 1, &mut evaluate) - redistributed(nodes, second, 1, 0, &mut evaluate) < 0.0 {
                    first.count += 1; second.count -= 1; second.start += 1;
                    first.dirty = true; second.dirty = true;
                    let (sn, ss) = (second.next, second.start);
                    if (sn == -1 || records[sn as usize].start <= ss) && slots <= ss {
                        return Err("Original keeper partition crossed its right boundary.".into());
                    }
                }
                let next = records[slot].next;
                if next == -1 { break; }
                slot = next as usize;
            }
            threshold = if (max_iterations as f64) * fraction <= iteration as f64 { final_threshold } else { multiplier * threshold };
        }
    }
    let mut active = Vec::new();
    let mut slot = 0i64;
    while slot != -1 { active.push(records[slot as usize].clone()); slot = records[slot as usize].next; }
    Ok(KeeperPartition { records: active, threshold })
}
