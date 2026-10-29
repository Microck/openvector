//! Port of `src/document-controls.mjs`: validated document settings, the
//! cache keys built from them, and batch control admission.
use crate::decimal_format::stream_general_decimal as stream_float;
use serde_json::{Map, Value};

/// Validated document controls. Colors are float32 BGRA quadruples.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSettings {
    pub category: i32,
    pub quality: i32,
    pub sensitivity: i32,
    pub advanced: bool,
    pub anti_alias: bool,
    pub background_mode: i32,
    pub background: [f32; 4],
    pub complexity: i32,
    pub minimum_pixels: i32,
    pub artifact_rejection: i32,
    pub cluster_colors: bool,
    pub palette_mode: i32,
    pub palette: Vec<f32>,
    pub smoothness: i32,
    pub detect_corners: bool,
    pub curve_complexity: i32,
    pub smoothing_anti_alias: bool,
}

impl Default for DocumentSettings {
    /// Fresh scalar state shared by document construction and batch profiles.
    fn default() -> Self {
        Self { category: -1, quality: 1, sensitivity: 1, advanced: false, anti_alias: true, background_mode: -1,
            background: [0.0; 4], complexity: 5, minimum_pixels: 0, artifact_rejection: 0, cluster_colors: true,
            palette_mode: 1, palette: Vec::new(), smoothness: 6, detect_corners: true, curve_complexity: 6,
            smoothing_anti_alias: true }
    }
}

fn stream_color(bgra: &[f32]) -> String {
    let parts: Vec<String> = bgra.iter().rev().map(|v| stream_float(*v as f64)).collect();
    format!("[{}]", parts.join(", "))
}

/// Internal key kernel over validated settings and float32 BGRA channels.
pub fn document_palette_key(s: &DocumentSettings) -> String {
    let aliased = !(if s.advanced { s.anti_alias } else { s.category == 1 });
    format!("{}{}{}{}", if aliased { "aliased" } else { "antialiased" }, s.background_mode, stream_color(&s.background), s.sensitivity)
}

/// Internal key kernel over validated document controls and float32 colors.
pub fn document_segmentation_key(s: &DocumentSettings) -> String {
    let controls = if s.advanced {
        format!("adv{} {} {} {}", s.complexity, s.minimum_pixels, s.artifact_rejection, s.cluster_colors as i32)
    } else { format!("basic {} {}", s.category, s.quality) };
    let mut colors = String::new();
    if s.palette_mode == 0 || s.palette_mode == 1 {
        for chunk in s.palette.chunks(4) { colors.push_str(&stream_color(chunk)); }
    }
    let mode = ["two colors ", "fixed ", "flexible", ""].get(s.palette_mode as usize).copied().unwrap_or("undefined");
    format!("{} {} {} {}{}", s.background_mode, stream_color(&s.background), controls, mode, colors)
}

pub fn document_smoothing_key(s: &DocumentSettings, edit_key: &str) -> String {
    let mut key = document_segmentation_key(s) + edit_key;
    if s.advanced { key += &format!(" {} {} {}", s.smoothness, s.detect_corners as i32, s.smoothing_anti_alias as i32); }
    key
}

