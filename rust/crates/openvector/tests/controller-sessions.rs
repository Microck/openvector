mod common;
mod editor_support;

use editor_support::{compare_json, hex};
use openvector::{document_controls::{document_palette_key, DocumentSettings}, image_pipeline::{prepare_image_state, classify_image_state}, palette::CoreSession};
use serde_json::{json, Value};

fn f32_bytes(values: &[f32]) -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() }
fn f64_bytes(values: &[f64]) -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() }
fn controller_input(frame: &Value) -> Value {
    let mut input = frame.clone();
    input["bgra"] = json!(common::b64(frame["bgra"].as_str().unwrap()));
    input["hasAlpha"] = json!(true);
    input
}
fn compare_raster(image: &openvector::image_resize::OwnedImage, frame: &Value, dimensions: &str, pixels: &str) {
    compare_json(&json!([image.width, image.height]), &frame[dimensions], dimensions);
    assert_eq!(image.bgra, common::b64(frame[pixels].as_str().unwrap()), "{} {pixels}", frame["id"]);
}

#[test]
fn retained_controller_sessions_match_all_successes_and_fatal_random_effects() {
    let mut session = CoreSession::default();
    let mut session_id = None;
    let (mut completed, mut fatal, mut sessions) = (0, 0, 0);
    for frame in common::read_ndjson("image-controller-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["stableNativePayloadRepeatIdentical"], true); continue; }
        if session_id != Some(frame["session"].as_u64().unwrap()) {
            session = CoreSession::default(); session_id = frame["session"].as_u64(); sessions += 1;
        }
        assert_eq!(session.random_state() as u64, frame["initialRandomState"].as_u64().unwrap(), "{}", frame["id"]);
        let input = controller_input(&frame); let before = input.clone();
        let result = session.analyze_image(&input);
        if frame["outcome"] == "empty-peak-fault" {
            assert!(result.unwrap_err().contains("has no histogram peaks"), "{}", frame["id"]); fatal += 1;
        } else {
            let mut analyzed = result.unwrap();
            compare_raster(&analyzed.image.source, &frame, "preparedDimensions", "preparedBgra");
            compare_raster(&analyzed.image.working, &frame, "workingDimensions", "workingBgra");
            assert_ne!(analyzed.image.source.bgra.as_ptr(), analyzed.image.working.bgra.as_ptr());
            let summary = analyzed.to_json();
            compare_json(&summary["imageClassification"], &frame["imageClassification"], "image classification");
            assert_eq!(f32_bytes(&analyzed.image_analysis.features), hex(frame["imageFeatureBits"].as_str().unwrap()));
            compare_json(&analyzed.parameters["Shared::is_anti_aliased"], &frame["shared"][1], "anti alias");
            compare_json(&analyzed.parameters["Shared::image_type_code"], &frame["shared"][5], "image type code");
            compare_json(&summary["paletteClassification"], &frame["paletteClassification"], "palette classification");
            assert_eq!(f64_bytes(&analyzed.palette_analysis.features), hex(frame["paletteFeatureBits"].as_str().unwrap()));
            assert_eq!(f64_bytes(&analyzed.palette_analysis.peak_weights), hex(frame["peakWeightBits"].as_str().unwrap()));
            assert_eq!(analyzed.palette_analysis.candidates.len(), 11);
            assert_eq!(frame["candidates"].as_array().unwrap().len(), 11);
            for (index, candidate) in analyzed.palette_analysis.candidates.iter().enumerate() {
                let expected = &frame["candidates"][index];
                assert_eq!(candidate.color_count as u64, expected["colorCount"].as_u64().unwrap());
                assert_eq!(f32_bytes(&candidate.generated.palette), hex(expected["paletteBits"].as_str().unwrap()));
                assert_eq!(f64_bytes(&candidate.generated.weights), hex(expected["weightBits"].as_str().unwrap()));
                assert_eq!(f32_bytes(&[analyzed.recommendation_scores[index], candidate.score.error, candidate.score.colors]), hex(expected["scoreBits"].as_str().unwrap()), "{} candidate {index}", frame["id"]);
            }
            assert_eq!(analyzed.palette_analysis.candidates.last().unwrap().generated.indices.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(), common::b64(frame["indices"].as_str().unwrap()));
            assert_eq!(analyzed.random_state as u64, frame["randomState"].as_u64().unwrap());
            analyzed.image.source.bgra.fill(0);
            assert_eq!(input, before);
            compare_raster(&analyzed.image.working, &frame, "workingDimensions", "workingBgra");
            completed += 1;
        }
        assert_eq!(session.random_state() as u64, frame["randomState"].as_u64().unwrap(), "{}", frame["id"]);
        assert_eq!(input, before);
    }
    assert_eq!((completed, fatal, sessions), (42, 2, 3));
}

