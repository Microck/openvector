#[path = "../../openvector/tests/common/mod.rs"]
mod common;
#[path = "../../openvector/tests/editor_support/mod.rs"]
mod editor_support;

use editor_support::compare_json;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "openvector-native-corpus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn invoke(root: &Path, args: &[&str], input: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openvector"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Read stdout concurrently with writing stdin: prepared graphs can exceed
    // pipe capacity, and an admission failure may close stdin before consuming it.
    let stdin = child.stdin.take().unwrap();
    let (output, written) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            let mut stdin = stdin;
            if let Some(input) = input { stdin.write_all(input) } else { Ok(()) }
        });
        let output = child.wait_with_output().unwrap();
        (output, writer.join().unwrap())
    });
    if let Err(error) = written {
        assert_eq!(output.status.code(), Some(1), "{args:?}: {error}");
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{args:?}");
    }
    output
}

fn success(output: &Output, context: &str) {
    assert_eq!(output.status.code(), Some(0), "{context}: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{context}: {}", String::from_utf8_lossy(&output.stderr));
}

fn rejected(output: &Output, diagnostic: &str, exact: bool) {
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stdout.is_empty(), "failure emitted partial output");
    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    assert!(stderr.ends_with('\n'), "unterminated diagnostic: {stderr:?}");
    if exact { assert_eq!(stderr, diagnostic); }
    else { assert!(stderr.contains(diagnostic), "expected {diagnostic:?}, got {stderr:?}"); }
}

fn request(root: &Path, command: &str, input: &Value, file: bool) -> Output {
    let encoded = serde_json::to_vec(input).unwrap();
    if file {
        let filename = "native document with spaces.json";
        let path = root.join(filename);
        std::fs::write(&path, &encoded).unwrap();
        let output = invoke(root, &[command, filename], None);
        assert_eq!(std::fs::read(path).unwrap(), encoded, "{command} changed its input file");
        output
    } else {
        invoke(root, &[command, "-"], Some(&encoded))
    }
}

fn json_output(output: &Output, context: &str) -> Value {
    success(output, context);
    assert!(output.stdout.ends_with(b"\n"), "{context}: JSON must end in a newline");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn latin1(value: &Value) -> Vec<u8> {
    value.as_str().unwrap().chars().map(|ch| u8::try_from(u32::from(ch)).unwrap()).collect()
}

fn assert_bytes(actual: &[u8], expected: &[u8], context: &str) {
    let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none() && actual.len() == expected.len(),
        "{context}: first differing byte {:?}; actual {} bytes, expected {} bytes",
        mismatch.or_else(|| (actual.len() != expected.len()).then_some(actual.len().min(expected.len()))),
        actual.len(), expected.len());
}

fn export_corpus(command: &str, filename: &str, count: usize) {
    let directory = Directory::new();
    let reference = common::read_json(filename);
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count, "{filename}");
    for (index, case) in cases.iter().enumerate() {
        let expected = latin1(&case["expected"]);
        let expected = match command {
            "export-pdf" => common::public_pdf_fixture(&expected),
            "export-emf" => common::public_emf_fixture(expected),
            _ => expected,
        };
        let mut input = case["input"].clone();
        if filename == "emf-expanded-cli-reference.json" {
            // This older dedicated capture omitted device metadata from input.
            // Recover only the native display measurements, not graph geometry,
            // from its ENHMETAHEADER (the same context used by the JS EMF suite).
            assert!(input.get("referenceDevice").is_none());
            assert_eq!(&expected[40..44], b" EMF");
            let pair = |offset| {
                [i32::from_le_bytes(expected[offset..offset + 4].try_into().unwrap()),
                 i32::from_le_bytes(expected[offset + 4..offset + 8].try_into().unwrap())]
            };
            input["referenceDevice"] = json!({
                "pixels": pair(72), "millimeters": pair(80), "micrometers": pair(100)
            });
        }
        let context = format!("{filename} row {index}");
        let output = request(&directory.0, command, &input, index % 2 != 0);
        success(&output, &context);
        assert_bytes(&output.stdout, &expected, &context);
        if command == "export-pdf" {
            let bytes = &output.stdout;
            assert!(bytes.windows(b"/Producer(OpenVector)".len()).any(|part| part == b"/Producer(OpenVector)"));
            let marker = b"startxref\r\n";
            let at = bytes.windows(marker.len()).rposition(|part| part == marker).unwrap() + marker.len();
            let end = at + bytes[at..].windows(2).position(|part| part == b"\r\n").unwrap();
            let offset: usize = std::str::from_utf8(&bytes[at..end]).unwrap().parse().unwrap();
            assert!(bytes[offset..].starts_with(b"xref\r\n"));
        }
    }
}

