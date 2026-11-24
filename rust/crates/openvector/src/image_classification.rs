//! Port of `src/image-classification.mjs`: the image type/quality decision
//! model and its twenty float32 feature producers.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::blending::boundary_mask;
use crate::palette::{byte_to_unit, calculate_measurement, detect_histogram_peaks_touched, js_max, TouchedBins};

/// Decision node: `[feature, threshold, less-than child, other child]`, an
/// extension literal test, or a label leaf. Equal and unordered comparisons
/// take the other child.
enum Node {
    Split(usize, f64, &'static Node, &'static Node),
    Extension(&'static str, &'static Node, &'static Node),
    Leaf(&'static str),
}

static IMAGE_TYPE_QUALITY_MODEL: &Node = &Node::Split(18, 0.0134977,
  &Node::Split(4, 0.960049,
    &Node::Split(2, 0.0197072,
      &Node::Split(18, 0.00219943,
        &Node::Split(3, 0.0923396,
          &Node::Split(16, 0.111529,
            &Node::Leaf("logo_aa 2"),
            &Node::Split(11, 0.00792502,
              &Node::Split(13, 57.5,
                &Node::Leaf("logo_aa 0"),
                &Node::Leaf("logo_aa 1")),
              &Node::Split(6, 0.00258916,
                &Node::Leaf("logo_aa 2"),
                &Node::Leaf("logo_aa 2")))),
          &Node::Split(2, 0.0111265,
            &Node::Split(0, 0.588894,
              &Node::Leaf("logo_aa 2"),
              &Node::Leaf("logo_aa 2")),
            &Node::Split(8, 0.000665414,
              &Node::Split(9, 400368.0,
                &Node::Leaf("logo_aa 2"),
                &Node::Split(19, 3.515e-7,
                  &Node::Leaf("logo_aa 1"),
                  &Node::Leaf("logo_aa 2"))),
              &Node::Leaf("logo_aa 1")))),
        &Node::Split(0, 0.406237,
          &Node::Split(2, 0.0162755,
            &Node::Leaf("logo_aa 2"),
            &Node::Leaf("photo 1")),
          &Node::Leaf("logo_aa 2"))),
      &Node::Split(18, 0.00291393,
        &Node::Split(2, 0.0285461,
          &Node::Split(4, 0.674069,
            &Node::Split(2, 0.0238979,
              &Node::Leaf("logo_aa 2"),
              &Node::Leaf("logo_aa 1")),
            &Node::Split(18, 0.00132773,
              &Node::Extension(".bmp",
                &Node::Leaf("logo_aa 1"),
                &Node::Extension(".jpeg",
                  &Node::Leaf("logo_aa 1"),
                  &Node::Extension(".png",
                    &Node::Leaf("logo_aa 1"),
                    &Node::Extension(".tiff",
                      &Node::Leaf("logo_aa 1"),
                      &Node::Leaf("logo_aa 2"))))),
              &Node::Leaf("logo_aa 2"))),
          &Node::Split(5, 0.998571,
            &Node::Leaf("logo_aa 1"),
            &Node::Leaf("logo_aa 1"))),
        &Node::Split(12, 2.63168,
          &Node::Split(2, 0.0540923,
            &Node::Leaf("photo 1"),
            &Node::Leaf("logo_aa 1")),
          &Node::Split(2, 0.0380126,
            &Node::Split(0, 0.400423,
              &Node::Split(3, 0.136656,
                &Node::Split(8, 0.000193388,
                  &Node::Leaf("photo 1"),
                  &Node::Leaf("logo_aa 2")),
                &Node::Leaf("logo_aa 1")),
              &Node::Leaf("logo_aa 1")),
            &Node::Leaf("logo_aa 1"))))),
    &Node::Split(18, 5.16e-7,
      &Node::Split(2, 0.00130897,
        &Node::Leaf("logo 2"),
        &Node::Leaf("logo_aa 2")),
      &Node::Split(0, 0.380912,
        &Node::Split(2, 0.00114401,
          &Node::Leaf("logo_aa 2"),
          &Node::Leaf("logo_aa 1")),
        &Node::Leaf("logo_aa 1")))),
  &Node::Split(5, 0.867741,
    &Node::Split(18, 0.0649332,
      &Node::Split(15, 0.0323685,
        &Node::Leaf("logo_aa 2"),
        &Node::Split(10, 9497.5,
          &Node::Leaf("photo 1"),
          &Node::Leaf("logo_aa 1"))),
      &Node::Split(0, 0.20806,
        &Node::Split(15, 0.158062,
          &Node::Split(4, 0.619184,
            &Node::Split(7, 0.00190362,
              &Node::Split(1, 0.00002355,
                &Node::Leaf("logo_aa 2"),
                &Node::Leaf("photo 1")),
              &Node::Leaf("photo 1")),
            &Node::Leaf("photo 1")),
          &Node::Split(12, 7.34716,
            &Node::Leaf("photo 1"),
            &Node::Leaf("photo 1"))),
        &Node::Split(0, 0.928628,
          &Node::Leaf("photo 1"),
          &Node::Leaf("logo_aa 0")))),
    &Node::Split(2, 0.0124402,
      &Node::Split(4, 0.984352,
        &Node::Split(3, 0.091711,
          &Node::Split(7, 0.00301763,
            &Node::Leaf("logo_aa 2"),
            &Node::Leaf("photo 1")),
          &Node::Leaf("logo_aa 2")),
        &Node::Leaf("logo 2")),
      &Node::Split(0, 0.241446,
        &Node::Split(3, 0.183605,
          &Node::Split(2, 0.0225216,
            &Node::Split(5, 0.915114,
              &Node::Leaf("photo 1"),
              &Node::Split(4, 0.649337,
                &Node::Leaf("logo_aa 2"),
                &Node::Split(13, 80543.5,
                  &Node::Leaf("photo 1"),
                  &Node::Leaf("logo_aa 2")))),
            &Node::Split(2, 0.103327,
              &Node::Split(15, 0.327859,
                &Node::Leaf("photo 1"),
                &Node::Leaf("photo 1")),
              &Node::Leaf("logo_aa 1"))),
          &Node::Split(7, 0.00268996,
            &Node::Leaf("photo 1"),
            &Node::Split(15, 1.19829,
              &Node::Leaf("logo_aa 1"),
              &Node::Leaf("photo 1")))),
        &Node::Split(3, 0.132534,
          &Node::Leaf("photo 1"),
          &Node::Leaf("logo_aa 2"))))));

/// Image type/quality decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageClassification {
    pub label: &'static str,
    /// 0 photo, 1 anti-aliased logo, 2 logo.
    pub image_type: u8,
    pub quality: u8,
}

/// Internal decision kernel over twenty float32 features and the exact
/// extension string supplied by the filename stage.
pub fn classify_image_features(features: &[f32], extension: &str) -> ImageClassification {
    let mut node = IMAGE_TYPE_QUALITY_MODEL;
    let label = loop {
        node = match *node {
            Node::Leaf(label) => break label,
            Node::Split(feature, threshold, less, other) => if (features[feature] as f64) < threshold { less } else { other },
            Node::Extension(literal, equal, other) => if extension == literal { equal } else { other },
        };
    };
    let image_type = if label.starts_with("photo") { 0 } else if label.starts_with("logo_aa") { 1 } else { 2 };
    let digit = label.as_bytes()[label.len() - 1] - b'0';
    ImageClassification { label, image_type, quality: 2 - digit }
}

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

const X86_INDEFINITE_F32: u32 = 0xffc0_0000;

static CLASSIFIER_BLUR_KERNEL: LazyLock<([f64; 25], f64)> = LazyLock::new(|| {
    let mut weights = [0.0f64; 25];
    let mut offset = 0;
    for y in -2i32..=2 {
        for x in -2i32..=2 {
            // The exponent is stored as float32 before dividing by the double
            // normalization constant.
            weights[offset] = f32r((-((x * x + y * y) as f64) / 1.125).exp()) / (0.5625 * (2.0 * std::f64::consts::PI));
            offset += 1;
        }
    }
    let sum = weights.iter().fold(0.0f64, |sum, weight| sum + weight);
    (weights, f32r(1.0 / sum))
});

/// The byte a truncation-then-store of `value` produces.
///
/// Rust's `as u8` cast truncates toward zero and saturates at the ends, which is
/// exactly `value.trunc()` followed by the same cast for every `f64`: below the
/// ends both give the same integer, above them both clamp, and `NaN` stores 0
/// either way. So the libm `trunc` call these two kernels used to make before
/// their store was doing work the cast repeats, and it is gone.
#[inline]
fn truncated_byte(value: f64) -> u8 {
    value as u8
}

/// Internal initial color preparation over decoded BGRA bytes. Preserve the
/// double multiply order and truncate each premultiplied channel.
pub fn premultiply_classifier_pixels(bgra: &[u8]) -> Vec<u8> {
    let mut prepared = bgra.to_vec();
    for (pixel, source) in prepared.chunks_exact_mut(4).zip(bgra.chunks_exact(4)) {
        let alpha = source[3] as f64 * (1.0 / 255.0);
        for channel in 0..3 { pixel[channel] = truncated_byte(source[channel] as f64 * alpha); }
    }
    prepared
}

/// Internal fixed Gaussian over a prepared Q8 raster. Clamp edge coordinates,
/// preserve alpha and own the output.
pub fn blur_classifier_pixels(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
    let (kernel_weights, reciprocal) = &*CLASSIFIER_BLUR_KERNEL;
    let mut blurred = bgra.to_vec();
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let mut sums = [0.0f32; 3];
            let mut kernel = 0;
            for dy in -2..=2i64 {
                let row = (height as i64 - 1).min((y + dy).max(0)) as usize;
                for dx in -2..=2i64 {
                    let column = (width as i64 - 1).min((x + dx).max(0)) as usize;
                    let pixel = (row * width + column) * 4;
                    let weight = kernel_weights[kernel];
                    kernel += 1;
                    for channel in 0..3 {
                        sums[channel] = (sums[channel] as f64 + bgra[pixel + channel] as f64 * weight) as f32;
                    }
                }
            }
            let target = (y as usize * width + x as usize) * 4;
            for channel in 0..3 {
                let value = f32r(sums[channel] as f64 * reciprocal);
                // The clamp bounds the value to `[0, 255]` before the store, and
                // the store's cast truncates toward zero, so the `trunc` that
                // used to sit between them was redundant: on a non-negative
                // value the cast is the truncation, and `f64::max`/`min` drop a
                // `NaN` to the same `0.0` the truncated form produced.
                blurred[target + channel] = truncated_byte((value + 0.5).max(0.0).min(255.0));
            }
        }
    }
    blurred
}

