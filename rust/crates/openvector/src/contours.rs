//! Port of `src/contours.mjs`: shared boundary recovery before smoothing.
use crate::pixel_input::{fail, read_palette_colors, InputError};
use serde_json::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node { pub x: f64, pub y: f64, pub kind: u8, pub flags: u8 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub pixel_count: i32,
    pub color_index: i32,
    pub bgra: [u8; 4],
    pub parent: i32,
    pub contour: Vec<usize>,
    pub boundary: Vec<[i32; 3]>,
    pub neighbors: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry { pub nodes: Vec<Node>, pub regions: Vec<Region> }

impl Geometry {
    /// Decode the canonical shared graph without lossy integer or byte casts.
    pub fn from_json(value: &Value) -> Option<Geometry> {
        Self::deserialize(value).ok()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("shared geometry is serializable")
    }
}

/// Completed segmentation consumed by contour extraction. Dimensions are
/// doubles so non-integer inputs can be rejected as in the reference.
#[derive(Debug, Clone)]
pub struct ContourInput<'a> {
    pub width: f64,
    pub height: f64,
    pub indices: &'a [i64],
    pub counts: &'a [i64],
    pub color_indices: &'a [i64],
    pub palette: &'a [f64],
    pub anti_alias: Option<bool>,
}

const STAIRCASE_PATTERNS: [(u32, u32, u32); 6] = [(5, 0, 33), (9, 170, 585), (9, 146, 585),
    (13, 4369, 8481), (10, 132, 1057), (10, 330, 1057)];
const CORNERS: [(usize, usize); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];
const QUADRANTS: [(i64, i64); 4] = [(-1, -1), (-1, 0), (0, 0), (0, -1)];
const DIRECTIONS: [(i64, i64); 4] = [(-1, 0), (0, 1), (1, 0), (0, -1)];

struct Prepared { width: usize, height: usize, indices: Vec<i32>, counts: Vec<i32>, color_indices: Vec<i32>, palette: Vec<f32>, anti_alias: bool }

fn read_contour_input(input: &ContourInput) -> Result<Prepared, InputError> {
    let (w, h) = (input.width, input.height);
    const MAX: f64 = 2147483647.0;
    if !w.is_finite() || !h.is_finite() || w.fract() != 0.0 || h.fract() != 0.0 || w < 2.0 || h < 2.0 || w > MAX || h > MAX || w * h > MAX {
        return fail("Contour extraction needs int32 dimensions of at least 2 by 2.");
    }
    let Some(anti_alias) = input.anti_alias else { return fail("Contour extraction requires antiAlias: true or false.") };
    let (width, height) = (w as usize, h as usize);
    let (indices, counts, colors) = (input.indices, input.counts, input.color_indices);
    if indices.len() != width * height || counts.is_empty() || counts.len() > indices.len() || colors.len() != counts.len() {
        return fail("Contour extraction needs pixel labels, region counts and one color identifier per region.");
    }
    let palette = read_palette_colors(input.palette)?;
    let mut observed = vec![0i32; counts.len()];
    let mut next_region = 0i64;
    for &region in indices {
        if region < 0 || region >= counts.len() as i64 { return fail("Contour labels must identify existing regions."); }
        if observed[region as usize] == 0 {
            let expected = next_region;
            next_region += 1;
            if region != expected { return fail("Contour regions must follow first row-major encounter order."); }
        }
        observed[region as usize] += 1;
    }
    for region in 0..counts.len() {
        if counts[region] != observed[region] as i64 || counts[region] == 0 { return fail("Contour counts must match nonempty pixel regions."); }
        if colors[region] < 0 || colors[region] >= (palette.len() / 4) as i64 { return fail("Contour colors must identify existing palette entries."); }
    }
    let mut visited = vec![false; indices.len()];
    let mut seen = vec![false; counts.len()];
    let mut queue = vec![0usize; indices.len()];
    for start in 0..indices.len() {
        if visited[start] { continue; }
        let region = indices[start];
        if seen[region as usize] { return fail("Contour regions must be four-connected."); }
        seen[region as usize] = true;
        let mut tail = 1;
        queue[0] = start;
        visited[start] = true;
        let mut head = 0;
        while head < tail {
            let pixel = queue[head];
            head += 1;
            let (x, y) = ((pixel % width) as i64, (pixel / width) as i64);
            for (dx, dy) in DIRECTIONS {
                let (c, r) = (x + dx, y + dy);
                if c < 0 || c >= width as i64 || r < 0 || r >= height as i64 { continue; }
                let n = r as usize * width + c as usize;
                if !visited[n] && indices[n] == region { visited[n] = true; queue[tail] = n; tail += 1; }
            }
        }
    }
    Ok(Prepared { width, height, indices: indices.iter().map(|&v| v as i32).collect(), counts: counts.iter().map(|&v| v as i32).collect(),
        color_indices: colors.iter().map(|&v| v as i32).collect(), palette, anti_alias })
}

