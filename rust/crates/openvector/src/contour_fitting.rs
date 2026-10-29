//! Port of `src/contour-fitting.mjs`.
use crate::contours::Geometry;
use crate::fitting::{fit_contour_interval, partition_contour_interval, FitNode, Point};
use serde_json::Value;
use serde::{Deserialize, Serialize};

/// `BezierFitter::*` controls. Iterations stay f64 so non-integers are rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FittingParameters { pub stat_thresh_initial: f64, pub stat_thresh_final: f64, pub max_iterations: f64 }

impl FittingParameters {
    /// Read from a resolved parameter object; absent values become NaN and fail validation.
    pub fn from_json(parameters: &Value) -> Self {
        let read = |key: &str| parameters.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN);
        FittingParameters { stat_thresh_initial: read("BezierFitter::stat_thresh_initial"),
            stat_thresh_final: read("BezierFitter::stat_thresh_final"), max_iterations: read("BezierFitter::max_iterations") }
    }
}

/// Edge: [index, type, neighbor, firstNode, lastNode, firstSample, lastSample].
pub type FittedEdge = [i64; 7];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitRegion {
    pub bgra: [u8; 4], pub pixel_count: i32, pub color_index: i32, pub parent: i32,
    pub contour: Vec<usize>,
    #[serde(skip)]
    pub boundary: Vec<[i32; 3]>,
    pub edges: Vec<FittedEdge>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cubic { pub index: usize, pub points: [f64; 8] }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitterSnapshot { pub cubic_storage_slots: usize, pub cubic_availability: Vec<u8>, pub cubics: Vec<Cubic>,
    pub nodes: Vec<FitNode>, pub regions: Vec<FitRegion> }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FittedGeometry { pub cubic_storage_slots: usize, pub cubics: Vec<Cubic>, pub nodes: Vec<FitNode>, pub regions: Vec<FitRegion> }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FittingProgress { pub phase: i32, pub substep: i32, pub counter: i32, pub fraction: f64, pub percent: i32 }

pub type FittingObserver<'o> = dyn FnMut(&FittingProgress, &ContourFitter) -> Result<(), String> + 'o;

impl FittedGeometry {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        Self::deserialize(value).map_err(|error| format!("Invalid fitted geometry: {error}"))
    }
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("fitted geometry is serializable")
    }
}

impl FitterSnapshot {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("fitter snapshot is serializable")
    }
}

fn marked_sample(nodes: &[FitNode], contour: &[usize], sample: usize, direction: i64, skipped_kind: u8) -> usize {
    let len = contour.len() as i64;
    let original = contour[sample % contour.len()];
    let mut current = len + sample as i64;
    loop {
        current += direction;
        if nodes[contour[(current % len) as usize]].kind != skipped_kind { break; }
        if contour[(current % len) as usize] == original { break; }
    }
    (current % len) as usize
}

fn select_intervals(fitter: &mut ContourFitter, p: &FittingParameters,
    checkpoint: &mut dyn FnMut(i32, i32, &ContourFitter) -> Result<(), String>) -> Result<usize, String> {
    let mut slots = 0usize;
    for index in 0..fitter.regions.len() {
        if index % 4 == 0 { checkpoint(1, ((index * 50) as f64 / fitter.regions.len() as f64 + 0.5).floor() as i32, fitter)?; }
        let nodes = &mut fitter.nodes;
        let region = &fitter.regions[index];
        let contour = &region.contour; let len = contour.len() as i64;
        for sample in 0..contour.len() {
            if nodes[contour[sample]].kind != 0 { continue; }
            let start = marked_sample(nodes, contour, sample, -1, 0) as i64;
            let mut end = marked_sample(nodes, contour, sample, 1, 0) as i64;
            while end <= start { end += len; }
            let records = partition_contour_interval(nodes, contour, start, end, p.stat_thresh_initial,
                p.stat_thresh_final, p.max_iterations as i64, 0.5)?.records;
            let mut current = start;
            for (i, record) in records.iter().enumerate() {
                let next = if i + 1 < records.len() { records[i + 1].start } else { end - start + 1 };
                nodes[contour[(current % len) as usize]].kind = 5; current += 1;
                for _ in record.start + 1..next {
                    let node = &mut nodes[contour[(current % len) as usize]]; current += 1;
                    node.kind = 1; node.cubic = slots as i32;
                }
                slots += 1;
            }
            nodes[contour[(start % len) as usize]].kind = 4;
            nodes[contour[(end % len) as usize]].kind = 4;
        }
    }
    Ok(slots)
}

