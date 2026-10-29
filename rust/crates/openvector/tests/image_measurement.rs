mod common;
use openvector::contours::Geometry;
use openvector::image_measurement::ContourImageModel;
use openvector::image_measurement_kernel::{Measurement, PixelFragmentScratch};
use serde_json::Value;

fn floats(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }
fn bits(values: &[f64]) -> Vec<u64> { values.iter().map(|v| v.to_bits()).collect() }

struct Input { width: f64, height: f64, bgra: Vec<u8>, indices: Vec<i64>, geometry: Geometry }
fn input_for(f: &Value) -> Input {
    Input { width: f["width"].as_f64().unwrap(), height: f["height"].as_f64().unwrap(),
        bgra: f["bgra"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect(),
        indices: f["segmentation"]["indices"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect(),
        geometry: Geometry::from_json(&f["geometry"]).unwrap() }
}
fn model(i: &Input) -> Result<ContourImageModel, openvector::pixel_input::InputError> {
    ContourImageModel::new(i.width, i.height, &i.bgra, &i.indices, &i.geometry)
}
fn positions_for(f: &Value) -> Vec<f64> {
    f["geometry"]["nodes"].as_array().unwrap().iter().flat_map(|n| [n["x"].as_f64().unwrap(), n["y"].as_f64().unwrap()]).collect()
}

fn assert_measurement(actual: &Measurement, expected: &Value, context: &str) {
    assert_eq!(actual.energy.to_bits(), expected["energy"].as_f64().unwrap().to_bits(), "{context} energy");
    assert_eq!(bits(&actual.gradient), bits(&floats(&expected["gradient"])), "{context} gradient");
    let colors: Vec<f64> = actual.colors.iter().map(|&c| c as f64).collect();
    assert_eq!(bits(&colors), bits(&floats(&expected["colors"])), "{context} colors");
    let flags: Vec<u64> = actual.flags.iter().map(|&f| f as u64).collect();
    let expected_flags: Vec<u64> = expected["flags"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect();
    assert_eq!(flags, expected_flags, "{context} flags");
    let pixels = expected["pixels"].as_array().unwrap();
    assert_eq!(actual.pixels.len(), pixels.len(), "{context} pixels");
    for (p, (a, e)) in actual.pixels.iter().zip(pixels).enumerate() {
        let e: Vec<Vec<u64>> = e.as_array().unwrap().iter().map(|r| bits(&floats(r))).collect();
        let a: Vec<Vec<u64>> = a.iter().map(|r| bits(r)).collect();
        assert_eq!(a, e, "{context} pixel {p}");
    }
}

#[test]
fn fragment_scratch_fields_match_the_original_overlapping_words_at_the_retained_cache_fault() {
    let reference = common::read_json("image-fragment-scratch-reference.json");
    let records: Vec<Vec<f64>> = reference["records"].as_array().unwrap().iter().map(floats).collect();
    let mut scratch = PixelFragmentScratch::new();
    scratch.set_count(0);
    scratch.set_background(-1);
    let mut groups = 0;
    let mut first = 0;
    while first < records.len() {
        let (mut last, mut area) = (first, 0.0);
        while last + 1 < records.len() && records[last + 1][2] == records[first][2] {
            area = (records[last + 1][4] * records[last][3] - records[last][4] * records[last + 1][3]) + area;
            last += 1;
        }
        area = ((records[first][4] * records[last][3] - records[last][4] * records[first][3]) + area) * 0.5;
        let count = scratch.count();
        scratch.write_identifier(count, records[first][2] as i32).unwrap();
        scratch.write_area(scratch.count(), area).unwrap();
        scratch.set_count(scratch.count().wrapping_add(1));
        groups += 1;
        first = last + 1;
    }
    assert_eq!(groups, 11);
    assert_eq!(scratch.count() as i64, reference["expectedCountWord"].as_i64().unwrap());
    let hex = reference["scratchBytesHex"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let word = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    assert_eq!(scratch.background(), word(0x7c));
    for index in 0..10 { assert_eq!(scratch.identifier(index).unwrap(), word(index as usize * 4)); }
    for index in 0..11 {
        let o = 0x28 + index as usize * 8;
        assert_eq!(scratch.area(index).unwrap(), f64::from_le_bytes(bytes[o..o + 8].try_into().unwrap()));
    }
}

#[test]
fn fragment_scratch_reports_native_bounds_failures_without_panicking() {
    let mut scratch = PixelFragmentScratch::new();
    for index in [-1, 32, i32::MAX] {
        assert!(scratch.identifier(index).is_err());
        assert!(scratch.write_identifier(index, 0).is_err());
    }
    for index in [-1, 11, i32::MAX] {
        assert!(scratch.area(index).is_err());
        assert!(scratch.write_area(index, 0.0).is_err());
    }
    scratch.write_area(10, 1.0).unwrap();
    assert_eq!(scratch.count(), 0);
    assert_eq!(scratch.background(), 0x3ff00000);
    scratch.set_count(33);
    assert!(scratch.find(-1).is_err());
}

#[test]
fn image_measurement_matches_all_2802_native_energies_colors_clipped_records_and_raw_gradients() {
    let reference = common::read_json("smoothing-image-measurement-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2802);
    let (mut records, mut pipeline_states) = (0usize, 0);
    let mut purposes = std::collections::HashSet::new();
    for (index, fixture) in cases.iter().enumerate() {
        let actual = model(&input_for(fixture)).unwrap().evaluate(&positions_for(fixture)).unwrap();
        assert_measurement(&actual, &fixture["expected"], &format!("image measurement {index}"));
        records += actual.pixels.iter().map(Vec::len).sum::<usize>();
        if fixture.get("sourcePipeline").is_some_and(|v| !v.is_null() && v != &Value::Bool(false)) { pipeline_states += 1; }
        if let Some(p) = fixture["purpose"].as_str() { purposes.insert(p.to_string()); }
    }
    assert_eq!(records, 925831);
    assert_eq!(pipeline_states, 45);
    for p in ["Exact quarter-pixel displacement", "Near-horizontal double directions, exponent -70", "Near-horizontal double directions, exponent -90"] {
        assert!(purposes.contains(p));
    }
    let wrap = cases.iter().find(|f| f["sourceObjective"].as_i64() == Some(2217)).unwrap();
    assert_eq!(wrap["phase"].as_i64(), Some(2));
    assert_eq!(wrap["expected"]["pixels"][1], serde_json::json!([]));
}

#[test]
fn persistent_image_measurement_matches_all_6981_native_states_across_1425_coordinate_sequences() {
    let sequences = common::read_json("smoothing-image-sequence-reference.json");
    let cases = sequences["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1425);
    let (mut states, mut records, mut hysteresis) = (0, 0usize, 0);
    for (index, fixture) in cases.iter().enumerate() {
        let mut m = model(&input_for(fixture)).unwrap();
        let expected = fixture["expected"].as_array().unwrap();
        for (step, positions) in fixture["positions"].as_array().unwrap().iter().enumerate() {
            let actual = m.evaluate(&floats(positions)).unwrap();
            assert_measurement(&actual, &expected[step], &format!("image sequence {index}, step {step}"));
            states += 1;
            records += actual.pixels.iter().map(Vec::len).sum::<usize>();
        }
        if expected[0]["energy"] != expected[expected.len() - 1]["energy"] { hysteresis += 1; }
    }
    assert_eq!(states, 6981);
    assert_eq!(records, 2346200);
    assert_eq!(hysteresis, 652);
}

#[test]
fn quarter_pixel_image_error_and_free_interior_derivatives_agree_with_direct_geometry() {
    let reference = common::read_json("smoothing-image-measurement-reference.json");
    let fixture = reference["cases"].as_array().unwrap().iter()
        .find(|f| f["purpose"].as_str() == Some("Exact quarter-pixel displacement")).unwrap();
    let (positions, input) = (positions_for(fixture), input_for(fixture));
    let actual = model(&input).unwrap().evaluate(&positions).unwrap();
    assert_eq!(actual.energy, 0.1875);
    let interior = input.geometry.nodes.iter().position(|n| n.kind == 0).unwrap();
    let step = 1.0 / 1024.0;
    for axis in [0, 1] {
        let (mut before, mut after) = (positions.clone(), positions.clone());
        let channel = interior * 2 + axis;
        before[channel] -= step;
        after[channel] += step;
        let derivative = (model(&input).unwrap().evaluate(&after).unwrap().energy
            - model(&input).unwrap().evaluate(&before).unwrap().energy) / (2.0 * step);
        assert_eq!(derivative, actual.gradient[channel]);
    }
}

#[test]
fn image_models_own_prepared_inputs_returned_snapshots_and_evaluation_coordinates() {
    let reference = common::read_json("smoothing-image-measurement-reference.json");
    let fixture = &reference["cases"][1];
    let mut input = input_for(fixture);
    let before_geometry = input.geometry.clone();
    let mut positions = positions_for(fixture);
    let mut m = model(&input).unwrap();
    let mut actual = m.evaluate(&positions).unwrap();
    assert_eq!(input.geometry, before_geometry);
    assert_measurement(&actual, &fixture["expected"], "first");
    input.bgra.fill(17);
    input.indices.fill(0);
    input.geometry.nodes[0].x = f64::NAN;
    input.geometry.regions[0].bgra.fill(17);
    input.geometry.regions[0].contour.fill(0);
    actual.colors.fill(7.0);
    actual.flags.fill(7);
    actual.gradient.fill(7.0);
    if let Some(r) = actual.pixels[0].first_mut() { r.fill(7.0); }
    positions.fill(7.0);
    assert_measurement(&m.evaluate(&positions_for(fixture)).unwrap(), &fixture["expected"], "second");
}

#[test]
fn invalid_prepared_image_inputs_and_coordinates_fail_before_changing_persistent_state() {
    let sequences = common::read_json("smoothing-image-sequence-reference.json");
    let fixture = &sequences["cases"][1];
    let input = input_for(fixture);
    let empty = Geometry { nodes: vec![], regions: vec![] };
    let negative = vec![-1i64; input.indices.len()];
    // null/{} map to empty inputs; a 256 channel has no u8 form.
    let invalid: [(f64, f64, &[u8], &[i64], &Geometry); 6] = [
        (f64::NAN, f64::NAN, &[], &[], &empty), (1.0, input.height, &input.bgra, &input.indices, &input.geometry),
        (input.width, input.height, &[], &input.indices, &input.geometry), (input.width, input.height, &input.bgra, &input.indices, &empty),
        (input.width, input.height, &input.bgra, &[], &input.geometry), (input.width, input.height, &input.bgra, &negative, &input.geometry)];
    for (w, h, b, i, g) in invalid { assert!(ContourImageModel::new(w, h, b, i, g).is_err()); }
    let mut m = model(&input).unwrap();
    let steps = fixture["positions"].as_array().unwrap();
    let expected = fixture["expected"].as_array().unwrap();
    assert_measurement(&m.evaluate(&floats(&steps[0])).unwrap(), &expected[0], "step 0");
    let n = steps[0].as_array().unwrap().len();
    // A Float32Array argument has no distinct Rust form; holes map to NaN.
    for positions in [vec![], vec![], vec![f64::NAN; n], vec![f64::INFINITY; n]] {
        assert!(m.evaluate(&positions).is_err());
    }
    assert_measurement(&m.evaluate(&floats(&steps[1])).unwrap(), &expected[1], "step 1");
}
