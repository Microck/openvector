//! Port of the leaf kernels of `src/palette.mjs`: palette distance and
//! assignment, unweighted scoring, reflected-neighbor contrast measurement,
//! histogram peak detection and final palette quantization.

use crate::pixel_input::{read_palette_input, read_palette_score_input, validate_bgra, InputError};

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

static BYTE_TO_UNIT: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    let scale = f32r(1.0 / 255.0);
    std::array::from_fn(|byte| (byte as f64 * scale) as f32)
});

/// The table's own scale, as the `f32` the table rounds to: `f32r(1.0 / 255.0)`
/// is `f32(1.0 / 255.0)` widened back to `f64`, so this constant is the same
/// bit pattern. `BYTE_TO_UNIT[b]` equals `b as f32 * BYTE_SCALE` for every byte
/// (`byte_scale_matches_the_table` pins it over all 256).
const BYTE_SCALE: f32 = (1.0f64 / 255.0f64) as f32;

/// `byteToUnit`: `f32(value * f32(1 / 255))`.
#[inline]
pub fn byte_to_unit(value: u8) -> f32 {
    BYTE_TO_UNIT[value as usize]
}

/// Original SSE float32 distance; each square and partial sum is stored.
#[inline]
fn distance(palette: &[f32], offset: usize, blue: f32, green: f32, red: f32, alpha: f32) -> f32 {
    let b = blue - palette[offset];
    let g = green - palette[offset + 1];
    let r = red - palette[offset + 2];
    let a = alpha - palette[offset + 3];
    let mut cost = b * b + g * g;
    cost += r * r;
    cost + a * a
}

/// One palette scan for one pixel. `WITH_ALPHA` is `false` only when the pixel's
/// alpha bits equal every palette entry's alpha bits, which makes the alpha term
/// exactly `+0.0`; the two forms then produce bitwise identical costs.
#[inline]
fn scan_entries<const WITH_ALPHA: bool>(
    entries: &[[f32; 4]],
    blue: f32,
    green: f32,
    red: f32,
    alpha: f32,
) -> (i32, f32) {
    let first = &entries[0];
    let mut best = 0;
    let mut best_cost = if WITH_ALPHA {
        let a = alpha - first[3];
        let b = blue - first[0];
        let g = green - first[1];
        let r = red - first[2];
        ((b * b + g * g) + r * r) + a * a
    } else {
        let b = blue - first[0];
        let g = green - first[1];
        let r = red - first[2];
        (b * b + g * g) + r * r
    };
    // `entries[0]` above already panicked for an empty palette, so the tail is
    // well-formed and its one bounds check is hoisted out of the loop. Walking
    // the tail directly instead of `entries.iter().enumerate().skip(1)` drops
    // the `Skip` adapter's per-iteration "how many are left to skip" test,
    // which the emitted loop was paying on every candidate. `color` is the
    // tail's own index, so the palette index is `color + 1`; the entries and
    // the comparison order are unchanged.
    for (color, entry) in entries[1..].iter().enumerate() {
        // `distance` is `((b*b + g*g) + r*r) + a*a` and every term is a
        // square, so the blue/green partial sum is a lower bound on the
        // total: if it already reaches `best_cost` the strict `<` test
        // below cannot pass, and skipping the red/alpha terms changes no
        // assignment. NaN and infinity fall through to the full sum, which
        // then fails the same comparison it always did.
        let b = blue - entry[0];
        let g = green - entry[1];
        let partial = b * b + g * g;
        if partial >= best_cost { continue; }
        let r = red - entry[2];
        let cost = if WITH_ALPHA {
            let a = alpha - entry[3];
            (partial + r * r) + a * a
        } else {
            partial + r * r
        };
        if cost < best_cost {
            best = (color + 1) as i32;
            best_cost = cost;
        }
    }
    (best, best_cost)
}

pub(crate) fn assign_indices(bgra: &[u8], colors: &[f32], indices: &mut [i32]) {
    // Artwork contains many identical BGRA samples, often beyond adjacent runs.
    // Cache only this call: refinement changes the palette before the next pass.
    // The full key check makes collisions affect speed, never assignments.
    const CACHE_BITS: u32 = 10;
    let mut cache = [(0u32, -1i32); 1 << CACHE_BITS];
    // The palette's alpha bits, when every entry carries the same ones. The
    // sentinel is `u32::MAX`, which no pixel can match: a pixel's alpha is always
    // a `BYTE_TO_UNIT` entry, and every one of those is a finite value in
    // `[0, 1]`, so its bits are never `u32::MAX` (a NaN). A palette whose shared
    // alpha is NaN or infinite therefore also falls to the general scan, where
    // `NaN - NaN` and `inf - inf` behave exactly as they always did.
    let palette_alpha = {
        let (entries, _) = colors.as_chunks::<4>();
        match entries.split_first() {
            Some((first, rest)) => {
                let bits = first[3].to_bits();
                if rest.iter().all(|e| e[3].to_bits() == bits) { bits } else { u32::MAX }
            }
            None => u32::MAX,
        }
    };
    // `byte_to_unit` reads the table through its `LazyLock`, so written per
    // channel it costs four once-flag loads and four branches per pixel on top
    // of the four table loads. Taking the table's reference once per call
    // leaves exactly the four loads: the same table, the same `byte as usize`
    // index and the same `f32` values, so every channel read is unchanged and
    // the assignments are bit-identical. (The same hoist is already the
    // production form in `refine_prepared`; #100 measured it there.)
    let table = &*BYTE_TO_UNIT;
    // Walk the raster as whole four-byte pixels instead of computing
    // `pixel * 4` and slicing `bgra[offset..offset + 4]` per pixel: the slice
    // range check was emitted as a compare-and-branch per pixel on the memo
    // hit path, and the four channel reads were bounds-checked again. A
    // pixel array is exactly the same four bytes at the same offset, so the
    // key and every channel value are unchanged. Only whole groups of four
    // are visited, which is the only shape the engine ever passes (the
    // assignment buffer is `bgra.len() / 4` long); the `assert` keeps the old
    // panic for a caller that asks for more pixels than the raster holds.
    let (pixels, _trailing) = bgra.as_chunks::<4>();
    assert!(indices.len() <= pixels.len(), "assignment needs four bytes per pixel");
    // Adjacent pixels are very often the same BGRA sample (flat fills, runs
    // along a scanline). Remember the previous pixel's full key and its
    // assigned index for this call only: refinement changes the palette
    // between calls, so nothing is carried across a call boundary. The
    // sentinel is 2^32, outside the 32-bit key domain, so the first pixel
    // never matches and no index is reused before one has been assigned.
    //
    // Reusing the previous index is exactly what the memo would return: the
    // immediately preceding pixel either missed and stored `(key, best)` in
    // the slot for `key`, or hit and left that same entry in place, so the
    // slot still holds `(key, prev_index)` when the next pixel repeats the
    // key. The fast path therefore skips the multiply, the shift and the two
    // cache loads without changing any assignment; the memo is still updated
    // on every miss, so later non-adjacent repeats and hash collisions behave
    // exactly as before.
    let mut prev_key: u64 = 1 << 32;
    let mut prev_index: i32 = 0;
    for (pixel, index) in pixels.iter().zip(indices.iter_mut()) {
        let key = u32::from_le_bytes(*pixel);
        if prev_key == key as u64 {
            *index = prev_index;
            continue;
        }
        prev_key = key as u64;
        let slot = (key.wrapping_mul(0x9e37_79b9) >> (32 - CACHE_BITS)) as usize;
        let cached = &mut cache[slot];
        if cached.1 >= 0 && cached.0 == key {
            *index = cached.1;
            prev_index = cached.1;
            continue;
        }
        let blue = table[pixel[0] as usize];
        let green = table[pixel[1] as usize];
        let red = table[pixel[2] as usize];
        let alpha = table[pixel[3] as usize];
        // One entry per palette color, viewed as its own four-channel array: the
        // channels are then constant-offset loads from that entry instead of a
        // `color * 4 + c` address and a bounds check per channel. A trailing
        // group of fewer than four is dropped, exactly as the old
        // `1..colors.len() / 4` range dropped it, and the entries arrive in the
        // same order, so every load is the same `f32` and the comparisons run in
        // the same sequence.
        let (entries, _remainder) = colors.as_chunks::<4>();
        // The alpha term is `a*a` with `a = alpha - entry[3]`. When every palette
        // entry carries the same alpha bits as the pixel, `a` is exactly `0.0`
        // for every candidate, so `a*a` is `+0.0` and `(partial + r*r) + 0.0` is
        // bitwise `partial + r*r` (the sum of squares is never `-0.0`). The scan
        // then runs without the alpha load, the subtraction, the square and the
        // add, and every comparison sees the same `f32` it saw before. The
        // palette's alpha is uniform on most calls and the raster's alpha equals
        // it on most pixels, so this is the common case; anything else takes the
        // general scan unchanged.
        let (best, _) = if alpha.to_bits() == palette_alpha {
            scan_entries::<false>(entries, blue, green, red, alpha)
        } else {
            scan_entries::<true>(entries, blue, green, red, alpha)
        };
        *index = best;
        prev_index = best;
        *cached = (key, best);
    }
}

/// Assign engine-format BGRA pixels to an explicitly supplied palette.
pub fn assign_palette(bgra: &[u8], palette: &[f64]) -> Result<Vec<i32>, InputError> {
    let colors = read_palette_input(bgra, palette)?;
    let mut indices = vec![0i32; bgra.len() / 4];
    assign_indices(bgra, &colors, &mut indices);
    Ok(indices)
}

/// Unweighted palette score (`total`, `error`, `colors`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteScore {
    pub total: f32,
    pub error: f32,
    pub colors: f32,
}

/// Score explicit unweighted assignments, without changing the palette or map.
pub fn score_unweighted_palette(bgra: &[u8], palette: &[f64], indices: &[i64], cost_per_color: f64) -> Result<PaletteScore, InputError> {
    let input = read_palette_score_input(bgra, palette, indices, cost_per_color)?;
    Ok(score_prepared_unweighted_palette(bgra, &input.colors, &input.indices, input.cost_per_color))
}

