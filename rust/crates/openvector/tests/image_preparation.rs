//! Native preparation kernels, failure outcomes and document source composition.
mod common;
use common::{b64, read_json, read_ndjson};
use openvector::image_classification::blur_classifier_pixels;
use openvector::image_preparation::{composite_source_image, despeckle_prepared_image, enhance_prepared_image,
    filter_prepared_image, prepare_enhanced_image, prepare_image_pixels, reduce_prepared_noise};
use openvector::image_resize::reduce_working_image;
use serde_json::Value;

fn bytes(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()
}

fn ints(value: &Value) -> Vec<i64> {
    value.as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()
}

fn int(frame: &Value, key: &str) -> i64 {
    frame[key].as_i64().unwrap_or_else(|| panic!("{key}"))
}

fn frames(name: &str) -> Vec<Value> {
    let frames = read_ndjson(name);
    assert_eq!(frames[0]["wholeNativeResponseRepeatIdentical"], true);
    assert_eq!((frames.len() - 1) as i64, int(&frames[0], "cases"));
    frames
}

fn enhance(scope: &str) {
    for frame in &frames(&format!("image-enhance-{scope}-reference.ndjson.gz"))[1..] {
        let id = frame["id"].as_str().unwrap();
        let bgra = bytes(&frame["bgra"]);
        let before = bgra.clone();
        let (width, height) = (int(frame, "width") as usize, int(frame, "height") as usize);
        let prepared = if scope == "kernel" { enhance_prepared_image(width, height, &bgra) }
            else { prepare_enhanced_image(width, height, &bgra, int(frame, "amount") as u32) };
        assert_eq!(vec![prepared.width as i64, prepared.height as i64], ints(&frame["expectedDimensions"]), "{id}");
        assert_eq!(prepared.bgra, bytes(&frame["expected"]), "{id}");
        assert_eq!(bgra, before, "{id}");
        if !frame["blurredBgra"].is_null() {
            assert_eq!(blur_classifier_pixels(prepared.width, prepared.height, &prepared.bgra), bytes(&frame["blurredBgra"]), "{id}");
        }
    }
}

#[test]
fn native_enhance_image_kernel() { enhance("kernel"); }

#[test]
fn native_enhance_image_preparation() { enhance("preparation"); }

fn filters(scope: &str) {
    for frame in &frames(&format!("image-filters-{scope}-reference.ndjson.gz"))[1..] {
        let id = frame["id"].as_str().unwrap();
        let bgra = bytes(&frame["bgra"]);
        let before = bgra.clone();
        let (width, height) = (int(frame, "width") as usize, int(frame, "height") as usize);
        let (mode, amount) = (int(frame, "mode") as u32, int(frame, "amount") as u32);
        let prepared = if scope == "preparation" { prepare_image_pixels(width, height, &bgra, mode, amount).unwrap() }
            else if mode == 2 { reduce_prepared_noise(width, height, &bgra, amount).unwrap() }
            else { despeckle_prepared_image(width, height, &bgra) };
        assert_eq!(vec![prepared.width as i64, prepared.height as i64], ints(&frame["expectedDimensions"]), "{id}");
        assert_eq!(prepared.bgra, bytes(&frame["expected"]), "{id}");
        assert_eq!(bgra, before, "{id}");
        if !frame["blurredBgra"].is_null() {
            assert_eq!(blur_classifier_pixels(prepared.width, prepared.height, &prepared.bgra), bytes(&frame["blurredBgra"]), "{id}");
        }
    }
}

#[test]
fn native_median_despeckle_kernel() { filters("kernel"); }

#[test]
fn native_median_despeckle_preparation() { filters("preparation"); }

#[test]
fn median_size_preconditions_preserve_every_native_error_reason() {
    let reference = read_json("image-filters-errors-reference.json");
    assert_eq!(reference["stableErrorTypeMessageStageRepeatIdentical"], true);
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 176);
    for frame in cases {
        let id = frame["id"].as_str().unwrap();
        let bgra = bytes(&frame["bgra"]);
        let before = bgra.clone();
        assert_eq!(frame["descriptor"], ".?AVErrorOption@Magick@@");
        assert_eq!(frame["reason"], "ImageSmallerThanKernelRadius");
        let (width, height) = (int(frame, "width") as usize, int(frame, "height") as usize);
        let (mode, amount) = (int(frame, "mode") as u32, int(frame, "amount") as u32);
        let result = if int(frame, "operation") == 85 { reduce_prepared_noise(width, height, &bgra, amount) }
            else { prepare_image_pixels(width, height, &bgra, mode, amount) };
        assert_eq!(result.unwrap_err(), frame["reason"].as_str().unwrap(), "{id}");
        assert_eq!(bgra, before, "{id}");
    }
}

#[test]
fn document_background_composition_and_filtering_match_native_source_caches() {
    let (mut cases, mut actions) = (0, 0);
    for frame in read_ndjson("document-source-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            continue;
        }
        let (width, height) = (int(&frame, "width") as usize, int(&frame, "height") as usize);
        let input = b64(frame["bgra"].as_str().unwrap());
        let original = prepare_image_pixels(width, height, &input, 0, 0).unwrap();
        assert_eq!(original.bgra, b64(frame["originalBgra"].as_str().unwrap()));
        let original_before = original.clone();
        let edits = frame["edits"].as_array().unwrap();
        let states = frame["states"].as_array().unwrap();
        assert_eq!(edits.len(), states.len());
        for (edit, expected) in edits.iter().zip(states) {
            let label = format!("{} {}", frame["id"], edit["id"]);
            let background_mode = edit["values"][7].as_i64().unwrap();
            let color: [f64; 4] = std::array::from_fn(|i| edit["background"][i].as_f64().unwrap());
            let composed = composite_source_image(width, height, &original.bgra,
                background_mode == 1 || background_mode == 2, &color);
            // Use recorded dispatch controls: this test isolates source preparation
            // from the separately tested settings resolver and document lifecycle.
            let mode = expected["effectivePreprocessing"][0].as_u64().unwrap() as u32;
            let amount = expected["effectivePreprocessing"][1].as_u64().unwrap() as u32;
            let source = filter_prepared_image(composed, mode, amount).unwrap();
            assert_eq!(vec![source.width as i64, source.height as i64], ints(&expected["sourceDimensions"]), "{label}");
            assert_eq!(source.bgra, b64(expected["sourceBgra"].as_str().unwrap()), "{label}");
            let shared = expected["sharedBits"].as_str().unwrap();
            let anti_alias = u32::from_str_radix(&shared[8..16], 16).unwrap() != 0;
            let working = reduce_working_image(source.width, source.height, &source.bgra, anti_alias, true);
            assert_eq!(vec![working.width as i64, working.height as i64], ints(&expected["workingDimensions"]), "{label}");
            assert_eq!(working.bgra, b64(expected["workingBgra"].as_str().unwrap()), "{label}");
            assert_eq!(original, original_before, "{label} retained original");
            actions += 1;
        }
        assert_eq!(input, b64(frame["bgra"].as_str().unwrap()));
        cases += 1;
    }
    assert_eq!((cases, actions), (10, 160));
}