fn fit_intervals(nodes: &mut [FitNode], regions: &[FitRegion], curves: &mut [Option<Vec<Point>>]) {
    for region in regions {
        let contour = &region.contour; let len = contour.len() as i64;
        for sample in 0..contour.len() {
            if nodes[contour[sample]].kind != 1 { continue; }
            let cubic = nodes[contour[sample]].cubic as usize;
            let start = marked_sample(nodes, contour, sample, -1, 1) as i64;
            let mut end = marked_sample(nodes, contour, sample, 1, 1) as i64;
            while end <= start { end += len; }
            curves[cubic] = Some(fit_contour_interval(nodes, contour, start, end, true).curve);
        }
    }
}

fn fitted_edges(nodes: &[FitNode], region: &FitRegion, curves: &[Option<Vec<Point>>]) -> Vec<FittedEdge> {
    let contour = &region.contour; let cycle = contour.len();
    let mut edges = Vec::new();
    let mut start = cycle;
    while nodes[contour[start % cycle]].kind == 2 { start -= 1; }
    start %= cycle;
    let mut current = start;
    while current < start + cycle {
        let sample = current % cycle; let previous = (current + cycle - 1) % cycle; let next = (current + 1) % cycle;
        let node = &nodes[contour[sample]]; let neighbor = region.boundary[sample][1] as i64;
        if node.kind != 2 && nodes[contour[next]].kind != 2 {
            edges.push([sample as i64, 0, neighbor, contour[sample] as i64, contour[next] as i64, sample as i64, next as i64]);
        }
        if node.kind == 2 {
            let index = node.cubic; let first = &nodes[contour[previous]];
            let curve = curves[index as usize].as_ref().expect("fitted cubic");
            while nodes[contour[current % cycle]].kind == 2 { current += 1; }
            let end = current % cycle;
            let reversed = (curve[0][0] - first.x).abs() > 9.999999974752427e-7 || (curve[0][1] - first.y).abs() > 9.999999974752427e-7;
            edges.push([index as i64, if reversed { 2 } else { 1 }, neighbor, contour[previous] as i64, contour[end] as i64, previous as i64, end as i64]);
            current -= 1;
        }
        current += 1;
    }
    edges
}

fn validate_parameters(p: &FittingParameters) -> Result<(), String> {
    let (i, f, n) = (p.stat_thresh_initial, p.stat_thresh_final, p.max_iterations);
    if !i.is_finite() || i <= 0.0 || !f.is_finite() || f <= 0.0 || !(f / i).is_finite() || f / i <= 0.0 ||
        !n.is_finite() || n.fract() != 0.0 || n < 0.0 || n > 2147483647.0 {
        return Err("Fitting needs positive finite thresholds with a finite ratio and a nonnegative int32 iteration count.".into());
    }
    Ok(())
}

fn validate_geometry(g: &Geometry) -> Result<(), String> {
    if g.nodes.is_empty() || g.regions.is_empty() { return Err("Fitting needs shared nodes and completed region contours.".into()); }
    if g.nodes.iter().any(|n| !n.x.is_finite() || !n.y.is_finite()) {
        return Err("Fitting needs finite node coordinates and byte kinds.".into());
    }
    let rc = g.regions.len() as i64;
    for r in &g.regions {
        if r.pixel_count < 1 || r.color_index < 0 || r.parent < -1 || r.parent as i64 >= rc ||
            r.contour.len() < 3 || r.boundary.len() != r.contour.len() {
            return Err("Fitting needs region colors, counts, parents and matching contour boundary records.".into());
        }
        for (s, &node) in r.contour.iter().enumerate() {
            let b = r.boundary[s];
            if node >= g.nodes.len() || b[0] < 0 || b[0] > 255 || b[1] < -1 || b[1] as i64 >= rc || b[2] < 1 {
                return Err("Fitting needs existing contour identifiers and valid boundary flags, neighbors and sample counts.".into());
            }
        }
    }
    Ok(())
}

fn fitting_range(substep: i32) -> (f64, f64) {
    match substep {
        -1 => (0.004614000208675861, 0.004614000208675861),
        0 => (0.005648000165820122, 0.005648000165820122),
        1 => (0.00572500005364418, 0.9383379817008972),
        2 => (0.9716119766235352, 0.9716119766235352),
        3 => (0.9902650117874146, 0.9902650117874146),
        _ => (0.994268000125885, 0.994268000125885),
    }
}

fn own_nodes(g: &Geometry) -> Vec<FitNode> { g.nodes.iter().map(|n| FitNode { x: n.x, y: n.y, kind: n.kind, cubic: -1 }).collect() }
fn own_regions(g: &Geometry) -> Vec<FitRegion> {
    g.regions.iter().map(|r| FitRegion { bgra: r.bgra, pixel_count: r.pixel_count, color_index: r.color_index, parent: r.parent,
        contour: r.contour.clone(), boundary: r.boundary.clone(), edges: Vec::new() }).collect()
}