#[inline]
fn byte_pixel_distance(bgra: &[u8], first: usize, second: usize) -> f32 {
    let mut distance = 0.0f32;
    for channel in 0..4 {
        let difference = byte_to_unit(bgra[first + channel]) - byte_to_unit(bgra[second + channel]);
        distance += difference * difference;
    }
    distance
}

/// Gradient maps and their float32 root mean squares.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierGradients {
    pub gradient: Vec<f32>,
    pub blur_gradient: Vec<f32>,
    pub root_mean_square: [f32; 2],
}

/// Internal gradient kernel over a working raster and full source blur. The
/// producer owns working dimensions of at least 2 by 2. Each grid keeps its
/// own row width; the source blur can be larger than the working image.
pub fn measure_classifier_gradients(width: usize, height: usize, bgra: &[u8], blurred_bgra: &[u8], blur_width: usize) -> ClassifierGradients {
    let mut gradient = vec![0.0f32; width * height];
    let mut blur_gradient = vec![0.0f32; width * height];
    let (mut square_sum, mut blur_square_sum) = (0.0f32, 0.0f32);
    for y in 0..height {
        let bottom_row = if y + 1 < height { y + 1 } else { y - 1 };
        let top_row = if y == 0 { 1 } else { y - 1 };
        for x in 0..width {
            let pixel = y * width + x;
            let center = pixel * 4;
            let right_column = if x + 1 < width { x + 1 } else { x - 1 };
            let left_column = if x == 0 { 1 } else { x - 1 };
            let right = (y * width + right_column) * 4;
            let left = (y * width + left_column) * 4;
            let bottom = (bottom_row * width + x) * 4;
            let top = (top_row * width + x) * 4;
            let blur_center = (y * blur_width + x) * 4;
            let blur_right = (y * blur_width + right_column) * 4;
            let blur_left = (y * blur_width + left_column) * 4;
            let blur_bottom = (bottom_row * blur_width + x) * 4;
            let blur_top = (top_row * blur_width + x) * 4;
            // Each neighbor's channels are summed before adding that distance.
            let mut neighbor_cost = byte_pixel_distance(bgra, center, right);
            neighbor_cost += byte_pixel_distance(bgra, center, left);
            neighbor_cost += byte_pixel_distance(bgra, center, bottom);
            neighbor_cost += byte_pixel_distance(bgra, center, top);
            let mut blurred_cost = 0.0f32;
            for channel in 0..4 {
                let mut mean = byte_to_unit(blurred_bgra[blur_right + channel]) + byte_to_unit(blurred_bgra[blur_left + channel]);
                mean += byte_to_unit(blurred_bgra[blur_bottom + channel]);
                mean += byte_to_unit(blurred_bgra[blur_top + channel]);
                mean *= 0.25;
                let residual = byte_to_unit(blurred_bgra[blur_center + channel]) - mean;
                blurred_cost += residual * residual;
            }
            gradient[pixel] = neighbor_cost;
            blur_gradient[pixel] = blurred_cost;
            square_sum += neighbor_cost * neighbor_cost;
            blur_square_sum += blurred_cost * blurred_cost;
        }
    }
    let pixels = (width * height) as f32;
    let root_mean_square = [((square_sum / pixels) as f64).sqrt() as f32, ((blur_square_sum / pixels) as f64).sqrt() as f32];
    ClassifierGradients { gradient, blur_gradient, root_mean_square }
}

