mod common;
mod fitted_geometry;
use openvector::{ai::export_ai, dxf::{export_dxf, DxfCurveMode}, emf::{export_emf, EmfDevice}, eps::{export_eps, EpsOptions}, pdf::export_pdf, svg::export_svg};
use openvector::contour_fitting::FittedGeometry;
use openvector::export_options::Layout;
use openvector::vector_output::VectorOutputInput;
use serde_json::Value;
use std::io::{BufRead, BufReader};

fn hidden(shape: &Value, frame: &Value) -> Vec<i64> {
    if let Some(ids) = frame.get("hiddenRegions") { return ids.as_array().unwrap().iter().map(|id| id.as_i64().unwrap()).collect(); }
    (0..shape["geometry"]["regions"].as_array().unwrap().len()).filter(|&i| {
        i < frame["hiddenSize"].as_u64().unwrap_or(0) as usize && frame["hiddenWords"][i >> 5].as_u64().unwrap() & (1u64 << (i & 31)) != 0
    }).map(|i| i as i64).collect()
}
fn input<'a>(shape: &Value, frame: &Value, geometry: &'a FittedGeometry, hidden: &'a [i64]) -> VectorOutputInput<'a> {
    let mut input = VectorOutputInput::new(geometry, shape["width"].as_i64().unwrap(), shape["height"].as_i64().unwrap());
    input.layout = match frame["shapeMode"].as_u64() { Some(0) => Layout::Stacked, Some(1) => Layout::Cutouts, Some(2) => Layout::GroupedCutouts,
        _ => match frame["layout"].as_str().unwrap_or("stacked") { "stacked" => Layout::Stacked, "cutouts" => Layout::Cutouts, "grouped-cutouts" => Layout::GroupedCutouts, _ => panic!("invalid fixture layout") } };
    input.thin_stroke = frame["thinStroke"].as_bool().unwrap_or(false);
    input.output_size = frame["outputWidth"].as_i64().map(|width| (width, frame["outputHeight"].as_i64().unwrap())); input.hidden_regions = hidden; input
}
fn latin1(text: &str) -> Vec<u8> { text.chars().map(|c| { assert!(c as u32 <= 255); c as u8 }).collect() }
fn device(value: &Value) -> EmfDevice {
    let pair = |name: &str| [value[name][0].as_i64().unwrap() as i32, value[name][1].as_i64().unwrap() as i32];
    EmfDevice { pixels: pair("pixels"), millimeters: pair("millimeters"), micrometers: pair("micrometers") }
}
fn assert_bytes(actual: &[u8], expected: &[u8], label: &str, index: usize) {
    let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none() && actual.len() == expected.len(), "{label} case {index}, first differing byte {:?}, actual length {}, expected length {}", mismatch, actual.len(), expected.len());
}
fn text_corpus(name: &str, count: usize, geometries_count: usize, format: &str) {
    let reference = common::read_json(name);
    let shapes = reference["geometries"].as_array().unwrap(); assert_eq!(shapes.len(), geometries_count);
    let geometries: Vec<_> = shapes.iter().map(|shape| fitted_geometry::fitted_geometry(&shape["geometry"])).collect();
    let cases = reference["cases"].as_array().unwrap(); assert_eq!(cases.len(), count);
    for (index, frame) in cases.iter().enumerate() {
        let id = frame["geometry"].as_u64().unwrap() as usize; let ids = hidden(&shapes[id], frame);
        let input = input(&shapes[id], frame, &geometries[id], &ids);
        let actual = match format {
            "svg" => export_svg(&input).unwrap().into_bytes(), "ai" => export_ai(&input).unwrap().into_bytes(),
            "pdf" => export_pdf(&input).unwrap(), "eps" => export_eps(&input, &EpsOptions { compact: frame["postscriptStyle"] != 1,
                creation_date: Some(frame["creationDate"].as_str().unwrap().to_owned()) }).unwrap().into_bytes(), _ => unreachable!(),
        };
        let expected = frame["expected"].as_str().unwrap();
        if format == "pdf" { assert_bytes(&actual, &common::public_pdf_fixture(&latin1(expected)), format, index); } else { assert_bytes(&actual, expected.as_bytes(), format, index); }
    }
}
#[test] fn svg_all_17436_native_files() { text_corpus("svg-reference.json", 17436, 2743, "svg"); }
#[test] fn eps_all_41010_native_files() { text_corpus("eps-reference.json.gz", 41010, 3005, "eps"); }
#[test] fn pdf_all_22014_native_files() { text_corpus("pdf-reference.json.gz", 22014, 3264, "pdf"); }
#[test] fn ai_all_22014_native_files() { text_corpus("ai-reference.json.gz", 22014, 3264, "ai"); }

