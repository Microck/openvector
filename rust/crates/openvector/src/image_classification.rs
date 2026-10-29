//! Port of `src/image-classification.mjs`: the image type/quality decision
//! model and its twenty float32 feature producers.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::blending::boundary_mask;
use crate::palette::{byte_to_unit, calculate_measurement, detect_histogram_peaks, js_max};

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

/// Internal initial color preparation over decoded BGRA bytes. Preserve the
/// double multiply order and truncate each premultiplied channel.
pub fn premultiply_classifier_pixels(bgra: &[u8]) -> Vec<u8> {
    let mut prepared = bgra.to_vec();
    for (pixel, source) in prepared.chunks_exact_mut(4).zip(bgra.chunks_exact(4)) {
        let alpha = source[3] as f64 * (1.0 / 255.0);
        for channel in 0..3 { pixel[channel] = (source[channel] as f64 * alpha).trunc() as u8; }
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
                blurred[target + channel] = (value + 0.5).trunc().max(0.0).min(255.0) as u8;
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

fn clear_peak_neighborhood(histogram: &mut [f64], peaks: &[u8], offset: usize) {
    let [blue, green, red, alpha] = [0, 1, 2, 3].map(|channel| (peaks[offset + channel] >> 4) as i32);
    for b in (blue - 1).max(0)..=(blue + 1).min(15) {
        for g in (green - 1).max(0)..=(green + 1).min(15) {
            for r in (red - 1).max(0)..=(red + 1).min(15) {
                for a in (alpha - 1).max(0)..=(alpha + 1).min(15) {
                    histogram[(b + (g << 4) + (r << 8) + (a << 12)) as usize] = 0.0;
                }
            }
        }
    }
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
    for pixel in 0..pixels { histogram[histogram_index(bgra, pixel * 4)] += weights[pixel]; }
    let mut peaks = detect_histogram_peaks(&histogram, 16, true).bgra;
    let initial_weight = histogram_weight(&histogram);
    for offset in (0..peaks.len()).step_by(4) {
        let index = histogram_index(&peaks, offset);
        histogram[index] = 0.0;
    }
    let bin_remainder = histogram_weight(&histogram);
    let after_bins = histogram.clone();
    for offset in (0..peaks.len()).step_by(4) { clear_peak_neighborhood(&mut histogram, &peaks, offset); }
    let neighborhood_remainder = histogram_weight(&histogram);
    histogram.copy_from_slice(&after_bins);
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