pub fn score_prepared_unweighted_palette(bgra: &[u8], colors: &[f32], indices: &[i32], cost_per_color: f64) -> PaletteScore {
    let color_count = colors.len() / 4;
    // `byte_to_unit` reaches the table through its `LazyLock`, so written per
    // channel this loop pays four once-flag loads and four branches per pixel
    // on top of the four table loads. Taking the table's reference once per
    // call leaves exactly the four loads: the same table, the same
    // `byte as usize` index and the same `f32` values, so each channel read is
    // unchanged and the score is bit-identical. (The same hoist is the
    // production form in `assign_indices` and `refine_prepared`.)
    let table = &*BYTE_TO_UNIT;
    let mut sum = 0.0f32;
    for (pixel, &index) in indices.iter().enumerate() {
        let offset = pixel * 4;
        sum += distance(colors, index as usize * 4, table[bgra[offset] as usize], table[bgra[offset + 1] as usize],
            table[bgra[offset + 2] as usize], table[bgra[offset + 3] as usize]);
    }
    // An x87 logarithm combined with an SSE float32 color penalty.
    let error = (1.0 + sum as f64).log10() as f32;
    let color_cost = (color_count as f64 - 2.0) as f32 * cost_per_color as f32;
    PaletteScore { total: error + color_cost, error, colors: color_cost }
}

/// Validated anti-alias measurement input (`readMeasurementInput`).
pub fn measure_anti_alias(width: i64, height: i64, bgra: &[u8]) -> Result<Vec<u8>, InputError> {
    validate_bgra(bgra)?;
    if width < 2 || height < 2 || width > 2147483647 || height > 2147483647 || width * height * 4 != bgra.len() as i64 {
        return Err(InputError::new("Measurement needs dimensions of at least 2 by 2 with exactly width * height BGRA pixels."));
    }
    Ok(calculate_measurement(width as usize, height as usize, bgra))
}

#[inline]
fn measurement_contrast(bgra: &[u8], center: usize, neighbor: usize) -> f64 {
    // Double-precision squares of float32 channel differences.
    // `byte_to_unit` reaches the table through its `LazyLock`, so written per
    // channel this kernel pays eight once-flag loads and eight branches per
    // pixel on top of the eight table loads. Taking the table's reference once
    // per call leaves exactly the eight loads: the same table, the same
    // `byte as usize` index and the same `f32` values, so each difference and
    // the f64 sum are bit-identical.
    let table = &*BYTE_TO_UNIT;
    let mut contrast = 0.0f64;
    for channel in 0..4 {
        let difference = (table[bgra[neighbor + channel] as usize] - table[bgra[center + channel] as usize]) as f64;
        contrast += difference * difference;
    }
    contrast
}

/// Internal contrast kernel; callers own validated raster dimensions and bytes.
pub fn calculate_measurement(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
    let mut measurement = vec![0u8; width * height];
    let normalization = 12f64.sqrt();
    for y in 0..height {
        for x in 0..width {
            let center = (y * width + x) * 4;
            let top = ((if y == 0 { 1 } else { y.wrapping_sub(1) }) * width + x) * 4;
            let bottom = ((if y == height - 1 { y.wrapping_sub(1) } else { y + 1 }) * width + x) * 4;
            let left = (y * width + if x == 0 { 1 } else { x.wrapping_sub(1) }) * 4;
            let right = (y * width + if x == width - 1 { x.wrapping_sub(1) } else { x + 1 }) * 4;
            let mut total = measurement_contrast(bgra, center, top);
            total += measurement_contrast(bgra, center, bottom);
            total += measurement_contrast(bgra, center, left);
            total += measurement_contrast(bgra, center, right);
            measurement[y * width + x] = (total.sqrt() * 255.0 / normalization).min(255.0).max(0.0).trunc() as u8;
        }
    }
    measurement
}

/// Edge weights in the original multiplication order (`measurementWeights`).
pub fn measurement_weights(measurement: &[u8], edge_scale: f64, edge_floor: f64) -> Vec<f64> {
    measurement.iter().map(|&value| js_max(1.0 - value as f64 * edge_scale * (1.0 / 255.0), edge_floor)).collect()
}

/// `Math.max` for two operands, propagating NaN.
#[inline]
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a > b || (a == b && b.is_sign_negative()) { a } else { b }
}

#[derive(Debug, Clone, Copy)]
struct Peak {
    weight: f64,
    color: [u8; 4],
}

fn merge_peak_runs(left: &[Peak], right: &[Peak]) -> Vec<Peak> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        let (a, b) = (&left[left_index], &right[right_index]);
        // Preserve the original non-lexicographic channel test.
        if a.weight < b.weight || (a.weight == b.weight && (0..4).any(|i| a.color[i] < b.color[i])) {
            merged.push(right[right_index]);
            right_index += 1;
        } else {
            merged.push(left[left_index]);
            left_index += 1;
        }
    }
    merged.extend_from_slice(&left[left_index..]);
    merged.extend_from_slice(&right[right_index..]);
    merged
}

fn order_peaks(peaks: Vec<Peak>) -> Vec<Peak> {
    let mut runs: Vec<Vec<Peak>> = vec![Vec::new(); 26];
    let mut used = 0usize;
    for peak in peaks {
        let mut carry = vec![peak];
        let mut run = 0;
        while run < used && !runs[run].is_empty() {
            carry = merge_peak_runs(&runs[run], &carry);
            runs[run] = Vec::new();
            run += 1;
        }
        if run >= runs.len() { runs.resize(run + 1, Vec::new()); }
        runs[run] = carry;
        if run == used { used += 1; }
    }
    for run in 1..used {
        runs[run] = merge_peak_runs(&runs[run], &runs[run - 1]);
    }
    if used == 0 { Vec::new() } else { std::mem::take(&mut runs[used - 1]) }
}

fn neighbor_offsets(connected: bool) -> Vec<[i64; 4]> {
    (0..81i64).map(|index| [index % 3 - 1, index / 3 % 3 - 1, index / 9 % 3 - 1, index / 27 - 1])
        .filter(|offset| offset.iter().any(|&v| v != 0))
        .filter(|offset| connected || offset.iter().map(|v| v.abs()).sum::<i64>() == 1)
        .collect()
}

/// Ordered histogram peaks: BGRA bin centers and their weights.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistogramPeaks {
    pub bgra: Vec<u8>,
    pub weights: Vec<f64>,
}

fn validate_histogram_controls(bins: usize) -> Result<(), InputError> {
    if bins != 8 && bins != 16 { return Err(InputError::new("Histogram bins must be 8 or 16.")); }
    Ok(())
}

/// Find and order four-dimensional color-histogram peaks as the original does.
pub fn find_histogram_peaks(histogram: &[f64], bins: usize, connected: bool) -> Result<HistogramPeaks, InputError> {
    validate_histogram_controls(bins)?;
    if histogram.len() != bins.pow(4) { return Err(InputError::new("Histogram must contain bins^4 weights.")); }
    if histogram.iter().any(|w| !w.is_finite() || *w < 0.0) {
        return Err(InputError::new("Histogram weights must be finite and nonnegative."));
    }
    Ok(detect_histogram_peaks(histogram, bins, connected))
}

/// The peak test for one bin: `Some` when `weight` is a local maximum among the
/// in-bounds neighbors. The dense scan and the touched-bin walk share it, so the
/// two cannot disagree about which bins are peaks.
#[inline]
fn histogram_peak_at(histogram: &[f64], n: i64, neighbors: &[[i64; 4]], index: i64, weight: f64) -> Option<Peak> {
    let (b, g, r, a) = (index % n, index / n % n, index / (n * n) % n, index / (n * n * n));
    for &[db, dg, dr, da] in neighbors {
        let (nb, ng, nr, na) = (b + db, g + dg, r + dr, a + da);
        if nb < 0 || nb >= n || ng < 0 || ng >= n || nr < 0 || nr >= n || na < 0 || na >= n { continue; }
        if histogram[(((na * n + nr) * n + ng) * n + nb) as usize] > weight { return None; }
    }
    let step = 256 / n;
    let half = 128 / n;
    Some(Peak { weight, color: [b, g, r, a].map(|v| (v * step + half) as u8) })
}

fn finish_peaks(peaks: Vec<Peak>) -> HistogramPeaks {
    let ordered = order_peaks(peaks);
    HistogramPeaks {
        bgra: ordered.iter().flat_map(|peak| peak.color).collect(),
        weights: ordered.iter().map(|peak| peak.weight).collect(),
    }
}

/// Internal peak kernel; callers own validated histogram storage and controls.
pub fn detect_histogram_peaks(histogram: &[f64], bins: usize, connected: bool) -> HistogramPeaks {
    let neighbors = neighbor_offsets(connected);
    let n = bins as i64;
    let mut peaks = Vec::new();
    for (index, &weight) in histogram.iter().enumerate() {
        if weight < 2.0 { continue; }
        if let Some(peak) = histogram_peak_at(histogram, n, &neighbors, index as i64, weight) { peaks.push(peak); }
    }
    finish_peaks(peaks)
}

/// The bins a histogram build touched, for `detect_histogram_peaks_touched`.
/// Recording stops once the list is long enough that the dense scan is the
/// cheaper path, so a dense image pays a bounded number of pushes and then
/// falls back, instead of building a list as long as its pixel count.
pub(crate) struct TouchedBins {
    pub(crate) indices: Vec<u32>,
    cap: usize,
}

impl TouchedBins {
    pub(crate) fn new(bins: usize) -> Self {
        Self { indices: Vec::new(), cap: bins.pow(4) / 4 + 1 }
    }

    /// Add `weight` to `histogram[index]`, recording the bin the first time it
    /// is touched. An untouched bin is `0.0`, and a NaN sum never compares
    /// equal to it, so both are recorded again on later touches; the kernel's
    /// sort drops the repeats.
    #[inline]
    pub(crate) fn add(&mut self, histogram: &mut [f64], index: usize, weight: f64) {
        let slot = &mut histogram[index];
        if self.indices.len() < self.cap && (*slot == 0.0 || slot.is_nan()) {
            self.indices.push(index as u32);
        }
        *slot += weight;
    }
}