macro_rules! export_test {
    ($name:ident, $command:literal, $filename:literal, $count:literal) => {
        #[test]
        fn $name() { export_corpus($command, $filename, $count); }
    };
}

export_test!(svg_all_native_cli_rows, "export-svg", "svg-cli-reference.json", 6);
export_test!(eps_all_native_cli_rows_and_captured_dates, "export-eps", "eps-cli-reference.json", 12);
export_test!(pdf_all_native_cli_binary_rows, "export-pdf", "pdf-cli-reference.json", 6);
export_test!(ai_all_native_cli_rows, "export-ai", "ai-cli-reference.json", 6);
export_test!(dxf_all_native_cli_modes_palettes_and_hidden_extremes, "export-dxf", "dxf-cli-reference.json", 38);
export_test!(emf_all_native_cli_devices_and_binary_rows, "export-emf", "emf-cli-reference.json", 24);
export_test!(emf_all_expanded_native_cli_binary_rows, "export-emf", "emf-expanded-cli-reference.json", 6);

fn prepared_corpus(command: &str, filename: &str, count: usize) {
    let directory = Directory::new();
    let reference = common::read_json(filename);
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    for (index, case) in cases.iter().enumerate() {
        for file in [false, true] {
            let context = format!("{filename} row {index}, file={file}");
            let output = request(&directory.0, command, &case["input"], file);
            compare_json(&json_output(&output, &context), &case["expected"], &context);
        }
    }
}

#[test]
fn smoothing_all_native_cli_prepared_inputs_and_float_bits() {
    prepared_corpus("smooth-contours", "smoothing-cli-reference.json", 2);
}

#[test]
fn fitting_all_native_cli_prepared_inputs_and_float_bits() {
    prepared_corpus("fit-contours", "fitting-cli-reference.json", 3);
}

#[test]
fn native_export_defaults_keep_exact_captured_bytes() {
    let directory = Directory::new();
    for (format, index, keys) in [
        ("emf", 0, vec!["layout", "thinStroke", "outputWidth", "outputHeight", "hiddenRegions"]),
        ("dxf", 20, vec!["curveMode"]),
    ] {
        let reference = common::read_json(&format!("{format}-cli-reference.json"));
        if format == "dxf" { assert_eq!(reference["defaultCaseIndex"], index); }
        let row = &reference["cases"][index as usize];
        let mut input = row["input"].clone();
        for key in keys { input.as_object_mut().unwrap().remove(key); }
        let command = format!("export-{format}");
        let output = request(&directory.0, &command, &input, false);
        success(&output, &command);
        let expected = latin1(&row["expected"]);
        let expected = if format == "emf" { common::public_emf_fixture(expected) } else { expected };
        assert_bytes(&output.stdout, &expected, &command);
    }
}

#[test]
fn export_admissions_preserve_retained_cli_diagnostics_and_empty_stdout() {
    let directory = Directory::new();
    for format in ["svg", "eps", "pdf", "ai", "dxf", "emf"] {
        let reference = common::read_json(&format!("{format}-cli-reference.json"));
        let valid = &reference["cases"][0]["input"];
        let mut invalid = vec![json!({})];
        match format {
            "eps" => {
                let mut compact = valid.clone(); compact["compact"] = json!(1); invalid.push(compact);
                let mut date = valid.clone(); date["creationDate"] = json!("injected\ncommand"); invalid.push(date);
            }
            "dxf" => {
                let mut missing = valid.clone(); missing.as_object_mut().unwrap().remove("palette"); invalid.push(missing);
                let mut mode = valid.clone(); mode["curveMode"] = json!("unknown"); invalid.push(mode);
                invalid.push(common::read_json("dxf-overlong-reference.json")["cases"][0]["input"].clone());
            }
            "emf" => {
                let mut missing = valid.clone(); missing.as_object_mut().unwrap().remove("referenceDevice"); invalid.push(missing);
                let mut device = valid.clone(); device["referenceDevice"] = json!({"pixels": [0, 1]}); invalid.push(device);
            }
            _ => { let mut layout = valid.clone(); layout["layout"] = json!("unknown"); invalid.push(layout); }
        }
        for input in invalid {
            rejected(&request(&directory.0, &format!("export-{format}"), &input, false), &format.to_uppercase(), false);
        }
    }
}

