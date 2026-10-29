//! Port of `test/image-resize.test.mjs`.
mod common;
use common::{b64, read_ndjson};
use openvector::image_preparation::prepare_image_pixels;
use openvector::image_resize::{fit_resize_geometry, reduce_working_image, resize_prepared_image, shrink_source_image, OwnedImage, ResizeRequest};
use serde_json::Value;

fn int(frame: &Value, key: &str) -> i64 {
    frame[key].as_i64().unwrap_or_else(|| panic!("{key}"))
}

fn dims(value: &Value) -> Vec<i64> {
    value.as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()
}

#[test]
fn q8_sampling_and_triangle_resizing_match_native_bytes() {
    let mut expected_cases = 0;
    let mut cases = 0;
    for frame in read_ndjson("image-resize-kernel-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            assert_eq!(frame["originalInstructionsPatched"], false);
            expected_cases = int(&frame, "cases");
            continue;
        }
        let id = frame["id"].as_str().unwrap();
        let bgra = b64(frame["bgra"].as_str().unwrap());
        let before = bgra.clone();
        let resized = resize_prepared_image(&ResizeRequest {
            width: int(&frame, "width") as usize, height: int(&frame, "height") as usize, bgra: &bgra,
            target_width: int(&frame, "targetWidth") as usize, target_height: int(&frame, "targetHeight") as usize,
            filtered: int(&frame, "filtered") != 0, matte: int(&frame, "matte") != 0,
        });
        assert_eq!(vec![resized.width as i64, resized.height as i64], dims(&frame["expectedDimensions"]), "{id}");
        assert_eq!(resized.bgra, b64(frame["expected"].as_str().unwrap()), "{id}");
        assert_eq!(bgra, before, "{id}");
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}

#[test]
fn native_geometry_preserves_asymmetric_float32_height_ratio() {
    let mut expected_cases = 0;
    let mut cases = 0;
    for frame in read_ndjson("image-resize-geometry-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            expected_cases = int(&frame, "cases");
            continue;
        }
        let actual = fit_resize_geometry(int(&frame, "width"), int(&frame, "height"),
            frame["geometryWidth"].as_i64().unwrap_or(0), frame["geometryHeight"].as_i64().unwrap_or(0));
        assert_eq!(actual.to_vec(), dims(&frame["expected"]), "{frame}");
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}

#[test]
fn complete_large_image_preparation_matches_native_working_and_cache_pixels() {
    let mut expected_cases = 0;
    let mut cases = 0;
    for frame in read_ndjson("image-resize-working-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponseRepeatIdentical"], true);
            expected_cases = int(&frame, "cases");
            continue;
        }
        let id = frame["id"].as_str().unwrap();
        let bgra = b64(frame["bgra"].as_str().unwrap());
        let before = bgra.clone();
        let prepared = prepare_image_pixels(int(&frame, "width") as usize, int(&frame, "height") as usize, &bgra,
            int(&frame, "mode") as u32, int(&frame, "amount") as u32).unwrap();
        assert_eq!(vec![prepared.width as i64, prepared.height as i64], dims(&frame["preparedDimensions"]), "{id}");
        assert_eq!(prepared.bgra, b64(frame["preparedBgra"].as_str().unwrap()), "{id}");
        let retained = prepared.bgra.clone();
        // The decoded BGRA adapter requests matte for every populated source.
        if prepared.width != 0 { assert_eq!(int(&frame, "matte"), 1, "{id}"); }
        let working = reduce_working_image(prepared.width, prepared.height, &prepared.bgra, int(&frame, "antiAlias") != 0, true);
        assert_eq!(vec![working.width as i64, working.height as i64], dims(&frame["expectedDimensions"]), "{id}");
        assert_eq!(working.bgra, b64(frame["expected"].as_str().unwrap()), "{id}");
        assert_eq!(bgra, before, "{id}");
        assert_eq!(prepared.bgra, retained, "{id}");
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}