#[inline]
fn sample_luminance(samples: &[f32], offset: usize) -> f64 {
    // x87 double precision until the difference is stored; order G, R, B.
    samples[offset + 1] as f64 * 0.59 + samples[offset + 2] as f64 * 0.3 + samples[offset] as f64 * 0.11
}

/// Internal six-pixel edge sampler over caller-owned normalized BGRA samples
/// and profile storage. Empty support retains native signed NaNs.
pub fn sample_classifier_edge(samples: &[f32], threshold: f32, profile: &mut [f32; 5]) -> bool {
    let mut previous = sample_luminance(samples, 0);
    for index in 0..5 {
        let current = sample_luminance(samples, (index + 1) * 4);
        profile[index] = (current - previous) as f32;
        previous = current;
    }
    if profile[2] < 0.0 {
        for value in profile.iter_mut() { *value = 0.0 - *value; }
    }
    if profile[3] < 0.0 { profile[3] = 0.0; profile[4] = 0.0; }
    else if profile[4] < 0.0 { profile[4] = 0.0; }
    if profile[1] < 0.0 { profile[1] = 0.0; profile[0] = 0.0; }
    else if profile[0] < 0.0 { profile[0] = 0.0; }
    let (mut maximum, mut total) = (profile[0], profile[0]);
    for index in 1..5 {
        if profile[index] > maximum { maximum = profile[index]; }
        total += profile[index];
    }
    let accepted = profile[2] > threshold && maximum == profile[2];
    let reciprocal = 1.0f32 / total;
    for value in profile.iter_mut() {
        // 0 * inf yields the x86 single-precision indefinite NaN 0xFFC00000.
        let product = *value as f64 * reciprocal as f64;
        *value = if product.is_nan() && !value.is_nan() && !reciprocal.is_nan() {
            f32::from_bits(X86_INDEFINITE_F32)
        } else {
            product as f32
        };
    }
    accepted
}

