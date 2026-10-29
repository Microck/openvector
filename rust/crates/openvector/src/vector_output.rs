//! Port of `src/vector-output.mjs`: shared, read-only traversal and numeric
//! helpers for every vector and bitmap writer of completed fitted geometry.
//!
//! Output kernels receive a [`FittedGeometry`], read the shared graph and
//! return owned path/order arrays without moving its nodes. Serializers
//! validate their common input once with [`read_vector_output`], then visit
//! shapes in native exporter order through [`walk_vector_output`] and a
//! [`VectorWriter`]. JavaScript type checks that Rust types already enforce
//! (`onProgress` being a function, writer callbacks, layout spelling, both
//! output dimensions or neither) have no runtime counterpart here.

use crate::contour_fitting::FittedGeometry;
use crate::decimal_format::{fixed_decimal, general_rgb_decimal, stream_fixed_decimal};
use crate::export_options::Layout;
use crate::extended_arithmetic::{binary_double, binary_parts, coordinate_int32, round_significand};
use crate::fitting::FitNode;
use num_bigint::BigInt;
use num_traits::{Signed, Zero};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Validation or output failure carrying the reference error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorOutputError(pub String);

impl fmt::Display for VectorOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for VectorOutputError {}

fn fail<T>(message: impl Into<String>) -> Result<T, VectorOutputError> { Err(VectorOutputError(message.into())) }

/// A hole path step: `(child region, edge index in that child)`.
pub type HoleStep = (usize, usize);

/// `assembleOutputShapes` result. `color_order` holds `(region, flag)` where
/// flag 0 opens a color group, 1 closes it, 2 does both and 3 neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputShapes {
    pub children: Vec<Vec<usize>>,
    pub holes: Vec<Vec<Vec<HoleStep>>>,
    pub color_order: Vec<(usize, u8)>,
}

fn color_order(geometry: &FittedGeometry) -> Vec<(usize, u8)> {
    let mut classes: Vec<Vec<usize>> = Vec::new();
    let mut by_color: HashMap<[u8; 4], usize> = HashMap::new();
    for (region, shape) in geometry.regions.iter().enumerate() {
        let index = *by_color.entry(shape.bgra).or_insert_with(|| { classes.push(Vec::new()); classes.len() - 1 });
        classes[index].push(region);
    }
    classes.iter().flat_map(|group| group.iter().enumerate().map(move |(index, &region)| {
        let flag = if group.len() == 1 { 2 } else if index == 0 { 0 } else if index == group.len() - 1 { 1 } else { 3 };
        (region, flag)
    })).collect()
}

/// Child lists, hole paths per parent and the grouped-cutout color order.
pub fn assemble_output_shapes(geometry: &FittedGeometry) -> OutputShapes {
    let regions = &geometry.regions;
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); regions.len()];
    for (region, shape) in regions.iter().enumerate() {
        if shape.parent != -1 { children[shape.parent as usize].push(region); }
    }
    let holes = children.iter().enumerate().map(|(parent, nested)| {
        let mut runs: Vec<Vec<HoleStep>> = Vec::new();
        for &child in nested {
            let edges = &regions[child].edges;
            let mut run = Vec::new();
            for (edge, values) in edges.iter().enumerate() {
                if values[2] == parent as i64 { run.push((child, edge)); }
                else if !run.is_empty() { runs.push(std::mem::take(&mut run)); }
            }
            if !run.is_empty() { runs.push(run); }
        }
        let mut following = vec![-1i64; runs.len()];
        // Later runs replace earlier entries, preserving the native scan's
        // last-match rule at junctions while avoiding a quadratic scan.
        let mut by_start: HashMap<i64, usize> = HashMap::new();
        for (index, run) in runs.iter().enumerate() {
            let (region, edge) = run[0];
            by_start.insert(regions[region].edges[edge][3], index);
        }
        for (index, run) in runs.iter().enumerate() {
            let (region, edge) = run[run.len() - 1];
            if let Some(&candidate) = by_start.get(&regions[region].edges[edge][4]) { following[index] = candidate as i64; }
        }
        let mut paths = Vec::new();
        for index in 0..runs.len() {
            if following[index] < 0 { continue; }
            let mut path = Vec::new();
            let mut current = index;
            while following[current] >= 0 {
                path.extend_from_slice(&runs[current]);
                let next = following[current] as usize;
                following[current] = -1;
                current = next;
            }
            paths.push(path);
        }
        paths
    }).collect();
    OutputShapes { children, holes, color_order: color_order(geometry) }
}

