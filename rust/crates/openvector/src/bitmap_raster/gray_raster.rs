//! Rust translation of the FreeType gray rasterizer distributed with Qt 4.4.3
//! (`src/vendor/freetype-raster/qgrayraster.c`), as modified by OpenVector and
//! compiled for the JavaScript reference's 32-bit WebAssembly kernel.
//!
//! This software is based in part on the work of the FreeType Team.
//! Portions are copyright 1996-2004 The FreeType Project (www.freetype.org).
//! All rights reserved. Distributed under the FreeType Project License; see
//! `vendor/freetype-raster/FTL.TXT`.
//!
//! The reference was built for a 32-bit target: `long` and `TPos` are 32-bit,
//! wrapping integers, a cell occupies 16 bytes and a cell pointer 4 bytes.
//! Those sizes decide band splitting, pool-growth retries and the spans
//! already flushed before a retry, so they are reproduced exactly here.

/// Upstream error codes surfaced to the caller.
pub const ROTTEN_GLYPH: i32 = -6;
const INVALID_OUTLINE: i32 = -1;
const INVALID_ARGUMENT: i32 = -3;
const MEMORY_OVERFLOW: i32 = -4;

const PIXEL_BITS: i32 = 8;
const ONE_PIXEL: i32 = 1 << PIXEL_BITS;
const MAX_GRAY_SPANS: usize = 256;
const TAG_ON: u8 = 1;
const TAG_CUBIC: u8 = 2;
/// The allocator retains the observed x86 reference's control quota.
const CONTROL_BYTES: i32 = 3200;
const CELL_BYTES: i32 = 16;
const CELL_POINTER_BYTES: i32 = 4;
const NO_CELL: i32 = -1;

/// A FreeType outline in 26.6 fixed point: tag 1 is on-curve, tag 2 cubic.
#[derive(Debug, Clone, Default)]
pub struct Outline {
    pub points: Vec<[i32; 2]>,
    pub tags: Vec<u8>,
    pub contours: Vec<i32>,
}

#[derive(Clone, Copy, Default)]
struct Cell { x: i32, cover: i32, area: i32, next: i32 }

#[derive(Clone, Copy, Default)]
struct Span { x: i16, len: u16, y: i16, coverage: u8 }

#[inline] fn trunc(x: i32) -> i32 { x >> PIXEL_BITS }
#[inline] fn subpixels(x: i32) -> i32 { x.wrapping_shl(PIXEL_BITS as u32) }
#[inline] fn upscale(x: i32) -> i32 { x.wrapping_shl((PIXEL_BITS - 6) as u32) }
#[inline] fn downscale(x: i32) -> i32 { x >> (PIXEL_BITS - 6) }

struct Worker<'a> {
    ex: i32, ey: i32,
    min_ex: i32, max_ex: i32, min_ey: i32, max_ey: i32,
    count_ex: i32, count_ey: i32,
    area: i32, cover: i32, invalid: bool,
    cells: Vec<Cell>, max_cells: i32,
    x: i32, y: i32, last_ey: i32,
    bez_stack: [[i32; 2]; 32 * 3 + 1],
    lev_stack: [i32; 32],
    even_odd: bool,
    clip: [i32; 4],
    gray_spans: [Span; MAX_GRAY_SPANS],
    num_gray_spans: usize,
    band_size: i32,
    band_shoot: i32,
    conic_level: i32,
    cubic_level: i32,
    overflow: bool,
    buffer_size: i32,
    ycells: Vec<i32>,
    ycount: i32,
    pixels: &'a mut [u8],
    width: usize,
}

