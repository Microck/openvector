//! Copyright (C) 2009 Nokia Corporation and/or its subsidiary(-ies).
//! Copyright (C) 2026 OpenVector contributors.
//!
//! Rust translation of `src/vendor/qt-bezier/square-stroke.mjs`, itself a
//! modified translation of selected Qt 4.5.3 QBezier and QStroker routines.
//! This file is licensed under LGPL-2.1. See `vendor/qt-bezier/license-lgpl.txt`
//! and `vendor/qt-bezier/modifications.txt`. The surrounding crate is MIT.

use super::PathCommand;

type Point = [f64; 2];
type Curve = [Point; 4];

fn equal(a: Point, b: Point) -> bool { a[0] == b[0] && a[1] == b[1] }
fn fuzzy(a: f64, b: f64) -> bool { (a - b).abs() <= 1e-12 * a.abs().min(b.abs()) }
fn add(a: Point, b: Point) -> Point { [a[0] + b[0], a[1] + b[1]] }
fn subtract(a: Point, b: Point) -> Point { [a[0] - b[0], a[1] - b[1]] }
fn multiply(a: Point, k: f64) -> Point { [a[0] * k, a[1] * k] }
fn length(a: Point) -> f64 { (a[0] * a[0] + a[1] * a[1]).sqrt() }
fn unit(a: Point) -> Point {
    let size = length(a);
    if size != 0.0 && !size.is_nan() { multiply(a, 1.0 / size) } else { [0.0, 0.0] }
}
fn normal(a: Point, b: Point) -> Point { unit([b[1] - a[1], a[0] - b[0]]) }
fn midpoint(a: Point, b: Point) -> Point { multiply(add(a, b), 0.5) }

fn point_at(curve: &Curve, t: f64) -> Point {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, u * u * (3.0 * t), t * t * (3.0 * u), t * t * t);
    [a * curve[0][0] + b * curve[1][0] + c * curve[2][0] + d * curve[3][0],
        a * curve[0][1] + b * curve[1][1] + c * curve[2][1] + d * curve[3][1]]
}

fn normal_at(curve: &Curve, t: f64) -> Point {
    let u = 1.0 - t;
    let (a, b, c) = (u * u, t * u, t * t);
    [(curve[1][1] - curve[0][1]) * a + (curve[2][1] - curve[1][1]) * b + (curve[3][1] - curve[2][1]) * c,
        -(curve[1][0] - curve[0][0]) * a - (curve[2][0] - curve[1][0]) * b - (curve[3][0] - curve[2][0]) * c]
}

fn split(curve: &Curve) -> (Curve, Curve) {
    let middle = midpoint(curve[1], curve[2]);
    let left1 = midpoint(curve[0], curve[1]);
    let right2 = midpoint(curve[2], curve[3]);
    let left2 = midpoint(left1, middle);
    let right1 = midpoint(right2, middle);
    let center = midpoint(left2, right1);
    ([curve[0], left1, left2, center], [center, right1, right2, curve[3]])
}

fn acceptable_offset(original: &Curve, shifted: &Curve, threshold: f64) -> bool {
    for t in [0.25, 0.5, 0.75] {
        let (p, q) = (point_at(original, t), point_at(shifted, t));
        let delta = subtract(p, q);
        if (delta[0] * delta[0] + delta[1] * delta[1] - 0.25).abs() > threshold * 0.25 { return false; }
        let n = normal_at(original, t);
        let size = n[0].abs() + n[1].abs();
        if size != 0.0 && !size.is_nan() && (n[0] * delta[1] - n[1] * delta[0]).abs() / size > threshold * 0.5 {
            return false;
        }
    }
    true
}

enum Shift { Discard, Ok(Curve), Split(Curve), Circle }

