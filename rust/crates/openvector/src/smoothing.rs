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

fn curvature_terms(first: P, middle: P, last: P, counts: [f64; 2], length_weight: f64, include_gradient: bool) -> Terms {
    let incoming_x = middle[0] - first[0]; let incoming_y = middle[1] - first[1];
    let outgoing_x = last[0] - middle[0]; let outgoing_y = last[1] - middle[1];
    let incoming_length = (incoming_x * incoming_x + incoming_y * incoming_y).sqrt();
    let outgoing_length = (outgoing_x * outgoing_x + outgoing_y * outgoing_y).sqrt();
    let incoming_inverse = 1.0 / incoming_length; let outgoing_inverse = 1.0 / outgoing_length;
    let iux = incoming_x * incoming_inverse; let iuy = incoming_y * incoming_inverse;
    let oux = outgoing_x * outgoing_inverse; let ouy = outgoing_y * outgoing_inverse;
    let dot = oux * iux + ouy * iuy;
    let curvature = ((2.0 - (dot + dot)) + 0.001).sqrt();
    let length_difference = outgoing_length / counts[1] - incoming_length / counts[0];
    let energy = (length_difference * length_weight) * length_difference + curvature;
    if !include_gradient { return (energy, None); }
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
    (energy, Some(([fx, fy], [lx, ly])))
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
    border_counts: [usize; 4],
    settings: Settings,
    image_kernel: Option<&'a mut ImageKernel>,
    image_positions: Vec<f64>,
}

fn num(v: &Value) -> f64 { v.as_f64().unwrap_or(f64::NAN) }
fn is_int(v: f64) -> bool { v.is_finite() && v.fract() == 0.0 }

