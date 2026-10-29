//! Genuine USGS public-domain photographs, captured twice in fresh original processes.
//! Compare complete automatic computation and exact original single-image SVG bytes.
mod common;
mod editor_support;

use openvector::export_options::ExportOptions;
use openvector::vector_file::{VectorFileInput, write_vector_file};
use openvector::vector_output::VectorOutputInput;
use serde_json::{Value, json};
use std::sync::LazyLock;

fn references() -> &'static [Value] {
    static ROWS: LazyLock<Vec<Value>> = LazyLock::new(|| {
        common::read_ndjson("fresh-photographic-20261020.ndjson.gz")
    });
    &ROWS
}

fn reference(slug: &str) -> &'static Value {
    let id = format!("fresh-photographic-20261020-{slug}");
    let row = references().iter().find(|row| row["id"] == id).unwrap();
    let runs = row["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{id}: two independent native attempts");
    assert_ne!(runs[0]["pid"], runs[1]["pid"], "{id}: fresh native processes");
    for run in runs {
        assert_eq!(run["exitCode"], 0, "{id}: retained native failure: {run}");
        assert!(run.get("decodeFailure").is_none(), "{id}: incomplete native state: {run}");
    }
    assert!(row.get("nativeFailure").is_none(), "{id}: original conversion/save failed");
    assert_eq!(row["wholeResponseRepeatIdentical"], true, "{id}: independent native responses");
    row
}

fn compare_automatic(slug: &str) {
    let row = reference(slug);
    let (_, automatic) = editor_support::automatic_document(&row["input"]);
    let expected = &row["expectedAutomatic"];
    let classification = &automatic.state.classification;
    assert_eq!(
        json!([classification.category, classification.quality, classification.palette_mode, classification.color_count]),
        expected["documentClassification"],
        "{slug}: automatic classification"
    );
    assert_eq!(
        json!([classification.category, classification.quality, classification.palette_mode]),
        expected["recommendedControls"],
        "{slug}: recommended controls"
    );
    editor_support::compare_settings(
        &automatic.settings,
        expected["selectedSettingsBits"].as_str().unwrap(),
        expected["selectedPaletteArgbBits"].as_str().unwrap(),
    );
    let selected = editor_support::hex(expected["selectedSettingsBits"].as_str().unwrap());
    let selected_i32 = |offset| i32::from_le_bytes(selected[offset..offset + 4].try_into().unwrap());
    assert_eq!(automatic.palette_count.unwrap_or(0) as i32, selected_i32(56), "{slug}: selected palette count");
    assert_eq!(automatic.recommended_background_mode, selected_i32(88), "{slug}: background recommendation");
    // Includes pixels, settings, all 100 typed parameters, candidate palettes and
    // scores, segmentation, contours, smoothing, fitted geometry and output chains.
    editor_support::compare_document_state(
        &automatic.state,
        &expected["state"],
        &references()[0]["parameters"],
    );
}

fn compare_single_image_svg(slug: &str) {
    let row = reference(slug);
    assert_eq!(row["svgBytesRepeatIdentical"], true, "{slug}: independent native SVG saves");
    assert_eq!(row["singleImageSvg"][0], row["singleImageSvg"][1], "{slug}: repeated SVG bytes");
    let (mut document, automatic) = editor_support::automatic_document(&row["input"]);
    let state = &automatic.state;
    let fitting = state.fitting.as_ref().unwrap();
    assert!(fitting.complete, "{slug}: fitting must finish before save");
    let mut vector = VectorOutputInput::new(
        &fitting.geometry, state.original.width as i64, state.original.height as i64,
    );
    // Match original Document::save defaults, not the separate batch workflow.
    let options = ExportOptions::default();
    vector.layout = options.layout;
    vector.thin_stroke = options.thin_stroke;
    let hidden: Vec<i64> = document.background_state().hidden_regions.iter()
        .map(|&region| region as i64).collect();
    vector.hidden_regions = &hidden;
    let input = VectorFileInput::new(vector);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.svg");
    let saved = document.run_export(|progress| {
        write_vector_file(&path, &input, &mut |percent| {
            progress(percent).map_err(openvector::vector_output::VectorOutputError)
        })
            .map(|result| result.status as u32)
            .map_err(|error| error.0)
    }).unwrap();
    let actual = std::fs::read(path).unwrap();
    let expected = common::b64(row["singleImageSvg"][0]["bytes"].as_str().unwrap());
    assert!(!expected.is_empty(), "{slug}: native output must exist");
    assert_eq!(saved.status, 0, "{slug}: single-image save status");
    let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none() && actual.len() == expected.len(),
        "{slug}: first SVG mismatch {mismatch:?}, Rust {} bytes, original {} bytes",
        actual.len(), expected.len());
}

macro_rules! photographic_case {
    ($name:ident, $slug:literal) => {
        mod $name {
            #[test]
            fn automatic() { super::compare_automatic($slug); }
            #[test]
            fn single_image_svg() { super::compare_single_image_svg($slug); }
        }
    };
}

photographic_case!(arctic_fox_texture, "arctic-fox");
photographic_case!(mojave_desert_landscape_jpeg, "mojave-desert");
photographic_case!(jim_devine_portrait, "jim-devine");