/// Accepted-edge averaging for classifier slots 2, 3 and 4. The six-pixel
/// window runs from center-2 through center+3 on each axis.
pub fn measure_classifier_edges(width: usize, height: usize, bgra: &[u8]) -> [f32; 3] {
    let mut samples = [0.0f32; 24];
    let mut profile = [0.0f32; 5];
    let mut sums = [0.0f32; 5];
    let mut accepted = 0u32;
    for y in 2..(height as i64 - 3).max(2) as usize {
        for x in 2..(width as i64 - 3).max(2) as usize {
            for direction in 0..2 {
                let stride = if direction == 0 { 4 } else { width * 4 };
                let start = (y * width + x) * 4 - stride * 2;
                for pixel in 0..6 {
                    for channel in 0..4 {
                        samples[pixel * 4 + channel] = byte_to_unit(bgra[start + pixel * stride + channel]);
                    }
                }
                if sample_classifier_edge(&samples, 0.1f32, &mut profile) {
                    accepted += 1;
                    for index in 0..5 { sums[index] += profile[index]; }
                }
            }
        }
    }
    if accepted != 0 {
        let reciprocal = 1.0f32 / accepted as f32;
        for sum in sums.iter_mut() { *sum *= reciprocal; }
    }
    [(sums[0] + sums[4]) * 0.5, (sums[1] + sums[3]) * 0.5, sums[2]]
}

/// Dense region labels and moments from the prepared region dispatch.
#[derive(Debug, Clone, Copy)]
pub struct ClassifierRegions<'a> {
    pub indices: &'a [i32],
    pub counts: &'a [i32],
    pub sums: &'a [f32],
}

/// Internal region kernel for slots 5..13.
pub fn measure_classifier_regions(width: usize, height: usize, bgra: &[u8], regions: ClassifierRegions) -> [f32; 9] {
    let ClassifierRegions { indices, counts, sums } = regions;
    let mask = boundary_mask(width, height, indices);
    let mut means = vec![0.0f32; sums.len()];
    for (region, &count) in counts.iter().enumerate() {
        let reciprocal = 1.0f32 / count as f32;
        for channel in 0..4 { means[region * 4 + channel] = sums[region * 4 + channel] * reciprocal; }
    }
    let (mut interior_sum, mut boundary_sum, mut total_sum) = (0.0f32, 0.0f32, 0.0f32);
    let (mut interior_count, mut boundary_count) = (0u32, 0u32);
    let mut colors = HashSet::new();
    for (pixel, &index) in indices.iter().enumerate() {
        let mut error = 0.0f32;
        for channel in 0..4 {
            let residual = byte_to_unit(bgra[pixel * 4 + channel]) - means[index as usize * 4 + channel];
            error += residual * residual;
        }
        if mask[pixel] == 0 { interior_sum += error; interior_count += 1; }
        else { boundary_sum += error; boundary_count += 1; }
        total_sum += error;
        let offset = pixel * 4;
        colors.insert(u32::from_le_bytes([bgra[offset], bgra[offset + 1], bgra[offset + 2], bgra[offset + 3]]));
    }
    let interior_mean = if interior_count == 0 { interior_sum } else { interior_sum / interior_count as f32 };
    let boundary_mean = if boundary_count == 0 { boundary_sum } else { boundary_sum / boundary_count as f32 };
    let mean_sum = boundary_mean + interior_mean;
    let pixels = width * height;
    let region_count = counts.len() as f32;
    [if mean_sum > 0.0 { boundary_mean / mean_sum } else { 0.0 }, total_sum / pixels as f32,
        boundary_mean, interior_mean, pixels as f32, region_count, region_count / pixels as f32,
        (counts.len() as f64 / (pixels as f64).sqrt()) as f32, colors.len() as f32]
}

#[inline]
fn histogram_index(bgra: &[u8], offset: usize) -> usize {
    (bgra[offset] >> 4) as usize + (((bgra[offset + 1] >> 4) as usize) << 4)
        + (((bgra[offset + 2] >> 4) as usize) << 8) + (((bgra[offset + 3] >> 4) as usize) << 12)
}

fn histogram_weight(histogram: &[f64]) -> f64 {
    histogram.iter().fold(0.0, |weight, bin| weight + bin)
}

/// The histogram bins one peak's 3x3x3x3 neighborhood clear writes, in the
/// order the clear visits them.
///
/// The clear and the save below must agree on the set exactly, so both walk
/// this one iterator: the same four clamped ranges, the same nesting, the same
/// index expression.
fn peak_neighborhood_indices(peaks: &[u8], offset: usize) -> impl Iterator<Item = usize> {
    let [blue, green, red, alpha] = [0, 1, 2, 3].map(|channel| (peaks[offset + channel] >> 4) as i32);
    ((blue - 1).max(0)..=(blue + 1).min(15)).flat_map(move |b| {
        ((green - 1).max(0)..=(green + 1).min(15)).flat_map(move |g| {
            ((red - 1).max(0)..=(red + 1).min(15)).flat_map(move |r| {
                ((alpha - 1).max(0)..=(alpha + 1).min(15))
                    .map(move |a| (b + (g << 4) + (r << 8) + (a << 12)) as usize)
            })
        })
    })
}

