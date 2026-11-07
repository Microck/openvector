//! Reproducible smoothing-only timings with exact output/progress fingerprints.
//!
//! cargo run --release --manifest-path rust/Cargo.toml -p openvector \
//!   --example benchmark_smoothing -- --samples=7 --warmup=2 --output=before.json
//! Repeat with the same compiler, machine and arguments after the change.
//! Use --case=<substring> to select cases, --verify-observers to fingerprint every
//! progress snapshot and three deterministic cancellation boundaries, and
//! --snapshots to include the complete final geometry as well as its SHA-256.
//! Fixture parsing, contour extraction, reference-output validation and hashing
//! are not timed. Each case also has one untimed correctness run before warmup.

use base64::Engine;
use openvector::contour_smoothing::{
    smooth_contour_geometry, SmoothContourInput, SmoothedContours, SmoothingObserver,
    SmoothingProgress, SmoothingSnapshot,
};
use openvector::contours::{extract_contours, ContourInput, Geometry};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

struct Case {
    id: String,
    width: i64,
    height: i64,
    bgra: Vec<u8>,
    geometry: Geometry,
    indices: Vec<i64>,
    palette: Vec<f64>,
    parameters: Value,
    random_state: i64,
    expected: Option<(Geometry, Vec<u8>, i64)>,
}

impl Case {
    fn run(&self, observer: Option<&mut SmoothingObserver>) -> (Result<SmoothedContours, String>, i64) {
        let mut state = self.random_state;
        let result = smooth_contour_geometry(&SmoothContourInput {
            width: self.width, height: self.height, bgra: &self.bgra,
            geometry: &self.geometry, indices: &self.indices, palette: &self.palette,
            parameters: &self.parameters,
        }, &mut || { state = state * 2003 % 9973; state as f64 * (1.0 / 9973.0) }, observer);
        (result, state)
    }
}

fn fixtures() -> PathBuf {
    std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from).unwrap_or_else(||
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"))
}

fn json_file(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixtures().join(name)).expect(name)).expect(name)
}
fn f64s(value: &Value) -> Vec<f64> { value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() }
fn i64s(value: &Value) -> Vec<i64> { value.as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect() }
fn bytes(value: &Value) -> Vec<u8> { i64s(value).into_iter().map(|v| u8::try_from(v).unwrap()).collect() }
fn geometry(value: &Value) -> Geometry { Geometry::from_json(value).expect("canonical fixture geometry") }

fn pipeline_cases() -> Vec<Case> {
    let smoothing = json_file("pipeline-smoothing-reference.json");
    let segmentation = json_file("pipeline-segmentation-reference.json");
    let smoothing = smoothing["cases"].as_array().unwrap();
    let segmentation = segmentation["cases"].as_array().unwrap();
    assert_eq!(smoothing.len(), segmentation.len(), "paired pipeline fixture counts");
    smoothing.iter().zip(segmentation).map(|(fx, seg)| {
        assert_eq!(fx["id"], seg["id"], "paired pipeline fixture IDs");
        Case {
        id: format!("pipeline/{}", fx["id"].as_str().unwrap()),
        width: fx["width"].as_i64().unwrap(), height: fx["height"].as_i64().unwrap(),
        bgra: bytes(&fx["bgra"]), geometry: geometry(&fx["before"]),
        indices: i64s(&seg["expected"]["indices"]), palette: f64s(&seg["expected"]["palette"]),
        parameters: fx["parameters"].clone(), random_state: fx["randomStateBefore"].as_i64().unwrap(),
        expected: Some((geometry(&fx["expected"]), bytes(&fx["stateMetadata"]["penaltyCounts"]), fx["randomStateAfter"].as_i64().unwrap())),
    }}).collect()
}

fn extract(width: i64, height: i64, indices: &[i64], counts: &[i64], colors: &[i64], palette: &[f64], aa: bool) -> Geometry {
    extract_contours(&ContourInput { width: width as f64, height: height as f64,
        indices, counts, color_indices: colors, palette, anti_alias: Some(aa) }).unwrap()
}

