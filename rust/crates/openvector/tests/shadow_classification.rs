mod common;
mod smoothing_support;
use openvector::shadow_classification::{accept_shadow_features, classify_shadow_features, principal_color_direction, reassign_shadow_pixels, write_shadow_proposal, Proposal, ShadowAssignment};
use serde_json::{json, Value};
use smoothing_support::*;
use std::collections::{BTreeMap, BTreeSet};

fn hex_bytes(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len()).step_by(2).map(|n| u8::from_str_radix(&s[n..n + 2], 16).unwrap()).collect()
}
fn float_bits(v: &Value) -> f32 { f32::from_le_bytes(hex_bytes(v).try_into().unwrap()) }
fn double_bits(v: &Value) -> f64 { f64::from_le_bytes(hex_bytes(v).try_into().unwrap()) }

#[test]
fn shadow_models_match_all_51008_native_feature_and_control_cases() {
    let mut count = 0;
    let mut controls = BTreeSet::new();
    let mut outcomes = BTreeMap::<String, usize>::new();
    let mut summary = Value::Null;
    for frame in common::read_ndjson("shadow-models-reference.ndjson.gz") {
        match frame["type"].as_str().unwrap() {
            "metadata" => {
                assert_eq!(frame["sourceSha256"], "c491656b962debc215a8b6e757a9b2949ffe50a36b5f2db692706bcd4ebface1");
                assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            }
            "summary" => summary = frame,
            "case" => {
                assert_eq!(frame["randomState"], 1);
                let features: Vec<f64> = frame["featureBits"].as_array().unwrap().iter().map(double_bits).collect();
                assert_eq!(features.len(), 42);
                let control = i(&frame["control"]) as i32;
                let region = classify_shadow_features(&features);
                let accepted = accept_shadow_features(&features, control);
                assert_eq!(json!({"region": region, "accepted": accepted}), frame["expected"], "shadow models {count}");
                *outcomes.entry(format!("{}:{}", u8::from(region), u8::from(accepted))).or_default() += 1;
                controls.insert(control);
                count += 1;
            }
            other => panic!("unknown shadow model record {other}"),
        }
    }
    assert_eq!(count, 51008);
    assert_eq!(controls.into_iter().collect::<Vec<_>>(), vec![i32::MIN, -1, 0, 1, 2, 3, i32::MAX]);
    assert_eq!(outcomes.len(), 4);
    assert_eq!(summary, json!({"type": "summary", "cases": count, "recordedFeatureCases": 14042, "thresholdComparisons": 24, "outcomes": outcomes}));
}

#[test]
fn principal_colors_match_all_15249_native_matrix_states_and_4012_image_calls() {
    let (mut count, mut calls) = (0, 0);
    let mut cases = BTreeSet::new();
    let mut summary = Value::Null;
    for frame in common::read_ndjson("principal-colors-reference.ndjson.gz") {
        match frame["type"].as_str().unwrap() {
            "metadata" => {
                for key in ["wholeNativeResponseRepeatIdentical", "tracedImageResponseRepeatIdentical", "tracedOriginalResultPayloadIdentical"] { assert_eq!(frame[key], true); }
            }
            "summary" => summary = frame,
            "case" => {
                let matrix: [f32; 16] = frame["matrixBits"].as_array().unwrap().iter().map(float_bits).collect::<Vec<_>>().try_into().unwrap();
                let actual = principal_color_direction(&matrix, float_bits(&frame["previousEigenvalueBits"]) as f64,
                    i32::try_from(i(&frame["iterations"])).unwrap(), float_bits(&frame["epsilonBits"]));
                let mut encoded = (actual.eigenvalue as f32).to_le_bytes().to_vec();
                for c in actual.direction { encoded.extend(c.to_le_bytes()); }
                let expected: Vec<u8> = frame["expectedBits"].as_array().unwrap().iter().flat_map(hex_bytes).collect();
                assert_eq!(encoded, expected, "principal color {count}");
                if let Some(call) = frame.get("nativeImageCall") {
                    calls += 1;
                    cases.insert(format!("{}:{}", call["trace"], call["case"]));
                }
                count += 1;
            }
            other => panic!("unknown principal color record {other}"),
        }
    }
    assert_eq!((count, calls, cases.len()), (15249, 4012, 144));
    assert_eq!(summary, json!({"type": "summary", "cases": count, "nativeImageCalls": calls, "originalCallsPerImageRun": 2006,
        "nativeImageRuns": 2, "thresholdCalibrationMatrices": 256}));
}