fn clear_peak_neighborhood(histogram: &mut [f64], peaks: &[u8], offset: usize) {
    for index in peak_neighborhood_indices(peaks, offset) { histogram[index] = 0.0; }
}

fn clear_peak_lines(histogram: &mut [f64], peaks: &[u8]) {
    let mut start = [0.0f32; 4];
    let mut difference = [0.0f32; 4];
    let mut sample = [0u8; 4];
    for first in (0..peaks.len()).step_by(4) {
        for channel in 0..4 { start[channel] = byte_to_unit(peaks[first + channel]); }
        for second in (first + 4..peaks.len()).step_by(4) {
            let mut squared_distance = 0.0f64;
            for channel in 0..4 {
                difference[channel] = byte_to_unit(peaks[second + channel]) - start[channel];
                squared_distance += difference[channel] as f64 * difference[channel] as f64;
            }
            let intervals = 2 - (squared_distance.sqrt() * -16.0).trunc() as i64;
            for index in 0..=intervals {
                let fraction = index as f32 / intervals as f32;
                for channel in 0..4 {
                    let coordinate = difference[channel] * fraction + start[channel];
                    sample[channel] = ((coordinate as f64 * 255.0 + 0.5).trunc() as i64 & 255) as u8;
                }
                histogram[histogram_index(&sample, 0)] = 0.0;
            }
        }
    }
}

/// Edge-weight measurement and features for classifier slots 14..19.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierColors {
    pub measurement: Vec<u8>,
    pub features: [f32; 6],
}

/// Internal weighted-color kernel for classifier slots 14..19. Explicit edge
/// controls belong to the prepared classification context.
pub fn measure_classifier_colors(width: usize, height: usize, bgra: &[u8], edge_scale: f64, edge_floor: f64) -> ClassifierColors {
    let measurement = calculate_measurement(width, height, bgra);
    let pixels = width * height;
    let weights: Vec<f64> = measurement.iter()
        .map(|&value| js_max(1.0 - value as f64 * edge_scale * (1.0 / 255.0), edge_floor)).collect();
    let mut histogram = vec![0.0f64; 65536];
    let mut touched = TouchedBins::new(16);
    for pixel in 0..pixels { touched.add(&mut histogram, histogram_index(bgra, pixel * 4), weights[pixel]); }
    let mut peaks = detect_histogram_peaks_touched(&histogram, 16, true, touched.indices).bgra;
    let initial_weight = histogram_weight(&histogram);
    for offset in (0..peaks.len()).step_by(4) {
        let index = histogram_index(&peaks, offset);
        histogram[index] = 0.0;
    }
    let bin_remainder = histogram_weight(&histogram);
    // The neighborhood clear below is undone before `clear_peak_lines` runs, and
    // the only write it makes is `= 0.0`. So instead of cloning the whole
    // 65,536-bin histogram (512 KiB) and copying it back, save the bins the
    // clear will touch and restore exactly those. The set is the union of the
    // peaks' neighborhoods (at most 12 x 81 bins), built from the same iterator
    // the clear walks, so the save and the clear cannot disagree; a bin shared
    // by two neighborhoods is saved once, before the first write, and restored
    // once, after the last. Every other bin is untouched by the clear, so it
    // needs no restore, and each restored bin gets back the value it held
    // before the clear — bit-identical to the copy-back.
    let mut cleared: Vec<usize> = (0..peaks.len()).step_by(4)
        .flat_map(|offset| peak_neighborhood_indices(&peaks, offset)).collect();
    cleared.sort_unstable();
    cleared.dedup();
    let saved: Vec<f64> = cleared.iter().map(|&index| histogram[index]).collect();
    for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut histogram, &peaks, offset); }
    let neighborhood_remainder = histogram_weight(&histogram);
    for (&index, &value) in cleared.iter().zip(&saved) { histogram[index] = value; }
    clear_peak_lines(&mut histogram, &peaks);
    let line_remainder = histogram_weight(&histogram);
    if peaks.is_empty() { peaks = vec![0; 4]; }
    let peak_count = (peaks.len() / 4).min(12);
    let mut assignments = vec![0u8; pixels];
    let mut sums = vec![0.0f32; peak_count * 4];
    let mut counts = vec![0.0f32; peak_count];
    let mut sample = [0.0f32; 4];
    let mut total_weight = 0.0f32;
    for pixel in 0..pixels {
        for channel in 0..4 { sample[channel] = byte_to_unit(bgra[pixel * 4 + channel]); }
        // Original peak bytes are deliberately not normalized here.
        let (mut selected, mut minimum) = (0usize, f64::INFINITY);
        for peak in 0..peak_count {
            let mut distance = 0.0f64;
            for channel in 0..4 {
                let difference = sample[channel] - peaks[peak * 4 + channel] as f32;
                distance += difference as f64 * difference as f64;
            }
            if distance < minimum { minimum = distance; selected = peak; }
        }
        assignments[pixel] = selected as u8;
        let weight = weights[pixel];
        for channel in 0..4 {
            sums[selected * 4 + channel] = (sums[selected * 4 + channel] as f64 + f32r(sample[channel] as f64 * weight)) as f32;
        }
        counts[selected] = (counts[selected] as f64 + weight) as f32;
        total_weight = (total_weight as f64 + weight) as f32;
    }
    for peak in 0..peak_count {
        let reciprocal = 1.0f32 / counts[peak];
        for channel in 0..4 { sums[peak * 4 + channel] *= reciprocal; }
    }
    let (mut mean, mut fourth) = (1e-6f32, 0.0f32);
    for pixel in 0..pixels {
        let assigned = assignments[pixel] as usize * 4;
        let mut residual = byte_to_unit(bgra[pixel * 4]) - sums[assigned];
        for channel in 1..4 {
            residual += byte_to_unit(bgra[pixel * 4 + channel]) - sums[assigned + channel];
        }
        let square = residual * residual;
        let weight = weights[pixel] as f32;
        mean += square * weight;
        fourth += square * square * weight;
    }
    let reciprocal = 1.0f32 / total_weight;
    mean *= reciprocal;
    fourth *= reciprocal;
    let weight_reciprocal = 1.0 / initial_weight;
    ClassifierColors { measurement, features: [
        (fourth as f64 / (mean as f64 * mean as f64) - 3.0) as f32, mean, fourth,
        (bin_remainder * weight_reciprocal) as f32, (neighborhood_remainder * weight_reciprocal) as f32,
        (line_remainder * weight_reciprocal) as f32,
    ] }
}

