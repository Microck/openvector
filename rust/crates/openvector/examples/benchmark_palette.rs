//! Reproducible palette generation/analysis benchmark. Fixture decoding,
//! reference-output validation, hashing and observer snapshots are outside timers.
//! Run the identical example against the baseline and candidate revisions.
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::read::GzDecoder;
use openvector::palette::{AnalysisControls, ColorAnalysis, CoreSession, GeneratedPalette, PreparedPaletteAnalysis};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::File, hint::black_box, io::{BufRead, BufReader}, path::PathBuf, time::Instant};

struct Case { id: String, bgra: Vec<u8>, controls: AnalysisControls }

fn parameter(metadata: &Value, state: &Value, key: &str) -> f64 {
    let hex = state["parameterBits"][key].as_str().expect(key);
    let bytes: Vec<u8> = (0..hex.len()).step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    match metadata["parameters"][key]["kind"].as_str().unwrap() {
        "int32" => i32::from_le_bytes(bytes.try_into().unwrap()) as f64,
        "float32" => f32::from_le_bytes(bytes.try_into().unwrap()) as f64,
        "float64" => f64::from_le_bytes(bytes.try_into().unwrap()),
        kind => panic!("unsupported parameter kind: {kind}"),
    }
}

fn captured_cases() -> Vec<Case> {
    let fixtures = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let mut cases = Vec::new();
    for file in ["fresh-representative-20261020.ndjson.gz", "fresh-photographic-20261020.ndjson.gz"] {
        let mut lines = BufReader::new(GzDecoder::new(File::open(fixtures.join(file)).unwrap())).lines();
        let metadata: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        for line in lines {
            let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let state = &row["expectedAutomatic"]["state"];
            let width = state["workingDimensions"][0].as_u64().unwrap() as usize;
            let height = state["workingDimensions"][1].as_u64().unwrap() as usize;
            let bgra = STANDARD.decode(state["workingBgra"].as_str().unwrap()).unwrap();
            assert_eq!(bgra.len(), width * height * 4);
            cases.push(Case {
                id: format!("captured/{}", row["id"].as_str().unwrap()), bgra,
                controls: AnalysisControls {
                    width, height,
                    bins: if parameter(&metadata, state, "PaletteFinder::use_16_hist_bins") != 0.0 { 16 } else { 8 },
                    connected: parameter(&metadata, state, "PaletteFinder::use_26_connected_peaks") != 0.0,
                    anti_alias: parameter(&metadata, state, "Shared::is_anti_aliased") != 0.0,
                    cost_per_color: parameter(&metadata, state, "PaletteFinder::cost_per_color"),
                    edge_scale: parameter(&metadata, state, "EdgeImage::edge_weight_scale"),
                    edge_floor: parameter(&metadata, state, "EdgeImage::edge_weight_floor"),
                },
            });
        }
    }
    assert_eq!(cases.len(), 11);
    cases
}

// A graphic poster with flat panels, diagonal ribbons, circular marks and type-like
// strokes. Four subpixel samples supply reproducible antialiased edge colors.
fn poster(width: usize, height: usize, anti_alias: bool) -> Vec<u8> {
    let colors = [[240, 243, 248, 255], [55, 48, 31, 255], [51, 116, 228, 255],
        [140, 180, 44, 255], [224, 161, 62, 255], [201, 75, 172, 255],
        [70, 205, 213, 255], [18, 24, 30, 160]];
    let sample = |x: usize, y: usize| {
        let w = width * 4; let h = height * 4;
        let mut color = 0;
        if y < h / 5 { color = 1; }
        if x > w / 8 && x < w * 7 / 8 && y > h / 3 && y < h * 4 / 5 {
            color = 2 + (x / (w / 6)) % 4;
        }
        let (dx, dy) = (x as i64 - w as i64 * 3 / 4, y as i64 - h as i64 / 4);
        if dx * dx + dy * dy < (h as i64 / 6).pow(2) { color = 6; }
        if y > h * 3 / 5 && y < h * 3 / 5 + h / 14 && (x + y) % (w / 7) < w / 12 { color = 7; }
        if x > w / 12 && x < w / 2 && y > h / 18 && y < h / 7 {
            let (gx, gy) = (x / 12 % 5, y / 12 % 7);
            if gx == 0 || gy == 0 || gy == 3 || (gx == 4 && gy < 4) { color = 0; }
        }
        colors[color]
    };
    let offsets: &[(usize, usize)] = if anti_alias { &[(1, 1), (3, 1), (1, 3), (3, 3)] } else { &[(2, 2)] };
    let mut bgra = Vec::with_capacity(width * height * 4);
    for y in 0..height { for x in 0..width {
        let mut sum = [0usize; 4];
        for &(ox, oy) in offsets { let value = sample(x * 4 + ox, y * 4 + oy); for c in 0..4 { sum[c] += value[c] as usize; } }
        bgra.extend(sum.map(|v| (v / offsets.len()) as u8));
    }}
    bgra
}