fn shift_curve(original: &Curve, threshold: f64) -> Shift {
    let mut points = vec![original[0]];
    let mut indices = [0usize; 4];
    for i in 1..4 {
        if !equal(original[i], original[i - 1]) { points.push(original[i]); }
        indices[i] = points.len() - 1;
    }
    if points.len() == 1 { return Shift::Discard; }
    let last = points[points.len() - 1];
    if points.len() > 2 {
        let d1 = subtract(points[1], points[0]);
        let d2 = subtract(points[2], points[0]);
        let d3 = subtract(last, points[0]);
        if fuzzy(d1[0] * d2[1], d2[0] * d1[1]) && (points.len() == 3 || fuzzy(d1[0] * d3[1], d3[0] * d1[1])) {
            if equal(points[0], last) { return Shift::Discard; }
            let offset = multiply(normal(points[0], last), 0.5);
            let start = add(points[0], offset);
            let end = add(last, offset);
            let delta = subtract(end, start);
            return Shift::Ok([start, add(start, multiply(delta, 0.33)), add(start, multiply(delta, 0.66)), end]);
        }
    }
    let [p0, p1, p2, p3] = *original;
    let spread = |axis: usize| {
        let values = original.map(|p| p[axis]);
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max) - values.iter().copied().fold(f64::INFINITY, f64::min)
    };
    if points.len() == 4 && spread(0) < 0.05 && spread(1) < 0.05 {
        // Preserve the upstream expression, including its asymmetric y term.
        let measure = (p0[0] - p1[0]) * (p0[0] - p1[0]) + (p0[1] - p1[1]) * (p0[1] - p0[1])
            * (p2[0] - p3[0]) * (p2[0] - p3[0]) + (p2[1] - p3[1]) * (p2[1] - p3[1]);
        let dot = (p0[0] - p1[0]) * (p2[0] - p3[0]) + (p0[1] - p1[1]) * (p2[1] - p3[1]);
        if dot < 0.0 && dot * dot < 0.8 * measure { return Shift::Circle; }
    }
    let mut previous = normal(points[0], points[1]);
    let mut shifted = vec![add(points[0], multiply(previous, 0.5))];
    for i in 1..points.len() - 1 {
        let next = normal(points[i], points[i + 1]);
        let r = 1.0 + previous[0] * next[0] + previous[1] * next[1];
        shifted.push(add(points[i], if fuzzy(r + 1.0, 1.0) { multiply(previous, 0.5) } else {
            multiply(add(previous, next), 0.5 / r)
        }));
        previous = next;
    }
    shifted.push(add(last, multiply(previous, 0.5)));
    let curve = indices.map(|index| shifted[index]);
    if acceptable_offset(original, &curve, threshold) { Shift::Ok(curve) } else { Shift::Split(curve) }
}

fn circle_curves(curve: &Curve) -> Vec<Curve> {
    let [a, b, c, d] = *curve;
    let first = normal(a, b);
    let last = normal(c, d);
    if fuzzy(length(subtract(b, a)) + 1.0, 1.0) || fuzzy(length(subtract(d, c)) + 1.0, 1.0) { return Vec::new(); }
    let mut middle = multiply(unit(add(subtract(a, b), subtract(d, c))), -1.0);
    let mut sign = 1.0;
    let mut angles = [first[0] * middle[0] + first[1] * middle[1], middle[0] * last[0] + middle[1] * last[1]]
        .map(|cosine: f64| cosine.clamp(-1.0, 1.0).acos() / std::f64::consts::PI);
    if angles[0] + angles[1] > 1.0 {
        middle = multiply(middle, -1.0);
        angles = angles.map(|angle| 1.0 - angle);
        sign = -1.0;
    }
    let normals = [first, middle, last];
    let points = [add(a, multiply(first, 0.5)), add(midpoint(a, d), multiply(middle, 0.5)), add(d, multiply(last, 0.5))];
    (0..2).map(|i| {
        let k = 2.0 * 0.5522847498 * sign * 0.5 * angles[i];
        let (n, m) = (normals[i], normals[i + 1]);
        [points[i], add(points[i], [-n[1] * k, n[0] * k]), add(points[i + 1], [m[1] * k, -m[0] * k]), points[i + 1]]
    }).collect()
}

