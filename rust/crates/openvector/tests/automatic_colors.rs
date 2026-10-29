//! Port of the `test/automatic-colors.test.mjs` groups covering the palette
//! count model, its feature producer and the image type/quality classifier
//! stages. Session, segmentation, subpixel, shadow and complete-pipeline
//! groups live with their own modules.
mod common;
use common::read_ndjson;
use openvector::automatic_colors::{build_palette_features, classify_palette_features, PaletteCandidate};
use openvector::image_classification::{blur_classifier_pixels, classify_image_features, measure_classifier_colors,
    measure_classifier_edges, measure_classifier_gradients, measure_classifier_regions, premultiply_classifier_pixels,
    sample_classifier_edge, ClassifierRegions};
use openvector::palette::{byte_to_unit, PaletteScore};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn joined_hex(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().flat_map(|v| hex(v.as_str().unwrap())).collect()
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn f64_bytes(values: &[f64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn decode_doubles(value: &Value) -> Vec<f64> {
    value.as_array().unwrap().iter().map(|v| f64::from_le_bytes(hex(v.as_str().unwrap()).try_into().unwrap())).collect()
}

fn bytes(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()
}

fn usize_of(value: &Value) -> usize {
    value.as_u64().unwrap() as usize
}

fn count_label(labels: &mut Map<String, Value>, label: &str) {
    let entry = labels.entry(label.to_string()).or_insert(json!(0));
    *entry = json!(entry.as_i64().unwrap() + 1);
}

fn palette_decision(features: &[f64]) -> Value {
    let decision = classify_palette_features(features);
    json!({ "label": decision.label, "mode": decision.mode, "colorCount": decision.color_count })
}

fn image_decision(features: &[f32], extension: &str) -> Value {
    let decision = classify_image_features(features, extension);
    json!({ "label": decision.label, "imageType": decision.image_type, "quality": decision.quality })
}

#[test]
fn palette_count_model_and_label_decoding_match_all_original_feature_vectors() {
    let mut count = 0;
    let mut summary = Value::Null;
    let mut labels = Map::new();
    let mut profiles = HashSet::new();
    for frame in read_ndjson("classification-model-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["sourceSha256"], "c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1");
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
        } else if frame["type"] == "summary" {
            summary = frame;
        } else {
            assert_eq!(frame["type"], "case");
            let features = decode_doubles(&frame["featureBits"]);
            assert_eq!(features.len(), 62);
            assert_eq!(palette_decision(&features), frame["expected"], "original palette classifier {count}");
            count_label(&mut labels, frame["expected"]["label"].as_str().unwrap());
            profiles.insert(frame["profile"].as_str().unwrap().to_string());
            count += 1;
        }
    }
    assert_eq!(count, 31604);
    assert_eq!(profiles.len(), 10);
    let mut keys: Vec<i64> = labels.keys().map(|k| k.parse().unwrap()).collect();
    keys.sort();
    assert_eq!(summary, json!({ "type": "summary", "cases": count, "labels": labels }));
    assert_eq!(keys, vec![2, 3, 4, 5, 6, 7, 8, 9, 13]);
}

#[test]
fn palette_feature_assembly_matches_every_original_double_bit_and_native_fatal_cases() {
    let (mut count, mut fatal_cases) = (0, 0);
    let mut summary = Value::Null;
    for frame in read_ndjson("classification-features-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["completedOutputRepeatIdentical"], true);
            assert_eq!(frame["nativeFaultsRepeatIdentical"], true);
            continue;
        }
        if frame["type"] == "summary" { summary = frame; continue; }
        let candidates: Vec<PaletteCandidate> = frame["candidates"].as_array().unwrap().iter().map(|candidate| {
            let score = hex(candidate["scoreBits"].as_str().unwrap());
            PaletteCandidate { weights: decode_doubles(&candidate["weightBits"]),
                score: PaletteScore { total: read_f32(&score, 0), error: read_f32(&score, 4), colors: read_f32(&score, 8) } }
        }).collect();
        let bgra = bytes(&frame["input"]["bgra"]);
        let built = build_palette_features(&bgra, &candidates, &decode_doubles(&frame["peakWeightBits"]));
        let id = frame["input"]["id"].as_str().unwrap();
        if !frame["expectedFault"].is_null() {
            assert_eq!(frame["expectedFault"], json!({ "code": "0xc0000005", "instruction": "0x48cac2", "stage": 13 }));
            assert!(built.unwrap_err().contains("Automatic color selection has no histogram peaks"), "{id}");
            fatal_cases += 1;
        } else {
            let features = built.unwrap();
            assert_eq!(f64_bytes(&features), joined_hex(&frame["featureBits"]), "{id}");
            assert_eq!(palette_decision(&features), frame["expected"], "{id}");
        }
        count += 1;
    }
    assert_eq!(count, 576);
    assert_eq!(fatal_cases, 306);
    assert_eq!(summary["cases"], count);
    assert_eq!(summary["candidates"], count * 11);
    assert_eq!(summary["fatalCases"], fatal_cases);
}

#[test]
fn image_type_quality_model_matches_all_original_float_vectors_and_extension_decisions() {
    let mut count = 0;
    let mut summary = Value::Null;
    let mut labels = Map::new();
    let mut extensions = HashSet::new();
    let mut final_extension_decisions = std::collections::HashMap::new();
    for frame in read_ndjson("image-classifier-model-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["sourceSha256"], "c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1");
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
        } else if frame["type"] == "summary" {
            summary = frame;
        } else {
            assert_eq!(frame["type"], "case");
            let raw = joined_hex(&frame["featureBits"]);
            let features: Vec<f32> = (0..raw.len()).step_by(4).map(|offset| read_f32(&raw, offset)).collect();
            assert_eq!(features.len(), 20);
            let extension = frame["extension"].as_str().unwrap();
            assert_eq!(image_decision(&features, extension), frame["expected"], "native image classifier {count}");
            let label = frame["expected"]["label"].as_str().unwrap().to_string();
            count_label(&mut labels, &label);
            extensions.insert(extension.to_string());
            if count >= 57450 { final_extension_decisions.insert(extension.to_string(), label); }
            count += 1;
        }
    }
    assert_eq!(count, 57472);
    assert_eq!(extensions.len(), 22);
    let mut keys: Vec<&String> = labels.keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["logo 2", "logo_aa 0", "logo_aa 1", "logo_aa 2", "photo 1"]);
    assert_eq!(summary, json!({ "type": "summary", "cases": count, "labels": labels, "thresholdComparisons": 65 }));
    assert_eq!(final_extension_decisions.len(), 22);
    for (extension, label) in &final_extension_decisions {
        let expected = if [".bmp", ".jpeg", ".png", ".tiff"].contains(&extension.as_str()) { "logo_aa 1" } else { "logo_aa 2" };
        assert_eq!(label, expected);
    }
}

