//! Port of `src/region-data.mjs`: raster/region input validation, connected
//! region rebuilding, byte palette deduplication and diagonal topology repair.
use std::collections::HashMap;

const FLOAT_BYTE_SCALE: f32 = 1.0 / 255.0;
const INT32_MAX: i64 = 2147483647;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionError(pub String);
impl std::fmt::Display for RegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for RegionError {}

/// Validated raster dimensions over borrowed BGRA bytes.
#[derive(Debug, Clone, Copy)]
pub struct Raster<'a> { pub width: usize, pub height: usize, pub bgra: &'a [u8] }

pub fn read_raster_input(width: i64, height: i64, bgra: &[u8]) -> Result<Raster<'_>, RegionError> {
    if width < 1 || height < 1 || width > INT32_MAX || height > INT32_MAX || width * height > INT32_MAX {
        return Err(RegionError("Region dimensions must be positive int32 values with an int32 pixel count.".into()));
    }
    if bgra.len() as i64 != width * height * 4 {
        return Err(RegionError("Region pixels must contain exactly width * height BGRA byte pixels.".into()));
    }
    Ok(Raster { width: width as usize, height: height as usize, bgra })
}

/// Region labels are validated as nonnegative int32 values, one per pixel.
pub fn read_region_input<'a>(width: i64, height: i64, bgra: &'a [u8], regions: &[i64]) -> Result<(Raster<'a>, Vec<i32>), RegionError> {
    let raster = read_raster_input(width, height, bgra)?;
    if regions.len() != raster.width * raster.height {
        return Err(RegionError("Optimizer needs one region label per pixel.".into()));
    }
    if regions.iter().any(|r| *r < 0 || *r > INT32_MAX) {
        return Err(RegionError("Optimizer labels must be nonnegative int32 values.".into()));
    }
    Ok((raster, regions.iter().map(|r| *r as i32).collect()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionStatistic { pub count: u32, pub sums: [f32; 4], pub color_index: i32 }

/// Rebuild validated labels and statistics without copying caller pixel storage.
/// `previous_statistics` maps an old label to its retained color index.
pub fn rebuild_connected_regions(width: usize, bgra: &[u8], previous: &[i32], previous_statistics: Option<&[RegionStatistic]>)
    -> (Vec<i32>, Vec<RegionStatistic>) {
    let total = previous.len();
    let mut indices = vec![-1i32; total];
    let mut queue = vec![0usize; total];
    let mut region_count = 0i32;
    let mut color_indices = Vec::new();
    for seed in 0..total {
        if indices[seed] >= 0 { continue; }
        let (mut head, mut tail) = (0usize, 1usize);
        queue[0] = seed;
        indices[seed] = region_count;
        color_indices.push(previous_statistics.map_or(-1, |s| s[previous[seed] as usize].color_index));
        let label = previous[seed];
        while head < tail {
            let pixel = queue[head];
            head += 1;
            let column = pixel % width;
            let mut visit = |p: usize| {
                if indices[p] < 0 && previous[p] == label { indices[p] = region_count; queue[tail] = p; tail += 1; }
            };
            if column > 0 { visit(pixel - 1); }
            if column + 1 < width { visit(pixel + 1); }
            if pixel >= width { visit(pixel - width); }
            if pixel + width < total { visit(pixel + width); }
        }
        region_count += 1;
    }
    let mut statistics: Vec<RegionStatistic> = color_indices.into_iter()
        .map(|color_index| RegionStatistic { count: 0, sums: [0.0; 4], color_index }).collect();
    // Statistics must follow row-major order, independent of flood traversal.
    // Summing during the traversal would change float32 cancellation and ties.
    for (pixel, index) in indices.iter().enumerate() {
        let statistic = &mut statistics[*index as usize];
        statistic.count += 1;
        for channel in 0..4 {
            statistic.sums[channel] += bgra[pixel * 4 + channel] as f32 * FLOAT_BYTE_SCALE;
        }
    }
    (indices, statistics)
}

/// Deduplicate engine BGRA bytes in encounter order and retain their identifiers.
pub fn palette_from_byte_colors(colors: &[[u8; 4]]) -> (Vec<f32>, Vec<i32>) {
    let mut palette = Vec::new();
    let mut indices = Vec::with_capacity(colors.len());
    let mut lookup: HashMap<u32, i32> = HashMap::new();
    for bytes in colors {
        let key = u32::from_le_bytes(*bytes);
        let color = *lookup.entry(key).or_insert_with(|| {
            let color = (palette.len() / 4) as i32;
            for channel in bytes { palette.push(*channel as f32 * FLOAT_BYTE_SCALE); }
            color
        });
        indices.push(color);
    }
    (palette, indices)
}

/// Mutate projected labels using the native diagonal-contact topology rules.
pub fn repair_diagonal_map(width: usize, height: usize, bgra: &[u8], indices: &mut [i32]) {
    let (w, h) = (width as i64, height as i64);
    let label_at = |indices: &[i32], x: i64, y: i64| -> i32 {
        if x < 0 || x >= w || y < 0 || y >= h { -1 } else { indices[(y * w + x) as usize] }
    };
    let protected_corner = |indices: &[i32], x: i64, y: i64, dx: i64, dy: i64| -> bool {
        let region = label_at(indices, x, y);
        region == label_at(indices, x, y + dy) && region == label_at(indices, x + dx, y) && region != label_at(indices, x + dx, y + dy)
    };
    let diagonal_line_cost = |x: i64, y: i64, dx: i64, dy: i64| -> f32 {
        if x - dx < 0 || x - dx >= w || y - dy < 0 || y - dy >= h || x + dx < 0 || x + dx >= w || y + dy < 0 || y + dy >= h { return 0.0; }
        let first = (((y - dy) * w + x - dx) * 4) as usize;
        let middle = ((y * w + x) * 4) as usize;
        let last = (((y + dy) * w + x + dx) * 4) as usize;
        let mut cost = 0f32;
        for channel in 0..4 {
            let combined = bgra[first + channel] as i32 - 2 * bgra[middle + channel] as i32 + bgra[last + channel] as i32;
            // JS multiplies the integer by the float32 scale in double, then rounds.
            let difference = (combined as f64 * FLOAT_BYTE_SCALE as f64) as f32;
            cost += difference * difference;
        }
        cost
    };
    let pixel_distance = |first: usize, second: usize| -> f64 {
        let mut distance = 0f64;
        for channel in 0..4 {
            let difference = ((bgra[first * 4 + channel] as i32 - bgra[second * 4 + channel] as i32) as f64 * FLOAT_BYTE_SCALE as f64) as f32;
            // The original uses double-precision x87 products and accumulation here.
            distance += difference as f64 * difference as f64;
        }
        distance
    };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let top_left = label_at(indices, x, y);
            let top_right = label_at(indices, x + 1, y);
            let bottom_left = label_at(indices, x, y + 1);
            let bottom_right = label_at(indices, x + 1, y + 1);
            if top_left == bottom_left || top_left == top_right || bottom_right == bottom_left || bottom_right == top_right { continue; }
            let mut main_diagonal = top_left == bottom_right;
            let other_diagonal = top_right == bottom_left;
            if !main_diagonal && !other_diagonal { continue; }
            if main_diagonal && other_diagonal {
                let (mut main_cost, mut other_cost) = (0f32, 0f32);
                for row in y..=y + 1 {
                    for column in x..=x + 1 {
                        main_cost += diagonal_line_cost(column, row, 1, 1);
                        other_cost += diagonal_line_cost(column, row, 1, -1);
                    }
                }
                if other_cost < main_cost { main_diagonal = false; } else if other_cost <= main_cost { continue; }
            }
            let first_x = if main_diagonal { x + 1 } else { x };
            let second_x = if main_diagonal { x } else { x + 1 };
            let dx = if main_diagonal { 1 } else { -1 };
            let first_protected = protected_corner(indices, first_x, y, dx, -1);
            let second_protected = protected_corner(indices, second_x, y + 1, -dx, 1);
            if first_protected && second_protected { continue; }
            let first = (y * w + first_x) as usize;
            let second = ((y + 1) * w + second_x) as usize;
            let mut replacement = second;
            if !first_protected && second_protected { replacement = first; }
            else if !first_protected {
                let first_target = ((if main_diagonal { y + 1 } else { y }) * w + x + 1) as usize;
                let second_target = ((if main_diagonal { y } else { y + 1 }) * w + x) as usize;
                // The first candidate's sum is stored as float32. The second stays
                // double until the comparison, so apparent float32 ties can differ.
                let first_cost = ((pixel_distance(first_target, first) as f32) as f64 + pixel_distance(second_target, first)) as f32;
                let second_cost = (pixel_distance(first_target, second) as f32) as f64 + pixel_distance(second_target, second);
                if second_cost > first_cost as f64 { replacement = first; }
            }
            indices[replacement] = if main_diagonal { top_left } else { bottom_left };
        }
    }
}
