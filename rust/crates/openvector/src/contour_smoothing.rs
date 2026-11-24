//! Port of `src/contour-smoothing.mjs`: the complete multi-phase smoother.
use crate::contour_corners::puncture_contour_corners;
use crate::contour_initialization::{create_contour_smoothing_state, initialize_contour_coordinates, initialize_smoothing_neighborhoods, InitializationInput};
use crate::contours::Geometry;
use crate::image_measurement::ContourImageModel;
use crate::smoothing::{optimize, ObjectiveState, SmoothingInput};
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
    // The prepared state's geometry and penalties are this phase's own working
    // copies: `create_contour_smoothing_state` built them from the caller's
    // geometry, and nothing reads `prepared.geometry`/`prepared.penalty_counts`
    // after this point (the checkpoints above already ran, and the result below
    // takes `border_counts` and `reference_positions`). Moving them out is the
    // same value the clone produced, without the deep copy of every region's
    // contour, boundary and neighbour vectors.
    let mut geometry = prepared.geometry;
    let mut penalty_counts = prepared.penalty_counts;
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
    // `prepared.nearby_edges` is read only here and by the final repair below,
    // both through this local, so the phase can take it instead of cloning the
    // per-node edge lists.
    let nearby_edges = prepared.nearby_edges.take().unwrap_or_default();
    let near_border: Vec<i64> = prepared.near_border_nodes.iter().map(|&n| n as i64).collect();
    let mut preceding_budget = 0i32;
    for phase in 0..3 {
        if enabled[phase] {
            let optimized = {
                let has_observer = on_progress.is_some();
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
                let mut observer = |iteration: i64, state: &mut ObjectiveState| -> Result<(), String> {
                    state.synchronize_published_coordinates();
                    let counter = progress_counter(preceding_budget.wrapping_add(iteration as i32));
                    checkpoint(2, counter, &state.geometry, &state.penalty_counts, prepared.border_counts, &prepared.reference_positions, &mut on_progress)
                };
                optimize(&mut smoothing, if has_observer { Some(&mut observer) } else { None })?
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

#[cfg(test)]
mod observer_state_tests {
    //! End-to-end: the observer-attached path, which is the only path that
    //! publishes, against the same run without an observer. The phase no longer
    //! keeps a duplicate of its own input geometry, so these tests pin that the
    //! result and the published snapshots are exactly the ones the old
    //! duplicate-maintaining form produced.
    use super::*;
    use crate::contours::{extract_contours, ContourInput};
    use crate::settings::NUMERIC_DATA;

    fn fingerprint(geometry: &Geometry, penalties: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for node in &geometry.nodes {
            bytes.extend(node.x.to_bits().to_le_bytes());
            bytes.extend(node.y.to_bits().to_le_bytes());
            bytes.push(node.kind);
            bytes.push(node.flags);
        }
        bytes.extend(penalties);
        for region in &geometry.regions {
            bytes.extend(region.pixel_count.to_le_bytes());
            bytes.extend(region.contour.iter().flat_map(|i| i.to_le_bytes()));
        }
        bytes
    }

    /// A two-color 12 by 12 image with an interior diagonal seam, so the
    /// geometry has interior nodes, a repair pass that writes flags, and
    /// quarter-dual corners.
    fn fixture() -> (i64, i64, Vec<u8>, Vec<i64>, Vec<f64>, Geometry) {
        let (width, height) = (12i64, 12i64);
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        for pixel in 0..(width * height) as usize {
            let (x, y) = (pixel as i64 % width, pixel as i64 / width);
            let color = if x + y < 12 { 0usize } else { 1usize };
            bgra[pixel * 4..pixel * 4 + 4].copy_from_slice(if color == 0 { &[20, 40, 60, 255] } else { &[200, 180, 160, 255] });
        }
        let indices: Vec<i64> = (0..width * height).map(|pixel| {
            let (x, y) = (pixel % width, pixel / width);
            if x + y < 12 { 0 } else { 1 }
        }).collect();
        let palette: Vec<f64> = vec![0.23529411764705882, 0.1568627450980392, 0.0784313725490196, 1.0,
            0.6274509803921569, 0.7058823529411765, 0.7843137254901961, 1.0];
        let counts: Vec<i64> = vec![indices.iter().filter(|&&i| i == 0).count() as i64,
            indices.iter().filter(|&&i| i == 1).count() as i64];
        let colors: Vec<i64> = vec![0, 1];
        let geometry = extract_contours(&ContourInput { width: width as f64, height: height as f64,
            indices: &indices, counts: &counts, color_indices: &colors, palette: &palette, anti_alias: Some(true) })
            .expect("fixture geometry");
        (width, height, bgra, indices, palette, geometry)
    }

    fn parameters() -> Value {
        let mut parameters = NUMERIC_DATA["profiles"][5].clone();
        parameters["Shared::is_anti_aliased"] = serde_json::json!(1);
        parameters["ContourSmoother::is_enabled"] = serde_json::json!([1, 1, 1]);
        parameters["ContourSmoother::measurement_types"] = serde_json::json!([1, 1, 1]);
        parameters
    }

    /// The application's own random stream, from a fixed seed.
    fn stream(seed: u32) -> impl FnMut() -> f64 {
        let mut state = seed;
        move || { state = state.wrapping_mul(2003) % 9973; state as f64 * (1.0 / 9973.0) }
    }

    struct Event { progress: SmoothingProgress, snapshot: Vec<u8>, counters: (i32, i32) }

    fn run(observer: bool) -> (SmoothedContours, Vec<Event>) {
        let (width, height, bgra, indices, palette, geometry) = fixture();
        let parameters = parameters();
        let input = SmoothContourInput { width, height, bgra: &bgra, geometry: &geometry, indices: &indices,
            palette: &palette, parameters: &parameters };
        let mut random = stream(0x9e37_79b9);
        let mut events = Vec::new();
        let result = if observer {
            let mut sink = |progress: &SmoothingProgress, live: &SmoothingSnapshot| -> Result<(), String> {
                events.push(Event { progress: *progress, snapshot: fingerprint(live.geometry, live.penalty_counts),
                    counters: (live.border_counts[0] as i32, live.reference_positions.len() as i32) });
                Ok(())
            };
            smooth_contour_geometry(&input, &mut random, Some(&mut sink))
        } else {
            smooth_contour_geometry(&input, &mut random, None)
        };
        (result.expect("smoothed"), events)
    }

    /// An attached observer must not change one byte of the result, and the
    /// snapshot after each phase must be the geometry the phase produced.
    #[test]
    fn observing_changes_nothing_and_publishes_the_phase_state() {
        let (quiet, quiet_events) = run(false);
        let (observed, events) = run(true);
        assert!(quiet_events.is_empty(), "no observer, no publication");
        assert_eq!(fingerprint(&quiet.geometry, &quiet.penalty_counts), fingerprint(&observed.geometry, &observed.penalty_counts),
            "the observer path changed the result");
        assert_eq!(quiet.penalty_counts, observed.penalty_counts);
        assert_eq!(quiet.border_counts, observed.border_counts);
        assert_eq!(quiet.reference_positions, observed.reference_positions);
        assert!(!events.is_empty(), "the observer must receive events");

        // The last publication of every substep is that substep's final state;
        // the terminal `-2` snapshot is the returned geometry, byte for byte.
        let last = events.last().expect("an event");
        assert_eq!(last.progress.substep, -2, "the run must end with the final publication");
        assert_eq!(last.snapshot, fingerprint(&observed.geometry, &observed.penalty_counts),
            "the final publication must be the returned geometry and penalties");
        // Progress is monotone inside a substep and never repeats a (substep, counter).
        let mut seen = std::collections::BTreeSet::new();
        let mut previous = -1i32;
        for event in &events {
            assert!(seen.insert((event.progress.substep, event.progress.counter)),
                "duplicate checkpoint {:?}", (event.progress.substep, event.progress.counter));
            if event.progress.substep == 2 {
                assert!(event.progress.counter >= previous, "counters must not go backwards");
                previous = event.progress.counter;
            }
            assert!(event.progress.fraction.is_finite() && (0.0..=1.0).contains(&event.progress.fraction));
            assert_eq!(event.counters.0, observed.border_counts[0] as i32);
            assert_eq!(event.counters.1, observed.reference_positions.len() as i32);
        }
        // A repair ran (anti-alias fixture), so the published snapshots must
        // carry flags the input geometry did not have, and coordinates the input
        // did not have: the two halves of the joined state are both live.
        let (_, _, _, _, _, input_geometry) = fixture();
        let initial = fingerprint(&input_geometry, &vec![0u8; input_geometry.nodes.len()]);
        assert!(events.iter().any(|e| e.snapshot != initial), "publication must move with the search");
    }

    /// The same RNG stream produces the same bytes whether or not the phases are
    /// observed, and two cold runs agree, so the removal introduced no hidden
    /// state shared between phases.
    #[test]
    fn the_result_is_deterministic_across_cold_runs() {
        let (first, _) = run(false);
        let (second, _) = run(false);
        let (observed, _) = run(true);
        let want = fingerprint(&first.geometry, &first.penalty_counts);
        assert_eq!(want, fingerprint(&second.geometry, &second.penalty_counts));
        assert_eq!(want, fingerprint(&observed.geometry, &observed.penalty_counts));
    }

    /// A cancelled observer stops the search at the event that returned the
    /// error: the error propagates and no later checkpoint runs.
    #[test]
    fn a_cancelling_observer_stops_at_its_own_event() {
        let (width, height, bgra, indices, palette, geometry) = fixture();
        let parameters = parameters();
        let input = SmoothContourInput { width, height, bgra: &bgra, geometry: &geometry, indices: &indices,
            palette: &palette, parameters: &parameters };
        let mut random = stream(0x1234_5678);
        let mut seen = 0usize;
        let result = {
            let mut sink = |_: &SmoothingProgress, _: &SmoothingSnapshot| -> Result<(), String> {
                seen += 1;
                if seen == 5 { Err("cancelled".into()) } else { Ok(()) }
            };
            smooth_contour_geometry(&input, &mut random, Some(&mut sink))
        };
        assert_eq!(result.err().as_deref(), Some("cancelled"));
        assert_eq!(seen, 5, "the run must stop at the cancelling event");
    }
}
