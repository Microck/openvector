//! Encoded file -> shared document -> complete native automatic conversion.
//! Every fixture's expected settings, pixels, parameters, segmentation,
//! contours, smoothing and fitted geometry are checked without tolerance.
mod common;
#[path = "editor_support/mod.rs"]
mod editor_support;
use common::read_ndjson;
use editor_support::{automatic_document,compare_document_state,compare_settings};

fn conversions(corpus: &str) {
    let rows = read_ndjson(&format!("{corpus}-conversion-reference.ndjson.gz"));
    assert_eq!(rows[0]["wholeFreshProcessResponsesRepeatIdentical"],true);
    assert!(rows.len() > 1,"{corpus} conversion corpus is empty");
    for reference in &rows[1..] {
        let id = reference["id"].as_str().unwrap();
        let (_document,automatic) = automatic_document(&reference["input"]);
        let expected = &reference["expectedAutomatic"];
        compare_settings(&automatic.settings,expected["selectedSettingsBits"].as_str().unwrap(),expected["selectedPaletteArgbBits"].as_str().unwrap());
        compare_document_state(&automatic.state,&expected["state"],&rows[0]["parameters"]);
        assert_eq!(automatic.state.progress,100,"{id} completion progress");
    }
}
#[test] fn native_bmp_conversion() { conversions("bmp-expanded"); }
#[test] fn native_gif_conversion() { conversions("gif"); }
#[test] fn native_jpeg_conversion() { conversions("jpeg"); }
#[test] fn native_tiff_conversion() { conversions("tiff"); }
#[test] fn native_tiff_jpeg_conversion() { conversions("tiff-jpeg"); }
#[test] fn native_jpeg_alpha_conversion() { conversions("jpeg-alpha"); }
#[test] fn native_tiff_legacy_conversion() { conversions("tiff-legacy"); }
#[test] fn native_tiff_predictor_conversion() { conversions("tiff-predictor"); }