#[test]
fn source_shrink_matches_native_opaque_artwork() {
    let frames = read_ndjson("source-shrink-reference.ndjson.gz");
    let frame = frames.iter().find(|frame| frame["id"] == "opaque-artwork-3600x2400-divisor3").unwrap();
    compare_source_shrink(frame, true);
}

fn compare_fresh_source_shrink(id: &str) {
    static ROWS: std::sync::LazyLock<Vec<Value>> = std::sync::LazyLock::new(|| {
        let mut rows = read_ndjson("source-shrink-fresh-20261021.ndjson.gz");
        rows.extend(read_ndjson("source-shrink-partial-three-20261021.ndjson.gz"));
        rows
    });
    let frame = ROWS.iter().find(|frame| frame["id"] == id).unwrap();
    compare_source_shrink(frame, frame["reduced"].as_bool().unwrap());
}

fn compare_source_shrink(frame: &Value, expected_reduction: bool) {
    let id = frame["id"].as_str().unwrap();
    let source = OwnedImage {
        width: int(frame, "width") as usize,
        height: int(frame, "height") as usize,
        bgra: b64(frame["bgra"].as_str().unwrap()),
    };
    let reduced = shrink_source_image(&source);
    assert_eq!(reduced.is_some(), expected_reduction, "{id}");
    let output = reduced.as_ref().unwrap_or(&source);
    assert_eq!(vec![output.width as i64, output.height as i64], dims(&frame["expectedDimensions"]), "{id}");
    let expected = b64(frame["expected"].as_str().unwrap());
    assert_eq!(output.bgra.len(), expected.len(), "{id}");
    let mismatch = output.bgra.iter().zip(&expected).position(|(actual, expected)| actual != expected);
    assert_eq!(mismatch, None, "{id}: first differing native source byte");
}

#[test] fn source_shrink_below_threshold() { compare_fresh_source_shrink("default-1599x1000"); }
#[test] fn source_shrink_at_threshold() { compare_fresh_source_shrink("default-1600x1000"); }
#[test] fn source_shrink_clipped_alpha_blocks() { compare_fresh_source_shrink("default-1601x1001"); }
#[test] fn source_shrink_clipped_alpha_blocks_alternate() { compare_fresh_source_shrink("default-1603x1003"); }
#[test] fn source_shrink_divisor_two_upper_boundary() { compare_fresh_source_shrink("default-3200x2000"); }
#[test] fn source_shrink_divisor_three_alpha() { compare_fresh_source_shrink("default-3201x2001"); }
#[test] fn source_shrink_divisor_three_single_pixel_remainder() { compare_fresh_source_shrink("default-3202x2002"); }
#[test] fn source_shrink_divisor_three_two_pixel_remainder() { compare_fresh_source_shrink("default-3203x2003"); }

/// The audit fixture: alpha 0 on even x and 255 on odd x (binary, kind 1).
/// Native recommended sizing reduces it to 801x501 with every alpha 128.
fn binary_alpha_document(session: &openvector::palette::CoreSession) -> openvector::image_document::ImageDocument {
    let mut document = openvector::image_document::ImageDocument::new(session.clone());
    let (width, height) = (1602, 1002);
    let mut pixels = vec![0; width * height * 4];
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        pixel[3] = if index % width % 2 == 0 { 0 } else { 255 };
    }
    document.set_source(width, height, &pixels, true, 17.0).unwrap();
    document
}

