//! Port of `src/contour-initialization.mjs`.
use crate::blend_solver::{collect_blend_indices, fit_prepared_blend, write_blend_sample, EIGHT_DIRECTIONS};
use crate::contour_neighborhoods::prepare_contour_neighborhoods;
use crate::contours::Geometry;
use crate::pixel_input::read_palette_colors;
use crate::region_data::read_raster_input;
use serde_json::{json, Value};

const BORDER_THRESHOLD: f64 = 1.1f32 as f64;

fn read_contour_dimensions(width: i64, height: i64) -> Result<(i64, i64), String> {
    if width < 2 || height < 2 || width > 2147483647 || height > 2147483647 {
        return Err("Contour preparation needs int32 dimensions of at least two.".into());
    }
    Ok((width, height))
}

fn read_fixed_border_count(border_counts: &[i64], node_count: usize) -> Result<usize, String> {
    if border_counts.len() != 4 || border_counts.iter().any(|&c| c < 0) || border_counts[0] < 2 || border_counts[1] < 2 {
        return Err("Contour preparation needs the four fixed border counts.".into());
    }
    let border: i64 = border_counts.iter().sum();
    if border > node_count as i64 { return Err("Border counts must fit the shared nodes.".into()); }
    Ok(border as usize)
}

fn collect_near_border_nodes(width: i64, height: i64, geometry: &Geometry, border: usize) -> Vec<i32> {
    let right = width as f64 - BORDER_THRESHOLD; let bottom = height as f64 - BORDER_THRESHOLD;
    (border..geometry.nodes.len()).filter(|&n| {
        let (x, y) = (geometry.nodes[n].x, geometry.nodes[n].y);
        x < BORDER_THRESHOLD || right < x || y < BORDER_THRESHOLD || bottom < y
    }).map(|n| n as i32).collect()
}

/// Record the fixed near-border list before optimization moves any nodes.
pub fn select_near_border_nodes(width: i64, height: i64, geometry: &Geometry, border_counts: &[i64]) -> Result<Vec<i32>, String> {
    let (width, height) = read_contour_dimensions(width, height)?;
    if geometry.nodes.len() < 4 { return Err("Contour preparation needs shared nodes.".into()); }
    let border = read_fixed_border_count(border_counts, geometry.nodes.len())?;
    if geometry.nodes.iter().any(|n| !n.x.is_finite() || !n.y.is_finite()) {
        return Err("Near-border selection needs finite node coordinates.".into());
    }
    Ok(collect_near_border_nodes(width, height, geometry, border))
}

/// Raw grid geometry and fixed caches used by all smoothing phases.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourSmoothingState {
    pub geometry: Geometry,
    pub reference_positions: Vec<f32>,
    pub penalty_counts: Vec<u8>,
    pub border_counts: [i64; 4],
    pub near_border_nodes: Vec<i32>,
    pub repair_interval: i64,
    pub nearby_edges: Option<Vec<Vec<[usize; 2]>>>,
}

impl ContourSmoothingState {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"geometry": self.geometry.to_json(), "borderCounts": self.border_counts.to_vec(),
            "referencePositions": self.reference_positions, "penaltyCounts": self.penalty_counts,
            "nearBorderNodes": self.near_border_nodes, "repairInterval": self.repair_interval});
        if let Some(e) = &self.nearby_edges {
            v["nearbyEdges"] = json!(e.iter().map(|l| l.iter().map(|p| p.to_vec()).collect::<Vec<_>>()).collect::<Vec<_>>());
        }
        v
    }
    pub fn reference_f64(&self) -> Vec<f64> { self.reference_positions.iter().map(|&v| v as f64).collect() }
}

/// Own the raw grid geometry; `anti_alias` None mirrors a missing boolean.
pub fn create_contour_smoothing_state(width: i64, height: i64, anti_alias: Option<bool>, geometry: &Geometry) -> Result<ContourSmoothingState, String> {
    let (width, height) = read_contour_dimensions(width, height)?;
    if anti_alias.is_none() { return Err("Contour preparation requires antiAlias: true or false.".into()); }
    if geometry.nodes.len() < 4 || geometry.regions.is_empty() { return Err("Contour preparation needs raw shared geometry.".into()); }
    let geometry = geometry.clone();
    let mut border_counts = [0i64; 4];
    let mut reference_positions = vec![0f32; geometry.nodes.len() * 2];
    let mut previous_group = 0;
    let (w, h) = (width as f64, height as f64);
    for (index, node) in geometry.nodes.iter().enumerate() {
        if !node.x.is_finite() || !node.y.is_finite() || node.x.fract() != 0.0 || node.y.fract() != 0.0 ||
            node.x < 0.0 || node.x > w || node.y < 0.0 || node.y > h || (node.kind != 0 && node.kind != 3) {
            return Err("Contour preparation needs integer-grid nodes, raw kinds and byte flags.".into());
        }
        let group = if node.x == 0.0 { 0 } else if node.x == w { 1 } else if node.y == 0.0 { 2 } else if node.y == h { 3 } else { 4 };
        if group < previous_group { return Err("Raw contour nodes must follow left/right/top/bottom/interior order.".into()); }
        previous_group = group;
        if group < 4 { border_counts[group] += 1; }
        reference_positions[index * 2] = node.x as f32;
        reference_positions[index * 2 + 1] = node.y as f32;
    }
    let border = read_fixed_border_count(&border_counts, geometry.nodes.len())?;
    let near_border_nodes = collect_near_border_nodes(width, height, &geometry, border);
    let penalty_counts = vec![0; geometry.nodes.len()];
    Ok(ContourSmoothingState { geometry, reference_positions, penalty_counts, border_counts, near_border_nodes, repair_interval: 10, nearby_edges: None })
}

