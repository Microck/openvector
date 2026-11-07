//! Port of `src/image-measurement-kernel.mjs`: pixel clipping of contour
//! polygons, the image energy and its raw gradient.
use crate::arctangent::contour_angle_step;
use crate::pixel_input::InputError;
use std::collections::BTreeSet;
use std::f64::consts::PI;

const BYTE_SCALE: f32 = 1.0 / 255.0;
const NONE: usize = usize::MAX;

/// Numeric fields from original model+0x78 through model+0xf7. Identifiers
/// and double areas overlap the count/background words when enough fragments
/// arrive; one byte record preserves those writes.
#[derive(Debug, Clone)]
pub struct PixelFragmentScratch { bytes: [u8; 0x80] }

impl Default for PixelFragmentScratch { fn default() -> Self { Self::new() } }

impl PixelFragmentScratch {
    pub fn new() -> Self { PixelFragmentScratch { bytes: [0; 0x80] } }
    fn i32_at(&self, offset: usize) -> i32 { i32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap()) }
    fn set_i32(&mut self, offset: usize, value: i32) { self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes()); }
    pub fn count(&self) -> i32 { self.i32_at(0x78) }
    pub fn set_count(&mut self, value: i32) { self.set_i32(0x78, value) }
    pub fn background(&self) -> i32 { self.i32_at(0x7c) }
    pub fn set_background(&mut self, value: i32) { self.set_i32(0x7c, value) }
    fn offset(index: i32, base: usize, size: usize) -> Result<usize, InputError> {
        if index < 0 || index as usize >= (0x80 - base) / size {
            return Err(InputError("Image fragment scratch access exceeds the original 128-byte record.".into()));
        }
        Ok(base + index as usize * size)
    }
    pub fn identifier(&self, index: i32) -> Result<i32, InputError> {
        Ok(self.i32_at(Self::offset(index, 0, 4)?))
    }
    pub fn write_identifier(&mut self, index: i32, value: i32) -> Result<(), InputError> {
        self.set_i32(Self::offset(index, 0, 4)?, value);
        Ok(())
    }
    pub fn area(&self, index: i32) -> Result<f64, InputError> {
        let o = Self::offset(index, 0x28, 8)?;
        Ok(f64::from_le_bytes(self.bytes[o..o + 8].try_into().unwrap()))
    }
    pub fn write_area(&mut self, index: i32, value: f64) -> Result<(), InputError> {
        let o = Self::offset(index, 0x28, 8)?;
        self.bytes[o..o + 8].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    pub fn find(&self, identifier: i32) -> Result<i32, InputError> {
        for index in 0..self.count() {
            if self.identifier(index)? == identifier { return Ok(index); }
        }
        Ok(-1)
    }
}

#[derive(Debug, Clone)]
pub struct KernelRegion { pub pixel_count: i32, pub contour: Vec<usize>, pub parent: i32, pub color: [f32; 4] }

#[derive(Debug, Clone, Copy)]
struct Record { flags: i32, sample: i32, region: usize, x: f64, y: f64, perimeter: f64, turn: f64, next: usize }

#[derive(Debug, Clone, Copy)]
struct List { head: usize, tail: usize }

/// One evaluated state: energy, gradient, cached colors, flags and records
/// `[flags, sample, region, x, y, perimeter, turn]` per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement { pub energy: f64, pub gradient: Vec<f64>, pub colors: Vec<f32>, pub flags: Vec<u8>, pub pixels: Vec<Vec<[f64; 7]>> }

/// Persistent prepared measurement state (`registerImageKernel`).
#[derive(Debug, Clone)]
pub struct ImageKernel {
    pub node_count: usize,
    pub width: usize,
    pub height: usize,
    pub regions: Vec<KernelRegion>,
    bgra: Vec<u8>,
    colors: Vec<f32>,
    flags: Vec<u8>,
    invalid: BTreeSet<usize>,
    scratch: PixelFragmentScratch,
    records: Vec<Record>,
    lists: Vec<List>,
    backgrounds: Vec<i32>,
    touched_pixels: Vec<usize>,
    energy: f64,
}