#[test]
fn committed_cli_fixtures_preserve_all_export_bytes() {
    // These small public native fixtures remain available even when the large
    // recording corpora are not installed. Cover layout, scale and thin strokes.
    for (format, count) in [("svg", 6), ("pdf", 6), ("eps", 12), ("ai", 6), ("emf", 24)] {
        let reference = common::read_json(&format!("{format}-cli-reference.json"));
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), count);
        for (index, frame) in cases.iter().enumerate() {
            let shape = &frame["input"];
            let geometry = fitted_geometry::fitted_geometry(&shape["geometry"]);
            let ids = hidden(shape, shape);
            let input = input(shape, shape, &geometry, &ids);
            let actual = match format {
                "svg" => export_svg(&input).unwrap().into_bytes(),
                "pdf" => export_pdf(&input).unwrap(),
                "eps" => export_eps(&input, &EpsOptions { compact: shape["compact"].as_bool().unwrap(),
                    creation_date: Some(shape["creationDate"].as_str().unwrap().to_owned()) }).unwrap().into_bytes(),
                "ai" => export_ai(&input).unwrap().into_bytes(),
                "emf" => export_emf(&input, &device(&shape["referenceDevice"])).unwrap(),
                _ => unreachable!(),
            };
            let expected = frame["expected"].as_str().unwrap();
            let expected = match format {
                "pdf" => common::public_pdf_fixture(&latin1(expected)),
                "emf" => common::public_emf_fixture(latin1(expected)),
                _ => expected.as_bytes().to_vec(),
            };
            assert_bytes(&actual, &expected, format, index);
        }
    }
}

