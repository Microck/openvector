mod common;
mod smoothing_support;
use openvector::contour_neighborhoods::prepare_contour_neighborhoods;
use openvector::smoothing::smoothing_optimize;
use openvector::smoothing_repair::{ContourRepair, RepairView};
use serde_json::{json, Value};
use smoothing_support::*;
use std::collections::BTreeSet;

fn original_neighborhoods(fx: &Value) -> Vec<Vec<[usize; 2]>> {
    let actual = prepare_contour_neighborhoods(&raw_geometry(fx)).unwrap().nearby_edges;
    assert_same(&json!(actual), &fx["nearbyEdges"], "independently prepared repair associations");
    actual
}

#[test]
fn neighborhoods_match_all_2814_native_ordered_lists_and_flags() {
    let reference = common::read_json("contour-neighborhoods-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2814);
    let (mut nodes, mut associations) = (0, 0);
    let mut variants = BTreeSet::new();
    for (index, fx) in cases.iter().enumerate() {
        let actual = prepare_contour_neighborhoods(&geometry(&fx["geometry"])).unwrap();
        assert_same(&json!({"nearbyEdges": actual.nearby_edges, "flags": actual.flags}), &fx["expected"], &format!("neighborhood {index}"));
        nodes += actual.flags.len();
        associations += actual.nearby_edges.iter().map(Vec::len).sum::<usize>();
        if let Some(v) = fx.get("neighborhoodVariant") { variants.insert(i(v)); }
    }
    assert_eq!(nodes, 64518);
    assert_eq!(associations, 1045003);
    assert_eq!(variants.len(), 12);
}

#[test]
fn neighborhoods_own_outputs_and_reject_invalid_geometry() {
    let reference = common::read_json("contour-neighborhoods-reference.json");
    let fx = &reference["cases"][0];
    let base = geometry(&fx["geometry"]);
    let before = base.clone();
    let mut actual = prepare_contour_neighborhoods(&base).unwrap();
    actual.flags.fill(255);
    for edges in &mut actual.nearby_edges { edges.fill([999, 999]); edges.push([0, 0]); }
    assert_eq!(base, before);
    let mut nonfinite = base.clone(); nonfinite.nodes[0].x = f64::NAN;
    let mut bad_id = base.clone(); bad_id.regions[0].contour[0] = base.nodes.len();
    let mut short = base.clone(); short.regions[0].contour = vec![0, 1];
    let mut few_samples = base.clone();
    few_samples.nodes = vec![base.nodes[0].clone(); 60];
    few_samples.regions.truncate(1); few_samples.regions[0].contour = vec![0, 1, 2];
    let empty = openvector::contours::Geometry { nodes: vec![], regions: vec![] };
    for invalid in [empty, nonfinite, bad_id, short, few_samples] { assert!(prepare_contour_neighborhoods(&invalid).is_err()); }
    let again = prepare_contour_neighborhoods(&base).unwrap();
    assert_same(&json!({"nearbyEdges": again.nearby_edges, "flags": again.flags}), &fx["expected"], "owned neighborhoods");
}

#[test]
fn repairs_match_all_14106_native_states_in_1425_persistent_sequences() {
    let reference = common::read_json("smoothing-repair-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1425);
    let (mut states, mut image_calls, mut nonempty) = (0, 0, 0);
    let mut kinds = BTreeSet::new();
    for (index, fx) in cases.iter().enumerate() {
        let mut g = geometry(&fx["geometry"]);
        let mut penalties = bytes(&fx["penaltyCounts"]);
        let mut repair = ContourRepair::new(g.regions.len(), original_neighborhoods(fx));
        let mut model = image_model(fx);
        let image = model.read_kernel(&g).unwrap();
        assert_eq!(i(&fx["repairInterval"]), 10);
        for (step, action) in fx["actions"].as_array().unwrap().iter().enumerate() {
            let positions = f64s(&action["positions"]);
            let points: Vec<[f64; 2]> = positions.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
            let mut view = RepairView { geometry: &mut g, penalty_counts: &mut penalties, points: &points };
            let kind = i(&action["kind"]);
            let changed = match kind {
                0 => repair.all(&mut view),
                1 => repair.region(&mut view, i(&action["region"]) as usize),
                2 => repair.decay(&mut view),
                3 => {
                    image.energy(&positions).unwrap();
                    assert_same(&json!(image.invalid_regions()), &action["invalidRegions"], &format!("invalid regions {index}/{step}"));
                    let identifiers = image.take_invalid_regions();
                    let changed = i64::from(repair.invalid(&mut view, &identifiers));
                    assert!(image.invalid_regions().is_empty());
                    image_calls += 1;
                    if !identifiers.is_empty() { nonempty += 1; }
                    changed
                }
                _ => panic!("unknown native repair action {kind}"),
            };
            let mut actual = repair.snapshot(&view);
            actual["changed"] = json!(changed);
            assert_same(&actual, &fx["expected"][step], &format!("repair {index}/{step}"));
            kinds.insert(kind);
            states += 1;
        }
        assert!(penalties.iter().all(|&p| p <= 1));
    }
    assert_eq!(kinds.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert_eq!((states, image_calls, nonempty), (14106, 2850, 274));
}

#[test]
fn repaired_optimizers_match_all_1029_native_trajectories() {
    let reference = common::read_json("smoothing-repair-optimizer-reference.json");
    let without = common::read_json("smoothing-image-optimizer-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1029);
    let (mut changed_positions, mut changed_restarts, mut long_runs) = (0, 0, 0);
    let (mut phases, mut line_types) = (BTreeSet::new(), BTreeSet::new());
    for (index, fx) in cases.iter().enumerate() {
        let mut owned = Owned::new(fx);
        owned.nearby = Some(original_neighborhoods(fx));
        let before = owned.geometry.clone();
        let mut image = image_model(fx);
        let mut input = owned.input(fx);
        input.image_model = Some(&mut image);
        let actual = smoothing_optimize(&mut input, None).unwrap();
        assert_same(&actual.to_json(), &fx["expected"], &format!("repaired optimizer {index}"));
        assert_eq!(owned.geometry, before);
        let original = &without["cases"][i(&fx["sourceImageOptimizer"]) as usize]["expected"];
        if actual.positions != f64s(&original["positions"]) { changed_positions += 1; }
        if actual.restarts != i(&original["restarts"]) { changed_restarts += 1; }
        if actual.iterations >= i(&fx["repairInterval"]) { long_runs += 1; }
        let phase = i(&fx["phase"]);
        phases.insert(phase);
        line_types.insert(i(&fx["parameters"][format!("ContourSmoother::phase_{phase}.cg_line_search_type")]));
    }
    assert_eq!(phases.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(line_types.into_iter().collect::<Vec<_>>(), vec![0, 5]);
    assert!(changed_positions > 0 && changed_restarts > 0 && long_runs > 0);
}

#[test]
fn invalid_repair_associations_fail_before_image_mutation() {
    let reference = common::read_json("smoothing-repair-optimizer-reference.json");
    let fx = reference["cases"].as_array().unwrap().iter().find(|f| f["geometry"]["regions"].as_array().unwrap().len() > 1).unwrap();
    let mut image = image_model(fx);
    let positions: Vec<f64> = geometry(&fx["geometry"]).nodes.iter().flat_map(|n| [n.x, n.y]).collect();
    let initial = image.evaluate(&positions).unwrap();
    for variant in 0..5 {
        let mut owned = Owned::new(fx);
        let interval = match variant { 2 => 0.0, 3 => 1.5, _ => f(&fx["repairInterval"]) };
        match variant {
            0 => owned.nearby = Some(vec![]),
            1 => owned.nearby.as_mut().unwrap()[0] = vec![[owned.geometry.regions.len(), 0]],
            4 => owned.nearby = Some(vec![vec![[0, owned.geometry.regions[0].contour.len()]]; owned.geometry.nodes.len()]),
            _ => {}
        }
        let mut input = owned.input(fx);
        input.repair_interval = interval;
        input.image_model = Some(&mut image);
        assert!(smoothing_optimize(&mut input, None).is_err());
    }
    assert_eq!(image.evaluate(&positions).unwrap(), initial);
}