impl Worker<'_> {
    fn compute_cbox(&mut self, outline: &Outline) {
        let Some(first) = outline.points.first() else {
            self.min_ex = 0; self.max_ex = 0; self.min_ey = 0; self.max_ey = 0;
            return;
        };
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (first[0], first[0], first[1], first[1]);
        for &[x, y] in &outline.points[1..] {
            if x < min_x { min_x = x; }
            if x > max_x { max_x = x; }
            if y < min_y { min_y = y; }
            if y > max_y { max_y = y; }
        }
        self.min_ex = min_x >> 6;
        self.min_ey = min_y >> 6;
        self.max_ex = max_x.wrapping_add(63) >> 6;
        self.max_ey = max_y.wrapping_add(63) >> 6;
    }

    fn find_cell(&mut self) -> Option<usize> {
        let mut x = self.ex;
        if x > self.max_ex { x = self.max_ex; }
        let mut previous = NO_CELL;
        let mut current = self.ycells[self.ey as usize];
        while current != NO_CELL {
            let cell = self.cells[current as usize];
            if cell.x > x { break; }
            if cell.x == x { return Some(current as usize); }
            previous = current;
            current = cell.next;
        }
        if self.cells.len() as i32 >= self.max_cells {
            self.overflow = true;
            return None;
        }
        let index = self.cells.len() as i32;
        self.cells.push(Cell { x, area: 0, cover: 0, next: current });
        if previous == NO_CELL { self.ycells[self.ey as usize] = index; } else { self.cells[previous as usize].next = index; }
        Some(index as usize)
    }

    fn record_cell(&mut self) {
        if !self.invalid && (self.area | self.cover) != 0 {
            let Some(index) = self.find_cell() else { return };
            let cell = &mut self.cells[index];
            cell.area = cell.area.wrapping_add(self.area);
            cell.cover = cell.cover.wrapping_add(self.cover);
        }
    }

    fn set_cell(&mut self, mut ex: i32, mut ey: i32) {
        ey = ey.wrapping_sub(self.min_ey);
        if ex > self.max_ex { ex = self.max_ex; }
        ex = ex.wrapping_sub(self.min_ex);
        if ex < 0 { ex = -1; }
        if ex != self.ex || ey != self.ey {
            if !self.invalid { self.record_cell(); }
            self.area = 0;
            self.cover = 0;
        }
        self.ex = ex;
        self.ey = ey;
        self.invalid = (ey as u32) >= (self.count_ey as u32) || ex >= self.count_ex;
    }

    fn start_cell(&mut self, mut ex: i32, ey: i32) {
        if ex > self.max_ex { ex = self.max_ex; }
        if ex < self.min_ex { ex = self.min_ex.wrapping_sub(1); }
        self.area = 0;
        self.cover = 0;
        self.ex = ex.wrapping_sub(self.min_ex);
        self.ey = ey.wrapping_sub(self.min_ey);
        self.last_ey = subpixels(ey);
        self.invalid = false;
        self.set_cell(ex, ey);
    }

    fn render_scanline(&mut self, ey: i32, x1: i32, mut y1: i32, x2: i32, y2: i32) {
        let mut dx = x2.wrapping_sub(x1);
        let mut ex1 = trunc(x1);
        let ex2 = trunc(x2);
        let fx1 = x1.wrapping_sub(subpixels(ex1));
        let fx2 = x2.wrapping_sub(subpixels(ex2));
        if y1 == y2 {
            self.set_cell(ex2, ey);
            return;
        }
        if ex1 == ex2 {
            let delta = y2.wrapping_sub(y1);
            self.area = self.area.wrapping_add(fx1.wrapping_add(fx2).wrapping_mul(delta));
            self.cover = self.cover.wrapping_add(delta);
            return;
        }
        let mut p = (ONE_PIXEL - fx1).wrapping_mul(y2.wrapping_sub(y1));
        let mut first = ONE_PIXEL;
        let mut incr = 1;
        if dx < 0 {
            p = fx1.wrapping_mul(y2.wrapping_sub(y1));
            first = 0;
            incr = -1;
            dx = dx.wrapping_neg();
        }
        let mut delta = p.wrapping_div(dx);
        let mut modulo = p.wrapping_rem(dx);
        if modulo < 0 {
            delta -= 1;
            modulo = modulo.wrapping_add(dx);
        }
        self.area = self.area.wrapping_add(fx1.wrapping_add(first).wrapping_mul(delta));
        self.cover = self.cover.wrapping_add(delta);
        ex1 = ex1.wrapping_add(incr);
        self.set_cell(ex1, ey);
        y1 = y1.wrapping_add(delta);
        if ex1 != ex2 {
            p = ONE_PIXEL.wrapping_mul(y2.wrapping_sub(y1).wrapping_add(delta));
            let mut lift = p.wrapping_div(dx);
            let mut rem = p.wrapping_rem(dx);
            if rem < 0 {
                lift -= 1;
                rem = rem.wrapping_add(dx);
            }
            modulo = modulo.wrapping_sub(dx);
            while ex1 != ex2 {
                delta = lift;
                modulo = modulo.wrapping_add(rem);
                if modulo >= 0 {
                    modulo = modulo.wrapping_sub(dx);
                    delta = delta.wrapping_add(1);
                }
                self.area = self.area.wrapping_add(ONE_PIXEL.wrapping_mul(delta));
                self.cover = self.cover.wrapping_add(delta);
                y1 = y1.wrapping_add(delta);
                ex1 = ex1.wrapping_add(incr);
                self.set_cell(ex1, ey);
            }
        }
        delta = y2.wrapping_sub(y1);
        self.area = self.area.wrapping_add(fx2.wrapping_add(ONE_PIXEL).wrapping_sub(first).wrapping_mul(delta));
        self.cover = self.cover.wrapping_add(delta);
    }

    fn render_line(&mut self, to_x: i32, to_y: i32) {
        let mut ey1 = trunc(self.last_ey);
        let ey2 = trunc(to_y);
        let fy1 = self.y.wrapping_sub(self.last_ey);
        let fy2 = to_y.wrapping_sub(subpixels(ey2));
        let dx = to_x.wrapping_sub(self.x);
        let mut dy = to_y.wrapping_sub(self.y);
        'draw: {
            let (min, max) = if ey1 > ey2 { (ey2, ey1) } else { (ey1, ey2) };
            if min >= self.max_ey || max < self.min_ey { break 'draw; }
            if ey1 == ey2 {
                self.render_scanline(ey1, self.x, fy1, to_x, fy2);
                break 'draw;
            }
            if dx == 0 {
                let ex = trunc(self.x);
                let two_fx = self.x.wrapping_sub(subpixels(ex)).wrapping_shl(1);
                let (first, incr) = if dy < 0 { (0, -1) } else { (ONE_PIXEL, 1) };
                let mut delta = first.wrapping_sub(fy1);
                self.area = self.area.wrapping_add(two_fx.wrapping_mul(delta));
                self.cover = self.cover.wrapping_add(delta);
                ey1 = ey1.wrapping_add(incr);
                self.set_cell(ex, ey1);
                delta = first + first - ONE_PIXEL;
                let area = two_fx.wrapping_mul(delta);
                while ey1 != ey2 {
                    self.area = self.area.wrapping_add(area);
                    self.cover = self.cover.wrapping_add(delta);
                    ey1 = ey1.wrapping_add(incr);
                    self.set_cell(ex, ey1);
                }
                delta = fy2.wrapping_sub(ONE_PIXEL).wrapping_add(first);
                self.area = self.area.wrapping_add(two_fx.wrapping_mul(delta));
                self.cover = self.cover.wrapping_add(delta);
                break 'draw;
            }
            let mut p = (ONE_PIXEL.wrapping_sub(fy1)).wrapping_mul(dx);
            let mut first = ONE_PIXEL;
            let mut incr = 1;
            if dy < 0 {
                p = fy1.wrapping_mul(dx);
                first = 0;
                incr = -1;
                dy = dy.wrapping_neg();
            }
            let mut delta = p.wrapping_div(dy);
            let mut modulo = p.wrapping_rem(dy);
            if modulo < 0 {
                delta = delta.wrapping_sub(1);
                modulo = modulo.wrapping_add(dy);
            }
            let mut x = self.x.wrapping_add(delta);
            self.render_scanline(ey1, self.x, fy1, x, first);
            ey1 = ey1.wrapping_add(incr);
            self.set_cell(trunc(x), ey1);
            if ey1 != ey2 {
                p = ONE_PIXEL.wrapping_mul(dx);
                let mut lift = p.wrapping_div(dy);
                let mut rem = p.wrapping_rem(dy);
                if rem < 0 {
                    lift = lift.wrapping_sub(1);
                    rem = rem.wrapping_add(dy);
                }
                modulo = modulo.wrapping_sub(dy);
                while ey1 != ey2 {
                    delta = lift;
                    modulo = modulo.wrapping_add(rem);
                    if modulo >= 0 {
                        modulo = modulo.wrapping_sub(dy);
                        delta = delta.wrapping_add(1);
                    }
                    let x2 = x.wrapping_add(delta);
                    self.render_scanline(ey1, x, ONE_PIXEL - first, x2, first);
                    x = x2;
                    ey1 = ey1.wrapping_add(incr);
                    self.set_cell(trunc(x), ey1);
                }
            }
            self.render_scanline(ey1, x, ONE_PIXEL - first, to_x, fy2);
        }
        self.x = to_x;
        self.y = to_y;
        self.last_ey = subpixels(ey2);
    }

    fn render_cubic(&mut self, control1: [i32; 2], control2: [i32; 2], to: [i32; 2]) {
        let mut dx = downscale(self.x).wrapping_add(to[0]).wrapping_sub(control1[0].wrapping_shl(1)).wrapping_abs();
        let dy = downscale(self.y).wrapping_add(to[1]).wrapping_sub(control1[1].wrapping_shl(1)).wrapping_abs();
        if dx < dy { dx = dy; }
        let mut da = dx;
        dx = downscale(self.x).wrapping_add(to[0])
            .wrapping_sub(3i32.wrapping_mul(control1[0].wrapping_add(control2[0]))).wrapping_abs();
        // Upstream mixes control1.x into the y estimate; the reference keeps it.
        let dy = downscale(self.y).wrapping_add(to[1])
            .wrapping_sub(3i32.wrapping_mul(control1[0].wrapping_add(control2[1]))).wrapping_abs();
        if dx < dy { dx = dy; }
        let mut db = dx;
        let mut level = 1;
        da /= self.cubic_level;
        db /= self.conic_level;
        while da > 0 || db > 0 {
            da >>= 2;
            db >>= 3;
            level += 1;
        }
        if level <= 1 {
            let to_x = upscale(to[0]);
            let to_y = upscale(to[1]);
            let mid_x = self.x.wrapping_add(to_x)
                .wrapping_add(3i32.wrapping_mul(upscale(control1[0].wrapping_add(control2[0])))) / 8;
            let mid_y = self.y.wrapping_add(to_y)
                .wrapping_add(3i32.wrapping_mul(upscale(control1[1].wrapping_add(control2[1])))) / 8;
            self.render_line(mid_x, mid_y);
            self.render_line(to_x, to_y);
            return;
        }
        let mut arc = 0usize;
        self.bez_stack[0] = [upscale(to[0]), upscale(to[1])];
        self.bez_stack[1] = [upscale(control2[0]), upscale(control2[1])];
        self.bez_stack[2] = [upscale(control1[0]), upscale(control1[1])];
        self.bez_stack[3] = [self.x, self.y];
        let mut top: i32 = 0;
        self.lev_stack[0] = level;
        while top >= 0 {
            let level = self.lev_stack[top as usize];
            if level > 1 {
                let base = &self.bez_stack[arc..arc + 4];
                let (mut min, mut max) = (base[0][1], base[0][1]);
                for point in &base[1..] {
                    if point[1] < min { min = point[1]; }
                    if point[1] > max { max = point[1]; }
                }
                if !(trunc(min) >= self.max_ey || trunc(max) < 0) {
                    split_cubic(&mut self.bez_stack[arc..arc + 7]);
                    arc += 3;
                    top += 1;
                    self.lev_stack[top as usize] = level - 1;
                    self.lev_stack[top as usize - 1] = level - 1;
                    continue;
                }
            }
            let to_x = self.bez_stack[arc][0];
            let to_y = self.bez_stack[arc][1];
            let mid_x = self.x.wrapping_add(to_x)
                .wrapping_add(3i32.wrapping_mul(self.bez_stack[arc + 1][0].wrapping_add(self.bez_stack[arc + 2][0]))) / 8;
            let mid_y = self.y.wrapping_add(to_y)
                .wrapping_add(3i32.wrapping_mul(self.bez_stack[arc + 1][1].wrapping_add(self.bez_stack[arc + 2][1]))) / 8;
            self.render_line(mid_x, mid_y);
            self.render_line(to_x, to_y);
            top -= 1;
            arc = arc.wrapping_sub(3);
        }
    }

    fn move_to(&mut self, to: [i32; 2]) {
        self.record_cell();
        let x = upscale(to[0]);
        let y = upscale(to[1]);
        self.start_cell(trunc(x), trunc(y));
        self.x = x;
        self.y = y;
    }

    fn line_to(&mut self, to: [i32; 2]) { self.render_line(upscale(to[0]), upscale(to[1])); }

    fn flush_spans(&mut self) {
        for span in &self.gray_spans[..self.num_gray_spans] {
            let start = span.y as isize as usize * self.width + span.x as isize as usize;
            self.pixels[start..start + span.len as usize].fill(span.coverage);
        }
    }

    fn hline(&mut self, mut x: i32, mut y: i32, area: i32, acount: i32) {
        let mut coverage = area >> (PIXEL_BITS * 2 + 1 - 8);
        if coverage < 0 { coverage = coverage.wrapping_neg(); }
        if self.even_odd {
            coverage &= 511;
            if coverage > 256 { coverage = 512 - coverage; } else if coverage == 256 { coverage = 255; }
        } else if coverage >= 256 {
            coverage = 255;
        }
        y = y.wrapping_add(self.min_ey);
        x = x.wrapping_add(self.min_ex);
        if x >= 32768 { x = 32767; }
        if coverage == 0 { return; }
        if self.num_gray_spans > 0 {
            let span = &mut self.gray_spans[self.num_gray_spans - 1];
            if span.y as i32 == y && span.x as i32 + span.len as i32 == x
                && span.coverage as i32 == coverage {
                span.len = (span.len as i32).wrapping_add(acount) as u16;
                return;
            }
        }
        if self.num_gray_spans >= MAX_GRAY_SPANS {
            self.flush_spans();
            self.num_gray_spans = 0;
        }
        self.gray_spans[self.num_gray_spans] = Span { x: x as i16, len: acount as u16, y: y as i16, coverage: coverage as u8 };
        self.num_gray_spans += 1;
    }

    fn sweep(&mut self) {
        if self.cells.is_empty() { return; }
        for yindex in 0..self.ycount {
            let mut index = self.ycells[yindex as usize];
            let mut cover: i32 = 0;
            let mut x: i32 = 0;
            while index != NO_CELL {
                let cell = self.cells[index as usize];
                if cell.x > x && cover != 0 {
                    self.hline(x, yindex, cover.wrapping_mul(ONE_PIXEL * 2), cell.x - x);
                }
                cover = cover.wrapping_add(cell.cover);
                let area = cover.wrapping_mul(ONE_PIXEL * 2).wrapping_sub(cell.area);
                if area != 0 && cell.x >= 0 { self.hline(cell.x, yindex, area, 1); }
                x = cell.x.wrapping_add(1);
                index = cell.next;
            }
            if self.count_ex > x && cover != 0 {
                self.hline(x, yindex, cover.wrapping_mul(ONE_PIXEL * 2), self.count_ex - x);
            }
        }
    }

    fn decompose(&mut self, outline: &Outline) -> i32 {
        let tag = |index: usize| outline.tags[index] & 3;
        let mut first = 0usize;
        for &last in &outline.contours {
            let last = last as usize;
            let limit = last;
            let start = outline.points[first];
            let mut point = first;
            match tag(point) {
                TAG_CUBIC => return INVALID_OUTLINE,
                TAG_ON => {}
                _ => unreachable!("raster outlines contain only on-curve and cubic points"),
            }
            self.move_to(start);
            let mut closed = false;
            while point < limit {
                point += 1;
                match tag(point) {
                    TAG_ON => self.line_to(outline.points[point]),
                    TAG_CUBIC => {
                        if point + 1 > limit || tag(point + 1) != TAG_CUBIC { return INVALID_OUTLINE; }
                        point += 2;
                        let (control1, control2) = (outline.points[point - 2], outline.points[point - 1]);
                        if point <= limit {
                            self.render_cubic(control1, control2, outline.points[point]);
                            continue;
                        }
                        self.render_cubic(control1, control2, start);
                        closed = true;
                        break;
                    }
                    _ => unreachable!("raster outlines contain only on-curve and cubic points"),
                }
            }
            if !closed { self.line_to(start); }
            first = last + 1;
        }
        0
    }

    fn convert_glyph_inner(&mut self, outline: &Outline) -> i32 {
        self.overflow = false;
        let mut error = self.decompose(outline);
        self.record_cell();
        if self.overflow { error = MEMORY_OVERFLOW; }
        error
    }

    fn convert_glyph(&mut self, outline: &Outline) -> i32 {
        self.num_gray_spans = 0;
        self.compute_cbox(outline);
        let [x_min, y_min, x_max, y_max] = self.clip;
        if self.max_ex <= x_min || self.min_ex >= x_max || self.max_ey <= y_min || self.min_ey >= y_max { return 0; }
        if self.min_ex < x_min { self.min_ex = x_min; }
        if self.min_ey < y_min { self.min_ey = y_min; }
        if self.max_ex > x_max { self.max_ex = x_max; }
        if self.max_ey > y_max { self.max_ey = y_max; }
        self.count_ex = self.max_ex - self.min_ex;
        self.count_ey = self.max_ey - self.min_ey;
        self.conic_level = 32;
        self.cubic_level = 16;
        let mut level = 0;
        if self.count_ex > 24 || self.count_ey > 24 { level += 1; }
        if self.count_ex > 120 || self.count_ey > 120 { level += 1; }
        self.conic_level <<= level;
        self.cubic_level <<= level;
        let mut num_bands = (self.max_ey - self.min_ey) / self.band_size;
        if num_bands == 0 { num_bands = 1; }
        if num_bands >= 39 { num_bands = 39; }
        self.band_shoot = 0;
        let mut min = self.min_ey;
        let max_y = self.max_ey;
        let mut bands: Vec<(i32, i32)> = Vec::with_capacity(40);
        for n in 0..num_bands {
            let mut max = min + self.band_size;
            if n == num_bands - 1 || max > max_y { max = max_y; }
            bands.clear();
            bands.push((min, max));
            while let Some(&(bottom, top)) = bands.last() {
                let ycount = top - bottom;
                let mut cell_start = CELL_POINTER_BYTES * ycount;
                let cell_mod = cell_start % CELL_BYTES;
                if cell_mod > 0 { cell_start += CELL_BYTES - cell_mod; }
                let cell_end = self.buffer_size - self.buffer_size % CELL_BYTES;
                let render = cell_start < cell_end && (cell_end - cell_start) / CELL_BYTES >= 2;
                if render {
                    self.ycount = ycount;
                    self.max_cells = (cell_end - cell_start) / CELL_BYTES;
                    self.ycells.clear();
                    self.ycells.resize(ycount as usize, NO_CELL);
                    self.cells.clear();
                    self.invalid = true;
                    self.min_ey = bottom;
                    self.max_ey = top;
                    self.count_ey = top - bottom;
                    let error = self.convert_glyph_inner(outline);
                    if error == 0 {
                        self.sweep();
                        bands.pop();
                        continue;
                    } else if error != MEMORY_OVERFLOW {
                        return 1;
                    }
                }
                let middle = bottom + ((top - bottom) >> 1);
                if middle == bottom { return ROTTEN_GLYPH; }
                if bottom - top >= self.band_size { self.band_shoot += 1; }
                bands.pop();
                bands.push((middle, top));
                bands.push((bottom, middle));
            }
            min = max;
        }
        if self.num_gray_spans > 0 { self.flush_spans(); }
        if self.band_shoot > 8 && self.band_size > 16 { self.band_size /= 2; }
        0
    }
}

