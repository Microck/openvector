//! Port of `src/smoothing-repair.mjs`. Trusted prepared-state kernel: the
//! complete smoother owns geometry, points and byte penalties; nearby
//! associations stay fixed while coordinates move.
use crate::contours::Geometry;
use serde_json::{json, Value};

type P = [f64; 2];

fn unit_direction(mut x: f64, mut y: f64) -> P {
    let length = (y * y + x * x).sqrt();
    if length > 0.0 { let inverse = 1.0 / length; x *= inverse; y = inverse * y; }
    [x, y]
}

fn joint_direction(previous: P, current: P, following: P) -> P {
    let incoming = unit_direction(previous[0] - current[0], previous[1] - current[1]);
    let outgoing = unit_direction(following[0] - current[0], following[1] - current[1]);
    let mut x = incoming[1] + -outgoing[1]; let mut y = -incoming[0] + outgoing[0];
    let sum_x = outgoing[0] + incoming[0]; let sum_y = outgoing[1] + incoming[1];
    if y * y + x * x < sum_y * sum_y + sum_x * sum_x {
        x = sum_x; y = sum_y;
        if outgoing[1] * incoming[0] - incoming[1] * outgoing[0] > 0.0 { x = -x; y = -y; }
    }
    unit_direction(x, y)
}

fn winding_contribution(origin: P, direction: P, first: P, last: P) -> i64 {
    let denominator = (last[1] - first[1]) * direction[0] - (last[0] - first[0]) * direction[1];
    if denominator != 0.0 {
        if ((last[1] - origin[1]) * direction[0] - (last[0] - origin[0]) * direction[1]).abs() < 0.0001 ||
            ((first[1] - origin[1]) * direction[0] - (first[0] - origin[0]) * direction[1]).abs() < 0.0001 {
            return if denominator <= 0.0 { -1 } else { 1 };
        }
        let along_ray = (last[0] - first[0]) * (origin[1] - first[1]) - (last[1] - first[1]) * (origin[0] - first[0]);
        let along_edge = (origin[1] - first[1]) * direction[0] - (origin[0] - first[0]) * direction[1];
        if denominator <= 0.0 {
            if along_ray <= 0.0 && along_edge < 0.0 && denominator < along_edge { return -2; }
        } else if along_ray >= 0.0 && along_edge > 0.0 && along_edge < denominator { return 2; }
    }
    0
}

fn on_segment(point: P, first: P, last: P, direction: P) -> bool {
    let dx = point[0] - first[0]; let dy = point[1] - first[1];
    if (dy * direction[0] - dx * direction[1]).abs() >= 0.0001 { return false; }
    let projection = dy * direction[1] + dx * direction[0];
    let outside = projection < 0.0 || ((last[1] - first[1]) * (last[1] - first[1]) + (last[0] - first[0]) * (last[0] - first[0])).sqrt() < projection;
    !(outside && dy * dy + dx * dx >= 1e-8 &&
        (point[1] - last[1]) * (point[1] - last[1]) + (point[0] - last[0]) * (point[0] - last[0]) >= 1e-8)
}

fn crossing(first: P, last: P, other_first: P, other_last: P) -> i64 {
    let mut denominator = (other_last[1] - other_first[1]) * (last[0] - first[0]) - (other_last[0] - other_first[0]) * (last[1] - first[1]);
    if denominator == 0.0 { return 0; }
    let sign = if denominator >= 0.0 { 1 } else { -1 };
    let direction = unit_direction(last[0] - first[0], last[1] - first[1]);
    let other_direction = unit_direction(other_last[0] - other_first[0], other_last[1] - other_first[1]);
    if on_segment(first, other_first, other_last, other_direction) || on_segment(last, other_first, other_last, other_direction) ||
        on_segment(other_first, first, last, direction) || on_segment(other_last, first, last, direction) { return sign; }
    let mut first_numerator = (other_last[0] - other_first[0]) * (first[1] - other_first[1]) - (other_last[1] - other_first[1]) * (first[0] - other_first[0]);
    let mut last_numerator = (last[0] - first[0]) * (first[1] - other_first[1]) - (last[1] - first[1]) * (first[0] - other_first[0]);
    if denominator < 0.0 { denominator = -denominator; first_numerator = -first_numerator; last_numerator = -last_numerator; }
    if first_numerator > 0.0 && first_numerator < denominator && last_numerator > 0.0 && last_numerator < denominator { sign * 2 } else { 0 }
}

/// Adjacent edges meet at one node; native 0x4811a0 compares the four
/// normalized rays around that joint.
fn adjacent_crossing(first: P, following: P, other_first: P, other_following: P, joint: P) -> i64 {
    let u = |p: P| unit_direction(p[0] - joint[0], p[1] - joint[1]);
    crossing(u(first), u(following), u(other_first), u(other_following))
}

/// Mutable view of the smoother's shared state.
pub struct RepairView<'a> { pub geometry: &'a mut Geometry, pub penalty_counts: &'a mut [u8], pub points: &'a [P] }

