//! Port of `src/bitmap-export.mjs`.
//!
//! The caller supplies the document's source-alpha classification and background
//! history separately: neither a hidden-region list nor format capabilities can
//! substitute for that state.

use crate::bitmap_codecs::{encode_bmp, encode_jpeg, encode_png, encode_tiff};
use crate::bitmap_raster::{render_bitmap, BitmapScene};
use crate::contour_fitting::FittedGeometry;
use std::{fmt, str::FromStr};

pub const BITMAP_FORMATS: [&str; 6] = ["bmp", "jpg", "jpeg", "png", "tif", "tiff"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapFormat { Bmp, Jpeg, Png, Tiff }

impl FromStr for BitmapFormat {
    type Err = BitmapExportError;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "bmp" => Ok(Self::Bmp),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "png" => Ok(Self::Png),
            "tif" | "tiff" => Ok(Self::Tiff),
            _ => Err(BitmapExportError::Message("Choose a bitmap format: BMP, JPEG, PNG or TIFF.".into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitmapExportError {
    FlattenRequired,
    Message(String),
}

impl BitmapExportError {
    pub fn code(&self) -> Option<&'static str> {
        match self { Self::FlattenRequired => Some("BITMAP_FLATTEN_REQUIRED"), Self::Message(_) => None }
    }
}

impl fmt::Display for BitmapExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FlattenRequired => "Non-PNG bitmap export may discard transparency. Confirm flattening before exporting.",
            Self::Message(message) => message,
        })
    }
}

impl std::error::Error for BitmapExportError {}

/// Read-only document state. `geometry` is `None` unless fitting is complete.
#[derive(Debug, Clone, Copy)]
pub struct BitmapExportInput<'a> {
    pub geometry: Option<&'a FittedGeometry>,
    pub width: i64,
    pub height: i64,
    pub source_alpha_type: u8,
    pub background_mode: i32,
    pub background_undo_count: usize,
    pub hidden_regions: &'a [i64],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BitmapExportOptions {
    pub format: BitmapFormat,
    pub scale: f64,
    pub flatten: bool,
}

impl BitmapExportOptions {
    pub fn new(format: BitmapFormat) -> Self { Self { format, scale: 1.0, flatten: false } }
}

/// Export a completed scene without modifying its geometry or background state.
pub fn export_bitmap(input: &BitmapExportInput<'_>, options: &BitmapExportOptions) -> Result<Vec<u8>, BitmapExportError> {
    let BitmapExportOptions { format, scale, flatten } = *options;
    if flatten && format == BitmapFormat::Png {
        return Err(BitmapExportError::Message("Flatten confirmation applies only to non-PNG bitmap output.".into()));
    }
    let geometry = input.geometry.ok_or_else(|| BitmapExportError::Message(
        "Finish vectorizing the image before exporting.".into()))?;
    if format != BitmapFormat::Png && input.source_alpha_type != 0 && input.background_mode == 0 && !flatten {
        return Err(BitmapExportError::FlattenRequired);
    }
    let has_alpha = input.background_undo_count != 0
        || (format == BitmapFormat::Png && input.source_alpha_type != 0 && ![1, 2].contains(&input.background_mode));
    let mut rendered = render_bitmap(&BitmapScene {
        geometry, width: input.width, height: input.height, scale, has_alpha, hidden_regions: input.hidden_regions,
    }).map_err(|error| BitmapExportError::Message(error.0))?;
    if has_alpha && matches!(format, BitmapFormat::Png | BitmapFormat::Tiff) {
        // Qt's PNG/TIFF paths convert to straight channels. BMP/JPEG read
        // stored RGB directly, including premultiplied background edits.
        for pixel in rendered.bgra.chunks_exact_mut(4) {
            let alpha = pixel[3] as u32;
            for channel in &mut pixel[..3] {
                *channel = if alpha == 0 { 0 } else { (*channel as u32 * 255 / alpha) as u8 };
            }
        }
    }
    let (width, height, bgra) = (rendered.width, rendered.height, &rendered.bgra);
    match format {
        BitmapFormat::Bmp => encode_bmp(width, height, bgra),
        BitmapFormat::Jpeg => encode_jpeg(width, height, bgra),
        BitmapFormat::Png => encode_png(width, height, bgra, has_alpha),
        BitmapFormat::Tiff => encode_tiff(width, height, bgra),
    }.map_err(|error| BitmapExportError::Message(error.0))
}
