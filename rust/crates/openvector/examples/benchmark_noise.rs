//! Noise-reduction-only timings and exact input/output fingerprints.
//! Build this same example against both revisions; compare with
//! docs/performance/benchmark-noise.py. Fixture decoding and hashing are untimed.

use base64::Engine;
use openvector::image_preparation::reduce_prepared_noise;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

struct Case { id: String, width: usize, height: usize, bgra: Vec<u8>, amount: u32 }

fn cases() -> Vec<Case> {
    let fixtures = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from).unwrap_or_else(||
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let mut cases = Vec::new();
    for file in ["fresh-representative-20261020.ndjson.gz", "fresh-photographic-20261020.ndjson.gz"] {
        let raw = std::fs::read(fixtures.join(file)).expect(file);
        let mut text = String::new();
        flate2::read::MultiGzDecoder::new(&raw[..]).read_to_string(&mut text).unwrap();
        for line in text.lines().skip(1).filter(|s| !s.is_empty()) {
            let row: Value = serde_json::from_str(line).unwrap();
            let state = &row["expectedAutomatic"]["state"];
            let width = state["segmentation"]["width"].as_u64().unwrap() as usize;
            let height = state["segmentation"]["height"].as_u64().unwrap() as usize;
            let bgra = base64::engine::general_purpose::STANDARD.decode(state["workingBgra"].as_str().unwrap()).unwrap();
            assert_eq!(bgra.len(), width * height * 4);
            for amount in 1..=3 {
                cases.push(Case { id: format!("{}/amount-{amount}", row["id"].as_str().unwrap()),
                    width, height, bgra: bgra.clone(), amount });
            }
        }
    }
    for pattern in ["flat", "noisy"] {
        let (width, height) = (512, 384);
        let mut seed = 0x962b_a713u32;
        let mut bgra = vec![0; width * height * 4];
        for pixel in bgra.chunks_exact_mut(4) {
            for channel in 0..4 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                pixel[channel] = if pattern == "flat" { [32, 96, 192, 255][channel] }
                    else if channel == 3 { 255 } else { (seed >> 24) as u8 };
            }
        }
        for amount in 1..=3 {
            cases.push(Case { id: format!("authored/{pattern}-{width}x{height}/amount-{amount}"),
                width, height, bgra: bgra.clone(), amount });
        }
    }
    cases
}

fn main() {
    let mut warmup = 1usize;
    let mut filter = String::new();
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--warmup=") { warmup = value.parse().unwrap(); }
        else if let Some(value) = arg.strip_prefix("--case=") { filter = value.into(); }
        else { panic!("unknown argument {arg}"); }
    }
    let mut rows = Vec::new();
    for case in cases().into_iter().filter(|case| case.id.contains(&filter)) {
        let output = reduce_prepared_noise(case.width, case.height, &case.bgra, case.amount).unwrap();
        let input_sha256 = format!("{:x}", Sha256::digest(&case.bgra));
        let output_sha256 = format!("{:x}", Sha256::digest(&output.bgra));
        assert_eq!((output.width, output.height), (case.width, case.height));
        let iterations = (16_384 / (case.width * case.height)).clamp(1, 32);
        let run = || {
            for _ in 0..iterations {
                black_box(reduce_prepared_noise(black_box(case.width), black_box(case.height),
                    black_box(&case.bgra), black_box(case.amount)).unwrap());
            }
        };
        for _ in 0..warmup { run(); }
        let started = Instant::now();
        run();
        let milliseconds = started.elapsed().as_secs_f64() * 1000.0 / iterations as f64;
        assert_eq!(format!("{:x}", Sha256::digest(&case.bgra)), input_sha256);
        rows.push(json!({ "id": case.id, "width": case.width, "height": case.height,
            "amount": case.amount, "input_sha256": input_sha256, "output_sha256": output_sha256,
            "iterations": iterations, "milliseconds": milliseconds }));
    }
    assert!(!rows.is_empty(), "case filter matched nothing");
    println!("{}", serde_json::to_string_pretty(&json!({
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "timed_operation": "reduce_prepared_noise (including output allocation and drop)", "cases": rows,
    })).unwrap());
}