fn assert_classifier_feature_stages(frame: &Value, edge_scale: f64, edge_floor: f64) {
    let id = frame["input"]["id"].as_str().unwrap();
    let (width, height) = (usize_of(&frame["dimensions"][4]), usize_of(&frame["dimensions"][5]));
    let working = bytes(&frame["workingBgra"]);
    let blurred = blur_classifier_pixels(width, height, &working);
    assert_eq!(blurred, bytes(&frame["blurredBgra"]), "{id} independent blur");
    let actual = measure_classifier_gradients(width, height, &working, &blurred, width);
    assert_eq!(f32_bytes(&actual.gradient), joined_hex(&frame["gradientBits"][0]), "{id} gradient");
    assert_eq!(f32_bytes(&actual.blur_gradient), joined_hex(&frame["gradientBits"][1]), "{id} blurred gradient");
    let feature_bits = joined_hex(&frame["featureBits"]);
    assert_eq!(f32_bytes(&actual.root_mean_square), feature_bits[0..8], "{id} RMS");
    let edges = measure_classifier_edges(width, height, &working);
    assert_eq!(f32_bytes(&edges), feature_bits[8..20], "{id} edge means");
    let indices: Vec<i32> = frame["indices"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap() as i32).collect();
    let moments: Vec<Vec<u8>> = frame["regionMomentBits"].as_array().unwrap().iter().map(|v| hex(v.as_str().unwrap())).collect();
    let counts: Vec<i32> = moments.iter().map(|m| i32::from_le_bytes(m[0..4].try_into().unwrap())).collect();
    let sums: Vec<f32> = moments.iter().flat_map(|m| (0..4).map(move |channel| read_f32(m, 4 + channel * 4))).collect();
    let region_features = measure_classifier_regions(width, height, &working,
        ClassifierRegions { indices: &indices, counts: &counts, sums: &sums });
    assert_eq!(f32_bytes(&region_features), feature_bits[20..56], "{id} region features");
    let colors = measure_classifier_colors(width, height, &working, edge_scale, edge_floor);
    assert_eq!(colors.measurement, bytes(&frame["measurement"]), "{id} measurement");
    assert_eq!(f32_bytes(&colors.features), feature_bits[56..80], "{id} color features");
    let mut features = [0.0f32; 20];
    features[0..2].copy_from_slice(&actual.root_mean_square);
    features[2..5].copy_from_slice(&edges);
    features[5..14].copy_from_slice(&region_features);
    features[14..20].copy_from_slice(&colors.features);
    assert_eq!(f32_bytes(&features), feature_bits, "{id} complete feature vector");
    assert_eq!(image_decision(&features, frame["input"]["extension"].as_str().unwrap()), frame["expected"], "{id}");
}