// Reuse recorded real input pixels, completed segmentation and exact parameter
// bits. Re-extract the pre-smoothing contours outside the timer; the captured
// `contours` field already contains the post-smoothing result. Seed 1 is explicit
// here, so these cases compare revisions, not the recorder's final RNG state.
fn representative_cases(name: &str) -> Vec<Case> {
    let raw = std::fs::read(fixtures().join(name)).expect(name);
    let mut decoded = String::new();
    flate2::read::MultiGzDecoder::new(&raw[..]).read_to_string(&mut decoded).unwrap();
    let rows: Vec<Value> = decoded.lines().filter(|s| !s.is_empty()).map(|s| serde_json::from_str(s).unwrap()).collect();
    let registry = rows[0]["parameters"].as_object().unwrap();
    rows.iter().skip(1).map(|row| {
        let state = &row["expectedAutomatic"]["state"];
        let seg = &state["segmentation"];
        let width = seg["width"].as_i64().unwrap();
        let height = seg["height"].as_i64().unwrap();
        let mut parameters = Map::new();
        for (key, descriptor) in registry {
            let text = state["parameterBits"][key].as_str().unwrap();
            let bits: Vec<u8> = text.as_bytes().chunks_exact(2).map(|p|
                u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap()).collect();
            let kind = descriptor["kind"].as_str().unwrap();
            let values: Vec<Value> = match kind {
                "int32" => bits.chunks_exact(4).map(|p| json!(i32::from_le_bytes(p.try_into().unwrap()))).collect(),
                "float32" => bits.chunks_exact(4).map(|p| json!(f32::from_le_bytes(p.try_into().unwrap()) as f64)).collect(),
                "float64" => bits.chunks_exact(8).map(|p| json!(f64::from_le_bytes(p.try_into().unwrap()))).collect(),
                _ => panic!("unsupported parameter type {kind}"),
            };
            assert_eq!(values.len(), descriptor["count"].as_u64().unwrap() as usize);
            parameters.insert(key.clone(), if values.len() == 1 { values[0].clone() } else { json!(values) });
        }
        let indices = i64s(&seg["indices"]);
        let counts = i64s(&seg["counts"]);
        let colors = i64s(&seg["colorIndices"]);
        let palette = f64s(&seg["palette"]);
        let aa = parameters["Shared::is_anti_aliased"].as_f64() == Some(1.0);
        let geometry = extract(width, height, &indices, &counts, &colors, &palette, aa);
        Case { id: row["id"].as_str().unwrap().to_string(), width, height,
            bgra: base64::engine::general_purpose::STANDARD.decode(state["workingBgra"].as_str().unwrap()).unwrap(),
            geometry, indices, palette, parameters: Value::Object(parameters), random_state: 1, expected: None }
    }).collect()
}

fn authored_case(width: usize, height: usize, aa: bool, parameters: &Value) -> Case {
    // Twelve connected blocks; boundaries retain realistic thousands of samples.
    let mut indices = Vec::with_capacity(width * height);
    let mut bgra = Vec::with_capacity(width * height * 4);
    let mut counts = vec![0; 12];
    let mut palette = Vec::with_capacity(48);
    let colors: Vec<[u8; 4]> = (0..12).map(|i| [((i * 47 + 31) % 256) as u8,
        ((i * 83 + 67) % 256) as u8, ((i * 29 + 173) % 256) as u8, 255]).collect();
    for color in &colors { palette.extend(color.iter().map(|&v| (v as f32 * (1.0f32 / 255.0)) as f64)); }
    for y in 0..height { for x in 0..width {
        let region = (y * 3 / height) * 4 + x * 4 / width;
        indices.push(region as i64); counts[region] += 1;
        bgra.extend_from_slice(&colors[region]);
    }}
    let geometry = extract(width as i64, height as i64, &indices, &counts, &(0..12).collect::<Vec<_>>(), &palette, aa);
    Case { id: format!("authored/blocks-{width}x{height}-aa{}", u8::from(aa)),
        width: width as i64, height: height as i64, bgra, geometry, indices, palette,
        parameters: parameters.clone(), random_state: 1, expected: None }
}

