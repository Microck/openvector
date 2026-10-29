//! Fresh larger original artwork and original single-image SVG saves.
//! This is deliberately not the batch workflow: automatic single-image alpha
//! stays intact. Every input has separate computation and file-export tests.
mod common;
mod editor_support;

use openvector::export_options::ExportOptions;
use openvector::vector_file::{write_vector_file, VectorFileInput};
use openvector::vector_output::VectorOutputInput;
use serde_json::Value;
use std::sync::LazyLock;

fn references() -> &'static [Value] {
    static ROWS: LazyLock<Vec<Value>> = LazyLock::new(|| common::read_ndjson("fresh-representative-20261020.ndjson.gz"));
    &ROWS
}

fn reference(id: &str) -> &'static Value {
    let id = format!("fresh-representative-20261020-{id}");
    let row = references().iter().find(|row| row["id"] == id).unwrap();
    assert_eq!(row["wholeResponseRepeatIdentical"], true, "{id}: independent native conversion processes");
    for run in row["runs"].as_array().unwrap() {
        assert_eq!(run["exitCode"], 0, "{id}: retained original failure: {run}");
    }
    assert!(row.get("nativeFailure").is_none(), "{id}: original conversion/save failed");
    row
}

fn compare_automatic(id: &str) {
    let row = reference(id);
    let (_, automatic) = editor_support::automatic_document(&row["input"]);
    let expected = &row["expectedAutomatic"];
    editor_support::compare_settings(
        &automatic.settings,
        expected["selectedSettingsBits"].as_str().unwrap(),
        expected["selectedPaletteArgbBits"].as_str().unwrap(),
    );
    editor_support::compare_document_state(
        &automatic.state, &expected["state"], &references()[0]["parameters"],
    );
}

fn compare_single_image_svg(id: &str) {
    let row = reference(id);
    assert_eq!(row["svgBytesRepeatIdentical"], true, "{id}: independent native SVG saves");
    assert_eq!(row["singleImageSvg"][0], row["singleImageSvg"][1], "{id}: repeated SVG bytes");
    let (mut document, automatic) = editor_support::automatic_document(&row["input"]);
    let state = &automatic.state;
    let fitting = state.fitting.as_ref().unwrap();
    assert!(fitting.complete, "{id}: fitting must finish before save");
    let mut vector = VectorOutputInput::new(
        &fitting.geometry, state.original.width as i64, state.original.height as i64,
    );
    // The native observer preserves the original document's default export
    // options, rather than imposing batch settings or flattening the source.
    let options = ExportOptions::default();
    vector.layout = options.layout;
    vector.thin_stroke = options.thin_stroke;
    let hidden: Vec<i64> = document.background_state().hidden_regions.iter().map(|&region| region as i64).collect();
    vector.hidden_regions = &hidden;
    let input = VectorFileInput::new(vector);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("result.svg");
    let saved = document.run_export(|progress| {
        write_vector_file(&path, &input, &mut |percent| progress(percent).map_err(openvector::vector_output::VectorOutputError))
            .map(|result| result.status as u32)
            .map_err(|error| error.0)
    }).unwrap();
    let actual = std::fs::read(path).unwrap();
    let expected = common::b64(row["singleImageSvg"][0]["bytes"].as_str().unwrap());
    assert!(!expected.is_empty(), "{id}: native output must exist");
    assert_eq!(saved.status, 0, "{id}: single-image save status");
    let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none() && actual.len() == expected.len(),
        "{id}: first SVG mismatch {mismatch:?}, Rust {} bytes, original {} bytes", actual.len(), expected.len());
}

macro_rules! representative_case {
    ($name:ident, $id:literal) => {
        mod $name {
            #[test]
            fn automatic() { super::compare_automatic($id); }
            #[test]
            fn single_image_svg() { super::compare_single_image_svg($id); }
        }
    };
}

representative_case!(existing_alpha_19x23, "existing-08-transparent-19x23");
representative_case!(existing_alpha_31x37, "existing-15-transparent-31x37");
representative_case!(antialiased_logo, "logo-aa-256x192");
representative_case!(antialiased_text, "text-aa-256x160");
representative_case!(thin_holes_and_line_art, "thin-holes-lines-192x128");
representative_case!(jpeg_logo, "logo-jpeg-256x192");
representative_case!(palette_rich_artwork, "palette-rich-192x128");
representative_case!(large_alpha_rings, "alpha-rings-192x128");
