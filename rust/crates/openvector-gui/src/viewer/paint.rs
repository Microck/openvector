use gpui::{Bounds, Pixels, PathBuilder, Window, point, px, rgba};
use openvector::{fitting::FitNode, vector_output::{VectorOutputInput, VectorOutputError, VectorWriter, read_vector_output, walk_vector_output}};
use super::geometry::ViewerView;

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Move(f64, f64), Line(f64, f64), Cubic([f64; 6]), Close,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub color: u32,
    pub segments: Vec<Segment>,
}
#[derive(Default, Clone, Debug)]
pub struct Scene { pub shapes: Vec<Shape>, include_transparent: bool }
impl Scene {
    pub fn from_output(input: &VectorOutputInput<'_>) -> Result<Self, VectorOutputError> {
        let options = read_vector_output(input, "Viewer")?;
        let mut scene = Self::default();
        walk_vector_output(input.geometry, &options, &mut scene)?;
        Ok(scene)
    }
    /// Removed-region outlines include alpha-zero regions, unlike the filled scene.
    pub fn from_removed_output(input: &VectorOutputInput<'_>) -> Result<Self, VectorOutputError> {
        let options = read_vector_output(input, "Viewer")?;
        let mut scene = Self { include_transparent: true, ..Self::default() };
        walk_vector_output(input.geometry, &options, &mut scene)?;
        Ok(scene)
    }
    pub fn paint(&self, bounds: Bounds<Pixels>, view: &ViewerView, paths: bool, outline: bool, window: &mut Window) {
        let at = |x: f64, y: f64| point(bounds.origin.x + px((view.origin.x + x * view.zoom) as f32),
            bounds.origin.y + px((view.origin.y + y * view.zoom) as f32));
        let passes = [outline, true];
        let passes = &passes[..if paths && !outline { 2 } else { 1 }];
        for shape in &self.shapes {
            for stroke in passes {
                let mut builder = if *stroke { PathBuilder::stroke(px(1.0)) } else { PathBuilder::fill() };
                if outline { builder = builder.dash_array(&[px(4.0), px(3.0)]); }
                for segment in &shape.segments {
                    match *segment {
                        Segment::Move(x, y) => builder.move_to(at(x, y)),
                        Segment::Line(x, y) => builder.line_to(at(x, y)),
                        Segment::Cubic(p) => builder.cubic_bezier_to(at(p[4], p[5]), at(p[0], p[1]), at(p[2], p[3])),
                        Segment::Close => builder.close(),
                    }
                }
                if let Ok(path) = builder.build() {
                    let color = if outline { 0xf87171ff } else if *stroke { 0x000000ff } else { shape.color };
                    window.paint_path(path, rgba(color));
                }
            }
        }
    }
}
impl VectorWriter for Scene {
    type Error = VectorOutputError;
    fn accept_color(&mut self, bgra: &[u8; 4]) -> bool { self.include_transparent || bgra[3] != 0 }
    fn begin_shape(&mut self, bgra: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        self.shapes.push(Shape { color: u32::from_be_bytes([bgra[2], bgra[1], bgra[0], bgra[3]]), segments: Vec::new() });
        Ok(())
    }
    fn end_shape(&mut self, _: &[u8; 4], _: bool) -> Result<(), Self::Error> {
        self.shapes.last_mut().expect("writer shape opened").segments.push(Segment::Close);
        Ok(())
    }
    fn line(&mut self, start: &FitNode, end: &FitNode, kind: u8) -> Result<(), Self::Error> {
        let segments = &mut self.shapes.last_mut().expect("writer shape opened").segments;
        if kind != 0 {
            if !segments.is_empty() { segments.push(Segment::Close); }
            segments.push(Segment::Move(start.x, start.y));
        }
        segments.push(Segment::Line(end.x, end.y));
        Ok(())
    }
    fn cubic(&mut self, p: &[f64; 8], kind: u8, _: bool) -> Result<(), Self::Error> {
        let segments = &mut self.shapes.last_mut().expect("writer shape opened").segments;
        if kind != 0 {
            if !segments.is_empty() { segments.push(Segment::Close); }
            segments.push(Segment::Move(p[0], p[1]));
        }
        segments.push(Segment::Cubic([p[2], p[3], p[4], p[5], p[6], p[7]]));
        Ok(())
    }
}