struct Labels<'a> { width: usize, height: usize, indices: &'a [i32] }
impl Labels<'_> {
    fn at(&self, x: i64, y: i64) -> i32 {
        if x < 0 || x >= self.width as i64 || y < 0 || y >= self.height as i64 { -1 } else { self.indices[y as usize * self.width + x as usize] }
    }
}

fn build_grid_nodes(labels: &Labels) -> (Vec<i32>, Vec<Node>) {
    let (width, height) = (labels.width, labels.height);
    let mut grid = vec![-1i32; (width + 1) * (height + 1)];
    let mut nodes = Vec::new();
    let mut add = |x: usize, y: usize, grid: &mut Vec<i32>, nodes: &mut Vec<Node>| {
        grid[y * (width + 1) + x] = nodes.len() as i32;
        let (xi, yi) = (x as i64, y as i64);
        let (ul, ur, ll, lr) = (labels.at(xi - 1, yi - 1), labels.at(xi, yi - 1), labels.at(xi - 1, yi), labels.at(xi, yi));
        let pairs = (ul == ur) as i32 + (ul == ll) as i32 + (ul == lr) as i32 + (ur == ll) as i32 + (ur == lr) as i32 + (ll == lr) as i32;
        let edge_x = x == 0 || x == width;
        let edge_y = y == 0 || y == height;
        nodes.push(Node { x: x as f64, y: y as f64, kind: if (edge_x && edge_y) || pairs < 2 { 3 } else { 0 },
            flags: if edge_x || edge_y { 128 } else { 0 } });
    };
    for y in 0..=height { add(0, y, &mut grid, &mut nodes); }
    for y in 0..=height { add(width, y, &mut grid, &mut nodes); }
    for x in 1..width { add(x, 0, &mut grid, &mut nodes); }
    for x in 1..width { add(x, height, &mut grid, &mut nodes); }
    for y in 1..height {
        for x in 1..width {
            let (xi, yi) = (x as i64, y as i64);
            let label = labels.at(xi - 1, yi - 1);
            if label != labels.at(xi, yi - 1) || label != labels.at(xi - 1, yi) || label != labels.at(xi, yi) { add(x, y, &mut grid, &mut nodes); }
        }
    }
    (grid, nodes)
}

struct Candidate { next_x: i64, next_y: i64, inside_x: i64, inside_y: i64, outside: i32 }

fn trace_region(region: i32, start_x: i64, start_y: i64, labels: &Labels, grid: &[i32], visited: &mut [i32], out: &mut Region) {
    let (width, height) = (labels.width as i64, labels.height as i64);
    let mut contour = Vec::new();
    let mut neighbors: Vec<i32> = Vec::new();
    let (mut x, mut y, mut prev_x, mut prev_y) = (start_x, start_y, -1i64, -1i64);
    loop {
        let current = (y * (width + 1) + x) as usize;
        contour.push(grid[current] as usize);
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut closing_outside = -1;
        for direction in 0..4 {
            let (dx, dy) = DIRECTIONS[direction];
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || nx > width || ny < 0 || ny > height { continue; }
            let next = (ny * (width + 1) + nx) as usize;
            let (ix, iy) = QUADRANTS[direction];
            let (ox, oy) = QUADRANTS[(direction + 1) % 4];
            let outside = labels.at(x + ox, y + oy);
            if grid[next] >= 0 && labels.at(x + ix, y + iy) == region && outside != region {
                closing_outside = outside;
                if visited[next] != region { candidates.push(Candidate { next_x: nx, next_y: ny, inside_x: x + ix, inside_y: y + iy, outside }); }
            }
        }
        if candidates.is_empty() {
            visited[current] = region;
            neighbors.push(closing_outside);
            break;
        }
        let chosen = if candidates.len() < 2 {
            visited[current] = region;
            let c = &candidates[0];
            prev_x = c.inside_x; prev_y = c.inside_y;
            c
        } else if prev_x == -1 || prev_y == -1 {
            let c = &candidates[0];
            prev_x = c.inside_x; prev_y = c.inside_y;
            c
        } else {
            candidates.iter().find(|c| c.inside_x == prev_x && c.inside_y == prev_y).expect("contour continuation")
        };
        neighbors.push(chosen.outside);
        x = chosen.next_x;
        y = chosen.next_y;
        if visited[(y * (width + 1) + x) as usize] == region { break; }
    }
    out.boundary = neighbors.iter().map(|&n| [0, n, 1]).collect();
    let mut unique: Vec<i32> = neighbors.iter().copied().filter(|&n| n >= 0).collect();
    unique.sort_unstable();
    unique.dedup();
    out.neighbors = unique;
    out.contour = contour;
}