#[test]
fn controller_validation_and_owned_parameters_preserve_inputs() {
    let frame = common::read_ndjson("image-controller-reference.ndjson.gz").into_iter().find(|f| f["type"] != "metadata").unwrap();
    let mut input = controller_input(&frame);
    let session = CoreSession::default();
    let mut bad_mode = input.clone(); bad_mode["mode"] = json!(9);
    let mut no_palette = input.clone(); no_palette.as_object_mut().unwrap().remove("paletteControls");
    let mut no_shadow = input.clone(); no_shadow["parameters"].as_object_mut().unwrap().remove("shadowing");
    for invalid in [Value::Null, bad_mode, no_palette, no_shadow] {
        assert!(session.analyze_image(&invalid).is_err()); assert_eq!(session.random_state(), 1);
    }
    input["parameters"]["ContourSmoother::prior_types"] = json!([3,3,3]);
    input["paletteControls"]["width"] = json!(0); input["paletteControls"]["height"] = json!(0); input["paletteControls"]["bgra"] = json!([]);
    let mut analyzed = session.analyze_image(&input).unwrap();
    compare_json(&analyzed.to_json()["paletteClassification"], &frame["paletteClassification"], "owned controls");
    assert_eq!(f64_bytes(&analyzed.palette_analysis.features), hex(frame["paletteFeatureBits"].as_str().unwrap()));
    analyzed.parameters["ContourSmoother::prior_types"][0] = json!(0);
    assert_eq!(input["parameters"]["ContourSmoother::prior_types"], json!([3,3,3]));
}

#[test]
fn complete_source_working_classifications_match_native_bytes() {
    let (mut cases, mut reduced) = (0, 0);
    for frame in common::read_ndjson("image-source-state-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); continue; }
        let pixels = common::b64(frame["bgra"].as_str().unwrap()); let before = pixels.clone();
        let image = prepare_image_state(frame["width"].as_u64().unwrap() as usize, frame["height"].as_u64().unwrap() as usize, &pixels, true, frame["mode"].as_u64().unwrap() as u32, frame["amount"].as_u64().unwrap() as u32, &frame["parameters"]).unwrap();
        compare_raster(&image.source, &frame, "preparedDimensions", "preparedBgra");
        compare_raster(&image.working, &frame, "workingDimensions", "workingBgra");
        assert_ne!(pixels.as_ptr(), image.source.bgra.as_ptr());
        assert_ne!(image.source.bgra.as_ptr(), image.working.bgra.as_ptr());
        let analyzed = classify_image_state(&image, frame["extension"].as_str().unwrap(), &frame["parameters"]).unwrap();
        assert_eq!(f32_bytes(&analyzed.features), hex(frame["featureBits"].as_str().unwrap()), "{} features", frame["id"]);
        let c = analyzed.classification;
        compare_json(&json!({"label":c.label,"imageType":c.image_type,"quality":c.quality}), &frame["expected"], "classification");
        compare_raster(&analyzed.blurred, &frame, "blurDimensions", "blurredBgra");
        for (index, gradient) in [&analyzed.gradient, &analyzed.blur_gradient].into_iter().enumerate() {
            assert_eq!(f32_bytes(gradient), common::b64(frame["gradientBits"][index].as_str().unwrap()), "{} gradient {index}", frame["id"]);
        }
        assert_eq!(analyzed.measurement, common::b64(frame["measurement"].as_str().unwrap()));
        assert_eq!(analyzed.regions.indices.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(), common::b64(frame["indices"].as_str().unwrap()));
        let moments = frame["regionMomentBits"].as_array().unwrap();
        assert_eq!(analyzed.regions.counts.len(), moments.len());
        assert_eq!(analyzed.regions.sums.len(), moments.len()*4);
        for (region, expected) in moments.iter().enumerate() {
            let mut bytes = analyzed.regions.counts[region].to_le_bytes().to_vec();
            bytes.extend(f32_bytes(&analyzed.regions.sums[region*4..region*4+4]));
            assert_eq!(bytes, hex(expected.as_str().unwrap()), "{} region {region}", frame["id"]);
        }
        assert_eq!(pixels, before);
        compare_raster(&image.source, &frame, "preparedDimensions", "preparedBgra");
        compare_raster(&image.working, &frame, "workingDimensions", "workingBgra");
        reduced += usize::from((image.source.width,image.source.height) != (image.working.width,image.working.height)); cases += 1;
    }
    assert_eq!((cases, reduced), (16, 12));
}

