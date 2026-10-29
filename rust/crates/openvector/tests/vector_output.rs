//! In-process tests from `test/vector-output.test.mjs`. The serializer
//! interruption and file-ownership tests belong to the serializer ports.
mod common;
mod fitted_geometry;
use openvector::vector_output::{assemble_output_shapes, export_progress_percent, is_output_region_visible, OutputShapes};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn shapes_json(shapes: &OutputShapes) -> Value {
    json!({
        "children": shapes.children,
        "holes": shapes.holes.iter().map(|paths| paths.iter().map(|path| path.iter()
            .map(|&(child, edge)| json!([child, edge])).collect::<Vec<_>>()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "colorOrder": shapes.color_order.iter().map(|&(region, flag)| json!([region, flag])).collect::<Vec<_>>(),
    })
}

fn expected_shapes(fixture: &Value) -> Value {
    let mut expected = fixture["expected"].clone();
    expected.as_object_mut().unwrap().remove("visibleQueries");
    expected
}

#[test]
fn all_51_export_progress_phases_match_the_original_document_integer_observer() {
    let reference = common::read_json("export-progress-reference.json");
    let phases = reference["phases"].as_array().unwrap();
    assert_eq!(phases.iter().map(|frame| frame["phase"].as_i64().unwrap()).collect::<Vec<_>>(), (0..51).collect::<Vec<_>>());
    for frame in phases {
        assert_eq!(frame["cancelled"], false);
        let phase = frame["phase"].as_i64().unwrap() as i32;
        assert_eq!(export_progress_percent(phase) as i64, frame["percent"].as_i64().unwrap(), "phase {phase}");
    }
}

#[test]
fn output_child_hole_paths_and_color_groups_match_all_2872_original_states() {
    let reference = common::read_json("output-shapes-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2872);
    let (mut base, mut visibility) = (0, 0);
    for (index, fixture) in cases.iter().enumerate() {
        let geometry = fitted_geometry::fitted_geometry(&fixture["geometry"]);
        assert_eq!(shapes_json(&assemble_output_shapes(&geometry)), expected_shapes(fixture), "original output assembly {index}");
        assert_eq!(fixture["expected"]["visibleQueries"].as_array().unwrap().len(), geometry.regions.len() + 4);
        if fixture.get("visibilityProbe").is_some_and(|probe| !probe.is_null()) { visibility += 1; } else { base += 1; }
    }
    assert_eq!((base, visibility), (2712, 160));
}

#[test]
fn output_visibility_matches_native_hidden_words_across_range_and_word_boundaries() {
    let reference = common::read_json("output-shapes-reference.json");
    let (mut limits, mut patterns) = (BTreeSet::new(), BTreeSet::new());
    for fixture in reference["cases"].as_array().unwrap() {
        let hidden_size = fixture["hiddenSize"].as_u64().unwrap() as usize;
        let words: Vec<u32> = fixture["hiddenWords"].as_array().unwrap().iter().map(|w| w.as_u64().unwrap() as u32).collect();
        let expected = fixture["expected"]["visibleQueries"].as_array().unwrap();
        let actual: Vec<Value> = (0..expected.len())
            .map(|region| Value::Bool(is_output_region_visible(hidden_size, &words, region))).collect();
        assert_eq!(&actual, expected);
        limits.insert(hidden_size);
        if let Some(probe) = fixture.get("visibilityProbe").and_then(Value::as_str) { patterns.insert(probe.to_string()); }
    }
    for boundary in [0, 1, 31, 32, 33, 63, 64, 65, 1023, 1024, 1025] { assert!(limits.contains(&boundary), "{boundary}"); }
    assert_eq!(patterns.into_iter().collect::<Vec<_>>(), ["all", "boundaries", "even", "none", "odd"]);
}

#[test]
fn assembled_output_arrays_own_their_paths_and_preserve_the_fitted_graph() {
    let reference = common::read_json("output-shapes-reference.json");
    let fixture = reference["cases"].as_array().unwrap().iter()
        .find(|frame| frame["expected"]["holes"].as_array().unwrap().iter().any(|paths| !paths.as_array().unwrap().is_empty()))
        .unwrap();
    let geometry = fitted_geometry::fitted_geometry(&fixture["geometry"]);
    let before = geometry.clone();
    let mut first = assemble_output_shapes(&geometry);
    let second = assemble_output_shapes(&geometry);
    let region = first.holes.iter().position(|paths| !paths.is_empty()).unwrap();
    first.holes[region][0][0].0 = usize::MAX;
    first.children[region][0] = usize::MAX;
    first.color_order[0].0 = usize::MAX;
    assert_eq!(shapes_json(&second), expected_shapes(fixture));
    assert_eq!(geometry, before);
}