#[test]
fn nonpositive_principal_iteration_limits_retain_the_initial_state() {
    let matrix = [2.0; 16];
    for iterations in [i32::MIN, -1, 0] {
        let actual = principal_color_direction(&matrix, -1.0, iterations, 1e-6);
        assert_eq!(actual.eigenvalue.to_bits(), (-1.0f64).to_bits());
        assert_eq!(actual.direction.map(f32::to_bits), [1.0f32.to_bits(); 4]);
    }
}

#[test]
fn shadow_proposals_and_130_ordered_fill_choices_match_all_1474_hardware_traces() {
    let mut by_image = BTreeMap::<String, Vec<Value>>::new();
    let mut images = BTreeMap::<String, usize>::new();
    let (mut count, mut accepted, mut rejected) = (0, 0, 0);
    let mut summary = Value::Null;
    for frame in common::read_ndjson("shadow-proposals-reference.ndjson.gz") {
        match frame["type"].as_str().unwrap() {
            "metadata" => {
                for key in ["wholeNativeResponseRepeatIdentical", "wholeHardwareTraceRepeatIdentical", "originalResultPayloadIdentical"] { assert_eq!(frame[key], true); }
            }
            "summary" => summary = frame,
            "case" => {
                let mut proposal = Proposal { cost: 0.0, target: -123 };
                let selected = write_shadow_proposal(i(&frame["width"]) as usize, i(&frame["height"]) as usize,
                    &bytes(&frame["bgra"]), &i32s(&frame["indices"]), &bytes(&frame["shadowFlags"]), i(&frame["pixel"]) as usize, &mut proposal);
                assert_eq!(selected, frame["expected"]["accepted"].as_bool().unwrap(), "proposal {count}");
                assert_eq!(proposal.cost.to_le_bytes().as_slice(), hex_bytes(&frame["expected"]["costBits"]), "proposal cost {count}");
                assert_eq!(proposal.target, if selected { i(&frame["expected"]["target"]) as i32 } else { -123 });
                if selected { accepted += 1; } else { rejected += 1; }
                let id = frame["id"].as_str().unwrap().to_string();
                *images.entry(id.clone()).or_default() += 1;
                by_image.entry(id).or_default().push(frame);
                count += 1;
            }
            other => panic!("unknown shadow proposal record {other}"),
        }
    }
    assert_eq!((count, accepted, rejected), (1474, 1274, 200));
    assert_eq!(summary, json!({"type": "summary", "cases": count, "accepted": accepted, "rejected": rejected, "images": images}));
    let mut assignments = 0;
    for (id, proposals) in &by_image {
        let first = &proposals[0];
        let mut choices = Vec::new();
        let mut previous: Option<Vec<i32>> = None;
        let mut best: Option<ShadowAssignment> = None;
        for frame in proposals {
            let indices = i32s(&frame["indices"]);
            if previous.as_ref().is_some_and(|p| p != &indices) { choices.push(best.take().unwrap()); }
            if frame["expected"]["accepted"] == true {
                let cost = float_bits(&frame["expected"]["costBits"]);
                if best.is_none_or(|b| cost < b.cost) {
                    best = Some(ShadowAssignment { pixel: i(&frame["pixel"]) as usize, target: i(&frame["expected"]["target"]) as i32, cost });
                }
            }
            previous = Some(indices);
        }
        choices.push(best.unwrap());
        let original = i32s(&first["indices"]);
        let before = original.clone();
        let (actual_map, actual_choices) = reassign_shadow_pixels(i(&first["width"]) as usize, i(&first["height"]) as usize,
            &bytes(&first["bgra"]), &original, &bytes(&first["shadowFlags"])).unwrap();
        assert_eq!(actual_choices, choices, "ordered shadow fill {id}");
        let mut expected_map = original.clone();
        for choice in &choices { expected_map[choice.pixel] = choice.target; }
        assert_eq!(actual_map, expected_map, "shadow reassignment {id}");
        assert_eq!(original, before);
        assignments += choices.len();
    }
    assert_eq!((assignments, by_image.len()), (130, 6));
}

