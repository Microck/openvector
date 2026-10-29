use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct BitmapSize { pub scale: f64, pub original: [i32; 2], pub panes: [[i32; 2]; 2], pub pane_scales: [f64; 2] }
impl BitmapSize {
    pub fn new(scale: f64) -> Result<Self, String> {
        let mut size = Self { scale: 1.0, original: [150, 250], panes: [[150, 250]; 2], pane_scales: [1.0; 2] };
        size.commit(0, 2, scale)?; size.commit(1, 2, scale)?; Ok(size)
    }
    pub fn source_changed(&mut self, width: usize, height: usize) -> Result<(), String> {
        let mut next = self.clone();
        next.original = [i32::try_from(width).ok().filter(|v| *v > 0).ok_or("Bitmap width must be a positive signed integer.")?, i32::try_from(height).ok().filter(|v| *v > 0).ok_or("Bitmap height must be a positive signed integer.")?];
        next.commit(0, 2, next.scale)?; next.commit(1, 2, next.scale)?; *self = next; Ok(())
    }
    pub fn commit(&mut self, mode: usize, axis: usize, value: f64) -> Result<(), String> {
        if mode > 1 || axis > 2 || axis < 2 && (!value.is_finite() || value.fract() != 0.0 || value < i32::MIN as f64 || value > i32::MAX as f64) { return Err("Commit a numeric scale or signed integer width/height.".into()); }
        let mut next = self.clone(); let value = if axis < 2 { value.clamp(1.0, 10000.0) } else { value };
        if axis < 2 { next.panes[mode][axis] = value as i32; }
        next.update(mode, axis, value, &mut HashSet::new())?; next.pane_scales[mode] = next.scale; *self = next; Ok(())
    }
    pub fn step(&mut self, mode: usize, axis: usize, direction: i32, large: bool) -> Result<(), String> {
        if mode > 1 || axis > 1 || ![-1, 1].contains(&direction) { return Err("Choose a width or height and step direction -1 or 1.".into()); }
        let increment = if large { 25 } else { 5 }; let current = self.panes[mode][axis];
        let value = if direction == 1 { (current / increment + 1) * increment } else { 10000 - ((10000 - current) / increment + 1) * increment };
        self.commit(mode, axis, value as f64)
    }
    fn update(&mut self, mode: usize, axis: usize, value: f64, seen: &mut HashSet<(usize, i32)>) -> Result<(), String> {
        if axis == 2 {
            self.scale = value;
            self.resolve(mode, 0, dimension(self.original[0] as f64 * self.scale + 0.5), seen)?;
            self.resolve(mode, 1, dimension(self.original[1] as f64 * self.scale + 0.5), seen)?;
        } else {
            self.scale = value / self.original[axis] as f64; let other = 1 - axis;
            self.resolve(mode, other, dimension(self.original[other] as f64 * self.scale + 0.5), seen)?;
        }
        Ok(())
    }
    fn resolve(&mut self, mode: usize, axis: usize, requested: i32, seen: &mut HashSet<(usize, i32)>) -> Result<(), String> {
        let value = requested.clamp(1, 10000); self.panes[mode][axis] = value;
        if requested != value {
            if !seen.insert((axis, value)) { return Err("Bitmap dimensions cannot settle within 1 to 10000 pixels. Use the CLI bitmap scale option for this aspect ratio.".into()); }
            self.update(mode, axis, value as f64, seen)?;
        }
        Ok(())
    }
}
fn dimension(value: f64) -> i32 { if !value.is_finite() || !(-(2.0_f64.powi(63))..2.0_f64.powi(63)).contains(&value) { 0 } else { value.trunc() as i64 as i32 } }