#[test]
fn all_twenty_classifier_feature_slots_match_complete_native_image_stage_recordings() {
    let (mut count, mut completed, mut empty) = (0, 0, 0);
    let mut summary = Value::Null;
    let mut previous_state = json!(1);
    let mut labels = Map::new();
    for frame in read_ndjson("image-features-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); continue; }
        if frame["type"] == "summary" { summary = frame; continue; }
        assert_eq!(frame["type"], "case");
        assert_eq!(frame["initialRandomState"], previous_state);
        let input = &frame["input"];
        assert_eq!(frame["dimensions"].as_array().unwrap()[0..4].to_vec(),
            vec![input["width"].clone(), input["height"].clone(), input["width"].clone(), input["height"].clone()]);
        let (width, height) = (usize_of(&frame["dimensions"][4]), usize_of(&frame["dimensions"][5]));
        if frame["outcome"] == "empty-working-image" {
            assert_eq!((width, height), (0, 0));
            assert_eq!(input["preprocessingMode"], 1);
            assert_eq!(input["preprocessingAmount"], 1);
            assert!(usize_of(&input["width"]) <= 4 || usize_of(&input["height"]) <= 4);
            empty += 1;
        } else {
            assert_eq!(frame["outcome"], "classified");
            assert_classifier_feature_stages(&frame, 50.0, 0.01);
            count_label(&mut labels, frame["expected"]["label"].as_str().unwrap());
            completed += 1;
        }
        previous_state = frame["randomState"].clone();
        count += 1;
    }
    assert_eq!((count, completed, empty), (96, 72, 24));
    assert_eq!(summary, json!({ "type": "summary", "cases": count, "completed": completed, "emptyWorkingImages": empty, "labels": labels }));
}

