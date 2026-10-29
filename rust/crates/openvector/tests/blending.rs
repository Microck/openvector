mod common;
mod smoothing_support;
use openvector::blending::{fit_color_blend, score_weighted_palette};
use serde_json::Value;

fn bytes(v: &Value) -> Vec<u8> { v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect() }
fn floats(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }
fn ints(v: &Value) -> Vec<i64> { v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect() }

#[test]
fn local_color_blends_match_all_1080_original_solver_calls_exactly() {
    let reference = common::read_json("blend-sample-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1080);
    for (index, fixture) in cases.iter().enumerate() {
        let blended = fit_color_blend(&bytes(&fixture["bgra"]), &floats(&fixture["colors"])).unwrap();
        let color: Vec<f64> = blended.color.iter().map(|&c| c as f64).collect();
        smoothing_support::assert_same(&serde_json::json!({"color": color, "weights": blended.weights, "selfWeight": blended.self_weight}),
            &fixture["expected"], &format!("blend {index}"));
    }
}

#[test]
fn blend_boundaries_preserve_inputs_and_reject_invalid_samples_and_colors() {
    let (bgra, colors) = ([0u8, 127, 255, 255], [0.0, 0.0, 0.0, 1.0]);
    let mut blended = fit_color_blend(&bgra, &colors).unwrap();
    blended.color.fill(7.0);
    assert_eq!((bgra, colors), ([0u8, 127, 255, 255], [0.0, 0.0, 0.0, 1.0]));
    // null/{} and sparse arrays have no typed Rust form; byte typing rejects 256.
    let forty = vec![0.0; 40];
    for (b, c) in [(&[][..], &colors[..]), (&[0u8][..], &colors[..]), (&bgra[..], &[][..]), (&bgra[..], &forty[..]),
        (&bgra[..], &[f64::NAN, 0.0, 0.0, 1.0][..]), (&bgra[..], &[0.0, 0.0, 0.0, 2.0][..]), (&bgra[..], &[f64::NAN; 4][..])] {
        assert!(fit_color_blend(b, c).is_err());
    }
}

fn score(f: &Value) -> Result<openvector::blending::WeightedScore, openvector::pixel_input::InputError> {
    score_weighted_palette(f["width"].as_f64().unwrap(), f["height"].as_f64().unwrap(), &bytes(&f["bgra"]), &floats(&f["palette"]),
        &ints(&f["indices"]), f["costPerColor"].as_f64().unwrap_or(f64::INFINITY))
}

#[test]
fn weighted_palette_scores_match_all_440_original_scorer_calls_exactly() {
    let reference = common::read_json("weighted-score-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 440);
    for (index, fixture) in cases.iter().enumerate() {
        let actual = score(fixture).unwrap();
        let e = &fixture["expected"];
        assert_eq!(((actual.total as f64).to_bits(), (actual.error as f64).to_bits(), (actual.colors as f64).to_bits()),
            (e["total"].as_f64().unwrap().to_bits(), e["error"].as_f64().unwrap().to_bits(), e["colors"].as_f64().unwrap().to_bits()), "case {index}");
    }
}

#[test]
fn weighted_scoring_validates_boundaries_and_preserves_supplied_assignments() {
    let reference = common::read_json("weighted-score-reference.json");
    let input = reference["cases"].as_array().unwrap().iter()
        .find(|f| f["width"].as_i64() == Some(3) && f["colorCount"].as_i64() == Some(3)).unwrap().clone();
    let before = input.clone();
    score(&input).unwrap();
    assert_eq!(input, before);
    let with = |key: &str, value: Value| { let mut v = input.clone(); v[key] = value; v };
    let count = input["colorCount"].as_i64().unwrap();
    let n = input["indices"].as_array().unwrap().len();
    for invalid in [with("width", Value::from(-1)), with("height", Value::from(1.5)), with("bgra", serde_json::json!([0])),
        with("indices", serde_json::json!([])), with("indices", serde_json::json!(vec![-1; n])),
        with("indices", serde_json::json!(vec![count; n])), with("costPerColor", Value::Null)] {
        assert!(score(&invalid).is_err());
    }
}
