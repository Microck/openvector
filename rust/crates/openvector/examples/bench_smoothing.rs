//! Reproducible full smoothing benchmark using the checked-in 15-case pipeline corpus.
//! Run with --release; optional first argument is repetitions of the complete corpus.
#[path = "../tests/smoothing_support/mod.rs"]
mod support;
use support::*;
use openvector::contour_smoothing::{smooth_contour_geometry, SmoothContourInput};
use serde_json::Value;
use std::{hint::black_box, time::Instant};

fn main() {
    let repetitions: usize = std::env::args().nth(1).unwrap_or("20".into()).parse().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures");
    let reference: Value = serde_json::from_slice(&std::fs::read(root.join("pipeline-smoothing-reference.json")).unwrap()).unwrap();
    let segmentation: Value = serde_json::from_slice(&std::fs::read(root.join("pipeline-segmentation-reference.json")).unwrap()).unwrap();
    for (index, fx) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let bgra = bytes(&fx["bgra"]);
        let geometry = geometry(&fx["before"]);
        let indices = i64s(&segmentation["cases"][index]["expected"]["indices"]);
        let palette = f64s(&segmentation["cases"][index]["expected"]["palette"]);
        let input = SmoothContourInput { width: i(&fx["width"]), height: i(&fx["height"]), bgra: &bgra, geometry: &geometry, indices: &indices, palette: &palette, parameters: &fx["parameters"] };
        let run = || {
            let mut state = i(&fx["randomStateBefore"]);
            let result = smooth_contour_geometry(&input, &mut || next_uniform(&mut state), None).unwrap();
            (result, state)
        };
        let (expected, state) = run();
        assert_same(&expected.geometry.to_json(), &fx["expected"], fx["id"].as_str().unwrap());
        assert_same(&serde_json::json!(expected.penalty_counts), &fx["stateMetadata"]["penaltyCounts"], "penalties");
        assert_eq!(state, i(&fx["randomStateAfter"]));
        let start = Instant::now();
        for _ in 0..repetitions { black_box(run()); }
        let elapsed = start.elapsed();
        // Correctness comparisons stay outside the timed region.
        let (actual, actual_state) = run();
        assert_eq!(actual, expected);
        assert_eq!(actual_state, state);
        println!("{}\t{}\t{:.6}", fx["id"].as_str().unwrap(), geometry.nodes.len(), elapsed.as_secs_f64() * 1000.0 / repetitions as f64);
    }
}