/// The same kernel over the bins a caller recorded as touched. A color
/// histogram is almost entirely zero — the dense scan reads all `bins^4` bins to
/// find the few that are not — so a caller that builds the histogram itself
/// hands over the bins it touched instead. The list is sorted and deduplicated
/// here, so the walk visits bins in the dense scan's ascending order and the
/// peak sequence, and therefore `order_peaks`' tie handling, is identical. A
/// histogram with more than a quarter of its bins touched is cheaper to scan
/// densely, so that case falls back to `detect_histogram_peaks`.
pub(crate) fn detect_histogram_peaks_touched(histogram: &[f64], bins: usize, connected: bool, mut touched: Vec<u32>) -> HistogramPeaks {
    if touched.len() * 4 > histogram.len() { return detect_histogram_peaks(histogram, bins, connected); }
    touched.sort_unstable();
    touched.dedup();
    let neighbors = neighbor_offsets(connected);
    let n = bins as i64;
    let mut peaks = Vec::new();
    for &index in &touched {
        let weight = histogram[index as usize];
        if weight < 2.0 { continue; }
        if let Some(peak) = histogram_peak_at(histogram, n, &neighbors, index as i64, weight) { peaks.push(peak); }
    }
    finish_peaks(peaks)
}

/// Build the weighted or unweighted histogram and detect its peaks.
pub fn prepare_histogram_peaks(bgra: &[u8], bins: usize, connected: bool, pixel_weights: Option<&[f64]>) -> HistogramPeaks {
    let mut histogram = vec![0.0f64; bins.pow(4)];
    let mut touched = TouchedBins::new(bins);
    let shift = if bins == 8 { 5 } else { 4 };
    for (pixel, chunk) in bgra.chunks_exact(4).enumerate() {
        let index = ((((chunk[3] >> shift) as usize * bins + (chunk[2] >> shift) as usize) * bins
            + (chunk[1] >> shift) as usize) * bins) + (chunk[0] >> shift) as usize;
        touched.add(&mut histogram, index, pixel_weights.map_or(1.0, |weights| weights[pixel]));
    }
    detect_histogram_peaks_touched(&histogram, bins, connected, touched.indices)
}

/// Final palette quantization after scoring (`finalizePalette`). The x87
/// multiply/add use double precision.
pub fn finalize_palette_colors(colors: &mut [f32]) {
    for channel in colors.iter_mut() {
        *channel = byte_to_unit(((*channel as f64 * 255.0 + 0.5).trunc() as i64 & 255) as u8);
    }
}

/// Application-owned random stream. Clones share the stream with documents.
#[derive(Clone, Debug)]
pub struct CoreSession { random_state: std::rc::Rc<std::cell::Cell<u32>> }
impl Default for CoreSession {
    fn default() -> Self { Self { random_state: std::rc::Rc::new(std::cell::Cell::new(1)) } }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedPalette { pub palette: Vec<f32>, pub indices: Vec<i32>, pub weights: Vec<f64>, pub passes: usize }
#[derive(Clone, Debug, PartialEq)]
pub struct AnalyzedPalette { pub color_count: usize, pub generated: GeneratedPalette, pub score: PaletteScore, pub random_state: u32 }
#[derive(Clone, Debug)]
pub struct ColorAnalysis { pub candidates: Vec<AnalyzedPalette>, pub peak_weights: Vec<f64>, pub features: [f64;62], pub classification: crate::automatic_colors::PaletteCountDecision }
#[derive(Clone, Copy, Debug)]
pub struct AnalysisControls { pub width: usize, pub height: usize, pub bins: usize, pub connected: bool, pub anti_alias: bool, pub cost_per_color: f64, pub edge_scale: f64, pub edge_floor: f64 }
pub type PaletteObserver<'a> = dyn FnMut(usize, &[f32], Option<&[i32]>) -> Result<(),String> + 'a;
#[derive(Clone, Debug)]
pub struct PreparedPaletteAnalysis<'a> { pub controls: AnalysisControls, pub peaks: HistogramPeaks, measurement: Option<std::borrow::Cow<'a,[u8]>>, pixel_weights: Option<Vec<f64>> }
impl<'a> PreparedPaletteAnalysis<'a> {
    pub fn new(bgra:&[u8],controls:AnalysisControls)->Result<Self,String>{
        Self::validate(bgra,controls)?;
        Ok(Self::from_prepared(bgra,controls))
    }
    fn validate(bgra:&[u8],controls:AnalysisControls)->Result<(),String>{
        validate_bgra(bgra).map_err(|e|e.to_string())?;
        validate_histogram_controls(controls.bins).map_err(|e|e.to_string())?;
        if controls.width==0 || controls.height==0 || controls.width>i32::MAX as usize || controls.height>i32::MAX as usize || controls.width.checked_mul(controls.height).is_none_or(|n|n>i32::MAX as usize || n*4!=bgra.len()){
            return Err("Automatic color analysis needs positive int32 dimensions and exactly width * height BGRA pixels.".into())
        }
        if controls.anti_alias && (controls.width<2||controls.height<2){return Err("antiAlias must be boolean, and anti-alias analysis needs dimensions of at least 2 by 2.".into())}
        if !(controls.cost_per_color as f32).is_finite(){return Err("costPerColor must be a finite float32 number.".into())}
        Ok(())
    }
    /// Internal document preparation also accepts the native empty-filter result.
    pub(crate) fn from_prepared(bgra:&[u8],controls:AnalysisControls)->Self{
        let measurement=controls.anti_alias.then(||std::borrow::Cow::Owned(calculate_measurement(controls.width,controls.height,bgra)));
        Self::from_measurement(bgra,controls,measurement,controls.width)
    }
    /// Classification keeps the image-feature measurement across raster reduction.
    /// Histogram and membership use its flat prefix; refinement uses its original stride.
    pub(crate) fn from_retained_measurement(bgra:&[u8],controls:AnalysisControls,measurement:&'a [u8],measurement_width:usize)->Result<Self,String>{
        Self::validate(bgra,controls)?;
        Ok(Self::from_measurement(bgra,controls,controls.anti_alias.then_some(std::borrow::Cow::Borrowed(measurement)),measurement_width))
    }
    fn from_measurement(bgra:&[u8],controls:AnalysisControls,measurement:Option<std::borrow::Cow<'a,[u8]>>,measurement_width:usize)->Self{
        let mut pixel_weights=measurement.as_ref().map(|m|measurement_weights(&m[..bgra.len()/4],controls.edge_scale,controls.edge_floor));
        let peaks=prepare_histogram_peaks(bgra,controls.bins,controls.connected,pixel_weights.as_deref());
        if measurement_width!=controls.width{
            if let (Some(measurement),Some(weights))=(&measurement,&mut pixel_weights){
                for (y,row) in weights.chunks_exact_mut(controls.width).enumerate(){
                    for (x,weight) in row.iter_mut().enumerate(){
                        *weight=js_max(1.0-measurement[y*measurement_width+x] as f64*controls.edge_scale*(1.0/255.0),controls.edge_floor);
                    }
                }
            }
        }
        Self{controls,peaks,measurement,pixel_weights}
    }
    fn generate(&self,session:&CoreSession,bgra:&[u8],count:usize,progress:Option<&mut PaletteObserver>)->Result<GeneratedPalette,String>{
        if count==0 || count>i32::MAX as usize{return Err("colorCount must be a positive int32 integer.".into())}
        let mut colors=vec![0.0;count*4];
        for (color,rgba) in colors.chunks_exact_mut(4).enumerate(){
            if color*4<self.peaks.bgra.len(){for channel in 0..4{rgba[channel]=byte_to_unit(self.peaks.bgra[color*4+channel])}}
            else{session.random_color(rgba)}
        }
        session.refine_prepared(bgra,colors,self.measurement.as_deref(),self.pixel_weights.as_deref(),progress)
    }
    pub fn evaluate(&self,session:&CoreSession,bgra:&[u8],count:usize,progress:Option<&mut PaletteObserver>)->Result<AnalyzedPalette,String>{
        let mut generated=self.generate(session,bgra,count,progress)?;
        let c=self.controls;
        let score=if c.anti_alias {let s=crate::blending::score_prepared_weighted_palette(c.width,c.height,bgra,&generated.palette,&generated.indices,c.cost_per_color);PaletteScore{total:s.total,error:s.error,colors:s.colors}}
            else{score_prepared_unweighted_palette(bgra,&generated.palette,&generated.indices,c.cost_per_color)};
        finalize_palette_colors(&mut generated.palette);
        Ok(AnalyzedPalette{color_count:count,generated,score,random_state:session.random_state()})
    }
    pub fn classify(&self,bgra:&[u8],candidates:&[AnalyzedPalette])->Result<([f64;62],crate::automatic_colors::PaletteCountDecision),String>{
        let candidates:Vec<_>=candidates.iter().map(|c|crate::automatic_colors::PaletteCandidate{weights:c.generated.weights.clone(),score:c.score}).collect();
        let features=crate::automatic_colors::build_palette_features(bgra,&candidates,&self.peaks.weights)?;
        let classification=crate::automatic_colors::classify_palette_features(&features);Ok((features,classification))
    }
    pub(crate) fn analyze(self,session:&CoreSession,bgra:&[u8])->Result<ColorAnalysis,String>{
        let mut candidates=Vec::with_capacity(11);for count in 2..=12{candidates.push(self.evaluate(session,bgra,count,None)?)}
        let (features,classification)=self.classify(bgra,&candidates)?;Ok(ColorAnalysis{candidates,peak_weights:self.peaks.weights,features,classification})
    }
}
fn accumulate_means(bgra: &[u8], indices: &[i32], weights: Option<&[f64]>, means: &mut [f32], counts: &mut [f32]) {
    let (pixels, _) = bgra.as_chunks::<4>();
    let (nodes, _) = means.as_chunks_mut::<4>();
    assert!(indices.len() <= pixels.len(), "the mean pass needs four bytes per pixel");
    let samples = pixels.iter().zip(indices);
    match weights {
        None => for (pixel, &index) in samples {
            let color = index as usize;
            for (sum, &byte) in nodes[color].iter_mut().zip(pixel) {
                *sum += byte as f32 * BYTE_SCALE;
            }
            counts[color] += 1.0;
        },
        Some(weights) => for (i, (pixel, &index)) in samples.enumerate() {
            let color = index as usize;
            let weight = weights[i] as f32;
            for (sum, &byte) in nodes[color].iter_mut().zip(pixel) {
                *sum += byte as f32 * BYTE_SCALE * weight;
            }
            counts[color] += weight;
        },
    }
}

