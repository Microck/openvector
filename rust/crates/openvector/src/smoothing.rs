//! Port of `src/smoothing.mjs`: priors, prepared objective, quadratic and
//! seeing/blind line searches and the Fletcher-Reeves controller.
use crate::contours::Geometry;
use crate::image_measurement::ContourImageModel;
use crate::image_measurement_kernel::ImageKernel;
use crate::smoothing_repair::{ContourRepair, RepairNodeState, RepairView};
use serde_json::{json, Value};

pub use crate::contour_corners::puncture_contour_corners;
pub use crate::contour_initialization::{preadjust_contours, prepare_contour_smoothing, select_near_border_nodes};
pub use crate::contour_neighborhoods::prepare_contour_neighborhoods;

type P = [f64; 2];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriorTerms { pub energy: f64, pub first_gradient: P, pub last_gradient: P }

fn read_prior_geometry(points: &[Vec<f64>], sample_counts: &[f64]) -> Result<([P; 3], [f64; 2]), String> {
    if points.len() != 3 { return Err("Smoothing prior needs three coordinate pairs.".into()); }
    for p in points {
        if p.len() != 2 || !p[0].is_finite() || !p[1].is_finite() {
            return Err("Smoothing coordinates must be finite double x/y pairs.".into());
        }
    }
    let ok = |c: f64| c.is_finite() && c.fract() == 0.0 && (1.0..=2147483647.0).contains(&c);
    if sample_counts.len() != 2 || !ok(sample_counts[0]) || !ok(sample_counts[1]) {
        return Err("Smoothing prior needs two positive int32 sample counts.".into());
    }
    Ok(([[points[0][0], points[0][1]], [points[1][0], points[1][1]], [points[2][0], points[2][1]]], [sample_counts[0], sample_counts[1]]))
}

/// Original type-3 prior before phase strength or total-objective assembly.
pub fn curvature_prior(points: &[Vec<f64>], sample_counts: &[f64], length_weight: f64) -> Result<PriorTerms, String> {
    let (p, c) = read_prior_geometry(points, sample_counts)?;
    if !length_weight.is_finite() || length_weight < 0.0 { return Err("Curvature lengthWeight must be finite and nonnegative.".into()); }
    let (energy, g) = curvature_terms(p[0], p[1], p[2], c, length_weight, true);
    let g = g.unwrap();
    Ok(PriorTerms { energy, first_gradient: g.0, last_gradient: g.1 })
}

/// Original quadratic fallback for small regions and flagged inversion penalties.
pub fn quadratic_prior(points: &[Vec<f64>], sample_counts: &[f64]) -> Result<PriorTerms, String> {
    let (p, c) = read_prior_geometry(points, sample_counts)?;
    let (energy, g) = quadratic_terms(p[0], p[1], p[2], c, true);
    let g = g.unwrap();
    Ok(PriorTerms { energy, first_gradient: g.0, last_gradient: g.1 })
}

type Terms = (f64, Option<(P, P)>);

/// Adjacent curvature samples share an edge and can reuse its length and direction.
#[derive(Clone, Copy)]
struct Edge { length: f64, inverse: f64, ux: f64, uy: f64 }

fn edge(first: P, middle: P) -> Edge {
    let x = middle[0] - first[0]; let y = middle[1] - first[1];
    let length = (x * x + y * y).sqrt();
    let inverse = 1.0 / length;
    Edge { length, inverse, ux: x * inverse, uy: y * inverse }
}

fn curvature_terms(first: P, middle: P, last: P, counts: [f64; 2], length_weight: f64, include_gradient: bool) -> Terms {
    curvature_terms_cached(middle, last, counts, length_weight, include_gradient, edge(first, middle)).0
}

/// Returns the outgoing edge for use as the next sample's incoming edge.
fn curvature_terms_cached(middle: P, last: P, counts: [f64; 2], length_weight: f64, include_gradient: bool, incoming: Edge) -> (Terms, Edge) {
    let outgoing = edge(middle, last);
    let (incoming_length, incoming_inverse, iux, iuy) = (incoming.length, incoming.inverse, incoming.ux, incoming.uy);
    let (outgoing_length, outgoing_inverse, oux, ouy) = (outgoing.length, outgoing.inverse, outgoing.ux, outgoing.uy);
    let dot = oux * iux + ouy * iuy;
    let curvature = ((2.0 - (dot + dot)) + 0.001).sqrt();
    let length_difference = outgoing_length / counts[1] - incoming_length / counts[0];
    let energy = (length_difference * length_weight) * length_difference + curvature;
    if !include_gradient { return ((energy, None), outgoing); }
    let ci = 1.0 / curvature;
    let mut fx = (-(iux * dot) + oux) * incoming_inverse * ci;
    let mut fy = (-(iuy * dot) + ouy) * incoming_inverse * ci;
    let mut lx = (-(oux * dot) + iux) * outgoing_inverse * -1.0 * ci;
    let mut ly = (-(ouy * dot) + iuy) * outgoing_inverse * -1.0 * ci;
    let weighted = length_difference * length_weight;
    let ld = weighted + weighted;
    let isi = 1.0 / counts[0]; let osi = 1.0 / counts[1];
    fx = iux * ld * isi + fx; fy = iuy * ld * isi + fy;
    lx = oux * ld * osi + lx; ly = ouy * ld * osi + ly;
    ((energy, Some(([fx, fy], [lx, ly]))), outgoing)
}

fn quadratic_terms(first: P, middle: P, last: P, counts: [f64; 2], include_gradient: bool) -> Terms {
    let ii = 1.0 / counts[0]; let oi = 1.0 / counts[1];
    let cx = (last[0] - middle[0]) * oi - (middle[0] - first[0]) * ii;
    let cy = (last[1] - middle[1]) * oi - (middle[1] - first[1]) * ii;
    let energy = cy * cy + cx * cx;
    if !include_gradient { return (energy, None); }
    let tx = cx + cx; let ty = cy + cy;
    (energy, Some(([tx * ii, ty * ii], [tx * oi, ty * oi])))
}

/// Prepared smoothing input. Parameters are the resolved settings object.
pub struct SmoothingInput<'a> {
    pub geometry: &'a Geometry,
    pub reference_positions: &'a [f64],
    pub penalty_counts: &'a [u8],
    pub border_counts: &'a [i64],
    pub parameters: &'a Value,
    pub phase: i64,
    pub image_model: Option<&'a mut ContourImageModel>,
    pub width: f64,
    pub height: f64,
    pub near_border_nodes: &'a [i64],
    pub direction: &'a [f64],
    pub epsilon: f64,
    pub nearby_edges: Option<&'a [Vec<[usize; 2]>]>,
    pub repair_interval: f64,
}

impl<'a> SmoothingInput<'a> {
    pub fn new(geometry: &'a Geometry, parameters: &'a Value) -> Self {
        SmoothingInput { geometry, reference_positions: &[], penalty_counts: &[], border_counts: &[], parameters, phase: 0,
            image_model: None, width: 0.0, height: 0.0, near_border_nodes: &[], direction: &[], epsilon: 0.0,
            nearby_edges: None, repair_interval: 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings { measurement: i64, prior: i64, strength: f64, length_weight: f64, pressure: f64, inversion_prior: f64, inversion_measurement: f64 }

/// Prepared objective state visible to iteration observers.
pub struct ObjectiveState<'a> {
    pub geometry: Geometry,
    pub points: Vec<P>,
    references: Vec<f32>,
    pub penalty_counts: Vec<u8>,
    // Same-width cache of the flag byte only; `geometry` stays canonical and is
    // refreshed (never trusted) around every mutation boundary.
    quadratic_flags: Vec<u8>,
    border_counts: [usize; 4],
    settings: Settings,
    image_kernel: Option<&'a mut ImageKernel>,
    image_positions: Vec<f64>,
}

impl ObjectiveState<'_> {
    /// Join the live coordinates into the canonical geometry a progress
    /// snapshot publishes.
    ///
    /// The objective's arithmetic reads `points`; `geometry` keeps the phase's
    /// input coordinates, which nothing reads after preparation. A published
    /// snapshot reads `geometry`, so the two are joined immediately before an
    /// observer sees them. Flags and byte penalties are already canonical in
    /// `geometry` and `penalty_counts` (a repair writes them there), so nothing
    /// else is copied and no phase-long duplicate of the input geometry is kept.
    pub(crate) fn synchronize_published_coordinates(&mut self) {
        for (node, point) in self.geometry.nodes.iter_mut().zip(&self.points) {
            node.x = point[0];
            node.y = point[1];
        }
    }
}

#[cfg(test)]
impl ObjectiveState<'_> {
    /// A state for the publication oracle: the phase's input geometry, the live
    /// coordinates and the byte penalties, with no image kernel.
    pub(crate) fn for_observer_test(geometry: Geometry, points: Vec<P>, penalty_counts: Vec<u8>) -> Self {
        let quadratic_flags = geometry.nodes.iter().map(|n| n.flags).collect();
        ObjectiveState { geometry, points, references: Vec::new(), quadratic_flags, penalty_counts,
            border_counts: [0; 4], settings: Settings { measurement: 0, prior: 0, strength: 0.0, length_weight: 0.0,
                pressure: 0.0, inversion_prior: 0.0, inversion_measurement: 0.0 },
            image_kernel: None, image_positions: Vec::new() }
    }
}

fn num(v: &Value) -> f64 { v.as_f64().unwrap_or(f64::NAN) }
fn is_int(v: f64) -> bool { v.is_finite() && v.fract() == 0.0 }

fn read_objective_input<'a>(input: &mut SmoothingInput<'a>) -> Result<ObjectiveState<'a>, String> {
    let phase = input.phase;
    if !(0..=2).contains(&phase) { return Err("Smoothing phase must be 0, 1 or 2.".into()); }
    let geometry = input.geometry;
    if geometry.nodes.len() < 4 || geometry.regions.is_empty() { return Err("Smoothing objective needs nodes and region contours.".into()); }
    let node_count = geometry.nodes.len();
    let quadratic_flags = geometry.nodes.iter().map(|n| n.flags).collect();
    let mut points = Vec::with_capacity(node_count);
    for n in &geometry.nodes {
        if !n.x.is_finite() || !n.y.is_finite() { return Err("Smoothing nodes need finite x/y and byte flags.".into()); }
        points.push([n.x, n.y]);
    }
    if input.reference_positions.len() != node_count * 2 { return Err("Reference positions need two finite float32 channels per node.".into()); }
    let mut references = Vec::with_capacity(node_count * 2);
    for &r in input.reference_positions {
        if !r.is_finite() || !(r as f32).is_finite() { return Err("Reference positions need two finite float32 channels per node.".into()); }
        references.push(r as f32);
    }
    if input.penalty_counts.len() != node_count { return Err("Smoothing penaltyCounts must contain one byte count per node.".into()); }
    let b = input.border_counts;
    if b.len() != 4 || b.iter().any(|&c| c < 0) || b[0] < 2 || b[1] < 2 || b.iter().sum::<i64>() > node_count as i64 {
        return Err("Smoothing borderCounts must describe the left, right, top and bottom node groups.".into());
    }
    for region in &geometry.regions {
        if region.pixel_count < 1 || region.contour.len() < 3 || region.boundary.len() != region.contour.len() {
            return Err("Smoothing regions need positive pixel counts and matching contour/boundary records.".into());
        }
        if region.contour.iter().any(|&n| n >= node_count) { return Err("Smoothing contours must identify existing nodes.".into()); }
        if region.boundary.iter().any(|e| e[0] < 0 || e[0] > 255 || e[2] < 1) {
            return Err("Smoothing boundaries need byte flags and positive int32 sample counts.".into());
        }
    }
    let parameters = input.parameters;
    if !parameters.is_object() { return Err("Expected numeric smoothing parameters.".into()); }
    let phase_value = |name: &str| -> Result<f64, String> {
        match parameters.get(format!("ContourSmoother::{name}")).and_then(Value::as_array) {
            Some(values) if values.len() == 3 => Ok(num(&values[phase as usize])),
            _ => Err(format!("{name} must supply three phase values.")),
        }
    };
    let nonnegative = |value: f64, name: &str| -> Result<f64, String> {
        if !value.is_finite() || value < 0.0 { Err(format!("{name} must be finite and nonnegative.")) } else { Ok(value) }
    };
    let measurement = phase_value("measurement_types")?; let prior = phase_value("prior_types")?;
    if measurement != 0.0 && measurement != 1.0 && measurement != 2.0 { return Err("This smoothing measurement remains under reconstruction.".into()); }
    if prior != 0.0 && prior != 3.0 { return Err("This smoothing prior remains under reconstruction.".into()); }
    let top = |k: &str| parameters.get(format!("ContourSmoother::{k}")).map_or(f64::NAN, num);
    let settings = Settings { measurement: measurement as i64, prior: prior as i64,
        strength: nonnegative(phase_value("prior_strengths")?, "prior_strengths")?,
        length_weight: nonnegative(phase_value("length_penalty_weights")?, "length_penalty_weights")?,
        pressure: nonnegative(top("air_pressure_weight"), "air_pressure_weight")?,
        inversion_prior: nonnegative(top("anti_inv_pot_prior_scale"), "anti_inv_pot_prior_scale")?,
        inversion_measurement: nonnegative(top("anti_inv_pot_meas_scale"), "anti_inv_pot_meas_scale")? };
    let image_kernel = if settings.measurement == 1 { Some(read_image_kernel(input)?) } else { None };
    let image_positions = if image_kernel.is_some() { vec![0.0; node_count * 2] } else { Vec::new() };
    Ok(ObjectiveState { geometry: geometry.clone(), points, references, quadratic_flags, penalty_counts: input.penalty_counts.to_vec(),
        border_counts: [b[0] as usize, b[1] as usize, b[2] as usize, b[3] as usize], settings, image_kernel, image_positions })
}

fn read_image_kernel<'a>(input: &mut SmoothingInput<'a>) -> Result<&'a mut ImageKernel, String> {
    let geometry = input.geometry;
    match input.image_model.take() {
        Some(model) => model.read_kernel(geometry).map_err(|e| e.0),
        None => Err("Image measurement code 1 needs a matching createContourImageModel instance.".into()),
    }
}