fn hash_geometry(hash: &mut Sha256, geometry: &Geometry) {
    hash.update((geometry.nodes.len() as u64).to_le_bytes());
    for n in &geometry.nodes {
        hash.update(n.x.to_bits().to_le_bytes()); hash.update(n.y.to_bits().to_le_bytes());
        hash.update([n.kind, n.flags]);
    }
    hash.update(serde_json::to_vec(&geometry.regions).unwrap());
}
fn hash_snapshot(hash: &mut Sha256, geometry: &Geometry, penalties: &[u8], borders: [i64; 4], references: &[f32]) {
    hash_geometry(hash, geometry);
    hash.update((penalties.len() as u64).to_le_bytes()); hash.update(penalties);
    for b in borders { hash.update(b.to_le_bytes()); }
    hash.update((references.len() as u64).to_le_bytes());
    for &p in references { hash.update(p.to_bits().to_le_bytes()); }
}
fn input_digest(case: &Case) -> String {
    let mut hash = Sha256::new();
    hash.update(case.width.to_le_bytes()); hash.update(case.height.to_le_bytes());
    hash.update(case.random_state.to_le_bytes()); hash.update(&case.bgra);
    hash_geometry(&mut hash, &case.geometry);
    for &index in &case.indices { hash.update(index.to_le_bytes()); }
    for &color in &case.palette { hash.update(color.to_bits().to_le_bytes()); }
    hash.update(serde_json::to_vec(&case.parameters).unwrap());
    format!("{:x}", hash.finalize())
}
fn output_digest(output: &SmoothedContours, state: i64) -> String {
    let mut hash = Sha256::new();
    hash_snapshot(&mut hash, &output.geometry, &output.penalty_counts, output.border_counts, &output.reference_positions);
    hash.update(state.to_le_bytes()); format!("{:x}", hash.finalize())
}
fn exact_geometry(actual: &Geometry, expected: &Geometry) -> bool {
    actual.regions == expected.regions && actual.nodes.len() == expected.nodes.len() &&
        actual.nodes.iter().zip(&expected.nodes).all(|(a, b)| a.x.to_bits() == b.x.to_bits() &&
            a.y.to_bits() == b.y.to_bits() && a.kind == b.kind && a.flags == b.flags)
}
fn hash_event(hash: &mut Sha256, event: &SmoothingProgress, snapshot: &SmoothingSnapshot) {
    for value in [event.phase, event.substep, event.counter, event.percent] { hash.update(value.to_le_bytes()); }
    hash.update(event.fraction.to_bits().to_le_bytes());
    hash_snapshot(hash, snapshot.geometry, snapshot.penalty_counts, snapshot.border_counts, snapshot.reference_positions);
}
fn observer_checks(case: &Case, expected_digest: &str) -> Value {
    let mut count = 0usize;
    let mut hash = Sha256::new();
    let (observed, state) = case.run(Some(&mut |event, snapshot| {
        count += 1; hash_event(&mut hash, event, snapshot); Ok(())
    }));
    assert_eq!(output_digest(&observed.unwrap(), state), expected_digest, "observed {}", case.id);
    let trace = format!("{:x}", hash.finalize());
    let mut boundaries = vec![1, (count / 2).max(1), count];
    boundaries.sort_unstable(); boundaries.dedup();
    let cancellations: Vec<Value> = boundaries.into_iter().map(|boundary| {
        let mut calls = 0usize;
        let mut hash = Sha256::new();
        let (result, state) = case.run(Some(&mut |event, snapshot| {
            calls += 1; hash_event(&mut hash, event, snapshot);
            if calls == boundary { Err("benchmark cancellation".into()) } else { Ok(()) }
        }));
        assert_eq!(result.unwrap_err(), "benchmark cancellation");
        assert_eq!(calls, boundary);
        json!({"boundary": boundary, "random_state": state, "trace_sha256": format!("{:x}", hash.finalize())})
    }).collect();
    json!({"events": count, "trace_sha256": trace, "cancellations": cancellations})
}