#[derive(Clone, Debug, PartialEq)]
pub struct RepairNodeState { pub flags: Vec<u8>, pub penalty_counts: Vec<u8> }

#[derive(Clone, Debug)]
pub struct ContourRepair { nearby_edges: Vec<Vec<[usize; 2]>>, probe_directions: Vec<P>, windings: Vec<i32> }

impl ContourRepair {
    pub fn new(region_count: usize, nearby_edges: Vec<Vec<[usize; 2]>>) -> Self {
        ContourRepair { nearby_edges, probe_directions: vec![[0.0, 0.0]; region_count], windings: vec![0; region_count] }
    }

    fn seed_winding(&mut self, s: &RepairView, region_id: usize, sample: usize) -> i32 {
        let contour = &s.geometry.regions[region_id].contour; let count = contour.len(); let points = s.points;
        let direction = joint_direction(points[contour[(sample + count - 1) % count]], points[contour[sample]], points[contour[(sample + 1) % count]]);
        self.probe_directions[region_id] = direction;
        let mut sum = 0i64;
        for edge in sample + 2..sample + count {
            sum += winding_contribution(points[contour[sample]], direction, points[contour[(edge - 1) % count]], points[contour[edge % count]]);
        }
        if sum % 2 != 0 { sum = 0; }
        (2 - sum) as i32
    }

    fn mark(s: &mut RepairView, node: usize) -> i64 {
        let flags = &mut s.geometry.nodes[node].flags;
        let changed = if *flags & 2 == 0 { 1 } else { 0 };
        *flags |= 0x16;
        s.penalty_counts[node] = 3;
        changed
    }

    pub fn region(&mut self, s: &mut RepairView, region_id: usize) -> i64 {
        self.windings[region_id] = self.seed_winding(s, region_id, 0);
        let contour = s.geometry.regions[region_id].contour.clone(); let count = contour.len(); let points = s.points;
        for &node in &contour { s.geometry.nodes[node].flags &= 0xf0; }
        let (mut changed, mut accumulated, mut consecutive) = (0i64, 0i64, 0i64);
        for sample in 0..count {
            let first = contour[sample]; let last = contour[(sample + 1) % count];
            let (mut signed, mut absolute) = (0i64, 0i64);
            for &[other_id, other_sample] in &self.nearby_edges[first] {
                let other = &s.geometry.regions[other_id].contour; let oc = other.len();
                let other_first = other[other_sample]; let other_last = other[(other_sample + 1) % oc];
                if last == other_last && region_id == other_id {
                    let c = adjacent_crossing(points[first], points[contour[(sample + 2) % count]],
                        points[other_first], points[other[(other_sample + 2) % oc]], points[last]);
                    absolute += c.abs(); signed += c;
                } else if first != other_first && first != other_last && last != other_first && last != other_last {
                    let c = crossing(points[first], points[last], points[other_first], points[other_last]);
                    absolute += c.abs();
                    if region_id == other_id { signed += c; }
                }
            }
            accumulated += signed;
            if absolute == 0 {
                if self.windings[region_id] as i64 + accumulated == 0 {
                    s.geometry.nodes[last].flags |= 4;
                    consecutive = 0;
                } else {
                    changed += Self::mark(s, last);
                    consecutive += 1;
                    if consecutive > 20 {
                        self.windings[region_id] = self.seed_winding(s, region_id, sample);
                        accumulated = 0;
                        consecutive = -2147483647;
                    }
                }
            } else {
                changed += Self::mark(s, first);
                changed += Self::mark(s, last);
            }
        }
        changed
    }

    pub fn decay(&mut self, s: &mut RepairView) -> i64 {
        let mut changed = 0;
        for count in s.penalty_counts.iter_mut() { if *count > 1 { *count -= 1; changed += 1; } }
        changed
    }

    pub fn node_state(s: &RepairView) -> RepairNodeState {
        RepairNodeState { flags: s.geometry.nodes.iter().map(|n| n.flags).collect(), penalty_counts: s.penalty_counts.to_vec() }
    }

    pub fn invalid(&mut self, s: &mut RepairView, identifiers: &[i32]) -> bool {
        let mut changed = 0;
        for &id in identifiers { changed += self.region(s, id as usize); }
        changed > 0
    }

    pub fn all(&mut self, s: &mut RepairView) -> i64 {
        let mut changed = self.decay(s);
        for id in 0..s.geometry.regions.len() { changed += self.region(s, id); }
        changed
    }

    /// `{flags, penaltyCounts, probeDirections, windings}` as in the JS snapshot.
    pub fn snapshot(&self, s: &RepairView) -> Value {
        let n = Self::node_state(s);
        json!({"flags": n.flags, "penaltyCounts": n.penalty_counts,
            "probeDirections": self.probe_directions.iter().map(|d| d.to_vec()).collect::<Vec<_>>(), "windings": self.windings})
    }
}