impl CoreSession {
    pub fn random_state(&self)->u32{self.random_state.get()}
    pub fn next_random(&self)->f64{let state=self.random_state.get()*2003%9973;self.random_state.set(state);state as f64*(1.0/9973.0)}
    fn random_color(&self,rgba:&mut[f32]){rgba[2]=self.next_random() as f32;rgba[1]=self.next_random() as f32;rgba[0]=self.next_random() as f32;rgba[3]=1.0}
    pub fn create_document(&self)->crate::image_document::ImageDocument{crate::image_document::ImageDocument::new(self.clone())}
    pub fn smooth_contours(&self,input:&crate::contour_smoothing::SmoothContourInput)->Result<crate::contour_smoothing::SmoothedContours,String>{
        let before=self.random_state();let result=crate::contour_smoothing::smooth_contour_geometry(input,&mut||self.next_random(),None);
        if result.is_err(){self.random_state.set(before)}result
    }
    pub fn refine_unweighted(&self,bgra:&[u8],palette:&[f64])->Result<GeneratedPalette,String>{
        let input=read_palette_input(bgra,palette).map_err(|e|e.to_string())?;
        self.refine_prepared(bgra,input,None,None,None)
    }
    pub fn refine_weighted(&self,width:i64,height:i64,bgra:&[u8],palette:&[f64])->Result<GeneratedPalette,String>{
        let measurement=measure_anti_alias(width,height,bgra).map_err(|e|e.to_string())?;
        let colors=crate::pixel_input::read_palette_colors(palette).map_err(|e|e.to_string())?;
        let weights=measurement_weights(&measurement,50.0,0.01);self.refine_prepared(bgra,colors,Some(&measurement),Some(&weights),None)
    }
    pub fn generate_unweighted(&self,bgra:&[u8],count:usize,bins:usize,connected:bool)->Result<GeneratedPalette,String>{
        validate_bgra(bgra).map_err(|e|e.to_string())?;validate_histogram_controls(bins).map_err(|e|e.to_string())?;
        if count==0||count>i32::MAX as usize{return Err("colorCount must be a positive int32 integer.".into())}
        let peaks=prepare_histogram_peaks(bgra,bins,connected,None);let mut colors=vec![0.0;count*4];
        for (i,color) in colors.chunks_exact_mut(4).enumerate(){if i*4<peaks.bgra.len(){for c in 0..4{color[c]=byte_to_unit(peaks.bgra[i*4+c])}}else{self.random_color(color)}}
        let mut result=self.refine_prepared(bgra,colors,None,None,None)?;finalize_palette_colors(&mut result.palette);Ok(result)
    }
    pub fn generate_weighted(&self,bgra:&[u8],count:usize,controls:AnalysisControls)->Result<GeneratedPalette,String>{
        let prepared=PreparedPaletteAnalysis::new(bgra,AnalysisControls{anti_alias:true,..controls})?;
        let mut generated=prepared.generate(self,bgra,count,None)?;finalize_palette_colors(&mut generated.palette);Ok(generated)
    }
    pub fn initialize_contours(&self,input:&crate::contour_initialization::InitializationInput)->Result<Vec<f64>,String>{
        crate::contour_initialization::initialize_contour_coordinates(input,&mut||self.next_random())
    }
    pub fn analyze_image(&self,input:&serde_json::Value)->Result<crate::image_pipeline::AnalyzedImage,String>{crate::image_pipeline::analyze_image(self,input)}
    pub fn analyze_color_counts(&self,bgra:&[u8],controls:AnalysisControls)->Result<ColorAnalysis,String>{
        PreparedPaletteAnalysis::new(bgra,controls)?.analyze(self,bgra)
    }
    fn refine_prepared(&self,bgra:&[u8],mut colors:Vec<f32>,measurement:Option<&[u8]>,pixel_weights:Option<&[f64]>,mut progress:Option<&mut PaletteObserver>)->Result<GeneratedPalette,String>{
        let mut indices=vec![0;bgra.len()/4];let mut means=vec![0.0f32;colors.len()];let mut counts=vec![0.0f32;colors.len()/4];let mut weights=vec![0.0;counts.len()];let mut converged=false;let mut passes=0;
        for iteration in 0..10{
            if let Some(callback)=progress.as_deref_mut(){callback(iteration,&colors,if iteration==0{None}else{Some(&indices)})?}
            assign_indices(bgra,&colors,&mut indices);passes+=1;
            // The weight per pixel is `1.0` when there is no measurement, and the
            // `map_or` was evaluated per pixel. Hoisting it to a `match` outside
            // the loop gives the unweighted path a plain increment: `x + 1.0` is
            // the same sum in the same pixel order, and the weighted arm is the
            // same expression it always was, so every `f64` accumulation is
            // unchanged.
            if iteration==9||converged{weights.fill(0.0);match measurement{
                None=>for &index in indices.iter(){weights[index as usize]+=1.0},
                Some(m)=>for (pixel,&index) in indices.iter().enumerate(){weights[index as usize]+=m[pixel] as f64*(1.0/255.0)},
            }}
            if converged{break}
            means.fill(0.0);counts.fill(0.0);
            accumulate_means(bgra, &indices, pixel_weights, &mut means, &mut counts);
            for (color,&count) in counts.iter().enumerate(){let m=&mut means[color*4..color*4+4];if count>0.0{let reciprocal=(1.0/count as f64) as f32;for value in m{*value*=reciprocal}}else{self.random_color(m)}}
            let mut change=0.0f64;for (a,b) in colors.iter().zip(&means){change=js_max(change,(*a-*b).abs() as f64)}
            colors.copy_from_slice(&means);converged=change<(0.01f32 as f64);
        }
        for (color,&count) in counts.iter().enumerate(){if count==0.0{colors[color*4..color*4+4].fill(0.0)}}
        Ok(GeneratedPalette{palette:colors,indices,weights,passes})
    }
}

#[cfg(test)]
mod assignment_equivalence {
    //! The production `assign_indices` (memo + blue/green partial-sum
    //! short-circuit) against the retained, verbatim original: every palette
    //! entry evaluated with the full four-channel `distance`, no memo, no
    //! early exit. Assignments are compared exactly, so a single changed
    //! choice fails.
    use super::*;

    /// The original loop, kept verbatim as the oracle.
    fn assign_indices_reference(bgra: &[u8], colors: &[f32], indices: &mut [i32]) {
        for (pixel, index) in indices.iter_mut().enumerate() {
            let offset = pixel * 4;
            let blue = byte_to_unit(bgra[offset]);
            let green = byte_to_unit(bgra[offset + 1]);
            let red = byte_to_unit(bgra[offset + 2]);
            let alpha = byte_to_unit(bgra[offset + 3]);
            let mut best = 0;
            let mut best_cost = distance(colors, 0, blue, green, red, alpha);
            for color in 1..colors.len() / 4 {
                let cost = distance(colors, color * 4, blue, green, red, alpha);
                if cost < best_cost {
                    best = color as i32;
                    best_cost = cost;
                }
            }
            *index = best;
        }
    }

    fn check(bgra: &[u8], colors: &[f32]) {
        let mut got = vec![0i32; bgra.len() / 4];
        let mut want = vec![0i32; bgra.len() / 4];
        assign_indices(bgra, colors, &mut got);
        assign_indices_reference(bgra, colors, &mut want);
        assert_eq!(got, want, "assignment differs for {} colors", colors.len() / 4);
    }

