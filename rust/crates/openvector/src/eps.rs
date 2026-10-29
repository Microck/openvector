//! EPS output, including compact operators and local creation metadata.
use crate::fitting::FitNode;
use crate::vector_file::{collect_text, TextOutput};
use crate::vector_output::*;

#[derive(Debug, Clone, Default)]
pub struct EpsOptions { pub compact: bool, pub creation_date: Option<String> }

fn current_creation_date() -> Result<String, VectorOutputError> {
    unsafe {
        let time = libc::time(std::ptr::null_mut());
        let mut date: libc::tm = std::mem::zeroed();
        #[cfg(unix)]
        let failed = libc::localtime_r(&time, &mut date).is_null();
        #[cfg(windows)]
        let failed = libc::localtime_s(&mut date, &time) != 0;
        if failed { return Err(VectorOutputError("EPS could not read the local creation date.".into())); }
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        Ok(format!("{} {} {:2} {:02}:{:02}:{:02} {}", DAYS[date.tm_wday as usize], MONTHS[date.tm_mon as usize], date.tm_mday, date.tm_hour, date.tm_min, date.tm_sec, date.tm_year + 1900))
    }
}

pub fn export_eps(input: &VectorOutputInput<'_>, eps: &EpsOptions) -> Result<String, VectorOutputError> {
    collect_text(|output| serialize_eps(input, eps, output))
}
pub fn serialize_eps(input: &VectorOutputInput<'_>, eps: &EpsOptions, output: &mut dyn TextOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "EPS")?;
    let current_date;
    let date = match &eps.creation_date {
        Some(date) => date.as_str(),
        None => { current_date = current_creation_date()?; &current_date }
    };
    if date.is_empty() || date.len() > 255 || !date.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(VectorOutputError("EPS compact must be true or false; creationDate must be 1..255 printable ASCII characters on one line.".into()));
    }
    output.write(&format!("%!PS-Adobe-3.0 EPSF-3.0\n%%CreationDate: {date}\n%%LanguageLevel: 3\n%%BoundingBox: 0 0 {} {}\n%%EndComments\n", options.output_width, options.output_height), true)?;
    if eps.compact {
        for (alias, operator) in [("mt", "moveto"), ("ct", "curveto"), ("lt", "lineto"), ("np", "newpath"), ("cp", "closepath"), ("sc", "setrgbcolor"), ("er", "gsave fill grestore 0.09375 setlinewidth stroke"), ("lw", "setlinewidth")] {
            output.write(&format!("/{alias} {{ {operator} }} def\n"), true)?;
        }
    }
    let mut writer = EpsWriter { output, formatter: ScaledOutputFormatter::new(&options), compact: eps.compact,
        thin: options.thin_stroke, grouped: options.shape_mode == 2, started: false };
    walk_vector_output(input.geometry, &options, &mut writer)?;
    writer.output.write("%%EOF\n", true)
}
struct EpsWriter<'a> { output: &'a mut dyn TextOutput, formatter: ScaledOutputFormatter, compact: bool, thin: bool, grouped: bool, started: bool }
impl EpsWriter<'_> {
    fn operator(&self, compact: &'static str, full: &'static str) -> &'static str { if self.compact { compact } else { full } }
    fn finish(&mut self, color: &[u8; 4]) -> Result<(), VectorOutputError> {
        let close = self.operator("cp", "closepath"); let set_color = self.operator("sc", "setrgbcolor");
        let (r, g, b) = (self.formatter.component(color[2]), self.formatter.component(color[1]), self.formatter.component(color[0]));
        self.output.write(&format!("{close}\n{r} {g} {b} {set_color}\n"), true)?;
        let paint = if self.thin { self.operator("er", "gsave fill grestore 0.09375 setlinewidth stroke") } else { "fill" };
        self.output.write(&format!("{paint}\n"), true)
    }
}
impl VectorWriter for EpsWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 50 }
    fn end_group(&mut self, color: &[u8; 4]) -> Result<(), Self::Error> { self.finish(color)?; self.started = false; Ok(()) }
    fn begin_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        if !self.started || !self.grouped { self.output.write(&format!("{}\n", self.operator("np", "newpath")), true)?; self.started = true; }
        Ok(())
    }
    fn end_shape(&mut self, color: &[u8; 4], _: bool) -> Result<(), Self::Error> { if self.grouped { Ok(()) } else { self.finish(color) } }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.formatter.point(start.x, start.y); self.output.write(&format!("{point} {}\n", self.operator("mt", "moveto")), true)?; }
        let point = self.formatter.point(end.x, end.y); self.output.write(&format!("{point} {}\n", self.operator("lt", "lineto")), true)
    }
    fn cubic(&mut self, curve: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        if kind != 0 { let point = self.formatter.point(curve[0], curve[1]); self.output.write(&format!("{point} {}\n", self.operator("mt", "moveto")), true)?; }
        for index in [2, 4, 6] { let point = self.formatter.point(curve[index], curve[index + 1]); self.output.write(&format!("{point} "), false)?; }
        self.output.write(&format!("{}\n", self.operator("ct", "curveto")), true)
    }
}