/// Original hidden-bit range rule, including the default visible tail.
pub fn is_output_region_visible(hidden_size: usize, hidden_words: &[u32], region: usize) -> bool {
    region >= hidden_size || (1u32 << (region & 31)) & hidden_words[region >> 5] == 0
}

fn region_contains_point(contour: &[usize], node: impl Fn(usize) -> [f64; 2], [x, y]: [f64; 2]) -> bool {
    let mut previous = node(contour[contour.len() - 1]);
    let mut inside = false;
    for &index in contour {
        let current = node(index);
        if (previous[1] > y) != (current[1] > y)
            && x < (y - previous[1]) * (current[0] - previous[0]) / (current[1] - previous[1]) + previous[0] {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Native 0x488460: test the label's live smoothed boundary first, then visit
/// distinct neighboring labels in the original expanding-ring order. Fitted
/// Bezier control points and visibility bits do not participate in hit
/// testing. `indices` is the width*height region label map. Returns -1 when
/// no region contains the point.
pub fn find_output_region(width: i32, height: i32, indices: &[i32], geometry: &FittedGeometry, point: [f64; 2]) -> i64 {
    find_region(width, height, indices, point, |region| {
        region_contains_point(&geometry.regions[region].contour, |index| {
            let node = &geometry.nodes[index];
            [node.x, node.y]
        }, point)
    })
}

/// Hit-test live contours before or after fitting without copying their nodes.
/// This uses the same native label and expanding-ring order as fitted output.
pub fn find_contour_region(width: i32, height: i32, indices: &[i32], geometry: &crate::contours::Geometry, point: [f64; 2]) -> i64 {
    find_region(width, height, indices, point, |region| {
        region_contains_point(&geometry.regions[region].contour, |index| {
            let node = &geometry.nodes[index];
            [node.x, node.y]
        }, point)
    })
}

fn find_region(width: i32, height: i32, indices: &[i32], point: [f64; 2], contains: impl Fn(usize) -> bool) -> i64 {
    let [x, y] = point;
    let (column, row) = (coordinate_int32(x), coordinate_int32(y));
    if column < 0 || column >= width || row < 0 || row >= height { return -1; }
    let initial = indices[(row as usize) * width as usize + column as usize];
    if contains(initial as usize) { return initial as i64; }
    let mut visited = HashSet::from([initial]);
    let limit = (width - column).max(column).max(height - row).max(row);
    for radius in 1..limit {
        let mut step = 1;
        for horizontal in -radius..=radius {
            let mut vertical = -radius;
            while vertical <= radius {
                let (candidate_x, candidate_y) = (column + horizontal, row + vertical);
                if candidate_x >= 0 && candidate_x < width && candidate_y >= 0 && candidate_y < height {
                    let region = indices[(candidate_y as usize) * width as usize + candidate_x as usize];
                    if visited.insert(region) && contains(region as usize) {
                        return region as i64;
                    }
                }
                vertical += step;
            }
            step = if horizontal < radius { radius * 2 } else { 1 };
        }
    }
    -1
}

/// Copy a cubic's four coordinate pairs in reverse order.
pub fn reverse_cubic_points(points: &[f64; 8]) -> [f64; 8] {
    [points[6], points[7], points[4], points[5], points[2], points[3], points[0], points[1]]
}

/// Native save progress uses the float32 segment at 0xa22ea0, not a
/// completed-row ratio. All 51 integer outcomes are separately captured.
pub fn export_progress_percent(phase: i32) -> i32 {
    let begin = 0.056559f32 as f64;
    let end = 0.96322f32 as f64;
    ((begin + (end - begin) * (phase as f64 / 50.0)) * 100.0 + 0.5).trunc() as i32
}

/// Start kind of a path segment: continuation, outer path or hole.
pub const START_CONTINUE: u8 = 0;
pub const START_OUTER: u8 = 1;
pub const START_HOLE: u8 = 2;

/// Callbacks in native exporter order. Every callback may fail; the first
/// failure stops the walk and is returned unchanged (consumer failures and
/// cancellation keep the bytes a writer already emitted).
pub trait VectorWriter {
    type Error;
    /// Progress percent from [`export_progress_percent`], reported before a
    /// row's shape when its phase changes (every fourth row is checked).
    fn progress(&mut self, _percent: i32) -> Result<(), Self::Error> { Ok(()) }
    fn accept_color(&mut self, bgra: &[u8; 4]) -> bool;
    fn begin_group(&mut self, _bgra: &[u8; 4]) -> Result<(), Self::Error> { Ok(()) }
    fn end_group(&mut self, _bgra: &[u8; 4]) -> Result<(), Self::Error> { Ok(()) }
    fn begin_shape(&mut self, bgra: &[u8; 4], has_holes: bool) -> Result<(), Self::Error>;
    fn end_shape(&mut self, bgra: &[u8; 4], has_holes: bool) -> Result<(), Self::Error>;
    fn line(&mut self, start: &FitNode, end: &FitNode, start_kind: u8) -> Result<(), Self::Error>;
    /// Oriented points, start kind, then the native direction flag.
    fn cubic(&mut self, points: &[f64; 8], start_kind: u8, backwards: bool) -> Result<(), Self::Error>;
}

/// Walk native exporter order without moving shared geometry. Start kinds are
/// 0 for continuation, 1 for an outer path and 2 for a hole.
pub fn walk_vector_output<W: VectorWriter>(geometry: &FittedGeometry, options: &VectorOutputOptions, writer: &mut W)
    -> Result<(), W::Error> {
    let FittedGeometry { regions, nodes, cubics, .. } = geometry;
    let curves: HashMap<usize, &[f64; 8]> = cubics.iter().map(|curve| (curve.index, &curve.points)).collect();
    let OutputShapes { holes, color_order, .. } = assemble_output_shapes(geometry);
    let emit_edge = |writer: &mut W, region: usize, edge: &[i64; 7], reversed: bool, start_kind: u8| {
        if edge[1] == 0 {
            let contour = &regions[region].contour;
            let start = &nodes[contour[edge[0] as usize]];
            let end = &nodes[contour[(edge[0] as usize + 1) % contour.len()]];
            if reversed { writer.line(end, start, start_kind) } else { writer.line(start, end, start_kind) }
        } else {
            let backwards = (edge[1] == 2) != reversed;
            let points = curves[&(edge[0] as usize)];
            if backwards { writer.cubic(&reverse_cubic_points(points), start_kind, true) }
            else { writer.cubic(points, start_kind, false) }
        }
    };
    let shape_mode = options.shape_mode;
    let mut previous_phase = -1;
    for row in 0..regions.len() {
        if row & 3 == 0 {
            let phase = (row as f64 * 50.0 / regions.len() as f64 + 0.5).trunc() as i32;
            if phase != previous_phase {
                previous_phase = phase;
                writer.progress(export_progress_percent(phase))?;
            }
        }
        let (region, flag) = if shape_mode == 2 { color_order[row] } else { (row, 3) };
        let shape = &regions[region];
        if !writer.accept_color(&shape.bgra) { continue; }
        if shape_mode == 2 && (flag == 0 || flag == 2) { writer.begin_group(&shape.bgra)?; }
        if is_output_region_visible(options.hidden_size, &options.hidden_words, region) {
            let has_holes = shape_mode != 0 && !holes[region].is_empty();
            writer.begin_shape(&shape.bgra, has_holes)?;
            for (index, edge) in shape.edges.iter().enumerate() {
                emit_edge(writer, region, edge, false, if index == 0 { START_OUTER } else { START_CONTINUE })?;
            }
            if shape_mode != 0 {
                for path in &holes[region] {
                    for index in (0..path.len()).rev() {
                        let (child, edge) = path[index];
                        let kind = if index == path.len() - 1 { START_HOLE } else { START_CONTINUE };
                        emit_edge(writer, child, &regions[child].edges[edge], true, kind)?;
                    }
                }
            }
            writer.end_shape(&shape.bgra, has_holes)?;
        }
        if shape_mode == 2 && (flag == 1 || flag == 2) { writer.end_group(&shape.bgra)?; }
    }
    Ok(())
}

/// The in-memory API consumes the same ordered text as a file writer. Keeping
/// collection outside each serializer preserves bytes already emitted on
/// failure. `serialize` receives the chunk sink; LF becomes CRLF.
pub fn collect_vector_text<E>(serialize: impl FnOnce(&mut dyn FnMut(&str) -> Result<(), E>) -> Result<(), E>)
    -> Result<String, E> {
    let mut text = String::new();
    serialize(&mut |chunk: &str| { text.push_str(chunk); Ok(()) })?;
    Ok(text.replace('\n', "\r\n"))
}

/// Common export input (`readVectorOutput`). `output_size` is
/// `(outputWidth, outputHeight)`; `None` uses the source dimensions.
#[derive(Debug, Clone, Copy)]
pub struct VectorOutputInput<'a> {
    pub geometry: &'a FittedGeometry,
    pub width: i64,
    pub height: i64,
    pub output_size: Option<(i64, i64)>,
    pub layout: Layout,
    pub thin_stroke: bool,
    pub hidden_regions: &'a [i64],
}

impl<'a> VectorOutputInput<'a> {
    /// Defaults: stacked layout, no thin stroke, nothing hidden.
    pub fn new(geometry: &'a FittedGeometry, width: i64, height: i64) -> Self {
        VectorOutputInput { geometry, width, height, output_size: None, layout: Layout::Stacked, thin_stroke: false,
            hidden_regions: &[] }
    }
}

/// Validated export options. `shape_mode` is 0 stacked, 1 cutouts, 2 grouped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorOutputOptions {
    pub width: i32,
    pub height: i32,
    pub output_width: i32,
    pub output_height: i32,
    pub shape_mode: u8,
    pub thin_stroke: bool,
    pub hidden_size: usize,
    pub hidden_words: Vec<u32>,
}

fn dimension(value: i64) -> bool { value > 0 && value <= 0x7fffffff }
fn identifier(value: i64, length: usize) -> bool { value >= 0 && (value as u64) < length as u64 }

fn validate_fitted_geometry(geometry: &FittedGeometry, format: &str) -> Result<(), VectorOutputError> {
    let FittedGeometry { nodes, regions, cubics, cubic_storage_slots } = geometry;
    if nodes.is_empty() || regions.is_empty() || *cubic_storage_slots > 0x7fffffff {
        return fail(format!("{format} needs a completed fitted graph."));
    }
    if nodes.iter().any(|node| !node.x.is_finite() || !node.y.is_finite()) {
        return fail(format!("{format} needs finite node coordinates."));
    }
    let mut curves = HashSet::new();
    for curve in cubics {
        if curve.index >= *cubic_storage_slots || !curves.insert(curve.index) || curve.points.iter().any(|v| !v.is_finite()) {
            return fail(format!("{format} needs unique initialized cubic IDs and eight finite coordinates per curve."));
        }
    }
    for shape in regions {
        if !(shape.parent == -1 || identifier(shape.parent as i64, regions.len())) || shape.contour.len() < 3
            || shape.contour.iter().any(|&node| node >= nodes.len()) || shape.edges.is_empty() {
            return fail(format!("{format} needs byte BGRA colors, existing parents, contours and fitted edges."));
        }
        let length = shape.contour.len();
        for edge in &shape.edges {
            let consistent = (0..=2).contains(&edge[1])
                && (edge[2] == -1 || identifier(edge[2], regions.len()))
                && identifier(edge[5], length) && identifier(edge[6], length)
                && edge[3] == shape.contour[edge[5] as usize] as i64 && edge[4] == shape.contour[edge[6] as usize] as i64
                && if edge[1] == 0 { edge[0] == edge[5] && edge[6] == (edge[5] + 1) % length as i64 }
                   else { edge[0] >= 0 && curves.contains(&(edge[0] as usize)) };
            if !consistent {
                return fail(format!(
                    "{format} needs consistent fitted edge samples, shared node IDs and initialized curve references."));
            }
        }
    }
    Ok(())
}

/// Validate common export inputs once; writers read the caller graph only.
pub fn read_vector_output(input: &VectorOutputInput, format: &str) -> Result<VectorOutputOptions, VectorOutputError> {
    let (output_width, output_height) = input.output_size.unwrap_or((input.width, input.height));
    if !dimension(input.width) || !dimension(input.height) || !dimension(output_width) || !dimension(output_height) {
        return fail(format!("{format} needs positive int32 source dimensions and both output dimensions or neither."));
    }
    let shape_mode = match input.layout { Layout::Stacked => 0, Layout::Cutouts => 1, Layout::GroupedCutouts => 2 };
    validate_fitted_geometry(input.geometry, format)?;
    let regions = input.geometry.regions.len();
    if input.hidden_regions.iter().any(|&region| !identifier(region, regions)) {
        return fail(format!("{format} hiddenRegions must list existing nonnegative integer region IDs."));
    }
    let hidden_size = if input.hidden_regions.is_empty() { 0 } else { regions };
    let mut hidden_words = vec![0u32; hidden_size.div_ceil(32)];
    for &region in input.hidden_regions { hidden_words[region as usize >> 5] |= 1 << (region & 31); }
    Ok(VectorOutputOptions { width: input.width as i32, height: input.height as i32, output_width: output_width as i32,
        output_height: output_height as i32, shape_mode, thin_stroke: input.thin_stroke, hidden_size, hidden_words })
}

fn bit_length(value: &BigInt) -> i64 { value.magnitude().bits() as i64 }

// Divide a dyadic register value by an integer, rounding its significand to
// 53 bits before the final double store. Register exponents remain unbounded.
fn divide_coordinate(numerator: BigInt, exponent: i64, denominator: BigInt) -> f64 {
    if numerator.is_zero() { return 0.0; }
    let negative = numerator.is_negative();
    let mut numerator = numerator.abs();
    let mut denominator = denominator;
    let mut highest = bit_length(&numerator) - bit_length(&denominator);
    let below_power = if highest >= 0 { numerator < (&denominator << highest as u64) }
        else { (&numerator << (-highest) as u64) < denominator };
    if below_power { highest -= 1; }
    let shift = highest - 52;
    if shift > 0 { denominator <<= shift as u64; } else { numerator <<= (-shift) as u64; }
    let mut quotient = &numerator / &denominator;
    let remainder = &numerator % &denominator;
    let twice: BigInt = &remainder * 2;
    if twice > denominator || (twice == denominator && quotient.bit(0)) { quotient += 1; }
    binary_double(&if negative { -quotient } else { quotient }, exponent + shift)
}

fn transform_output_coordinate(value: f64, source_size: i64, output_size: i64, flipped: bool, divisor: i64) -> f64 {
    // Curve arithmetic can produce nonfinite values even from finite controls.
    // IEEE operations preserve the native signs and quiet signaling NaNs here.
    if !value.is_finite() {
        return ((if flipped { source_size as f64 - value } else { value }) * output_size as f64) / divisor as f64;
    }
    let (magnitude, exponent) = binary_parts(value.abs());
    let mut significand = BigInt::from(magnitude);
    if value < 0.0 { significand = -significand; }
    let mut exponent = exponent as i64;
    if flipped {
        let common = exponent.min(0);
        significand = round_significand(&((BigInt::from(source_size) << (-common) as u64)
            - (significand << (exponent - common) as u64)), 53);
        exponent = common;
    }
    significand = round_significand(&(significand * BigInt::from(output_size)), 53);
    if divisor == 0 {
        return if significand.is_zero() { -f64::NAN } else if significand.is_negative() { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    if significand.is_zero() {
        let negative_zero = !flipped && value == 0.0 && value.is_sign_negative();
        return if negative_zero != (divisor < 0) { -0.0 } else { 0.0 };
    }
    if divisor < 0 { significand = -significand; }
    divide_coordinate(significand, exponent, BigInt::from(divisor.abs()))
}

/// DXF uses inches and signed int32 multiplication for its denominators.
pub fn transform_dxf_point(options: &VectorOutputOptions, x: f64, y: f64) -> [f64; 2] {
    let (width, height) = (options.width as i64, options.height as i64);
    [transform_output_coordinate(x, width, options.output_width as i64, false, options.width.wrapping_mul(72) as i64),
        transform_output_coordinate(y, height, options.output_height as i64, true, options.height.wrapping_mul(72) as i64)]
}

/// Original EMF coordinates truncate through signed 64-bit storage, then use
/// its low 32 bits. Invalid/overflow stores have a zero low word. Products too
/// large for that storage do not need to be rounded into binary64 first.
pub fn transform_emf_point(height: i32, x: f64, y: f64) -> [i32; 2] {
    let integer = |value: f64| if (-9223372036854775808.0..9223372036854775808.0).contains(&value) {
        value.trunc() as i64 as i32
    } else { 0 };
    [integer(x * 35.27777778), integer((height as f64 - y) * 35.27777778)]
}

/// GDI rounds logical inputs and matrix products to float32, converts to
/// signed 28.4 fixed point with ties away from zero, then rounds device pixels
/// with (fixed+8)>>4. Header conversion wraps bound*100 before MulDiv.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmfDeviceMapper { pixels: [i32; 2], millimeters: [i32; 2], scales: [f32; 2] }

impl EmfDeviceMapper {
    pub fn new(pixels: [i32; 2], millimeters: [i32; 2]) -> Self {
        let scales = [0, 1].map(|axis| (pixels[axis] as f64 / (millimeters[axis] as f64 * 100.0)) as f32);
        EmfDeviceMapper { pixels, millimeters, scales }
    }

    fn coordinate(&self, value: f64, axis: usize) -> Result<i64, VectorOutputError> {
        let scale = if axis == 0 { self.scales[0] as f64 } else { -(self.scales[1] as f64) };
        let product = ((value as f32 as f64) * scale) as f32 as f64;
        let magnitude = ((product * 16.0).abs() + 0.5).floor();
        let fixed = if product < 0.0 { -magnitude } else { magnitude };
        if !(-2147483648.0..=2147483647.0).contains(&fixed) {
            return fail("EMF device coordinates exceed signed 28.4 storage.");
        }
        Ok(((fixed + 8.0) / 16.0).floor() as i64)
    }

    pub fn point(&self, x: f64, y: f64) -> Result<[i64; 2], VectorOutputError> {
        Ok([self.coordinate(x, 0)?, self.coordinate(y, 1)?])
    }

    /// Device bounds to the header frame in 0.01 mm; -1 marks int32 overflow.
    pub fn frame(&self, bounds: &[i32]) -> Result<Vec<i64>, VectorOutputError> {
        bounds.iter().enumerate().map(|(index, &bound)| {
            let axis = index & 1;
            let product = bound.wrapping_mul(100) as i128 * self.millimeters[axis] as i128;
            let magnitude = product.abs();
            let divisor = self.pixels[axis] as i128;
            if divisor == 0 { return fail("Division by zero"); }
            let mut quotient = magnitude / divisor;
            if (magnitude % divisor) * 2 >= divisor { quotient += 1; }
            if product < 0 { quotient = -quotient; }
            Ok(if quotient < -0x80000000 || quotient > 0x7fffffff { -1 } else { quotient as i64 })
        }).collect()
    }
}

/// Match the raw CRT conversion and trailing-zero trimming. Check the raw
/// length first: the native stack buffer overflows before it trims the field.
pub fn format_dxf_coordinate(value: f64) -> Result<String, VectorOutputError> {
    let field = fixed_decimal(value, 16);
    if field.len() >= 256 { return fail("DXF coordinate exceeds the reference writer's 256-byte numeric field."); }
    let bytes = field.as_bytes();
    let mut end = bytes.len();
    let first_fraction = field.find('.').map_or(0, |index| index + 1);
    while end > first_fraction + 1 && bytes[end - 1] == b'0' { end -= 1; }
    Ok(field[..end].to_string())
}

/// JavaScript `Map` keys compare with SameValueZero: both zeroes and all NaNs
/// share one entry.
fn same_value_zero_key(value: f64) -> u64 {
    if value == 0.0 { 0 } else if value.is_nan() { f64::NAN.to_bits() } else { value.to_bits() }
}

/// Scaled path operands and RGB share the native stream's fixed/general
/// state. Fixed header fields and coordinates set that flag for the file.
#[derive(Debug, Clone)]
pub struct ScaledOutputFormatter {
    width: i64,
    height: i64,
    output_width: i64,
    output_height: i64,
    x_coordinates: HashMap<u64, String>,
    y_coordinates: HashMap<u64, String>,
    fixed_components: HashMap<u8, String>,
    general_components: HashMap<u8, String>,
    fixed_formatting: bool,
}

impl ScaledOutputFormatter {
    pub fn new(options: &VectorOutputOptions) -> Self {
        ScaledOutputFormatter { width: options.width as i64, height: options.height as i64,
            output_width: options.output_width as i64, output_height: options.output_height as i64,
            x_coordinates: HashMap::new(), y_coordinates: HashMap::new(), fixed_components: HashMap::new(),
            general_components: HashMap::new(), fixed_formatting: false }
    }

    fn coordinate(&mut self, value: f64, flipped: bool) -> String {
        let (source, output) = if flipped { (self.height, self.output_height) } else { (self.width, self.output_width) };
        let cache = if flipped { &mut self.y_coordinates } else { &mut self.x_coordinates };
        cache.entry(same_value_zero_key(value)).or_insert_with(|| {
            stream_fixed_decimal(transform_output_coordinate(value, source, output, flipped, source), 2)
        }).clone()
    }

    pub fn fixed(&mut self, value: f64, precision: usize) -> String {
        self.fixed_formatting = true;
        stream_fixed_decimal(value, precision)
    }

    pub fn point(&mut self, x: f64, y: f64) -> String {
        self.fixed_formatting = true;
        format!("{} {}", self.coordinate(x, false), self.coordinate(y, true))
    }

    pub fn component(&mut self, byte: u8) -> String {
        let fixed = self.fixed_formatting;
        let components = if fixed { &mut self.fixed_components } else { &mut self.general_components };
        components.entry(byte).or_insert_with(|| {
            let value = (byte as f64 * (1.0f64 / 255.0) as f32 as f64) as f32 as f64;
            if fixed { stream_fixed_decimal(value, 3) } else { general_rgb_decimal(value) }
        }).clone()
    }
}