fn add_endpoint_gradient(gradient: &mut [f64], first: usize, middle: usize, last: usize, g: (P, P), scale: f64) {
    let fx = g.0[0] * scale; let fy = g.0[1] * scale; let lx = g.1[0] * scale; let ly = g.1[1] * scale;
    // The gradient is two `f64` channels per node, so view it as node pairs: the
    // eight stores below then address one node each instead of computing `node * 2`
    // and `node * 2 + 1` and bounds-checking both. `objective_terms` allocates the
    // buffer as `vec![0.0; points.len() * 2]`, so the remainder is always empty;
    // `debug_assert` pins that, and a non-empty remainder would still be handled
    // correctly by the `as_chunks_mut` contract (it is simply never indexed here).
    let (nodes, remainder) = gradient.as_chunks_mut::<2>();
    debug_assert!(remainder.is_empty(), "the gradient buffer is two channels per node");
    nodes[first][0] = fx + nodes[first][0];
    nodes[first][1] = fy + nodes[first][1];
    nodes[middle][0] -= fx;
    nodes[middle][1] -= fy;
    nodes[middle][0] -= lx;
    nodes[middle][1] -= ly;
    nodes[last][0] = lx + nodes[last][0];
    nodes[last][1] = ly + nodes[last][1];
}

fn region_area(points: &[P], contour: &[usize]) -> f64 {
    // The wrap-around sample `(contour[len-1], contour[0])` is peeled out of the
    // loop, so the body runs over exact-length `windows(2)` pairs of `contour`
    // instead of two independently bounds-checked `contour[i]` loads with a
    // `(s + 1) % len` remainder. The peeled sample is the loop's own last
    // iteration, written with the same expression in the same order and added
    // **last**, so every sample is visited once in the same sequence and the
    // `f64` summation order is unchanged. `read_objective_input` enforces
    // `contour.len() >= 3` and `contour[i] < points.len()`, so the peeled index
    // and every `points[...]` are in range exactly as before.
    let mut twice = 0.0;
    for pair in contour.windows(2) {
        let (c, n) = (pair[0], pair[1]);
        let (c, n) = (&points[c], &points[n]);
        twice = (n[1] * c[0] - c[1] * n[0]) + twice;
    }
    let (c, n) = (contour[contour.len() - 1], contour[0]);
    let (c, n) = (&points[c], &points[n]);
    twice = (n[1] * c[0] - c[1] * n[0]) + twice;
    twice * 0.5
}

/// The small-region pressure term's gradient: every contour vertex pushes on
/// its successor and is pushed on by it, with the same four expressions in the
/// same order as the original modulo loop.
///
/// The next sample's index is carried instead of taken as a remainder:
/// `next_index` starts at 1 and steps to 0 after the last sample, so the loop
/// visits exactly the pairs `(c[s], c[(s + 1) % len])` in the same order, and
/// the wrap sample is the loop's own last iteration. `read_objective_input`
/// enforces `contour.len() >= 3`, so `next_index` is always in range.
fn add_pressure_gradient(gradient: &mut [f64], points: &[P], c: &[usize], factor: f64) {
    let mut next_index = 1;
    for s in 0..c.len() {
        let current = c[s]; let next = c[next_index];
        gradient[current * 2] = points[next][1] * 0.5 * factor + gradient[current * 2];
        gradient[current * 2 + 1] = points[next][0] * -0.5 * factor + gradient[current * 2 + 1];
        gradient[next * 2] = points[current][1] * -0.5 * factor + gradient[next * 2];
        gradient[next * 2 + 1] = points[current][0] * 0.5 * factor + gradient[next * 2 + 1];
        next_index = if next_index + 1 == c.len() { 0 } else { next_index + 1 };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective { pub energy: f64, pub gradient: Vec<f64> }

/// Original prepared objective, including optional persistent image measurement.
pub fn smoothing_objective(input: &mut SmoothingInput) -> Result<Objective, String> {
    let mut state = read_objective_input(input)?;
    let (e, g) = objective_terms_const::<true, true>(&mut state)?;
    Ok(Objective { energy: e, gradient: g.unwrap() })
}

fn energy_only(state: &mut ObjectiveState) -> Result<f64, String> { Ok(objective_terms_const::<true, false>(state)?.0) }

/// The objective with its two accumulations chosen at compile time.
///
/// Every caller asks for one fixed pair: the line searches want the energy
/// alone, the controller's per-iteration gradient wants the gradient alone, and
/// the prepared objective wants both. The flags are const parameters, so each
/// instantiation runs exactly the operations the runtime-flag form ran for that
/// pair — the same expressions, in the same order, with the branches that pair
/// never took removed. Nothing is re-associated and no value is computed that
/// the pair did not compute before, so every instantiation is bit-identical to
/// the retained runtime-flag oracle (`objective_terms_reference`).
///
/// `settings.prior` is fixed for the whole call and `read_objective_input` has
/// already validated it to be 0 or 3, so the per-sample `settings.prior == 0`
/// test is decided once here and handed to the body as a const parameter.
fn objective_terms_const<const ENERGY: bool, const GRADIENT: bool>(state: &mut ObjectiveState) -> Result<(f64, Option<Vec<f64>>), String> {
    if state.settings.prior == 0 {
        objective_terms_prior::<ENERGY, GRADIENT, true>(state)
    } else {
        objective_terms_prior::<ENERGY, GRADIENT, false>(state)
    }
}

fn objective_terms_prior<const ENERGY: bool, const GRADIENT: bool, const PRIOR_ZERO: bool>(state: &mut ObjectiveState) -> Result<(f64, Option<Vec<f64>>), String> {
    let include_energy = ENERGY;
    let include_gradient = GRADIENT;
    let ObjectiveState { geometry, points, references, penalty_counts, quadratic_flags, border_counts, settings, image_kernel, image_positions } = state;
    let mut gradient = if include_gradient { vec![0.0; points.len() * 2] } else { Vec::new() };
    let barrier_scale = settings.inversion_prior / settings.strength;
    let mut energy = 0.0;
    for region in &geometry.regions {
        let (contour, boundary) = (&region.contour, &region.boundary);
        let len = contour.len();
        // The wrap-around sample is peeled out of the loop, so the body runs over
        // exact-length `windows(2)` pairs of `contour` and `boundary` instead of
        // three independent `contour[i]` loads with a `+ 1 == len` select on each
        // sample. The peeled sample is the loop's own last iteration (`first =
        // contour[len - 2]`, `middle = contour[len - 1]`, `last = contour[0]`),
        // written with the same expressions in the same order, so the sequence of
        // samples, the arithmetic and the summation order are unchanged — measured
        // byte-exact on the sample, every format variant, and the 3000-case oracle.
        // (`boundary.len() == contour.len()` is enforced by `read_objective_input`.)
        let mut first = contour[len - 1];
        // The first sample's incoming edge is the peeled sample's outgoing edge,
        // which is not computed yet; `carried` holds the previous iteration's
        // outgoing edge when that iteration computed one.
        let mut carried: Option<Edge> = None;
        // `boundary.len() == contour.len()` (`read_objective_input` enforces it), so the
        // boundary's own two-record window runs in lockstep with the contour's: zipping
        // them takes `bs`/`bn` from one window pair instead of two independently
        // bounds-checked `boundary[sample]`/`boundary[sample + 1]` loads per sample. The
        // pairs are the same records in the same order, and the peeled sample below still
        // uses the raw indices.
        for (pair, bpair) in contour.windows(2).zip(boundary.windows(2)) {
            let (middle, last) = (pair[0], pair[1]);
            let (bs, bn) = (bpair[0], bpair[1]);
            let counts = [bs[2] as f64, bn[2] as f64];
            let mut quadratic = None;
            if quadratic_flags[middle] & 16 != 0 {
                let q = quadratic_terms(points[first], points[middle], points[last], counts, include_gradient);
                if include_energy { energy = q.0 * barrier_scale + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, q.1.unwrap(), barrier_scale); }
                quadratic = Some(q);
            }
            let terms = if region.pixel_count < 8 || (bs[0] & 1 == 0 && PRIOR_ZERO) {
                carried = None;
                Some(quadratic.unwrap_or_else(|| quadratic_terms(points[first], points[middle], points[last], counts, include_gradient)))
            } else if bs[0] & 1 == 0 {
                let incoming = carried.take().unwrap_or_else(|| edge(points[first], points[middle]));
                let (t, outgoing) = curvature_terms_cached(points[middle], points[last], counts, settings.length_weight, include_gradient, incoming);
                carried = Some(outgoing);
                Some(t)
            } else { carried = None; None };
            if let Some(t) = terms {
                if include_energy { energy = t.0 + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, t.1.unwrap(), 1.0); }
            }
            first = middle;
        }
        {
            let sample = len - 1;
            let middle = contour[sample]; let last = contour[0];
            let (bs, bn) = (boundary[sample], boundary[0]);
            let counts = [bs[2] as f64, bn[2] as f64];
            let mut quadratic = None;
            if quadratic_flags[middle] & 16 != 0 {
                let q = quadratic_terms(points[first], points[middle], points[last], counts, include_gradient);
                if include_energy { energy = q.0 * barrier_scale + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, q.1.unwrap(), barrier_scale); }
                quadratic = Some(q);
            }
            let terms = if region.pixel_count < 8 || (bs[0] & 1 == 0 && PRIOR_ZERO) {
                Some(quadratic.unwrap_or_else(|| quadratic_terms(points[first], points[middle], points[last], counts, include_gradient)))
            } else if bs[0] & 1 == 0 {
                let incoming = carried.take().unwrap_or_else(|| edge(points[first], points[middle]));
                Some(curvature_terms_cached(points[middle], points[last], counts, settings.length_weight, include_gradient, incoming).0)
            } else { None };
            if let Some(t) = terms {
                if include_energy { energy = t.0 + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, t.1.unwrap(), 1.0); }
            }
        }
    }
    if include_energy { energy *= settings.strength; }
    if include_gradient { for g in gradient.iter_mut() { *g = settings.strength * *g; } }
    let pressure_scale = settings.strength * settings.pressure;
    for region in &geometry.regions {
        if region.pixel_count >= 8 { continue; }
        let difference = region_area(points, &region.contour) - region.pixel_count as f64;
        if include_energy { energy = difference * difference * pressure_scale + energy; }
        if !include_gradient { continue; }
        let weighted = difference * pressure_scale; let factor = weighted + weighted;
        add_pressure_gradient(&mut gradient, points, &region.contour, factor);
    }
    // `references` is two `f32` channels per node and the gradient two `f64` channels
    // per node, so the loops below address one node at a time instead of computing
    // `node * 2` and `node * 2 + 1` and bounds-checking each of the four loads and
    // four stores. `references` is allocated as two channels per node and `gradient`
    // as `points.len() * 2`, so both remainders are empty; `debug_assert` pins that,
    // and a non-empty remainder would still be handled correctly by the `as_chunks`
    // contract (it is simply never indexed here). The same channels feed the same
    // expressions in the same order.
    let (reference_pairs, reference_remainder) = references.as_chunks::<2>();
    debug_assert!(reference_remainder.is_empty(), "references are two channels per node");
    if settings.measurement == 1 {
        for (node, p) in points.iter().enumerate() { image_positions[node * 2] = p[0]; image_positions[node * 2 + 1] = p[1]; }
        let kernel = image_kernel.as_mut().expect("prepared image kernel");
        if include_energy { energy = kernel.energy(image_positions).map_err(|e| e.0)? + energy; }
        if include_gradient { kernel.add_gradient(&mut gradient, image_positions); }
    } else {
        let (gradient_nodes, gradient_remainder) = gradient.as_chunks_mut::<2>();
        debug_assert!(gradient_remainder.is_empty(), "the gradient is two channels per node");
        for (node, p) in points.iter().enumerate() {
            let reference = &reference_pairs[node];
            let dx = p[0] - reference[0] as f64; let dy = p[1] - reference[1] as f64;
            if settings.measurement == 0 {
                if include_energy { energy = (dy * dy + dx * dx) + energy; }
                if include_gradient {
                    gradient_nodes[node][0] = dx + dx + gradient_nodes[node][0];
                    gradient_nodes[node][1] = dy + dy + gradient_nodes[node][1];
                }
            } else {
                let distance = (dy * dy + dx * dx).sqrt();
                if include_energy {
                    let excess = distance - 0.7; let penalty = if excess <= 0.0 { 0.0 } else { excess };
                    energy = penalty * penalty + energy;
                }
                if include_gradient && distance > 0.7 {
                    let fraction = 1.0 - 0.7 / distance; let factor = fraction + fraction;
                    gradient_nodes[node][0] = dx * factor + gradient_nodes[node][0];
                    gradient_nodes[node][1] = dy * factor + gradient_nodes[node][1];
                }
            }
        }
    }
    {
        let (gradient_nodes, gradient_remainder) = gradient.as_chunks_mut::<2>();
        debug_assert!(gradient_remainder.is_empty(), "the gradient is two channels per node");
        for (node, p) in points.iter().enumerate() {
            if penalty_counts[node] == 0 { continue; }
            let reference = &reference_pairs[node];
            let dx = p[0] - reference[0] as f64; let dy = p[1] - reference[1] as f64;
            let scale = penalty_counts[node] as f64 * settings.inversion_measurement;
            if include_energy { energy = scale * (dy * dy + dx * dx) + energy; }
            if !include_gradient { continue; }
            let factor = scale + scale;
            gradient_nodes[node][0] = factor * dx + gradient_nodes[node][0];
            gradient_nodes[node][1] = factor * dy + gradient_nodes[node][1];
        }
    }
    if !include_gradient { return Ok((energy, None)); }
    let [left, right, top, bottom] = *border_counts;
    gradient[1] = 0.0;
    for node in 0..left + right { gradient[node * 2] = 0.0; }
    gradient[left * 2 - 1] = 0.0;
    gradient[left * 2 + 1] = 0.0;
    gradient[(left + right) * 2 - 1] = 0.0;
    for node in left + right..left + right + top + bottom { gradient[node * 2 + 1] = 0.0; }
    Ok((energy, Some(gradient)))
}

/// The runtime-flag form, kept for the equivalence tests: it dispatches to the
/// same four const instantiations the production callers use, so a test that
/// sweeps the flag pairs exercises exactly the code the engine runs.
#[cfg(test)]
fn objective_terms(state: &mut ObjectiveState, include_energy: bool, include_gradient: bool) -> Result<(f64, Option<Vec<f64>>), String> {
    match (include_energy, include_gradient) {
        (true, true) => objective_terms_const::<true, true>(state),
        (true, false) => objective_terms_const::<true, false>(state),
        (false, true) => objective_terms_const::<false, true>(state),
        (false, false) => objective_terms_const::<false, false>(state),
    }
}