#[test]
fn smoothing_and_fitting_missing_settings_and_malformed_graph_admissions() {
    let directory = Directory::new();
    for (command, filename, diagnostic) in [
        ("smooth-contours", "smoothing-cli-reference.json", "Smoothing needs a settings object with explicit controls.\n"),
        ("fit-contours", "fitting-cli-reference.json", "Fitting needs a settings object with explicit controls.\n"),
    ] {
        let mut input = common::read_json(filename)["cases"][0]["input"].clone();
        input.as_object_mut().unwrap().remove("settings");
        rejected(&request(&directory.0, command, &input, false), diagnostic, true);
    }
    let mut input = common::read_json("fitting-cli-reference.json")["cases"][0]["input"].clone();
    input["geometry"]["regions"][0]["contour"][0] = json!(-1);
    rejected(&request(&directory.0, "fit-contours", &input, false), "Fitting needs", false);
}

#[test]
fn every_retained_numeric_command_is_documented() {
    let directory = Directory::new();
    for command in ["settings", "fill", "assign-palette", "generate-palette", "refine-palette", "analyze-image",
        "segment", "trace-contours", "smooth-contours", "fit-contours", "export-svg", "export-eps",
        "export-ai", "export-pdf", "export-dxf", "export-emf"] {
        let output = invoke(&directory.0, &[command, "--help"], None);
        success(&output, command);
        assert!(std::str::from_utf8(&output.stdout).unwrap().contains(command), "{command}");
    }
}

#[test]
fn settings_basic_category_quality_and_native_f32_parameters() {
    let directory = Directory::new();
    for (category_index, category) in ["photograph", "artwork-with-blending", "artwork-without-blending"].iter().enumerate() {
        for (quality_index, quality) in ["high", "medium", "low"].iter().enumerate() {
            let input = json!({"mode": "basic", "stage": "segmentation", "category": category, "quality": quality});
            let output = request(&directory.0, "settings", &input, quality_index % 2 != 0);
            let parameters = json_output(&output, "basic settings");
            assert_eq!(parameters.as_object().unwrap().len(), 100);
            assert_eq!(parameters["Shared::image_type_code"], json!(8 - category_index * 3 - quality_index));
            assert_eq!(parameters["SubPixelSegmenter::color_cluster_margin"].as_f64().unwrap().to_bits(), (0.1_f32 as f64).to_bits());
        }
    }
}

