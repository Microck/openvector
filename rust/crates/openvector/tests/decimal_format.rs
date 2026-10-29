mod common;
use openvector::decimal_format::{fixed_decimal, stream_general_decimal};

#[test]
fn general_key_formatting_retains_integer_zeroes_and_trims_only_fractional_zeroes() {
    for (value, expected) in [(0.0, "0"), (-0.0, "0"), (10.0, "10"), (100000.0, "100000"), (-100000.0, "-100000"),
        (1000000.0, "1e+006"), (0.00001, "1e-005"), (0.0001, "0.0001"), (12.5, "12.5")] {
        assert_eq!(stream_general_decimal(value), expected, "{value}");
    }
}

fn check(name: &str, count: usize, precision: usize) {
    let reference = common::read_json(name);
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(fixed_decimal(case["value"].as_f64().unwrap(), precision), case["expected"].as_str().unwrap(),
            "{name} {index}");
    }
}

#[test]
fn fixed_decimal_output_matches_all_16406_original_msvcr71_strings() { check("cpp-fixed-reference.json", 16406, 2); }

#[test]
fn three_decimal_output_matches_all_28956_original_probes() { check("cpp-fixed-three-reference.json", 28956, 3); }

#[test]
fn sixteen_decimal_output_matches_all_28700_original_probes() { check("cpp-fixed-sixteen-reference.json", 28700, 16); }

#[test]
fn special_decimal_labels_match_original_bit_patterns() {
    let special = common::read_json("cpp-special-reference.json");
    let cases = special["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let bits = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
        let precision = case["precision"].as_u64().unwrap() as usize;
        assert_eq!(fixed_decimal(f64::from_bits(bits), precision), case["expected"].as_str().unwrap());
    }
}