/// The line search's scalar controls. The direction is *not* a field: it is the
/// controller's live buffer, rewritten between segments, so each segment
/// borrows it for the call instead of the controls owning a copy of it.
#[derive(Clone, Debug)]
struct LineControls { width: f64, height: f64, near_border_nodes: Vec<usize>, epsilon: f64,
    minimum: f64, maximum: f64, max_step: f64, fraction: f64, tolerance: f64, max_iterations: i64, use_step: bool }

fn read_line_search_input(input: &SmoothingInput, state: &ObjectiveState, direction: &[f64], epsilon: f64) -> Result<LineControls, String> {
    let (width, height) = (input.width, input.height);
    let dim = |v: f64| is_int(v) && (2.0..=2147483647.0).contains(&v);
    if !dim(width) || !dim(height) { return Err("Line search needs positive image dimensions of at least two pixels.".into()); }
    if let Some(k) = &state.image_kernel {
        if width != k.width as f64 || height != k.height as f64 { return Err("Line-search dimensions must match the prepared image model.".into()); }
    }
    if direction.len() != state.points.len() * 2 || direction.iter().any(|v| !v.is_finite()) {
        return Err("Line-search direction needs two finite double channels per node.".into());
    }
    if !epsilon.is_finite() || epsilon <= 0.0 { return Err("Line-search epsilon must be finite and positive.".into()); }
    let border_total: usize = state.border_counts.iter().sum();
    if input.near_border_nodes.iter().any(|&n| n < border_total as i64 || n >= state.points.len() as i64) {
        return Err("Line search needs the prepared cache of near-border interior node identifiers.".into());
    }
    let pv = |name: &str| input.parameters.get(format!("ContourSmoother::phase_{}.{name}", input.phase)).map_or(f64::NAN, num);
    let minimum = pv("cg_min_eps"); let maximum = pv("cg_max_eps");
    let max_step = pv("cg_max_step_size"); let fraction = pv("cg_step_fraction");
    let tolerance = pv("cg_quad_line_srch_rel_tol"); let max_iterations = pv("cg_quad_line_srch_max_iter");
    let use_step = pv("cg_use_eps_from_step");
    if !minimum.is_finite() || minimum <= 0.0 || !maximum.is_finite() || maximum < minimum ||
        !max_step.is_finite() || max_step < 0.0 || !fraction.is_finite() || fraction < 0.0 ||
        !tolerance.is_finite() || tolerance < 0.0 || !is_int(max_iterations) || max_iterations < 1.0 ||
        max_iterations > 2147483647.0 || (use_step != 0.0 && use_step != 1.0) {
        return Err("Invalid quadratic line-search phase controls.".into());
    }
    Ok(LineControls { width, height, near_border_nodes: input.near_border_nodes.iter().map(|&n| n as usize).collect(),
        epsilon, minimum, maximum, max_step, fraction, tolerance,
        max_iterations: max_iterations as i64, use_step: use_step == 1.0 })
}

fn clamp_interior_coordinate(coordinate: f64, maximum: f64) -> f64 {
    if 0.01 <= coordinate { return if maximum < coordinate { maximum } else { coordinate }; }
    0.01
}

fn move_points(points: &mut [P], border_counts: [usize; 4], c: &LineControls, direction: &[f64], step: f64) {
    // The direction holds two `f64` channels per node, so view it as node pairs:
    // each node then reads one pair instead of computing `node * 2` and
    // `node * 2 + 1` and bounds-checking both loads. `read_line_search_input`
    // requires `direction.len() == points.len() * 2`, so the two slices are the
    // same length and the remainder is empty; `debug_assert` pins that, and a
    // non-empty remainder would still be handled correctly by the `as_chunks`
    // contract (it is simply never indexed here). The same two channels feed the
    // same expressions in the same order.
    let (direction_nodes, remainder) = direction.as_chunks::<2>();
    debug_assert!(remainder.is_empty(), "the direction is two channels per node");
    for (p, d) in points.iter_mut().zip(direction_nodes) {
        p[0] = step * d[0] + p[0];
        p[1] = step * d[1] + p[1];
    }
    let [left, right, top, bottom] = border_counts;
    points[0][1] = 0.0;
    for p in &mut points[..left] { p[0] = 0.0; }
    points[left - 1][1] = c.height;
    points[left][1] = 0.0;
    for p in &mut points[left..left + right] { p[0] = c.width; }
    points[left + right - 1][1] = c.height;
    for p in &mut points[left + right..left + right + top] { p[1] = 0.0; }
    for p in &mut points[left + right + top..left + right + top + bottom] { p[1] = c.height; }
    for &node in &c.near_border_nodes {
        points[node][0] = clamp_interior_coordinate(points[node][0], c.width - 0.01);
        points[node][1] = clamp_interior_coordinate(points[node][1], c.height - 0.01);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LineResult { pub positions: Vec<f64>, pub energy: f64, pub delta: f64, pub epsilon: f64, pub step: f64, pub iterations: i64 }

impl LineResult {
    pub fn to_json(&self) -> Value {
        json!({"positions": self.positions, "energy": self.energy, "delta": self.delta, "epsilon": self.epsilon,
            "step": self.step, "iterations": self.iterations})
    }
}

struct Line { energy: f64, delta: f64, epsilon: f64, step: f64, iterations: i64, gradient: Option<Vec<f64>> }

/// Original quadratic line search on an explicit prepared geometric objective.
pub fn smoothing_line_search(input: &mut SmoothingInput) -> Result<LineResult, String> {
    let mut state = read_objective_input(input)?;
    let controls = read_line_search_input(input, &state, input.direction, input.epsilon)?;
    let initial = energy_only(&mut state)?;
    let line = quadratic_line(&mut state, &controls, input.direction, initial, false)?;
    Ok(LineResult { positions: state.points.iter().flatten().copied().collect(), energy: line.energy, delta: line.delta,
        epsilon: line.epsilon, step: line.step, iterations: line.iterations })
}

fn quadratic_line(state: &mut ObjectiveState, c: &LineControls, direction: &[f64], initial_energy: f64, want_gradient: bool) -> Result<Line, String> {
    let mut max_direction = 0.0f64;
    for v in direction { if v.abs() > max_direction { max_direction = v.abs(); } }
    let mut epsilon = c.epsilon;
    if c.max_step > 0.0 && c.max_step < max_direction * epsilon { epsilon = c.max_step / max_direction; }
    let initial_epsilon = epsilon;
    let mut energy = initial_energy; let mut base_energy; let mut step = 0.0; let mut iterations = 0i64;
    // The gradient of the position this segment leaves behind, when the caller
    // asked for it. The controller's next step needs exactly that gradient, and
    // it is the same position whose energy the segment's last evaluation
    // produced, so the two accumulations are formed in one pass instead of two.
    let mut final_gradient;
    let borders = state.border_counts;
    loop {
        let backup = state.points.clone();
        base_energy = energy;
        move_points(&mut state.points, borders, c, direction, epsilon);
        let first_energy = energy_only(state)?;
        move_points(&mut state.points, borders, c, direction, epsilon);
        let second_energy = energy_only(state)?;
        let squared = epsilon * epsilon;
        let curvature = ((second_energy - (first_energy + first_energy)) + base_energy) / (squared + squared);
        let mut offset = 0.0;
        if !(curvature <= 0.0) {
            let product = curvature * epsilon;
            offset = (-1.0 / (curvature + curvature)) * (product + product + (second_energy - base_energy) / (epsilon + epsilon));
        }
        if c.max_step > 0.0 {
            if c.max_step < offset * max_direction { offset = c.max_step / max_direction; }
            if c.max_step < -(offset * max_direction) { offset = (-1.0 / max_direction) * c.max_step; }
        }
        let mut candidate_energy = base_energy + 1.0;
        let mut candidate_gradient = None;
        if offset != 0.0 {
            move_points(&mut state.points, borders, c, direction, offset);
            if want_gradient {
                let (e, g) = objective_terms_const::<true, true>(state)?;
                candidate_energy = e; candidate_gradient = g;
            } else {
                candidate_energy = energy_only(state)?;
            }
        }
        let mut best_energy = base_energy; let mut best_index = 0.0;
        if first_energy < base_energy { best_energy = first_energy; best_index = 1.0; }
        if second_energy < best_energy { best_energy = second_energy; best_index = 2.0; }
        let accepted_step;
        if candidate_energy < best_energy {
            accepted_step = epsilon + epsilon + offset;
            energy = candidate_energy;
            final_gradient = candidate_gradient;
        } else {
            accepted_step = best_index * epsilon;
            state.points = backup;
            move_points(&mut state.points, borders, c, direction, accepted_step);
            if want_gradient {
                let (e, g) = objective_terms_const::<true, true>(state)?;
                energy = e; final_gradient = g;
            } else {
                energy = energy_only(state)?;
                final_gradient = None;
            }
        }
        step = accepted_step + step;
        epsilon *= 0.5;
        if epsilon < c.minimum { epsilon = c.minimum; }
        if c.maximum < epsilon { epsilon = c.maximum; }
        iterations += 1;
        if !(base_energy != 0.0 && c.tolerance < (base_energy - energy) / base_energy && iterations < c.max_iterations) { break; }
    }
    if c.use_step {
        epsilon = step.abs() * c.fraction;
        if epsilon < c.minimum { epsilon = c.minimum; }
        if epsilon > c.maximum { epsilon = c.maximum; }
    } else { epsilon = initial_epsilon; }
    Ok(Line { energy, delta: energy - base_energy, epsilon, step, iterations, gradient: final_gradient })
}

struct Cache { seeing: i64, blind: i64, blend: f64, position: i64, average: f64 }

fn cached_line(state: &mut ObjectiveState, c: &LineControls, direction: &[f64], cache: &mut Cache, initial_energy: f64, previous_step: f64, iteration: i64, want_gradient: bool) -> Result<Line, String> {
    cache.position %= cache.seeing + cache.blind;
    let mut energy = initial_energy;
    let mut line = Line { energy, delta: 0.0, epsilon: c.epsilon, step: previous_step, iterations: 0, gradient: None };
    if cache.position >= cache.seeing {
        let backup = state.points.clone();
        let mut offset = cache.average; let mut max_direction = 0.0f64;
        for v in direction { if v.abs() > max_direction { max_direction = v.abs(); } }
        if c.max_step > 0.0 {
            if c.max_step < offset * max_direction { offset = c.max_step / max_direction; }
            if c.max_step < -(offset * max_direction) { offset = -(c.max_step / max_direction); }
        }
        let borders = state.border_counts;
        move_points(&mut state.points, borders, c, direction, offset * 0.8);
        // The controller's next step needs the gradient at the position this
        // blind step leaves behind, and the step's own evaluation already
        // produced the energy there. When the caller asked for the gradient,
        // form both accumulations in that one pass and carry the gradient out,
        // so the controller does not scan the whole objective a second time at
        // the same position. The two instantiations differ only in the
        // `GRADIENT` flag: the energy accumulation, its order and every
        // comparison are the same, and the gradient buffer is a fresh one, so
        // the energy is bit-identical and the gradient is the one the second
        // scan would have built.
        if want_gradient {
            let (e, g) = objective_terms_const::<true, true>(state)?;
            energy = e; line.gradient = g;
        } else {
            energy = energy_only(state)?;
        }
        if initial_energy < energy {
            // The step is rejected and the position moves back, so the gradient
            // just formed is at a position the controller will never read. Drop
            // it: the caller falls back to its own scan, exactly as before.
            state.points = backup;
            energy = initial_energy;
            cache.position = 0;
            line.gradient = None;
        }
        line.energy = energy;
    }
    if cache.position < cache.seeing {
        line = quadratic_line(state, c, direction, energy, want_gradient)?;
        let retained = (1.0 - cache.blend) * cache.average;
        cache.average = if iteration == 0 { line.step } else { line.step * cache.blend + retained };
    }
    cache.position += 1;
    Ok(line)
}

fn read_prepared_repair<'a>(input: &mut SmoothingInput<'a>, state: &mut ObjectiveState<'a>) -> Result<Option<ContourRepair>, String> {
    let Some(nearby_edges) = input.nearby_edges else { return Ok(None) };
    let (nodes, regions) = (&state.geometry.nodes, &state.geometry.regions);
    let ri = input.repair_interval;
    if nearby_edges.len() != nodes.len() || !is_int(ri) || ri < 1.0 || ri > 2147483647.0 {
        return Err("Contour repair needs fixed nearby edges per node and a positive int32 repair interval.".into());
    }
    for edges in nearby_edges {
        for e in edges {
            if e[0] >= regions.len() || e[1] >= regions[e[0]].contour.len() {
                return Err("Nearby contour edges must identify existing region samples.".into());
            }
        }
    }
    if state.image_kernel.is_none() { state.image_kernel = Some(read_image_kernel(input)?); }
    Ok(Some(ContourRepair::new(regions.len(), nearby_edges.to_vec())))
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptimizeResult {
    pub positions: Vec<f64>, pub energy: f64, pub baseline: f64, pub epsilon: f64, pub step: f64, pub beta: f64,
    pub iterations: i64, pub restarts: i64, pub line_iterations: i64, pub countdown: i64,
    pub stored_gradient: Vec<f64>, pub direction: Vec<f64>, pub repair_state: Option<RepairNodeState>,
}

impl OptimizeResult {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"positions": self.positions, "energy": self.energy, "baseline": self.baseline, "epsilon": self.epsilon,
            "step": self.step, "beta": self.beta, "iterations": self.iterations, "restarts": self.restarts,
            "lineIterations": self.line_iterations, "countdown": self.countdown, "storedGradient": self.stored_gradient,
            "direction": self.direction});
        if let Some(r) = &self.repair_state {
            v["repairState"] = json!({"flags": r.flags, "penaltyCounts": r.penalty_counts});
        }
        v
    }
}

fn repair_view<'s>(state: &'s mut ObjectiveState) -> RepairView<'s> {
    RepairView { geometry: &mut state.geometry, penalty_counts: &mut state.penalty_counts, points: &state.points }
}