fn reduce_staircases(nodes: &mut [Node], regions: &[Region]) {
    for region in regions {
        let contour = &region.contour;
        let length = contour.len();
        if length <= 19 { continue; }
        let (mut run, mut x_bits, mut y_bits, mut x_dir, mut y_dir) = (0u32, 0u32, 0u32, 0i64, 0i64);
        for step in 1..length + 31 {
            let (pi, ci) = (contour[(step - 1) % length], contour[step % length]);
            if (nodes[pi].flags & 64) != 0 || (nodes[ci].flags & 128) != 0 || nodes[pi].kind == 3 || nodes[ci].kind == 3 {
                run = 0; x_bits = 0; y_bits = 0; x_dir = 0; y_dir = 0;
                continue;
            }
            if step > 30 { nodes[pi].flags |= 128; }
            let dx = (nodes[pi].x - nodes[ci].x + 0.5).floor() as i64;
            let dy = (nodes[pi].y - nodes[ci].y + 0.5).floor() as i64;
            if (x_dir != 0 && dx != 0 && dx != x_dir) || (y_dir != 0 && dy != 0 && dy != y_dir) { run = 0; x_bits = 0; y_bits = 0; }
            if dx != 0 { x_dir = dx; }
            if dy != 0 { y_dir = dy; }
            x_bits = (x_bits << 1) | (dx != 0) as u32;
            y_bits = (y_bits << 1) | (dy != 0) as u32;
            run = (run + 1).min(31);
            if run < 5 { continue; }
            for (len, mask, replacement) in STAIRCASE_PATTERNS {
                let bits = (1u32 << len) - 1;
                if len > run || ((x_bits & bits) != mask && (y_bits & bits) != mask) { continue; }
                let mut retained = replacement;
                for sample in 0..=len as usize {
                    if retained & 1 == 0 { nodes[contour[(step + length - sample) % length]].flags |= 192; }
                    retained >>= 1;
                }
                run = 0; x_bits = 0; y_bits = 0; x_dir = 0; y_dir = 0;
                break;
            }
        }
    }
}

fn compact_contours(nodes: Vec<Node>, regions: &mut [Region]) -> Vec<Node> {
    let mut remap = vec![-1i64; nodes.len()];
    let mut kept = Vec::new();
    for (index, node) in nodes.into_iter().enumerate() {
        if node.flags & 64 == 0 { remap[index] = kept.len() as i64; kept.push(node); }
    }
    for region in regions.iter_mut() {
        let mut contour = Vec::new();
        let mut boundary = Vec::new();
        let mut last = 0usize;
        for (sample, &node) in region.contour.iter().enumerate() {
            if remap[node] < 0 { continue; }
            contour.push(remap[node] as usize);
            boundary.push([region.boundary[sample][0], region.boundary[sample][1], (sample - last) as i32]);
            last = sample;
        }
        boundary[0][2] = (region.contour.len() - last) as i32;
        region.contour = contour;
        region.boundary = boundary;
    }
    kept
}

/// Recover shared boundaries before smoothing or curve fitting.
pub fn extract_contours(input: &ContourInput) -> Result<Geometry, InputError> {
    let p = read_contour_input(input)?;
    let labels = Labels { width: p.width, height: p.height, indices: &p.indices };
    let (grid, mut nodes) = build_grid_nodes(&labels);
    let mut visited = vec![-1i32; grid.len()];
    let mut regions: Vec<Region> = p.counts.iter().enumerate().map(|(r, &pixel_count)| {
        let ci = p.color_indices[r] as usize;
        let mut bgra = [0u8; 4];
        for c in 0..4 { bgra[c] = ((p.palette[ci * 4 + c] as f64 * 255.0 + 0.5).trunc() as i64 & 255) as u8; }
        Region { pixel_count, color_index: p.color_indices[r], bgra, parent: -1, contour: vec![], boundary: vec![], neighbors: vec![] }
    }).collect();
    for pixel in 0..p.indices.len() {
        let region = p.indices[pixel];
        if !regions[region as usize].contour.is_empty() { continue; }
        let (x, y) = (pixel % p.width, pixel / p.width);
        let (dx, dy) = *CORNERS.iter().find(|(ox, oy)| grid[(y + oy) * (p.width + 1) + x + ox] >= 0).expect("region corner");
        let mut out = regions[region as usize].clone();
        trace_region(region, (x + dx) as i64, (y + dy) as i64, &labels, &grid, &mut visited, &mut out);
        regions[region as usize] = out;
    }
    for node in nodes.iter_mut() {
        if node.flags & 128 != 0 && node.kind != 3 { node.flags |= 64; }
    }
    if !p.anti_alias { reduce_staircases(&mut nodes, &regions); }
    let compacted = compact_contours(nodes, &mut regions);
    let parents: Vec<i32> = (0..regions.len()).map(|r| regions[r].neighbors.iter().copied()
        .find(|&n| !regions[n as usize].neighbors.contains(&(r as i32))).unwrap_or(-1)).collect();
    for (region, parent) in regions.iter_mut().zip(parents) { region.parent = parent; }
    Ok(Geometry { nodes: compacted, regions })
}