fn main() {
    let mut samples = 7usize;
    let mut warmup = 2usize;
    let mut filter = String::new();
    let mut output_path = None;
    let mut snapshots = false;
    let mut verify_observers = false;
    for argument in std::env::args().skip(1) {
        if let Some(v) = argument.strip_prefix("--samples=") { samples = v.parse().unwrap(); }
        else if let Some(v) = argument.strip_prefix("--warmup=") { warmup = v.parse().unwrap(); }
        else if let Some(v) = argument.strip_prefix("--case=") { filter = v.to_string(); }
        else if let Some(v) = argument.strip_prefix("--output=") { output_path = Some(PathBuf::from(v)); }
        else if argument == "--snapshots" { snapshots = true; }
        else if argument == "--verify-observers" { verify_observers = true; }
        else { panic!("unknown argument {argument}"); }
    }
    assert!(samples > 0, "at least one timing sample is required");
    let mut cases = pipeline_cases();
    let no_aa = cases[0].parameters.clone();
    let aa = cases[5].parameters.clone();
    cases.extend(representative_cases("fresh-representative-20261020.ndjson.gz"));
    cases.extend(representative_cases("fresh-photographic-20261020.ndjson.gz"));
    for (width, height) in [(256, 192), (512, 384)] {
        cases.push(authored_case(width, height, false, &no_aa));
        cases.push(authored_case(width, height, true, &aa));
    }
    cases.retain(|case| case.id.contains(&filter));
    assert!(!cases.is_empty(), "no cases match {filter}");
    let mut results = Vec::new();
    for case in cases {
        let original = case.geometry.clone();
        let (initial, final_state) = case.run(None);
        let initial = initial.unwrap_or_else(|e| panic!("{}: {e}", case.id));
        let digest = output_digest(&initial, final_state);
        if let Some((expected, penalties, state)) = &case.expected {
            assert!(exact_geometry(&initial.geometry, expected), "native geometry {}", case.id);
            assert_eq!(&initial.penalty_counts, penalties, "native penalties {}", case.id);
            assert_eq!(final_state, *state, "native RNG {}", case.id);
        }
        for _ in 0..warmup { black_box(case.run(None).0.unwrap()); }
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let started = Instant::now();
            let (actual, state) = black_box(case.run(None));
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let actual = actual.unwrap();
            assert_eq!(output_digest(&actual, state), digest, "repeatability {}", case.id);
            times.push(elapsed);
        }
        let mut sorted = times.clone(); sorted.sort_by(f64::total_cmp);
        let median = if sorted.len() % 2 == 0 { (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0 }
            else { sorted[sorted.len() / 2] };
        let mut result = json!({"id": case.id, "width": case.width, "height": case.height,
            "nodes": case.geometry.nodes.len(), "regions": case.geometry.regions.len(),
            "anti_alias": case.parameters["Shared::is_anti_aliased"], "samples_ms": times,
            "median_ms": median, "min_ms": sorted[0], "max_ms": sorted[sorted.len() - 1],
            "input_sha256": input_digest(&case), "output_sha256": digest, "random_state": final_state,
            "native_expected_checked": case.expected.is_some()});
        if verify_observers { result["observer_checks"] = observer_checks(&case, &digest); }
        if snapshots { result["output"] = serde_json::to_value(&initial).unwrap(); }
        assert!(exact_geometry(&case.geometry, &original), "input mutation {}", case.id);
        eprintln!("{}: {:.3} ms ({} nodes, {} regions)", case.id, median, case.geometry.nodes.len(), case.geometry.regions.len());
        results.push(result);
    }
    let report = json!({"schema_version": 1, "timed_operation": "smooth_contour_geometry without an observer",
        "samples": samples, "warmup": warmup, "fixture_setup_excluded": true,
        "profile": if cfg!(debug_assertions) { "debug assertions enabled" } else { "release" },
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cases": results});
    let report = serde_json::to_string_pretty(&report).unwrap();
    if let Some(path) = output_path { std::fs::write(path, report).unwrap(); } else { println!("{report}"); }
}