fn fr(v: f64) -> f64 { v as f32 as f64 }

impl ImageKernel {
    pub fn new(width: usize, height: usize, bgra: Vec<u8>, node_count: usize, regions: Vec<KernelRegion>, colors: Vec<f32>) -> Self {
        ImageKernel { node_count, width, height, regions, bgra, colors, flags: vec![0; width * height], invalid: BTreeSet::new(),
            scratch: PixelFragmentScratch::new(), records: Vec::new(), lists: Vec::new(), backgrounds: Vec::new(), touched_pixels: Vec::new(), energy: 0.0 }
    }

    #[allow(clippy::too_many_arguments)]
    fn append(&mut self, column: f64, row: f64, region: usize, sample: usize, flags: i32, x: f64, y: f64, turn: f64) {
        if column < 0.0 || row < 0.0 || column >= self.width as f64 || row >= self.height as f64 { return; }
        let mut perimeter = -1.0;
        if flags & 1 != 0 {
            perimeter = if flags & 0x100 == 0 {
                if y == row { fr(x - column) } else { fr(3.0 - (x - column)) }
            } else if x == column { fr(4.0 - (y - row)) } else { fr((y - row) + 1.0) };
        }
        let id = self.records.len();
        self.records.push(Record { flags, sample: sample as i32, region, x: fr(x), y: fr(y), perimeter, turn: fr(turn), next: NONE });
        let pixel = row as usize * self.width + column as usize;
        let list = &mut self.lists[pixel];
        // Track each pixel once per evaluation, even if clipping later removes its head.
        if list.tail == NONE { self.touched_pixels.push(pixel); }
        if list.tail != NONE { self.records[list.tail].next = id; } else { list.head = id; }
        list.tail = id;
    }

