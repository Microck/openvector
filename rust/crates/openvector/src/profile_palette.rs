//! Port of `src/profile-palette.mjs`: GUI palette commands over one owned
//! profile selection and its separate runtime revert copy.

use serde_json::Value;

use crate::pixel_input::{quantize_qcolor_channels, read_palette_colors};

/// Runtime revert copy (`readPaletteRuntime`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaletteRuntime {
    pub backup: Vec<f32>,
    pub customized: bool,
}

/// The palette fields a command mutates: the full settings record or the
/// palette fields owned by a browser draft.
#[derive(Debug)]
pub struct PaletteProfile<'a> {
    pub color_count: &'a mut f64,
    pub palette_color_count: &'a mut f64,
    pub palette: &'a mut Vec<f32>,
}

/// One palette command. Numeric operands stay doubles so integer validation
/// keeps the original messages.
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteCommand {
    Count(f64),
    Replace(Vec<f64>),
    Choose(Vec<f64>),
    Color { index: f64, color: Vec<f64> },
    Add,
    Delete(f64),
    Revert,
    Source,
}

const PALETTE_SHAPE: &str = "Palette must contain four normalized BGRA channels per color.";
const UNKNOWN_ACTION: &str = "Choose count, replace, choose, color, add, delete, revert or source for the palette.";

/// An empty palette array is accepted as "no colors"; anything else must be
/// a valid normalized palette.
fn colors(palette: &[f64]) -> Result<Vec<f32>, String> {
    if palette.is_empty() { return Ok(Vec::new()); }
    read_palette_colors(palette).map_err(String::from)
}

/// Numeric array view of a JSON value. Non-numeric channels become NaN so the
/// palette reader reports its own shape-then-channel errors.
fn json_numbers(value: Option<&Value>) -> Result<Vec<f64>, String> {
    match value {
        Some(Value::Array(items)) => Ok(items.iter().map(|item| item.as_f64().unwrap_or(f64::NAN)).collect()),
        _ => Err(PALETTE_SHAPE.to_string()),
    }
}

fn json_number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(f64::NAN)
}

pub fn read_palette_runtime(input: &Value) -> Result<PaletteRuntime, String> {
    let object = input.as_object().filter(|o| o.get("customized").is_some_and(Value::is_boolean))
        .ok_or_else(|| "Expected palette runtime state.".to_string())?;
    let backup = colors(&json_numbers(object.get("backup"))?)?;
    Ok(PaletteRuntime { backup, customized: object["customized"].as_bool().unwrap() })
}

impl PaletteCommand {
    /// Parse a command object, reproducing the original action errors.
    pub fn from_json(command: &Value) -> Result<Self, String> {
        let object = command.as_object().ok_or_else(|| "Choose a palette command.".to_string())?;
        Ok(match object.get("action").and_then(Value::as_str) {
            Some("count") => PaletteCommand::Count(json_number(object.get("count"))),
            Some("replace") => PaletteCommand::Replace(json_numbers(object.get("palette"))?),
            Some("choose") => PaletteCommand::Choose(json_numbers(object.get("palette"))?),
            Some("color") => PaletteCommand::Color { index: json_number(object.get("index")), color: json_numbers(object.get("color"))? },
            Some("add") => PaletteCommand::Add,
            Some("delete") => PaletteCommand::Delete(json_number(object.get("index"))),
            Some("revert") => PaletteCommand::Revert,
            Some("source") => PaletteCommand::Source,
            _ => return Err(UNKNOWN_ACTION.to_string()),
        })
    }
}

#[inline]
fn is_integer(value: f64) -> bool {
    value.is_finite() && value.trunc() == value
}

/// Mutate one owned selection and its runtime revert copy. Returns
/// `Some(changed)` only for `count`, matching the original return value.
pub fn apply_palette_command(profile: &mut PaletteProfile, runtime: &mut PaletteRuntime, command: &PaletteCommand) -> Result<Option<bool>, String> {
    match command {
        PaletteCommand::Count(count) => {
            let count = *count;
            if !is_integer(count) || !(2.0..=24.0).contains(&count) {
                return Err("Number of colors must be an integer from 2 through 24.".to_string());
            }
            let changed = *profile.color_count != count;
            *profile.color_count = count;
            return Ok(Some(changed));
        }
        PaletteCommand::Replace(palette) | PaletteCommand::Choose(palette) => {
            let selected = quantize_qcolor_channels(&colors(palette)?);
            if matches!(command, PaletteCommand::Choose(_)) {
                apply_palette_command(profile, runtime, &PaletteCommand::Count((selected.len() / 4) as f64))?;
            }
            *profile.palette = selected.clone();
            *profile.palette_color_count = (selected.len() / 4) as f64;
            runtime.backup = selected;
            runtime.customized = false;
        }
        PaletteCommand::Color { index, color } => {
            let selected = read_palette_colors(color).map_err(String::from)?;
            if selected.len() != 4 { return Err("A palette color needs four BGRA channels.".to_string()); }
            if !is_integer(*index) { return Err("Choose an integer palette color index.".to_string()); }
            if *index >= 0.0 && *index < (profile.palette.len() / 4) as f64 {
                let offset = *index as usize * 4;
                profile.palette[offset..offset + 4].copy_from_slice(&quantize_qcolor_channels(&selected));
            }
            runtime.customized = true;
        }
        PaletteCommand::Add => {
            if profile.palette.len() / 4 >= 25 { return Ok(None); }
            profile.palette.extend_from_slice(&[1.0; 4]);
            *profile.palette_color_count = (profile.palette.len() / 4) as f64;
            runtime.customized = true;
        }
        PaletteCommand::Delete(index) => {
            let index = *index;
            if !is_integer(index) || index < 0.0 || index >= (profile.palette.len() / 4) as f64 {
                return Err("Choose an existing palette color.".to_string());
            }
            let offset = index as usize * 4;
            profile.palette.drain(offset..offset + 4);
            // The native delete slot deliberately does not write paletteColorCount.
            runtime.customized = true;
        }
        PaletteCommand::Revert => {
            *profile.palette = runtime.backup.clone();
            *profile.palette_color_count = (runtime.backup.len() / 4) as f64;
            runtime.customized = false;
        }
        PaletteCommand::Source => {
            profile.palette.clear();
            *profile.palette_color_count = 2.0;
            runtime.customized = false;
        }
    }
    Ok(None)
}