    /// Deterministic xorshift so the test needs no crate and no clock.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
        }
        fn byte(&mut self) -> u8 { (self.next() >> 24) as u8 }
    }

    #[test]
    fn short_circuit_matches_the_retained_full_distance_loop() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let mut comparisons = 0usize;
        // Palettes from 1 to 12 colors (the engine's own range), pixels drawn
        // from a small alphabet so the memo hits, misses and collisions all run.
        for color_count in 1..=12usize {
            for alphabet in [2usize, 3, 5, 16, 64, 256] {
                for _ in 0..12 {
                    let colors: Vec<f32> = (0..color_count * 4)
                        .map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32)
                        .collect();
                    let pixels = 1 + (rng.next() % 300) as usize;
                    let bgra: Vec<u8> = (0..pixels * 4)
                        .map(|_| (rng.next() % alphabet as u64) as u8)
                        .collect();
                    check(&bgra, &colors);
                    comparisons += 1;
                }
            }
        }
        assert!(comparisons >= 800, "the sweep must not thin out: {comparisons}");
    }

    #[test]
    fn short_circuit_matches_on_ties_duplicates_and_extremes() {
        // Identical palette entries (ties), all-zero and all-255 pixels, and a
        // palette that repeats one color: the strict `<` must keep the first.
        let cases: [(&[u8], &[f32]); 5] = [
            (&[0, 0, 0, 0, 255, 255, 255, 255], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            (&[0, 0, 0, 0, 255, 255, 255, 255], &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
            (&[7, 7, 7, 7], &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.5]),
            (&[0, 255, 0, 255, 255, 0, 255, 0], &[0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0]),
            (&[1, 2, 3, 4, 4, 3, 2, 1, 0, 0, 0, 0], &[0.0, 0.0, 0.0, 0.0]),
        ];
        for (bgra, colors) in cases { check(bgra, colors); }
    }

    /// The call-scoped table reference must be the *same* table
    /// `byte_to_unit` reads, and it must stay valid across calls: the harness
    /// runs many `assign_indices` passes, so a stale table reference would be a
    /// silent misassignment the oracle above would not catch on its own.
    #[test]
    fn the_call_scoped_table_is_the_one_byte_to_unit_reads() {
        let table = &*BYTE_TO_UNIT;
        for byte in 0..=255u8 {
            assert_eq!(table[byte as usize].to_bits(), byte_to_unit(byte).to_bits(),
                "table entry {byte} differs from byte_to_unit");
        }
        let mut rng = Rng(0xa5a5_5a5a_1234_9876);
        let colors: Vec<f32> = (0..9 * 4).map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32).collect();
        let bgra: Vec<u8> = (0..512 * 4).map(|_| rng.byte()).collect();
        let mut first = vec![0i32; bgra.len() / 4];
        assign_indices(&bgra, &colors, &mut first);
        // Intervening work that allocates and touches the table's cache lines.
        let noise: Vec<u8> = (0..4096 * 4).map(|_| rng.byte()).collect();
        let mut other = vec![0i32; noise.len() / 4];
        assign_indices(&noise, &colors, &mut other);
        let mut second = vec![0i32; bgra.len() / 4];
        assign_indices(&bgra, &colors, &mut second);
        assert_eq!(first, second, "a repeated call assigned differently");
        let mut want = vec![0i32; bgra.len() / 4];
        assign_indices_reference(&bgra, &colors, &mut want);
        assert_eq!(second, want, "the hoisted table changed an assignment");
    }

    /// The four-channel entry view must see exactly the entries the old
    /// `1..colors.len() / 4` range did — including a palette whose channel count
    /// is not a multiple of four, where the trailing partial group is skipped.
    ///
    /// The control is a reader that folds those trailing channels into one more
    /// entry. It is built to bite on every shape: the whole entries are all
    /// white, the pixel is black, and the phantom entry is black too, so a
    /// reader that includes it picks the phantom at cost 0 while the production
    /// loop must keep the first whole entry (all whole entries tie at 4.0 and
    /// the strict `<` keeps the earliest). A tie at cost 0 cannot mask it,
    /// because no whole entry is black.
    #[test]
    fn a_partial_trailing_group_is_skipped_exactly_like_the_range() {
        fn reads_trailing(colors: &[f32], blue: f32, green: f32, red: f32, alpha: f32) -> i32 {
            let whole = colors.len() / 4;
            let mut best = 0;
            let mut best_cost = distance(colors, 0, blue, green, red, alpha);
            let mut entry = [0.0f32; 4];
            for (channel, value) in colors[whole * 4..].iter().enumerate() { entry[channel] = *value; }
            let b = blue - entry[0];
            let g = green - entry[1];
            let partial = b * b + g * g;
            if partial < best_cost {
                let r = red - entry[2];
                let a = alpha - entry[3];
                let cost = (partial + r * r) + a * a;
                if cost < best_cost { best = whole as i32; best_cost = cost; }
            }
            best
        }
        let black = [0u8, 0, 0, 0];
        let mut shapes = 0usize;
        for whole in 1..=4usize {
            for extra in 1..=3usize {
                shapes += 1;
                // Every whole entry is white; the trailing channels are black.
                let mut colors = vec![1.0f32; whole * 4];
                colors.extend(std::iter::repeat(0.0f32).take(extra));

                let mut got = [0i32; 1];
                assign_indices(&black, &colors, &mut got);
                let mut want = [0i32; 1];
                assign_indices_reference(&black, &colors, &mut want);
                assert_eq!(got, want, "the entry view skipped the wrong groups for {whole}+{extra} channels");
                assert_eq!(got[0], 0, "the first whole entry must win the tie at 4.0");

                let (blue, green, red, alpha) = (byte_to_unit(0), byte_to_unit(0), byte_to_unit(0), byte_to_unit(0));
                assert_eq!(reads_trailing(&colors, blue, green, red, alpha), whole as i32,
                    "the trailing-group control must bite for {whole}+{extra} channels");
            }
        }
        assert_eq!(shapes, 12, "the sweep must cover every partial-group shape");
    }

    /// The guard must be re-derived per pixel. A version that carries
    /// `best_cost` across pixels (a stale guard) is the obvious way to get this
    /// wrong, and it must be caught here: if it were not, the equivalence tests
    /// above would be proving nothing about the guard.
    #[test]
    fn a_stale_guard_is_caught_by_the_retained_oracle() {
        fn stale(bgra: &[u8], colors: &[f32], indices: &mut [i32]) {
            let mut carried = f32::INFINITY;
            for (pixel, index) in indices.iter_mut().enumerate() {
                let offset = pixel * 4;
                let blue = byte_to_unit(bgra[offset]);
                let green = byte_to_unit(bgra[offset + 1]);
                let red = byte_to_unit(bgra[offset + 2]);
                let alpha = byte_to_unit(bgra[offset + 3]);
                let mut best = 0;
                for color in 0..colors.len() / 4 {
                    let base = color * 4;
                    let b = blue - colors[base];
                    let g = green - colors[base + 1];
                    let partial = b * b + g * g;
                    if partial >= carried { continue; }
                    let r = red - colors[base + 2];
                    let a = alpha - colors[base + 3];
                    let cost = (partial + r * r) + a * a;
                    if cost < carried || color == 0 { best = color as i32; carried = cost; }
                }
                *index = best;
            }
        }
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut caught = 0usize;
        for _ in 0..200 {
            let color_count = 2 + (rng.next() % 11) as usize;
            let colors: Vec<f32> = (0..color_count * 4)
                .map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32)
                .collect();
            let pixels = 1 + (rng.next() % 60) as usize;
            let bgra: Vec<u8> = (0..pixels * 4).map(|_| rng.byte()).collect();
            let mut got = vec![0i32; pixels];
            let mut want = vec![0i32; pixels];
            stale(&bgra, &colors, &mut got);
            assign_indices_reference(&bgra, &colors, &mut want);
            if got != want { caught += 1; }
        }
        assert!(caught >= 100, "the oracle must reject a stale guard: caught {caught}/200");
    }

    /// The alpha-free scan is only sound when the pixel's alpha bits equal every
    /// palette entry's alpha bits *and* that value is finite: then `a` is exactly
    /// `+0.0` for every candidate and `(partial + r*r) + 0.0` is bitwise
    /// `partial + r*r`. This pins the equivalence on the shapes that take the
    /// fast path, and the negative control below pins the shapes that must not.
    #[test]
    fn the_alpha_free_scan_matches_the_full_distance_on_uniform_alpha() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let mut cases = 0usize;
        for alpha_byte in [0u8, 1, 128, 254, 255] {
            for color_count in 1..=12usize {
                for _ in 0..6 {
                    // Every entry carries the pixel's alpha, so the fast path runs.
                    let alpha = byte_to_unit(alpha_byte);
                    let mut colors: Vec<f32> = (0..color_count * 4)
                        .map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32)
                        .collect();
                    for entry in colors.chunks_mut(4) { entry[3] = alpha; }
                    let pixels = 1 + (rng.next() % 200) as usize;
                    let mut bgra: Vec<u8> = (0..pixels * 4).map(|_| rng.byte()).collect();
                    for pixel in bgra.chunks_mut(4) { pixel[3] = alpha_byte; }
                    check(&bgra, &colors);
                    cases += 1;
                }
            }
        }
        assert!(cases >= 300, "the sweep must not thin out: {cases}");
    }

    /// Negative control: a palette whose shared alpha is NaN or infinite must
    /// take the general scan. `NaN - NaN` and `inf - inf` are NaN, not `+0.0`,
    /// so an alpha-free scan would compare a different `f32` and could pick a
    /// different index. The control is the fast path forced on, which must
    /// disagree with the oracle on at least one of these palettes.
    #[test]
    fn a_non_finite_palette_alpha_must_not_take_the_alpha_free_scan() {
        fn forced_alpha_free(bgra: &[u8], colors: &[f32], indices: &mut [i32]) {
            let table = &*BYTE_TO_UNIT;
            let (entries, _) = colors.as_chunks::<4>();
            for (pixel, index) in indices.iter_mut().enumerate() {
                let offset = pixel * 4;
                let (blue, green, red, alpha) = (
                    table[bgra[offset] as usize], table[bgra[offset + 1] as usize],
                    table[bgra[offset + 2] as usize], table[bgra[offset + 3] as usize]);
                *index = scan_entries::<false>(entries, blue, green, red, alpha).0;
            }
        }
        let mut caught = 0usize;
        for shared in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            // Two entries that differ only in a channel the alpha term cannot
            // mask: with a NaN alpha every cost is NaN, so the strict `<` keeps
            // entry 0, while the alpha-free scan compares real numbers.
            let colors = [0.0f32, 0.0, 0.0, shared, 1.0, 1.0, 1.0, shared];
            let bgra = [255u8, 255, 255, 255, 0, 0, 0, 255];
            let mut got = vec![0i32; 2];
            let mut want = vec![0i32; 2];
            forced_alpha_free(&bgra, &colors, &mut got);
            assign_indices_reference(&bgra, &colors, &mut want);
            if got != want { caught += 1; }
            // And the production path must agree with the oracle regardless.
            check(&bgra, &colors);
        }
        assert_eq!(caught, 3, "the forced fast path must be caught on every non-finite alpha");
    }

    /// The probe's sentinel is `u32::MAX`, so no pixel alpha may ever carry those
    /// bits: every `BYTE_TO_UNIT` entry is a finite value in `[0, 1]`, and none of
    /// them is a NaN. This is what lets a non-finite palette alpha fall to the
    /// general scan without an `is_finite` test on the hot path.
    #[test]
    fn no_byte_to_unit_entry_can_collide_with_the_probe_sentinel() {
        for byte in 0..=255u8 {
            let value = byte_to_unit(byte);
            assert!(value.is_finite(), "byte {byte} is not finite");
            assert!((0.0..=1.0).contains(&value), "byte {byte} is outside [0, 1]");
            assert_ne!(value.to_bits(), u32::MAX, "byte {byte} collides with the sentinel");
        }
    }

    /// The previous-key fast path must reuse exactly the index the memo holds
    /// for an adjacent repeat. The first pixel of a run is always assigned by
    /// the full lookup; only when the key repeats the immediately preceding
    /// pixel does the shortcut fire, and it must not need the cache at all.
    #[test]
    fn adjacent_repeats_reuse_the_previous_index() {
        // A palette whose best color for the black pixel is 1, so a reuse of
        // the initial `prev_index` (0) would be visible.
        let colors = [1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let black = [0u8, 0, 0, 0];
        let white = [255u8, 255, 255, 255];
        let mut bgra = Vec::new();
        for _ in 0..5 { bgra.extend_from_slice(&white); }
        for _ in 0..7 { bgra.extend_from_slice(&black); }
        bgra.extend_from_slice(&white);
        let mut got = vec![0i32; bgra.len() / 4];
        let mut want = vec![0i32; bgra.len() / 4];
        assign_indices(&bgra, &colors, &mut got);
        assign_indices_reference(&bgra, &colors, &mut want);
        assert_eq!(got, want);
        assert_eq!(got[5], 1, "the first black pixel must be assigned by the full lookup");
        assert!(got[5..12].iter().all(|&v| v == 1), "a reused index must not fall back to 0");
    }

    /// Two adjacent but different keys that share a memo slot: the fast path
    /// must not fire on a key mismatch, and the second key's full lookup must
    /// see the collision and recompute. Two different pixels with the same
    /// four bytes elsewhere in the raster must still find their own entry.
    #[test]
    fn adjacent_different_keys_with_the_same_slot_stay_apart() {
        const CACHE_BITS: u32 = 10;
        fn slot(key: u32) -> usize {
            (key.wrapping_mul(0x9e37_79b9) >> (32 - CACHE_BITS)) as usize
        }
        // Two distinct keys that land in the same memo slot; they are placed
        // next to each other in the raster, so the fast path sees a key change
        // into a slot that already holds the other key.
        let a = 0u32;
        let b = (1..1_000_000u32)
            .find(|&v| slot(v) == slot(a))
            .expect("the probe must find a colliding key");
        assert_ne!(a, b);
        // Colors 1 and 2 are exactly the two pixels, so each key's best is its
        // own entry and a leaked or reused index cannot pass.
        let a_bytes = a.to_le_bytes();
        let b_bytes = b.to_le_bytes();
        let mut colors = vec![0.0f32; 12];
        for channel in 0..4 {
            colors[4 + channel] = byte_to_unit(a_bytes[channel]);
            colors[8 + channel] = byte_to_unit(b_bytes[channel]);
        }
        let mut bgra = Vec::new();
        bgra.extend_from_slice(&a.to_le_bytes());
        bgra.extend_from_slice(&b.to_le_bytes());
        bgra.extend_from_slice(&a.to_le_bytes());
        bgra.extend_from_slice(&b.to_le_bytes());
        let mut got = vec![0i32; 4];
        let mut want = vec![0i32; 4];
        assign_indices(&bgra, &colors, &mut got);
        assign_indices_reference(&bgra, &colors, &mut want);
        assert_eq!(got, want, "collisions must not change assignments");
        assert_ne!(got[0], got[1], "the probe needs two different assignments");
        assert_eq!(got[0], got[2]);
        assert_eq!(got[1], got[3]);
    }

    /// The sentinel is outside the key domain: the very first pixel of a call,
    /// even when its key is all zeroes, must take the full lookup (and its
    /// index must be whatever the palette says, not the zero initial value).
    #[test]
    fn the_initial_sentinel_never_matches_the_first_key() {
        let colors = [1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let black = [0u8, 0, 0, 0];
        let mut got = [0i32; 1];
        assign_indices(&black, &colors, &mut got);
        assert_eq!(got[0], 1, "the first all-zero key must be looked up, not reused");
    }

    /// The previous key belongs to one call. Refinement hands the next pass a
    /// new palette; the same raster must be assigned under the new palette
    /// without any reuse of the old call's index.
    #[test]
    fn palette_changes_between_calls_do_not_reuse_the_old_index() {
        let black = [0u8, 0, 0, 0];
        let bgra = [black, black, black, black].concat();
        let first = [1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let second = [0.0f32, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let mut got = vec![0i32; 4];
        assign_indices(&bgra, &first, &mut got);
        assert_eq!(got, vec![1, 1, 1, 1]);
        assign_indices(&bgra, &second, &mut got);
        assert_eq!(got, vec![0, 0, 0, 0], "the second call must not reuse the first call's index");
        let mut want = vec![0i32; 4];
        assign_indices_reference(&bgra, &second, &mut want);
        assert_eq!(got, want);
    }

    #[test]
    fn short_circuit_matches_when_the_palette_has_one_color() {
        // The `for color in 1..1` loop never runs: the memo and the first
        // `distance` alone decide, and every pixel must take color 0.
        let colors = [0.25f32, 0.5, 0.75, 1.0];
        let bgra: Vec<u8> = (0..64u16).map(|v| v as u8).collect();
        let mut got = vec![0i32; bgra.len() / 4];
        assign_indices(&bgra, &colors, &mut got);
        assert!(got.iter().all(|&v| v == 0));
        check(&bgra, &colors);
    }
}

#[cfg(test)]
mod refinement_equivalence {
    //! `refine_prepared`'s mean accumulation against the retained original,
    //! which calls `byte_to_unit` per channel per pixel. The production loop
    //! hoists the `BYTE_TO_UNIT` table out of the pixel loop; the table is the
    //! same `LazyLock` and the same values, so every product and every float32
    //! accumulation must be bit-identical.
    use super::*;

    /// The original accumulation, kept verbatim as the oracle.
    fn means_reference(bgra: &[u8], indices: &[i32], pixel_weights: Option<&[f64]>, color_count: usize)
        -> (Vec<f32>, Vec<f32>) {
        let mut means = vec![0.0f32; color_count * 4];
        let mut counts = vec![0.0f32; color_count];
        for (pixel, &index) in indices.iter().enumerate() {
            let color = index as usize;
            let weight = pixel_weights.map_or(1.0, |w| w[pixel] as f32);
            for c in 0..4 { means[color * 4 + c] += byte_to_unit(bgra[pixel * 4 + c]) * weight }
            counts[color] += weight;
        }
        (means, counts)
    }

    fn means_production(bgra: &[u8], indices: &[i32], pixel_weights: Option<&[f64]>, color_count: usize)
        -> (Vec<f32>, Vec<f32>) {
        let mut means = vec![0.0; color_count * 4];
        let mut counts = vec![0.0; color_count];
        accumulate_means(bgra, indices, pixel_weights, &mut means, &mut counts);
        (means, counts)
    }

    /// The identity the production loop now relies on, over every byte: the
    /// table's own expression and the arithmetic form agree bit for bit, and
    /// `BYTE_SCALE` is the `f32` the table's scale rounds to.
    #[test]
    fn byte_scale_matches_the_table() {
        assert_eq!(BYTE_SCALE.to_bits(), (f32r(1.0 / 255.0) as f32).to_bits(),
            "BYTE_SCALE is not the table's own scale");
        for byte in 0..=255u8 {
            let table = BYTE_TO_UNIT[byte as usize];
            let arithmetic = byte as f32 * BYTE_SCALE;
            assert_eq!(table.to_bits(), arithmetic.to_bits(),
                "byte {byte}: table {} vs arithmetic {}", table, arithmetic);
        }
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
        }
    }

    /// The unweighted path drops the `* 1.0` multiply entirely. `x * 1.0` is
    /// the identity for every finite value, and the table holds only finite
    /// values, so the two paths must agree bit for bit.
    #[test]
    fn unweighted_path_matches_the_retained_multiply_by_one() {
        let mut rng = Rng(0x0f0f_1234_5678_9abc);
        let mut comparisons = 0usize;
        for color_count in 1..=12usize {
            for _ in 0..10 {
                let pixels = 1 + (rng.next() % 400) as usize;
                let bgra: Vec<u8> = (0..pixels * 4).map(|_| (rng.next() >> 33) as u8).collect();
                let indices: Vec<i32> = (0..pixels)
                    .map(|_| (rng.next() % color_count as u64) as i32).collect();
                let (m1, c1) = means_reference(&bgra, &indices, None, color_count);
                let (m2, c2) = means_production(&bgra, &indices, None, color_count);
                for (a, b) in m1.iter().zip(&m2) {
                    assert_eq!(a.to_bits(), b.to_bits(), "unweighted mean channel differs");
                }
                for (a, b) in c1.iter().zip(&c2) {
                    assert_eq!(a.to_bits(), b.to_bits(), "unweighted count differs");
                }
                comparisons += 1;
            }
        }
        assert!(comparisons >= 100, "the sweep must not thin out: {comparisons}");
    }

    #[test]
    fn hoisted_table_matches_the_retained_per_channel_lookup() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let mut comparisons = 0usize;
        for color_count in 1..=12usize {
            for weighted in [false, true] {
                for _ in 0..10 {
                    let pixels = 1 + (rng.next() % 400) as usize;
                    let bgra: Vec<u8> = (0..pixels * 4).map(|_| (rng.next() >> 33) as u8).collect();
                    let indices: Vec<i32> = (0..pixels)
                        .map(|_| (rng.next() % color_count as u64) as i32).collect();
                    let weights: Option<Vec<f64>> = weighted.then(|| {
                        (0..pixels).map(|_| (rng.next() % 1000) as f64 * 0.001).collect()
                    });
                    let (m1, c1) = means_reference(&bgra, &indices, weights.as_deref(), color_count);
                    let (m2, c2) = means_production(&bgra, &indices, weights.as_deref(), color_count);
                    assert_eq!(m1.len(), m2.len());
                    for (a, b) in m1.iter().zip(&m2) {
                        assert_eq!(a.to_bits(), b.to_bits(), "mean channel differs");
                    }
                    for (a, b) in c1.iter().zip(&c2) {
                        assert_eq!(a.to_bits(), b.to_bits(), "count differs");
                    }
                    comparisons += 1;
                }
            }
        }
        assert!(comparisons >= 200, "the sweep must not thin out: {comparisons}");
    }
}

#[cfg(test)]
mod score_table_equivalence {
    //! The call-scoped table reference in `score_prepared_unweighted_palette`
    //! against the retained per-channel `byte_to_unit` form. Both read the same
    //! `LazyLock`, so every channel value, every squared difference and the
    //! float32 sum must be bit-identical, including across repeated calls.
    use super::*;

    /// The retained original: `byte_to_unit` per channel per pixel.
    fn score_reference(bgra: &[u8], colors: &[f32], indices: &[i32], cost_per_color: f64) -> PaletteScore {
        let color_count = colors.len() / 4;
        let mut sum = 0.0f32;
        for (pixel, &index) in indices.iter().enumerate() {
            let offset = pixel * 4;
            sum += distance(colors, index as usize * 4, byte_to_unit(bgra[offset]), byte_to_unit(bgra[offset + 1]),
                byte_to_unit(bgra[offset + 2]), byte_to_unit(bgra[offset + 3]));
        }
        let error = (1.0 + sum as f64).log10() as f32;
        let color_cost = (color_count as f64 - 2.0) as f32 * cost_per_color as f32;
        PaletteScore { total: error + color_cost, error, colors: color_cost }
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
        }
        fn byte(&mut self) -> u8 { (self.next() >> 33) as u8 }
    }

    fn same_bits(a: PaletteScore, b: PaletteScore, what: &str) {
        assert_eq!(a.total.to_bits(), b.total.to_bits(), "total differs: {what}");
        assert_eq!(a.error.to_bits(), b.error.to_bits(), "error differs: {what}");
        assert_eq!(a.colors.to_bits(), b.colors.to_bits(), "color cost differs: {what}");
    }

    #[test]
    fn hoisted_score_table_matches_the_retained_per_channel_lookup() {
        let mut rng = Rng(0x51ed_2f04_c3b1_9a7e);
        let mut comparisons = 0usize;
        for color_count in 2..=12usize {
            for _ in 0..20 {
                let pixels = 1 + (rng.next() % 500) as usize;
                let bgra: Vec<u8> = (0..pixels * 4).map(|_| rng.byte()).collect();
                let colors: Vec<f32> = (0..color_count * 4)
                    .map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32)
                    .collect();
                let indices: Vec<i32> = (0..pixels)
                    .map(|_| (rng.next() % color_count as u64) as i32).collect();
                let cost_per_color = ((rng.next() % 10_000) as f64) * 0.0001;
                let got = score_prepared_unweighted_palette(&bgra, &colors, &indices, cost_per_color);
                let want = score_reference(&bgra, &colors, &indices, cost_per_color);
                same_bits(got, want, "random case");
                comparisons += 1;
            }
        }
        assert!(comparisons >= 200, "the sweep must not thin out: {comparisons}");
    }

    /// The table reference must be the *same* table `byte_to_unit` reads, and it
    /// must stay valid across calls: a stale table would silently score pixels
    /// wrong, and the random sweep above could not tell.
    #[test]
    fn the_score_table_is_the_one_byte_to_unit_reads() {
        let table = &*BYTE_TO_UNIT;
        for byte in 0..=255u8 {
            assert_eq!(table[byte as usize].to_bits(), byte_to_unit(byte).to_bits(),
                "table entry {byte} differs from byte_to_unit");
        }
        let mut rng = Rng(0x0d15_ea5e_5f00_1234);
        let colors: Vec<f32> = (0..9 * 4)
            .map(|_| (rng.byte() as f64 * f32r(1.0 / 255.0)) as f32).collect();
        let bgra: Vec<u8> = (0..512 * 4).map(|_| rng.byte()).collect();
        let indices: Vec<i32> = (0..512).map(|i| (i % 9) as i32).collect();
        let first = score_prepared_unweighted_palette(&bgra, &colors, &indices, 0.5);
        // Intervening scoring work that allocates and touches the table's lines.
        let noise: Vec<u8> = (0..4096 * 4).map(|_| rng.byte()).collect();
        let noise_indices: Vec<i32> = (0..4096).map(|i| (i % 9) as i32).collect();
        let _ = score_prepared_unweighted_palette(&noise, &colors, &noise_indices, 0.5);
        let second = score_prepared_unweighted_palette(&bgra, &colors, &indices, 0.5);
        same_bits(first, second, "a repeated call scored differently");
        same_bits(second, score_reference(&bgra, &colors, &indices, 0.5), "the hoisted table changed the score");
    }

    /// Non-finite channel data must score identically too: the hoist changes only
    /// how the table is reached, so NaN/inf propagation is unchanged.
    #[test]
    fn hoisted_score_matches_on_non_finite_palette_values() {
        let bgra: Vec<u8> = (0..16u8).map(|v| v.wrapping_mul(17)).collect();
        let indices: Vec<i32> = vec![0; 4];
        let palette = [0.0f32, f32::NAN, 1.0, 0.5];
        same_bits(score_prepared_unweighted_palette(&bgra, &palette, &indices, 0.0),
            score_reference(&bgra, &palette, &indices, 0.0), "NaN palette");
        let palette_inf = [f32::INFINITY, 0.0, f32::NEG_INFINITY, 1.0];
        same_bits(score_prepared_unweighted_palette(&bgra, &palette_inf, &indices, 0.0),
            score_reference(&bgra, &palette_inf, &indices, 0.0), "infinite palette");
    }
}

