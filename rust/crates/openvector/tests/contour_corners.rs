mod common;
mod smoothing_support;
use openvector::contour_corners::{classify_corner_features, puncture_contour_corners};
use openvector::contours::Geometry;
use serde_json::Value;

fn number(v: &Value) -> f64 {
    match v {
        Value::String(s) => match s.as_str() { "NaN" => f64::NAN, "Infinity" => f64::INFINITY, "-Infinity" => f64::NEG_INFINITY, _ => s.parse().unwrap() },
        _ => v.as_f64().unwrap(),
    }
}

#[test]
fn corner_decisions_match_all_3504_original_threshold_arbitrary_and_nonfinite_feature_vectors() {
    let decisions = common::read_json("corner-decision-reference.json");
    let cases = decisions["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3504);
    for (index, fixture) in cases.iter().enumerate() {
        let features: Vec<f64> = fixture["features"].as_array().unwrap().iter().map(number).collect();
        assert_eq!(classify_corner_features(&features), fixture["expected"].as_bool().unwrap(), "corner decision {index}");
    }
}

#[test]
fn complete_corner_puncturing_matches_all_3680_original_raw_smoothed_and_transformed_geometries() {
    let puncturing = common::read_json("contour-puncturing-reference.json");
    let cases = puncturing["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3680);
    for (index, fixture) in cases.iter().enumerate() {
        let geometry = Geometry::from_json(&fixture["geometry"]).unwrap();
        let expected = Geometry::from_json(&fixture["expected"]).unwrap();
        smoothing_support::assert_same(&puncture_contour_corners(&geometry).unwrap().to_json(), &expected.to_json(), &format!("corner puncturing {index}"));
    }
}

#[test]
fn corner_puncturing_owns_geometry_and_rejects_malformed_inputs_before_producing_a_partial_result() {
    let puncturing = common::read_json("contour-puncturing-reference.json");
    let fixture = puncturing["cases"].as_array().unwrap().iter().find(|c| c["geometry"]["regions"].as_array().unwrap().iter()
        .any(|r| r["contour"].as_array().unwrap().len() > 6)).unwrap();
    let input = Geometry::from_json(&fixture["geometry"]).unwrap();
    let before = input.clone();
    let mut actual = puncture_contour_corners(&input).unwrap();
    actual.nodes[0].kind = 255;
    actual.regions[0].boundary[0][0] = 255;
    assert_eq!(input, before);
    // A sparse node array has no Rust Vec form; null/{} map to empty geometry.
    let mut invalid_node = input.clone();
    invalid_node.nodes[0].x = f64::INFINITY;
    let mut invalid_boundary = input.clone();
    invalid_boundary.regions[0].boundary[0][0] = 256;
    let mut invalid_contour = input.clone();
    invalid_contour.regions[0].contour[0] = invalid_contour.nodes.len();
    let empty = Geometry { nodes: vec![], regions: vec![] };
    let no_regions = Geometry { nodes: input.nodes.clone(), regions: vec![] };
    for geometry in [empty, no_regions, invalid_node, invalid_boundary, invalid_contour] {
        assert!(puncture_contour_corners(&geometry).is_err());
    }
    assert_eq!(input, before);
}