/// Re-derive the flag-byte cache from canonical `geometry`. Called *after* every
/// repair that can write `nodes[..].flags` through a `RepairView`, never before:
/// the view hands out `&mut geometry`, so a refresh taken before the repair
/// would be stale for the next `objective_terms`.
fn refresh_quadratic_flags(state: &mut ObjectiveState) {
    state.quadratic_flags.clear();
    state.quadratic_flags.extend(state.geometry.nodes.iter().map(|n| n.flags));
}

/// Original prepared Fletcher-Reeves controller, before multi-phase smoothing.
pub fn smoothing_optimize(input: &mut SmoothingInput, on_iteration: Option<&mut dyn FnMut(i64, &ObjectiveState) -> Result<(), String>>)
    -> Result<OptimizeResult, String> {
    match on_iteration {
        Some(observer) => optimize(input, Some(&mut |iteration, state| observer(iteration, state))),
        None => optimize(input, None),
    }
}

pub(crate) fn optimize(input: &mut SmoothingInput, mut on_iteration: Option<&mut dyn FnMut(i64, &mut ObjectiveState) -> Result<(), String>>)
    -> Result<OptimizeResult, String> {
    let mut state = read_objective_input(input)?;
    let parameters = input.parameters; let phase = input.phase;
    let pv = |name: &str| parameters.get(format!("ContourSmoother::phase_{phase}.{name}")).map_or(f64::NAN, num);
    let optimizer_types = parameters.get("ContourSmoother::optimizer_type").and_then(Value::as_array);
    let line_type = pv("cg_line_search_type");
    if !optimizer_types.is_some_and(|t| t.len() == 3 && num(&t[phase as usize]) == 0.0) || (line_type != 0.0 && line_type != 5.0) {
        return Err("Prepared smoothing optimization requires optimizer 0 and line-search type 0 or 5.".into());
    }
    let mut direction = vec![0.0; state.points.len() * 2];
    let mut controls = read_line_search_input(input, &state, &direction, pv("cg_max_eps"))?;
    let min_iterations = pv("cg_min_iter"); let max_iterations = pv("cg_max_iter");
    let restart_interval = pv("cg_iter_for_restart"); let knockout = pv("cg_knock_out_count_down");
    let relative_tolerance = pv("cg_rel_tol"); let absolute_tolerance = pv("cg_abs_tol");
    let int_in = |v: f64, lo: f64| is_int(v) && v >= lo && v <= 2147483647.0;
    if !int_in(min_iterations, 0.0) || !int_in(max_iterations, 1.0) || !int_in(restart_interval, 1.0) || !int_in(knockout, 0.0) ||
        !relative_tolerance.is_finite() || relative_tolerance < 0.0 || !absolute_tolerance.is_finite() || absolute_tolerance < 0.0 {
        return Err("Invalid conjugate-gradient phase controls.".into());
    }
    let (min_iterations, max_iterations, restart_interval, knockout) =
        (min_iterations as i64, max_iterations as i64, restart_interval as i64, knockout as i64);
    let mut cache = None;
    if line_type == 5.0 {
        let seeing = pv("cg_num_seeing_steps"); let blind = pv("cg_num_blind_steps"); let blend = pv("cg_blind_blend_latest");
        if !is_int(seeing) || seeing < 1.0 || seeing > 2147483647.0 || !is_int(blind) || blind < 0.0 ||
            seeing + blind > 2147483647.0 || !blend.is_finite() || !(0.0..=1.0).contains(&blend) {
            return Err("Invalid seeing/blind line-search controls.".into());
        }
        cache = Some(Cache { seeing: seeing as i64, blind: blind as i64, blend, position: 0, average: 0.0 });
    }
    let mut repair = read_prepared_repair(input, &mut state)?;
    let repair_interval = input.repair_interval as i64;
    let (mut energy, g) = objective_terms_const::<true, true>(&mut state)?;
    let mut stored_gradient = g.unwrap();
    let mut norm = 0.0;
    for ch in 0..direction.len() {
        direction[ch] = -stored_gradient[ch];
        norm = direction[ch] * direction[ch] + norm;
    }
    let mut baseline = energy; let mut epsilon = controls.epsilon; let mut step = 0.0; let mut beta = 0.0;
    let (mut iterations, mut restarts, mut line_iterations, mut countdown) = (0i64, 0i64, 0i64, 0i64);
    while iterations < max_iterations {
        if energy < baseline { baseline = energy; }
        controls.epsilon = epsilon;
        // The segment borrows the controller's live direction; nothing is copied.
        // The controller's next step needs the gradient at the position this
        // segment leaves behind, and the segment's last evaluation already
        // produced the energy there. When nothing between the two can change
        // the objective's inputs, the segment forms both accumulations in that
        // one pass and the controller reuses the gradient instead of scanning
        // the whole objective a second time at the same position.
        //
        // The precondition is structural, not assumed: a repair writes flags
        // and byte penalties through a `RepairView` and consumes the kernel's
        // invalid set, so with a repair configured the carried gradient is
        // discarded; the iteration observer is read-only, so it cannot
        // invalidate anything.
        let fuse_gradient = repair.is_none();
        let line = match cache.as_mut() {
            Some(cache) => cached_line(&mut state, &controls, &direction, cache, energy, step, iterations, fuse_gradient),
            None => quadratic_line(&mut state, &controls, &direction, energy, fuse_gradient),
        }?;
        energy = line.energy; epsilon = line.epsilon; step = line.step; line_iterations += line.iterations;
        if let Some(repair) = repair.as_mut() {
            let changed = if iterations % repair_interval == repair_interval - 1 {
                repair.all(&mut repair_view(&mut state)) > 0
            } else {
                let ids = state.image_kernel.as_mut().expect("repair kernel").take_invalid_regions();
                repair.invalid(&mut repair_view(&mut state), &ids)
            };
            // The repair wrote flags through the view; re-derive before any read.
            refresh_quadratic_flags(&mut state);
            if changed {
                beta = 0.0;
                energy = energy_only(&mut state)?;
                baseline = energy;
                for ch in 0..direction.len() { direction[ch] = -stored_gradient[ch]; }
                restarts += 1;
            }
        }
        if let Some(cb) = on_iteration.as_mut() {
            cb(iterations, &mut state)?;
        }
        let improvement = baseline - energy;
        let relative = if baseline <= 0.0 { 0.0 } else { improvement / baseline };
        if relative < relative_tolerance || (improvement < absolute_tolerance && improvement >= 0.0) {
            countdown += 1;
            if knockout < countdown && min_iterations < iterations { break; }
        } else { countdown = 0; }
        stored_gradient = match line.gradient {
            Some(gradient) => gradient,
            None => objective_terms_const::<false, true>(&mut state)?.1.unwrap(),
        };
        let previous_norm = norm;
        norm = 0.0;
        for v in &stored_gradient { norm = v * v + norm; }
        beta = if previous_norm.abs() >= 1e-30 { norm / previous_norm } else { 0.0 };
        let mut alignment = 0.0;
        for ch in 0..direction.len() {
            direction[ch] = beta * direction[ch] - stored_gradient[ch];
            alignment = alignment - stored_gradient[ch] * direction[ch];
        }
        if alignment <= 0.0 || iterations % restart_interval == 0 {
            beta = 0.0;
            for ch in 0..direction.len() { direction[ch] = -stored_gradient[ch]; }
            restarts += 1;
        }
        iterations += 1;
    }
    energy = energy_only(&mut state)?;
    let repair_state = repair.as_ref().map(|_| ContourRepair::node_state(&repair_view(&mut state)));
    Ok(OptimizeResult { positions: state.points.iter().flatten().copied().collect(), energy, baseline, epsilon, step, beta,
        iterations, restarts, line_iterations, countdown, stored_gradient, direction, repair_state })
}

/// The original `objective_terms`, kept verbatim with its modulo indexing, as
/// the exact oracle for the carried-index production loop. Test-only.
#[cfg(test)]
#[allow(clippy::type_complexity)]
fn objective_terms_reference(state: &mut ObjectiveState, include_energy: bool, include_gradient: bool) -> Result<(f64, Option<Vec<f64>>), String> {
    let ObjectiveState { geometry, points, references, penalty_counts, border_counts, settings, image_kernel, image_positions, quadratic_flags: _ } = state;
    let mut gradient = if include_gradient { vec![0.0; points.len() * 2] } else { Vec::new() };
    let barrier_scale = settings.inversion_prior / settings.strength;
    let mut energy = 0.0;
    for region in &geometry.regions {
        let (contour, boundary) = (&region.contour, &region.boundary);
        let len = contour.len();
        for sample in 0..len {
            let first = contour[(sample + len - 1) % len]; let middle = contour[sample];
            let next_sample = (sample + 1) % len; let last = contour[next_sample];
            let counts = [boundary[sample][2] as f64, boundary[next_sample][2] as f64];
            let mut quadratic = None;
            if geometry.nodes[middle].flags & 16 != 0 {
                let q = quadratic_terms(points[first], points[middle], points[last], counts, include_gradient);
                if include_energy { energy = q.0 * barrier_scale + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, q.1.unwrap(), barrier_scale); }
                quadratic = Some(q);
            }
            let terms = if region.pixel_count < 8 || (boundary[sample][0] & 1 == 0 && settings.prior == 0) {
                Some(quadratic.unwrap_or_else(|| quadratic_terms(points[first], points[middle], points[last], counts, include_gradient)))
            } else if boundary[sample][0] & 1 == 0 {
                Some(curvature_terms(points[first], points[middle], points[last], counts, settings.length_weight, include_gradient))
            } else { None };
            if let Some(t) = terms {
                if include_energy { energy = t.0 + energy; }
                if include_gradient { add_endpoint_gradient(&mut gradient, first, middle, last, t.1.unwrap(), 1.0); }
            }
        }
    }
    if include_energy { energy *= settings.strength; }
    if include_gradient { for g in gradient.iter_mut() { *g = settings.strength * *g; } }
    let pressure_scale = settings.strength * settings.pressure;
    for region in &geometry.regions {
        if region.pixel_count >= 8 { continue; }
        let difference = region_area(points, &region.contour) - region.pixel_count as f64;
        if include_energy { energy = difference * difference * pressure_scale + energy; }
        if !include_gradient { continue; }
        let weighted = difference * pressure_scale; let factor = weighted + weighted;
        let c = &region.contour;
        for s in 0..c.len() {
            let current = c[s]; let next = c[(s + 1) % c.len()];
            gradient[current * 2] = points[next][1] * 0.5 * factor + gradient[current * 2];
            gradient[current * 2 + 1] = points[next][0] * -0.5 * factor + gradient[current * 2 + 1];
            gradient[next * 2] = points[current][1] * -0.5 * factor + gradient[next * 2];
            gradient[next * 2 + 1] = points[current][0] * 0.5 * factor + gradient[next * 2 + 1];
        }
    }
    if settings.measurement == 1 {
        for (node, p) in points.iter().enumerate() { image_positions[node * 2] = p[0]; image_positions[node * 2 + 1] = p[1]; }
        let kernel = image_kernel.as_mut().expect("prepared image kernel");
        if include_energy { energy = kernel.energy(image_positions).map_err(|e| e.0)? + energy; }
        if include_gradient { kernel.add_gradient(&mut gradient, image_positions); }
    } else {
        for (node, p) in points.iter().enumerate() {
            let dx = p[0] - references[node * 2] as f64; let dy = p[1] - references[node * 2 + 1] as f64;
            if settings.measurement == 0 {
                if include_energy { energy = (dy * dy + dx * dx) + energy; }
                if include_gradient {
                    gradient[node * 2] = dx + dx + gradient[node * 2];
                    gradient[node * 2 + 1] = dy + dy + gradient[node * 2 + 1];
                }
            } else {
                let distance = (dy * dy + dx * dx).sqrt();
                if include_energy {
                    let excess = distance - 0.7; let penalty = if excess <= 0.0 { 0.0 } else { excess };
                    energy = penalty * penalty + energy;
                }
                if include_gradient && distance > 0.7 {
                    let fraction = 1.0 - 0.7 / distance; let factor = fraction + fraction;
                    gradient[node * 2] = dx * factor + gradient[node * 2];
                    gradient[node * 2 + 1] = dy * factor + gradient[node * 2 + 1];
                }
            }
        }
    }
    for (node, p) in points.iter().enumerate() {
        if penalty_counts[node] == 0 { continue; }
        let dx = p[0] - references[node * 2] as f64; let dy = p[1] - references[node * 2 + 1] as f64;
        let scale = penalty_counts[node] as f64 * settings.inversion_measurement;
        if include_energy { energy = scale * (dy * dy + dx * dx) + energy; }
        if !include_gradient { continue; }
        let factor = scale + scale;
        gradient[node * 2] = factor * dx + gradient[node * 2];
        gradient[node * 2 + 1] = factor * dy + gradient[node * 2 + 1];
    }
    if !include_gradient { return Ok((energy, None)); }
    let [left, right, top, bottom] = *border_counts;
    gradient[1] = 0.0;
    for node in 0..left + right { gradient[node * 2] = 0.0; }
    gradient[left * 2 - 1] = 0.0;
    gradient[left * 2 + 1] = 0.0;
    gradient[(left + right) * 2 - 1] = 0.0;
    for node in left + right..left + right + top + bottom { gradient[node * 2 + 1] = 0.0; }
    Ok((energy, Some(gradient)))
}

#[cfg(test)]
mod move_points_equivalence {
    //! The production `move_points` against the retained, per-channel-indexed
    //! original: the same points, direction, borders and step must produce the
    //! same bits in every coordinate, including the border writes and the
    //! near-border clamps.
    use super::*;

    /// The original loop, kept verbatim as the oracle.
    fn move_points_reference(points: &mut [P], border_counts: [usize; 4], c: &LineControls, direction: &[f64], step: f64) {
        for (node, p) in points.iter_mut().enumerate() {
            p[0] = step * direction[node * 2] + p[0];
            p[1] = step * direction[node * 2 + 1] + p[1];
        }
        let [left, right, top, bottom] = border_counts;
        points[0][1] = 0.0;
        for p in &mut points[..left] { p[0] = 0.0; }
        points[left - 1][1] = c.height;
        points[left][1] = 0.0;
        for p in &mut points[left..left + right] { p[0] = c.width; }
        points[left + right - 1][1] = c.height;
        for p in &mut points[left + right..left + right + top] { p[1] = 0.0; }
        for p in &mut points[left + right + top..left + right + top + bottom] { p[1] = c.height; }
        for &node in &c.near_border_nodes {
            points[node][0] = clamp_interior_coordinate(points[node][0], c.width - 0.01);
            points[node][1] = clamp_interior_coordinate(points[node][1], c.height - 0.01);
        }
    }

