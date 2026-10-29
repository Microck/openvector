mod common;
use openvector::bitmap_raster::{rasterize_paths, FillRule, PathCommand, RasterInput, RasterPath};
use serde_json::Value;

fn command(values: &Value) -> PathCommand {
    // Qt enums and wire opcodes belong only to fixture translation.
    let values: Vec<f64> = values.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    match values[0] as i64 {
        0 => PathCommand::M([values[1], values[2]]),
        1 => PathCommand::L([values[1], values[2]]),
        2 => PathCommand::C(values[1..7].try_into().unwrap()),
        3 => PathCommand::Z,
        kind => panic!("unknown command {kind}"),
    }
}

fn check(name: &str, count: usize) {
    let references = common::read_ndjson(&format!("{name}.ndjson.gz"));
    assert_eq!(references.len(), count, "{name} fixture count");
    let mut failures = Vec::new();
    for reference in &references {
        let id = reference["id"].as_str().unwrap();
        let (width, height) = (reference["width"].as_i64().unwrap(), reference["height"].as_i64().unwrap());
        let has_alpha = reference["format"].as_i64() == Some(6);
        let fill = reference["fill"].as_u64().unwrap();
        let background = std::array::from_fn(|channel| ((fill >> (channel * 8)) & 255) as u8);
        let paths: Vec<RasterPath> = reference["paths"].as_array().unwrap().iter().map(|path| RasterPath {
            color: std::array::from_fn(|i| path["bgra"][i].as_u64().unwrap() as u8),
            fill_rule: if path["rule"].as_i64() == Some(1) { FillRule::Nonzero } else { FillRule::EvenOdd },
            stroke: path["rule"].as_i64() == Some(2),
            commands: path["commands"].as_array().unwrap().iter().map(command).collect(),
        }).collect();
        let transform = std::array::from_fn(|i| reference["matrix"][i].as_f64().unwrap());
        let input = RasterInput { width, height, has_alpha, background: Some(background), transform, paths: &paths };
        let before = input.clone();
        let actual = rasterize_paths(&input).unwrap_or_else(|error| panic!("{id}: {error}"));
        if actual.bgra != common::b64(reference["expected"]["bgra"].as_str().unwrap()) {
            failures.push(format!("{id} pixel bytes differ"));
        }
        assert_eq!((actual.width, actual.height, actual.has_alpha), (width, height, has_alpha), "{id}");
        assert_eq!(input, before, "{id}");
    }
    assert!(failures.is_empty(), "{name}: {failures:?}");
}

#[test]
fn native_raster_pixels() { check("raster-pixels-reference", 20); }

#[test]
fn native_raster_pixels_bands() { check("raster-pixels-bands-reference", 3); }

#[test]
fn native_raster_pixels_pool() { check("raster-pixels-pool-reference", 1); }

#[test]
fn native_raster_pixels_partial() { check("raster-pixels-partial-reference", 1); }

#[test]
fn native_raster_pixels_quota_flushed() { check("raster-pixels-quota-flushed-reference", 1); }

#[test]
fn native_raster_strokes() { check("raster-strokes-reference", 26); }

#[test]
fn native_raster_strokes_boundaries() { check("raster-strokes-boundaries-reference", 15); }

#[test]
fn raster_path_admission_rejects_invalid_geometry_before_returning_pixels() {
    let frame = |transform: [f64; 4], paths: &[RasterPath]| {
        rasterize_paths(&RasterInput { width: 8, height: 8, has_alpha: false, background: None, transform, paths })
            .unwrap_err().0
    };
    let identity = [1.0, 1.0, 0.0, 0.0];
    let path = |commands: Vec<PathCommand>, stroke: bool| RasterPath { color: [0, 0, 0, 255], fill_rule: FillRule::EvenOdd, stroke, commands };
    assert!(frame([f64::INFINITY, 1.0, 0.0, 0.0], &[]).contains("four finite"));
    assert!(frame(identity, &[path(vec![PathCommand::L([1.0, 1.0])], false)]).contains("Start each raster subpath"));
    assert!(frame(identity, &[path(vec![PathCommand::M([f64::NAN, 1.0])], false)]).contains("coordinates must be finite"));
    assert!(frame(identity, &[path(vec![PathCommand::M([0.0, 0.0]), PathCommand::L([1.0, 1.0]), PathCommand::L([2.0, 2.0])], true)])
        .contains("one M followed by one L or C"));
}
