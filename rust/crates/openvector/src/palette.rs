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

pub(crate) fn assign_indices(bgra: &[u8], colors: &[f32], indices: &mut [i32]) {
    // Artwork contains many identical BGRA samples, often beyond adjacent runs.
    // Cache only this call: refinement changes the palette before the next pass.
    // The full key check makes collisions affect speed, never assignments.
    const CACHE_BITS: u32 = 10;
    let mut cache = [(0u32, -1i32); 1 << CACHE_BITS];
    for (pixel, index) in indices.iter_mut().enumerate() {
        let offset = pixel * 4;
        let key = u32::from_le_bytes(bgra[offset..offset + 4].try_into().unwrap());
        let slot = (key.wrapping_mul(0x9e37_79b9) >> (32 - CACHE_BITS)) as usize;
        let cached = &mut cache[slot];
        if cached.1 >= 0 && cached.0 == key {
            *index = cached.1;
            continue;
        }
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
    let mut sum = 0.0f32;
    for (pixel, &index) in indices.iter().enumerate() {
        let offset = pixel * 4;
        sum += distance(colors, index as usize * 4, byte_to_unit(bgra[offset]), byte_to_unit(bgra[offset + 1]),
            byte_to_unit(bgra[offset + 2]), byte_to_unit(bgra[offset + 3]));
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
    let mut contrast = 0.0f64;
    for channel in 0..4 {
        let difference = (byte_to_unit(bgra[neighbor + channel]) - byte_to_unit(bgra[center + channel])) as f64;
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

/// Internal peak kernel; callers own validated histogram storage and controls.
pub fn detect_histogram_peaks(histogram: &[f64], bins: usize, connected: bool) -> HistogramPeaks {
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

/// Build the weighted or unweighted histogram and detect its peaks.
pub fn prepare_histogram_peaks(bgra: &[u8], bins: usize, connected: bool, pixel_weights: Option<&[f64]>) -> HistogramPeaks {
    let mut histogram = vec![0.0f64; bins.pow(4)];
    let shift = if bins == 8 { 5 } else { 4 };
    for (pixel, chunk) in bgra.chunks_exact(4).enumerate() {
        let index = ((((chunk[3] >> shift) as usize * bins + (chunk[2] >> shift) as usize) * bins
            + (chunk[1] >> shift) as usize) * bins) + (chunk[0] >> shift) as usize;
        histogram[index] += pixel_weights.map_or(1.0, |weights| weights[pixel]);
    }
    detect_histogram_peaks(&histogram, bins, connected)
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
            if iteration==9||converged{weights.fill(0.0);for (pixel,&index) in indices.iter().enumerate(){weights[index as usize]+=measurement.map_or(1.0,|m|m[pixel] as f64*(1.0/255.0))}}
            if converged{break}
            means.fill(0.0);counts.fill(0.0);
            for (pixel,&index) in indices.iter().enumerate(){let color=index as usize;let weight=pixel_weights.map_or(1.0,|w|w[pixel] as f32);for c in 0..4{means[color*4+c]+=byte_to_unit(bgra[pixel*4+c])*weight}counts[color]+=weight}
            for (color,&count) in counts.iter().enumerate(){let m=&mut means[color*4..color*4+4];if count>0.0{let reciprocal=(1.0/count as f64) as f32;for value in m{*value*=reciprocal}}else{self.random_color(m)}}
            let mut change=0.0f64;for (a,b) in colors.iter().zip(&means){change=js_max(change,(*a-*b).abs() as f64)}
            colors.copy_from_slice(&means);converged=change<(0.01f32 as f64);
        }
        for (color,&count) in counts.iter().enumerate(){if count==0.0{colors[color*4..color*4+4].fill(0.0)}}
        Ok(GeneratedPalette{palette:colors,indices,weights,passes})
    }
}
