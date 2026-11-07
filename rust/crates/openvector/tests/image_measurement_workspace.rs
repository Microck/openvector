//! Regression for persistent pixel lists as contours enter and leave pixels.
mod common;
mod smoothing_support;
use openvector::image_measurement::ContourImageModel;
use serde_json::json;
use sha2::{Digest, Sha256};
use smoothing_support::*;

#[test]
fn moving_contours_preserve_all_persistent_image_measurement_fields() {
    let pipeline = common::read_json("pipeline-smoothing-reference.json");
    let segmentation = common::read_json("pipeline-segmentation-reference.json");
    let fx = &pipeline["cases"][2];
    let geometry = geometry(&fx["before"]);
    let mut model = ContourImageModel::new(i(&fx["width"]) as f64, i(&fx["height"]) as f64,
        &bytes(&fx["bgra"]), &i64s(&segmentation["cases"][2]["expected"]["indices"]), &geometry).unwrap();
    let positions: Vec<f64> = geometry.nodes.iter().flat_map(|n| [n.x, n.y]).collect();
    let sequence: Vec<Vec<f64>> = [0.0, 0.125, -0.125, 0.0].into_iter()
        .map(|offset| positions.iter().map(|p| p + offset).collect()).collect();
    let kernel = model.read_kernel(&geometry).unwrap();
    let mut gradient = vec![0.0; positions.len()];
    // Repeat the coordinate cycle to cover reuse, including pixels clipped away.
    for step in 0..44 {
        let p = &sequence[step % sequence.len()];
        kernel.energy(p).unwrap();
        gradient.fill(0.0);
        kernel.add_gradient(&mut gradient, p);
    }
    let mut hash = Sha256::new();
    for p in &sequence {
        kernel.energy(p).unwrap();
        gradient.fill(0.0);
        kernel.add_gradient(&mut gradient, p);
        let m = kernel.snapshot(gradient.clone());
        assert!(m.energy.is_finite() && m.gradient.iter().all(|v| v.is_finite()));
        // Keep key order independent of serde_json's optional preserve_order feature.
        let snapshot = std::collections::BTreeMap::from([
            ("energy", json!(m.energy)), ("gradient", json!(m.gradient)),
            ("colors", json!(m.colors)), ("flags", json!(m.flags)),
            ("pixels", json!(m.pixels)), ("invalid", json!(kernel.invalid_regions())),
        ]);
        hash.update(serde_json::to_vec(&snapshot).unwrap());
    }
    // Captured from unchanged main 0efa9aad869e6b58cd7947f3172f44b8a6144215,
    // using this authored coordinate sequence and checked-in ring input.
    assert_eq!(format!("{:x}", hash.finalize()), "c008c4e2712ac8f47cff30762a29bd7bdd7121e8b53e5900b2f531fbdea84af8");
}
