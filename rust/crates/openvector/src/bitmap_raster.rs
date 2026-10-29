//! Port of `src/bitmap-raster.mjs`: render fills and one-pixel square-capped
//! edges to owned premultiplied BGRA with the reference FreeType coverage.

mod gray_raster;
mod square_stroke;

use crate::contour_fitting::FittedGeometry;
use crate::export_options::Layout;
use crate::fitting::FitNode;
use crate::vector_output::{
    read_vector_output, reverse_cubic_points, walk_vector_output, VectorOutputInput, VectorWriter,
};
use std::collections::HashMap;
use crate::pixel_input::image_size;
use gray_raster::{Outline, ROTTEN_GLYPH};
use std::fmt;

/// Rendering failure carrying the reference error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterError(pub String);

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for RasterError {}

fn fail<T>(message: impl Into<String>) -> Result<T, RasterError> { Err(RasterError(message.into())) }

/// One path command; coordinates are `[x, y]` pairs (`C` holds three pairs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand { M([f64; 2]), L([f64; 2]), C([f64; 6]), Z }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule { #[default] EvenOdd, Nonzero }

/// A filled path, or a cosmetic one-pixel stroke of one M followed by one L/C.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterPath {
    pub color: [u8; 4],
    pub fill_rule: FillRule,
    pub stroke: bool,
    pub commands: Vec<PathCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RasterInput<'a> {
    pub width: i64,
    pub height: i64,
    pub has_alpha: bool,
    /// Defaults to transparent black with alpha, else opaque white.
    pub background: Option<[u8; 4]>,
    /// `[scaleX, scaleY, translateX, translateY]`.
    pub transform: [f64; 4],
    pub paths: &'a [RasterPath],
}

/// Owned premultiplied BGRA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage { pub width: i64, pub height: i64, pub bgra: Vec<u8>, pub has_alpha: bool }

// Packed channel multiplication uses this integer approximation to /255.
// Exact rounding differs at products such as 239*247 and changes native bytes.
fn divide255(value: u32) -> u32 { (value + (value >> 8) + 128) >> 8 }
fn divide255_simd(value: u32) -> u32 { let biased = value + 128; (biased + (biased >> 8)) >> 8 }

fn mapped_commands(path: &RasterPath, transform: &[f64; 4]) -> Result<Vec<PathCommand>, RasterError> {
    let commands = &path.commands;
    if path.stroke && (commands.len() != 2 || !matches!(commands[0], PathCommand::M(_))
        || !matches!(commands[1], PathCommand::L(_) | PathCommand::C(_))) {
        return fail("A cosmetic stroke needs one M followed by one L or C edge.");
    }
    let map = |values: &mut [f64]| -> Result<(), RasterError> {
        for (index, value) in values.iter_mut().enumerate() {
            if !value.is_finite() { return fail("Raster coordinates must be finite numbers."); }
            let mapped = *value * transform[index % 2] + transform[2 + index % 2];
            if !mapped.is_finite() || mapped.abs() > 32767.0 {
                return fail("Raster coordinates exceed the current scan converter range.");
            }
            *value = mapped;
        }
        Ok(())
    };
    commands.iter().map(|&command| {
        let mut command = command;
        match &mut command {
            PathCommand::M(values) | PathCommand::L(values) => map(values)?,
            PathCommand::C(values) => map(values)?,
            PathCommand::Z => {}
        }
        Ok(command)
    }).collect()
}

fn path_outline(commands: &[PathCommand]) -> Result<Outline, RasterError> {
    let mut outline = Outline::default();
    let mut start: Option<usize> = None;
    let close = |outline: &mut Outline, start: &mut Option<usize>| {
        let Some(first) = start.take() else { return };
        let first_point = outline.points[first];
        if *outline.points.last().unwrap() != first_point {
            outline.points.push(first_point);
            outline.tags.push(1);
        }
        outline.contours.push(outline.tags.len() as i32 - 1);
    };
    let point = |outline: &mut Outline, [x, y]: [f64; 2], tag: u8| {
        outline.points.push([(x * 64.0).trunc() as i32, (y * 64.0).trunc() as i32]);
        outline.tags.push(tag);
    };
    for command in commands {
        if let PathCommand::M(values) = *command {
            close(&mut outline, &mut start);
            start = Some(outline.tags.len());
            point(&mut outline, values, 1);
            continue;
        }
        if start.is_none() { return fail("Start each raster subpath with M."); }
        match *command {
            PathCommand::L(values) => point(&mut outline, values, 1),
            PathCommand::C(v) => {
                point(&mut outline, [v[0], v[1]], 2);
                point(&mut outline, [v[2], v[3]], 2);
                point(&mut outline, [v[4], v[5]], 1);
            }
            _ => close(&mut outline, &mut start),
        }
    }
    close(&mut outline, &mut start);
    Ok(outline)
}

