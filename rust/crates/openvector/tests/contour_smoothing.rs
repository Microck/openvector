mod common;
mod smoothing_support;
use openvector::contour_smoothing::{smooth_contour_geometry, SmoothContourInput, SmoothingObserver, SmoothedContours};
use serde_json::{json, Value};
use smoothing_support::*;
use std::collections::BTreeSet;

fn with_input<T>(fx: &Value, call: impl FnOnce(&SmoothContourInput) -> T) -> T {
    let bgra = bytes(&fx["bgra"]);
    let g = geometry(&fx["geometry"]);
    let indices = i64s(&fx["segmentation"]["indices"]);
    let palette = f64s(&fx["segmentation"]["palette"]);
    let input = SmoothContourInput { width: i(&fx["width"]), height: i(&fx["height"]), bgra: &bgra,
        geometry: &g, indices: &indices, palette: &palette, parameters: &fx["parameters"] };
    call(&input)
}

fn smooth(fx: &Value, state: &mut i64, observer: Option<&mut SmoothingObserver>) -> Result<SmoothedContours, String> {
    with_input(fx, |input| smooth_contour_geometry(input, &mut || next_uniform(state), observer))
}

#[test]
fn full_smoothing_matches_all_2713_native_outcomes_across_retained_sessions() {
    let reference = common::read_json("smoothing-full-sessions-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2713);
    assert_eq!(reference["status"], "complete");
    let (mut successes, mut failures) = (0, 0);
    let (mut profiles, mut masks) = (BTreeSet::new(), BTreeSet::new());
    let mut session = None;
    for (index, fx) in cases.iter().enumerate() {
        if fx["sessionStart"] == true { session = Some(openvector::palette::CoreSession::default()); }
        let session = session.as_ref().expect("native session start");
        assert_eq!(session.random_state() as i64, i(&fx["randomStateBefore"]), "session boundary {index}");
        let actual = with_input(fx, |input| session.smooth_contours(input));
        if fx.get("expectedFailure").is_some() {
            let error = actual.unwrap_err();
            assert!(error.contains("fragment scratch"), "native failure {index}: {error}");
            assert_eq!(session.random_state() as i64, i(&fx["randomStateBefore"]), "failed session rollback {index}");
            failures += 1;
        } else {
            let actual = actual.unwrap_or_else(|error| panic!("full smoothing {index}: {error}"));
            assert_same(&json!({"geometry": actual.geometry.to_json(), "penaltyCounts": actual.penalty_counts, "randomState": session.random_state()}),
                &fx["expected"], &format!("completed full smoothing {index}"));
            successes += 1;
        }
        if let Some(p) = fx.get("fullProfile") { profiles.insert(i(p)); }
        if let Some(m) = fx.get("phaseMask") { masks.insert(i(m)); }
    }
    assert_eq!((successes, failures), (2712, 1));
    assert_eq!(profiles.into_iter().collect::<Vec<_>>(), (1..=9).collect::<Vec<_>>());
    assert_eq!(masks.into_iter().collect::<Vec<_>>(), (0..=7).collect::<Vec<_>>());
}

#[test]
fn retained_original_failure_is_an_error_not_a_vector_or_panic() {
    let reference = common::read_json("smoothing-full-prefix-reference.json");
    let failure = &reference["nativeFailure"];
    let mut state = i(&failure["randomStateBefore"]);
    let error = smooth(&failure["input"], &mut state, None).unwrap_err();
    assert!(error.contains("fragment scratch"), "{error}");
}

#[test]
fn full_smoothing_matches_all_15_original_pipeline_targets() {
    let reference = common::read_json("pipeline-smoothing-reference.json");
    let segmentation = common::read_json("pipeline-segmentation-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for (index, fx) in cases.iter().enumerate() {
        let mut input = fx.clone();
        input["geometry"] = fx["before"].clone();
        input["segmentation"] = segmentation["cases"][index]["expected"].clone();
        let before = input.clone();
        let mut state = i(&fx["randomStateBefore"]);
        let actual = smooth(&input, &mut state, None).unwrap();
        assert_same(&actual.geometry.to_json(), &fx["expected"], fx["id"].as_str().unwrap());
        assert_same(&json!(actual.penalty_counts), &fx["stateMetadata"]["penaltyCounts"], "pipeline smoothing penalties");
        assert_eq!(state, i(&fx["randomStateAfter"]));
        assert_eq!(input, before);
    }
}

#[test]
fn observing_all_eight_phase_masks_preserves_completed_geometry() {
    let reference = common::read_json("smoothing-full-sessions-reference.json");
    let mut selected = BTreeSet::new();
    for fx in reference["cases"].as_array().unwrap() {
        let Some(mask) = fx.get("phaseMask") else { continue };
        if fx.get("expectedFailure").is_some() || !selected.insert(i(mask)) { continue; }
        let mut state = i(&fx["randomStateBefore"]);
        let mut events = Vec::new();
        let mut observer = |event: &openvector::contour_smoothing::SmoothingProgress, snapshot: &openvector::contour_smoothing::SmoothingSnapshot| {
            assert_eq!(snapshot.geometry.nodes.len(), snapshot.penalty_counts.len());
            assert_eq!(snapshot.reference_positions.len(), snapshot.geometry.nodes.len() * 2);
            events.push(*event);
            Ok(())
        };
        let actual = smooth(fx, &mut state, Some(&mut observer)).unwrap();
        assert_same(&json!({"geometry": actual.geometry.to_json(), "penaltyCounts": actual.penalty_counts, "randomState": state}),
            &fx["expected"], &format!("observed phase mask {mask}"));
        assert!(events.len() > 4);
        for event in events { assert_eq!(event.phase, 8); assert!(event.fraction.is_finite()); }
    }
    assert_eq!(selected.len(), 8);
}

#[test]
fn observer_failure_propagates_without_mutating_input_geometry() {
    let reference = common::read_json("smoothing-full-sessions-reference.json");
    let fx = &reference["cases"][0];
    let mut state = i(&fx["randomStateBefore"]);
    let before = fx.clone();
    let mut calls = 0;
    let mut observer = |_: &openvector::contour_smoothing::SmoothingProgress, _: &openvector::contour_smoothing::SmoothingSnapshot| {
        calls += 1;
        Err("interrupted".to_string())
    };
    assert_eq!(smooth(fx, &mut state, Some(&mut observer)).unwrap_err(), "interrupted");
    assert_eq!(calls, 1);
    assert_eq!(state, i(&fx["randomStateBefore"]));
    assert_eq!(fx, &before);
}

#[test]
fn failed_public_smoothing_restores_the_stream_before_the_next_successful_call() {
    let reference = common::read_json("pipeline-smoothing-reference.json");
    let segmentation = common::read_json("pipeline-segmentation-reference.json");
    let session = openvector::palette::CoreSession::default();
    for (index, fx) in reference["cases"].as_array().unwrap().iter().enumerate()
        .filter(|(_, fx)| fx["parameters"]["Shared::is_anti_aliased"] == 1).take(3) {
        let mut input = fx.clone();
        input["geometry"] = fx["before"].clone();
        input["segmentation"] = segmentation["cases"][index]["expected"].clone();
        assert_eq!(session.random_state() as i64, i(&fx["randomStateBefore"]));
        let mut invalid = input.clone();
        invalid["parameters"]["ContourSmoother::phase_2.cg_min_eps"] = json!(-1);
        let error = with_input(&invalid, |input| session.smooth_contours(input)).unwrap_err();
        assert!(error.contains("Invalid quadratic line-search phase controls"), "{error}");
        assert_eq!(session.random_state() as i64, i(&fx["randomStateBefore"]));
        let actual = with_input(&input, |input| session.smooth_contours(input)).unwrap();
        assert_same(&actual.geometry.to_json(), &fx["expected"], fx["id"].as_str().unwrap());
        assert_eq!(session.random_state() as i64, i(&fx["randomStateAfter"]));
    }
}