fn rows(name: &str, mut consume: impl FnMut(Value)) {
    let file = std::fs::File::open(common::fixture_path(name)).unwrap();
    let reader = BufReader::new(flate2::read::MultiGzDecoder::new(file));
    for line in reader.lines() { let line = line.unwrap(); if !line.is_empty() { consume(serde_json::from_str(&line).unwrap()); } }
}
#[test]
fn dxf_all_65934_native_files() {
    let index = common::read_json("dxf-reference/index.json");
    let mut shapes = Vec::new(); let mut geometries = Vec::new(); let mut palettes = Vec::new(); let mut count = 0; let mut summary = Value::Null;
    for part in index["parts"].as_array().unwrap() {
        rows(&format!("dxf-reference/{}", part["file"].as_str().unwrap()), |frame| {
            match frame["type"].as_str().unwrap() {
                "metadata" => (), "summary" => summary = frame,
                "geometry" => {
                    assert_eq!(frame["index"], shapes.len()); let shape = frame["value"].clone();
                    geometries.push(fitted_geometry::fitted_geometry(&shape["geometry"]));
                    palettes.push(shape["palette"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect::<Vec<_>>()); shapes.push(shape);
                }
                "case" => {
                    let id = frame["geometry"].as_u64().unwrap() as usize; let ids = hidden(&shapes[id], &frame);
                    let input = input(&shapes[id], &frame, &geometries[id], &ids);
                    let mode = [DxfCurveMode::Splines, DxfCurveMode::SparseLines, DxfCurveMode::DenseLines][frame["curveMode"].as_u64().unwrap() as usize];
                    let actual = export_dxf(&input, &palettes[id], mode).unwrap(); assert_bytes(actual.as_bytes(), frame["expected"].as_str().unwrap().as_bytes(), "dxf", count); count += 1;
                }
                kind => panic!("unexpected {kind}"),
            }
        });
    }
    assert_eq!(count, 65934); assert_eq!(shapes.len(), 3264); assert_eq!(summary["cases"], count); assert_eq!(summary["geometries"], shapes.len());
}
#[test]
fn emf_all_22014_native_files() {
    let mut shapes = Vec::new(); let mut geometries = Vec::new(); let mut reference_device = None; let mut count = 0; let mut summary = Value::Null;
    rows("emf-reference.ndjson.gz", |frame| match frame["type"].as_str().unwrap() {
        "metadata" => reference_device = Some(device(&frame["referenceDevice"])), "summary" => summary = frame,
        "geometry" => { assert_eq!(frame["index"], shapes.len()); geometries.push(fitted_geometry::fitted_geometry(&frame["value"]["geometry"])); shapes.push(frame["value"].clone()); }
        "case" => {
            let id = frame["geometry"].as_u64().unwrap() as usize; let ids = hidden(&shapes[id], &frame);
            let input = input(&shapes[id], &frame, &geometries[id], &ids);
            assert_bytes(&export_emf(&input, &reference_device.unwrap()).unwrap(), &common::public_emf_fixture(latin1(frame["expected"].as_str().unwrap())), "emf", count); count += 1;
        }
        kind => panic!("unexpected {kind}"),
    });
    assert_eq!(count, 22014); assert_eq!(shapes.len(), 3264); assert_eq!(summary["cases"], count);
    assert_eq!(summary["recordingSha256"], "6a4b5b3319e5094ccbb47c88bb214873e2eaeba6090e712734d0d0e8bbe20897");
}
#[test]
fn emf_all_402_expanded_native_files() {
    let reference = common::read_json("emf-expanded-reference.json.gz");
    let bytes = latin1(reference["cases"][0]["expected"].as_str().unwrap());
    let at = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let device = EmfDevice { pixels: [at(72), at(76)], millimeters: [at(80), at(84)], micrometers: [at(100), at(104)] };
    assert_eq!(reference["cases"].as_array().unwrap().len(), 402);
    for (index, frame) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let shape = &reference["geometries"][frame["geometry"].as_u64().unwrap() as usize]; let geometry = fitted_geometry::fitted_geometry(&shape["geometry"]);
        let ids = hidden(shape, frame); let input = input(shape, frame, &geometry, &ids);
        assert_bytes(&export_emf(&input, &device).unwrap(), &common::public_emf_fixture(latin1(frame["expected"].as_str().unwrap())), "expanded emf", index);
    }
}
#[test]
fn dxf_all_144_visible_overlong_fields_are_errors() {
    let reference = common::read_json("dxf-overlong-reference.json"); assert_eq!(reference["cases"].as_array().unwrap().len(), 144);
    for frame in reference["cases"].as_array().unwrap() {
        let shape = &frame["input"]; let geometry = fitted_geometry::fitted_geometry(&shape["geometry"]); let ids = hidden(shape, shape);
        let input = input(shape, shape, &geometry, &ids); let palette: Vec<_> = shape["palette"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
        let mode = match shape["curveMode"].as_str().unwrap_or("dense-lines") { "splines" => DxfCurveMode::Splines, "sparse-lines" => DxfCurveMode::SparseLines, "dense-lines" => DxfCurveMode::DenseLines, _ => panic!() };
        assert!(export_dxf(&input, &palette, mode).unwrap_err().0.contains("256-byte"));
    }
}

#[test]
fn serializer_validation_and_consumer_failures_preserve_boundaries() {
    use openvector::vector_file::{TextOutput, BinaryOutput};
    use openvector::vector_output::VectorOutputError;
    let reference = common::read_json("dxf-cli-reference.json");
    let shape = &reference["cases"][0]["input"];
    let geometry = fitted_geometry::fitted_geometry(&shape["geometry"]);
    let before = geometry.clone();
    let ids = hidden(shape, shape);
    let input = input(shape, shape, &geometry, &ids);
    let palette: Vec<_> = shape["palette"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    let eps = EpsOptions { compact: false, creation_date: Some("Tue Oct 20 12:34:56 2026".into()) };
    struct Reject { checkpoints: usize }
    impl TextOutput for Reject {
        fn write(&mut self, _: &str, _: bool) -> Result<(), VectorOutputError> { Err(VectorOutputError("consumer failed".into())) }
        fn progress(&mut self, _: i32) -> Result<(), VectorOutputError> { self.checkpoints += 1; Ok(()) }
    }
    impl BinaryOutput for Reject {
        fn write(&mut self, _: &[u8], _: u64) -> Result<(), VectorOutputError> { Err(VectorOutputError("consumer failed".into())) }
    }
    let mut reject = Reject { checkpoints: 0 };
    for result in [
        openvector::svg::serialize_svg(&input, &mut reject),
        openvector::eps::serialize_eps(&input, &eps, &mut reject),
        openvector::pdf::serialize_pdf(&input, &mut reject),
        openvector::ai::serialize_ai(&input, &mut reject),
        openvector::dxf::serialize_dxf(&input, &palette, DxfCurveMode::Splines, &mut reject),
    ] { assert_eq!(result.unwrap_err().0, "consumer failed"); }
    assert_eq!(reject.checkpoints, 0);
    let emf = common::read_json("emf-cli-reference.json");
    let emf_device = device(&emf["cases"][0]["input"]["referenceDevice"]);
    assert_eq!(openvector::emf::serialize_emf(&input, &emf_device, &mut reject).unwrap_err().0, "consumer failed");
    for date in ["", "line\nbreak", "line\rbreak", "é", &"x".repeat(256)] {
        assert!(export_eps(&input, &EpsOptions { compact: false, creation_date: Some(date.into()) }).unwrap_err().0.contains("printable ASCII"));
    }
    for palette in [&[][..], &[0.0, 0.0, 0.0], &[0.0, -1.0, 0.0, 1.0], &[0.0, 0.0, f32::NAN, 1.0]] {
        assert!(export_dxf(&input, palette, DxfCurveMode::DenseLines).is_err());
    }
    let mut invalid_device = emf_device; invalid_device.pixels[0] = 0;
    assert!(export_emf(&input, &invalid_device).unwrap_err().0.contains("referenceDevice"));
    let mut invalid = input; invalid.width = 0;
    assert!(export_svg(&invalid).is_err()); assert!(export_pdf(&invalid).is_err()); assert!(export_ai(&invalid).is_err());
    assert!(export_eps(&invalid, &eps).is_err()); assert!(export_dxf(&invalid, &palette, DxfCurveMode::Splines).is_err()); assert!(export_emf(&invalid, &emf_device).is_err());
    assert_eq!(geometry, before);
}

fn double_bits(text: &str) -> f64 {
    assert_eq!(text.len(), 16);
    f64::from_le_bytes(std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap()))
}
fn bits_hex(value: f64) -> String { value.to_le_bytes().iter().map(|byte| format!("{byte:02x}")).collect() }

#[test]
fn dxf_all_126984_coordinate_pairs_and_30_special_pairs() {
    use openvector::vector_output::{VectorOutputOptions, transform_dxf_point, format_dxf_coordinate};
    use openvector::decimal_format::fixed_decimal;
    for (name, expected_count, special) in [("dxf-coordinates-reference.ndjson.gz", 126984, false), ("dxf-special-coordinates-reference.ndjson.gz", 30, true)] {
        let mut count = 0; let mut overlong = 0; let mut nonfinite = 0; let mut summary = Value::Null;
        rows(name, |frame| match frame["type"].as_str().unwrap() {
            "metadata" => assert_eq!(frame["sourceSha256"], "c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1"),
            "summary" => summary = frame,
            "case" => {
                let options = VectorOutputOptions { width: frame["width"].as_i64().unwrap() as i32, height: frame["height"].as_i64().unwrap() as i32,
                    output_width: frame["outputWidth"].as_i64().unwrap() as i32, output_height: frame["outputHeight"].as_i64().unwrap() as i32,
                    shape_mode: 0, thin_stroke: false, hidden_size: 0, hidden_words: Vec::new() };
                let point: [f64; 2] = std::array::from_fn(|axis| if special { double_bits(frame["pointBits"][axis].as_str().unwrap()) } else { frame["point"][axis].as_f64().unwrap() });
                for (axis, coordinate) in transform_dxf_point(&options, point[0], point[1]).into_iter().enumerate() {
                    assert_eq!(bits_hex(coordinate), frame["expected"]["bits"][axis], "{name} pair {count} axis {axis}");
                    assert_eq!(fixed_decimal(coordinate, 16), frame["expected"]["fields"][axis], "{name} pair {count} axis {axis}");
                    if frame["expected"]["emitted"][axis].is_null() {
                        assert!(format_dxf_coordinate(coordinate).unwrap_err().0.contains("256-byte")); overlong += 1;
                    } else { assert_eq!(format_dxf_coordinate(coordinate).unwrap(), frame["expected"]["emitted"][axis], "{name} pair {count} axis {axis}"); }
                    if !coordinate.is_finite() { nonfinite += 1; }
                }
                count += 1;
            }
            kind => panic!("unknown coordinate fixture {kind}"),
        });
        assert_eq!(count, expected_count);
        assert_eq!((overlong, nonfinite), if special { (0, 60) } else { (98, 109) });
        assert_eq!(summary, serde_json::json!({"type":"summary","cases":count,"overlongFields":overlong,"nonfiniteFields":nonfinite}));
    }
}

#[test]
fn emf_all_277909_native_integer_storage_pairs() {
    use openvector::vector_output::transform_emf_point;
    let mut count = 0; let mut summary = Value::Null;
    rows("emf-coordinates-reference.ndjson.gz", |frame| match frame["type"].as_str().unwrap() {
        "metadata" => { assert_eq!(frame["sourceSha256"], "c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1"); assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); }
        "summary" => summary = frame,
        "case" => {
            let point = [0, 1].map(|axis| double_bits(frame["pointBits"][axis].as_str().unwrap()));
            let expected = [0, 1].map(|axis| frame["expected"][axis].as_i64().unwrap() as i32);
            assert_eq!(transform_emf_point(frame["height"].as_i64().unwrap() as i32, point[0], point[1]), expected, "EMF pair {count}");
            count += 1;
        }
        kind => panic!("unknown coordinate fixture {kind}"),
    });
    assert_eq!(count, 277909); assert_eq!(summary, serde_json::json!({"type":"summary","cases":count}));
}