#[test]
fn source_shrink_keeps_load_time_alpha_classification_and_preserves_controls() {
    use openvector::{document_controls::DocumentSettings, palette::CoreSession};
    let session = CoreSession::default();
    let mut document = binary_alpha_document(&session);
    let settings = DocumentSettings { smoothness: 8, ..Default::default() };
    document.commit_settings(&Value::Object(settings.to_json())).unwrap();
    let random_state = session.random_state();
    // No query before the shrink: the classification must still describe the
    // loaded raster, as native fills it at load and shrinking never clears it.
    assert!(document.shrink_source());
    assert_eq!([document.state().original.width, document.state().original.height], [801, 501]);
    assert!(document.state().original.bgra.chunks_exact(4).all(|pixel| pixel[3] == 128));
    assert_eq!(document.source_alpha_type(), 1);
    assert_eq!(document.state().settings.to_json(), settings.to_json());
    assert_eq!(document.state().parameters["shadowControl"], 17);
    assert_eq!(session.random_state(), random_state);
    assert!(!document.shrink_source());
    assert_eq!(document.source_alpha_type(), 1);
}

/// Native reference: evidence/native-alpha-cache-20261021/observations.json.
/// Basic, category 1, recommended sizing: page-6 entry on the 801x501 partial-alpha
/// raster recommends flatten mode 1 with opaque white, as for the unreduced source.
#[test]
fn shrunk_binary_alpha_category_one_recommends_opaque_white_background() {
    let session = openvector::palette::CoreSession::default();
    let mut document = binary_alpha_document(&session);
    assert!(document.shrink_source());
    let result = document.run_batch_image(&serde_json::json!({
        "extension": ".png", "category": 1, "quality": 1, "paletteMode": 2, "backgroundMode": 3,
    })).unwrap();
    assert_eq!((result.source_alpha_type, result.recommended_background_mode), (1, 1));
    assert_eq!((result.settings.background_mode, result.settings.background), (1, [1.0, 1.0, 1.0, 1.0]));
}

fn fitted_edited_document(width: usize, height: usize) -> openvector::image_document::ImageDocument {
    let mut pixels = vec![255; width * height * 4];
    for y in height / 4..height * 3 / 4 {
        for x in width / 4..width * 3 / 4 {
            pixels[(y * width + x) * 4..(y * width + x + 1) * 4].copy_from_slice(&[30, 40, 220, 255]);
        }
    }
    let mut document = openvector::palette::CoreSession::default().create_document();
    document.set_source(width, height, &pixels, false, 0.0).unwrap();
    let automatic = document.run_automatic(&serde_json::json!({"extension": ".png"})).unwrap();
    document.begin_editing().unwrap();
    document.edit(&serde_json::json!({
        "tool": "pencil", "from": [width / 2, height / 2], "to": [width / 2 + 1, height / 2],
        "color": [1, 1, 1, 1], "commit": true
    })).unwrap();
    document.finish_editing(&Value::Object(automatic.settings.to_json()), None).unwrap();
    assert!(document.has_committed_edits());
    assert!(document.state().fitting.as_ref().unwrap().complete);
    document
}

#[test]
fn source_shrink_invalidates_processed_geometry_and_editing_history() {
    let mut document = fitted_edited_document(1603, 1003);
    let controls = document.state().settings.to_json();
    assert!(document.shrink_source());
    let state = document.state();
    assert_eq!([state.original.width, state.original.height], [802, 502]);
    assert_eq!(state.working, OwnedImage::default());
    assert!(state.segmentation.is_none());
    assert!(state.contours.is_none());
    assert!(state.smoothing.is_none());
    assert!(state.fitting.is_none());
    assert_eq!(state.settings.to_json(), controls);
    assert!(document.editing_state().is_none());
    assert!(!document.has_committed_edits());
    assert!(!document.has_applied_edits());
}

#[test]
fn source_shrink_noop_preserves_processed_geometry_and_editing_history() {
    let mut document = fitted_edited_document(15, 13);
    let before = document.snapshot().to_json();
    let editing = serde_json::to_value(document.editing_state()).unwrap();
    assert!(!document.shrink_source());
    assert_eq!(document.snapshot().to_json(), before);
    assert_eq!(serde_json::to_value(document.editing_state()).unwrap(), editing);
    assert!(document.has_committed_edits());
}