/// Each coverage attempt: QPainter has already composed successful bands when
/// it grows the pool, so every attempt, including partial ones, is composited.
fn path_coverage(width: i64, height: i64, outline: &Outline, even_odd: bool, pool_size: &mut i32,
    mut composite: impl FnMut(&[u8])) -> Result<(), RasterError> {
    let mut pixels = vec![0u8; (width * height) as usize];
    loop {
        pixels.fill(0);
        let status = gray_raster::coverage(width as i32, height as i32, outline, even_odd, &mut pixels, *pool_size);
        if status != 0 && status != ROTTEN_GLYPH { return fail(format!("Raster coverage failed with status {status}.")); }
        composite(&pixels);
        if status == 0 { return Ok(()); }
        *pool_size *= 2;
        if *pool_size > 1024 * 1024 { return fail("Raster coverage exceeds the one MiB pool limit."); }
    }
}

/// Render fills and one-pixel square-capped edges to owned premultiplied BGRA.
/// Fitted scene assembly remains a separate operation.
pub fn rasterize_paths(input: &RasterInput) -> Result<RasterImage, RasterError> {
    let &RasterInput { width, height, has_alpha, background, transform, paths } = input;
    let pixels = image_size(width, height).map_err(|error| RasterError(error.0))?;
    if width > 32767 || height > 32767 { return fail("Raster dimensions exceed the current scan converter range."); }
    if transform.iter().any(|value| !value.is_finite()) {
        return fail("Raster transform needs four finite scale/translation values.");
    }
    let initial = background.unwrap_or(if has_alpha { [0, 0, 0, 0] } else { [255, 255, 255, 255] });
    if !has_alpha && initial[3] != 255 { return fail("Opaque raster output needs an opaque background."); }
    let premultiply = |color: [u8; 4]| -> [u32; 4] {
        let mut result = color.map(u32::from);
        for channel in &mut result[..3] { *channel = divide255(*channel * color[3] as u32); }
        result
    };
    let fill = premultiply(initial).map(|value| value as u8);
    let mut bgra = fill.repeat(pixels);
    // The reference RGB32 blend remains scalar. Its premultiplied ARGB32 blend
    // uses MMX/SSE, which biases before the high-byte correction instead.
    let multiply: fn(u32) -> u32 = if has_alpha { divide255_simd } else { divide255 };
    let mut pool_size = 8192;
    for path in paths {
        let commands = mapped_commands(path, &transform)?;
        let outline = if path.stroke {
            let PathCommand::M(start) = commands[0] else { unreachable!() };
            path_outline(&square_stroke::square_stroke(start, &commands[1]))?
        } else {
            path_outline(&commands)?
        };
        let premultiplied = premultiply(path.color);
        let alpha_source = path.color[3] as u32;
        let even_odd = !path.stroke && path.fill_rule == FillRule::EvenOdd;
        path_coverage(width, height, &outline, even_odd, &mut pool_size, |coverage| {
            for (pixel, &amount) in coverage.iter().enumerate() {
                if amount == 0 { continue; }
                let amount = amount as u32;
                let alpha = multiply(alpha_source * amount);
                let inverse = 255 - alpha;
                let target = &mut bgra[pixel * 4..pixel * 4 + 4];
                for channel in 0..3 {
                    target[channel] = (multiply(premultiplied[channel] * amount)
                        + multiply(target[channel] as u32 * inverse)) as u8;
                }
                target[3] = (alpha + multiply(target[3] as u32 * inverse)) as u8;
            }
        })?;
    }
    Ok(RasterImage { width, height, bgra, has_alpha })
}

/// Collects cutout fills in exporter order as raster paths.
struct FillCollector { fills: Vec<RasterPath> }