#[cfg(test)]
mod contrast_table_equivalence {
    //! The call-scoped table reference in `measurement_contrast` against the
    //! retained per-channel `byte_to_unit` form, and the whole measurement
    //! raster built from it.
    use super::*;

    fn contrast_reference(bgra: &[u8], center: usize, neighbor: usize) -> f64 {
        let mut contrast = 0.0f64;
        for channel in 0..4 {
            let difference = (byte_to_unit(bgra[neighbor + channel]) - byte_to_unit(bgra[center + channel])) as f64;
            contrast += difference * difference;
        }
        contrast
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
        }
        fn byte(&mut self) -> u8 { (self.next() >> 33) as u8 }
    }

    #[test]
    fn hoisted_contrast_matches_the_retained_per_channel_lookup() {
        let mut rng = Rng(0x7c0f_5a11_2b3d_9e42);
        let mut comparisons = 0usize;
        for _ in 0..400 {
            let pixels = 2 + (rng.next() % 40) as usize;
            let bgra: Vec<u8> = (0..pixels * 4).map(|_| rng.byte()).collect();
            let center = (rng.next() as usize % pixels) * 4;
            let neighbor = (rng.next() as usize % pixels) * 4;
            let got = measurement_contrast(&bgra, center, neighbor);
            let want = contrast_reference(&bgra, center, neighbor);
            assert_eq!(got.to_bits(), want.to_bits(), "contrast differs");
            comparisons += 1;
        }
        assert!(comparisons >= 400, "the sweep must not thin out: {comparisons}");
    }

    /// The table reference must be the same table `byte_to_unit` reads and must
    /// stay valid across calls; a stale table would silently change the raster.
    #[test]
    fn the_contrast_table_is_the_one_byte_to_unit_reads() {
        let table = &*BYTE_TO_UNIT;
        for byte in 0..=255u8 {
            assert_eq!(table[byte as usize].to_bits(), byte_to_unit(byte).to_bits(),
                "table entry {byte} differs from byte_to_unit");
        }
        let mut rng = Rng(0x2b1c_9d77_4e50_a3f8);
        let width = 7usize; let height = 5usize;
        let bgra: Vec<u8> = (0..width * height * 4).map(|_| rng.byte()).collect();
        let first = calculate_measurement(width, height, &bgra);
        let noise: Vec<u8> = (0..64 * 64 * 4).map(|_| rng.byte()).collect();
        let _ = calculate_measurement(64, 64, &noise);
        let second = calculate_measurement(width, height, &bgra);
        assert_eq!(first, second, "a repeated measurement differed");
    }

    /// The full raster must match a reference built from the retained kernel.
    #[test]
    fn measurement_raster_matches_the_retained_kernel() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for (width, height) in [(2usize, 2usize), (3, 5), (8, 6), (11, 4)] {
            let bgra: Vec<u8> = (0..width * height * 4).map(|_| rng.byte()).collect();
            let got = calculate_measurement(width, height, &bgra);
            let normalization = 12f64.sqrt();
            let mut want = vec![0u8; width * height];
            for y in 0..height {
                for x in 0..width {
                    let center = (y * width + x) * 4;
                    let top = ((if y == 0 { 1 } else { y - 1 }) * width + x) * 4;
                    let bottom = ((if y == height - 1 { y - 1 } else { y + 1 }) * width + x) * 4;
                    let left = (y * width + if x == 0 { 1 } else { x - 1 }) * 4;
                    let right = (y * width + if x == width - 1 { x - 1 } else { x + 1 }) * 4;
                    let mut total = contrast_reference(&bgra, center, top);
                    total += contrast_reference(&bgra, center, bottom);
                    total += contrast_reference(&bgra, center, left);
                    total += contrast_reference(&bgra, center, right);
                    want[y * width + x] = (total.sqrt() * 255.0 / normalization).min(255.0).max(0.0).trunc() as u8;
                }
            }
            assert_eq!(got, want, "raster differs at {width}x{height}");
        }
    }
}