    fn trace_edge(&mut self, positions: &[f64], region: usize, sample: usize, turn: f64) {
        let contour = &self.regions[region].contour;
        let first = contour[sample] * 2;
        let last = contour[(sample + 1) % contour.len()] * 2;
        let (x0, y0, x1, y1) = (positions[first], positions[first + 1], positions[last], positions[last + 1]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let (mut grid_x, mut grid_y) = (x0.floor(), y0.floor());
        let (mut column, mut row) = (grid_x, grid_y);
        let mut flags;
        if x0 == grid_x || y0 == grid_y {
            if x0 == grid_x {
                column += if dx < 0.0 { -1.0 } else { 0.0 };
                if y0 == grid_y { row += if dy < 0.0 { -1.0 } else { 0.0 }; flags = 0x187; } else { flags = 0x105; }
            } else { row += if dy < 0.0 { -1.0 } else { 0.0 }; flags = 0x85; }
        } else { flags = 4; }
        self.append(column, row, region, sample, flags, x0, y0, turn);
        let (mut cx, mut cy) = (x0, y0);
        loop {
            let (mut fx, mut fy) = (2.0, 2.0);
            if dx > 0.0 { grid_x = cx.floor() + 1.0; fx = (grid_x - x0) / dx; }
            else if dx < 0.0 { grid_x = cx.ceil() - 1.0; fx = (grid_x - x0) / dx; }
            if dy > 0.0 { grid_y = cy.floor() + 1.0; fy = (grid_y - y0) / dy; }
            else if dy < 0.0 { grid_y = cy.ceil() - 1.0; fy = (grid_y - y0) / dy; }
            if fx >= 1.0 && fy >= 1.0 { break; }
            let (mut ox, oy);
            if fy <= fx { ox = 0.0; oy = -1.0; flags = 0x80; grid_x = fy * dx + x0; }
            else { ox = -1.0; oy = 0.0; flags = 0x100; grid_y = fx * dy + y0; }
            column = grid_x.trunc();
            row = grid_y.trunc();
            if grid_x == column && grid_y == row {
                self.append(column, row, region, sample, 0x183, grid_x, grid_y, turn);
                ox = column - 1.0;
                self.append(ox, row, region, sample, 0x183, grid_x, grid_y, turn);
                row -= 1.0;
                self.append(column, row, region, sample, 0x183, grid_x, grid_y, turn);
                flags = 0x183;
            } else {
                flags |= 1;
                self.append(column, row, region, sample, flags, grid_x, grid_y, turn);
                row += oy;
                ox += column;
            }
            self.append(ox, row, region, sample, flags, grid_x, grid_y, turn);
            cx = grid_x; cy = grid_y;
        }
        grid_x = x1.floor(); grid_y = y1.floor();
        if x1 == grid_x || y1 == grid_y {
            let ox = if dx <= 0.0 { 0.0 } else { -1.0 };
            let oy = if dy <= 0.0 { 0.0 } else { -1.0 };
            column = x1.trunc(); row = y1.trunc();
            if x1 == grid_x {
                if y1 == grid_y { self.append(column + ox, row + oy, region, sample, 0x197, x1, y1, turn); }
                else { self.append(column + ox, row, region, sample, 0x115, x1, y1, turn); }
            } else { self.append(column, row + oy, region, sample, 0x95, x1, y1, turn); }
        }
    }

    fn insert_corner(&mut self, first: usize, following: usize, column: usize, row: usize, position: i64) -> usize {
        let corner = (position + 4) % 4;
        let id = self.records.len();
        let region = self.records[first].region;
        self.records.push(Record { flags: 0x18b, sample: -2, region,
            x: (column + (corner == 1 || corner == 2) as usize) as f64, y: (row + (corner == 2 || corner == 3) as usize) as f64,
            perimeter: corner as f64, turn: 0.0, next: following });
        self.records[first].next = id;
        id
    }

    fn connect_perimeter(&mut self, mut first: usize, following: usize, column: usize, row: usize) -> usize {
        let (f, g) = (self.records[first], self.records[following]);
        let mut turn = g.turn - f.turn;
        if g.sample < f.sample { turn += 2.0 * PI; }
        if turn < 0.0 {
            let mut end = g.perimeter.floor() as i64;
            if f.perimeter < g.perimeter { end -= 4; }
            let mut corner = (f.perimeter - 5.0).trunc() as i64 + 4;
            while end < corner { first = self.insert_corner(first, following, column, row, corner); corner -= 1; }
        } else {
            let mut end = g.perimeter.ceil() as i64;
            if g.perimeter < f.perimeter { end += 4; }
            let mut corner = (f.perimeter + 1.0).trunc() as i64;
            while corner < end { first = self.insert_corner(first, following, column, row, corner); corner += 1; }
        }
        first
    }

    fn clip_pixel(&mut self, pixel: usize) {
        let (column, row) = (pixel % self.width, pixel / self.width);
        let mut head = self.lists[pixel].head;
        let next_of = |records: &Vec<Record>, r: usize| records[r].next;
        let mut current = head;
        while current != NONE {
            let mut start = current;
            while current != NONE && {
                let n = next_of(&self.records, current);
                n == NONE || self.records[current].region != self.records[n].region
            } {
                current = self.records[current].next;
                if head == start { head = current; start = current; }
            }
            if current == NONE { break; }
            let region = self.records[current].region;
            let count = self.regions[region].contour.len() as i32;
            start = current;
            let mut last = current;
            loop {
                let following = self.records[last].next;
                if following == NONE || self.records[following].region != region { break; }
                let (l, f) = (self.records[last], self.records[following]);
                let common = f.flags & l.flags;
                if common & 1 != 0 && l.sample != f.sample && ((l.sample + 1) % count != f.sample || l.perimeter != f.perimeter)
                    && (common & 2 == 0 || (l.x != f.x && l.y != f.y)) {
                    last = self.connect_perimeter(last, following, column, row);
                }
                last = self.records[last].next;
            }
            let following_region = self.records[last].next;
            let (l, s) = (self.records[last], self.records[start]);
            if l.flags & s.flags & 1 != 0 && ((l.sample + 1) % count != s.sample || l.perimeter != s.perimeter) {
                last = self.connect_perimeter(last, start, column, row);
                self.records[last].next = following_region;
            }
            current = self.records[last].next;
        }
        let mut previous = NONE;
        current = head;
        while current != NONE {
            let mut last = current;
            let mut count = 1;
            let region = self.records[current].region;
            loop {
                let n = self.records[last].next;
                if n == NONE || self.records[n].region != region { break; }
                last = n; count += 1;
            }
            if count < 3 {
                if current == head { head = self.records[last].next; } else { self.records[previous].next = self.records[last].next; }
            }
            previous = last;
            current = self.records[last].next;
        }
        self.lists[pixel].head = head;
    }

    fn trace_pixels(&mut self, positions: &[f64]) {
        self.records.clear();
        // Only touched lists can retain records from the previous evaluation.
        for &pixel in &self.touched_pixels { self.lists[pixel] = List { head: NONE, tail: NONE }; }
        self.touched_pixels.clear();
        self.lists.resize(self.width * self.height, List { head: NONE, tail: NONE });
        for region in 0..self.regions.len() {
            let contour = self.regions[region].contour.clone();
            let n = contour.len();
            let mut previous = contour_angle_step(positions[contour[1] * 2 + 1] - positions[contour[0] * 2 + 1],
                positions[contour[1] * 2] - positions[contour[0] * 2], 0.0).angle;
            let mut turn = 0.0;
            for sample in 0..=n {
                if sample > 0 {
                    let (first, last) = (contour[sample % n] * 2, contour[(sample + 1) % n] * 2);
                    let step = contour_angle_step(positions[last + 1] - positions[first + 1], positions[last] - positions[first], previous);
                    let mut difference = step.difference;
                    if difference > PI { difference -= 2.0 * PI; } else if difference < -PI { difference += 2.0 * PI; }
                    turn = difference + turn;
                    previous = step.angle;
                }
                if sample < n { self.trace_edge(positions, region, sample, turn); }
            }
            if (turn - 2.0 * PI).abs() > 0.001 { self.invalid.insert(region); }
        }
        // Keep clipping and gradient additions in the original row-major order.
        self.touched_pixels.sort_unstable();
        for index in 0..self.touched_pixels.len() {
            let pixel = self.touched_pixels[index];
            if self.lists[pixel].head != NONE { self.clip_pixel(pixel); }
        }
    }

    fn fragment_region(&self, identifier: i32) -> Result<&KernelRegion, InputError> {
        self.regions.get(identifier as usize).ok_or_else(|| {
            InputError("Image fragment scratch identifies a region outside the prepared model.".into())
        })
    }

    fn color_pixel(&mut self, pixel: usize) -> Result<i32, InputError> {
        self.scratch.set_count(0);
        self.scratch.set_background(-1);
        let offset = pixel * 4;
        self.colors[offset..offset + 4].fill(0.0);
        let mut total = 0.0f64;
        let mut negative = false;
        let mut current = self.lists[pixel].head;
        while current != NONE {
            let start = self.records[current];
            let mut twice = 0.0;
            loop {
                let n = self.records[current].next;
                if n == NONE || self.records[n].region != start.region { break; }
                let (c, f) = (self.records[current], self.records[n]);
                twice = (f.y * c.x - c.y * f.x) + twice;
                current = n;
            }
            let c = self.records[current];
            let area = ((start.y * c.x - c.y * start.x) + twice) * 0.5;
            total += area;
            let rounded = area as f32;
            let color = self.regions[start.region].color;
            for ch in 0..4 { self.colors[offset + ch] += rounded * color[ch]; }
            self.scratch.write_identifier(self.scratch.count(), start.region as i32)?;
            self.scratch.write_area(self.scratch.count(), area)?;
            if area < 0.0 {
                self.flags[pixel] |= 1;
                if self.regions[start.region].contour.len() > 4 { negative = true; }
            }
            self.scratch.set_count(self.scratch.count().wrapping_add(1));
            current = self.records[current].next;
        }
        let original = self.scratch.count();
        if !negative && (total - 1.0).abs() > 0.001 {
            for index in 0..original {
                let parent = self.fragment_region(self.scratch.identifier(index)?)?.parent;
                if parent >= 0 && self.scratch.find(parent)? < 0 && self.scratch.background() < 0 { self.scratch.set_background(parent); }
            }
            let background = self.scratch.background();
            if background >= 0 {
                total += 1.0;
                let color = self.fragment_region(background)?.color;
                for ch in 0..4 { self.colors[offset + ch] += color[ch]; }
                self.scratch.write_identifier(self.scratch.count(), background)?;
                self.scratch.write_area(self.scratch.count(), 1.0)?;
                self.scratch.set_count(self.scratch.count().wrapping_add(1));
            }
            for index in 0..original {
                let parent = self.fragment_region(self.scratch.identifier(index)?)?.parent;
                let parent_index = self.scratch.find(parent)?;
                let color_region = if parent_index >= 0 { parent } else if parent < 0 { self.scratch.background() } else { -1 };
                if color_region >= 0 {
                    let rounded = self.scratch.area(index)? as f32;
                    let color = self.fragment_region(color_region)?.color;
                    for ch in 0..4 { self.colors[offset + ch] -= color[ch] * rounded; }
                    if parent_index >= 0 {
                        let v = self.scratch.area(parent_index)? - self.scratch.area(index)?;
                        self.scratch.write_area(parent_index, v)?;
                    } else {
                        let count = self.scratch.count();
                        let v = self.scratch.area(count)? - self.scratch.area(index)?;
                        self.scratch.write_area(count, v)?;
                    }
                    total -= self.scratch.area(index)?;
                }
            }
            for index in 0..self.scratch.count() {
                if self.scratch.area(index)? < 0.0 { negative = true; break; }
            }
        }
        if negative || (total - 1.0).abs() > 0.001 {
            for index in 0..self.scratch.count() { self.invalid.insert(self.scratch.identifier(index)? as usize); }
            if !negative { self.flags[pixel] |= 2; }
            self.lists[pixel].head = NONE;
            for ch in 0..4 { self.colors[offset + ch] = self.bgra[offset + ch] as f32 * BYTE_SCALE; }
        }
        Ok(self.scratch.background())
    }

    fn residuals(&self, offset: usize) -> [f32; 4] {
        std::array::from_fn(|ch| self.colors[offset + ch] - self.bgra[offset + ch] as f32 * BYTE_SCALE)
    }

    pub fn energy(&mut self, positions: &[f64]) -> Result<f64, InputError> {
        self.invalid.clear();
        self.trace_pixels(positions);
        self.backgrounds.resize(self.lists.len(), -1);
        for &pixel in &self.touched_pixels { self.backgrounds[pixel] = -1; }
        let mut energy = 0.0;
        for pixel in 0..self.lists.len() {
            if self.lists[pixel].head != NONE { self.backgrounds[pixel] = self.color_pixel(pixel)?; }
            let [b, g, r, a] = self.residuals(pixel * 4).map(|v| v as f64);
            energy = (((b * b + g * g) + r * r) + a * a) + energy;
        }
        energy *= 0.5;
        self.energy = energy;
        Ok(energy)
    }

    pub fn add_gradient(&self, gradient: &mut [f64], positions: &[f64]) {
        for &pixel in &self.touched_pixels {
            let head = self.lists[pixel].head;
            if head == NONE { continue; }
            let res = self.residuals(pixel * 4);
            let (mut current, mut start, mut region_id, mut factor) = (head, head, NONE, 0.0f64);
            while current != NONE {
                let record = self.records[current];
                if record.region != region_id {
                    region_id = record.region;
                    let region = &self.regions[region_id];
                    let mut c = region.color;
                    if self.backgrounds[pixel] >= 0 && region.parent >= 0 {
                        let p = self.regions[region.parent as usize].color;
                        for ch in 0..4 { c[ch] -= p[ch]; }
                    }
                    let [rb, rg, rr, ra] = res.map(|v| v as f64);
                    factor = (((rb * c[0] as f64 + rg * c[1] as f64) + rr * c[2] as f64) + ra * c[3] as f64) * 0.5;
                    start = current;
                }
                let n = record.next;
                let following = if n != NONE && self.records[n].region == region_id { n } else { start };
                let contour = &self.regions[region_id].contour;
                add_record_gradient(gradient, positions, contour, &record, &self.records[following], factor, false);
                add_record_gradient(gradient, positions, contour, &self.records[following], &record, factor, true);
                current = n;
            }
        }
    }

    pub fn invalid_regions(&self) -> Vec<i32> { self.invalid.iter().map(|&r| r as i32).collect() }

    pub fn take_invalid_regions(&mut self) -> Vec<i32> {
        let ids = self.invalid_regions();
        self.invalid.clear();
        ids
    }

    pub fn snapshot(&self, gradient: Vec<f64>) -> Measurement {
        let pixels = self.lists.iter().map(|list| {
            let mut out = Vec::new();
            let mut r = list.head;
            while r != NONE {
                let c = &self.records[r];
                out.push([c.flags as f64, c.sample as f64, c.region as f64, c.x, c.y, c.perimeter, c.turn]);
                r = c.next;
            }
            out
        }).collect();
        Measurement { energy: self.energy, gradient, colors: self.colors.clone(), flags: self.flags.clone(), pixels }
    }
}

fn add_record_gradient(gradient: &mut [f64], positions: &[f64], contour: &[usize], record: &Record, adjacent: &Record, factor: f64, negate: bool) {
    if record.sample < 0 { return; }
    let n = contour.len();
    let sample = if record.flags & 0x10 != 0 { (record.sample as usize + 1) % n } else { record.sample as usize };
    let first = contour[sample] * 2;
    let last = contour[(sample + 1) % n] * 2;
    if record.flags & 1 == 0 {
        gradient[first] = (if negate { -adjacent.y } else { adjacent.y }) * factor + gradient[first];
        gradient[first + 1] = (if negate { adjacent.x } else { -adjacent.x }) * factor + gradient[first + 1];
        return;
    }
    let variable = if (record.flags & 0x80 == 0 && positions[first] != positions[last]) || positions[first + 1] == positions[last + 1] { 1 } else { 0 };
    let fixed = 1 - variable;
    let coordinate = if fixed == 0 { record.x } else { record.y };
    let fraction = (coordinate - positions[last + fixed]) / (positions[first + fixed] - positions[last + fixed]);
    let slope = (positions[first + variable] - positions[last + variable]) / (positions[first + fixed] - positions[last + fixed]);
    let first_fixed = -(slope * fraction);
    let last_variable = 1.0 - fraction;
    let last_fixed = -((1.0 - fraction) * slope);
    let multiplier = if variable == 0 { if negate { -adjacent.y } else { adjacent.y } } else if negate { adjacent.x } else { -adjacent.x };
    gradient[first] = (if variable == 0 { fraction } else { first_fixed }) * multiplier * factor + gradient[first];
    gradient[first + 1] = (if variable == 1 { fraction } else { first_fixed }) * multiplier * factor + gradient[first + 1];
    gradient[last] = (if variable == 0 { last_variable } else { last_fixed }) * multiplier * factor + gradient[last];
    gradient[last + 1] = (if variable == 1 { last_variable } else { last_fixed }) * multiplier * factor + gradient[last + 1];
}