#[test]
fn shadow_features_match_all_42_channels_covariances_and_directions_for_2006_regions() {
    use openvector::image_classification::{blur_classifier_pixels, measure_classifier_gradients};
    use openvector::shadow_classification::{measure_shadow_features, ShadowRegion};
    use openvector::subpixel::{build_subpixel_statistics, Neighborhood};
    let mut images = BTreeMap::new();
    let mut native_case = 0;
    for frame in common::read_ndjson("image-features-reference.ndjson.gz") {
        if frame["type"] != "case" { continue; }
        if frame["outcome"] == "classified" {
            images.insert(frame["input"]["id"].as_str().unwrap().to_string(), (native_case, frame));
        }
        native_case += 1;
    }
    let mut principal_calls = BTreeMap::<i64, Vec<Value>>::new();
    for frame in common::read_ndjson("principal-colors-reference.ndjson.gz") {
        if frame["type"] == "case" && frame["nativeImageCall"]["trace"] == "engine-principal-trace-native.bin" {
            principal_calls.entry(i(&frame["nativeImageCall"]["case"])).or_default().push(frame);
        }
    }
    let (mut cases, mut region_count) = (0, 0);
    for frame in common::read_ndjson("classifier-regions-reference.ndjson.gz") {
        if frame["type"] != "case" { continue; }
        let id = frame["id"].as_str().unwrap();
        let (native_case, image) = &images[id];
        let calls = &principal_calls[native_case];
        let (width, height) = (i(&frame["width"]) as usize, i(&frame["height"]) as usize);
        let bgra = bytes(&frame["bgra"]);
        let state = build_subpixel_statistics(width as i64, height as i64, &bgra,
            &i64s(&frame["stages"][3]["indices"]), &frame["parameters"], Neighborhood::Eight).unwrap();
        assert_eq!(state.indices, i32s(&frame["stages"][4]["indices"]), "{id} shadow preparation labels");
        let blurred = blur_classifier_pixels(width, height, &bgra);
        assert_eq!(blurred, bytes(&image["blurredBgra"]), "{id} independent shadow blur");
        let gradients = measure_classifier_gradients(width, height, &bgra, &blurred, width);
        let gradient: Vec<f64> = gradients.gradient.iter().map(|&v| v as f64).collect();
        let blur_gradient: Vec<f64> = gradients.blur_gradient.iter().map(|&v| v as f64).collect();
        let regions: Vec<ShadowRegion> = state.regions.iter().map(|r| ShadowRegion {
            count: r.count, interior_count: r.interior_count, sums: r.sums, squares: r.squares,
            color: r.color, boundary: r.boundary.clone(),
        }).collect();
        let actual = measure_shadow_features(width, height, &bgra, &state.indices, &regions,
            &gradient, &blur_gradient, float_bits(&calls[0]["previousEigenvalueBits"]) as f64);
        let canonical = measure_shadow_features(width, height, &bgra, &state.indices, &regions,
            &gradient, &blur_gradient, 0.0);
        assert_eq!(actual.features.len(), frame["stages"][4]["shadowFeatureBits"].as_array().unwrap().len());
        assert_eq!(actual.features.len(), calls.len());
        for (region, features) in actual.features.iter().enumerate() {
            let covariance: Vec<u8> = actual.covariances[region].iter().flat_map(|v| v.to_le_bytes()).collect();
            let expected: Vec<u8> = calls[region]["matrixBits"].as_array().unwrap().iter().flat_map(hex_bytes).collect();
            assert_eq!(covariance, expected, "{id} covariance {region}");
            let direction: Vec<u8> = actual.directions[region].iter().flat_map(|v| v.to_le_bytes()).collect();
            let expected: Vec<u8> = calls[region]["expectedBits"].as_array().unwrap()[1..].iter().flat_map(hex_bytes).collect();
            assert_eq!(direction, expected, "{id} principal direction {region}");
            let zero_direction: Vec<u8> = canonical.directions[region].iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(zero_direction, direction, "{id} observable zero-state direction {region}");
            let feature_bits: Vec<u8> = features.iter().flat_map(|v| v.to_le_bytes()).collect();
            let expected: Vec<u8> = frame["stages"][4]["shadowFeatureBits"][region].as_array().unwrap().iter().flat_map(hex_bytes).collect();
            assert_eq!(feature_bits, expected, "{id} shadow features {region}");
            let zero_features: Vec<u8> = canonical.features[region].iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(zero_features, feature_bits, "{id} observable zero-state features {region}");
            assert_eq!(classify_shadow_features(features), i(&frame["stages"][4]["shadowFlags"][region]) & 1 != 0);
            region_count += 1;
        }
        assert_eq!((actual.eigenvalue as f32).to_le_bytes().as_slice(),
            hex_bytes(&calls.last().unwrap()["expectedBits"][0]), "{id} retained principal eigenvalue");
        cases += 1;
    }
    assert_eq!((cases, region_count), (72, 2006));
}
