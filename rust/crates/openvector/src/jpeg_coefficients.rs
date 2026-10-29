//! JPEG coefficient reconstruction adapted from the Apache-2.0 jpeg-js
//! decoder by notmasteryet (2011), as retained in src/jpeg-coefficients.mjs.
//! Sampling and color conversion belong to jpeg_input.
//
// Copyright 2011 notmasteryet
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use crate::pixel_input::{fail, image_size, InputError};

const ZIGZAG: [usize; 64] = [0,1,8,16,9,2,3,10,17,24,32,25,18,11,4,5,12,19,26,33,40,48,41,34,27,20,13,6,7,14,21,28,35,42,49,56,57,50,43,36,29,22,15,23,30,37,44,51,58,59,52,45,38,31,39,46,53,60,61,54,47,55,62,63];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jfif {
    pub major: u8, pub minor: u8, pub density_units: u8,
    pub x_density: u16, pub y_density: u16, pub thumb_width: u8,
    pub thumb_height: u8, pub thumb_data: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adobe { pub version: u8, pub flags0: u16, pub flags1: u16, pub transform_code: u8 }
#[derive(Debug, Clone)]
pub struct JpegComponent {
    pub id: u8, pub h: usize, pub v: usize,
    pub blocks_per_line: usize, pub blocks_per_column: usize,
    pub block_stride: usize, pub blocks: Vec<[i32; 64]>, pub quantization_table: [i32; 64],
    quantization_id: usize, pred: i32,
}
#[derive(Debug, Clone)]
pub struct JpegCoefficients {
    pub width: usize, pub height: usize, pub jfif: Option<Jfif>, pub adobe: Option<Adobe>,
    pub components: Vec<JpegComponent>, pub max_h: usize, pub max_v: usize,
    progressive: bool, mcus_per_line: usize, mcus_per_column: usize,
}
#[derive(Clone, Default)]
struct Huffman { first_code: [u32;16], first_symbol: [usize;16], counts: [u8;16], values: Vec<u8> }
impl Huffman {
    fn new(lengths: &[u8], values: &[u8]) -> Result<Self, InputError> {
        let mut table = Self { values: values.to_vec(), ..Self::default() };
        let (mut code, mut index) = (0u32, 0usize);
        for (i, &count) in lengths.iter().enumerate() {
            if code + count as u32 > 1 << (i + 1) { return fail("Could not recreate Huffman Table"); }
            table.first_code[i] = code; table.first_symbol[i] = index; table.counts[i] = count;
            code = (code + count as u32) << 1; index += count as usize;
        }
        Ok(table)
    }
}
struct Reader<'a> { data: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, InputError> {
        let b = *self.data.get(self.offset).ok_or_else(|| InputError::new("JPEG marker or header is incomplete."))?;
        self.offset += 1; Ok(b)
    }
    fn word(&mut self) -> Result<u16, InputError> { Ok((self.byte()? as u16) << 8 | self.byte()? as u16) }
    fn take(&mut self, count: usize) -> Result<&'a [u8], InputError> {
        let end = self.offset.checked_add(count).filter(|&x| x <= self.data.len()).ok_or_else(|| InputError::new("JPEG marker payload is incomplete."))?;
        let out = &self.data[self.offset..end]; self.offset = end; Ok(out)
    }
    fn block(&mut self) -> Result<&'a [u8], InputError> {
        let n = self.word()? as usize; if n < 2 { return fail("JPEG marker payload is incomplete."); } self.take(n - 2)
    }
}
struct Entropy<'a> { reader: Reader<'a>, byte: u8, bits: u8, eob: i32, state: u8, next_value: i32 }
impl Entropy<'_> {
    fn bit(&mut self) -> Result<i32, InputError> {
        if self.bits == 0 {
            self.byte = self.reader.byte()?;
            if self.byte == 255 { let b = self.reader.byte()?; if b != 0 { return Err(InputError::new(format!("unexpected marker: {:x}", 0xff00u16 | b as u16))); } }
            self.bits = 8;
        }
        self.bits -= 1; Ok(((self.byte >> self.bits) & 1) as i32)
    }
    fn receive(&mut self, bits: u8) -> Result<i32, InputError> { let mut n = 0i32; for _ in 0..bits { n = n.wrapping_shl(1) | self.bit()?; } Ok(n) }
    fn extend(&mut self, bits: u8) -> Result<i32, InputError> {
        if bits == 0 { return Ok(0); }
        let n = self.receive(bits)?;
        Ok(if n >= 1i32.wrapping_shl((bits - 1) as u32) { n } else { n.wrapping_add((-1i32).wrapping_shl(bits as u32)).wrapping_add(1) })
    }
    fn huffman(&mut self, table: &Huffman) -> Result<u8, InputError> {
        let mut code = 0u32;
        for bits in 0..16 {
            code = (code << 1) | self.bit()? as u32;
            if let Some(index) = code.checked_sub(table.first_code[bits]) {
                if index < table.counts[bits] as u32 { return Ok(table.values[table.first_symbol[bits] + index as usize]); }
            }
        }
        fail("invalid huffman sequence")
    }
    fn block(&mut self, block: &mut [i32; 64], pred: &mut i32, dc: &Huffman, ac: &Huffman, progressive: bool, start: usize, end: usize, prev: u8, successive: u8) -> Result<(), InputError> {
        if !progressive || start == 0 && prev == 0 {
            let t = self.huffman(dc)?;
            let diff = self.extend(t)?.wrapping_shl(if progressive { successive as u32 } else { 0 });
            *pred = pred.wrapping_add(diff); block[0] = *pred;
            if progressive { return Ok(()); }
            let mut k = 1;
            while k < 64 {
                let rs = self.huffman(ac)?; let (s, r) = (rs & 15, (rs >> 4) as usize);
                if s == 0 { if r < 15 { break; } k += 16; continue; }
                k += r; let value = self.extend(s)?; if k < 64 { block[ZIGZAG[k]] = value; } k += 1;
            }
        } else if start == 0 { block[0] |= self.bit()?.wrapping_shl(successive as u32); }
        else if prev == 0 {
            if self.eob > 0 { self.eob -= 1; return Ok(()); }
            let mut k = start;
            while k <= end {
                let rs = self.huffman(ac)?; let (s, r) = (rs & 15, (rs >> 4) as usize);
                if s == 0 { if r < 15 { self.eob = self.receive(r as u8)? + (1 << r) - 1; break; } k += 16; continue; }
                k += r; let v = self.extend(s)?.wrapping_shl(successive as u32); if k < 64 { block[ZIGZAG[k]] = v; } k += 1;
            }
        } else {
            let (mut k, mut r) = (start, 0i32);
            while k <= end && k < 64 {
                let z = ZIGZAG[k]; let direction = if block[z] < 0 { -1 } else { 1 };
                match self.state {
                    0 => {
                        let rs = self.huffman(ac)?; let s = rs & 15; r = (rs >> 4) as i32;
                        if s == 0 { if r < 15 { self.eob = self.receive(r as u8)? + (1 << r); self.state = 4; } else { r = 16; self.state = 1; } }
                        else { if s != 1 { return fail("invalid ACn encoding"); } self.next_value = self.extend(s)?; self.state = if r != 0 { 2 } else { 3 }; }
                        continue;
                    }
                    1 | 2 => { if block[z] != 0 { block[z] = block[z].wrapping_add((self.bit()? << successive) * direction); } else { r -= 1; if r == 0 { self.state = if self.state == 2 { 3 } else { 0 }; } } }
                    3 => { if block[z] != 0 { block[z] = block[z].wrapping_add((self.bit()? << successive) * direction); } else { block[z] = self.next_value << successive; self.state = 0; } }
                    4 => { if block[z] != 0 { block[z] = block[z].wrapping_add((self.bit()? << successive) * direction); } }
                    _ => unreachable!(),
                }
                k += 1;
            }
            if self.state == 4 { self.eob -= 1; if self.eob == 0 { self.state = 0; } }
        }
        Ok(())
    }
}

