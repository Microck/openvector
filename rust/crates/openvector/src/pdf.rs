//! PDF 1.4 output with single-byte header, CRLF offsets and alpha resources.
use crate::fitting::FitNode;
use crate::vector_file::TextOutput;
use crate::vector_output::*;

pub fn export_pdf(input: &VectorOutputInput<'_>) -> Result<Vec<u8>, VectorOutputError> {
    struct Collector(Vec<u8>);
    impl TextOutput for Collector {
        fn write(&mut self, text: &str, _: bool) -> Result<(), VectorOutputError> {
            for character in text.chars() {
                if character == '\n' { self.0.push(b'\r'); }
                self.0.push(character as u8);
            }
            Ok(())
        }
    }
    let mut output = Collector(Vec::new());
    serialize_pdf(input, &mut output)?;
    Ok(output.0)
}
pub fn serialize_pdf(input: &VectorOutputInput<'_>, output: &mut dyn TextOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "PDF")?;
    let mut writer = PdfWriter { output, formatter: ScaledOutputFormatter::new(&options), position: 0,
        offsets: [0; 9], used_alpha: [false; 256], alpha: 255, grouped: options.shape_mode == 2, thin: options.thin_stroke };
    writer.write("%PDF-1.4\n%\u{c7}\u{ec}\u{8f}\u{a2}\n")?;
    writer.object(5)?; writer.write("<</Length 6 0 R>>\nstream\n")?;
    let content_start = writer.position;
    writer.write("0.09375 w\n")?;
    walk_vector_output(input.geometry, &options, &mut writer)?;
    let content_length = writer.position - content_start + 1;
    writer.write("\nendstream\nendobj\n")?;
    writer.object(6)?; writer.write(&format!("{content_length}\nendobj\n"))?;
    writer.object(4)?;
    writer.write(&format!("<</Type/Page/MediaBox [0 0 {} {}]\n/Parent 3 0 R\n/Resources<</ProcSet[/PDF]\n/ExtGState 8 0 R\n>>\n/Contents 5 0 R\n>>\nendobj\n", options.width, options.height))?;
    writer.object(3)?; writer.write("<< /Type /Pages /Kids [\n4 0 R\n] /Count 1\n>>\nendobj\n")?;
    writer.object(1)?; writer.write("<</Type /Catalog /Pages 3 0 R\n>>\nendobj\n")?;
    writer.object(7)?; writer.write("<</Type/ExtGState\n/OPM 1>>endobj\n")?;
    writer.object(8)?; writer.write("<</R7\n7 0 R\n")?;
    for alpha in 0..256 { if writer.used_alpha[alpha] {
        let opacity = writer.formatter.component(alpha as u8);
        writer.write(&format!("/Vmt{alpha} <</CA {opacity}/ca {opacity}>>\n"))?;
    } }
    writer.write(">>endobj\n")?;
    writer.object(2)?; writer.write("<</Producer(OpenVector)     \n>>endobj\n")?;
    let xref = writer.position;
    writer.write("xref\n0 9\n0000000000 65535 f\n")?;
    for number in 1..9 { writer.write(&format!("{:010} 00000 n\n", writer.offsets[number]))?; }
    writer.write(&format!("trailer\n<< /Size 10 /Root 1 0 R /Info 2 0 R\n>>\nstartxref\n{xref}\n%%EOF\n"))
}
struct PdfWriter<'a> { output: &'a mut dyn TextOutput, formatter: ScaledOutputFormatter, position: usize,
    offsets: [usize; 9], used_alpha: [bool; 256], alpha: u8, grouped: bool, thin: bool }
impl PdfWriter<'_> {
    fn write(&mut self, text: &str) -> Result<(), VectorOutputError> {
        self.position += text.chars().count() + text.bytes().filter(|&byte| byte == b'\n').count();
        self.output.write(text, text.ends_with('\n'))
    }
    fn object(&mut self, number: usize) -> Result<(), VectorOutputError> { self.offsets[number] = self.position; self.write(&format!("{number} 0 obj\n")) }
    fn paint(&mut self) -> Result<(), VectorOutputError> { self.write(if self.thin { "b\n" } else { "f\n" }) }
}
impl VectorWriter for PdfWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 10 }
    fn end_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> { self.paint() }
    fn begin_shape(&mut self, color: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        let rgb = format!("{} {} {}", self.formatter.component(color[2]), self.formatter.component(color[1]), self.formatter.component(color[0]));
        self.write(&format!("{rgb} RG\n{rgb} rg\n"))?;
        if color[3] != self.alpha { self.alpha = color[3]; self.used_alpha[self.alpha as usize] = true; self.write(&format!("/Vmt{} gs\n", self.alpha))?; }
        Ok(())
    }
    fn end_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> { if self.grouped { Ok(()) } else { self.paint() } }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.formatter.point(start.x, start.y); self.write(&format!("{point} m\n"))?; }
        let point = self.formatter.point(end.x, end.y); self.write(&format!("{point} l\n"))
    }
    fn cubic(&mut self, curve: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.formatter.point(curve[0], curve[1]); self.write(&format!("{point} m\n"))?; }
        for index in [2, 4, 6] { let point = self.formatter.point(curve[index], curve[index + 1]); self.write(&format!("{point} "))?; }
        self.write("c\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contour_fitting::FittedGeometry;

    #[test]
    fn public_pdf_producer_and_xref_point_to_the_actual_table() {
        let geometry = FittedGeometry { cubic_storage_slots: 0, cubics: vec![], nodes: vec![], regions: vec![] };
        let bytes = export_pdf(&VectorOutputInput::new(&geometry, 4, 4)).unwrap();
        assert!(bytes.windows(b"/Producer(OpenVector)".len()).any(|part| part == b"/Producer(OpenVector)"));
        assert!(!bytes.windows(b"VectorMagic.com".len()).any(|part| part == b"VectorMagic.com"));
        let marker = b"startxref\r\n";
        let at = bytes.windows(marker.len()).rposition(|part| part == marker).unwrap() + marker.len();
        let end = at + bytes[at..].windows(2).position(|part| part == b"\r\n").unwrap();
        let offset: usize = std::str::from_utf8(&bytes[at..end]).unwrap().parse().unwrap();
        assert!(bytes[offset..].starts_with(b"xref\r\n"));
    }
}
