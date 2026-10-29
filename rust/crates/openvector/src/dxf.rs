//! DXF layer/entity traversal and native cubic sampling.
use crate::dxf_container::{close_dxf, dxf_handle, open_dxf, write_dxf_field};
use crate::fitting::FitNode;
use crate::vector_file::{collect_text, TextOutput};
use crate::vector_output::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DxfCurveMode { Splines, SparseLines, #[default] DenseLines }
fn color_name(color: &[u8; 4]) -> String { format!("color#{:02x}{:02x}{:02x}{:02x}", color[2], color[1], color[0], color[3]) }
pub fn export_dxf(input: &VectorOutputInput<'_>, palette: &[f32], mode: DxfCurveMode) -> Result<String, VectorOutputError> {
    collect_text(|output| serialize_dxf(input, palette, mode, output))
}
pub fn serialize_dxf(input: &VectorOutputInput<'_>, palette: &[f32], mode: DxfCurveMode, output: &mut dyn TextOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "DXF")?;
    if palette.len() < 4 || palette.len() % 4 != 0 { return Err(VectorOutputError("DXF needs the original normalized float32 BGRA palette, with four channels per color.".into())); }
    if palette.iter().any(|value| !value.is_finite() || !(0.0..=1.0).contains(value)) { return Err(VectorOutputError("DXF palette channels must be finite numbers in [0,1].".into())); }
    let shape_width = input.geometry.regions.len().to_string().len();
    let layer_names: Vec<String> = if options.shape_mode == 2 {
        palette.chunks_exact(4).map(|color| color_name(&std::array::from_fn(|i| (color[i] as f64 * 255.0 + 0.5).trunc() as u8))).collect()
    } else { (0..input.geometry.regions.len()).map(|i| format!("shape{i:0shape_width$}")).collect() };
    open_dxf(&layer_names, &mut |text, drain| output.write(text, drain))?;
    let mut writer = DxfWriter { output, options: &options, mode, next_handle: 0x47 + layer_names.len() as i64,
        shape_index: 0, shape_width, layer: String::new(), points: HashMap::new() };
    walk_vector_output(input.geometry, &options, &mut writer)?;
    close_dxf(writer.next_handle, &mut |text, drain| writer.output.write(text, drain))
}
struct DxfWriter<'a> { output: &'a mut dyn TextOutput, options: &'a VectorOutputOptions, mode: DxfCurveMode,
    next_handle: i64, shape_index: usize, shape_width: usize, layer: String, points: HashMap<(u64, u64), [String; 2]> }
impl DxfWriter<'_> {
    fn field(&mut self, code: u32, text: &str) -> Result<(), VectorOutputError> { write_dxf_field(&mut |text, drain| self.output.write(text, drain), code, text) }
    fn attributes(&mut self) -> Result<(), VectorOutputError> {
        write_dxf_field(&mut |text, drain| self.output.write(text, drain), 8, &self.layer)?;
        self.field(62, "256")?; self.field(370, "-1")?; self.field(6, "BYLAYER")
    }
    fn point(&mut self, point: [f64; 2], code: u32) -> Result<(), VectorOutputError> {
        let key = |v: f64| if v == 0.0 { 0 } else if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() };
        let key = (key(point[0]), key(point[1]));
        let formatted = match self.points.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let transformed = transform_dxf_point(self.options, point[0], point[1]);
                entry.insert([format_dxf_coordinate(transformed[0])?, format_dxf_coordinate(transformed[1])?])
            }
        };
        write_dxf_field(&mut |text, drain| self.output.write(text, drain), code, &formatted[0])?;
        write_dxf_field(&mut |text, drain| self.output.write(text, drain), code + 10, &formatted[1])?;
        self.field(code + 20, "0.0")
    }
    fn entity(&mut self, kind: &str) -> Result<(), VectorOutputError> {
        self.field(0, kind)?; self.field(5, &dxf_handle(self.next_handle))?; self.next_handle += 1; Ok(())
    }
    fn emit_line(&mut self, start: [f64; 2], end: [f64; 2]) -> Result<(), VectorOutputError> {
        self.entity("LINE")?; self.field(100, "AcDbEntity")?; self.field(100, "AcDbLine")?; self.attributes()?;
        self.point(start, 10)?; self.point(end, 11)
    }
}
fn sample_cubic(points: &[f64; 8], sample: usize) -> [f64; 2] {
    let t = sample as f64 * 0.034482758620689655; let u = 1.0 - t;
    let (t2, u2) = (t * t, u * u);
    let weights = [u2 * u, (u2 * t) * 3.0, (u * t2) * 3.0, t2 * t];
    [0, 1].map(|axis| ((points[axis] * weights[0] + points[axis + 2] * weights[1]) + points[axis + 4] * weights[2]) + points[axis + 6] * weights[3])
}
impl VectorWriter for DxfWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 50 }
    fn begin_shape(&mut self, color: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        self.layer = if self.options.shape_mode == 2 { color_name(color) } else {
            let name = format!("shape{:0width$}", self.shape_index, width = self.shape_width); self.shape_index += 1; name
        }; Ok(())
    }
    fn end_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> { Ok(()) }
    fn line(&mut self, start: &FitNode, end: &FitNode, _: u8) -> Result<(), Self::Error> { self.emit_line([start.x, start.y], [end.x, end.y]) }
    fn cubic(&mut self, curve: &[f64; 8], _: u8, backwards: bool) -> Result<(), Self::Error> {
        if self.mode == DxfCurveMode::Splines {
            self.entity("SPLINE")?; self.attributes()?;
            for (code, value) in [(100, "AcDbEntity"), (100, "AcDbSpline"), (70, "8"), (71, "3"), (72, "8"), (73, "4"), (74, "0")] { self.field(code, value)?; }
            for index in 0..8 { self.field(40, if index < 4 { "0.0" } else { "1.0" })?; }
            for index in [0, 2, 4, 6] { self.point([curve[index], curve[index + 1]], 10)?; }
            return Ok(());
        }
        let reversed; let points = if backwards { reversed = reverse_cubic_points(curve); &reversed } else { curve };
        let samples: [[f64; 2]; 30] = std::array::from_fn(|index| sample_cubic(points, index));
        let mut keep = [false; 30]; keep[0] = true; keep[29] = true;
        let threshold = if self.mode == DxfCurveMode::SparseLines { 0.002 } else { 0.0005 };
        let mut accumulated = 0.0;
        for index in 1..29 {
            let x = (samples[index + 1][0] - samples[index][0]) - (samples[index][0] - samples[index - 1][0]);
            let y = (samples[index + 1][1] - samples[index][1]) - (samples[index][1] - samples[index - 1][1]);
            accumulated = (x * x + y * y) + accumulated;
            if accumulated > threshold { keep[index] = true; accumulated = 0.0; }
        }
        let mut start = [curve[0], curve[1]];
        for step in 1..30 { let index = if backwards { 29 - step } else { step };
            if keep[index] { self.emit_line(start, samples[index])?; start = samples[index]; }
        }
        Ok(())
    }
}