#[test]
fn emf_all_93880_native_device_bounds_and_frames() {
    use openvector::vector_output::EmfDeviceMapper;
    let mut mapper = None; let mut count = 0; let mut summary = Value::Null; let mut reference_device = Value::Null;
    rows("emf-device-bounds-reference.ndjson.gz", |frame| match frame["type"].as_str().unwrap() {
        "metadata" => { let device = device(&frame["referenceDevice"]); mapper = Some(EmfDeviceMapper::new(device.pixels, device.millimeters)); reference_device = frame["referenceDevice"].clone(); }
        "summary" => summary = frame,
        "case" => {
            let mapper = mapper.as_ref().unwrap();
            let point = mapper.point(frame["point"][0].as_f64().unwrap(), frame["point"][1].as_f64().unwrap()).unwrap();
            assert_eq!(serde_json::json!([point[0].min(0), point[1].min(0), point[0].max(0), point[1].max(0)]), frame["fillBounds"], "device bounds {count}");
            let bounds: Vec<_> = frame["bounds"].as_array().unwrap().iter().map(|value| value.as_i64().unwrap() as i32).collect();
            assert_eq!(serde_json::to_value(mapper.frame(&bounds).unwrap()).unwrap(), frame["frame"], "device frame {count}");
            count += 1;
        }
        kind => panic!("unknown device fixture {kind}"),
    });
    assert_eq!(count, 93880);
    assert_eq!(summary, serde_json::json!({"type":"summary","cases":count,"missingFills":0,"referenceDevice":reference_device}));
}
