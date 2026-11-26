//! SVG serialization with native stream checkpoints and alpha thresholds.
use crate::decimal_format::{fixed_decimal, stream_fixed_decimal};
use crate::fitting::FitNode;
use crate::vector_file::{collect_text, TextOutput};
use crate::vector_output::*;
use std::collections::HashMap;

const OPAQUE_ALPHA: u8 = 251;

pub fn export_svg(input: &VectorOutputInput<'_>) -> Result<String, VectorOutputError> {
    collect_text(|output| serialize_svg(input, output))
}

fn rgb(color: &[u8; 4]) -> String { format!("{:02x}{:02x}{:02x}", color[2], color[1], color[0]) }

pub fn serialize_svg(input: &VectorOutputInput<'_>, output: &mut dyn TextOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "SVG")?;
    output.write("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n", true)?;
    output.write(&format!("<svg width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\" version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\">", options.output_width, options.output_height, options.width, options.height), false)?;
    let mut writer = SvgWriter { output, thin: options.thin_stroke, coordinates: HashMap::new() };
    walk_vector_output(input.geometry, &options, &mut writer)?;
    writer.output.write("\n</svg>\n", true)
}

struct SvgWriter<'a> { output: &'a mut dyn TextOutput, thin: bool, coordinates: HashMap<u64, String> }
impl SvgWriter<'_> {
    fn coordinate(&mut self, value: f64) -> &str {
        let key = if value == 0.0 { 0 } else { value.to_bits() };
        self.coordinates.entry(key).or_insert_with(|| stream_fixed_decimal(value, 2))
    }
    fn point(&mut self, x: f64, y: f64) -> String {
        let mut point = self.coordinate(x).to_owned(); point.push(' '); point.push_str(self.coordinate(y)); point
    }
}
impl VectorWriter for SvgWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 10 }
    fn cutout_backing_alpha(&self) -> Option<u8> { (!self.thin).then_some(OPAQUE_ALPHA) }
    fn begin_group(&mut self, color: &[u8; 4]) -> Result<(), Self::Error> {
        self.output.write("\n", true)?;
        self.output.write(&format!("<g id=\"#{}{:02x}\">", rgb(color), color[3]), false)
    }
    fn end_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> {
        self.output.write("\n", true)?; self.output.write("</g>", false)
    }
    fn begin_shape(&mut self, color: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        self.output.write("\n", true)?;
        let rgb = rgb(color);
        self.output.write(&format!("<path fill=\"#{rgb}\""), false)?;
        if self.thin { self.output.write(&format!(" stroke=\"#{rgb}\" stroke-width=\"0.09375\""), false)?; }
        let opacity = if color[3] >= OPAQUE_ALPHA { "1.00".into() } else { fixed_decimal(color[3] as f64 * (1.0 / 255.0), 2) };
        self.output.write(&format!(" opacity=\"{opacity}\" d=\""), false)
    }
    fn end_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> { self.output.write(" Z\" />", false) }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.point(start.x, start.y); self.output.write(&format!(" M {point}"), false)?; }
        let point = self.point(end.x, end.y); self.output.write(&format!(" L {point}"), false)
    }
    fn cubic(&mut self, curve: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.point(curve[0], curve[1]); self.output.write(&format!(" M {point}"), false)?; }
        self.output.write(" C", false)?;
        for index in [2, 4, 6] { let point = self.point(curve[index], curve[index + 1]); self.output.write(&format!(" {point}"), false)?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contour_fitting::{FitRegion, FittedGeometry};
    use crate::export_options::Layout;

    #[test]
    fn cutout_backing_preserves_hidden_and_translucent_descendants() {
        let mut geometry = FittedGeometry {
            cubic_storage_slots: 0,
            cubics: vec![],
            nodes: vec![],
            regions: vec![],
        };
        for (layer, bgra) in [[0, 0, 255, 255], [0, 255, 0, 255], [255, 0, 0, 255]].into_iter().enumerate() {
            let lo = layer as f64 * 2.0;
            let hi = 12.0 - lo;
            let base = geometry.nodes.len();
            let parent = layer as i32 - 1;
            geometry.nodes.extend([(lo, lo), (hi, lo), (hi, hi), (lo, hi)].map(|(x, y)| {
                FitNode { x, y, kind: 0, cubic: -1 }
            }));
            geometry.regions.push(FitRegion {
                bgra, pixel_count: 1, color_index: layer as i32, parent,
                contour: (base..base + 4).collect(),
                boundary: vec![],
                edges: (0..4).map(|index| {
                    let next = (index + 1) % 4;
                    [index as i64, 0, parent as i64, (base + index) as i64, (base + next) as i64, index as i64, next as i64]
                }).collect(),
            });
        }
        fn render(geometry: &FittedGeometry, hidden_regions: &[i64]) -> String {
            let mut input = VectorOutputInput::new(geometry, 12, 12);
            input.layout = Layout::Cutouts;
            input.hidden_regions = hidden_regions;
            export_svg(&input).unwrap()
        }
        assert_eq!(render(&geometry, &[]).matches("<path ").count(), 5);
        assert_eq!(render(&geometry, &[2]).matches("<path ").count(), 2);
        geometry.regions[2].bgra[3] = 128;
        assert_eq!(render(&geometry, &[]).matches("<path ").count(), 3);
    }
}