#[test]
fn initial_alpha_preparation_and_gaussian_blur_match_every_original_byte() {
    let (mut cases, mut compared, mut one_dimensional, mut changed, mut exhaustive) = (0, 0, 0, 0, 0);
    let mut summary = Value::Null;
    for frame in read_ndjson("classifier-blur-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            assert_eq!(frame["wholeHardwareTraceRepeatIdentical"], true);
            assert_eq!(frame["originalImageResultPayloadIdentical"], true);
            assert_eq!(frame["kernelCalls"], 288);
            continue;
        }
        if frame["type"] == "summary" { summary = frame; continue; }
        assert_eq!(frame["type"], "case");
        assert_eq!(frame["outcome"], "blurred");
        let id = frame["id"].as_str().unwrap();
        let (width, height) = (usize_of(&frame["width"]), usize_of(&frame["height"]));
        let original = bytes(&frame["inputBgra"]);
        let prepared = premultiply_classifier_pixels(&original);
        assert_eq!(prepared, bytes(&frame["bgra"]), "{id} initial native preparation");
        assert_eq!(bytes(&frame["inputBgra"]), original, "{id} raw input ownership");
        let blurred = blur_classifier_pixels(width, height, &prepared);
        assert_eq!(blurred, bytes(&frame["expected"]), "{id} native Gaussian bytes");
        assert_eq!(prepared, bytes(&frame["bgra"]), "{id} prepared input ownership");
        for pixel in 0..width * height {
            assert_eq!(blurred[pixel * 4 + 3], prepared[pixel * 4 + 3], "{id} original alpha {pixel}");
        }
        cases += 1;
        compared += blurred.len();
        one_dimensional += (width == 1 || height == 1) as usize;
        changed += original.iter().zip(&prepared).any(|(a, b)| a != b) as usize;
        if id.starts_with("all-alpha-byte-pairs-") { exhaustive += width * height; }
    }
    assert_eq!((cases, compared, one_dimensional, changed, exhaustive), (2224, 3208328, 290, 648, 65536));
    assert_eq!(summary, json!({ "type": "summary", "cases": cases, "completed": cases, "emptyWorkingImages": 0,
        "comparedBytes": compared, "oneDimensionalCases": one_dimensional, "kernelCalls": 288,
        "changedPreparationCases": changed, "exhaustiveAlphaBytePairs": exhaustive }));
}

#[test]
fn six_pixel_classifier_edges_match_all_original_stencils_including_signed_nonfinite_profiles() {
    let (mut count, mut accepted, mut nonfinite) = (0, 0, 0);
    let mut summary = Value::Null;
    let mut profile = [0.0f32; 5];
    for frame in read_ndjson("classifier-edges-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); continue; }
        if frame["type"] == "summary" { summary = frame; continue; }
        assert_eq!(frame["type"], "case");
        let samples: Vec<f32> = bytes(&frame["bgra"]).into_iter().map(byte_to_unit).collect();
        assert_eq!(f32_bytes(&samples), joined_hex(&frame["expected"]["sampleBits"]), "native edge sample {count}");
        let threshold = read_f32(&hex(frame["thresholdBits"].as_str().unwrap()), 0);
        let selected = sample_classifier_edge(&samples, threshold, &mut profile);
        assert_eq!(selected, frame["expected"]["accepted"].as_bool().unwrap(), "native edge acceptance {count}");
        assert_eq!(f32_bytes(&profile), joined_hex(&frame["expected"]["profileBits"]), "native edge profile {count}");
        accepted += selected as usize;
        nonfinite += profile.iter().filter(|v| !v.is_finite()).count();
        count += 1;
    }
    assert_eq!(count, 71620);
    assert_eq!(summary, json!({ "type": "summary", "cases": count, "accepted": accepted, "nonfiniteProfileValues": nonfinite }));
    assert_eq!(nonfinite, 1550);
}

#[test]
fn multipeak_image_features_match_all_native_edge_control_and_color_distribution_cases() {
    let mut count = 0;
    let mut summary = Value::Null;
    let mut labels = Map::new();
    for frame in read_ndjson("classifier-colors-reference.ndjson.gz") {
        if frame["type"] == "metadata" { assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true); continue; }
        if frame["type"] == "summary" { summary = frame; continue; }
        assert_eq!(frame["type"], "case");
        assert_eq!(frame["outcome"], "classified");
        assert_eq!(frame["input"]["antiAlias"], 0);
        assert_classifier_feature_stages(&frame, frame["input"]["edgeScale"].as_f64().unwrap(), frame["input"]["edgeFloor"].as_f64().unwrap());
        count_label(&mut labels, frame["expected"]["label"].as_str().unwrap());
        count += 1;
    }
    assert_eq!(count, 78);
    assert_eq!(summary, json!({ "type": "summary", "cases": count, "completed": count, "emptyWorkingImages": 0, "labels": labels }));
}