pub fn read_jpeg_coefficients(data: &[u8]) -> Result<JpegCoefficients, InputError> {
    let mut reader = Reader { data, offset: 0 };
    if reader.word()? != 0xffd8 { return fail("SOI not found"); }
    let mut quantization = [None; 16];
    let mut dc: [Huffman; 16] = std::array::from_fn(|_| Huffman::default());
    let mut ac: [Huffman; 16] = std::array::from_fn(|_| Huffman::default());
    let (mut jfif, mut adobe, mut frame) = (None, None, None::<JpegCoefficients>);
    let (mut frames, mut reset, mut allocated) = (0, 0usize, 0usize);
    loop {
        let marker = reader.word()?;
        match marker {
            0xffd9 => break,
            0xff00 => {},
            0xffe0..=0xffef | 0xfffe => {
                let b = reader.block()?;
                if marker == 0xffe0 && b.starts_with(b"JFIF\0") && b.len() >= 14 {
                    jfif = Some(Jfif { major: b[5], minor: b[6], density_units: b[7], x_density: u16::from_be_bytes([b[8],b[9]]), y_density: u16::from_be_bytes([b[10],b[11]]), thumb_width: b[12], thumb_height: b[13], thumb_data: b[14..b.len().min(14 + 3 * b[12] as usize * b[13] as usize)].to_vec() });
                }
                if marker == 0xffee && b.starts_with(b"Adobe\0") && b.len() >= 12 {
                    adobe = Some(Adobe { version: b[6], flags0: u16::from_be_bytes([b[7],b[8]]), flags1: u16::from_be_bytes([b[9],b[10]]), transform_code: b[11] });
                }
            }
            0xffdb => {
                let b = reader.block()?; let mut r = Reader { data: b, offset: 0 };
                while r.offset < b.len() {
                    let spec = r.byte()?; let mut table = [0; 64];
                    for &z in &ZIGZAG { table[z] = match spec >> 4 { 0 => r.byte()? as i32, 1 => r.word()? as i32, _ => return fail("DQT: invalid table spec") }; }
                    allocated += 256; quantization[(spec & 15) as usize] = Some(table);
                }
            }
            0xffc0..=0xffc2 => {
                reader.word()?;
                if reader.byte()? != 8 { return fail("JPEG data precision must be eight bits."); }
                let height = reader.word()? as usize; let width = reader.word()? as usize;
                image_size(width as i64, height as i64)?;
                let count = reader.byte()?; let mut components = Vec::with_capacity(count as usize);
                let (mut max_h, mut max_v) = (1usize, 1usize);
                for _ in 0..count {
                    let id = reader.byte()?; let hv = reader.byte()?; let q = reader.byte()? as usize;
                    let (h, v) = ((hv >> 4) as usize, (hv & 15) as usize);
                    if !(1..=4).contains(&h) || !(1..=4).contains(&v) { return fail("JPEG sampling factors must be from one through four."); }
                    max_h = max_h.max(h); max_v = max_v.max(v);
                    components.push(JpegComponent { id, h, v, blocks_per_line: 0, blocks_per_column: 0, block_stride: 0, blocks: Vec::new(), quantization_table: [0;64], quantization_id: q, pred: 0 });
                }
                let mcus_per_line = width.div_ceil(8 * max_h); let mcus_per_column = height.div_ceil(8 * max_v);
                for c in &mut components {
                    c.blocks_per_line = (width.div_ceil(8) * c.h).div_ceil(max_h);
                    c.blocks_per_column = (height.div_ceil(8) * c.v).div_ceil(max_v);
                    c.block_stride = mcus_per_line * c.h;
                    let count = c.block_stride * mcus_per_column * c.v;
                    allocated += count * 256;
                    if allocated > 2147483647 { return fail("JPEG component storage exceeds an int32 byte count."); }
                    c.blocks = vec![[0;64]; count];
                }
                frame = Some(JpegCoefficients { width, height, jfif: None, adobe: None, components, max_h, max_v, progressive: marker == 0xffc2, mcus_per_line, mcus_per_column }); frames += 1;
            }
            0xffc4 => {
                let b = reader.block()?; let mut r = Reader { data: b, offset: 0 };
                while r.offset < b.len() { let spec = r.byte()?; let lengths = r.take(16)?; let n = lengths.iter().map(|&x| x as usize).sum(); let values = r.take(n)?; allocated += 16 + n; let table = Huffman::new(lengths, values)?; if spec >> 4 == 0 { dc[(spec & 15) as usize] = table; } else { ac[(spec & 15) as usize] = table; } }
            }
            0xffdd => { reader.word()?; reset = reader.word()? as usize; }
            0xffdc => { reader.word()?; reader.word()?; }
            0xffda => {
                reader.word()?; let count = reader.byte()? as usize;
                let f = frame.as_mut().ok_or_else(|| InputError::new("only single frame JPEGs supported"))?;
                let mut selectors = Vec::with_capacity(count);
                for _ in 0..count { let id = reader.byte()?; let spec = reader.byte()?; let index = f.components.iter().position(|c| c.id == id).ok_or_else(|| InputError::new("invalid huffman sequence"))?; selectors.push((index, (spec >> 4) as usize, (spec & 15) as usize)); }
                let start = reader.byte()? as usize; let end = reader.byte()? as usize; let approximation = reader.byte()?;
                let expected = if count == 1 { let c = &f.components[selectors[0].0]; c.blocks_per_line * c.blocks_per_column } else { f.mcus_per_line * f.mcus_per_column };
                let interval = if reset == 0 { expected } else { reset };
                let mut e = Entropy { reader: Reader { data, offset: reader.offset }, byte: 0, bits: 0, eob: 0, state: 0, next_value: 0 };
                let mut mcu = 0;
                while mcu < expected {
                    for &(i, _, _) in &selectors { f.components[i].pred = 0; } e.eob = 0;
                    for _ in 0..interval.min(expected - mcu) {
                        for &(i, d, a) in &selectors {
                            let c = &mut f.components[i];
                            let (rows, cols) = if count == 1 { (1,1) } else { (c.v,c.h) };
                            for row in 0..rows { for col in 0..cols {
                                let (by, bx) = if count == 1 { (mcu / c.blocks_per_line, mcu % c.blocks_per_line) } else { (mcu / f.mcus_per_line * c.v + row, mcu % f.mcus_per_line * c.h + col) };
                                e.block(&mut c.blocks[by * c.block_stride + bx], &mut c.pred, &dc[d], &ac[a], f.progressive, start, end, approximation >> 4, approximation & 15)?;
                            } }
                        }
                        mcu += 1;
                    }
                    if mcu == expected { while e.reader.offset + 1 < data.len() { if data[e.reader.offset] == 255 && data[e.reader.offset + 1] != 0 { break; } e.reader.offset += 1; } }
                    e.bits = 0;
                    let pos = e.reader.offset; let mark = ((data.get(pos).copied().unwrap_or(0) as u16) << 8) | data.get(pos + 1).copied().unwrap_or(0) as u16;
                    if mark < 0xff00 { return fail("marker was not found"); }
                    if (0xffd0..=0xffd7).contains(&mark) { e.reader.offset += 2; } else { break; }
                }
                reader.offset = e.reader.offset;
            }
            0xffff => { if data.get(reader.offset) != Some(&255) { reader.offset -= 1; } }
            _ => return Err(InputError::new(format!("unknown JPEG marker {marker:x}"))),
        }
        if allocated > 2147483647 { return fail("JPEG component storage exceeds an int32 byte count."); }
    }
    if frames != 1 { return fail("only single frame JPEGs supported"); }
    let mut frame = frame.unwrap(); frame.jfif = jfif; frame.adobe = adobe;
    for c in &mut frame.components { c.quantization_table = quantization.get(c.quantization_id).and_then(|x| *x).ok_or_else(|| InputError::new("DQT: invalid table spec"))?; }
    Ok(frame)
}
