mod common;
use openvector::bitmap_codecs::{encode_bmp, encode_jpeg, encode_png, encode_tiff};
use openvector::bitmap_export::{export_bitmap, BitmapExportError, BitmapExportInput, BitmapExportOptions, BitmapFormat, BITMAP_FORMATS};
use openvector::contour_fitting::{FitRegion, FittedGeometry};
use openvector::fitting::FitNode;
use openvector::image_document::ImageDocument;
use openvector::image_input::{decode_image, resolve_image_filename};
use openvector::palette::CoreSession;
use serde_json::{json, Value};

fn rectangle(color: [u8; 4]) -> FittedGeometry {
    FittedGeometry {
        cubic_storage_slots: 0,
        cubics: Vec::new(),
        nodes: [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]].map(|[x, y]| FitNode { x, y, kind: 0, cubic: -1 }).to_vec(),
        regions: vec![FitRegion {
            bgra: color, pixel_count: 16, color_index: 0, parent: -1,
            contour: vec![0, 1, 2, 3], boundary: Vec::new(),
            edges: (0..4).map(|i| [i, 0, -1, i, (i + 1) % 4, i, (i + 1) % 4]).collect(),
        }],
    }
}

fn input(geometry: &FittedGeometry) -> BitmapExportInput<'_> {
    BitmapExportInput { geometry: Some(geometry), width: 4, height: 4, source_alpha_type: 0,
        background_mode: 0, background_undo_count: 0, hidden_regions: &[] }
}

fn expected_bytes(format: BitmapFormat, pixel: [u8; 4], has_alpha: bool, size: i64) -> Vec<u8> {
    let pixels = pixel.repeat((size * size) as usize);
    match format {
        BitmapFormat::Bmp => encode_bmp(size, size, &pixels),
        BitmapFormat::Jpeg => encode_jpeg(size, size, &pixels),
        BitmapFormat::Png => encode_png(size, size, &pixels, has_alpha),
        BitmapFormat::Tiff => encode_tiff(size, size, &pixels),
    }.unwrap()
}

#[test]
fn bitmap_format_names_and_aliases_match_the_public_reference() {
    assert_eq!(BITMAP_FORMATS, ["bmp", "jpg", "jpeg", "png", "tif", "tiff"]);
    for (name, format) in [("bmp", BitmapFormat::Bmp), ("jpg", BitmapFormat::Jpeg), ("jpeg", BitmapFormat::Jpeg),
        ("png", BitmapFormat::Png), ("tif", BitmapFormat::Tiff), ("tiff", BitmapFormat::Tiff)] {
        assert_eq!(name.parse::<BitmapFormat>().unwrap(), format);
    }
    for name in ["svg", "PNG", "", ".png"] {
        assert_eq!(name.parse::<BitmapFormat>().unwrap_err().to_string(), "Choose a bitmap format: BMP, JPEG, PNG or TIFF.");
    }
}

#[test]
fn flatten_consent_and_validation_order_preserve_document_state() {
    let geometry = rectangle([20, 80, 160, 128]);
    let before = geometry.clone();
    let input = BitmapExportInput { source_alpha_type: 1, ..input(&geometry) };
    for format in [BitmapFormat::Bmp, BitmapFormat::Jpeg, BitmapFormat::Tiff] {
        let mut options = BitmapExportOptions::new(format);
        let error = export_bitmap(&input, &options).unwrap_err();
        assert_eq!(error, BitmapExportError::FlattenRequired);
        assert_eq!(error.code(), Some("BITMAP_FLATTEN_REQUIRED"));
        options.flatten = true;
        assert_eq!(export_bitmap(&input, &options).unwrap(), expected_bytes(format, [137, 167, 207, 255], false, 4));
    }
    let png = BitmapExportOptions::new(BitmapFormat::Png);
    assert!(export_bitmap(&input, &BitmapExportOptions { flatten: true, ..png }).unwrap_err().to_string().contains("only to non-PNG"));
    assert!(export_bitmap(&BitmapExportInput { geometry: None, ..input }, &png).unwrap_err().to_string().contains("Finish vectorizing"));
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(export_bitmap(&input, &BitmapExportOptions { scale, ..png }).unwrap_err().to_string().contains("positive finite"));
    }
    assert_eq!(geometry, before);
}

