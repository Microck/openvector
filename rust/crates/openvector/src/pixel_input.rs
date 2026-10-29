//! Port of `src/pixel-input.mjs`: shared boundaries for prepared byte pixels
//! and owned normalized colors.
//!
//! Errors carry the exact JavaScript messages so fixtures that compare error
//! text keep working.

use std::fmt;

/// Input validation or decoding failure with the reference error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputError(pub String);

impl InputError {
    pub fn new(message: impl Into<String>) -> Self {
        InputError(message.into())
    }
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InputError {}

impl From<InputError> for String {
    fn from(error: InputError) -> String {
        error.0
    }
}

/// Shorthand for `Err(InputError::new(message))`.
pub fn fail<T>(message: &str) -> Result<T, InputError> {
    Err(InputError::new(message))
}

/// Validates decoded dimensions and returns the pixel count (`imageSize`).
pub fn image_size(width: i64, height: i64) -> Result<usize, InputError> {
    const MAX: i64 = 2147483647;
    if width < 1 || height < 1 || width > MAX || height > MAX || width * height > MAX {
        return fail("Decoded image dimensions must be positive int32 values with an int32 pixel count.");
    }
    Ok((width * height) as usize)
}

/// `validateBgra`: byte typing already guarantees the channel range, so only
/// the four-channel length check remains.
pub fn validate_bgra(bgra: &[u8]) -> Result<(), InputError> {
    if bgra.len() % 4 != 0 {
        return fail("BGRA pixels must contain four byte channels per pixel.");
    }
    Ok(())
}

/// Convert owned CMYK byte planes to RGB in place (`normalizeCmykPixels`).
/// `bgra` holds C, M, Y in its first three channels; opacity stays separate.
pub fn normalize_cmyk_pixels(bgra: &mut [u8], black: &[u8]) {
    for (pixel, &k) in black.iter().enumerate() {
        let light = 255 - k as u32;
        let offset = pixel * 4;
        for channel in 0..3 {
            let value = 255 - bgra[offset + channel] as u32;
            bgra[offset + channel] = ((value * light + 127) / 255) as u8;
        }
    }
}

/// `readPaletteColors`: validates normalized BGRA channels and narrows to f32.
pub fn read_palette_colors(palette: &[f64]) -> Result<Vec<f32>, InputError> {
    if palette.is_empty() || palette.len() % 4 != 0 {
        return fail("Palette must contain four normalized BGRA channels per color.");
    }
    if palette.iter().any(|c| !c.is_finite() || *c < 0.0 || *c > 1.0) {
        return fail("Palette channels must be finite numbers from 0 to 1.");
    }
    Ok(palette.iter().map(|&c| c as f32).collect())
}

/// QColor converts validated float32 unit channels through 16-bit storage.
pub fn quantize_qcolor_channels(channels: &[f32]) -> Vec<f32> {
    // Math.round on non-negative values equals round-half-away-from-zero.
    channels.iter().map(|&v| ((v as f64 * 65535.0).round() / 65535.0) as f32).collect()
}

/// `readPaletteInput`: validated BGRA plus owned float32 palette colors.
pub fn read_palette_input(bgra: &[u8], palette: &[f64]) -> Result<Vec<f32>, InputError> {
    validate_bgra(bgra)?;
    read_palette_colors(palette)
}

/// Validated score input (`readPaletteScoreInput`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteScoreInput {
    pub colors: Vec<f32>,
    pub indices: Vec<i32>,
    pub cost_per_color: f64,
}

pub fn read_palette_score_input(
    bgra: &[u8],
    palette: &[f64],
    indices: &[i64],
    cost_per_color: f64,
) -> Result<PaletteScoreInput, InputError> {
    let colors = read_palette_input(bgra, palette)?;
    let color_count = (colors.len() / 4) as i64;
    if indices.len() != bgra.len() / 4 {
        return fail("Score needs one palette index per BGRA pixel.");
    }
    if indices.iter().any(|&i| i < 0 || i >= color_count) {
        return fail("Score indices must identify existing palette colors.");
    }
    if !cost_per_color.is_finite() || !(cost_per_color as f32).is_finite() {
        return fail("costPerColor must be a finite float32 number.");
    }
    Ok(PaletteScoreInput { colors, indices: indices.iter().map(|&i| i as i32).collect(), cost_per_color })
}

/// Decoded image returned by the format readers (`{width,height,bgra,hasAlpha,format}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
    pub has_alpha: bool,
    pub format: &'static str,
}
