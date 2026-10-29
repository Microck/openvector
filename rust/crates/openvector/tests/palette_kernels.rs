//! Port of the leaf-kernel groups of `test/palette.test.mjs`: assignment,
//! histogram peaks, anti-alias measurement and unweighted scoring. The
//! CoreSession groups belong with the session port.
mod common;
use common::read_json;
use openvector::palette::{assign_palette, find_histogram_peaks, measure_anti_alias, score_unweighted_palette};
use serde_json::Value;

fn floats(value: &Value) -> Vec<f64> {
    value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
}

fn bytes(value: &Value) -> Vec<u8> {
    value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()
}

fn ints(value: &Value) -> Vec<i64> {
    value.as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()
}

#[test]
fn palette_assignment_matches_all_original_host_cpu_pixel_decisions() {
    let reference = read_json("palette-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1193);
    for fixture in cases {
        let indices = assign_palette(&bytes(&fixture["pixels"]), &floats(&fixture["palette"])).unwrap();
        assert_eq!(indices.iter().map(|&i| i as i64).collect::<Vec<_>>(), ints(&fixture["expected"]));
    }
}

#[test]
fn assignment_handles_empty_images_and_keeps_the_first_color_on_ties() {
    let bgra = [128u8, 128, 128, 255];
    let palette = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    assert_eq!(assign_palette(&bgra, &palette).unwrap(), vec![0]);
    assert_eq!(assign_palette(&[], &palette).unwrap().len(), 0);
    for (bgra, palette) in [(&[0u8, 0][..], &[0.0, 0.0, 0.0, 1.0][..]), (&[], &[]), (&[], &[0.0, f64::NAN, 0.0, 1.0]),
        (&[], &[0.0, 0.0, 0.0, 1.1])] {
        assert!(assign_palette(bgra, palette).is_err());
    }
}

#[test]
fn histogram_peaks_and_their_tie_ordering_match_all_native_scans() {
    let reference = read_json("peaks-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 44);
    for fixture in cases {
        let bins = fixture["bins"].as_u64().unwrap() as usize;
        let mut histogram = vec![0.0f64; bins.pow(4)];
        for entry in fixture["entries"].as_array().unwrap() {
            histogram[entry[0].as_u64().unwrap() as usize] = entry[1].as_f64().unwrap();
        }
        let peaks = find_histogram_peaks(&histogram, bins, fixture["connected"].as_bool().unwrap()).unwrap();
        assert_eq!(peaks.bgra, bytes(&fixture["expected"]["bgra"]), "ordered peak colors");
        assert_eq!(peaks.weights, floats(&fixture["expected"]["weights"]), "ordered peak weights");
    }
}

#[test]
fn histogram_inputs_reject_malformed_shapes_and_invalid_weights() {
    let mut histogram = vec![0.0f64; 8usize.pow(4)];
    assert!(find_histogram_peaks(&histogram, 7, true).is_err());
    assert!(find_histogram_peaks(&[0.0], 8, true).is_err());
    assert!(find_histogram_peaks(&vec![f64::NAN; 8usize.pow(4)], 8, true).is_err());
    histogram[0] = -1.0;
    assert!(find_histogram_peaks(&histogram, 8, true).is_err());
    histogram[0] = f64::INFINITY;
    assert!(find_histogram_peaks(&histogram, 8, true).is_err());
}

#[test]
fn anti_alias_contrast_bytes_match_all_original_measurement_runs() {
    let reference = read_json("measurement-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 471);
    for fixture in cases {
        let measurement = measure_anti_alias(fixture["width"].as_i64().unwrap(), fixture["height"].as_i64().unwrap(),
            &bytes(&fixture["bgra"])).unwrap();
        assert_eq!(measurement, bytes(&fixture["expected"]));
    }
}

#[test]
fn measurement_validates_its_sampling_dimensions() {
    let bgra = [0u8, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
    assert!(measure_anti_alias(2, 2, &bgra).is_ok());
    assert!(measure_anti_alias(1, 2, &bgra).is_err());
    assert!(measure_anti_alias(2, 1, &bgra).is_err());
    assert!(measure_anti_alias(2, 2, &bgra[4..]).is_err());
}

#[test]
fn unweighted_candidate_scores_match_all_unmodified_original_engine_calls() {
    let reference = read_json("score-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1235);
    for fixture in cases {
        let score = score_unweighted_palette(&bytes(&fixture["bgra"]), &floats(&fixture["palette"]), &ints(&fixture["indices"]),
            fixture["costPerColor"].as_f64().unwrap()).unwrap();
        let expected = &fixture["expected"];
        assert_eq!([score.total as f64, score.error as f64, score.colors as f64],
            [expected["total"].as_f64().unwrap(), expected["error"].as_f64().unwrap(), expected["colors"].as_f64().unwrap()]);
    }
}

#[test]
fn scoring_consumes_supplied_indices_and_validates_the_boundary() {
    let (bgra, palette) = ([0u8, 0, 0, 255], [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    let score = score_unweighted_palette(&bgra, &palette, &[1], 0.15).unwrap();
    let expected = 4f64.log10() as f32;
    assert_eq!((score.total, score.error, score.colors), (expected, expected, 0.0));
    let score = score_unweighted_palette(&bgra, &palette, &[0], 0.15).unwrap();
    assert_eq!((score.total, score.error, score.colors), (0.0, 0.0, 0.0));
    for (indices, cost) in [(&[][..], 0.15), (&[-1][..], 0.15), (&[2][..], 0.15), (&[1][..], f64::INFINITY), (&[1][..], 1e100)] {
        assert!(score_unweighted_palette(&bgra, &palette, indices, cost).is_err());
    }
}