#[test]
fn format_storage_policy_uses_source_alpha_background_mode_and_undo_history() {
    let geometry = rectangle([20, 80, 160, 128]);
    let before = geometry.clone();
    for format in [BitmapFormat::Bmp, BitmapFormat::Jpeg, BitmapFormat::Png, BitmapFormat::Tiff] {
        for source_alpha_type in [0, 1, 2] {
            for background_mode in [0, 1, 2, 3] {
                for background_undo_count in [0, 1] {
                    let input = BitmapExportInput { source_alpha_type, background_mode, background_undo_count, ..input(&geometry) };
                    let has_alpha = background_undo_count != 0 || (format == BitmapFormat::Png && source_alpha_type != 0 && ![1, 2].contains(&background_mode));
                    let pixel = if !has_alpha { [137, 167, 207, 255] }
                        else if matches!(format, BitmapFormat::Png | BitmapFormat::Tiff) { [19, 79, 159, 128] }
                        else { [10, 40, 80, 128] };
                    let options = BitmapExportOptions { format, scale: 1.0, flatten: format != BitmapFormat::Png };
                    assert_eq!(export_bitmap(&input, &options).unwrap(), expected_bytes(format, pixel, has_alpha, 4),
                        "{format:?} alpha={source_alpha_type} mode={background_mode} undo={background_undo_count}");
                }
            }
        }
    }
    assert_eq!(geometry, before);
}

#[test]
fn hidden_regions_do_not_replace_undo_history_and_scaling_rounds_to_nearest_pixel() {
    let geometry = rectangle([20, 80, 160, 255]);
    let options = BitmapExportOptions { format: BitmapFormat::Png, scale: 0.5, flatten: false };
    assert_eq!(export_bitmap(&input(&geometry), &options).unwrap(), expected_bytes(BitmapFormat::Png, [20, 80, 160, 255], false, 2));
    let hidden = BitmapExportInput { hidden_regions: &[0], ..input(&geometry) };
    assert_eq!(export_bitmap(&hidden, &options).unwrap(), expected_bytes(BitmapFormat::Png, [255; 4], false, 2));
    let undone = BitmapExportInput { background_undo_count: 1, ..hidden };
    assert_eq!(export_bitmap(&undone, &options).unwrap(), expected_bytes(BitmapFormat::Png, [0; 4], true, 2));
    assert!(export_bitmap(&input(&geometry), &BitmapExportOptions { scale: 0.1, ..options }).unwrap_err().to_string().contains("dimensions must be positive"));
    assert!(export_bitmap(&BitmapExportInput { hidden_regions: &[1], ..hidden }, &options).unwrap_err().to_string().contains("hiddenRegions"));
}

fn scene_document(reference: &Value) -> (ImageDocument, i64, i64) {
    let decoded = decode_image(&common::b64(reference["sourceBytes"].as_str().unwrap())).unwrap();
    let filename = resolve_image_filename(reference["sourceFilename"].as_str().unwrap());
    let mut document = ImageDocument::new(CoreSession::default());
    document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, filename.shadow_control as f64).unwrap();
    document.run_automatic(&json!({"extension": filename.extension})).unwrap();
    for action in reference["actions"].as_array().into_iter().flatten() {
        match action[1].as_i64().unwrap() {
            0 => { document.toggle_background([action[2].as_f64().unwrap(), action[3].as_f64().unwrap()]).unwrap(); }
            1 => { document.undo_background(); }
            2 => { document.redo_background(); }
            _ => { document.revert_background(); }
        }
    }
    (document, decoded.width as i64, decoded.height as i64)
}