fn read_objective_input<'a>(input: &mut SmoothingInput<'a>) -> Result<ObjectiveState<'a>, String> {
    let phase = input.phase;
    if !(0..=2).contains(&phase) { return Err("Smoothing phase must be 0, 1 or 2.".into()); }
    let geometry = input.geometry;
    if geometry.nodes.len() < 4 || geometry.regions.is_empty() { return Err("Smoothing objective needs nodes and region contours.".into()); }
    let node_count = geometry.nodes.len();
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
    Ok(ObjectiveState { geometry: geometry.clone(), points, references, penalty_counts: input.penalty_counts.to_vec(),
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
    gradient[first * 2] = fx + gradient[first * 2];
    gradient[first * 2 + 1] = fy + gradient[first * 2 + 1];
    gradient[middle * 2] -= fx;
    gradient[middle * 2 + 1] -= fy;
    gradient[middle * 2] -= lx;
    gradient[middle * 2 + 1] -= ly;
    gradient[last * 2] = lx + gradient[last * 2];
    gradient[last * 2 + 1] = ly + gradient[last * 2 + 1];
}

fn region_area(points: &[P], contour: &[usize]) -> f64 {
    let mut twice = 0.0;
    for s in 0..contour.len() {
        let c = points[contour[s]]; let n = points[contour[(s + 1) % contour.len()]];
        twice = (n[1] * c[0] - c[1] * n[0]) + twice;
    }
    twice * 0.5
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective { pub energy: f64, pub gradient: Vec<f64> }

/// Original prepared objective, including optional persistent image measurement.
pub fn smoothing_objective(input: &mut SmoothingInput) -> Result<Objective, String> {
    let mut state = read_objective_input(input)?;
    let (e, g) = objective_terms(&mut state, true, true)?;
    Ok(Objective { energy: e, gradient: g.unwrap() })
}

fn energy_only(state: &mut ObjectiveState) -> Result<f64, String> { Ok(objective_terms(state, true, false)?.0) }

fn objective_terms(state: &mut ObjectiveState, include_energy: bool, include_gradient: bool) -> Result<(f64, Option<Vec<f64>>), String> {
    let ObjectiveState { geometry, points, references, penalty_counts, border_counts, settings, image_kernel, image_positions } = state;
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

#[derive(Clone, Debug)]
struct LineControls { width: f64, height: f64, near_border_nodes: Vec<usize>, direction: Vec<f64>, epsilon: f64,
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
        direction: direction.to_vec(), epsilon, minimum, maximum, max_step, fraction, tolerance,
        max_iterations: max_iterations as i64, use_step: use_step == 1.0 })
}

fn clamp_interior_coordinate(coordinate: f64, maximum: f64) -> f64 {
    if 0.01 <= coordinate { return if maximum < coordinate { maximum } else { coordinate }; }
    0.01
}

fn move_points(points: &mut [P], border_counts: [usize; 4], c: &LineControls, step: f64) {
    for (node, p) in points.iter_mut().enumerate() {
        p[0] = step * c.direction[node * 2] + p[0];
        p[1] = step * c.direction[node * 2 + 1] + p[1];
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

struct Line { energy: f64, delta: f64, epsilon: f64, step: f64, iterations: i64 }

/// Original quadratic line search on an explicit prepared geometric objective.
pub fn smoothing_line_search(input: &mut SmoothingInput) -> Result<LineResult, String> {
    let mut state = read_objective_input(input)?;
    let controls = read_line_search_input(input, &state, input.direction, input.epsilon)?;
    let initial = energy_only(&mut state)?;
    let line = quadratic_line(&mut state, &controls, initial)?;
    Ok(LineResult { positions: state.points.iter().flatten().copied().collect(), energy: line.energy, delta: line.delta,
        epsilon: line.epsilon, step: line.step, iterations: line.iterations })
}

fn quadratic_line(state: &mut ObjectiveState, c: &LineControls, initial_energy: f64) -> Result<Line, String> {
    let mut max_direction = 0.0f64;
    for v in &c.direction { if v.abs() > max_direction { max_direction = v.abs(); } }
    let mut epsilon = c.epsilon;
    if c.max_step > 0.0 && c.max_step < max_direction * epsilon { epsilon = c.max_step / max_direction; }
    let initial_epsilon = epsilon;
    let mut energy = initial_energy; let mut base_energy; let mut step = 0.0; let mut iterations = 0i64;
    let borders = state.border_counts;
    loop {
        let backup = state.points.clone();
        base_energy = energy;
        move_points(&mut state.points, borders, c, epsilon);
        let first_energy = energy_only(state)?;
        move_points(&mut state.points, borders, c, epsilon);
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
        if offset != 0.0 {
            move_points(&mut state.points, borders, c, offset);
            candidate_energy = energy_only(state)?;
        }
        let mut best_energy = base_energy; let mut best_index = 0.0;
        if first_energy < base_energy { best_energy = first_energy; best_index = 1.0; }
        if second_energy < best_energy { best_energy = second_energy; best_index = 2.0; }
        let accepted_step;
        if candidate_energy < best_energy {
            accepted_step = epsilon + epsilon + offset;
            energy = candidate_energy;
        } else {
            accepted_step = best_index * epsilon;
            state.points = backup;
            move_points(&mut state.points, borders, c, accepted_step);
            energy = energy_only(state)?;
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
    Ok(Line { energy, delta: energy - base_energy, epsilon, step, iterations })
}

struct Cache { seeing: i64, blind: i64, blend: f64, position: i64, average: f64 }

fn cached_line(state: &mut ObjectiveState, c: &LineControls, cache: &mut Cache, initial_energy: f64, previous_step: f64, iteration: i64) -> Result<Line, String> {
    cache.position %= cache.seeing + cache.blind;
    let mut energy = initial_energy;
    let mut line = Line { energy, delta: 0.0, epsilon: c.epsilon, step: previous_step, iterations: 0 };
    if cache.position >= cache.seeing {
        let backup = state.points.clone();
        let mut offset = cache.average; let mut max_direction = 0.0f64;
        for v in &c.direction { if v.abs() > max_direction { max_direction = v.abs(); } }
        if c.max_step > 0.0 {
            if c.max_step < offset * max_direction { offset = c.max_step / max_direction; }
            if c.max_step < -(offset * max_direction) { offset = -(c.max_step / max_direction); }
        }
        let borders = state.border_counts;
        move_points(&mut state.points, borders, c, offset * 0.8);
        energy = energy_only(state)?;
        if initial_energy < energy {
            state.points = backup;
            energy = initial_energy;
            cache.position = 0;
        }
        line.energy = energy;
    }
    if cache.position < cache.seeing {
        line = quadratic_line(state, c, energy)?;
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

/// Original prepared Fletcher-Reeves controller, before multi-phase smoothing.
pub fn smoothing_optimize(input: &mut SmoothingInput, mut on_iteration: Option<&mut dyn FnMut(i64, &ObjectiveState) -> Result<(), String>>)
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
    let (mut energy, g) = objective_terms(&mut state, true, true)?;
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
        controls.direction.copy_from_slice(&direction);
        let line = match cache.as_mut() {
            Some(cache) => cached_line(&mut state, &controls, cache, energy, step, iterations),
            None => quadratic_line(&mut state, &controls, energy),
        }?;
        energy = line.energy; epsilon = line.epsilon; step = line.step; line_iterations += line.iterations;
        if let Some(repair) = repair.as_mut() {
            let changed = if iterations % repair_interval == repair_interval - 1 {
                repair.all(&mut repair_view(&mut state)) > 0
            } else {
                let ids = state.image_kernel.as_mut().expect("repair kernel").take_invalid_regions();
                repair.invalid(&mut repair_view(&mut state), &ids)
            };
            if changed {
                beta = 0.0;
                energy = energy_only(&mut state)?;
                baseline = energy;
                for ch in 0..direction.len() { direction[ch] = -stored_gradient[ch]; }
                restarts += 1;
            }
        }
        if let Some(cb) = on_iteration.as_mut() { cb(iterations, &state)?; }
        let improvement = baseline - energy;
        let relative = if baseline <= 0.0 { 0.0 } else { improvement / baseline };
        if relative < relative_tolerance || (improvement < absolute_tolerance && improvement >= 0.0) {
            countdown += 1;
            if knockout < countdown && min_iterations < iterations { break; }
        } else { countdown = 0; }
        stored_gradient = objective_terms(&mut state, false, true)?.1.unwrap();
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
