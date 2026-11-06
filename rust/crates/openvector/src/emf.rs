//! GDI-compatible EMF records, device bounds and positioned buffer writes.
use crate::fitting::FitNode;
use crate::vector_file::BinaryOutput;
use crate::vector_output::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmfDevice { pub pixels: [i32; 2], pub millimeters: [i32; 2], pub micrometers: [i32; 2] }

impl EmfDevice {
    /// Read the host display, not the machine that produced a reference fixture.
    /// Headless callers can pass explicit device metrics to the low-level exporter.
    pub fn current_display() -> Result<Self, VectorOutputError> {
        let (pixels, millimeters) = display_dimensions()?;
        if pixels.iter().chain(&millimeters).any(|&value| value <= 0) {
            return Err(VectorOutputError("The display did not report positive pixel and physical dimensions for EMF export.".into()));
        }
        let mut micrometers = [0; 2];
        for axis in 0..2 {
            micrometers[axis] = millimeters[axis].checked_mul(1000)
                .ok_or_else(|| VectorOutputError("The display dimensions exceed the EMF int32 range.".into()))?;
        }
        Ok(Self { pixels, millimeters, micrometers })
    }
}

#[cfg(windows)]
fn display_dimensions() -> Result<([i32; 2], [i32; 2]), VectorOutputError> {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDC(window: *mut c_void) -> *mut c_void;
        fn ReleaseDC(window: *mut c_void, dc: *mut c_void) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetDeviceCaps(dc: *mut c_void, index: i32) -> i32;
    }
    const HORZSIZE: i32 = 4;
    const VERTSIZE: i32 = 6;
    const HORZRES: i32 = 8;
    const VERTRES: i32 = 10;
    // A null window selects the same desktop reference DC used by GDI metafiles.
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return Err(VectorOutputError(format!("Cannot acquire the EMF reference display: {}", std::io::Error::last_os_error())));
        }
        let pixels = [GetDeviceCaps(dc, HORZRES), GetDeviceCaps(dc, VERTRES)];
        let millimeters = [GetDeviceCaps(dc, HORZSIZE), GetDeviceCaps(dc, VERTSIZE)];
        ReleaseDC(std::ptr::null_mut(), dc);
        Ok((pixels, millimeters))
    }
}