/// Native 0x4822b0 writes neighborhood flags after the first preparation checkpoint.
pub fn initialize_smoothing_neighborhoods(state: &mut ContourSmoothingState, anti_alias: bool) -> Result<(), String> {
    if anti_alias {
        let prepared = prepare_contour_neighborhoods(&state.geometry).map_err(|e| e.0)?;
        for (node, &flags) in state.geometry.nodes.iter_mut().zip(&prepared.flags) { node.flags = flags; }
        state.nearby_edges = Some(prepared.nearby_edges);
    } else {
        state.nearby_edges = Some(vec![Vec::new(); state.geometry.nodes.len()]);
    }
    Ok(())
}

/// Prepare all fixed caches for callers that do not observe interruption.
pub fn prepare_contour_smoothing(width: i64, height: i64, anti_alias: Option<bool>, geometry: &Geometry) -> Result<ContourSmoothingState, String> {
    let mut state = create_contour_smoothing_state(width, height, anti_alias, geometry)?;
    initialize_smoothing_neighborhoods(&mut state, anti_alias.unwrap())?;
    Ok(state)
}

/// Inputs to blend preadjustment and coordinate initialization.
pub struct InitializationInput<'a> {
    pub width: i64,
    pub height: i64,
    pub bgra: &'a [u8],
    pub geometry: &'a Geometry,
    pub indices: &'a [i64],
    pub reference_positions: &'a [f64],
    pub border_counts: &'a [i64],
    pub parameters: &'a Value,
    pub palette: &'a [f64],
}

struct Initialization<'a> {
    width: usize, height: usize, bgra: &'a [u8], palette: Vec<f32>, palette_indices: Vec<i32>, positions: Vec<f64>,
    references: Vec<f32>, border: usize, anti_alias: bool, amplitude: f64,
}

fn read_initialization<'a>(input: &InitializationInput<'a>) -> Result<Initialization<'a>, String> {
    let raster = read_raster_input(input.width, input.height, input.bgra).map_err(|e| e.0)?;
    let palette = read_palette_colors(input.palette).map_err(|e| e.0)?;
    let geometry = input.geometry;
    if raster.width < 2 || raster.height < 2 || geometry.nodes.len() < 4 || geometry.regions.is_empty() {
        return Err("Contour initialization needs dimensions of at least two and shared geometry.".into());
    }
    let mut positions = Vec::with_capacity(geometry.nodes.len() * 2);
    for n in &geometry.nodes {
        if !n.x.is_finite() || !n.y.is_finite() { return Err("Contour initialization needs finite node coordinates.".into()); }
        positions.push(n.x); positions.push(n.y);
    }
    if input.reference_positions.len() != positions.len() { return Err("Contour initialization needs two float32 references per node.".into()); }
    for &v in input.reference_positions {
        if !v.is_finite() || !(v as f32).is_finite() { return Err("Contour references must fit finite float32 coordinates.".into()); }
    }
    let references: Vec<f32> = input.reference_positions.iter().map(|&v| v as f32).collect();
    let b = input.border_counts;
    if b.len() != 4 || b.iter().any(|&c| c < 0) || b[0] < 2 || b[1] < 2 {
        return Err("Contour initialization needs the four fixed border counts.".into());
    }
    let border: i64 = b.iter().sum();
    if border > geometry.nodes.len() as i64 { return Err("Border counts must fit the shared nodes.".into()); }
    let border = border as usize;
    let (width, height) = (raster.width as i64, raster.height as i64);
    for node in border..geometry.nodes.len() {
        let column = references[node * 2].trunc() as i64 - 1; let row = references[node * 2 + 1].trunc() as i64 - 1;
        if column < 0 || row < 0 || column + 1 >= width || row + 1 >= height {
            return Err("Interior node references must identify a complete original 2 by 2 pixel joint.".into());
        }
    }
    if input.indices.len() != raster.width * raster.height { return Err("Contour initialization needs one completed region label per pixel.".into()); }
    let anti_alias = input.parameters.get("Shared::is_anti_aliased").and_then(Value::as_f64);
    let amplitude = input.parameters.get("ContourSmoother::perturbation_range").and_then(Value::as_f64).unwrap_or(f64::NAN);
    if (anti_alias != Some(0.0) && anti_alias != Some(1.0)) || !amplitude.is_finite() || amplitude < 0.0 {
        return Err("Contour initialization needs an anti-alias flag and finite nonnegative perturbation range.".into());
    }
    let anti_alias = anti_alias == Some(1.0);
    let mut color_indices = Vec::with_capacity(geometry.regions.len());
    for r in &geometry.regions {
        if r.color_index < 0 || (anti_alias && r.color_index as usize >= palette.len() / 4) {
            return Err("Contour region colors must identify the completed palette.".into());
        }
        color_indices.push(r.color_index);
    }
    let mut palette_indices = Vec::with_capacity(input.indices.len());
    for &r in input.indices {
        if r < 0 || r as usize >= color_indices.len() { return Err("Contour labels must identify existing regions.".into()); }
        palette_indices.push(color_indices[r as usize]);
    }
    Ok(Initialization { width: raster.width, height: raster.height, bgra: raster.bgra, palette, palette_indices, positions,
        references, border, anti_alias, amplitude })
}

