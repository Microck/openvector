//! Illustrator's native libplot dialect and compound-path grouping.
use crate::fitting::FitNode;
use crate::vector_file::{collect_text, TextOutput};
use crate::vector_output::*;

pub fn export_ai(input: &VectorOutputInput<'_>) -> Result<String, VectorOutputError> { collect_text(|output| serialize_ai(input, output)) }
pub fn serialize_ai(input: &VectorOutputInput<'_>, output: &mut dyn TextOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "AI")?;
    let (width, height) = (options.width, options.height);
    let mut formatter = ScaledOutputFormatter::new(&options);
    let (fixed_width, fixed_height) = (formatter.fixed(width as f64, 2), formatter.fixed(height as f64, 2));
    output.write(&format!("%!PS-Adobe-3.0\n%%Creator: GNU libplot drawing library 4.1\n%%For: (Unknown) (Unknown)\n%%Title: (Untitled)\n%%CreationDate: \n%%BoundingBox: 0 0 {width} {height}\n%%HiResBoundingBox: 0.0000 0.0000 {fixed_width} {fixed_height}\n%%DocumentProcessColors: Cyan Magenta Yellow Black\n%%DocumentFonts: \n%%DocumentNeededResources: procset Adobe_level2_AI5 1.0 0\n%%+ procset Adobe_typography_AI5 1.0 0\n%%+ procset Adobe_Illustrator_AI6_vars Adobe_Illustrator_AI6\n%%+ procset Adobe_Illustrator_AI5 1.0 0\n%AI5_FileFormat 3\n%AI3_ColorUsage: Color\n%AI7_ImageSettings: 0\n%AI3_DocumentPreview: None\n%AI5_ArtSize: {width} {height}\n%AI5_RulerUnits: 0\n%AI5_ArtFlags: 1 0 0 1 0 0 1 1 0\n%AI5_TargetResolution: 800\n%AI5_NumLayers: 1\n%AI5_OpenViewLayers: 7\n%%PageOrigin:0 0\n%%AI3_PaperRect:0 0{width} {height}\n%%AI3_Margin:0 0 0 0\n%AI7_GridSettings: 72 8 72 8 1 0 0.8 0.8 0.8 0.9 0.9 0.9\n%%EndComments\n%%BeginProlog\n%%IncludeResource: procset Adobe_level2_AI5 1.0 0\n%%IncludeResource: procset Adobe_typography_AI5 1.0 0\n%%IncludeResource: procset Adobe_Illustrator_AI6_vars Adobe_Illustrator_AI6\n%%IncludeResource: procset Adobe_Illustrator_AI5 1.0 0\n%%EndProlog\n%%BeginSetup\nAdobe_level2_AI5 /initialize get exec\nAdobe_Illustrator_AI5_vars Adobe_Illustrator_AI5 Adobe_typography_AI5 /initialize get exec\nAdobe_ColorImage_AI6 /initialize get exec\nAdobe_Illustrator_AI5 /initialize get exec\n%%EndSetup\n%AI5_BeginLayer\n1 1 1 1 0 0 0 79 128 255 Lb\n(Layer 1) Ln\n10.43 M\n0.09375 w\n[] 0.0000 d\n"), false)?;
    let mut writer = AiWriter { output, formatter, thin: options.thin_stroke };
    walk_vector_output(input.geometry, &options, &mut writer)?;
    writer.output.write("LB\n%AI5_EndLayer--\n%%PageTrailer\ngsave annotatepage grestore showpage\n%%Trailer\nAdobe_Illustrator_AI5 /terminate get exec\nAdobe_ColorImage_AI6 /terminate get exec\nAdobe_typography_AI5 /terminate get exec\nAdobe_level2_AI5 /terminate get exec\n%%EOF\n", false)
}
struct AiWriter<'a> { output: &'a mut dyn TextOutput, formatter: ScaledOutputFormatter, thin: bool }
impl AiWriter<'_> {
    fn paint(&mut self) -> Result<(), VectorOutputError> { self.output.write(if self.thin { "b\n" } else { "f\n" }, false) }
    fn begin_path(&mut self, x: f64, y: f64, kind: u8) -> Result<(), VectorOutputError> {
        if kind == START_HOLE { self.paint()?; }
        if kind != 0 { let point = self.formatter.point(x, y); self.output.write(&format!("{point} m\n"), true)?; }
        Ok(())
    }
}
impl VectorWriter for AiWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 50 }
    fn begin_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> { self.output.write("u\n", false) }
    fn end_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> { self.output.write("U\n", false) }
    fn begin_shape(&mut self, color: &[u8; 4], holes: bool) -> Result<(), Self::Error> {
        let rgb = format!("{} {} {}", self.formatter.component(color[2]), self.formatter.component(color[1]), self.formatter.component(color[0]));
        self.output.write(&format!("{rgb} Xa\n"), true)?;
        if self.thin { self.output.write(&format!("{rgb} XA\n"), true)?; }
        if holes { self.output.write("u\n*u\n", false)?; }
        Ok(())
    }
    fn end_shape(&mut self, _: &[u8; 4], holes: bool) -> Result<(), Self::Error> {
        self.paint()?; if holes { self.output.write("*U\nU\n", false)?; } Ok(())
    }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        self.begin_path(start.x, start.y, kind)?;
        let point = self.formatter.point(end.x, end.y); self.output.write(&format!("{point} L\n"), true)
    }
    fn cubic(&mut self, curve: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        self.begin_path(curve[0], curve[1], kind)?;
        for index in [2, 4, 6] { let point = self.formatter.point(curve[index], curve[index + 1]); self.output.write(&format!("{point} "), false)?; }
        self.output.write(" C\n", true)
    }
}
