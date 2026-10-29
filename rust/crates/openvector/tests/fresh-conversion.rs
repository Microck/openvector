//! Fresh original-app conversions and exports, captured independently of the
//! existing corpus. Each source has independent tests so one mismatch cannot
//! prevent later inputs from being checked.
mod common;
mod editor_support;

use openvector::batch_queue::{BatchQueue, OverwriteDecision};
use openvector::batch_runner::BatchRunner;
use openvector::bitmap_export::{export_bitmap, BitmapExportInput, BitmapExportOptions, BitmapFormat};
use openvector::palette::CoreSession;
use openvector::settings_profile::{batch_profile_controls, default_batch_profile};
use serde_json::{json, Value};
use std::sync::OnceLock;

fn conversions() -> &'static [Value] {
    static ROWS: OnceLock<Vec<Value>> = OnceLock::new();
    ROWS.get_or_init(|| common::read_ndjson("fresh-conversion-20261020.ndjson.gz"))
}

fn exports() -> &'static [Value] {
    static ROWS: OnceLock<Vec<Value>> = OnceLock::new();
    ROWS.get_or_init(|| common::read_ndjson("fresh-conversion-20261020-exports.ndjson.gz"))
}

fn conversion(id: &str) -> &'static Value {
    let id = format!("fresh-conversion-20261020-{id}");
    conversions().iter().find(|row| row["id"] == id).unwrap()
}

fn export_reference(id: &str, workflow: &str) -> &'static Value {
    let id = format!("fresh-conversion-20261020-{id}-{workflow}");
    let reference = exports().iter().find(|row| row["id"] == id).unwrap();
    assert_eq!(reference["exportBytesRepeatIdentical"], true, "{id}");
    assert_eq!(reference["exports"][0], reference["exports"][1], "{id} repeated native files");
    for run in reference["runs"].as_array().unwrap() { assert_eq!(run["exitCode"], 0, "{id}"); }
    reference
}

fn compare_conversion(id: &str) {
    let reference = conversion(id);
    assert_eq!(reference["wholeResponseRepeatIdentical"], true, "{id}");
    let (_, automatic) = editor_support::automatic_document(&reference["input"]);
    let expected = &reference["expectedAutomatic"];
    editor_support::compare_settings(&automatic.settings,
        expected["selectedSettingsBits"].as_str().unwrap(),
        expected["selectedPaletteArgbBits"].as_str().unwrap());
    editor_support::compare_document_state(&automatic.state, &expected["state"], &conversions()[0]["parameters"]);
}

fn compare_bitmaps(id: &str) {
    let reference = export_reference(id, "bitmap");
    let (mut document, _) = editor_support::automatic_document(&reference["input"]);
    let source_alpha_type = document.source_alpha_type();
    let background = document.background_state();
    let hidden_regions: Vec<i64> = background.hidden_regions.iter().map(|&region| region as i64).collect();
    let state = document.state();
    let input = BitmapExportInput {
        geometry: state.fitting.as_ref().filter(|fitting| fitting.complete).map(|fitting| &fitting.geometry),
        width: state.original.width as i64, height: state.original.height as i64,
        source_alpha_type, background_mode: state.settings.background_mode,
        background_undo_count: background.undo.len(), hidden_regions: &hidden_regions,
    };
    for (filename, expected) in reference["exports"][0].as_object().unwrap() {
        let format = filename.rsplit('.').next().unwrap().parse().unwrap();
        let actual = export_bitmap(&input, &BitmapExportOptions { format, scale: 1.0, flatten: format != BitmapFormat::Png }).unwrap();
        assert_eq!(actual, common::b64(expected["bytes"].as_str().unwrap()), "{id}/{filename}");
    }
}

fn compare_batch_svg(id: &str) {
    let reference = export_reference(id, "svg-batch");
    let directory = tempfile::tempdir().unwrap();
    let directory_name = directory.path().to_str().unwrap();
    let filename = directory.path().join(reference["input"]["name"].as_str().unwrap());
    std::fs::write(&filename, common::b64(reference["input"]["bytes"].as_str().unwrap())).unwrap();
    let mut queue = BatchQueue::new(directory_name, directory_name);
    queue.add_files(&[filename.to_str().unwrap().to_owned()]);
    queue.export_directory = directory_name.to_owned();
    queue.export_format = "svg".to_owned();
    let mut runner = BatchRunner::new(queue, CoreSession::default());
    let result = runner.run(&batch_profile_controls(&default_batch_profile()), &json!({}),
        &mut |_: &[String]| OverwriteDecision::Overwrite, None).unwrap();
    assert_eq!(result.failed, 0, "{id}: {result:?}");
    assert_eq!(result.converted, 1, "{id}: {result:?}");
    for (filename, expected) in reference["exports"][0].as_object().unwrap() {
        let actual = std::fs::read(directory.path().join(filename)).unwrap();
        assert_eq!(actual, common::b64(expected["bytes"].as_str().unwrap()), "{id}/{filename}");
    }
}

macro_rules! fresh_case {
    ($name:ident, $id:literal) => {
        mod $name {
            #[test] fn automatic() { super::compare_conversion($id); }
            #[test] fn bitmap() { super::compare_bitmaps($id); }
            #[test] fn batch_svg() { super::compare_batch_svg($id); }
        }
    };
}

fresh_case!(logo_5x7, "00-logo-5x7");
fresh_case!(diagonal_7x5, "01-diagonal-7x5");
fresh_case!(ring_11x13, "02-ring-11x13");
fresh_case!(two_holes_17x19, "03-two-holes-17x19");
fresh_case!(curve_23x17, "04-curve-23x17");
fresh_case!(logo_29x23, "05-logo-29x23");
fresh_case!(checker_31x29, "06-checker-31x29");
fresh_case!(islands_37x31, "07-islands-37x31");
fresh_case!(transparent_19x23, "08-transparent-19x23");
fresh_case!(ring_41x37, "10-ring-41x37");
fresh_case!(curve_43x31, "11-curve-43x31");
fresh_case!(diagonal_13x3, "12-diagonal-13x3");
fresh_case!(logo_3x17, "13-logo-3x17");
fresh_case!(two_holes_47x43, "14-two-holes-47x43");
fresh_case!(transparent_31x37, "15-transparent-31x37");

#[test]
fn gradient_23x29_retains_native_empty_peak_failure_boundary() {
    let reference = conversion("09-gradient-23x29");
    assert_eq!(reference["nativeFailure"]["exitCodes"], json!([11, 11]));
    let input = &reference["input"];
    let decoded = openvector::image_input::decode_image(&common::b64(input["bytes"].as_str().unwrap())).unwrap();
    let mut document = CoreSession::default().create_document();
    document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, 0.0).unwrap();
    let mut controls = editor_support::clean_startup_profile();
    controls["extension"] = json!("png");
    let error = document.run_automatic(&controls).expect_err("Original fails on an empty peak-weight array; no conversion result exists.");
    assert!(error.contains("no histogram peaks"), "{error}");
    assert!(document.state().fitting.is_none());
    assert!(!document.snapshot().cancelled);
}