fn preadjust_node(s: &mut Initialization, node: usize) -> Result<(), String> {
    let diagonal = 1.0 / 2f64.sqrt();
    let width = s.width;
    let column = s.references[node * 2].trunc() as usize - 1; let row = s.references[node * 2 + 1].trunc() as usize - 1;
    let first = row * width + column;
    let pi = &s.palette_indices;
    let quad = [pi[first], pi[first + 1], pi[first + width], pi[first + width + 1]];
    let mut totals = [0.0f64; 4];
    let mut sample = [0.0f64; 4];
    for dy in 0..2 {
        for dx in 0..2 {
            let pixel = (row + dy) * width + column + dx;
            let selected = collect_blend_indices(pi, width, s.height, pixel, &EIGHT_DIRECTIONS, -1, -1);
            let mut colors = vec![0f32; selected.len() * 4];
            for (index, &color) in selected.iter().enumerate() {
                colors[index * 4..index * 4 + 4].copy_from_slice(&s.palette[color as usize * 4..color as usize * 4 + 4]);
            }
            write_blend_sample(s.bgra, pixel * 4, &mut sample);
            let weights = fit_prepared_blend(&sample, &colors)?.weights;
            for color in quad {
                if let Some(index) = selected.iter().position(|&c| c == color) {
                    if index > 0 { totals[dy * 2 + dx] = weights[index] + totals[dy * 2 + dx]; }
                }
            }
        }
    }
    let a = totals[0] - totals[3]; let b = totals[2] - totals[1];
    let x = (b * -diagonal + a * -diagonal) * 0.5;
    let y = (a * -diagonal + b * diagonal) * 0.5;
    s.positions[node * 2] = clamp_shift(x) + s.positions[node * 2];
    s.positions[node * 2 + 1] = clamp_shift(y) + s.positions[node * 2 + 1];
    Ok(())
}

/// `Math.max(-0.333, Math.min(0.333, v))`, propagating NaN like JS.
fn clamp_shift(v: f64) -> f64 {
    if v.is_nan() { return v; }
    let low = if v < 0.333 { v } else { 0.333 };
    if low > -0.333 { low } else { -0.333 }
}

/// Original deterministic blend-driven adjustment, without PRNG consumption.
pub fn preadjust_contours(input: &InitializationInput) -> Result<Vec<f64>, String> {
    let mut s = read_initialization(input)?;
    if s.anti_alias { for node in s.border..s.positions.len() / 2 { preadjust_node(&mut s, node)?; } }
    Ok(s.positions)
}

/// Per-node adjustment and both draws in original order on the session stream.
pub fn initialize_contour_coordinates(input: &InitializationInput, next_uniform: &mut dyn FnMut() -> f64) -> Result<Vec<f64>, String> {
    let mut s = read_initialization(input)?;
    if s.anti_alias {
        for node in s.border..s.positions.len() / 2 {
            preadjust_node(&mut s, node)?;
            let x = next_uniform(); let y = next_uniform(); let range = s.amplitude + s.amplitude;
            s.positions[node * 2] = (x * range - s.amplitude) + s.positions[node * 2];
            s.positions[node * 2 + 1] = (range * y - s.amplitude) + s.positions[node * 2 + 1];
        }
    }
    Ok(s.positions)
}