pub fn document_fitting_key(s: &DocumentSettings, edit_key: &str) -> String {
    let mut key = document_smoothing_key(s, edit_key);
    if s.advanced { key += &s.curve_complexity.to_string(); }
    key
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlsError(pub String);
impl std::fmt::Display for ControlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for ControlsError {}
fn fail<T>(message: impl Into<String>) -> Result<T, ControlsError> { Err(ControlsError(message.into())) }

/// JavaScript `Number.isInteger` over a JSON value.
fn integer(value: Option<&Value>) -> Option<f64> {
    let number = value?.as_f64()?;
    (number.is_finite() && number.fract() == 0.0).then_some(number)
}

fn unit_channels(value: Option<&Value>) -> Option<Vec<f32>> {
    let array = value?.as_array()?;
    let mut out = Vec::with_capacity(array.len());
    for entry in array {
        let number = entry.as_f64()?;
        if !(number.is_finite() && (0.0..=1.0).contains(&number)) { return Some(vec![f32::NAN]); }
        out.push(number as f32);
    }
    Some(out)
}

impl DocumentSettings {
    /// Settings as the JSON-shaped object the reference uses.
    pub fn to_json(&self) -> Map<String, Value> {
        let floats = |values: &[f32]| Value::from(values.iter().map(|v| *v as f64).collect::<Vec<_>>());
        let mut m = Map::new();
        m.insert("category".into(), self.category.into());
        m.insert("quality".into(), self.quality.into());
        m.insert("sensitivity".into(), self.sensitivity.into());
        m.insert("advanced".into(), self.advanced.into());
        m.insert("antiAlias".into(), self.anti_alias.into());
        m.insert("backgroundMode".into(), self.background_mode.into());
        m.insert("background".into(), floats(&self.background));
        m.insert("complexity".into(), self.complexity.into());
        m.insert("minimumPixels".into(), self.minimum_pixels.into());
        m.insert("artifactRejection".into(), self.artifact_rejection.into());
        m.insert("clusterColors".into(), self.cluster_colors.into());
        m.insert("paletteMode".into(), self.palette_mode.into());
        m.insert("palette".into(), floats(&self.palette));
        m.insert("smoothness".into(), self.smoothness.into());
        m.insert("detectCorners".into(), self.detect_corners.into());
        m.insert("curveComplexity".into(), self.curve_complexity.into());
        m.insert("smoothingAntiAlias".into(), self.smoothing_anti_alias.into());
        m
    }
}

/// Validate a complete JSON-shaped settings object, with the reference messages.
pub fn read_document_settings(input: &Value) -> Result<DocumentSettings, ControlsError> {
    let Some(o) = input.as_object() else { return fail("Expected document controls.") };
    let mut ints = [0i32; 5];
    for (slot, name) in ["category", "quality", "sensitivity", "artifactRejection", "paletteMode"].iter().enumerate() {
        let minimum = if *name == "category" { -1.0 } else { 0.0 };
        // Selected batch controls can retain auto sentinels across profile changes.
        // Basic processing kernels still require a classified category and quality.
        let maximum = if matches!(*name, "category" | "quality" | "paletteMode") { 3.0 } else { 2.0 };
        match integer(o.get(*name)) {
            Some(v) if v >= minimum && v <= maximum => ints[slot] = v as i32,
            _ => return fail(format!("Document {name} must be an integer from {minimum} through {maximum}.")),
        }
    }
    let mut bools = [false; 5];
    for (slot, name) in ["advanced", "antiAlias", "clusterColors", "detectCorners", "smoothingAntiAlias"].iter().enumerate() {
        match o.get(*name).and_then(Value::as_bool) { Some(v) => bools[slot] = v, None => return fail(format!("Document {name} must be boolean.")) }
    }
    let mut ranged = [0i32; 2];
    for (slot, name) in ["smoothness", "curveComplexity"].iter().enumerate() {
        match integer(o.get(*name)) { Some(v) if (1.0..=12.0).contains(&v) => ranged[slot] = v as i32,
            _ => return fail(format!("Document {name} must be an integer from 1 through 12.")) }
    }
    let complexity = match integer(o.get("complexity")) { Some(v) if (1.0..=12.0).contains(&v) => v as i32,
        _ => return fail("Document complexity must be an integer from 1 through 12.") };
    let minimum_pixels = match integer(o.get("minimumPixels")) { Some(v) if (0.0..=2147483647.0).contains(&v) => v as i32,
        _ => return fail("Document minimumPixels must be a nonnegative int32 integer.") };
    let background_mode = match integer(o.get("backgroundMode")) { Some(v) if (-2147483648.0..=2147483647.0).contains(&v) => v as i32,
        _ => return fail("Document backgroundMode must be a signed int32 integer.") };
    let background = match unit_channels(o.get("background")) {
        Some(v) if v.len() == 4 && v.iter().all(|c| !c.is_nan()) => [v[0], v[1], v[2], v[3]],
        _ => return fail("Document background must contain four finite unit BGRA channels."),
    };
    let palette_value = o.get("palette").and_then(Value::as_array);
    if palette_value.is_none_or(|p| p.len() % 4 != 0 || p.len() > 1024) {
        return fail("Document palette must contain up to 256 BGRA colors.");
    }
    let palette = match unit_channels(o.get("palette")) {
        Some(v) if v.iter().all(|c| !c.is_nan()) => v,
        _ => return fail("Document palette must contain finite unit BGRA channels."),
    };
    Ok(DocumentSettings { category: ints[0], quality: ints[1], sensitivity: ints[2], artifact_rejection: ints[3],
        palette_mode: ints[4], advanced: bools[0], anti_alias: bools[1], cluster_colors: bools[2], detect_corners: bools[3],
        smoothing_anti_alias: bools[4], smoothness: ranged[0], curve_complexity: ranged[1], complexity, minimum_pixels,
        background_mode, background, palette })
}

pub const ADVANCED_BATCH_CONTROLS: [&str; 10] = ["sensitivity", "antiAlias", "complexity", "minimumPixels", "artifactRejection",
    "clusterColors", "smoothness", "detectCorners", "curveComplexity", "smoothingAntiAlias"];

/// Validated batch controls. `overrides` holds only the advanced controls the
/// reference copies onto the controls object, in the reference's shape.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchControls {
    pub advanced: bool,
    pub category: i32,
    pub quality: i32,
    pub palette_mode: i32,
    pub color_count: i32,
    pub background_mode: i32,
    pub palette: Vec<f32>,
    pub overrides: Map<String, Value>,
}