#[cfg(test)]
mod sparse_peak_equivalence {
    //! `prepare_histogram_peaks` now hands the detector the bins it touched
    //! instead of letting it scan all `bins^4`. The oracle is the original
    //! dense kernel, kept verbatim: it walks every bin in ascending order and
    //! applies the same `weight < 2.0` skip and the same neighbor test. The
    //! touched list must reproduce its peak sequence exactly — same peaks, same
    //! order — for both bin widths and both `connected` values, on rasters that
    //! are sparse, dense, single-bin, and weighted with zeros, negatives and
    //! non-finite values.
    use super::*;

    /// The original dense kernel, verbatim from before the touched-bin walk.
    fn detect_reference(histogram: &[f64], bins: usize, connected: bool) -> HistogramPeaks {
        let neighbors = neighbor_offsets(connected);
        let n = bins as i64;
        let mut peaks = Vec::new();
        for (index, &weight) in histogram.iter().enumerate() {
            if weight < 2.0 { continue; }
            let index = index as i64;
            let (b, g, r, a) = (index % n, index / n % n, index / (n * n) % n, index / (n * n * n));
            let mut maximum = true;
            for &[db, dg, dr, da] in &neighbors {
                let (nb, ng, nr, na) = (b + db, g + dg, r + dr, a + da);
                if nb < 0 || nb >= n || ng < 0 || ng >= n || nr < 0 || nr >= n || na < 0 || na >= n { continue; }
                if histogram[(((na * n + nr) * n + ng) * n + nb) as usize] > weight {
                    maximum = false;
                    break;
                }
            }
            if maximum {
                let step = 256 / n;
                let half = 128 / n;
                peaks.push(Peak { weight, color: [b, g, r, a].map(|v| (v * step + half) as u8) });
            }
        }
        let ordered = order_peaks(peaks);
        HistogramPeaks {
            bgra: ordered.iter().flat_map(|peak| peak.color).collect(),
            weights: ordered.iter().map(|peak| peak.weight).collect(),
        }
    }

