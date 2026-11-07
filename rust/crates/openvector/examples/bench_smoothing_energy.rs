//! Persistent image-objective benchmark, including large authored scaled geometry.
//! Run in release mode with an optional integer scale and evaluation count.
#[path = "../tests/smoothing_support/mod.rs"]
mod support;
use support::*;
use openvector::image_measurement::ContourImageModel;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scale: usize = args.get(1).map_or(16, |v| v.parse().unwrap());
    let repetitions: usize = args.get(2).map_or(40, |v| v.parse().unwrap());
    assert!(scale > 0 && repetitions > 0);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures");
    let pipeline: Value = serde_json::from_slice(&std::fs::read(root.join("pipeline-smoothing-reference.json")).unwrap()).unwrap();
    let segmentation: Value = serde_json::from_slice(&std::fs::read(root.join("pipeline-segmentation-reference.json")).unwrap()).unwrap();
    let full = args.get(3).is_some_and(|v| v == "full");
    let case = if full { 7 } else { 2 };
    let fx = &pipeline["cases"][case]; // Ring, three regions; profile 6 for full smoothing.
    let (w, h) = (i(&fx["width"]) as usize, i(&fx["height"]) as usize);
    let pixels = bytes(&fx["bgra"]);
    let labels = i64s(&segmentation["cases"][case]["expected"]["indices"]);
    let (mut bgra, mut indices) = (Vec::new(), Vec::new());
    for y in 0..h * scale { for x in 0..w * scale {
        let p = (y / scale) * w + x / scale;
        bgra.extend_from_slice(&pixels[p * 4..p * 4 + 4]); indices.push(labels[p]);
    }}
    let mut geometry = geometry(&fx["before"]);
    for n in &mut geometry.nodes { n.x *= scale as f64; n.y *= scale as f64; }
    for r in &mut geometry.regions { r.pixel_count *= (scale * scale) as i32; }
    if full {
        use openvector::contour_smoothing::{smooth_contour_geometry, SmoothContourInput};
        for r in &mut geometry.regions { for boundary in &mut r.boundary { boundary[2] *= scale as i32; } }
        let palette = f64s(&segmentation["cases"][case]["expected"]["palette"]);
        let input = SmoothContourInput { width: (w * scale) as i64, height: (h * scale) as i64, bgra: &bgra,
            geometry: &geometry, indices: &indices, palette: &palette, parameters: &fx["parameters"] };
        let run = || {
            let mut state = i(&fx["randomStateBefore"]);
            let result = smooth_contour_geometry(&input, &mut || next_uniform(&mut state), None).unwrap();
            (result, state)
        };
        let expected = run();
        let start = Instant::now();
        for _ in 0..repetitions { black_box(run()); }
        let milliseconds = start.elapsed().as_secs_f64() * 1000.0 / repetitions as f64;
        assert_eq!(run(), expected);
        let digest = Sha256::digest(serde_json::to_vec(&expected).unwrap());
        println!("full-{}x{}\t{}\t{:.6}\t{:x}", w * scale, h * scale, repetitions, milliseconds, digest);
        return;
    }
    let mut model = ContourImageModel::new((w * scale) as f64, (h * scale) as f64, &bgra, &indices, &geometry).unwrap();
    let positions: Vec<f64> = geometry.nodes.iter().flat_map(|n| [n.x, n.y]).collect();
    // Exercise persistent caches with a changing contour, including returning to the start.
    let sequence: Vec<Vec<f64>> = [0.0, 0.125, -0.125, 0.0].into_iter().map(|offset| positions.iter().map(|p| p + offset).collect()).collect();
    if args.get(3).is_some_and(|v| v == "cold") {
        let mut elapsed = std::time::Duration::ZERO;
        for _ in 0..repetitions {
            let mut fresh = model.clone();
            let kernel = fresh.read_kernel(&geometry).unwrap();
            let mut gradient = vec![0.0; positions.len()];
            let start = Instant::now();
            black_box(kernel.energy(&positions).unwrap());
            kernel.add_gradient(&mut gradient, &positions);
            elapsed += start.elapsed();
            black_box(&gradient);
        }
        let kernel = model.read_kernel(&geometry).unwrap();
        kernel.energy(&positions).unwrap();
        let mut gradient = vec![0.0; positions.len()];
        kernel.add_gradient(&mut gradient, &positions);
        let m = kernel.snapshot(gradient);
        let snapshot = json!([m.energy,m.gradient,m.colors,m.flags,m.pixels,kernel.invalid_regions()]);
        let digest = Sha256::digest(serde_json::to_vec(&snapshot).unwrap());
        println!("cold-{}x{}\t{}\t{:.6}\t{:x}", w * scale, h * scale, repetitions,
            elapsed.as_secs_f64() * 1000.0 / repetitions as f64, digest);
        return;
    }
    let kernel = model.read_kernel(&geometry).unwrap();
    let mut gradient = vec![0.0; positions.len()];
    for p in &sequence { black_box(kernel.energy(p).unwrap()); gradient.fill(0.0); kernel.add_gradient(&mut gradient, p); }
    let start = Instant::now();
    for step in 0..repetitions {
        let p = &sequence[step % sequence.len()];
        black_box(kernel.energy(p).unwrap()); gradient.fill(0.0); kernel.add_gradient(&mut gradient, p); black_box(&gradient);
    }
    let milliseconds = start.elapsed().as_secs_f64() * 1000.0 / repetitions as f64;
    // Hash full public snapshots outside the timed region, for exact A/B comparison.
    let mut hash = Sha256::new();
    for p in &sequence {
        kernel.energy(p).unwrap(); gradient.fill(0.0); kernel.add_gradient(&mut gradient, p);
        let m = kernel.snapshot(gradient.clone());
        hash.update(serde_json::to_vec(&json!({"energy":m.energy,"gradient":m.gradient,"colors":m.colors,"flags":m.flags,"pixels":m.pixels,"invalid":kernel.invalid_regions()})).unwrap());
    }
    println!("{}x{}\t{}\t{:.6}\t{:x}", w * scale, h * scale, repetitions, milliseconds, hash.finalize());
}