#[cfg(target_os = "linux")]
fn display_dimensions() -> Result<([i32; 2], [i32; 2]), VectorOutputError> {
    use x11rb::connection::Connection;
    let (connection, screen) = x11rb::connect(None)
        .map_err(|error| VectorOutputError(format!("Cannot read the X11 reference display for EMF export: {error}")))?;
    let screen = &connection.setup().roots[screen];
    Ok((
        [i32::from(screen.width_in_pixels), i32::from(screen.height_in_pixels)],
        [i32::from(screen.width_in_millimeters), i32::from(screen.height_in_millimeters)],
    ))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn display_dimensions() -> Result<([i32; 2], [i32; 2]), VectorOutputError> {
    Err(VectorOutputError("EMF reference-display discovery is not implemented on this platform.".into()))
}
const EMPTY: [i32; 4] = [0, 0, -1, -1];
const fn description() -> [u8; 128] {
    let label = b"OpenVector\0OpenVector\0\0";
    let mut bytes = [0; 128];
    let mut i = 0;
    while i < label.len() { bytes[i * 2] = label[i]; i += 1; }
    bytes
}
fn header(device: &EmfDevice, bounds: [i32; 4], frame: [i32; 4], length: u32, records: u32, handles: u32) -> [u8; 236] {
    let mut bytes = [0; 236];
    let values = [1, 236, bounds[0] as u32, bounds[1] as u32, bounds[2] as u32, bounds[3] as u32,
        frame[0] as u32, frame[1] as u32, frame[2] as u32, frame[3] as u32, 0x464d4520, 0x10000,
        length, records, handles, 64, 108, 0, device.pixels[0] as u32, device.pixels[1] as u32,
        device.millimeters[0] as u32, device.millimeters[1] as u32, 0, 0, 0, device.micrometers[0] as u32, device.micrometers[1] as u32];
    for (index, value) in values.iter().enumerate() { bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes()); }
    bytes[108..].copy_from_slice(&description()); bytes
}
pub fn export_emf(input: &VectorOutputInput<'_>, device: &EmfDevice) -> Result<Vec<u8>, VectorOutputError> {
    struct Collector(Vec<u8>);
    impl BinaryOutput for Collector {
        fn write(&mut self, bytes: &[u8], position: u64) -> Result<(), VectorOutputError> {
            let position = position as usize; self.0.resize(self.0.len().max(position + bytes.len()), 0);
            self.0[position..position + bytes.len()].copy_from_slice(bytes); Ok(())
        }
    }
    let mut output = Collector(Vec::new()); serialize_emf(input, device, &mut output)?; Ok(output.0)
}
pub fn serialize_emf(input: &VectorOutputInput<'_>, device: &EmfDevice, output: &mut dyn BinaryOutput) -> Result<(), VectorOutputError> {
    let options = read_vector_output(input, "EMF")?;
    if device.pixels.iter().chain(&device.millimeters).chain(&device.micrometers).any(|&value| value <= 0) {
        return Err(VectorOutputError("EMF referenceDevice needs pixels, millimeters and micrometers pairs of positive int32 integers.".into()));
    }
    let dimensions = transform_emf_point(options.height, options.width as f64, 0.0);
    let mut writer = EmfWriter { output, height: options.height, thin: options.thin_stroke, dimensions,
        mapper: EmfDeviceMapper::new(device.pixels, device.millimeters), pending: [0; 16384], used: 0, written: 0,
        record_count: 1, next_handle: 1, path_bounds: None, file_bounds: None };
    writer.append(&header(device, EMPTY, EMPTY, 0, 0, 1))?;
    writer.record(17, &[3])?; writer.record(9, &dimensions)?; writer.record(11, &dimensions)?;
    walk_vector_output(input.geometry, &options, &mut writer)?;
    if writer.next_handle > 0xffff { return Err(VectorOutputError("EMF needs fewer than 65535 drawing objects.".into())); }
    writer.record(14, &[0, 16, 20])?;
    let bounds = writer.file_bounds.unwrap_or(EMPTY);
    let frame = writer.mapper.frame(&bounds)?;
    let final_header = header(device, bounds, [frame[0] as i32, frame[1] as i32, frame[2] as i32, frame[3] as i32],
        (writer.written + writer.used as u64) as u32, writer.record_count, writer.next_handle as u32);
    writer.flush()?; writer.output.write(&final_header, 0)
}
struct EmfWriter<'a> { output: &'a mut dyn BinaryOutput, height: i32, thin: bool, dimensions: [i32; 2], mapper: EmfDeviceMapper,
    pending: [u8; 16384], used: usize, written: u64, record_count: u32, next_handle: i32,
    path_bounds: Option<[i32; 4]>, file_bounds: Option<[i32; 4]> }