    /// The original histogram build, verbatim, followed by the dense kernel.
    fn prepare_reference(bgra: &[u8], bins: usize, connected: bool, pixel_weights: Option<&[f64]>) -> HistogramPeaks {
        let mut histogram = vec![0.0f64; bins.pow(4)];
        let shift = if bins == 8 { 5 } else { 4 };
        for (pixel, chunk) in bgra.chunks_exact(4).enumerate() {
            let index = ((((chunk[3] >> shift) as usize * bins + (chunk[2] >> shift) as usize) * bins
                + (chunk[1] >> shift) as usize) * bins) + (chunk[0] >> shift) as usize;
            histogram[index] += pixel_weights.map_or(1.0, |weights| weights[pixel]);
        }
        detect_reference(&histogram, bins, connected)
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
        }
        fn byte(&mut self) -> u8 { (self.next() >> 24) as u8 }
        fn below(&mut self, bound: usize) -> usize { (self.next() % bound as u64) as usize }
    }

    fn same(got: &HistogramPeaks, want: &HistogramPeaks, what: &str) {
        assert_eq!(got.bgra, want.bgra, "peak colors differ: {what}");
        assert_eq!(got.weights.len(), want.weights.len(), "peak count differs: {what}");
        for (index, (a, b)) in got.weights.iter().zip(&want.weights).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "peak weight {index} differs: {what}");
        }
    }

    /// Every raster shape the detector sees, at both widths and both
    /// connectivities: all-zero, all-one, all-two, one hot bin, a hot corner,
    /// a hot center, a random sparse raster, a random dense raster, and a
    /// raster whose bins are all distinct.
    #[test]
    fn touched_walk_matches_the_dense_kernel_on_rasters() {
        let mut rng = Rng(0x5eed_92a1_0bad_f00d);
        let mut rasters: Vec<Vec<u8>> = Vec::new();
        for fill in [0u8, 0x10, 0x20, 0xff] {
            rasters.push(vec![fill; 64 * 4]);
        }
        // One hot bin, at the first bin, the last bin, and the middle.
        for hot in [0usize, 1, 0x8000, 0xffff] {
            let mut raster = vec![0u8; 64 * 4];
            raster[hot % 64 * 4] = 0xff;
            rasters.push(raster);
        }
        // Random sparse and random dense rasters, and one with every bin distinct.
        for density in [1usize, 3, 16, 64] {
            let mut raster = vec![0u8; 64 * 4];
            for pixel in 0..64 {
                if rng.below(64) < density {
                    for channel in 0..4 { raster[pixel * 4 + channel] = rng.byte(); }
                }
            }
            rasters.push(raster);
        }
        let mut distinct = vec![0u8; 64 * 4];
        for (index, pixel) in distinct.chunks_exact_mut(4).enumerate() {
            pixel[0] = ((index % 8) << 5) as u8;
            pixel[1] = ((index / 8) << 5) as u8;
        }
        rasters.push(distinct);

        for bins in [8usize, 16] {
            for connected in [false, true] {
                for (which, raster) in rasters.iter().enumerate() {
                    let got = prepare_histogram_peaks(raster, bins, connected, None);
                    let want = prepare_reference(raster, bins, connected, None);
                    same(&got, &want, &format!("raster {which}, bins {bins}, connected {connected}"));
                }
            }
        }
    }

    /// Weighted rasters, including weights that are zero, negative, and
    /// non-finite. A zero weight leaves a bin at `0.0` (so it is never a peak);
    /// a negative or NaN sum is below the `2.0` threshold or not a maximum, and
    /// the touched list must still reproduce the dense kernel's answer.
    #[test]
    fn touched_walk_matches_the_dense_kernel_on_weights() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let raster: Vec<u8> = (0..96 * 4).map(|_| rng.byte()).collect();
        let pixels = raster.len() / 4;
        let weight_sets: Vec<Vec<f64>> = vec![
            vec![0.0; pixels],
            vec![1.0; pixels],
            vec![2.0; pixels],
            vec![0.5; pixels],
            (0..pixels).map(|i| if i % 3 == 0 { 0.0 } else { 1.0 + (i % 5) as f64 }).collect(),
            (0..pixels).map(|i| if i % 7 == 0 { -1.0 } else { 3.0 }).collect(),
            (0..pixels).map(|i| if i % 11 == 0 { f64::NAN } else { 4.0 }).collect(),
            (0..pixels).map(|i| if i % 13 == 0 { f64::INFINITY } else { 1.0 }).collect(),
            (0..pixels).map(|i| if i % 17 == 0 { f64::NEG_INFINITY } else { 2.5 }).collect(),
            (0..pixels).map(|_| rng.below(5) as f64).collect(),
        ];
        for bins in [8usize, 16] {
            for connected in [false, true] {
                for (which, weights) in weight_sets.iter().enumerate() {
                    let got = prepare_histogram_peaks(&raster, bins, connected, Some(weights));
                    let want = prepare_reference(&raster, bins, connected, Some(weights));
                    same(&got, &want, &format!("weights {which}, bins {bins}, connected {connected}"));
                }
            }
        }
    }

    /// A dense touched list must fall back to the dense kernel and still agree;
    /// a sparse list with repeats and a reversed order must dedupe and sort to
    /// the same ascending walk.
    #[test]
    fn touched_walk_handles_dense_and_repeated_lists() {
        let mut rng = Rng(0xfeed_face_dead_beef);
        for bins in [8usize, 16] {
            for connected in [false, true] {
                let len = bins.pow(4);
                // Dense: every bin nonzero, so the list is over the fallback
                // threshold and the kernel must take the dense path.
                let dense: Vec<f64> = (0..len).map(|_| 1.0 + rng.below(4) as f64).collect();
                let want = detect_reference(&dense, bins, connected);
                let all: Vec<u32> = (0..len as u32).collect();
                same(&detect_histogram_peaks_touched(&dense, bins, connected, all.clone()), &want,
                    &format!("dense list, bins {bins}, connected {connected}"));
                let mut repeated: Vec<u32> = all.iter().flat_map(|&i| [i, i, i]).collect();
                repeated.reverse();
                same(&detect_histogram_peaks_touched(&dense, bins, connected, repeated), &want,
                    &format!("repeated reversed list, bins {bins}, connected {connected}"));

                // Sparse: a few hot bins in a zero field, so the list stays
                // under the threshold and the sparse walk runs. The list holds
                // every bin the dense kernel could report, plus extras, in a
                // shuffled order with repeats.
                let mut sparse_histogram = vec![0.0f64; len];
                let mut hot: Vec<u32> = Vec::new();
                for _ in 0..(len / 512).max(4) {
                    let index = rng.below(len) as u32;
                    sparse_histogram[index as usize] = 2.0 + rng.below(9) as f64;
                    hot.push(index);
                }
                let want = detect_reference(&sparse_histogram, bins, connected);
                let mut list: Vec<u32> = hot.iter().flat_map(|&i| [i, i]).collect();
                for _ in 0..8 { list.push(rng.below(len) as u32); }
                list.reverse();
                same(&detect_histogram_peaks_touched(&sparse_histogram, bins, connected, list), &want,
                    &format!("sparse list, bins {bins}, connected {connected}"));
            }
        }
    }

    /// The accumulator stops recording at `bins^4 / 4 + 1`, and a list that long
    /// always takes the dense fallback, so a truncated list is never walked.
    /// This pins that invariant and checks the truncated case still agrees.
    #[test]
    fn touched_accumulator_cap_always_falls_back() {
        for bins in [8usize, 16] {
            let len = bins.pow(4);
            let cap = len / 4 + 1;
            assert!(cap * 4 > len, "a full list must exceed the fallback threshold at bins {bins}");
            // A raster that touches every bin: the accumulator fills to the cap
            // and the kernel must take the dense path and match the oracle.
            // Pixel `p` carries bin `p`'s four channel indices in its high bits,
            // which is exactly what the histogram build shifts out.
            let (shift, mask) = if bins == 8 { (5u32, 7u32) } else { (4, 15) };
            let mut raster = vec![0u8; len * 4];
            for pixel in 0..len {
                for channel in 0..4 {
                    let value = ((pixel >> (channel * (if bins == 8 { 3 } else { 4 }))) as u32) & mask;
                    raster[pixel * 4 + channel] = (value << shift) as u8;
                }
            }
            for connected in [false, true] {
                let got = prepare_histogram_peaks(&raster, bins, connected, None);
                let want = prepare_reference(&raster, bins, connected, None);
                same(&got, &want, &format!("capped accumulator, bins {bins}, connected {connected}"));
            }
        }
    }
}