/// Basic batches resolve auto sentinels. Advanced batches retain them and
/// use the existing document profile; their fixed palette evaluates one count.
pub fn read_batch_controls(input: &Value) -> Result<BatchControls, ControlsError> {
    let Some(o) = input.as_object() else { return fail("Batch image processing requires controls with an extension.") };
    let advanced = match o.get("advanced") { None => false, Some(v) => v.as_bool().ok_or_else(|| ControlsError("Batch advanced must be boolean.".into()))? };
    let mut ints = [3i32; 3];
    for (slot, name) in ["category", "quality", "paletteMode"].iter().enumerate() {
        if let Some(value) = o.get(*name) {
            match integer(Some(value)) { Some(v) if (0.0..=3.0).contains(&v) => ints[slot] = v as i32,
                _ => return fail(format!("Batch {name} must be an integer from 0 through 3.")) }
        }
    }
    let [category, quality, palette_mode] = ints;
    if !advanced && palette_mode == 0 { return fail("Batch two-color processing requires paletteMode 1 and colorCount 2."); }
    let color_count = match o.get("colorCount") { None => if advanced { 2.0 } else { 0.0 }, Some(v) => integer(Some(v)).unwrap_or(f64::NAN) };
    let valid = if advanced { (2.0..=24.0).contains(&color_count) } else { color_count == 0.0 || (2.0..=12.0).contains(&color_count) };
    if !valid {
        return fail(if advanced { "Advanced batch colorCount must be an integer from 2 through 24." }
            else { "Batch colorCount must be 0 for automatic or an integer from 2 through 12." });
    }
    let background_mode = match o.get("backgroundMode") { None => 1.0, Some(v) => integer(Some(v)).unwrap_or(f64::NAN) };
    if ![0.0, 1.0, 3.0].contains(&background_mode) { return fail("Batch backgroundMode must be 0, 1 or 3."); }
    let mut controls = BatchControls { advanced, category, quality, palette_mode, color_count: color_count as i32,
        background_mode: background_mode as i32, palette: Vec::new(), overrides: Map::new() };
    if advanced || o.contains_key("palette") || ADVANCED_BATCH_CONTROLS.iter().any(|n| o.contains_key(*n)) {
        let mut merged = DocumentSettings::default().to_json();
        for (k, v) in o { merged.insert(k.clone(), v.clone()); }
        merged.insert("advanced".into(), advanced.into());
        let profile = read_document_settings(&Value::Object(merged))?.to_json();
        controls.palette = profile["palette"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
        for name in ADVANCED_BATCH_CONTROLS {
            if advanced || o.contains_key(name) { controls.overrides.insert(name.into(), profile[name].clone()); }
        }
    }
    Ok(controls)
}