fn check_native_scenes(name: &str, count: usize) {
    let references = common::read_ndjson(&format!("{name}.ndjson.gz"));
    assert_eq!(references.len(), count, "{name} fixture count");
    for reference in &references {
        let id = reference["id"].as_str().unwrap();
        let (mut document, width, height) = scene_document(reference);
        let state = document.snapshot();
        let background = document.background_state();
        let geometry = state.fitting.as_ref().filter(|fitting| fitting.complete).map(|fitting| &fitting.geometry);
        assert!(geometry.is_some(), "{id}: fitting must complete");
        let hidden_regions: Vec<i64> = background.hidden_regions.iter().map(|&region| region as i64).collect();
        let input = BitmapExportInput {
            geometry, width, height,
            source_alpha_type: document.source_alpha_type(), background_mode: state.settings.background_mode,
            background_undo_count: background.undo.len(), hidden_regions: &hidden_regions,
        };
        for (filename, expected) in reference["expected"].as_object().unwrap() {
            let format = filename.rsplit('.').next().unwrap().parse().unwrap();
            let options = BitmapExportOptions { format, scale: reference["scale"].as_f64().unwrap(), flatten: false };
            let actual = export_bitmap(&input, &options).unwrap_or_else(|error| panic!("{id} {filename}: {error}"));
            assert!(actual == common::b64(expected["bytes"].as_str().unwrap()), "{id} {filename}: encoded bytes differ");
        }
        let background_after = document.background_state();
        assert_eq!(background_after.hidden_regions, background.hidden_regions, "{id}");
        assert_eq!(background_after.undo, background.undo, "{id}");
        assert_eq!(background_after.redo, background.redo, "{id}");
        assert_eq!(&document.snapshot().fitting.unwrap().geometry, geometry.unwrap(), "{id}");
    }
}

#[test]
fn native_fitted_bitmap_files() { check_native_scenes("bitmap-scenes-reference", 11); }

#[test]
fn native_fitted_jpeg_files() { check_native_scenes("bitmap-jpeg-scenes-reference", 2); }

#[test]
fn native_background_visibility_and_history_files() { check_native_scenes("bitmap-visibility-reference", 5); }

#[test]
fn native_view_scale_bitmap_files() { check_native_scenes("bitmap-view-scenes-reference", 3); }

#[test]
fn native_alpha_scene_requires_consent_before_non_png_export() {
    let references = common::read_ndjson("bitmap-scenes-reference.ndjson.gz");
    let reference = references.iter().find(|row| row["id"] == "alpha-png").unwrap();
    let (mut document, width, height) = scene_document(reference);
    let state = document.snapshot();
    let fitting = state.fitting.as_ref().unwrap();
    assert!(fitting.complete);
    let background = document.background_state();
    let hidden: Vec<i64> = background.hidden_regions.iter().map(|&region| region as i64).collect();
    let input = BitmapExportInput {
        geometry: Some(&fitting.geometry), width, height,
        source_alpha_type: document.source_alpha_type(), background_mode: state.settings.background_mode,
        background_undo_count: background.undo.len(), hidden_regions: &hidden,
    };
    for format in [BitmapFormat::Bmp, BitmapFormat::Jpeg, BitmapFormat::Tiff] {
        let options = BitmapExportOptions::new(format);
        assert_eq!(export_bitmap(&input, &options).unwrap_err().code(), Some("BITMAP_FLATTEN_REQUIRED"));
        let bytes = export_bitmap(&input, &BitmapExportOptions { flatten: true, ..options }).unwrap();
        let decoded = decode_image(&bytes).unwrap();
        assert!(decoded.bgra.chunks_exact(4).all(|pixel| pixel[3] == 255), "{format:?}");
    }
    assert_eq!(document.snapshot().fitting.unwrap().geometry, fitting.geometry);
    let after = document.background_state();
    assert_eq!(after.hidden_regions, background.hidden_regions);
    assert_eq!(after.undo, background.undo);
    assert_eq!(after.redo, background.redo);
}