impl VectorWriter for FillCollector {
    type Error = std::convert::Infallible;
    fn accept_color(&mut self, _bgra: &[u8; 4]) -> bool { true }
    fn begin_shape(&mut self, bgra: &[u8; 4], _has_holes: bool) -> Result<(), Self::Error> {
        self.fills.push(RasterPath { color: *bgra, fill_rule: FillRule::EvenOdd, stroke: false, commands: Vec::new() });
        Ok(())
    }
    fn end_shape(&mut self, _bgra: &[u8; 4], _has_holes: bool) -> Result<(), Self::Error> { Ok(()) }
    fn line(&mut self, start: &FitNode, end: &FitNode, start_kind: u8) -> Result<(), Self::Error> {
        let commands = &mut self.fills.last_mut().unwrap().commands;
        if start_kind != 0 { commands.push(PathCommand::M([start.x, start.y])); }
        commands.push(PathCommand::L([end.x, end.y]));
        Ok(())
    }
    fn cubic(&mut self, points: &[f64; 8], start_kind: u8, _backwards: bool) -> Result<(), Self::Error> {
        let commands = &mut self.fills.last_mut().unwrap().commands;
        if start_kind != 0 { commands.push(PathCommand::M([points[0], points[1]])); }
        commands.push(PathCommand::C(points[2..].try_into().unwrap()));
        Ok(())
    }
}

/// `renderBitmap` input: completed fitted geometry at source size.
#[derive(Debug, Clone, Copy)]
pub struct BitmapScene<'a> {
    pub geometry: &'a FittedGeometry,
    pub width: i64,
    pub height: i64,
    pub scale: f64,
    pub has_alpha: bool,
    pub hidden_regions: &'a [i64],
}

/// Render completed fitted geometry to the native premultiplied image storage.
/// File-specific channel conversion belongs to bitmap-export.
pub fn render_bitmap(scene: &BitmapScene) -> Result<RasterImage, RasterError> {
    let &BitmapScene { geometry, width, height, scale, has_alpha, hidden_regions } = scene;
    if !scale.is_finite() || scale <= 0.0 { return fail("Bitmap scale must be a positive finite number."); }
    let options = read_vector_output(&VectorOutputInput { layout: Layout::Cutouts, hidden_regions,
        ..VectorOutputInput::new(geometry, width, height) }, "Bitmap").map_err(|error| RasterError(error.0))?;
    let mut collector = FillCollector { fills: Vec::new() };
    let Ok(()) = walk_vector_output(geometry, &options, &mut collector);
    // The native scene preallocates cubic slots, then appends every line in
    // region traversal order. Shared cubic slots retain the last write.
    let mut seams: Vec<Option<RasterPath>> = Vec::new();
    if !has_alpha {
        let curves: HashMap<usize, &[f64; 8]> = geometry.cubics.iter().map(|curve| (curve.index, &curve.points)).collect();
        seams.resize(geometry.cubic_storage_slots, None);
        for region in &geometry.regions {
            for edge in &region.edges {
                let neighbor = if edge[2] < 0 { [region.bgra[0], region.bgra[1], region.bgra[2], 0] }
                    else { geometry.regions[edge[2] as usize].bgra };
                let color = std::array::from_fn(|i| ((region.bgra[i] as u32 + neighbor[i] as u32) / 2) as u8);
                let commands = if edge[1] == 0 {
                    let (start, end) = (&geometry.nodes[edge[3] as usize], &geometry.nodes[edge[4] as usize]);
                    vec![PathCommand::M([start.x, start.y]), PathCommand::L([end.x, end.y])]
                } else {
                    let points = curves[&(edge[0] as usize)];
                    let oriented = if edge[1] == 2 { reverse_cubic_points(points) } else { *points };
                    vec![PathCommand::M([oriented[0], oriented[1]]), PathCommand::C(oriented[2..].try_into().unwrap())]
                };
                let path = RasterPath { color, fill_rule: FillRule::EvenOdd, stroke: true, commands };
                if edge[1] == 0 { seams.push(Some(path)); } else { seams[edge[0] as usize] = Some(path); }
            }
        }
    }
    let paths: Vec<RasterPath> = seams.into_iter().flatten().filter(|path| path.color[3] >= 250)
        .chain(collector.fills).collect();
    rasterize_paths(&RasterInput {
        width: (width as f64 * scale + 0.5).floor() as i64,
        height: (height as f64 * scale + 0.5).floor() as i64,
        has_alpha, background: None, transform: [scale, scale, 0.0, 0.0], paths: &paths,
    })
}