fn include(bounds: &mut Option<[i32; 4]>, other: [i32; 4]) {
    if let Some(bounds) = bounds { bounds[0] = bounds[0].min(other[0]); bounds[1] = bounds[1].min(other[1]); bounds[2] = bounds[2].max(other[2]); bounds[3] = bounds[3].max(other[3]); }
    else { *bounds = Some(other); }
}
impl EmfWriter<'_> {
    fn flush(&mut self) -> Result<(), VectorOutputError> {
        if self.used != 0 { self.output.write(&self.pending[..self.used], self.written)?; self.written += self.used as u64; self.used = 0; } Ok(())
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), VectorOutputError> {
        if self.used + bytes.len() > self.pending.len() { self.flush()?; }
        self.pending[self.used..self.used + bytes.len()].copy_from_slice(bytes); self.used += bytes.len(); Ok(())
    }
    fn record(&mut self, kind: u32, values: &[i32]) -> Result<(), VectorOutputError> {
        let mut bytes = [0; 64]; let size = 8 + values.len() * 4;
        bytes[..4].copy_from_slice(&kind.to_le_bytes()); bytes[4..8].copy_from_slice(&(size as u32).to_le_bytes());
        for (index, value) in values.iter().enumerate() { bytes[index * 4 + 8..index * 4 + 12].copy_from_slice(&value.to_le_bytes()); }
        self.append(&bytes[..size])?; self.record_count += 1; Ok(())
    }
    fn point(&mut self, x: f64, y: f64) -> Result<[i32; 2], VectorOutputError> {
        let logical = transform_emf_point(self.height, x, y);
        let mapped = self.mapper.point(logical[0] as f64, logical[1] as f64)?;
        include(&mut self.path_bounds, [mapped[0] as i32, mapped[1] as i32, mapped[0] as i32, mapped[1] as i32]); Ok(logical)
    }
    fn move_to(&mut self, x: f64, y: f64, kind: u8) -> Result<(), VectorOutputError> {
        if kind != 0 { let point = self.point(x, y)?; self.record(27, &point)?; } Ok(())
    }
}
impl VectorWriter for EmfWriter<'_> {
    type Error = VectorOutputError;
    fn progress(&mut self, percent: i32) -> Result<(), Self::Error> { self.output.progress(percent) }
    fn accept_color(&mut self, color: &[u8; 4]) -> bool { color[3] > 50 }
    fn begin_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> { self.record(70, &[28, 0x43494447, 2, 0, 0, self.dimensions[0], self.dimensions[1], 0]) }
    fn end_group(&mut self, _: &[u8; 4]) -> Result<(), Self::Error> { self.record(70, &[8, 0x43494447, 3]) }
    fn begin_shape(&mut self, color: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        let rgb = color[2] as i32 | (color[1] as i32) << 8 | (color[0] as i32) << 16;
        if self.thin { self.record(38, &[self.next_handle, 0, 0, 0, rgb])?; self.record(37, &[self.next_handle])?; self.next_handle += 1; }
        else { self.record(37, &[0x80000008u32 as i32])?; }
        self.record(39, &[self.next_handle, 0, rgb, 0])?; self.record(37, &[self.next_handle])?; self.next_handle += 1;
        self.record(59, &[])?; self.path_bounds = None; Ok(())
    }
    fn end_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        let bounds = self.path_bounds.expect("validated fitted edges emit path coordinates");
        self.record(60, &[])?; self.record(62, &bounds)?; include(&mut self.file_bounds, bounds); Ok(())
    }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        self.move_to(start.x, start.y, kind)?; let point = self.point(end.x, end.y)?; self.record(54, &point)
    }
    fn cubic(&mut self, curve: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        self.move_to(curve[0], curve[1], kind)?;
        let points = [self.point(curve[2], curve[3])?, self.point(curve[4], curve[5])?, self.point(curve[6], curve[7])?];
        if points.iter().flatten().any(|&value| !(-32768..=32767).contains(&value)) {
            return self.record(5, &[0, 0, -1, -1, 3, points[0][0], points[0][1], points[1][0], points[1][1], points[2][0], points[2][1]]);
        }
        let mut bytes = [0; 40];
        for (index, value) in [88i32, 40, 0, 0, -1, -1, 3].iter().enumerate() { bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes()); }
        for (index, value) in points.iter().flatten().enumerate() { bytes[28 + index * 2..30 + index * 2].copy_from_slice(&(*value as i16).to_le_bytes()); }
        self.append(&bytes)?; self.record_count += 1; Ok(())
    }
}

#[cfg(test)]
mod description_tests {
    use super::description;

    #[test]
    fn description_has_project_labels_and_zero_padding() {
        let bytes = description();
        let label: Vec<u16> = "OpenVector\0OpenVector\0\0".encode_utf16().collect();
        for (index, expected) in label.iter().enumerate() {
            assert_eq!(u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]), *expected);
        }
        assert!(bytes[label.len() * 2..].iter().all(|byte| *byte == 0));
    }
}