fn offset_curves(curve: &Curve) -> Vec<Curve> {
    if curve.iter().all(|&p| equal(p, curve[0])) { return Vec::new(); }
    let mut threshold: f64 = 0.25;
    // Match the bounded stack and float threshold used by the reference. The
    // retry relaxes geometric approximation, independently of the pixel pool.
    'retry: loop {
        let mut pending = vec![*curve];
        let mut output: Vec<Curve> = Vec::new();
        while !pending.is_empty() {
            if pending.len() == 10 || output.len() + pending.len() == 15 {
                threshold = (threshold * 1.5) as f32 as f64;
                if threshold <= 2.0 { continue 'retry; }
                while let Some(current) = pending.pop() {
                    if let Shift::Ok(shifted) | Shift::Split(shifted) = shift_curve(&current, threshold) {
                        output.push(shifted);
                    }
                }
                return output;
            }
            let current = pending.pop().unwrap();
            match shift_curve(&current, threshold) {
                Shift::Ok(shifted) => output.push(shifted),
                Shift::Circle if 15 - output.len() as i64 >= 2 => output.extend(circle_curves(&current)),
                Shift::Discard => {}
                _ => {
                    let (left, right) = split(&current);
                    pending.push(right);
                    pending.push(left);
                }
            }
        }
        return output;
    }
}

struct Side { start: Point, tangent: Point, commands: Vec<PathCommand>, before_end: Point, end: Point }

fn stroke_side(points: &[Point], cubic: bool) -> Option<Side> {
    if !cubic {
        if equal(points[0], points[1]) { return None; }
        let n = multiply(normal(points[0], points[1]), 0.5);
        let (start, end) = (add(points[0], n), add(points[1], n));
        return Some(Side { start, tangent: subtract(points[1], points[0]), commands: vec![PathCommand::L(end)],
            before_end: start, end });
    }
    let curves = offset_curves(&[points[0], points[1], points[2], points[3]]);
    let (first, last) = (curves.first()?, curves.last()?);
    let tangent = subtract(*points[1..].iter().find(|&&p| !equal(p, points[0]))?, points[0]);
    // A repeated final control point must not erase the cap direction.
    let before_end = if !equal(last[2], last[3]) { last[2] } else if !equal(last[1], last[3]) { last[1] } else { last[0] };
    Some(Side {
        start: first[0], tangent,
        commands: curves.iter().map(|c| PathCommand::C([c[1][0], c[1][1], c[2][0], c[2][1], c[3][0], c[3][1]])).collect(),
        before_end, end: last[3],
    })
}

fn square_cap(from: &Side, to: &Side) -> [PathCommand; 3] {
    let previous = multiply(unit(subtract(from.end, from.before_end)), 0.5);
    let following = multiply(unit(to.tangent), -0.5);
    [PathCommand::L(add(from.end, previous)), PathCommand::L(add(to.start, following)), PathCommand::L(to.start)]
}

/// One device-space M/L or M/C edge, as used by the application's seam pass.
/// Input geometry has already been validated and mapped by the caller.
pub fn square_stroke(start: Point, edge: &PathCommand) -> Vec<PathCommand> {
    let (points, cubic) = match *edge {
        PathCommand::L(end) => (vec![start, end], false),
        PathCommand::C(c) => (vec![start, [c[0], c[1]], [c[2], c[3]], [c[4], c[5]]], true),
        _ => unreachable!("validated cosmetic strokes end in one L or C edge"),
    };
    let reversed: Vec<Point> = points.iter().rev().copied().collect();
    let (Some(forward), Some(backward)) = (stroke_side(&points, cubic), stroke_side(&reversed, cubic)) else {
        return Vec::new();
    };
    let mut commands = vec![PathCommand::M(forward.start)];
    // Equal endpoints close a subpath even without an explicit Z command.
    if equal(points[0], points[points.len() - 1]) {
        commands.extend(forward.commands);
        commands.extend([PathCommand::Z, PathCommand::M(backward.start)]);
        commands.extend(backward.commands);
        commands.push(PathCommand::Z);
        return commands;
    }
    let forward_cap = square_cap(&forward, &backward);
    let backward_cap = square_cap(&backward, &forward);
    commands.extend(forward.commands);
    commands.extend(forward_cap);
    commands.extend(backward.commands);
    commands.extend(backward_cap);
    commands.push(PathCommand::Z);
    commands
}