    fn controls(near_border_nodes: Vec<usize>) -> LineControls {
        LineControls { width: 37.0, height: 23.0, near_border_nodes, epsilon: 0.5,
            minimum: 1e-6, maximum: 1e6, max_step: 0.0, fraction: 0.5, tolerance: 1e-9,
            max_iterations: 8, use_step: false }
    }

    fn check(points: &[P], borders: [usize; 4], c: &LineControls, direction: &[f64], step: f64, label: &str) {
        let mut fast = points.to_vec();
        let mut slow = points.to_vec();
        move_points(&mut fast, borders, c, direction, step);
        move_points_reference(&mut slow, borders, c, direction, step);
        assert_eq!(fast.len(), slow.len(), "{label}: length");
        for (node, (a, b)) in fast.iter().zip(&slow).enumerate() {
            assert_eq!(a[0].to_bits(), b[0].to_bits(), "{label}: node {node} x");
            assert_eq!(a[1].to_bits(), b[1].to_bits(), "{label}: node {node} y");
        }
    }

    #[test]
    fn the_node_pair_view_moves_every_coordinate_exactly_as_the_original() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
        let mut checked = 0;
        for &len in &[4usize, 5, 6, 7, 8, 9, 16, 17, 33, 64, 65, 129] {
            for &(left, right, top, bottom) in &[(2usize, 2usize, 0usize, 0usize), (2, 2, 1, 1), (3, 2, 2, 1)] {
                if left + right + top + bottom > len { continue; }
                let points: Vec<P> = (0..len).map(|i| {
                    let jitter = (next() % 1000) as f64 / 500.0;
                    [0.5 + i as f64 * 0.25 + jitter, 0.25 + (i % 5) as f64 + jitter]
                }).collect();
                let direction: Vec<f64> = (0..len * 2).map(|_| (next() % 2001) as f64 / 1000.0 - 1.0).collect();
                let near: Vec<usize> = (left + right + top + bottom..len).step_by(3).collect();
                let c = controls(near);
                for &step in &[0.0, 1.0, -1.0, 0.5, -0.25, 1e-9, 1e9] {
                    check(&points, [left, right, top, bottom], &c, &direction, step, "grid");
                    checked += 1;
                }
            }
        }
        assert!(checked >= 200, "the sweep must not be vacuous: {checked}");
    }

    #[test]
    fn the_node_pair_view_handles_the_border_and_clamp_edges() {
        // A point already outside the clamp window, one exactly on it, and one
        // past the far edge: the clamp must land on the same bits in both forms.
        let points: Vec<P> = vec![[0.0, 0.0], [0.01, 0.01], [36.99, 22.99], [100.0, -100.0], [1e300, -1e300], [5.0, 5.0]];
        let direction: Vec<f64> = vec![0.0, 0.0, 1.0, -1.0, -1.0, 1.0, 0.5, 0.5, -0.5, -0.5, 0.0, 0.0];
        let c = controls(vec![2, 3, 4, 5]);
        for &step in &[0.0, 1.0, -1.0, 1e300, -1e300] {
            check(&points, [2, 2, 0, 0], &c, &direction, step, "edges");
        }
    }
}

#[cfg(test)]
mod direction_borrow {
    //! The line search must read the controller's *live* direction, not a copy
    //! taken when the controls were built.
    //!
    //! `smoothing_optimize` rewrites its direction buffer between segments, so a
    //! segment that owned a copy would have to be handed a fresh copy each
    //! iteration. The segment now borrows the buffer for the call. These tests
    //! pin the two properties that make the borrow equivalent to the copy it
    //! replaced: the segment sees a direction written *after* the controls were
    //! built, and the segment never writes the direction it reads.
    use super::*;
    use crate::contours::{Node, Region};

    fn state_for(len: usize, seed: u64) -> ObjectiveState<'static> {
        let mut state = seed | 1;
        let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
        let nodes: Vec<Node> = (0..len + 2).map(|i| {
            let jitter = (next() % 1000) as f64 / 4000.0;
            Node { x: 3.0 + i as f64 * 0.75 + jitter, y: 4.0 + (i % 3) as f64 + jitter, kind: 1, flags: 0 }
        }).collect();
        let contour: Vec<usize> = (1..=len).collect();
        let boundary: Vec<[i32; 3]> = (0..len).map(|i| [0, 0, 1 + (i as i32 % 5)]).collect();
        let region = Region { pixel_count: 64, color_index: 0, bgra: [1, 2, 3, 255], parent: -1,
            contour, boundary, neighbors: Vec::new() };
        let points: Vec<P> = nodes.iter().map(|n| [n.x, n.y]).collect();
        let references: Vec<f32> = points.iter().flat_map(|p| [p[0] as f32 + 0.25f32, p[1] as f32 - 0.5f32]).collect();
        let penalty_counts: Vec<u8> = (0..nodes.len()).map(|i| (i % 3) as u8).collect();
        let quadratic_flags: Vec<u8> = nodes.iter().map(|n| n.flags).collect();
        ObjectiveState { geometry: Geometry { nodes, regions: vec![region] }, points,
            references, quadratic_flags, penalty_counts, border_counts: [2, 2, 0, 0],
            settings: Settings { measurement: 0, prior: 3, strength: 1.5, length_weight: 0.25,
                pressure: 2.0, inversion_prior: 0.5, inversion_measurement: 0.75 },
            image_kernel: None, image_positions: Vec::new() }
    }

    fn controls() -> LineControls {
        LineControls { width: 37.0, height: 23.0, near_border_nodes: Vec::new(), epsilon: 1e-3,
            minimum: 1e-12, maximum: 1e6, max_step: 0.0, fraction: 0.1, tolerance: 0.001,
            max_iterations: 1, use_step: true }
    }

    /// The descent direction the controller would hand the segment: minus the
    /// gradient, scaled so the largest channel is one.
    fn descent(state: &mut ObjectiveState) -> Vec<f64> {
        let gradient = objective_terms_const::<false, true>(state).expect("gradient").1.unwrap();
        let largest = gradient.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        gradient.iter().map(|v| -v / largest).collect()
    }

    /// A direction written after the controls exist must be the one the segment
    /// moves along: the controls hold no direction of their own to go stale.
    #[test]
    fn the_segment_reads_a_direction_written_after_the_controls_were_built() {
        let c = controls();
        let len = 9usize;
        let mut direction = descent(&mut state_for(len, 0x2545_f491_4f6c_dd1d));
        let mut first = state_for(len, 0x2545_f491_4f6c_dd1d);
        // The segment starts from the clamped position, as the blind block does.
        move_points(&mut first.points, first.border_counts, &c, &direction, 0.0);
        let initial = energy_only(&mut first).expect("initial");
        let first_line = quadratic_line(&mut first, &c, &direction, initial, false).expect("first");
        let first_points: Vec<u64> = first.points.iter().flatten().map(|v| v.to_bits()).collect();
        assert_ne!(first_line.step.to_bits(), 0.0f64.to_bits(), "the first segment must accept a step");

        // Rewrite the buffer in place, exactly as the controller does between
        // segments, and run the same controls again.
        for (i, v) in direction.iter_mut().enumerate() { *v = -0.25 - (i as f64) * 0.125; }
        let mut second = state_for(len, 0x2545_f491_4f6c_dd1d);
        move_points(&mut second.points, second.border_counts, &c, &direction, 0.0);
        let second_line = quadratic_line(&mut second, &c, &direction, initial, false).expect("second");
        let second_points: Vec<u64> = second.points.iter().flatten().map(|v| v.to_bits()).collect();

        assert_ne!(first_points, second_points, "the segment must follow the rewritten direction");
        assert_ne!(first_line.energy.to_bits(), second_line.energy.to_bits(),
            "a different direction must reach a different energy");

        // And the second run must be the one a fresh copy of the rewritten
        // direction produces, so the borrow is not merely "some other" buffer.
        let copied = direction.clone();
        let mut third = state_for(len, 0x2545_f491_4f6c_dd1d);
        move_points(&mut third.points, third.border_counts, &c, &copied, 0.0);
        let third_line = quadratic_line(&mut third, &c, &copied, initial, false).expect("third");
        assert_eq!(second_line.energy.to_bits(), third_line.energy.to_bits(), "energy");
        assert_eq!(second_line.step.to_bits(), third_line.step.to_bits(), "step");
        assert_eq!(second_points, third.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "the borrowed and the copied direction must move the points identically");
    }

    /// The segment only reads the direction: every channel must come back with
    /// the same bits, including the non-finite and signed-zero cases the
    /// controller can carry.
    #[test]
    fn the_segment_never_writes_the_direction_it_reads() {
        let c = controls();
        let len = 8usize;
        let direction: Vec<f64> = (0..len * 2).map(|i| match i {
            0 => -0.0, 1 => f64::NAN, 2 => f64::INFINITY, 3 => f64::NEG_INFINITY,
            _ => ((i as f64) * 0.61).cos() * 2.0,
        }).collect();
        let before: Vec<u64> = direction.iter().map(|v| v.to_bits()).collect();
        let mut state = state_for(len, 0x51ed_270b_9a3c_1f77);
        let initial = energy_only(&mut state).expect("initial");
        let _ = quadratic_line(&mut state, &c, &direction, initial, false).expect("segment");
        assert_eq!(direction.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), before,
            "the direction is an input, not scratch");
        // The blind block reads it too.
        let mut cache = Cache { seeing: 0, blind: 1, blend: 0.5, position: 0, average: -5.0 };
        let mut state = state_for(len, 0x51ed_270b_9a3c_1f77);
        let initial = energy_only(&mut state).expect("initial");
        let _ = cached_line(&mut state, &c, &direction, &mut cache, initial, 0.0, 0, false).expect("blind");
        assert_eq!(direction.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), before,
            "the blind block must not write the direction either");
    }
}

#[cfg(test)]
mod index_equivalence {
    //! The production, carried-index `objective_terms` against the retained,
    //! modulo-indexed original: contour lengths from the shortest a region may
    //! have (3) past every power of two in the loop, both boundary flag
    //! branches, small and large regions (so both the quadratic and curvature
    //! terms and the pressure branch run), both measurement codes and both
    //! energy/gradient combinations. Energy and every gradient channel are
    //! compared by bit pattern, so a single changed rounding fails.
    use super::*;
    use crate::contours::{Node, Region};

    fn settings_with(pressure: f64, prior: i64, strength: f64, measurement: i64, length_weight: f64) -> Settings {
        Settings { measurement, prior, strength, length_weight, pressure, inversion_prior: 0.5, inversion_measurement: 0.75 }
    }

    fn state_for(len: usize, seed: u64, curved: bool, quadratic_flag: bool, small: bool) -> (Geometry, Vec<P>, Vec<f32>, Vec<u8>) {
        let mut state = seed | 1;
        let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
        let nodes: Vec<Node> = (0..len + 2).map(|i| {
            let jitter = (next() % 1000) as f64 / 4000.0;
            Node { x: 3.0 + i as f64 * 0.75 + jitter, y: 4.0 + (i % 3) as f64 + jitter, kind: 1,
                flags: if i == 1 && quadratic_flag { 16 } else { 0 } }
        }).collect();
        let mut contour: Vec<usize> = (1..=len).collect();
        if len % 2 == 0 { contour[len / 2] = contour[0]; }
        let boundary: Vec<[i32; 3]> = (0..len).map(|i| {
            let flags = if curved { (i as i32 & 1) << 1 } else { 0 };
            [flags, 0, 1 + (i as i32 % 5)]
        }).collect();
        let region = Region { pixel_count: if small { 4 } else { 64 }, color_index: 0, bgra: [1, 2, 3, 255],
            parent: -1, contour, boundary, neighbors: Vec::new() };
        let points: Vec<P> = nodes.iter().map(|n| [n.x, n.y]).collect();
        let references: Vec<f32> = points.iter().flat_map(|p| [p[0] as f32 + 0.25f32, p[1] as f32 - 0.5f32]).collect();
        let penalty_counts: Vec<u8> = (0..nodes.len()).map(|i| (i % 3) as u8).collect();
        (Geometry { nodes, regions: vec![region] }, points, references, penalty_counts)
    }