#[cfg(test)]
mod classifier_store_equivalence {
    //! The two classifier kernels' byte stores against the retained, verbatim
    //! pre-change expressions (`f64::trunc` followed by the same cast). Every
    //! byte is compared, so a rounding, saturation or NaN mistake fails here.
    use super::*;

    /// The pre-change premultiply, kept verbatim as the oracle.
    fn premultiply_reference(bgra: &[u8]) -> Vec<u8> {
        let mut prepared = bgra.to_vec();
        for (pixel, source) in prepared.chunks_exact_mut(4).zip(bgra.chunks_exact(4)) {
            let alpha = source[3] as f64 * (1.0 / 255.0);
            for channel in 0..3 { pixel[channel] = (source[channel] as f64 * alpha).trunc() as u8; }
        }
        prepared
    }

    /// The pre-change blur, kept verbatim as the oracle.
    fn blur_reference(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
        let (kernel_weights, reciprocal) = &*CLASSIFIER_BLUR_KERNEL;
        let mut blurred = bgra.to_vec();
        for y in 0..height as i64 {
            for x in 0..width as i64 {
                let mut sums = [0.0f32; 3];
                let mut kernel = 0;
                for dy in -2..=2i64 {
                    let row = (height as i64 - 1).min((y + dy).max(0)) as usize;
                    for dx in -2..=2i64 {
                        let column = (width as i64 - 1).min((x + dx).max(0)) as usize;
                        let pixel = (row * width + column) * 4;
                        let weight = kernel_weights[kernel];
                        kernel += 1;
                        for channel in 0..3 {
                            sums[channel] = (sums[channel] as f64 + bgra[pixel + channel] as f64 * weight) as f32;
                        }
                    }
                }
                let target = (y as usize * width + x as usize) * 4;
                for channel in 0..3 {
                    let value = f32r(sums[channel] as f64 * reciprocal);
                    blurred[target + channel] = (value + 0.5).trunc().max(0.0).min(255.0) as u8;
                }
            }
        }
        blurred
    }

    fn stream(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed | 1;
        (0..len).map(|_| { state ^= state << 13; state ^= state >> 7; state ^= state << 17; (state >> 24) as u8 }).collect()
    }

    /// The premultiply store's whole reachable domain: every `(channel, alpha)`
    /// byte pair, so every product `s * (a / 255)` the kernel can form.
    #[test]
    fn premultiply_store_matches_the_retained_expression() {
        let mut bgra = Vec::with_capacity(256 * 256 * 4);
        for alpha in 0..=255u8 {
            for channel in 0..=255u8 {
                bgra.extend_from_slice(&[channel, channel, channel, alpha]);
            }
        }
        assert_eq!(premultiply_classifier_pixels(&bgra), premultiply_reference(&bgra));
        // And the helper itself over the same products plus the ends.
        for alpha in 0..=255u8 {
            let unit = alpha as f64 * (1.0 / 255.0);
            for channel in 0..=255u8 {
                let value = channel as f64 * unit;
                assert_eq!(truncated_byte(value), (value.trunc() as u8), "value {value:?}");
            }
        }
        for value in [0.0, -0.0, 0.5, 1.0, 254.5, 255.0, 255.5, 256.0, -1.0, -0.5,
                      f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300] {
            assert_eq!(truncated_byte(value), (value.trunc() as u8), "value {value:?}");
        }
    }

