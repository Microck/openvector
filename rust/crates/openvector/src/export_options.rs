//! Port of `src/export-options.mjs`: MainModel's shared output choices.
//! Standalone file serializers retain their own per-call defaults;
//! application and batch export use these.
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout { Stacked, Cutouts, GroupedCutouts }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveMode { Splines, SparseLines, DenseLines }

impl Layout {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text { "stacked" => Self::Stacked, "cutouts" => Self::Cutouts, "grouped-cutouts" => Self::GroupedCutouts, _ => return None })
    }
    pub fn name(self) -> &'static str {
        match self { Self::Stacked => "stacked", Self::Cutouts => "cutouts", Self::GroupedCutouts => "grouped-cutouts" }
    }
}

impl CurveMode {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text { "splines" => Self::Splines, "sparse-lines" => Self::SparseLines, "dense-lines" => Self::DenseLines, _ => return None })
    }
    pub fn name(self) -> &'static str {
        match self { Self::Splines => "splines", Self::SparseLines => "sparse-lines", Self::DenseLines => "dense-lines" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions { pub layout: Layout, pub thin_stroke: bool, pub compact: bool, pub curve_mode: CurveMode }

impl Default for ExportOptions {
    fn default() -> Self { Self { layout: Layout::GroupedCutouts, thin_stroke: false, compact: false, curve_mode: CurveMode::DenseLines } }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOptionsError { NotObject, Unknown(String), Invalid }

impl std::fmt::Display for ExportOptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotObject => f.write_str("Export options must be an object."),
            Self::Unknown(key) => write!(f, "Unknown export option: {key}"),
            Self::Invalid => f.write_str("Choose a valid layout, thinStroke/compact booleans and DXF curveMode."),
        }
    }
}
impl std::error::Error for ExportOptionsError {}

/// Overlay JSON-shaped options (`layout`, `thinStroke`, `compact`,
/// `curveMode`) on `base`, with the reference's validation and messages.
pub fn read_export_options(input: &Value, base: ExportOptions) -> Result<ExportOptions, ExportOptionsError> {
    let object = input.as_object().ok_or(ExportOptionsError::NotObject)?;
    for key in object.keys() {
        if !matches!(key.as_str(), "layout" | "thinStroke" | "compact" | "curveMode") { return Err(ExportOptionsError::Unknown(key.clone())); }
    }
    let mut options = base;
    if let Some(value) = object.get("layout") { options.layout = value.as_str().and_then(Layout::parse).ok_or(ExportOptionsError::Invalid)?; }
    if let Some(value) = object.get("thinStroke") { options.thin_stroke = value.as_bool().ok_or(ExportOptionsError::Invalid)?; }
    if let Some(value) = object.get("compact") { options.compact = value.as_bool().ok_or(ExportOptionsError::Invalid)?; }
    if let Some(value) = object.get("curveMode") { options.curve_mode = value.as_str().and_then(CurveMode::parse).ok_or(ExportOptionsError::Invalid)?; }
    Ok(options)
}