fn cases() -> Vec<Case> {
    let mut cases = captured_cases();
    for anti_alias in [false, true] {
        let (width, height) = (512, 384);
        cases.push(Case { id: format!("authored/poster-512x384-aa{}", u8::from(anti_alias)),
            bgra: poster(width, height, anti_alias), controls: AnalysisControls {
                width, height, bins: 16, connected: true, anti_alias,
                cost_per_color: 0.15, edge_scale: 50.0, edge_floor: 0.01,
            } });
    }
    // Deliberately high-entropy input detects cache-overhead regressions. This is
    // deterministic noise, not a claimed photographic sample.
    let (width, height) = (256, 192); let mut state = 0x1234_abcd_u32;
    let mut bgra = Vec::with_capacity(width * height * 4);
    for _ in 0..width * height {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        bgra.extend_from_slice(&[state as u8, (state >> 8) as u8, (state >> 16) as u8, 255]);
    }
    cases.push(Case { id: "authored/noise-256x192-aa0".into(), bgra, controls: AnalysisControls {
        width, height, bins: 16, connected: true, anti_alias: false,
        cost_per_color: 0.15, edge_scale: 50.0, edge_floor: 0.01,
    } });
    cases
}

fn generated_snapshot(result: &GeneratedPalette) -> Value {
    json!({"palette_bits": result.palette.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "indices": result.indices, "weights_bits": result.weights.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "passes": result.passes})
}

fn analysis_snapshot(result: &ColorAnalysis, random_state: u32) -> Value {
    json!({"candidates": result.candidates.iter().map(|v| json!({"count": v.color_count,
        "generated": generated_snapshot(&v.generated), "score_bits": [v.score.total.to_bits(), v.score.error.to_bits(), v.score.colors.to_bits()],
        "random_state": v.random_state})).collect::<Vec<_>>(),
        "peak_weights_bits": result.peak_weights.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "features_bits": result.features.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "classification": {"label": result.classification.label, "mode": result.classification.mode, "count": result.classification.color_count},
        "random_state": random_state})
}

fn fingerprint(value: &Value) -> String { format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap())) }

enum Output { Generated(GeneratedPalette, u32), Analyzed(ColorAnalysis, u32) }
impl Output {
    fn snapshot(&self) -> Value { match self {
        Self::Generated(g, rng) => json!({"generated": generated_snapshot(g), "random_state": rng}),
        Self::Analyzed(a, rng) => analysis_snapshot(a, *rng),
    }}
}
fn run(case: &Case, operation: &str, count: usize) -> Output {
    let session = CoreSession::default();
    if operation == "generate" {
        let generated = if case.controls.anti_alias { session.generate_weighted(&case.bgra, count, case.controls) }
            else { session.generate_unweighted(&case.bgra, count, case.controls.bins, case.controls.connected) }.unwrap();
        Output::Generated(generated, session.random_state())
    } else {
        Output::Analyzed(session.analyze_color_counts(&case.bgra, case.controls).unwrap(), session.random_state())
    }
}