    /// The blur store over real rasters of every small shape (so the clamped
    /// edge taps are exercised) and over the store's own boundaries.
    #[test]
    fn blur_store_matches_the_retained_expression() {
        for &(w, h) in &[(1, 1), (1, 2), (2, 1), (2, 2), (3, 5), (5, 3), (7, 7), (9, 4), (16, 11)] {
            for seed in 0..3u64 {
                let bgra = stream(w * h * 4, seed * 104_729 + (w * h) as u64);
                assert_eq!(blur_classifier_pixels(w, h, &bgra), blur_reference(w, h, &bgra),
                           "blur differs at {w}x{h} seed {seed}");
            }
        }
        // Saturated and transparent rasters, so the sums reach both ends.
        for &(w, h) in &[(4, 4), (6, 3)] {
            for fill in [0u8, 255u8] {
                let bgra = vec![fill; w * h * 4];
                assert_eq!(blur_classifier_pixels(w, h, &bgra), blur_reference(w, h, &bgra));
            }
        }
        // The store's boundaries: the clamp makes the value non-negative, so the
        // cast is the truncation; a NaN is dropped to 0 by `max` in both forms.
        let store = |value: f64| -> u8 { (value + 0.5).max(0.0).min(255.0) as u8 };
        let oracle = |value: f64| -> u8 { (value + 0.5).trunc().max(0.0).min(255.0) as u8 };
        let mut values = vec![0.0, -0.0, 0.5, -0.5, 1.0, 254.0, 254.5, 255.0, 255.5, 256.0,
                              -1.0, -1e300, 1e300, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        for k in 0..=512 {
            values.push(k as f64 - 0.5);
            values.push(k as f64);
            values.push(k as f64 + 0.5);
        }
        for value in values {
            assert_eq!(store(value), oracle(value), "store differs at {value:?}");
        }
    }
}

#[cfg(test)]
mod histogram_restore_equivalence {
    //! The saved-bin restore against the retained clone/copy-back form. The
    //! oracle is the pre-change code verbatim: clone the whole histogram, run
    //! the neighborhood clears, then copy the clone back. Every bin is compared
    //! by bit pattern, so a missed, doubled or mis-ordered restore fails here.
    use super::*;

    /// The pre-change form, kept verbatim as the oracle.
    fn clear_and_restore_reference(histogram: &mut [f64], peaks: &[u8]) {
        let after_bins = histogram.to_vec();
        for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(histogram, peaks, offset); }
        histogram.copy_from_slice(&after_bins);
    }

