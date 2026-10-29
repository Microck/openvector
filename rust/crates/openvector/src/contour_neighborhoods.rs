//! Port of `src/contour-neighborhoods.mjs`: original sliding-midpoint tree and
//! capped nearest search over contour samples (shared-node duplicates included).
use crate::contours::Geometry;
use crate::pixel_input::{fail, InputError};

type Point = [f64; 2];

fn coordinate_range(points: &[Point], order: &[usize], start: usize, count: usize, axis: usize) -> (f64, f64) {
    let mut minimum = points[order[start]][axis];
    let mut maximum = minimum;
    for index in start + 1..start + count {
        let value = points[order[index]][axis];
        if minimum <= value { if maximum < value { maximum = value; } } else { minimum = value; }
    }
    (minimum, maximum)
}

fn partition(points: &[Point], order: &mut [usize], start: usize, count: usize, axis: usize, cut: f64) -> (usize, usize) {
    let count = count as i64;
    let coord = |order: &[usize], i: i64| points[order[start + i as usize]][axis];
    let (mut left, mut right) = (0i64, count - 1);
    loop {
        while left < count && coord(order, left) < cut { left += 1; }
        while right >= 0 && coord(order, right) >= cut { right -= 1; }
        if right < left { break; }
        order.swap(start + left as usize, start + right as usize);
        left += 1; right -= 1;
    }
    let below = left;
    right = count - 1;
    loop {
        while left < count && coord(order, left) <= cut { left += 1; }
        while right >= below && coord(order, right) > cut { right -= 1; }
        if right < left { return (below as usize, left as usize); }
        order.swap(start + left as usize, start + right as usize);
        left += 1; right -= 1;
    }
}

enum Tree {
    Leaf(usize),
    Split { axis: usize, cut: f64, lower: f64, upper: f64, left: Box<Tree>, right: Box<Tree> },
}

fn build_tree(points: &[Point], order: &mut [usize], start: usize, count: usize, lower: &mut [f64; 2], upper: &mut [f64; 2]) -> Tree {
    if count == 1 { return Tree::Leaf(order[start]); }
    let widest = (upper[0] - lower[0]).max(upper[1] - lower[1]);
    let (mut axis, mut spread, mut selected) = (0usize, -1.0f64, (0.0, 0.0));
    for candidate in 0..2 {
        if widest * 0.999 <= upper[candidate] - lower[candidate] {
            let (mn, mx) = coordinate_range(points, order, start, count, candidate);
            if spread < mx - mn { spread = mx - mn; axis = candidate; selected = (mn, mx); }
        }
    }
    let middle = (lower[axis] + upper[axis]) * 0.5;
    let (minimum, maximum) = selected;
    let mut cut = minimum;
    if minimum <= middle { cut = if maximum < middle { maximum } else { middle }; }
    let (below, equal_end) = partition(points, order, start, count, axis, cut);
    let left_count = if middle < minimum { 1 } else if maximum < middle { count - 1 } else { below.max((count / 2).min(equal_end)) };
    let (old_lower, old_upper) = (lower[axis], upper[axis]);
    upper[axis] = cut;
    let left = build_tree(points, order, start, left_count, lower, upper);
    upper[axis] = old_upper;
    lower[axis] = cut;
    let right = build_tree(points, order, start + left_count, count - left_count, lower, upper);
    lower[axis] = old_lower;
    Tree::Split { axis, cut, lower: old_lower, upper: old_upper, left: Box::new(left), right: Box::new(right) }
}

struct Search<'a> { points: &'a [Point], query: Point, capacity: usize, nearest: Vec<(usize, f64)> }

impl Search<'_> {
    fn maximum(&self) -> f64 {
        if self.nearest.len() == self.capacity { self.nearest[self.capacity - 1].1 } else { f64::MAX }
    }
    fn visit(&mut self, branch: &Tree, box_distance: f64) {
        const APPROXIMATION: f64 = (1.0 + 1e-12) * (1.0 + 1e-12);
        match branch {
            Tree::Leaf(sample) => {
                let point = self.points[*sample];
                let dx = self.query[0] - point[0];
                let mut distance = dx * dx + 0.0;
                if self.maximum() < distance { return; }
                let dy = self.query[1] - point[1];
                distance = dy * dy + distance;
                if self.maximum() < distance { return; }
                let mut index = self.nearest.len();
                while index > 0 && self.nearest[index - 1].1 > distance { index -= 1; }
                self.nearest.insert(index, (*sample, distance));
                if self.nearest.len() > self.capacity { self.nearest.pop(); }
            }
            Tree::Split { axis, cut, lower, upper, left, right } => {
                let q = self.query[*axis];
                let difference = q - cut;
                let (near, far, outside) = if difference >= 0.0 { (right, left, (q - upper).max(0.0)) } else { (left, right, (lower - q).max(0.0)) };
                self.visit(near, box_distance);
                let other = (difference * difference - outside * outside) + box_distance;
                if APPROXIMATION * other < self.maximum() { self.visit(far, other); }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Neighborhoods { pub nearby_edges: Vec<Vec<[usize; 2]>>, pub flags: Vec<u8> }

/// Build the original fixed contour neighborhoods before moving shared nodes.
pub fn prepare_contour_neighborhoods(geometry: &Geometry) -> Result<Neighborhoods, InputError> {
    if geometry.nodes.is_empty() || geometry.regions.is_empty() {
        return fail("Contour neighborhoods need shared nodes and region contours.");
    }
    if geometry.nodes.iter().any(|n| !n.x.is_finite() || !n.y.is_finite()) {
        return fail("Contour neighborhoods need finite node coordinates and byte flags.");
    }
    let mut flags: Vec<u8> = geometry.nodes.iter().map(|n| n.flags).collect();
    let mut points = Vec::new();
    let mut samples = Vec::new();
    for (region, entry) in geometry.regions.iter().enumerate() {
        if entry.contour.len() < 3 { return fail("Contour neighborhoods need nonempty region contours."); }
        for (sample, &node) in entry.contour.iter().enumerate() {
            if node >= geometry.nodes.len() { return fail("Contour neighborhoods must identify existing nodes."); }
            points.push([geometry.nodes[node].x, geometry.nodes[node].y]);
            samples.push((region, sample, node));
        }
    }
    let capacity = geometry.nodes.len().min(50);
    if points.len() < capacity { return fail("Contour neighborhoods need at least as many samples as requested neighbors."); }
    let mut order: Vec<usize> = (0..points.len()).collect();
    let (mut lower, mut upper) = ([0.0; 2], [0.0; 2]);
    for axis in 0..2 { (lower[axis], upper[axis]) = coordinate_range(&points, &order, 0, points.len(), axis); }
    let tree = build_tree(&points, &mut order, 0, points.len(), &mut lower, &mut upper);
    let mut nearby_edges = vec![Vec::new(); geometry.nodes.len()];
    for entry in &geometry.regions {
        for &node in &entry.contour {
            if flags[node] & 1 != 0 { continue; }
            let mut search = Search { points: &points, query: [geometry.nodes[node].x, geometry.nodes[node].y], capacity, nearest: Vec::new() };
            search.visit(&tree, 0.0);
            for (sample, distance) in search.nearest {
                let (other_region, other_sample, other_node) = samples[sample];
                if distance < 25.0 && other_node != node { nearby_edges[node].push([other_region, other_sample]); }
            }
            flags[node] |= 1;
        }
    }
    Ok(Neighborhoods { nearby_edges, flags })
}
