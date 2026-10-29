//! Port of `src/contour-smoothing.mjs`: the complete multi-phase smoother.
use crate::contour_corners::puncture_contour_corners;
use crate::contour_initialization::{create_contour_smoothing_state, initialize_contour_coordinates, initialize_smoothing_neighborhoods, InitializationInput};
use crate::contours::Geometry;
use crate::image_measurement::ContourImageModel;
use crate::smoothing::{smoothing_optimize, ObjectiveState, SmoothingInput};
use crate::smoothing_repair::{ContourRepair, RepairView};
use serde_json::Value;
use serde::{Deserialize, Serialize};

/// Float32 table entries promoted to double (0xa22f98, 0xa22f74). Substep 2 spans 0..500.
fn smoothing_range(anti_alias: bool, substep: i32) -> (f64, f64) {
    if !anti_alias {
        match substep {
            -1 => (0.03551600128412247, 0.03551600128412247),
            0 => (0.035645999014377594, 0.03569699823856354),
            1 => (0.03570299968123436, 0.03570299968123436),
            2 => (0.03896399959921837, 0.9999909996986389),
            3 => (0.9999949932098389, 0.9999949932098389),
            _ => (0.9999970197677612, 0.9999970197677612),
        }
    } else {
        match substep {
            -1 => (0.0037529999390244484, 0.0037529999390244484),
            0 => (0.004172999877482653, 0.026402000337839127),
            1 => (0.02749899961054325, 0.02749899961054325),
            2 => (0.03209700062870979, 0.9967300295829773),
            3 => (0.9967319965362549, 0.9967319965362549),
            _ => (0.9999989867210388, 0.9999989867210388),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothingProgress { pub phase: i32, pub substep: i32, pub counter: i32, pub fraction: f64, pub percent: i32 }

/// Live stage state reported with each notification.
pub struct SmoothingSnapshot<'a> { pub geometry: &'a Geometry, pub penalty_counts: &'a [u8], pub border_counts: [i64; 4], pub reference_positions: &'a [f32] }

pub type SmoothingObserver<'o> = dyn FnMut(&SmoothingProgress, &SmoothingSnapshot) -> Result<(), String> + 'o;

pub struct SmoothContourInput<'a> {
    pub width: i64,
    pub height: i64,
    pub bgra: &'a [u8],
    pub geometry: &'a Geometry,
    pub indices: &'a [i64],
    pub palette: &'a [f64],
    pub parameters: &'a Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmoothedContours { pub geometry: Geometry, pub penalty_counts: Vec<u8>, pub border_counts: [i64; 4], pub reference_positions: Vec<f32> }

fn write_positions(geometry: &mut Geometry, positions: &[f64]) {
    for (node, n) in geometry.nodes.iter_mut().enumerate() { n.x = positions[node * 2]; n.y = positions[node * 2 + 1]; }
}

fn flags3(parameters: &Value, key: &str) -> Option<[bool; 3]> {
    let values = parameters.get(key)?.as_array()?;
    if values.len() != 3 { return None; }
    let mut out = [false; 3];
    for (i, v) in values.iter().enumerate() {
        match v.as_f64() { Some(0.0) => {}, Some(1.0) => out[i] = true, _ => return None }
    }
    Some(out)
}

/// Session-owned entry: `next_uniform` is the application's random stream.
pub fn smooth_contour_geometry(input: &SmoothContourInput, next_uniform: &mut dyn FnMut() -> f64,
    mut on_progress: Option<&mut SmoothingObserver>) -> Result<SmoothedContours, String> {
    let parameters = input.parameters;
    let enabled = flags3(parameters, "ContourSmoother::is_enabled").ok_or("is_enabled must contain three phase flags.")?;
    let puncture = flags3(parameters, "ContourSmoother::do_puncture_corners").ok_or("do_puncture_corners must contain three phase flags.")?;
    let anti_alias = parameters.get("Shared::is_anti_aliased").and_then(Value::as_f64);
    let measurements = parameters.get("ContourSmoother::measurement_types").and_then(Value::as_array);
    let measurements = match (anti_alias, measurements) {
        (Some(a), Some(m)) if (a == 0.0 || a == 1.0) && m.len() == 3 => m,
        _ => return Err("Full smoothing needs numeric anti-alias and measurement controls.".into()),
    };
    let anti_alias = anti_alias == Some(1.0);
    let mut prepared = create_contour_smoothing_state(input.width, input.height, Some(anti_alias), input.geometry)?;
    let mut previous: Option<(i32, i32)> = None;
    let mut checkpoint = |substep: i32, counter: i32, geometry: &Geometry, penalty_counts: &[u8], border_counts: [i64; 4], references: &[f32],
        on_progress: &mut Option<&mut SmoothingObserver>| -> Result<(), String> {
        if previous == Some((substep, counter)) { return Ok(()); }
        previous = Some((substep, counter));
        let (start, end) = smoothing_range(anti_alias, substep);
        let upper = if substep == 2 { 500.0 } else { 1.0 };
        let c = counter as f64;
        let fraction = if c <= 0.0 { start } else if c >= upper { end } else { (end - start) * (c / upper) + start };
        if let Some(cb) = on_progress.as_mut() {
            cb(&SmoothingProgress { phase: 8, substep, counter, fraction, percent: (fraction * 100.0 + 0.5).floor() as i32 },
                &SmoothingSnapshot { geometry, penalty_counts, border_counts, reference_positions: references })?;
        }
        Ok(())
    };
    macro_rules! cp {
        ($s:expr, $c:expr, $g:expr, $p:expr) => {
            checkpoint($s, $c, $g, $p, prepared.border_counts, &prepared.reference_positions, &mut on_progress)?
        };
    }
    cp!(-1, 0, &prepared.geometry, &prepared.penalty_counts);
    cp!(0, 0, &prepared.geometry, &prepared.penalty_counts);
    initialize_smoothing_neighborhoods(&mut prepared, anti_alias)?;
    cp!(0, 1, &prepared.geometry, &prepared.penalty_counts);
    let mut image_model = if anti_alias || measurements.iter().any(|m| m.as_f64() == Some(1.0)) {
        Some(ContourImageModel::new(input.width as f64, input.height as f64, input.bgra, input.indices, &prepared.geometry).map_err(|e| e.0)?)
    } else { None };
    let references = prepared.reference_f64();
    let border_counts = prepared.border_counts;
    let initial = initialize_contour_coordinates(&InitializationInput { width: input.width, height: input.height, bgra: input.bgra,
        geometry: &prepared.geometry, indices: input.indices, reference_positions: &references, border_counts: &border_counts,
        parameters, palette: input.palette }, next_uniform)?;
    let mut geometry = prepared.geometry.clone();
    let mut penalty_counts = prepared.penalty_counts.clone();
    write_positions(&mut geometry, &initial);
    cp!(1, 0, &geometry, &penalty_counts);
    let budget = |phase: usize| -> i32 {
        if enabled[phase] {
            parameters.get(format!("ContourSmoother::phase_{phase}.cg_max_iter")).and_then(Value::as_f64).map_or(0, |v| v as i64 as i32)
        } else { 0 }
    };
    let budgets = [budget(0), budget(1), budget(2)];
    let total_budget = budgets.iter().fold(0i32, |s, &b| s.wrapping_add(b));
    let progress_counter = |completed: i32| -> i32 {
        let q = (completed.wrapping_mul(500) as f64 / total_budget as f64 + 0.5).trunc();
        if q.is_finite() { q as i64 as i32 } else { 0 }
    };
    let nearby_edges = prepared.nearby_edges.clone().unwrap_or_default();
    let near_border: Vec<i64> = prepared.near_border_nodes.iter().map(|&n| n as i64).collect();
    let mut preceding_budget = 0i32;
    for phase in 0..3 {
        if enabled[phase] {
            let optimized = {
                let has_observer = on_progress.is_some();
                let mut observed = if on_progress.is_some() { Some((geometry.clone(), penalty_counts.clone())) } else { None };
                let mut smoothing = SmoothingInput::new(&geometry, parameters);
                smoothing.reference_positions = &references;
                smoothing.penalty_counts = &penalty_counts;
                smoothing.border_counts = &border_counts;
                smoothing.phase = phase as i64;
                smoothing.image_model = image_model.as_mut();
                smoothing.width = input.width as f64;
                smoothing.height = input.height as f64;
                smoothing.near_border_nodes = &near_border;
                if anti_alias { smoothing.nearby_edges = Some(&nearby_edges); smoothing.repair_interval = prepared.repair_interval as f64; }
                let mut observer = |iteration: i64, state: &ObjectiveState| -> Result<(), String> {
                    let (og, op) = observed.as_mut().unwrap();
                    for (node, point) in state.points.iter().enumerate() {
                        og.nodes[node].x = point[0]; og.nodes[node].y = point[1]; og.nodes[node].flags = state.geometry.nodes[node].flags;
                    }
                    op.copy_from_slice(&state.penalty_counts);
                    let counter = progress_counter(preceding_budget.wrapping_add(iteration as i32));
                    checkpoint(2, counter, og, op, prepared.border_counts, &prepared.reference_positions, &mut on_progress)
                };
                smoothing_optimize(&mut smoothing, if has_observer { Some(&mut observer) } else { None })?
            };
            write_positions(&mut geometry, &optimized.positions);
            if let Some(repair) = &optimized.repair_state {
                penalty_counts.copy_from_slice(&repair.penalty_counts);
                for (n, &f) in geometry.nodes.iter_mut().zip(&repair.flags) { n.flags = f; }
            }
            if puncture[phase] { geometry = puncture_contour_corners(&geometry).map_err(|e| e.0)?; }
        }
        preceding_budget = preceding_budget.wrapping_add(budgets[phase]);
        cp!(2, progress_counter(preceding_budget), &geometry, &penalty_counts);
    }
    cp!(3, 0, &geometry, &penalty_counts);
    if anti_alias {
        // Original 0x497c30 ends with one all-region repair, including decay.
        let points: Vec<[f64; 2]> = geometry.nodes.iter().map(|n| [n.x, n.y]).collect();
        let mut repair = ContourRepair::new(geometry.regions.len(), nearby_edges);
        repair.all(&mut RepairView { geometry: &mut geometry, penalty_counts: &mut penalty_counts, points: &points });
    }
    cp!(-2, 0, &geometry, &penalty_counts);
    Ok(SmoothedContours { geometry, penalty_counts, border_counts: prepared.border_counts, reference_positions: prepared.reference_positions })
}
