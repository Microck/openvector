mod common;
mod smoothing_support;
use openvector::contour_fitting::{fit_contours, FittingParameters};
use openvector::fitting::{fit_contour_interval, partition_contour_interval, prepare_fitting_samples, FitNode};
use serde_json::{json, Value};
use smoothing_support::*;

fn fit_nodes(v: &Value) -> Vec<FitNode> {
    v.as_array().unwrap().iter().map(|n| FitNode { x: f(&n["x"]), y: f(&n["y"]), kind: i(&n["kind"]) as u8, cubic: -1 }).collect()
}
fn kinds(nodes: &[FitNode]) -> Value { json!(nodes.iter().map(|n| n.kind).collect::<Vec<_>>()) }
fn points(p: &[[f64; 2]]) -> Value { json!(p.iter().map(|q| q.to_vec()).collect::<Vec<_>>()) }
fn without_random(expected: &Value) -> Value {
    let mut e = expected.clone();
    assert_eq!(e["randomState"], json!(1));
    e.as_object_mut().unwrap().remove("randomState");
    e
}

#[test]
fn fitting_sample_preparation_matches_all_2784_original_states() {
    let reference = common::read_json("fitting-samples-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2784);
    let mut samples = 0;
    for (index, fx) in cases.iter().enumerate() {
        let mut nodes = fit_nodes(&fx["nodes"]); let contour = usizes(&fx["contour"]);
        let actual = prepare_fitting_samples(&mut nodes, &contour, i(&fx["start"]), i(&fx["end"]), fx["markInterior"].as_bool().unwrap());
        let got = json!({"parameters": actual.parameters, "points": points(&actual.points), "kinds": kinds(&nodes)});
        assert_same(&got, &without_random(&fx["expected"]), &format!("original sample preparation {index}"));
        for (n, o) in nodes.iter().zip(fx["nodes"].as_array().unwrap()) { assert!(n.x == f(&o["x"]) && n.y == f(&o["y"])); }
        samples += actual.points.len();
    }
    assert_eq!(samples, 7408);
}

#[test]
fn fitting_sample_outputs_own_their_coordinates_and_parameters() {
    let reference = common::read_json("fitting-samples-reference.json");
    let fx = reference["cases"].as_array().unwrap().iter()
        .find(|s| s["expected"]["points"].as_array().unwrap().len() >= 4 && !s["markInterior"].as_bool().unwrap()).unwrap();
    let mut nodes = fit_nodes(&fx["nodes"]); let before = nodes.clone(); let contour = usizes(&fx["contour"]);
    let mut first = prepare_fitting_samples(&mut nodes, &contour, i(&fx["start"]), i(&fx["end"]), false);
    let second = prepare_fitting_samples(&mut nodes, &contour, i(&fx["start"]), i(&fx["end"]), false);
    first.parameters[0] = -1.0; first.points[0][0] = -1.0;
    assert_eq!(nodes, before);
    assert_same(&json!(second.parameters), &fx["expected"]["parameters"], "parameters");
    assert_same(&points(&second.points), &fx["expected"]["points"], "points");
}

#[test]
fn interval_fit_costs_and_control_points_match_all_2784_branches() {
    let costs = common::read_json("fitting-cost-reference.json");
    let cases = costs["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2784);
    for (index, fx) in cases.iter().enumerate() {
        let mut nodes = fit_nodes(&fx["nodes"]); let contour = usizes(&fx["contour"]);
        let actual = fit_contour_interval(&mut nodes, &contour, i(&fx["start"]), i(&fx["end"]), fx["markInterior"].as_bool().unwrap());
        let got = json!({"cost": actual.cost, "curve": points(&actual.curve), "kinds": kinds(&nodes)});
        assert_same(&got, &without_random(&fx["expected"]), &format!("original fit cost {index}"));
    }
}

#[test]
fn keeper_partitions_match_all_3907_original_states() {
    let keepers = common::read_json("fitting-keepers-reference.json");
    let cases = keepers["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3907);
    for (index, fx) in cases.iter().enumerate() {
        let mut nodes = fit_nodes(&fx["nodes"]); let before = nodes.clone(); let contour = usizes(&fx["contour"]);
        let p = partition_contour_interval(&mut nodes, &contour, i(&fx["start"]), i(&fx["end"]), f(&fx["initialThreshold"]),
            f(&fx["finalThreshold"]), i(&fx["maxIterations"]), f(&fx["fraction"])).unwrap();
        let got = json!({"records": p.records.iter().map(|r| json!({"slot": r.slot, "start": r.start, "count": r.count,
            "next": r.next, "cost": r.cost, "dirty": r.dirty})).collect::<Vec<_>>(), "threshold": p.threshold});
        assert_same(&got, &without_random(&fx["expected"]), &format!("original keeper partition {index}"));
        assert_eq!(nodes, before);
    }
}

fn fit_fixture(fx: &Value) -> Value {
    fit_contours(&geometry(&fx["geometry"]), &FittingParameters::from_json(&fx["parameters"]), None).unwrap().to_json()
}

#[test]
fn complete_fitting_matches_all_15_pipeline_targets() {
    let pipeline = common::read_json("pipeline-fitting-reference.json");
    let cases = pipeline["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for fx in cases {
        assert_same(&fit_fixture(fx), &fx["expected"], fx["id"].as_str().unwrap());
        assert_eq!(fx["randomStateBefore"], fx["randomStateAfter"]);
    }
}

#[test]
fn complete_fitting_matches_all_2712_full_corpus_geometries() {
    let complete = common::read_json("fitting-full-reference.json");
    let cases = complete["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2712);
    let mut profiles = std::collections::BTreeSet::new();
    for (index, fx) in cases.iter().enumerate() {
        assert_same(&fit_fixture(fx), &fx["expected"], &format!("complete original fitting {index}"));
        assert_eq!(fx["randomStateBefore"], fx["randomStateAfter"]);
        if let Some(p) = fx.get("fullProfile") { profiles.insert(i(p)); }
    }
    assert_eq!(profiles.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn public_fitting_validates_geometry_and_controls() {
    let pipeline = common::read_json("pipeline-fitting-reference.json");
    let fx = &pipeline["cases"][0];
    let base = geometry(&fx["geometry"]); let params = FittingParameters::from_json(&fx["parameters"]);
    let mut g1 = base.clone(); g1.nodes[0].x = f64::INFINITY;
    let mut g2 = base.clone(); g2.regions[0].contour[0] = base.nodes.len();
    let mut g3 = base.clone(); g3.regions[0].boundary.pop();
    let mut p4 = params; p4.stat_thresh_initial = 0.0;
    let mut p5 = params; p5.max_iterations = -1.0;
    for (g, p) in [(&g1, params), (&g2, params), (&g3, params), (&base, p4), (&base, p5)] {
        let error = fit_contours(g, &p, None).unwrap_err();
        assert!(error.contains("Fitting needs"), "{error}");
    }
    let mut fitted = fit_contours(&base, &params, None).unwrap();
    fitted.nodes[0].x = -1.0;
    assert_same(&fit_fixture(fx), &fx["expected"], "repeatable");
}

#[test]
fn fitted_geometry_json_uses_strict_canonical_types_and_preserves_the_wire_shape() {
    use openvector::contour_fitting::FittedGeometry;
    let reference = common::read_json("pipeline-fitting-reference.json");
    for fx in reference["cases"].as_array().unwrap() {
        let parsed = FittedGeometry::from_json(&fx["expected"]).unwrap();
        assert_same(&parsed.to_json(), &fx["expected"], "fitted JSON roundtrip");
        assert_same(&serde_json::to_value(&parsed).unwrap(), &fx["expected"], "canonical serde output");
        assert!(parsed.regions.iter().all(|r| r.boundary.is_empty()));
    }
    let base = &reference["cases"][0]["expected"];
    for value in [json!(-1), json!(256), json!(1.5), json!("1")] {
        let mut invalid = base.clone();
        invalid["nodes"][0]["kind"] = value;
        assert!(FittedGeometry::from_json(&invalid).is_err());
    }
    let mut invalid = base.clone(); invalid["cubicStorageSlots"] = json!(-1);
    assert!(FittedGeometry::from_json(&invalid).is_err());
    let mut invalid = base.clone(); invalid["regions"][0]["bgra"] = json!([0, 0, 0]);
    assert!(FittedGeometry::from_json(&invalid).is_err());
}

#[test]
fn fitting_observers_receive_live_partial_state_and_cancellation_retains_it() {
    use openvector::contour_fitting::{ContourFitter, FittingProgress};
    let reference = common::read_json("pipeline-fitting-reference.json");
    let fx = reference["cases"].as_array().unwrap().iter()
        .find(|f| f["expected"]["cubicStorageSlots"].as_u64().unwrap() > 0).unwrap();
    let g = geometry(&fx["geometry"]);
    let parameters = FittingParameters::from_json(&fx["parameters"]);
    for stop in [-1, 0, 1, 2, 3, -2] {
        let mut fitter = ContourFitter::new(&g).unwrap();
        let mut observed = None;
        let mut observer = |event: &FittingProgress, live: &ContourFitter| {
            if event.substep == stop {
                observed = Some(live.snapshot());
                Err("cancelled".to_string())
            } else { Ok(()) }
        };
        assert_eq!(fitter.run(&parameters, Some(&mut observer)).unwrap_err(), "cancelled");
        assert_eq!(fitter.snapshot(), observed.unwrap(), "live checkpoint {stop}");
    }
    let mut fitter = ContourFitter::new(&g).unwrap();
    let mut events = Vec::new();
    let mut observer = |event: &FittingProgress, live: &ContourFitter| {
        events.push((event.substep, event.counter));
        assert_eq!(live.snapshot().nodes.len(), g.nodes.len());
        Ok(())
    };
    let result = fitter.run(&parameters, Some(&mut observer)).unwrap();
    assert_same(&result.to_json(), &fx["expected"], "observed fitting result");
    assert_eq!(events.first(), Some(&(-1, 0)));
    assert_eq!(events.last(), Some(&(-2, 0)));
    assert!(events.windows(2).all(|pair| pair[0] != pair[1]));
}