    /// The production form: save the bins the clear will touch, clear, restore.
    fn clear_and_restore_saved(histogram: &mut [f64], peaks: &[u8]) {
        let mut cleared: Vec<usize> = (0..peaks.len()).step_by(4)
            .flat_map(|offset| peak_neighborhood_indices(peaks, offset)).collect();
        cleared.sort_unstable();
        cleared.dedup();
        let saved: Vec<f64> = cleared.iter().map(|&index| histogram[index]).collect();
        for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(histogram, peaks, offset); }
        for (&index, &value) in cleared.iter().zip(&saved) { histogram[index] = value; }
    }

    fn bits(values: &[f64]) -> Vec<u64> { values.iter().map(|v| v.to_bits()).collect() }

    #[test]
    fn the_saved_bin_restore_matches_the_clone_and_copy_back() {
        // A deterministic spread of bin values, including the ones a restore
        // could confuse: zero, negative zero, NaN, infinities and tiny values.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let mut histogram: Vec<f64> = (0..65536).map(|i| match i % 97 {
            0 => 0.0,
            1 => -0.0,
            2 => f64::NAN,
            3 => f64::INFINITY,
            4 => f64::NEG_INFINITY,
            _ => (next() % 1_000_000) as f64 * 1e-6,
        }).collect();
        // Peaks at the corners, the middle and the edges, so the clamped ranges
        // are exercised at both ends and in the interior.
        let peaks: Vec<u8> = vec![
            0, 0, 0, 0,
            255, 255, 255, 255,
            0, 255, 0, 255,
            128, 128, 128, 128,
            16, 32, 48, 64,
            240, 224, 208, 192,
        ];
        let mut reference = histogram.clone();
        clear_and_restore_reference(&mut reference, &peaks);
        clear_and_restore_saved(&mut histogram, &peaks);
        assert_eq!(bits(&histogram), bits(&reference), "the restore changed a bin");
        // Non-vacuity: the clear really did write, and the restore really did
        // put values back (the reference and the saved form both differ from a
        // histogram that was cleared and never restored).
        let mut cleared_only = reference.clone();
        for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut cleared_only, &peaks, offset); }
        assert_ne!(bits(&cleared_only), bits(&reference), "the clear wrote nothing");
        let touched = (0..peaks.len()).step_by(4)
            .flat_map(|offset| peak_neighborhood_indices(&peaks, offset)).count();
        assert!(touched >= 4 * 27, "the neighborhood set is too small: {touched}");
    }

    #[test]
    fn the_saved_set_is_exactly_the_set_the_clear_writes() {
        let mut histogram = vec![1.0; 65536];
        for blue in 0..16u8 {
            for green in 0..16u8 {
                for red in 0..16u8 {
                    for alpha in 0..16u8 {
                        let peaks = [blue << 4, green << 4, red << 4, alpha << 4];
                        let mut saved: Vec<_> = peak_neighborhood_indices(&peaks, 0).collect();
                        saved.sort_unstable();
                        saved.dedup();
                        assert!(!saved.is_empty());
                        // A stray write is visible either before the next clear
                        // that covers its bin, or in the final histogram.
                        assert!(saved.iter().all(|&i| histogram[i] == 1.0), "peak {peaks:?}");
                        clear_peak_neighborhood(&mut histogram, &peaks, 0);
                        for i in saved {
                            assert_eq!(histogram[i], 0.0, "peak {peaks:?}, bin {i}");
                            histogram[i] = 1.0;
                        }
                    }
                }
            }
        }
        assert!(histogram.iter().all(|&value| value == 1.0));
    }

    /// Overlapping and repeated neighborhoods: a bin shared by several peaks is
    /// saved once, before the first write, and restored once, after the last.
    #[test]
    fn overlapping_and_repeated_neighborhoods_restore_the_pre_clear_values() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let histogram: Vec<f64> = (0..65536).map(|i| match i % 89 {
            0 => 0.0,
            1 => -0.0,
            2 => f64::NAN,
            3 => f64::NEG_INFINITY,
            _ => (next() % 1_000_000) as f64 * 1e-6,
        }).collect();
        // The same peak four times, then peaks one nibble apart in each channel,
        // so every neighborhood overlaps its neighbours and the union is much
        // smaller than the sum of the parts.
        let peaks: Vec<u8> = vec![
            0x80, 0x80, 0x80, 0x80,
            0x80, 0x80, 0x80, 0x80,
            0x80, 0x80, 0x80, 0x80,
            0x80, 0x80, 0x80, 0x80,
            0x70, 0x80, 0x80, 0x80,
            0x80, 0x70, 0x80, 0x80,
            0x80, 0x80, 0x70, 0x80,
            0x80, 0x80, 0x80, 0x70,
            0x90, 0x90, 0x90, 0x90,
        ];
        let mut reference = histogram.clone();
        clear_and_restore_reference(&mut reference, &peaks);
        let mut saved_form = histogram.clone();
        clear_and_restore_saved(&mut saved_form, &peaks);
        assert_eq!(bits(&saved_form), bits(&reference), "an overlapping restore changed a bin");
        let mut saved: Vec<usize> = (0..peaks.len()).step_by(4)
            .flat_map(|offset| peak_neighborhood_indices(&peaks, offset)).collect();
        let raw = saved.len();
        saved.sort_unstable();
        saved.dedup();
        assert!(saved.len() < raw, "the fixture does not overlap: {raw} vs {}", saved.len());
        // Every saved bin really was written by the clear, and every bin the
        // clear wrote is in the saved set.
        let mut cleared_only = histogram.clone();
        for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut cleared_only, &peaks, offset); }
        let written: Vec<usize> = (0..65536).filter(|&i| cleared_only[i] == 0.0 && histogram[i] != 0.0).collect();
        for index in &written { assert!(saved.binary_search(index).is_ok(), "bin {index} written but not saved"); }
    }

    /// Negative controls: the comparison above must fail for a restore that
    /// skips a bin, and for one that saves after the clear instead of before.
    #[test]
    fn a_wrong_restore_is_observable_on_this_corpus() {
        let histogram: Vec<f64> = (0..65536).map(|i| 1.0 + i as f64).collect();
        let peaks: Vec<u8> = vec![0x80, 0x80, 0x80, 0x80, 0x00, 0xff, 0x10, 0x20];
        let mut reference = histogram.clone();
        clear_and_restore_reference(&mut reference, &peaks);
        // Skipping the last saved bin leaves it zeroed.
        let mut skipped = histogram.clone();
        {
            let mut cleared: Vec<usize> = (0..peaks.len()).step_by(4)
                .flat_map(|offset| peak_neighborhood_indices(&peaks, offset)).collect();
            cleared.sort_unstable();
            cleared.dedup();
            let saved: Vec<f64> = cleared.iter().map(|&index| histogram[index]).collect();
            for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut skipped, &peaks, offset); }
            for (&index, &value) in cleared.iter().zip(&saved).take(cleared.len() - 1) { skipped[index] = value; }
        }
        assert_ne!(bits(&skipped), bits(&reference), "a skipped restore went unnoticed");
        // Saving after the clear restores zeros.
        let mut late = histogram.clone();
        {
            let mut cleared: Vec<usize> = (0..peaks.len()).step_by(4)
                .flat_map(|offset| peak_neighborhood_indices(&peaks, offset)).collect();
            cleared.sort_unstable();
            cleared.dedup();
            for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut late, &peaks, offset); }
            let saved: Vec<f64> = cleared.iter().map(|&index| late[index]).collect();
            for (&index, &value) in cleared.iter().zip(&saved) { late[index] = value; }
        }
        assert_ne!(bits(&late), bits(&reference), "a post-clear save went unnoticed");
    }

    #[test]
    fn an_empty_peak_list_restores_nothing_and_changes_nothing() {
        let histogram: Vec<f64> = (0..65536).map(|i| i as f64).collect();
        let mut saved_form = histogram.clone();
        clear_and_restore_saved(&mut saved_form, &[]);
        assert_eq!(bits(&saved_form), bits(&histogram));
    }
}
