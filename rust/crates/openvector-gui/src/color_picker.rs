//! Colour conversions for the colour picker dialog. The dialog edits HSV
//! (hue strip plus saturation/value square) and shows the result as hex,
//! while palette and paint colours are stored as RGBA channels in 0..=1.

/// Hue in degrees 0..360, saturation and value in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv { pub h: f32, pub s: f32, pub v: f32 }

impl Hsv {
    pub fn from_rgb([r, g, b]: [f32; 3]) -> Self {
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let h = if delta == 0.0 { 0.0 }
            else if max == r { 60.0 * ((g - b) / delta).rem_euclid(6.0) }
            else if max == g { 60.0 * ((b - r) / delta + 2.0) }
            else { 60.0 * ((r - g) / delta + 4.0) };
        Self { h, s: if max == 0.0 { 0.0 } else { delta / max }, v: max }
    }
    pub fn to_rgb(self) -> [f32; 3] {
        let c = self.v * self.s;
        let x = c * (1.0 - ((self.h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = self.v - c;
        let [r, g, b] = match (self.h.rem_euclid(360.0) / 60.0) as u32 {
            0 => [c, x, 0.0], 1 => [x, c, 0.0], 2 => [0.0, c, x],
            3 => [0.0, x, c], 4 => [x, 0.0, c], _ => [c, 0.0, x],
        };
        [r + m, g + m, b + m]
    }
}

/// `#RRGGBB` for channels in 0..=1.
pub fn to_hex([r, g, b]: [f32; 3]) -> String {
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))
}

/// Accepts `RRGGBB`, `#RRGGBB`, `RGB` or `#RGB`, case-insensitive.
pub fn parse_hex(text: &str) -> Option<[f32; 3]> {
    let digits = text.trim().trim_start_matches('#');
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|digit| [digit, digit]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    let value = u32::from_str_radix(&expanded, 16).ok()?;
    Some([(value >> 16) as f32 / 255.0, ((value >> 8) & 0xff) as f32 / 255.0, (value & 0xff) as f32 / 255.0])
}

/// Opacity typed as a whole percentage, `0`..=`100`, with an optional `%`.
pub fn parse_percent(text: &str) -> Option<f32> {
    let value = text.trim().trim_end_matches('%').trim().parse::<f32>().ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_hsv() {
        for hex in ["#000000", "#FFFFFF", "#E0182A", "#18B64C", "#1C4FD8", "#808080"] {
            let rgb = parse_hex(hex).expect("valid hex");
            assert_eq!(to_hex(Hsv::from_rgb(rgb).to_rgb()), hex);
        }
    }

    #[test]
    fn hex_and_percent_inputs_are_forgiving_but_bounded() {
        assert_eq!(parse_hex("fa0"), parse_hex("#FFAA00"));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(parse_percent("50 %"), Some(0.5));
        assert_eq!(parse_percent("101"), None);
        assert_eq!(parse_percent("-1"), None);
    }
}
