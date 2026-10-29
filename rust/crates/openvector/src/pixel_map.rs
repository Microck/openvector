//! Port of `src/pixel-map.mjs`: owns segment identifiers independently of the
//! palette and source bitmap.

use crate::pixel_input::{fail, InputError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelMap {
    width: i64,
    height: i64,
    segments: Vec<i32>,
}

fn is_int32(v: i64) -> bool { v >= -2147483648 && v <= 2147483647 }

impl PixelMap {
    /// Segments arrive as i64 so out-of-range identifiers can be rejected.
    pub fn new(width: i64, height: i64, segments: &[i64]) -> Result<Self, InputError> {
        if !is_int32(width) || width < 1 || !is_int32(height) || height < 1 {
            return fail("Map width and height must be positive int32 integers.");
        }
        if segments.len() as i64 != width * height || width * height > 2147483647 {
            return fail("Map segments must contain exactly width * height identifiers.");
        }
        if segments.iter().any(|&s| !is_int32(s)) {
            return fail("Each segment identifier must be a signed int32 integer.");
        }
        Ok(PixelMap { width, height, segments: segments.iter().map(|&s| s as i32).collect() })
    }

    pub fn width(&self) -> i64 { self.width }
    pub fn height(&self) -> i64 { self.height }
    pub fn segments(&self) -> &[i32] { &self.segments }

    pub fn flood_fill(&mut self, seed: i64, replacement: i64) -> Result<(), InputError> {
        let len = self.segments.len();
        if seed < 0 || seed as usize >= len {
            return fail("Fill seed must be a valid row-major pixel index.");
        }
        if !is_int32(replacement) { return fail("Replacement must be a signed int32 identifier."); }
        let replacement = replacement as i32;
        let seed = seed as usize;
        let segments = &mut self.segments;
        let original = segments[seed];
        if original == replacement { return fail("Replacement must differ from the seed segment."); }
        let width = self.width as usize;
        let mut queue = vec![0usize; len];
        let (mut head, mut tail) = (0usize, 1usize);
        queue[0] = seed;
        segments[seed] = replacement;
        while head < tail {
            let index = queue[head];
            head += 1;
            let column = index % width;
            let mut visit = |i: usize, segments: &mut Vec<i32>| {
                if segments[i] == original {
                    segments[i] = replacement;
                    queue[tail] = i;
                    tail += 1;
                }
            };
            if column > 0 { visit(index - 1, segments); }
            if column + 1 < width { visit(index + 1, segments); }
            if index >= width { visit(index - width, segments); }
            if index + width < len { visit(index + width, segments); }
        }
        Ok(())
    }
}