    fn compare(geometry: &Geometry, points: &[P], references: &[f32], penalty_counts: &[u8],
        settings: Settings, label: &str) -> usize {
        let borders = [2usize, 2, 0, 0];
        let mut checked = 0;
        for &energy in &[true, false] {
            for &gradient in &[true, false] {
                let mut fast = ObjectiveState { geometry: geometry.clone(), points: points.to_vec(),
                    references: references.to_vec(), quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(), penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                    settings, image_kernel: None, image_positions: vec![0.0; points.len() * 2] };
                let mut slow = ObjectiveState { geometry: geometry.clone(), points: points.to_vec(),
                    references: references.to_vec(), quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(), penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                    settings, image_kernel: None, image_positions: vec![0.0; points.len() * 2] };
                let got = objective_terms(&mut fast, energy, gradient).expect("fast");
                let want = objective_terms_reference(&mut slow, energy, gradient).expect("reference");
                assert_eq!(want.0.to_bits(), got.0.to_bits(), "energy {label} energy={energy} gradient={gradient}");
                match (want.1, got.1) {
                    (None, None) => {}
                    (Some(a), Some(b)) => {
                        assert_eq!(a.len(), b.len(), "{label}");
                        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
                            assert_eq!(x.to_bits(), y.to_bits(), "gradient[{i}] {label}");
                        }
                    }
                    _ => panic!("gradient presence differs: {label}"),
                }
                assert_eq!(fast.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    slow.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(), "points moved {label}");
                checked += 1;
            }
        }
        checked
    }

    #[test]
    fn carried_indices_match_the_retained_modulo_objective_exactly() {
        let mut checked = 0usize;
        for len in 3..=70usize {
            for &curved in &[false, true] {
                for &quadratic_flag in &[false, true] {
                    for &small in &[false, true] {
                        let (geometry, points, references, penalty_counts) =
                            state_for(len, 0x9e37_79b9_7f4a_7c15 ^ len as u64, curved, quadratic_flag, small);
                        for &(pressure, prior, strength, measurement, weight) in
                            &[(2.0, 3, 1.5, 0, 0.25), (0.0, 0, 1.0, 2, 0.0), (0.5, 3, 0.25, 2, 0.75)] {
                            let settings = settings_with(pressure, prior, strength, measurement, weight);
                            checked += compare(&geometry, &points, &references, &penalty_counts, settings,
                                &format!("len={len} curved={curved} quad={quadratic_flag} small={small} m={measurement}"));
                        }
                    }
                }
            }
        }
        assert!(checked >= 3000, "the proof must not be vacuous: {checked} comparisons");
    }

    #[test]
    fn carried_indices_agree_on_the_shortest_and_longest_regions() {
        let mut checked = 0usize;
        for &len in &[3usize, 4, 5, 255, 256, 257] {
            let (geometry, points, references, penalty_counts) = state_for(len, 0xfeed_face_dead_beef, true, true, false);
            checked += compare(&geometry, &points, &references, &penalty_counts,
                settings_with(1.0, 3, 1.0, 0, 0.5), &format!("long len={len}"));
        }
        assert!(checked >= 24);
    }

    /// The fused line search must return exactly the energy and gradient the
    /// two-pass form returns. The fused form asks the segment's last evaluation
    /// for both accumulations; the two-pass form asks for the energy there and
    /// then scans the same position again for the gradient. Both are the same
    /// `objective_terms_const` instantiation on the same state, so the pair must
    /// agree bit for bit — and the position the segment leaves behind must be
    /// the same too, or the gradient would describe a different point.
    #[test]
    fn the_fused_line_search_returns_the_two_pass_energy_and_gradient() {
        let mut checked = 0usize;
        for &len in &[3usize, 4, 5, 8, 9, 16, 33, 64] {
            for &curved in &[false, true] {
                for &quadratic_flag in &[false, true] {
                    for &small in &[false, true] {
                        let (geometry, points, references, penalty_counts) =
                            state_for(len, 0x2545_f491_4f6c_dd1d ^ len as u64, curved, quadratic_flag, small);
                        for &(pressure, prior, strength, measurement, weight) in
                            &[(2.0, 3, 1.5, 0, 0.25), (0.0, 0, 1.0, 2, 0.0), (0.5, 3, 0.25, 2, 0.75)] {
                            let settings = settings_with(pressure, prior, strength, measurement, weight);
                            let borders = [2usize, 2, 0, 0];
                            let direction: Vec<f64> = (0..points.len() * 2)
                                .map(|i| ((i as f64) * 0.37).sin() * 0.5).collect();
                            let c = LineControls { width: 37.0, height: 23.0, near_border_nodes: Vec::new(),
                                epsilon: 0.01, minimum: 1e-6, maximum: 0.01, max_step: 0.3,
                                fraction: 0.1, tolerance: 0.001, max_iterations: 1, use_step: true };
                            let make = || ObjectiveState { geometry: geometry.clone(), points: points.to_vec(),
                                references: references.to_vec(),
                                quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(),
                                penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                                settings, image_kernel: None, image_positions: vec![0.0; points.len() * 2] };
                            let mut fused = make();
                            let mut plain = make();
                            let initial = energy_only(&mut fused).expect("initial");
                            let fused_line = quadratic_line(&mut fused, &c, &direction, initial, true).expect("fused");
                            let plain_line = quadratic_line(&mut plain, &c, &direction, initial, false).expect("plain");
                            assert_eq!(fused_line.energy.to_bits(), plain_line.energy.to_bits(),
                                "energy len={len} curved={curved} quad={quadratic_flag} small={small}");
                            assert_eq!(fused_line.step.to_bits(), plain_line.step.to_bits(), "step len={len}");
                            assert_eq!(fused_line.iterations, plain_line.iterations, "iterations len={len}");
                            assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                plain.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                "the segment must leave the same position len={len}");
                            let carried = fused_line.gradient.expect("the fused segment carries a gradient");
                            let rescanned = objective_terms_const::<false, true>(&mut plain).expect("rescan").1.unwrap();
                            assert_eq!(carried.len(), rescanned.len(), "gradient length len={len}");
                            for (i, (a, b)) in carried.iter().zip(&rescanned).enumerate() {
                                assert_eq!(a.to_bits(), b.to_bits(),
                                    "gradient[{i}] len={len} curved={curved} quad={quadratic_flag} small={small}");
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked >= 192, "the sweep must not be vacuous: {checked}");
    }

    /// Non-finite control: the fused and two-pass segments must agree bit for
    /// bit when the search's own arithmetic goes non-finite, and the carried
    /// gradient must be a buffer of its own.
    ///
    /// `quadratic_line` is reachable with non-finite inputs in production: the
    /// controller builds `direction` from `stored_gradient`, which is the
    /// objective's own output, and a non-finite energy makes that gradient
    /// non-finite. `read_line_search_input` validates the *public* entry, not
    /// this internal call. So the four cases below are legal states, not
    /// synthetic ones:
    ///  - coincident contour points, so `curvature_terms` divides by a zero
    ///    length (`1.0 / 0.0`, then `0.0 * inf`) and the energy is NaN;
    ///  - coordinates near `f64::MAX`, so a squared delta overflows to inf;
    ///  - a non-finite direction channel (the controller's own gradient path);
    ///  - a non-finite step.
    ///
    /// Every value is compared by `to_bits`, because NaN compares unequal to
    /// itself: a different NaN payload is a different result.
    #[test]
    fn the_fused_segment_agrees_with_the_two_pass_form_on_non_finite_values() {
        let mut checked = 0usize;
        let mut non_finite_seen = 0usize;
        for &len in &[8usize, 16] {
            let (geometry, points, references, penalty_counts) =
                state_for(len, 0x9e37_79b9_7f4a_7c15 ^ len as u64, true, false, false);
            let settings = settings_with(1.0, 3, 1.0, 0, 0.5);
            let borders = [2usize, 2, 0, 0];
            let wave: Vec<f64> = (0..points.len() * 2).map(|i| ((i as f64) * 0.37).sin() * 0.5).collect();
            let mut cases: Vec<(&str, Vec<P>, Vec<f64>, f64)> = Vec::new();
            // (a) Coincident points at nodes 4 and 5, which `move_points` does
            // not touch with these border counts, and a zero direction so they
            // stay coincident: a zero-length edge makes the curvature NaN.
            let mut coincident = points.clone();
            coincident[5] = coincident[4];
            cases.push(("coincident", coincident, vec![0.0; points.len() * 2], 0.01));
            // (b) Coordinates near f64::MAX: the squared delta overflows.
            let mut huge = points.clone();
            huge[4] = [f64::MAX, f64::MAX];
            huge[5] = [-f64::MAX, -f64::MAX];
            cases.push(("overflow", huge, wave.clone(), 0.01));
            // (c) A non-finite direction channel.
            let mut nan_direction = wave.clone();
            nan_direction[0] = f64::NAN;
            cases.push(("nan-direction", points.clone(), nan_direction, 0.01));
            // (d) A non-finite step.
            cases.push(("nan-epsilon", points.clone(), wave.clone(), f64::NAN));
            for (label, pts, direction, epsilon) in cases {
                let c = LineControls { width: 37.0, height: 23.0, near_border_nodes: Vec::new(),
                    epsilon, minimum: 1e-6, maximum: 0.01, max_step: 0.3,
                    fraction: 0.1, tolerance: 0.001, max_iterations: 1, use_step: true };
                let make = || ObjectiveState { geometry: geometry.clone(), points: pts.clone(),
                    references: references.to_vec(),
                    quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(),
                    penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                    settings, image_kernel: None, image_positions: vec![0.0; pts.len() * 2] };
                let mut fused = make();
                let mut plain = make();
                let initial = energy_only(&mut fused).expect("initial");
                let fused_line = quadratic_line(&mut fused, &c, &direction, initial, true).expect("fused");
                let plain_line = quadratic_line(&mut plain, &c, &direction, initial, false).expect("plain");
                if !fused_line.energy.is_finite() { non_finite_seen += 1; }
                assert_eq!(fused_line.energy.to_bits(), plain_line.energy.to_bits(), "energy {label} len={len}");
                assert_eq!(fused_line.step.to_bits(), plain_line.step.to_bits(), "step {label} len={len}");
                assert_eq!(fused_line.iterations, plain_line.iterations, "iterations {label} len={len}");
                assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    plain.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "position {label} len={len}");
                let carried = fused_line.gradient.expect("the fused segment carries a gradient");
                let rescanned = objective_terms_const::<false, true>(&mut plain).expect("rescan").1.unwrap();
                assert_eq!(carried.len(), rescanned.len(), "gradient length {label} len={len}");
                for (i, (a, b)) in carried.iter().zip(&rescanned).enumerate() {
                    assert_eq!(a.to_bits(), b.to_bits(), "gradient[{i}] {label} len={len}");
                }
                // The carried gradient is a buffer of its own: `objective_terms_const`
                // allocates it, so writing to it cannot move the state the next
                // iteration reads. This is the alias check, not a comment.
                let before: Vec<u64> = fused.points.iter().flatten().map(|v| v.to_bits()).collect();
                let mut carried = carried;
                for v in carried.iter_mut() { *v = 12345.0; }
                assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(), before,
                    "the carried gradient must not alias the live points {label} len={len}");
                checked += 1;
            }
        }
        assert!(checked >= 8, "the sweep must not be vacuous: {checked}");
        assert!(non_finite_seen >= 4, "the control must actually reach non-finite values: {non_finite_seen}");
    }

    /// The blind step's fused evaluation must return exactly the energy and
    /// gradient the two-pass form returns, and must leave the same position.
    ///
    /// The blind block (`cache.position >= cache.seeing`) evaluates the
    /// objective once at the stepped position. The fused form asks that one
    /// evaluation for both accumulations and carries the gradient out; the
    /// two-pass form asks for the energy there and then scans the same
    /// position again for the gradient. Both are the same
    /// `objective_terms_const` instantiation on the same state, so the pair
    /// must agree bit for bit.
    ///
    /// The rejected-step arm is the other half: when `initial_energy < energy`
    /// the position is restored, so the gradient formed at the stepped
    /// position describes a point the controller never reads. The fused form
    /// must drop it there, and the caller's own scan must then produce the
    /// same gradient the two-pass form produces. Both arms are exercised and
    /// counted, so neither can go vacuous.
    #[test]
    fn the_blind_step_returns_the_two_pass_energy_and_gradient() {
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for &len in &[3usize, 4, 5, 8, 9, 16, 33, 64] {
            for &curved in &[false, true] {
                for &quadratic_flag in &[false, true] {
                    for &small in &[false, true] {
                        let (geometry, points, references, penalty_counts) =
                            state_for(len, 0x51ed_270b_9a3c_1f77 ^ len as u64, curved, quadratic_flag, small);
                        for &(pressure, prior, strength, measurement, weight) in
                            &[(2.0, 3, 1.5, 0, 0.25), (0.0, 0, 1.0, 2, 0.0), (0.5, 3, 0.25, 2, 0.75)] {
                            let settings = settings_with(pressure, prior, strength, measurement, weight);
                            let borders = [2usize, 2, 0, 0];
                            let direction: Vec<f64> = (0..points.len() * 2)
                                .map(|i| ((i as f64) * 0.37).sin() * 0.5).collect();
                            let c = LineControls { width: 37.0, height: 23.0, near_border_nodes: Vec::new(),
                                epsilon: 0.01, minimum: 1e-6, maximum: 0.01, max_step: 0.3,
                                fraction: 0.1, tolerance: 0.001, max_iterations: 1, use_step: true };
                            // The blind block steps by `cache.average * 0.8` and
                            // then clamps the border nodes, so the position it
                            // evaluates is the clamped one. `move_points` with a
                            // zero step is exactly that clamp, and it is
                            // idempotent, so taking `initial` after it puts the
                            // baseline at the position the block starts from.
                            // Two averages then exercise both arms: `0.0` leaves
                            // the energy unchanged (accepted), and a large
                            // negative one makes it worse (rejected).
                            let make = || ObjectiveState { geometry: geometry.clone(), points: points.to_vec(),
                                references: references.to_vec(),
                                quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(),
                                penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                                settings, image_kernel: None, image_positions: vec![0.0; points.len() * 2] };
                            for &average in &[0.0f64, -5.0] {
                                // `seeing = 0` puts every call in the blind block;
                                // `blind = 1` keeps `position % 1 == 0`, so the
                                // block runs on every call and the
                                // `quadratic_line` arm never overwrites the
                                // carried gradient.
                                let cache_for = || Cache { seeing: 0, blind: 1, blend: 0.5, position: 0, average };
                                let mut fused = make();
                                let mut plain = make();
                                let mut fused_cache = cache_for();
                                let mut plain_cache = cache_for();
                                move_points(&mut fused.points, borders, &c, &direction, 0.0);
                                move_points(&mut plain.points, borders, &c, &direction, 0.0);
                                let start = fused.points.clone();
                                let initial = energy_only(&mut fused).expect("initial");
                                let fused_line = cached_line(&mut fused, &c, &direction, &mut fused_cache, initial, 0.0, 0, true)
                                    .expect("fused");
                                let plain_line = cached_line(&mut plain, &c, &direction, &mut plain_cache, initial, 0.0, 0, false)
                                    .expect("plain");
                                assert_eq!(fused_line.energy.to_bits(), plain_line.energy.to_bits(),
                                    "energy len={len} curved={curved} quad={quadratic_flag} small={small} avg={average}");
                                assert_eq!(fused_line.step.to_bits(), plain_line.step.to_bits(), "step len={len}");
                                assert_eq!(fused_line.iterations, plain_line.iterations, "iterations len={len}");
                                assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                    plain.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                    "the blind step must leave the same position len={len}");
                                // The two-pass form's gradient is the controller's
                                // own scan at the position the blind step left
                                // behind.
                                let rescanned = objective_terms_const::<false, true>(&mut plain).expect("rescan").1.unwrap();
                                match fused_line.gradient {
                                    Some(carried) => {
                                        // Accepted step: the carried gradient must
                                        // be the one the second scan would build.
                                        assert_eq!(carried.len(), rescanned.len(), "gradient length len={len}");
                                        for (i, (a, b)) in carried.iter().zip(&rescanned).enumerate() {
                                            assert_eq!(a.to_bits(), b.to_bits(),
                                                "gradient[{i}] len={len} curved={curved} quad={quadratic_flag} small={small}");
                                        }
                                        accepted += 1;
                                    }
                                    None => {
                                        // Rejected step: the fused form dropped
                                        // the gradient and the position was
                                        // restored, so the energy is the
                                        // baseline again and the caller's own
                                        // scan answers at the restored position.
                                        assert_eq!(fused_line.energy.to_bits(), initial.to_bits(),
                                            "a dropped gradient means the step was rejected and the energy restored len={len}");
                                        assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                            start.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                            "a rejected step must restore the position len={len}");
                                        rejected += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(accepted >= 100, "the accepted arm must not be vacuous: {accepted}");
        assert!(rejected >= 1, "the rejected arm must not be vacuous: {rejected}");
    }

    /// The blind step's fused form must agree with the two-pass form on
    /// non-finite values too, and the carried gradient must be a buffer of its
    /// own. The cases are the same legal states the segment control uses: a
    /// zero-length edge (NaN curvature), coordinates near `f64::MAX` (an
    /// overflowed squared delta), a non-finite direction channel and a
    /// non-finite step. Every value is compared by `to_bits`, because a
    /// different NaN payload is a different result.
    #[test]
    fn the_blind_step_agrees_with_the_two_pass_form_on_non_finite_values() {
        let mut checked = 0usize;
        let mut non_finite_seen = 0usize;
        for &len in &[8usize, 16] {
            let (geometry, points, references, penalty_counts) =
                state_for(len, 0x51ed_270b_9a3c_1f77 ^ len as u64, true, false, false);
            let settings = settings_with(1.0, 3, 1.0, 0, 0.5);
            let borders = [2usize, 2, 0, 0];
            let wave: Vec<f64> = (0..points.len() * 2).map(|i| ((i as f64) * 0.37).sin() * 0.5).collect();
            // The blind block's own step is `cache.average * 0.8`, so the
            // non-finite cases that reach *its* arithmetic are a non-finite
            // average and a non-finite direction channel; the state cases
            // (coincident points, overflowed coordinates) reach the objective
            // the block evaluates. `epsilon` is not read by the blind block at
            // all, so it is not a case here.
            let mut cases: Vec<(&str, Vec<P>, Vec<f64>, f64)> = Vec::new();
            let mut coincident = points.clone();
            coincident[5] = coincident[4];
            cases.push(("coincident", coincident, vec![0.0; points.len() * 2], 0.0));
            let mut huge = points.clone();
            huge[4] = [f64::MAX, f64::MAX];
            huge[5] = [-f64::MAX, -f64::MAX];
            cases.push(("overflow", huge, wave.clone(), 0.0));
            let mut nan_direction = wave.clone();
            nan_direction[0] = f64::NAN;
            cases.push(("nan-direction", points.clone(), nan_direction, 0.0));
            cases.push(("nan-average", points.clone(), wave.clone(), f64::NAN));
            cases.push(("inf-average", points.clone(), wave.clone(), f64::INFINITY));
            for (label, pts, direction, average) in cases {
                let c = LineControls { width: 37.0, height: 23.0, near_border_nodes: Vec::new(),
                    epsilon: 0.01, minimum: 1e-6, maximum: 0.01, max_step: 0.3,
                    fraction: 0.1, tolerance: 0.001, max_iterations: 1, use_step: true };
                let make = || ObjectiveState { geometry: geometry.clone(), points: pts.clone(),
                    references: references.to_vec(),
                    quadratic_flags: geometry.nodes.iter().map(|n| n.flags).collect(),
                    penalty_counts: penalty_counts.to_vec(), border_counts: borders,
                    settings, image_kernel: None, image_positions: vec![0.0; pts.len() * 2] };
                let cache_for = || Cache { seeing: 0, blind: 1, blend: 0.5, position: 0, average };
                let mut fused = make();
                let mut plain = make();
                let mut fused_cache = cache_for();
                let mut plain_cache = cache_for();
                // The blind block steps by `cache.average * 0.8` and then clamps
                // the border nodes, so the position it evaluates is the clamped
                // one. `move_points` with a zero step is exactly that clamp and
                // is idempotent, so taking `initial` after it puts the baseline
                // at the position the block starts from.
                move_points(&mut fused.points, borders, &c, &direction, 0.0);
                move_points(&mut plain.points, borders, &c, &direction, 0.0);
                let initial = energy_only(&mut fused).expect("initial");
                let fused_line = cached_line(&mut fused, &c, &direction, &mut fused_cache, initial, 0.0, 0, true).expect("fused");
                let plain_line = cached_line(&mut plain, &c, &direction, &mut plain_cache, initial, 0.0, 0, false).expect("plain");
                if !fused_line.energy.is_finite() { non_finite_seen += 1; }
                assert_eq!(fused_line.energy.to_bits(), plain_line.energy.to_bits(), "energy {label} len={len}");
                assert_eq!(fused_line.step.to_bits(), plain_line.step.to_bits(), "step {label} len={len}");
                assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    plain.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "position {label} len={len}");
                let rescanned = objective_terms_const::<false, true>(&mut plain).expect("rescan").1.unwrap();
                if let Some(carried) = fused_line.gradient {
                    assert_eq!(carried.len(), rescanned.len(), "gradient length {label} len={len}");
                    for (i, (a, b)) in carried.iter().zip(&rescanned).enumerate() {
                        assert_eq!(a.to_bits(), b.to_bits(), "gradient[{i}] {label} len={len}");
                    }
                    // The carried gradient is a buffer of its own: writing to it
                    // cannot move the state the next iteration reads.
                    let before: Vec<u64> = fused.points.iter().flatten().map(|v| v.to_bits()).collect();
                    let mut carried = carried;
                    for v in carried.iter_mut() { *v = 12345.0; }
                    assert_eq!(fused.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(), before,
                        "the carried gradient must not alias the live points {label} len={len}");
                }
                checked += 1;
            }
        }
        assert!(checked >= 10, "the sweep must not be vacuous: {checked}");
        assert!(non_finite_seen >= 4, "the control must actually reach non-finite values: {non_finite_seen}");
    }

    /// The peeled wrap sample is the loop's own last iteration, so the sample
    /// sequence must be exactly `(len-1, 0), (0, 1), ... (len-2, len-1)` — the
    /// same `(first, middle, last)` triples in the same order as the retained
    /// modulo original. This pins the peel itself, independently of the term
    /// arithmetic the tests above already compare bit for bit.
    #[test]
    fn the_peeled_wrap_sample_keeps_the_original_triple_sequence() {
        for len in 3..=40usize {
            let contour: Vec<usize> = (0..len).map(|i| (i * 7 + 3) % (len + 2)).collect();
            let mut peeled = Vec::new();
            let mut first = contour[len - 1];
            for pair in contour.windows(2) {
                peeled.push((first, pair[0], pair[1]));
                first = pair[0];
            }
            peeled.push((first, contour[len - 1], contour[0]));
            let original: Vec<(usize, usize, usize)> = (0..len)
                .map(|s| (contour[(s + len - 1) % len], contour[s], contour[(s + 1) % len]))
                .collect();
            assert_eq!(peeled, original, "triple sequence differs at len={len}");
            assert_eq!(peeled.len(), len, "the peel must visit every sample once");
        }
    }

    /// The production objective's pressure branch, end to end: a small region
    /// (so the pressure term runs) at every contour length the ticket names,
    /// against the retained modulo oracle, gradient compared by bit pattern.
    #[test]
    fn the_objective_pressure_branch_matches_the_modulo_oracle_at_every_length() {
        let mut checked = 0usize;
        for &len in &[3usize, 4, 5, 17, 29] {
            for &curved in &[false, true] {
                for &quadratic_flag in &[false, true] {
                    let (geometry, points, references, penalty_counts) =
                        state_for(len, 0x2545_f491_4f6c_dd1d ^ len as u64, curved, quadratic_flag, true);
                    for &(pressure, prior, strength, measurement, weight) in
                        &[(2.0, 3, 1.5, 0, 0.25), (0.5, 3, 0.25, 2, 0.75), (1.0, 0, 1.0, 0, 0.0)] {
                        let settings = settings_with(pressure, prior, strength, measurement, weight);
                        checked += compare(&geometry, &points, &references, &penalty_counts, settings,
                            &format!("pressure len={len} curved={curved} quad={quadratic_flag}"));
                    }
                }
            }
        }
        assert!(checked >= 60, "the sweep must not be vacuous: {checked}");
    }
}

#[cfg(test)]
mod observer_publication_equivalence {
    //! The observer publication against the retained old-copy oracle.
    //!
    //! Before this change the phase kept a duplicate of its own input geometry
    //! and byte penalties alive for the whole phase, and each iteration copied
    //! the live coordinates, the canonical flags and the penalties into that
    //! duplicate before publishing it. The objective now owns the geometry and
    //! the penalties, so the publication joins the live coordinates into that
    //! owned geometry instead. This module pins the two forms together: the
    //! published bytes must be identical, the objective's `points` must not be
    //! touched, every non-coordinate field must survive, and the caller's input
    //! geometry must never be written.
    use super::*;
    use crate::contours::{Node, Region};

    /// Every published byte: coordinates, kind, flags, penalties and the whole
    /// region table, so a replaced field cannot pass unnoticed.
    fn fingerprint(geometry: &Geometry, penalty_counts: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for node in &geometry.nodes {
            bytes.extend(node.x.to_bits().to_le_bytes());
            bytes.extend(node.y.to_bits().to_le_bytes());
            bytes.push(node.kind);
            bytes.push(node.flags);
        }
        bytes.extend(penalty_counts);
        for region in &geometry.regions {
            bytes.extend(region.pixel_count.to_le_bytes());
            bytes.extend(region.color_index.to_le_bytes());
            bytes.extend(region.bgra);
            bytes.extend(region.parent.to_le_bytes());
            for index in &region.contour { bytes.extend(index.to_le_bytes()); }
            for boundary in &region.boundary { for value in boundary { bytes.extend(value.to_le_bytes()); } }
            for neighbor in &region.neighbors { bytes.extend(neighbor.to_le_bytes()); }
        }
        bytes
    }

    /// The retained pre-change publication: a fresh copy of the phase's input
    /// geometry and penalties, with the live coordinates and canonical flags
    /// written into the copy.
    fn old_copy_publication(input: &Geometry, points: &[P], penalties: &[u8]) -> (Geometry, Vec<u8>) {
        let mut geometry = input.clone();
        let mut penalty_counts = penalties.to_vec();
        for (node, point) in points.iter().enumerate() {
            geometry.nodes[node].x = point[0];
            geometry.nodes[node].y = point[1];
            geometry.nodes[node].flags = input.nodes[node].flags;
        }
        penalty_counts.copy_from_slice(penalties);
        (geometry, penalty_counts)
    }

    fn geometry_for(length: usize, neighbors: usize, x: f64) -> Geometry {
        let regions = (0..=length / 3).map(|r| Region {
            pixel_count: r as i32 * 7 - 3,
            color_index: -(r as i32) - 1,
            bgra: [r as u8, !(r as u8), 0, 255],
            parent: r as i32 - 2,
            contour: (r..r + neighbors).map(|v| (length + r + v) % (length + 1)).collect(),
            boundary: (0..neighbors).map(|v| [(v as u32 >> 8) as i32, v as i32, v as i32 + 0x7fff]).collect(),
            neighbors: (-((neighbors as i32) / 2)..neighbors as i32 + 1).collect(),
        }).collect();
        let nodes = (0..=length).map(|i| Node {
            x,
            y: if i & 1 == 0 { 17.0 + i as f64 } else { -0.0 },
            kind: (i as u8).wrapping_mul(173),
            flags: (i as u8).wrapping_mul(97),
        }).collect();
        Geometry { nodes, regions }
    }

    #[test]
    fn published_state_matches_the_old_copy_and_leaves_the_input_alone() {
        let mut checks = 0usize;
        for &length in &[0usize, 1, 2, 3, 4, 5, 31, 32, 33, 63, 64, 65] {
            for &neighbors in &[0usize, 1, 4, 7] {
                for x in [-0.0, 0.0, f64::from_bits(1), -4.25, 1.0e100] {
                    let input = geometry_for(length, neighbors, x);
                    let input_bytes = fingerprint(&input, &[]);
                    let points: Vec<P> = (0..=length).map(|i| [x + i as f64 * 0.5, -x - i as f64 * 0.25]).collect();
                    let penalties: Vec<u8> = (0..=length).map(|i| (i as u8).wrapping_mul(37)).collect();
                    let (want_geometry, want_penalties) = old_copy_publication(&input, &points, &penalties);
                    let want = fingerprint(&want_geometry, &want_penalties);

                    let mut state = ObjectiveState::for_observer_test(input.clone(), points.clone(), penalties.clone());
                    let points_before: Vec<u64> = state.points.iter().flatten().map(|v| v.to_bits()).collect();
                    state.synchronize_published_coordinates();
                    assert_eq!(fingerprint(&state.geometry, &state.penalty_counts), want,
                        "length={length} neighbors={neighbors} x={x:?}");
                    assert_eq!(fingerprint(&input, &[]), input_bytes, "the caller's input geometry was written");
                    assert_eq!(state.points.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>(), points_before,
                        "the live coordinates must not move");
                    assert_eq!(state.penalty_counts, penalties, "the byte penalties must not move");

                    // A second publication in the same phase (the next iteration)
                    // must be identical: the join is idempotent, so a plateau of
                    // equal counters cannot publish a different state.
                    state.synchronize_published_coordinates();
                    assert_eq!(fingerprint(&state.geometry, &state.penalty_counts), want, "repeat publication");
                    checks += 1;
                }
            }
        }
        assert_eq!(checks, 240, "the sweep must not thin out");
    }

    /// The join must be the *only* difference: a state whose live coordinates
    /// differ from its geometry must publish the live ones, so the test above
    /// cannot pass on a no-op.
    #[test]
    fn the_join_really_moves_the_published_coordinates() {
        let input = geometry_for(6, 3, 2.0);
        let points: Vec<P> = (0..=6).map(|i| [100.0 + i as f64, -50.0 - i as f64]).collect();
        let penalties = vec![9u8; 7];
        let mut state = ObjectiveState::for_observer_test(input.clone(), points.clone(), penalties);
        let before = fingerprint(&state.geometry, &state.penalty_counts);
        state.synchronize_published_coordinates();
        let after = fingerprint(&state.geometry, &state.penalty_counts);
        assert_ne!(before, after, "without the join the published geometry is stale");
        for (node, point) in state.geometry.nodes.iter().zip(&points) {
            assert_eq!(node.x.to_bits(), point[0].to_bits());
            assert_eq!(node.y.to_bits(), point[1].to_bits());
        }
    }
}

#[cfg(test)]
mod flag_cache_equivalence {
    //! The cached flag byte against canonical `geometry.nodes[..].flags`.
    //! `objective_terms` must be bit-identical whether it reads the cache or the
    //! geometry, at every point the production loop can reach — including after
    //! a repair has written flags through a `RepairView`.
    use super::*;
    use crate::contours::{Node, Region};

    fn state_with(flags: &[u8]) -> ObjectiveState<'static> {
        let nodes: Vec<Node> = flags.iter().enumerate().map(|(i, &f)| Node {
            x: 3.0 + i as f64, y: 4.0 + (i % 3) as f64, kind: 1, flags: f }).collect();
        let contour: Vec<usize> = (0..flags.len()).collect();
        let boundary: Vec<[i32; 3]> = (0..flags.len()).map(|i| [0, 0, 1 + (i as i32 % 5)]).collect();
        let region = Region { pixel_count: 64, color_index: 0, bgra: [1, 2, 3, 255], parent: -1,
            contour, boundary, neighbors: Vec::new() };
        let geometry = Geometry { nodes, regions: vec![region] };
        let points: Vec<P> = geometry.nodes.iter().map(|n| [n.x, n.y]).collect();
        let references: Vec<f32> = points.iter().flat_map(|p| [p[0] as f32, p[1] as f32]).collect();
        let penalty_counts: Vec<u8> = (0..flags.len()).map(|i| (i % 3) as u8).collect();
        let settings = Settings { measurement: 0, prior: 3, strength: 1.0, length_weight: 0.5,
            pressure: 1.0, inversion_prior: 0.5, inversion_measurement: 0.75 };
        let quadratic_flags = geometry.nodes.iter().map(|n| n.flags).collect();
        ObjectiveState { geometry, points, references, quadratic_flags, penalty_counts,
            border_counts: [2, 2, 0, 0], settings, image_kernel: None, image_positions: Vec::new() }
    }

    /// The cache must equal the canonical flags after every refresh, and the
    /// objective must be bit-identical to the retained oracle that reads
    /// `geometry` directly.
    #[test]
    fn refreshed_cache_matches_canonical_flags_and_the_oracle() {
        let mut checked = 0usize;
        // `border_counts` [2,2,0,0] needs at least four nodes (production invariant).
        for len in 4..=24usize {
            for pattern in 0..6u8 {
                let flags: Vec<u8> = (0..len).map(|i| match pattern {
                    0 => 0,
                    1 => if i % 2 == 0 { 16 } else { 0 },
                    2 => if i % 3 == 0 { 0x16 } else { 4 },
                    3 => 0xf0,
                    4 => if i == 1 { 16 } else { 0 },
                    _ => (i as u8) & 0x1f,
                }).collect();
                let mut state = state_with(&flags);
                refresh_quadratic_flags(&mut state);
                assert_eq!(state.quadratic_flags, flags, "cache must equal canonical flags");
                for &(energy, gradient) in &[(true, false), (true, true), (false, true)] {
                    let got = objective_terms(&mut state, energy, gradient).expect("cached");
                    let want = objective_terms_reference(&mut state, energy, gradient).expect("oracle");
                    assert_eq!(want.0.to_bits(), got.0.to_bits(), "energy len={len} pattern={pattern}");
                    match (want.1, got.1) {
                        (None, None) => {}
                        (Some(a), Some(b)) => {
                            assert_eq!(a.len(), b.len());
                            for (i, (x, y)) in a.iter().zip(&b).enumerate() {
                                assert_eq!(x.to_bits(), y.to_bits(), "gradient[{i}] len={len} pattern={pattern}");
                            }
                        }
                        _ => panic!("gradient presence differs"),
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked >= 300, "the sweep must not thin out: {checked}");
    }

    /// The ordering property the m1-11 variant got wrong: a repair writes flags
    /// *through* the view, so the refresh must come after it. This test performs
    /// the write, refreshes, and requires the cached read to match the oracle —
    /// and separately proves that skipping the refresh really would differ, so
    /// the test is not vacuous.
    #[test]
    fn a_repair_write_is_visible_only_after_the_post_repair_refresh() {
        let mut state = state_with(&[0u8; 8]);
        refresh_quadratic_flags(&mut state);
        assert_eq!(state.quadratic_flags[3], 0);
        // Exactly what `ContourRepair::mark` does through a `RepairView`.
        {
            let view = repair_view(&mut state);
            view.geometry.nodes[3].flags |= 0x16;
            view.penalty_counts[3] = 3;
        }
        let stale = objective_terms(&mut state, true, true).expect("stale");
        refresh_quadratic_flags(&mut state);
        let fresh = objective_terms(&mut state, true, true).expect("fresh");
        let oracle = objective_terms_reference(&mut state, true, true).expect("oracle");
        // After the refresh every channel must be the canonical recompute, bit for bit.
        assert_eq!(oracle.0.to_bits(), fresh.0.to_bits(), "energy after the refresh must match the oracle");
        let (want, got) = (oracle.1.expect("oracle gradient"), fresh.1.expect("fresh gradient"));
        assert_eq!(want.len(), got.len());
        for (i, (x, y)) in want.iter().zip(&got).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "gradient[{i}] after the refresh must match the oracle");
        }
        // And the control still bites: without the refresh the read differs.
        let stale_gradient = stale.1.expect("stale gradient");
        assert_ne!(stale.0.to_bits(), oracle.0.to_bits(),
            "without the refresh the energy read must differ, or this test proves nothing");
        assert!(stale_gradient.iter().zip(&want).any(|(x, y)| x.to_bits() != y.to_bits()),
            "without the refresh the gradient read must differ, or this test proves nothing");
    }
}

#[cfg(test)]
mod region_area_equivalence {
    //! The peeled `region_area` against the retained, verbatim modulo original.
    //!
    //! The change peels the wrap-around sample out of the loop and walks the
    //! rest as exact-length `windows(2)` pairs. The oracle below is the original
    //! loop, character for character, so a changed sample sequence or a changed
    //! summation order fails the bit comparison.
    use super::*;

    /// The original `region_area`, kept verbatim: modulo indexing, wrap sample
    /// visited first-in-order (last in the loop), two bounds-checked loads.
    fn region_area_reference(points: &[P], contour: &[usize]) -> f64 {
        let mut twice = 0.0;
        for s in 0..contour.len() {
            let c = points[contour[s]]; let n = points[contour[(s + 1) % contour.len()]];
            twice = (n[1] * c[0] - c[1] * n[0]) + twice;
        }
        twice * 0.5
    }

    /// Deterministic non-finite/negative-zero/denormal-heavy point streams, so
    /// the comparison covers the payload cases the ticket requires.
    fn point_stream(len: usize, seed: u64) -> Vec<P> {
        let mut state = seed | 1;
        let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
        let special = |bits: u64| f64::from_bits(bits);
        (0..len).map(|i| {
            let pick = next() % 12;
            let (x, y) = match pick {
                0 => (special(0x7ff8_0000_0000_0001), 1.5),          // NaN, payload 1
                1 => (f64::NEG_INFINITY, 0.0),
                2 => (f64::INFINITY, -0.0),                           // ±0 mix
                3 => (-0.0, 0.0),
                4 => (f64::from_bits(1), f64::from_bits(0x000f_ffff_ffff_ffff)), // denormals
                5 => (f64::MAX, f64::MIN_POSITIVE),
                _ => ((next() % 4096) as f64 * 0.25 - 512.0 + i as f64, (next() % 4096) as f64 * 0.125 - 256.0),
            };
            [x, y]
        }).collect()
    }

    #[test]
    fn peeled_wrap_sample_matches_the_retained_modulo_original_bit_for_bit() {
        let mut compared = 0usize;
        for &len in &[3usize, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129,
                      255, 256, 257] {
            for seed in [1u64, 7, 99, 0xdead_beef] {
                // A permutation of `0..len` as the contour: contours may repeat a
                // node and the order is load-bearing.
                let mut state = seed;
                let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
                let contour: Vec<usize> = (0..len).map(|_| (next() % len as u64) as usize).collect();
                // The identity contour, where every sample is distinct.
                let identity: Vec<usize> = (0..len).collect();
                let points = point_stream(len, seed);
                for c in [&contour, &identity] {
                    let got = region_area(&points, c);
                    let want = region_area_reference(&points, c);
                    assert_eq!(got.to_bits(), want.to_bits(),
                        "len={len} seed={seed} contour={c:?}: peeled {} vs original {}", got, want);
                    compared += 1;
                }
                // A repeated node every other position, which is legal and stresses
                // the wrap pair equal to a body pair.
                let repeated: Vec<usize> = (0..len).map(|i| i % 2).collect();
                let got = region_area(&points, &repeated);
                let want = region_area_reference(&points, &repeated);
                assert_eq!(got.to_bits(), want.to_bits(), "len={len} seed={seed} repeated");
                compared += 1;
            }
        }
        assert!(compared >= 250, "the corpus must actually run, compared={compared}");
    }

    #[test]
    fn region_area_accumulates_the_wrap_edge_last() {
        // The cross products are 2^54, -2^54 and 1. Adding the wrap edge
        // first loses the unit to rounding and incorrectly gives zero area.
        let large = (1_u64 << 54) as f64;
        let points = [[1.0, 0.0], [large, large], [0.0, -1.0]];
        assert_eq!(region_area(&points, &[0, 1, 2]), 0.5);
    }

    /// The original pressure-gradient loop, kept verbatim with its modulo
    /// indexing, as the exact oracle for the carried-index production loop.
    fn pressure_gradient_reference(gradient: &mut [f64], points: &[P], c: &[usize], factor: f64) {
        for s in 0..c.len() {
            let current = c[s]; let next = c[(s + 1) % c.len()];
            gradient[current * 2] = points[next][1] * 0.5 * factor + gradient[current * 2];
            gradient[current * 2 + 1] = points[next][0] * -0.5 * factor + gradient[current * 2 + 1];
            gradient[next * 2] = points[current][1] * -0.5 * factor + gradient[next * 2];
            gradient[next * 2 + 1] = points[current][0] * 0.5 * factor + gradient[next * 2 + 1];
        }
    }

    /// A deliberately wrong carry (it starts at the current sample, so every
    /// vertex is paired with itself), for the negative control below.
    fn pressure_gradient_wrong_carry(gradient: &mut [f64], points: &[P], c: &[usize], factor: f64, start: usize) {
        let mut next_index = start;
        for s in 0..c.len() {
            let current = c[s]; let next = c[next_index];
            gradient[current * 2] = points[next][1] * 0.5 * factor + gradient[current * 2];
            gradient[current * 2 + 1] = points[next][0] * -0.5 * factor + gradient[current * 2 + 1];
            gradient[next * 2] = points[current][1] * -0.5 * factor + gradient[next * 2];
            gradient[next * 2 + 1] = points[current][0] * 0.5 * factor + gradient[next * 2 + 1];
            next_index = if next_index + 1 == c.len() { 0 } else { next_index + 1 };
        }
    }

    #[test]
    fn the_carried_pressure_gradient_matches_the_modulo_original_bit_for_bit() {
        let mut compared = 0usize;
        for &len in &[3usize, 4, 5, 6, 7, 8, 9, 15, 16, 17, 29, 31, 32, 33, 64, 65, 129, 257] {
            for seed in [1u64, 7, 99, 0xdead_beef] {
                let mut state = seed;
                let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
                let points = point_stream(len, seed);
                let contour: Vec<usize> = (0..len).map(|_| (next() % len as u64) as usize).collect();
                let identity: Vec<usize> = (0..len).collect();
                let repeated: Vec<usize> = (0..len).map(|i| i % 2).collect();
                for c in [&contour, &identity, &repeated] {
                    for &factor in &[0.0, -0.0, 1.0, -2.5, f64::NAN, f64::INFINITY, 1e-300] {
                        // A non-zero starting gradient, so the `+ gradient[...]`
                        // accumulation order is observable too.
                        let start: Vec<f64> = (0..points.len() * 2)
                            .map(|i| ((i as f64) * 0.37).sin() * 3.0).collect();
                        let mut got = start.clone();
                        let mut want = start.clone();
                        add_pressure_gradient(&mut got, &points, c, factor);
                        pressure_gradient_reference(&mut want, &points, c, factor);
                        assert_eq!(got.len(), want.len());
                        for (i, (a, b)) in got.iter().zip(&want).enumerate() {
                            assert_eq!(a.to_bits(), b.to_bits(),
                                "gradient[{i}] len={len} seed={seed} factor={factor} contour={c:?}");
                        }
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared >= 1000, "the corpus must actually run, compared={compared}");
    }

    /// Negative control: a carry that starts at the current sample (so every
    /// vertex is paired with itself) must change the gradient on this corpus,
    /// so the equivalence test above cannot be vacuous.
    #[test]
    fn a_wrong_pressure_carry_is_observable_on_this_corpus() {
        let mut observable = 0usize;
        for &len in &[3usize, 4, 5, 17, 29] {
            for seed in [1u64, 7, 99, 0xdead_beef] {
                let mut state = seed;
                let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
                let points = point_stream(len, seed);
                let contour: Vec<usize> = (0..len).map(|_| (next() % len as u64) as usize).collect();
                let start = vec![0.0f64; points.len() * 2];
                let mut got = start.clone();
                let mut want = start.clone();
                pressure_gradient_wrong_carry(&mut got, &points, &contour, 1.5, 1);
                pressure_gradient_wrong_carry(&mut want, &points, &contour, 1.5, 0);
                if got.iter().zip(&want).any(|(a, b)| a.to_bits() != b.to_bits()) {
                    observable += 1;
                }
            }
        }
        assert!(observable > 0, "a wrong carry must be observable on this corpus");
    }
}