#[test]
fn every_native_float32_document_key_matches() {
    let mut cases = 0; let mut expected_cases = None;
    for frame in common::read_ndjson("document-key-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); expected_cases = frame["cases"].as_u64(); continue; }
        let fields = frame["fields"].as_array().unwrap();
        let bytes = hex(frame["backgroundBits"].as_str().unwrap()); assert_eq!(bytes.len(), 16);
        let settings = DocumentSettings { category: fields[0].as_i64().unwrap() as i32, sensitivity: fields[1].as_i64().unwrap() as i32, advanced: fields[2].as_i64().unwrap()!=0, anti_alias: fields[3].as_i64().unwrap()!=0, background_mode: fields[4].as_i64().unwrap() as i32, background: std::array::from_fn(|i| f32::from_le_bytes(bytes[i*4..i*4+4].try_into().unwrap())), ..Default::default() };
        assert_eq!(document_palette_key(&settings), frame["key"].as_str().unwrap(), "{}", frame["id"]); cases += 1;
    }
    assert_eq!(Some(cases), expected_cases); assert!(cases > 21000);
}

#[test]
fn independent_source_blur_and_distinct_stride_gradients_match_native() {
    use openvector::image_classification::{blur_classifier_pixels, measure_classifier_gradients};
    let mut cases = 0;
    for frame in common::read_ndjson("image-source-gradients-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); continue; }
        let pixels = common::b64(frame["bgra"].as_str().unwrap());
        let before = pixels.clone();
        let image = prepare_image_state(frame["width"].as_u64().unwrap() as usize, frame["height"].as_u64().unwrap() as usize, &pixels, true, frame["mode"].as_u64().unwrap() as u32, frame["amount"].as_u64().unwrap() as u32, &json!({"Shared::is_anti_aliased":frame["antiAlias"]})).unwrap();
        assert_ne!(image.source.bgra.as_ptr(), pixels.as_ptr());
        assert_ne!(image.working.bgra.as_ptr(), image.source.bgra.as_ptr());
        assert_eq!(image.source.bgra, common::b64(frame["preparedBgra"].as_str().unwrap()));
        assert_eq!(image.working.bgra, common::b64(frame["workingBgra"].as_str().unwrap()));
        let blurred = blur_classifier_pixels(image.source.width, image.source.height, &image.source.bgra);
        assert_eq!(blurred, common::b64(frame["blurredBgra"].as_str().unwrap()));
        let gradients = measure_classifier_gradients(image.working.width, image.working.height, &image.working.bgra, &blurred, image.source.width);
        for (index, gradient) in [&gradients.gradient, &gradients.blur_gradient].into_iter().enumerate() {
            assert_eq!(f32_bytes(gradient), common::b64(frame["gradientBits"][index].as_str().unwrap()), "{} gradient {index}", frame["id"]);
        }
        assert_eq!(pixels, before);
        assert_eq!(image.source.bgra, common::b64(frame["preparedBgra"].as_str().unwrap()));
        assert_eq!(image.working.bgra, common::b64(frame["workingBgra"].as_str().unwrap()));
        cases += 1;
    }
    assert_eq!(cases, 5);
}
