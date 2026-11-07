mod common;
mod smoothing_support;
use openvector::contour_fitting::{fit_contours, ContourFitter, FittingParameters, FittingProgress};
use smoothing_support::{assert_same, geometry};

// Reuse the stage controller with an uncached public partition kernel. This
// preserves its stateful history (including inactive nodes' old cubic slots)
// while independently recomputing the original multiplier for every interval.
mod contours { pub use openvector::contours::*; }
mod fitting {
    pub use openvector::fitting::{fit_contour_interval, FitNode, Point};
    #[allow(clippy::too_many_arguments)]
    pub fn partition_contour_interval_cached(nodes: &mut [FitNode], contour: &[usize], start: i64, end: i64,
        initial: f64, final_: f64, iterations: i64, fraction: f64, _: &mut Option<f64>)
        -> Result<openvector::fitting::KeeperPartition, String> {
        openvector::fitting::partition_contour_interval(nodes, contour, start, end, initial, final_, iterations, fraction)
    }
}
#[allow(dead_code)]
#[path = "../src/contour_fitting.rs"]
mod uncached;
fn uncached_parameters(p: FittingParameters) -> uncached::FittingParameters {
    uncached::FittingParameters { stat_thresh_initial: p.stat_thresh_initial,
        stat_thresh_final: p.stat_thresh_final, max_iterations: p.max_iterations }
}

fn fixture() -> (openvector::contours::Geometry, FittingParameters) {
    let fixtures = common::read_json("pipeline-fitting-reference.json");
    let case = fixtures["cases"].as_array().unwrap().iter().find(|c| c["id"] == "alpha-profile-3").unwrap();
    (geometry(&case["geometry"]), FittingParameters::from_json(&case["parameters"]))
}

#[test]
fn repeated_fitting_recomputes_thresholds_when_controls_change() {
    let (geometry, original) = fixture();
    let changed = FittingParameters { stat_thresh_initial: 0.125, stat_thresh_final: 0.5, max_iterations: 7.0 };
    let decreasing = FittingParameters { stat_thresh_initial: 0.0625, stat_thresh_final: 0.0078125, max_iterations: 13.0 };
    let disabled = FittingParameters { stat_thresh_initial: 1.0, stat_thresh_final: f64::from_bits(1), max_iterations: 0.0 };
    let mut fitter = ContourFitter::new(&geometry).unwrap();
    let mut reference = uncached::ContourFitter::new(&geometry).unwrap();
    for p in [original, changed, disabled, decreasing, original] {
        let expected = reference.run(&uncached_parameters(p), None).unwrap();
        let actual = fitter.run(&p, None).unwrap();
        assert_same(&actual.to_json(), &expected.to_json(), "changed fitting controls on retained instance");
    }
}

#[test]
fn cancelled_fitting_can_retry_with_a_different_threshold_schedule() {
    let (geometry, original) = fixture();
    let changed = FittingParameters { stat_thresh_initial: 0.125, stat_thresh_final: 0.5, max_iterations: 7.0 };
    let retry = FittingParameters { stat_thresh_initial: 0.0625, stat_thresh_final: 0.0078125, max_iterations: 13.0 };
    for stop in [-1, 0, 1, 2, 3, -2] {
        let mut fitter = ContourFitter::new(&geometry).unwrap();
        fitter.run(&original, None).unwrap();
        let mut reference = uncached::ContourFitter::new(&geometry).unwrap();
        reference.run(&uncached_parameters(original), None).unwrap();
        let mut reference_observer = |event: &uncached::FittingProgress, _: &uncached::ContourFitter| {
            if event.substep == stop { Err("cancelled".to_owned()) } else { Ok(()) }
        };
        assert_eq!(reference.run(&uncached_parameters(changed), Some(&mut reference_observer)).unwrap_err(), "cancelled");
        let mut observed = None;
        let mut observer = |event: &FittingProgress, live: &ContourFitter| {
            if event.substep == stop {
                observed = Some(live.snapshot()); Err("cancelled".to_owned())
            } else { Ok(()) }
        };
        assert_eq!(fitter.run(&changed, Some(&mut observer)).unwrap_err(), "cancelled");
        assert_eq!(fitter.snapshot(), observed.unwrap(), "state at cancellation checkpoint {stop}");
        assert_same(&fitter.snapshot().to_json(), &reference.snapshot().to_json(), "cancelled state matches uncached controller");
        let expected = reference.run(&uncached_parameters(retry), None).unwrap();
        let actual = fitter.run(&retry, None).unwrap();
        assert_same(&actual.to_json(), &expected.to_json(), "retry uses new fitting controls");
    }
}

#[test]
fn invalid_controls_return_before_callbacks_and_preserve_retained_state() {
    let (geometry, original) = fixture();
    let mut fitter = ContourFitter::new(&geometry).unwrap();
    fitter.run(&original, None).unwrap();
    let before = fitter.snapshot();
    for invalid in [
        FittingParameters { stat_thresh_initial: 0.0, ..original },
        FittingParameters { stat_thresh_final: f64::NAN, ..original },
        FittingParameters { max_iterations: 0.5, ..original },
        FittingParameters { max_iterations: -1.0, ..original },
        FittingParameters { stat_thresh_initial: f64::MIN_POSITIVE, stat_thresh_final: f64::MAX, ..original },
    ] {
        let mut observer = |_: &FittingProgress, _: &ContourFitter| -> Result<(), String> { panic!("invalid controls reached a checkpoint") };
        assert!(fitter.run(&invalid, Some(&mut observer)).unwrap_err().starts_with("Fitting needs"));
        assert_eq!(fitter.snapshot(), before);
    }
    assert_same(&fitter.run(&original, None).unwrap().to_json(),
        &fit_contours(&geometry, &original, None).unwrap().to_json(), "retry after invalid controls");
}
