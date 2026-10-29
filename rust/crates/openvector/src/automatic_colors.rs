//! Port of `src/automatic-colors.mjs`: the compiled palette-count classifier
//! and its 62-feature producer.

use std::collections::HashSet;

use crate::palette::PaletteScore;

/// Decision node `[feature, threshold, less-than child, other child]` or a
/// label leaf. Equal and unordered values take the other child, matching the
/// original x87 FCOMIP/JBE branches.
enum Node {
    Split(usize, f64, &'static Node, &'static Node),
    Leaf(&'static str),
}

static PALETTE_COUNT_MODEL: &Node = &Node::Split(50, 2.69034,
  &Node::Split(50, 1.73555,
    &Node::Split(51, 217.677,
      &Node::Split(37, 35.25,
        &Node::Leaf("3"),
        &Node::Leaf("2")),
      &Node::Leaf("2")),
    &Node::Split(51, 995.704,
      &Node::Split(36, 2.5,
        &Node::Leaf("2"),
        &Node::Split(39, 0.351533,
          &Node::Leaf("3"),
          &Node::Split(13, 0.938407,
            &Node::Split(50, 2.52056,
              &Node::Leaf("13"),
              &Node::Leaf("4")),
            &Node::Leaf("13")))),
      &Node::Leaf("2"))),
  &Node::Split(40, 0.288933,
    &Node::Split(39, 0.333629,
      &Node::Leaf("3"),
      &Node::Split(15, 6.5,
        &Node::Split(12, 2.2514,
          &Node::Split(36, 3.5,
            &Node::Leaf("3"),
            &Node::Split(39, 0.413465,
              &Node::Leaf("3"),
              &Node::Leaf("4"))),
          &Node::Leaf("13")),
        &Node::Leaf("4"))),
    &Node::Split(13, 2.04511,
      &Node::Split(15, 7.5,
        &Node::Split(51, 2004.84,
          &Node::Split(36, 6.5,
            &Node::Leaf("13"),
            &Node::Split(10, 1.24186,
              &Node::Leaf("8"),
              &Node::Leaf("13"))),
          &Node::Split(0, 986.0,
            &Node::Leaf("2"),
            &Node::Leaf("3"))),
        &Node::Split(36, 5.5,
          &Node::Split(41, 0.136787,
            &Node::Leaf("5"),
            &Node::Leaf("4")),
          &Node::Split(3, 8.5,
            &Node::Split(16, 6.5,
              &Node::Leaf("13"),
              &Node::Split(42, 0.313304,
                &Node::Leaf("6"),
                &Node::Leaf("7"))),
            &Node::Split(44, 0.129347,
              &Node::Leaf("8"),
              &Node::Leaf("9"))))),
      &Node::Split(36, 7.5,
        &Node::Split(50, 3.56775,
          &Node::Split(51, 3296.27,
            &Node::Leaf("13"),
            &Node::Leaf("2")),
          &Node::Split(41, 0.160751,
            &Node::Leaf("5"),
            &Node::Leaf("13"))),
        &Node::Leaf("13")))));

/// Decoded palette-count label from the original registered 2..13 domain.
/// Mode 0 lets the controller choose its minimum-score candidate (12 is then
/// a bound), mode 1 is an exact count and mode 2 means 12 colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCountDecision {
    pub label: &'static str,
    pub mode: u8,
    pub color_count: u32,
}

pub fn decode_palette_count_label(label: &'static str) -> PaletteCountDecision {
    let mode = match label { "2" => 0, "13" => 2, _ => 1 };
    let color_count = if label == "13" { 12 } else { label.parse().expect("registered palette-count label") };
    PaletteCountDecision { label, mode, color_count }
}

/// Internal model kernel over the 62-double feature contract. Derived NaNs
/// are observable native states and are not rejected.
pub fn classify_palette_features(features: &[f64]) -> PaletteCountDecision {
    let mut node = PALETTE_COUNT_MODEL;
    loop {
        match *node {
            Node::Leaf(label) => return decode_palette_count_label(label),
            Node::Split(feature, threshold, less, other) => node = if features[feature] < threshold { less } else { other },
        }
    }
}