fn split_cubic(base: &mut [[i32; 2]]) {
    for axis in 0..2 {
        base[6][axis] = base[3][axis];
        let mut c = base[1][axis];
        let d = base[2][axis];
        let mut a = base[0][axis].wrapping_add(c) / 2;
        base[1][axis] = a;
        let mut b = base[3][axis].wrapping_add(d) / 2;
        base[5][axis] = b;
        c = c.wrapping_add(d) / 2;
        a = a.wrapping_add(c) / 2;
        base[2][axis] = a;
        b = b.wrapping_add(c) / 2;
        base[4][axis] = b;
        base[3][axis] = a.wrapping_add(b) / 2;
    }
}

/// `ov_coverage`: render one outline's anti-aliased coverage directly into
/// `pixels` (one byte per position) using a `pool_size`-byte render pool.
/// Returns 0, `ROTTEN_GLYPH` when the pool cannot hold a one-row band, or
/// another upstream status. Spans flushed before a failure stay written.
pub fn coverage(width: i32, height: i32, outline: &Outline, even_odd: bool, pixels: &mut [u8], pool_size: i32) -> i32 {
    // gray_raster_reset
    if pool_size < CONTROL_BYTES + 2048 { return INVALID_ARGUMENT; }
    let buffer_size = (pool_size - CONTROL_BYTES) & !(CELL_BYTES - 1);
    if buffer_size == 0 { return INVALID_ARGUMENT; }
    let band_size = buffer_size / (CELL_BYTES * 8);
    // gray_raster_render
    let n_points = outline.points.len() as i32;
    let n_contours = outline.contours.len() as i32;
    if n_points == 0 || n_contours <= 0 { return 0; }
    if n_points != outline.contours[n_contours as usize - 1].wrapping_add(1) { return INVALID_OUTLINE; }
    let mut worker = Worker {
        ex: 0, ey: 0, min_ex: 0, max_ex: 0, min_ey: 0, max_ey: 0, count_ex: 0, count_ey: 0,
        area: 0, cover: 0, invalid: true,
        cells: Vec::new(), max_cells: 0,
        x: 0, y: 0, last_ey: 0,
        bez_stack: [[0; 2]; 32 * 3 + 1], lev_stack: [0; 32],
        even_odd, clip: [0, 0, width, height],
        gray_spans: [Span::default(); MAX_GRAY_SPANS], num_gray_spans: 0,
        band_size, band_shoot: 0, conic_level: 0, cubic_level: 0,
        overflow: false, buffer_size,
        ycells: Vec::new(), ycount: 0,
        pixels, width: width as usize,
    };
    worker.convert_glyph(outline)
}