// This is independent of timed calls. Include all observable iteration states and
// the prefix/state returned when cancellation interrupts a middle callback.
fn observer_snapshot(case: &Case, count: usize) -> Value {
    let prepared = PreparedPaletteAnalysis::new(&case.bgra, case.controls).unwrap();
    let session = CoreSession::default(); let mut events = Vec::new();
    let result = prepared.evaluate(&session, &case.bgra, count, Some(&mut |iteration, colors, indices| {
        events.push(json!({"iteration": iteration, "palette_bits": colors.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "indices": indices})); Ok(())
    })).unwrap();
    let cancel_at = events.len() / 2;
    let cancelled = CoreSession::default(); let mut prefix = Vec::new();
    let error = prepared.evaluate(&cancelled, &case.bgra, count, Some(&mut |iteration, colors, indices| {
        prefix.push(json!({"iteration": iteration, "palette_bits": colors.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "indices": indices}));
        if iteration == cancel_at { Err("benchmark cancellation".into()) } else { Ok(()) }
    })).unwrap_err();
    assert_eq!(prefix, events[..=cancel_at]);
    json!({"events": events, "generated": generated_snapshot(&result.generated),
        "score_bits": [result.score.total.to_bits(), result.score.error.to_bits(), result.score.colors.to_bits()],
        "random_state": session.random_state(), "cancel_at": cancel_at, "cancel_prefix": prefix,
        "cancel_error": error, "cancel_random_state": cancelled.random_state()})
}

fn main() {
    let mut samples = 5; let mut warmup = 1; let mut filter = String::new(); let mut operation = "generate".to_string();
    let mut count = 12; let mut observers = false; let mut snapshots = None;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--samples=") { samples = v.parse::<usize>().unwrap(); }
        else if let Some(v) = arg.strip_prefix("--warmup=") { warmup = v.parse::<usize>().unwrap(); }
        else if let Some(v) = arg.strip_prefix("--case=") { filter = v.into(); }
        else if let Some(v) = arg.strip_prefix("--operation=") { operation = v.into(); }
        else if let Some(v) = arg.strip_prefix("--count=") { count = v.parse::<usize>().unwrap(); }
        else if let Some(v) = arg.strip_prefix("--snapshots=") { snapshots = Some(PathBuf::from(v)); }
        else if arg == "--observers" { observers = true; }
        else if arg == "--help" { println!("benchmark_palette [--samples=5] [--warmup=1] [--case=substring] [--operation=generate|analyze] [--count=12] [--observers] [--snapshots=path.json]\nTiming includes palette preparation and the named stage; decode, hashing, result destruction and observer checks are outside the timer. Each call starts with RNG seed 1. Captured rasters are seeded cross-revision checks, not replay of the recorder's final random state."); return; }
        else { panic!("unknown argument: {arg}"); }
    }
    assert!(samples > 0 && count > 0 && ["generate", "analyze"].contains(&operation.as_str()));
    let mut results = Vec::new(); let mut full = Vec::new();
    for case in cases().into_iter().filter(|c| c.id.contains(&filter)) {
        let expected = run(&case, &operation, count).snapshot(); let expected_hash = fingerprint(&expected);
        let observer = observers.then(|| observer_snapshot(&case, count));
        for _ in 0..warmup { black_box(run(&case, &operation, count)); }
        let mut elapsed = Vec::new();
        for _ in 0..samples {
            let start = Instant::now(); let result = black_box(run(black_box(&case), &operation, count));
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(fingerprint(&result.snapshot()), expected_hash, "nondeterministic case {}", case.id);
        }
        let mut sorted = elapsed.clone(); sorted.sort_by(f64::total_cmp);
        let median = (sorted[(samples - 1) / 2] + sorted[samples / 2]) / 2.0;
        let mut input = Sha256::new(); input.update(&case.bgra);
        let controls = json!({"width": case.controls.width, "height": case.controls.height, "bins": case.controls.bins,
            "connected": case.controls.connected, "anti_alias": case.controls.anti_alias,
            "cost_bits": case.controls.cost_per_color.to_bits(), "edge_scale_bits": case.controls.edge_scale.to_bits(), "edge_floor_bits": case.controls.edge_floor.to_bits()});
        input.update(serde_json::to_vec(&controls).unwrap());
        results.push(json!({"id": case.id, "controls": controls, "input_sha256": format!("{:x}", input.finalize()),
            "output_sha256": expected_hash, "observer_sha256": observer.as_ref().map(fingerprint), "samples_ms": elapsed, "median_ms": median}));
        if snapshots.is_some() { full.push(json!({"id": case.id, "output": expected, "observer": observer})); }
    }
    assert!(!results.is_empty(), "no cases matched");
    if let Some(path) = snapshots { std::fs::write(path, serde_json::to_vec(&full).unwrap()).unwrap(); }
    println!("{}", serde_json::to_string_pretty(&json!({"schema": 1, "operation": operation, "count": count, "samples": samples,
        "warmup_per_case": warmup, "initial_correctness_call_per_case": 1, "rng_seed": 1, "cases": results})).unwrap());
}