#[test]
fn settings_native_advanced_extremes_and_midpoints() {
    let directory = Directory::new();
    let reference = common::read_json("settings-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 648);
    // The engine owns the exhaustive arithmetic matrix. Exercise transport for
    // every rejection/cluster choice at low, middle and high complexity, and
    // every anti-alias/corner choice at the corresponding contour controls.
    let mut checked = 0;
    for row in cases {
        if ![1, 6, 12].contains(&row["complexity"].as_i64().unwrap()) { continue; }
        if row["stage"] == "segmentation" {
            let input = json!({"mode": "advanced", "stage": "segmentation", "complexity": row["complexity"],
                "minimumPixels": row["minimumPixels"], "clusterColors": row["cluster"],
                "artifactRejection": (["none", "moderate", "aggressive"][row["rejection"].as_u64().unwrap() as usize])});
            let p = json_output(&request(&directory.0, "settings", &input, checked % 2 != 0), "advanced segmentation settings");
            let actual = json!({"lambda": [p["Segmenter::lambda_pre"], p["Segmenter::lambda_initial"], p["Segmenter::lambda_final"]],
                "minimumPixels": p["Segmenter::min_num_pixels"], "antiAlias": p["Shared::is_anti_aliased"],
                "shadowing": i32::from(p["shadowing"].as_bool().unwrap()), "clusterColors": i32::from(p["clusterColors"].as_bool().unwrap())});
            compare_json(&actual, &row["expected"], "advanced segmentation settings");
            checked += 1;
        } else if row["smoothness"] == row["complexity"] {
            assert_eq!(row["stage"], "contours");
            for stage in ["smoothing", "fitting"] {
                let input = json!({"mode": "advanced", "stage": stage, "smoothness": row["smoothness"],
                    "curveComplexity": row["complexity"], "antiAlias": row["antiAlias"], "detectCorners": row["corners"]});
                let p = json_output(&request(&directory.0, "settings", &input, checked % 2 != 0), stage);
                let actual = json!({"measurementTypes": p["ContourSmoother::measurement_types"],
                    "priorTypes": p["ContourSmoother::prior_types"], "priorStrengths": p["ContourSmoother::prior_strengths"],
                    "lengthPenalties": p["ContourSmoother::length_penalty_weights"], "punctureCorners": p["ContourSmoother::do_puncture_corners"],
                    "enabled": p["ContourSmoother::is_enabled"], "bezierThresholds": [p["BezierFitter::stat_thresh_initial"], p["BezierFitter::stat_thresh_final"]],
                    "antiAlias": p["Shared::is_anti_aliased"]});
                compare_json(&actual, &row["expected"], stage);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 42);
}

#[test]
fn fresh_palette_generation_and_refinement_preserve_captured_numbers() {
    let directory = Directory::new();
    for (command, filename, anti_alias) in [
        ("generate-palette", "generation-reference.json", false),
        ("refine-palette", "refinement-reference.json", false),
        ("generate-palette", "weighted-generation-reference.json", true),
        ("refine-palette", "weighted-refinement-reference.json", true),
    ] {
        let reference = common::read_json(filename);
        let row = &reference["cases"][0];
        let mut input = row.clone();
        input.as_object_mut().unwrap().remove("expected");
        input["antiAlias"] = json!(anti_alias);
        let output = request(&directory.0, command, &input, anti_alias);
        let expected = json!({"palette": row["expected"]["palette"], "indices": row["expected"]["indices"],
            "weights": row["expected"]["weights"], "passes": row["expected"]["passes"]});
        compare_json(&json_output(&output, filename), &expected, filename);
    }
}

#[test]
fn segmentation_from_raw_pixels_matches_all_retained_basic_cli_profiles() {
    let directory = Directory::new();
    for (filename, profiles, category) in [
        ("unweighted-segmentation-reference.json", [9, 8, 7], "photograph"),
        ("unweighted-diagonal-segmentation-reference.json", [3, 2, 1], "artwork-without-blending"),
        ("anti-alias-segmentation-reference.json", [6, 5, 4], "artwork-with-blending"),
    ] {
        let reference = common::read_json(filename);
        for (profile, quality) in profiles.into_iter().zip(["high", "medium", "low"]) {
            let row = reference["cases"].as_array().unwrap().iter().rev()
                .find(|row| row["profile"] == profile && row["width"] == 17).unwrap();
            let input = json!({"width": row["width"], "height": row["height"], "bgra": row["bgra"],
                "settings": {"mode": "basic", "category": category, "quality": quality}});
            let output = request(&directory.0, "segment", &input, false);
            compare_json(&json_output(&output, filename), &row["expected"], &format!("{filename} profile {profile}"));
        }
    }
}

#[test]
fn segmentation_from_raw_pixels_preserves_supplied_palettes() {
    let directory = Directory::new();
    let reference = common::read_json("supplied-palette-segmentation-reference.json");
    for (profile, category) in [(3, "artwork-without-blending"), (6, "artwork-with-blending")] {
        let row = reference["cases"].as_array().unwrap().iter().find(|row|
            row["profile"] == profile && row["width"] == 17 && row["palette"].as_array().unwrap().len() > 4).unwrap();
        let input = json!({"width": row["width"], "height": row["height"], "bgra": row["bgra"], "palette": row["palette"],
            "settings": {"mode": "basic", "category": category, "quality": "high"}});
        let output = request(&directory.0, "segment", &input, true);
        compare_json(&json_output(&output, "supplied palette"), &row["expected"], &format!("supplied palette {profile}"));
    }
}

#[test]
fn contour_extraction_starts_from_raw_pixels_not_captured_segmentation() {
    let directory = Directory::new();
    let contours = common::read_json("contour-extraction-reference.json");
    for (mode, profile, category) in [
        ("unweighted-diagonal-segmentation", 3, "artwork-without-blending"),
        ("anti-alias-segmentation", 6, "artwork-with-blending"),
        ("supplied-palette-segmentation", 3, "artwork-without-blending"),
        ("supplied-palette-segmentation", 6, "artwork-with-blending"),
    ] {
        let reference = common::read_json(&format!("{mode}-reference.json"));
        let (index, row) = reference["cases"].as_array().unwrap().iter().enumerate()
            .find(|(_, row)| row["profile"] == profile && row["width"] == 17).unwrap();
        let expected = &contours["cases"].as_array().unwrap().iter()
            .find(|row| row["sourceMode"] == mode && row["sourceCase"] == json!(index)).unwrap()["expected"];
        let mut input = json!({"width": row["width"], "height": row["height"], "bgra": row["bgra"],
            "settings": {"mode": "basic", "category": category, "quality": "high"}});
        if mode == "supplied-palette-segmentation" { input["palette"] = row["palette"].clone(); }
        let output = request(&directory.0, "trace-contours", &input, false);
        compare_json(&json_output(&output, mode), expected, &format!("{mode} source row {index}"));
    }
}

#[test]
fn retained_numeric_errors_have_no_partial_json() {
    let directory = Directory::new();
    for (command, input, diagnostic, exact) in [
        ("settings", json!({"mode": "basic", "stage": "segmentation", "category": "invalid", "quality": "high"}), "category must be one of", false),
        ("segment", json!({"width": 1, "height": 1, "bgra": [0, 0, 0, 255], "settings": {
            "mode": "advanced", "complexity": 6, "minimumPixels": 0, "artifactRejection": "moderate", "clusterColors": "yes"}}),
            "clusterColors must be true or false.\n", true),
        ("trace-contours", json!({"width": 1, "height": 1, "bgra": [0, 0, 0, 255], "settings": {
            "mode": "basic", "category": "photograph", "quality": "high"}}),
            "Contour extraction needs int32 dimensions of at least 2 by 2.\n", true),
        ("generate-palette", common::read_json("generation-reference.json")["cases"][0].clone(), "require antiAlias: true or false", false),
    ] {
        rejected(&request(&directory.0, command, &input, false), diagnostic, exact);
    }
    let map = br#"{"width":2,"height":2,"segments":[0,1,1,0]}"#;
    let output = invoke(&directory.0, &["fill", "-", "0", "7"], Some(map));
    compare_json(&json_output(&output, "fill"), &json!({"width": 2, "height": 2, "segments": [7, 1, 1, 0]}), "four-connected fill");
    rejected(&invoke(&directory.0, &["fill", "-", "", "7"], Some(map)), "seed must be an integer", false);
    let assignment = json!({"bgra": [0, 0, 0, 255, 255, 255, 255, 255], "palette": [0, 0, 0, 1, 1, 1, 1, 1]});
    let output = request(&directory.0, "assign-palette", &assignment, false);
    assert_eq!(json_output(&output, "assign-palette"), json!([0, 1]));
}

#[test]
fn advanced_clustering_from_raw_pixels_matches_retained_cli_rows() {
    let directory = Directory::new();
    let reference = common::read_json("clustered-anti-alias-segmentation-reference.json");
    let settings = common::read_json("settings-reference.json");
    for complexity in [1, 6, 12] {
        let parameters = settings["cases"].as_array().unwrap().iter().find(|row|
            row["stage"] == "segmentation" && row["complexity"] == complexity &&
            row["rejection"] == 1 && row["cluster"] == true).unwrap();
        let row = reference["cases"].as_array().unwrap().iter().find(|row|
            row["profile"].is_null() && row["width"] == 17 &&
            row["parameters"]["Segmenter::min_num_pixels"] == 2 &&
            row["parameters"]["Segmenter::use_over_seg_diag_extr"] == 0 &&
            row["parameters"]["Segmenter::lambda_final"] == parameters["expected"]["lambda"][2]).unwrap();
        let input = json!({"width": row["width"], "height": row["height"], "bgra": row["bgra"],
            "settings": {"mode": "advanced", "complexity": complexity, "minimumPixels": 2,
                "artifactRejection": "moderate", "clusterColors": true}});
        let output = request(&directory.0, "segment", &input, false);
        compare_json(&json_output(&output, "advanced clustering"), &row["expected"], &format!("clustering {complexity}"));
    }
}

#[test]
fn advanced_segmentation_requires_native_shadow_context() {
    let directory = Directory::new();
    let rows = common::read_ndjson("document-segmentation-reference.ndjson.gz");
    let row = rows.iter().find(|row| row["id"] == "31x27-noisy").unwrap();
    let (index, step) = row["steps"].as_array().unwrap().iter().enumerate()
        .find(|(_, step)| step["id"] == "advanced-0-0").unwrap();
    let native = &row["states"][index]["state"];
    // This command's input is the captured pixel raster, not a document replay:
    // segmentation itself must be recomputed by the child process.
    let mut input = json!({"width": native["workingDimensions"][0], "height": native["workingDimensions"][1],
        "bgra": common::b64(native["workingBgra"].as_str().unwrap()), "shadowControl": native["shadowControl"],
        "settings": {"mode": "advanced", "complexity": step["complexity"], "minimumPixels": step["minimumPixels"],
            "artifactRejection": "none", "clusterColors": step["clusterColors"]}});
    let output = request(&directory.0, "segment", &input, false);
    let segmentation = &native["segmentation"];
    let hex = |value: &Value| {
        let bytes = editor_support::hex(value.as_str().unwrap());
        assert_eq!(bytes.len() % 4, 0);
        bytes
    };
    let indices = hex(&segmentation["indexBits"]).chunks_exact(4)
        .map(|bytes| i32::from_le_bytes(bytes.try_into().unwrap())).collect::<Vec<_>>();
    let palette = hex(&segmentation["paletteBits"]).chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()) as f64).collect::<Vec<_>>();
    let expected = json!({"indices": indices, "palette": palette, "counts": segmentation["counts"], "colorIndices": segmentation["colorIndices"]});
    compare_json(&json_output(&output, "shadow segmentation"), &expected, "native shadow bits");
    input.as_object_mut().unwrap().remove("shadowControl");
    rejected(&request(&directory.0, "segment", &input, false),
        "Enabled shadowing requires an explicit int32 shadowControl.\n", true);
}

fn fresh_application_exports(id: &str) {
    let directory = Directory::new();
    let rows = common::read_ndjson("fresh-conversion-20261020-exports.ndjson.gz");
    for workflow in ["svg-batch", "bitmap"] {
        let fixture_id = format!("fresh-conversion-20261020-{id}-{workflow}");
        let row = rows.iter().find(|row| row["id"] == fixture_id).expect("captured native workflow");
        let source = row["input"]["name"].as_str().unwrap();
        let pixels = common::b64(row["input"]["bytes"].as_str().unwrap());
        std::fs::write(directory.0.join(source), &pixels).unwrap();
        if workflow == "svg-batch" {
            // Native batch defaults flatten alpha; automatic single-image
            // processing can preserve it. Replay the captured workflow itself.
            let output = invoke(&directory.0, &["batch", source, "--output-dir", ".", "--format", "svg"], None);
            let summary = json_output(&output, id);
            assert_eq!(summary["converted"], 1);
            assert_eq!(summary["failed"], 0);
            for (filename, expected) in row["exports"][0].as_object().unwrap() {
                assert_eq!(std::fs::read(directory.0.join(filename)).unwrap(),
                    common::application_svg_fixture(&common::b64(expected["bytes"].as_str().unwrap())), "{id}/{filename}");
            }
            continue;
        }
        for (filename, expected) in row["exports"][0].as_object().unwrap() {
            let expected = common::b64(expected["bytes"].as_str().unwrap());
            let output = if filename.ends_with(".png") {
                let output = invoke(&directory.0, &["vectorize", source, "--output", filename], None);
                success(&output, id);
                assert!(output.stdout.is_empty());
                assert_eq!(std::fs::read(directory.0.join(filename)).unwrap(), expected, "{id}/{filename}");
                continue;
            } else {
                invoke(&directory.0, &["vectorize", source, "--format", "bmp", "--flatten"], None)
            };
            success(&output, id);
            assert_eq!(output.stdout, expected, "{id}/{filename}");
        }
    }
}

macro_rules! fresh_application_case {
    ($name:ident, $id:literal) => { #[test] fn $name() { fresh_application_exports($id); } };
}
fresh_application_case!(fresh_logo_5x7, "00-logo-5x7");
fresh_application_case!(fresh_diagonal_7x5, "01-diagonal-7x5");
fresh_application_case!(fresh_ring_11x13, "02-ring-11x13");
fresh_application_case!(fresh_two_holes_17x19, "03-two-holes-17x19");
fresh_application_case!(fresh_curve_23x17, "04-curve-23x17");
fresh_application_case!(fresh_logo_29x23, "05-logo-29x23");
fresh_application_case!(fresh_checker_31x29, "06-checker-31x29");
fresh_application_case!(fresh_islands_37x31, "07-islands-37x31");
fresh_application_case!(fresh_transparent_19x23, "08-transparent-19x23");
fresh_application_case!(fresh_ring_41x37, "10-ring-41x37");
fresh_application_case!(fresh_curve_43x31, "11-curve-43x31");
fresh_application_case!(fresh_diagonal_13x3, "12-diagonal-13x3");
fresh_application_case!(fresh_logo_3x17, "13-logo-3x17");
fresh_application_case!(fresh_two_holes_47x43, "14-two-holes-47x43");
fresh_application_case!(fresh_transparent_31x37, "15-transparent-31x37");

#[test]
fn invalid_vector_options_preserve_existing_output_before_processing() {
    let directory = Directory::new();
    let output = directory.0.join("protected.png");
    std::fs::write(&output, b"existing output").unwrap();
    for args in [
        vec!["vectorize", "missing.png", "--output", "protected.png", "--force", "--layout", "stacked"],
        vec!["vectorize", "missing.png", "--output", "protected.png", "--force", "--thin-stroke"],
    ] {
        rejected(&invoke(&directory.0, &args, None), "vector output", false);
        assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
    }
}

fn fresh_single_image_svg(id: &str) {
    static ROWS: std::sync::LazyLock<Vec<Value>> = std::sync::LazyLock::new(||
        common::read_ndjson("fresh-representative-20261020.ndjson.gz"));
    let fixture_id = format!("fresh-representative-20261020-{id}");
    let reference = ROWS.iter().find(|row| row["id"] == fixture_id).unwrap();
    assert_eq!(reference["svgBytesRepeatIdentical"], true);
    assert_eq!(reference["singleImageSvg"][0], reference["singleImageSvg"][1]);
    let source = reference["input"]["name"].as_str().unwrap();
    let pixels = common::b64(reference["input"]["bytes"].as_str().unwrap());
    let expected = common::application_svg_fixture(&common::b64(reference["singleImageSvg"][0]["bytes"].as_str().unwrap()));
    let directory = Directory::new();
    let output = invoke(&directory.0, &["vectorize", "-", "--filename", source], Some(&pixels));
    success(&output, id);
    assert_bytes(&output.stdout, &expected, id);
}

macro_rules! fresh_single_image_case {
    ($name:ident, $id:literal) => { #[test] fn $name() { fresh_single_image_svg($id); } };
}
fresh_single_image_case!(single_alpha_19x23, "existing-08-transparent-19x23");
fresh_single_image_case!(single_alpha_31x37, "existing-15-transparent-31x37");
fresh_single_image_case!(single_antialiased_logo, "logo-aa-256x192");
fresh_single_image_case!(single_antialiased_text, "text-aa-256x160");
fresh_single_image_case!(single_thin_holes_and_lines, "thin-holes-lines-192x128");
fresh_single_image_case!(single_jpeg_logo, "logo-jpeg-256x192");
fresh_single_image_case!(single_palette_rich_artwork, "palette-rich-192x128");
fresh_single_image_case!(single_large_alpha_rings, "alpha-rings-192x128");