/// One live fitting graph across runs. Observer errors stop the current run
/// without rolling back nodes, retained edges or cubic storage.
#[derive(Clone, Debug)]
pub struct ContourFitter { nodes: Vec<FitNode>, regions: Vec<FitRegion>, curves: Vec<Option<Vec<Point>>> }

impl ContourFitter {
    pub fn new(geometry: &Geometry) -> Result<Self, String> {
        let mut f = ContourFitter { nodes: Vec::new(), regions: Vec::new(), curves: Vec::new() };
        f.set_regions(geometry)?; Ok(f)
    }

    pub fn set_regions(&mut self, geometry: &Geometry) -> Result<(), String> {
        validate_geometry(geometry)?;
        self.nodes = own_nodes(geometry); self.regions = own_regions(geometry); Ok(())
    }

    pub fn set_contours(&mut self, geometry: &Geometry) -> Result<(), String> {
        validate_geometry(geometry)?;
        if geometry.regions.len() != self.regions.len() {
            return Err("Contour replacement must preserve region identities. Use setRegions after region reconstruction.".into());
        }
        let mut regions = own_regions(geometry);
        for (r, old) in regions.iter_mut().zip(self.regions.iter_mut()) { r.edges = std::mem::take(&mut old.edges); }
        self.regions = regions; self.nodes = own_nodes(geometry); Ok(())
    }

    pub fn cubic_availability(&self) -> Vec<u8> { self.curves.iter().map(|c| c.is_some() as u8).collect() }

    pub fn snapshot(&self) -> FitterSnapshot {
        let mut cubics = Vec::new();
        for (index, curve) in self.curves.iter().enumerate() {
            if let Some(c) = curve { cubics.push(Cubic { index, points: flat(c) }); }
        }
        FitterSnapshot { cubic_storage_slots: self.curves.len(), cubic_availability: self.cubic_availability(), cubics,
            nodes: self.nodes.clone(), regions: self.regions.clone() }
    }

    pub fn run(&mut self, parameters: &FittingParameters,
        mut on_progress: Option<&mut FittingObserver>) -> Result<FittedGeometry, String> {
        validate_parameters(parameters)?;
        let mut previous: Option<(i32, i32)> = None;
        let mut checkpoint = |substep: i32, counter: i32, fitter: &ContourFitter| -> Result<(), String> {
            if previous == Some((substep, counter)) { return Ok(()); }
            previous = Some((substep, counter));
            let (start, end) = fitting_range(substep);
            let fraction = if counter <= 0 { start } else if counter >= 50 { end } else { (end - start) * (counter as f64 / 50.0) + start };
            if let Some(cb) = on_progress.as_mut() {
                cb(&FittingProgress { phase: 10, substep, counter, fraction, percent: (fraction * 100.0 + 0.5).floor() as i32 }, fitter)?;
            }
            Ok(())
        };
        checkpoint(-1, 0, self)?;
        for n in self.nodes.iter_mut() {
            n.kind = match n.kind { 1 | 2 | 5 => 0, 4 => 3, k => k };
        }
        checkpoint(0, 0, self)?;
        let slots = select_intervals(self, parameters, &mut checkpoint)?;
        self.curves = vec![None; slots];
        checkpoint(2, 0, self)?;
        fit_intervals(&mut self.nodes, &self.regions, &mut self.curves);
        checkpoint(3, 0, self)?;
        for i in 0..self.regions.len() {
            let edges = fitted_edges(&self.nodes, &self.regions[i], &self.curves);
            self.regions[i].edges = edges;
        }
        checkpoint(-2, 0, self)?;
        let snap = self.snapshot();
        let mut referenced = std::collections::HashSet::new();
        for r in &snap.regions { for e in &r.edges { if e[1] != 0 { referenced.insert(e[0] as usize); } } }
        Ok(FittedGeometry { cubic_storage_slots: snap.cubic_storage_slots,
            cubics: snap.cubics.into_iter().filter(|c| referenced.contains(&c.index)).collect(), nodes: snap.nodes, regions: snap.regions })
    }
}

fn flat(c: &[Point]) -> [f64; 8] {
    let mut out = [0.0; 8];
    for (i, p) in c.iter().enumerate().take(4) { out[i * 2] = p[0]; out[i * 2 + 1] = p[1]; }
    out
}

/// Fit a completed smoothing graph without changing caller storage.
pub fn fit_contours(geometry: &Geometry, parameters: &FittingParameters,
    on_progress: Option<&mut FittingObserver>) -> Result<FittedGeometry, String> {
    validate_parameters(parameters)?;
    ContourFitter::new(geometry)?.run(parameters, on_progress)
}
