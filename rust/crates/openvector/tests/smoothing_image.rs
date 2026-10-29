mod common;
mod smoothing_support;
use openvector::smoothing::{smoothing_line_search, smoothing_objective, smoothing_optimize};
use serde_json::json;
use smoothing_support::*;
use std::collections::BTreeSet;

#[test]
fn image_objectives_match_all_2799_native_states() {
    let reference = common::read_json("smoothing-image-objective-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2799);
    for (index, fx) in cases.iter().enumerate() {
        let owned = Owned::new(fx);
        let mut model = image_model(fx);
        let mut input = owned.input(fx);
        input.image_model = Some(&mut model);
        let actual = smoothing_objective(&mut input).unwrap();
        assert_same(&json!({"energy": actual.energy, "gradient": actual.gradient}), &fx["expected"], &format!("image objective {index}"));
    }
}

#[test]
fn image_line_searches_match_all_3312_native_controllers() {
    let reference = common::read_json("smoothing-image-line-search-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3312);
    for (index, fx) in cases.iter().enumerate() {
        let owned = Owned::new(fx);
        let mut model = image_model(fx);
        let mut input = owned.input(fx);
        input.image_model = Some(&mut model);
        assert_same(&smoothing_line_search(&mut input).unwrap().to_json(), &fx["expected"], &format!("image line search {index}"));
    }
}

#[test]
fn image_optimizers_match_all_1029_native_runs() {
    let reference = common::read_json("smoothing-image-optimizer-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1029);
    let (mut phases, mut line_types) = (BTreeSet::new(), BTreeSet::new());
    let (mut early, mut restarts) = (0, 0);
    for (index, fx) in cases.iter().enumerate() {
        let owned = Owned::new(fx);
        let before = owned.geometry.clone();
        let mut model = image_model(fx);
        let mut input = owned.input(fx);
        input.image_model = Some(&mut model);
        let actual = smoothing_optimize(&mut input, None).unwrap();
        assert_same(&actual.to_json(), &fx["expected"], &format!("image optimizer {index}"));
        assert_eq!(owned.geometry, before);
        let phase = i(&fx["phase"]);
        phases.insert(phase);
        line_types.insert(i(&fx["parameters"][format!("ContourSmoother::phase_{phase}.cg_line_search_type")]));
        if actual.iterations < i(&fx["parameters"][format!("ContourSmoother::phase_{phase}.cg_max_iter")]) { early += 1; }
        restarts += actual.restarts;
    }
    assert_eq!(phases.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(line_types.into_iter().collect::<Vec<_>>(), vec![0, 5]);
    assert!(early > 0 && restarts > 0);
}

#[test]
fn persistent_image_objectives_match_all_6981_sequence_states() {
    let reference = common::read_json("smoothing-image-objective-sequence-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1425);
    let mut states = 0;
    for (index, fx) in cases.iter().enumerate() {
        let mut model = image_model(fx);
        let mut owned = Owned::new(fx);
        for (step, positions) in fx["positions"].as_array().unwrap().iter().enumerate() {
            let positions = f64s(positions);
            for (node, pair) in owned.geometry.nodes.iter_mut().zip(positions.chunks_exact(2)) { node.x = pair[0]; node.y = pair[1]; }
            let mut input = owned.input(fx);
            input.image_model = Some(&mut model);
            let actual = smoothing_objective(&mut input).unwrap();
            assert_same(&json!({"energy": actual.energy, "gradient": actual.gradient}), &fx["expected"][step], &format!("image objective sequence {index}/{step}"));
            states += 1;
        }
    }
    assert_eq!(states, 6981);
}

#[test]
fn image_binding_validation_preserves_the_persistent_cache() {
    let reference = common::read_json("smoothing-image-objective-sequence-reference.json");
    let fx = &reference["cases"][0];
    let mut model = image_model(fx);
    let owned = Owned::new(fx);
    let before = owned.geometry.clone();
    let mut input = owned.input(fx);
    input.image_model = Some(&mut model);
    smoothing_objective(&mut input).unwrap().gradient.fill(7.0);
    assert_eq!(owned.geometry, before);
    assert!(smoothing_objective(&mut owned.input(fx)).is_err());
    for alter_count in [false, true] {
        let mut wrong = Owned::new(fx);
        if alter_count { wrong.geometry.regions[0].pixel_count += 1; } else { wrong.geometry.regions[0].contour.reverse(); }
        let mut input = wrong.input(fx);
        input.image_model = Some(&mut model);
        assert!(smoothing_objective(&mut input).is_err());
    }
    let direction = vec![0.0; owned.geometry.nodes.len() * 2];
    let mut input = owned.input(fx);
    input.image_model = Some(&mut model);
    input.width += 1.0;
    input.direction = &direction;
    input.epsilon = 0.01;
    assert!(smoothing_line_search(&mut input).is_err());
    let mut moved = Owned::new(fx);
    let positions = f64s(&fx["positions"][1]);
    for (node, pair) in moved.geometry.nodes.iter_mut().zip(positions.chunks_exact(2)) { node.x = pair[0]; node.y = pair[1]; }
    let mut input = moved.input(fx);
    input.image_model = Some(&mut model);
    let actual = smoothing_objective(&mut input).unwrap();
    assert_same(&json!({"energy": actual.energy, "gradient": actual.gradient}), &fx["expected"][1], "cache after rejected bindings");
}