/// x86 turns an invalid operation on non-NaN operands into the default
/// indefinite NaN 0xFFF8000000000000; AArch64 yields 0x7FF8000000000000.
/// Features are compared bit-exactly, so generated NaNs take the x86 pattern.
fn x86_double(result: f64, operands: &[f64]) -> f64 {
    if result.is_nan() && !operands.iter().any(|value| value.is_nan()) {
        f64::from_bits(0xfff8_0000_0000_0000)
    } else {
        result
    }
}

/// `Math.min` over a list, propagating NaN and ordering -0 below +0.
fn js_min(values: &[f64]) -> f64 {
    values.iter().fold(f64::INFINITY, |minimum, &value| {
        if minimum.is_nan() || value.is_nan() { f64::NAN }
        else if value < minimum || (value == minimum && value.is_sign_negative()) { value }
        else { minimum }
    })
}

/// One retained 2..12 candidate: membership weights after the last
/// assignment pass and its pre-quantization score.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteCandidate {
    pub weights: Vec<f64>,
    pub score: PaletteScore,
}

pub const EMPTY_PEAKS_FAULT: &str = "Automatic color selection has no histogram peaks. The original classifier faults while reading the empty peak-weight array.";

#[derive(Clone, Copy)]
struct Ranked {
    value: f64,
    id: i32,
}

/// Build the original 62 features from validated BGRA and original-order
/// candidates. Empty peak arrays reproduce a native fatal as a diagnostic.
pub fn build_palette_features(bgra: &[u8], candidates: &[PaletteCandidate], peak_weights: &[f64]) -> Result<[f64; 62], String> {
    if peak_weights.is_empty() { return Err(EMPTY_PEAKS_FAULT.to_string()); }
    let sentinel = Ranked { value: 1e6, id: -1 };
    let mut total_order = vec![sentinel, sentinel];
    let mut error_order = vec![sentinel, sentinel];
    let mut maximum_error = candidates[0].score.error as f64;
    let mut minimum_error = maximum_error;
    for (index, candidate) in candidates.iter().enumerate() {
        let (total, error) = (candidate.score.total as f64, candidate.score.error as f64);
        total_order.push(Ranked { value: total, id: index as i32 + 2 });
        error_order.push(Ranked { value: error, id: index as i32 + 2 });
        if error > maximum_error { maximum_error = error; }
        if error < minimum_error { minimum_error = error; }
    }
    // Ties break by candidate ID; only the two sentinels are equal pairs.
    let compare = |left: &Ranked, right: &Ranked| {
        if left.value > right.value { std::cmp::Ordering::Greater }
        else if left.value < right.value { std::cmp::Ordering::Less }
        else { left.id.cmp(&right.id) }
    };
    total_order.sort_by(compare);
    error_order.sort_by(compare);
    let mut features = [0.0f64; 62];
    let distinct: HashSet<u32> = bgra.chunks_exact(4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]])).collect();
    features[0] = distinct.len() as f64;
    features[1] = (bgra.len() / 4) as f64;
    for rank in 0..total_order.len() {
        if rank < 4 {
            features[2 + rank] = total_order[rank].id as f64;
            features[6 + rank] = error_order[rank].id as f64;
            features[10 + rank] = error_order[rank].value;
        }
        if total_order[rank].id > 1 { features[14 + total_order[rank].id as usize - 2] = rank as f64; }
        if error_order[rank].id > 1 { features[25 + error_order[rank].id as usize - 2] = rank as f64; }
    }
    features[36] = peak_weights.len() as f64;
    features[37] = js_min(peak_weights);
    for (index, candidate) in candidates.iter().enumerate() {
        let error = candidate.score.error as f64;
        let numerator = x86_double(error - minimum_error, &[error, minimum_error]);
        let range = x86_double(maximum_error - minimum_error, &[maximum_error, minimum_error]);
        features[38 + index] = x86_double(numerator / range, &[numerator, range]);
        features[51 + index] = js_min(&candidate.weights);
    }
    features[49] = minimum_error;
    features[50] = maximum_error;
    Ok(features)
}
