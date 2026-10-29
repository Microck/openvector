//! CCITT run tables adapted from UTIF.js (Photopea, 2017, MIT).
//! See src/tiff-licenses.txt. Bitstream decoding follows src/tiff-fax.mjs.
use crate::pixel_input::{fail, InputError};
use std::sync::LazyLock;
const TERM_W: &str = concat!(
"00110101,000111,0111,1000,1011,1100,1110,1111,10011,10100,00111,01000,001000,000011,110100,110101,",
"101010,101011,0100111,0001100,0001000,0010111,0000011,0000100,0101000,0101011,0010011,0100100,0011000,00000010,00000011,00011010,",
"00011011,00010010,00010011,00010100,00010101,00010110,00010111,00101000,00101001,00101010,00101011,00101100,00101101,00000100,00000101,00001010,",
"00001011,01010010,01010011,01010100,01010101,00100100,00100101,01011000,01011001,01011010,01011011,01001010,01001011,00110010,00110011,00110100");
const TERM_B: &str = concat!(
"0000110111,010,11,10,011,0011,0010,00011,000101,000100,0000100,0000101,0000111,00000100,00000111,000011000,",
"0000010111,0000011000,0000001000,00001100111,00001101000,00001101100,00000110111,00000101000,00000010111,00000011000,000011001010,000011001011,000011001100,000011001101,000001101000,000001101001,",
"000001101010,000001101011,000011010010,000011010011,000011010100,000011010101,000011010110,000011010111,000001101100,000001101101,000011011010,000011011011,000001010100,000001010101,000001010110,000001010111,",
"000001100100,000001100101,000001010010,000001010011,000000100100,000000110111,000000111000,000000100111,000000101000,000001011000,000001011001,000000101011,000000101100,000001011010,000001100110,000001100111");
const MAKE_W: &str = "11011,10010,010111,0110111,00110110,00110111,01100100,01100101,01101000,01100111,011001100,011001101,011010010,011010011,011010100,011010101,011010110,011010111,011011000,011011001,011011010,011011011,010011000,010011001,010011010,011000,010011011";
const MAKE_B: &str = "0000001111,000011001000,000011001001,000001011011,000000110011,000000110100,000000110101,0000001101100,0000001101101,0000001001010,0000001001011,0000001001100,0000001001101,0000001110010,0000001110011,0000001110100,0000001110101,0000001110110,0000001110111,0000001010010,0000001010011,0000001010100,0000001010101,0000001011010,0000001011011,0000001100100,0000001100101";
const MAKE_A: &str = "00000001000,00000001100,00000001101,000000010010,000000010011,000000010100,000000010101,000000010110,000000010111,000000011100,000000011101,000000011110,000000011111";
static TABLES: LazyLock<[[u16; 16384]; 2]> = LazyLock::new(|| {
        let mut tables = [[0;16384];2];
        for (color, table) in tables.iter_mut().enumerate() {
            for (text, start, step) in [(if color == 0 { TERM_W } else { TERM_B }, 0, 1), (if color == 0 { MAKE_W } else { MAKE_B },64,64), (MAKE_A,1792,64)] {
                for (i, word) in text.split(',').enumerate() { let code = word.bytes().fold(1usize, |code, b| (code << 1) | (b - b'0') as usize); table[code] = (start + i * step + 1) as u16; }
            }
        }
        tables
});
struct Bits<'a> { data: &'a [u8], bit: usize, fill_order: u32 }
impl Bits<'_> {
    fn read(&mut self) -> Result<usize, InputError> {
        if self.bit >= self.data.len() * 8 { return fail("TIFF CCITT bitstream is incomplete."); }
        let shift = if self.fill_order == 2 { self.bit & 7 } else { 7 - (self.bit & 7) };
        let value = (self.data[self.bit / 8] >> shift) & 1; self.bit += 1; Ok(value as usize)
    }
    fn run(&mut self, color: usize, width: usize) -> Result<usize, InputError> {
        let mut length = 0;
        loop {
            let (mut code, mut part) = (1usize, 0u16);
            for _ in 0..13 { code = (code << 1) | self.read()?; part = TABLES[color][code]; if part != 0 { break; } }
            if part == 0 { return fail("TIFF CCITT run code is invalid."); }
            let part = part as usize - 1; length += part;
            if length > width { return fail("TIFF CCITT run exceeds the scanline."); }
            if part < 64 { return Ok(length); }
        }
    }
    fn eol(&mut self) -> Result<(), InputError> {
        let mut zeros = 0; while self.read()? == 0 { zeros += 1; }
        if zeros < 11 { return fail("TIFF Group3 end-of-line code is missing."); } Ok(())
    }
}
fn paint(line: &mut [u8], position: &mut usize, end: isize, color: usize) -> Result<(), InputError> {
    if end < *position as isize || end > line.len() as isize { return fail("TIFF CCITT transition is outside the scanline."); }
    if color != 0 { line[*position..end as usize].fill(1); } *position = end as usize; Ok(())
}
pub fn decode_fax(encoded: &[u8], width: usize, rows: usize, compression: u32, fill_order: u32, t4_options: u32) -> Result<Vec<u8>, InputError> {
    let row_bytes = width.div_ceil(8); let mut output = vec![0; row_bytes * rows];
    let mut bits = Bits { data: encoded, bit: 0, fill_order };
    let mut previous = vec![0u8; width]; let mut line = vec![0u8;width]; let mut changes = Vec::with_capacity(width + 2);
    for row in 0..rows {
        let mut one = compression == 2 || compression == 32771;
        if compression == 3 { bits.eol()?; one = t4_options & 1 == 0 || bits.read()? == 1; }
        line.fill(0); let (mut position, mut color) = (0usize, 0usize);
        if one { while position < width { let end = position + bits.run(color,width)?; paint(&mut line,&mut position,end as isize,color)?; color ^= 1; } }
        else {
            changes.clear();
            if previous.first() == Some(&1) { changes.push((0,1)); }
            for x in 1..width { if previous[x] != previous[x - 1] { changes.push((x,previous[x] as usize)); } }
            changes.push((width,0)); changes.push((width,1)); let mut reference = 0;
            while position < width {
                let at = position + usize::from(position != 0);
                while changes[reference].0 < at { reference += 1; }
                let mut first = reference; while changes[first].1 != 1 - color { first += 1; }
                let b1 = changes[first].0;
                let mut second = reference; while changes[second].0 < b1 || changes[second].1 != color { second += 1; }
                let b2 = changes[second].0;
                let (mut code, mut mode) = (1usize, None);
                for _ in 0..7 {
                    code = (code << 1) | bits.read()?;
                    mode = match code { 0b10001 => Some((0,0)), 0b1001 => Some((1,0)), 0b11 => Some((2,0)), 0b1011 => Some((2,1)), 0b1000011 => Some((2,2)), 0b10000011 => Some((2,3)), 0b1010 => Some((2,-1)), 0b1000010 => Some((2,-2)), 0b10000010 => Some((2,-3)), _ => None };
                    if mode.is_some() { break; }
                }
                match mode {
                    Some((0,_)) => paint(&mut line,&mut position,b2 as isize,color)?,
                    Some((1,_)) => { for _ in 0..2 { let end = position + bits.run(color,width)?; paint(&mut line,&mut position,end as isize,color)?; color ^= 1; } }
                    Some((2,delta)) => { paint(&mut line,&mut position,b1 as isize + delta,color)?; color ^= 1; }
                    _ => return fail("TIFF CCITT two-dimensional code is unsupported or invalid."),
                }
            }
        }
        for x in 0..width { if line[x] != 0 { output[row * row_bytes + x / 8] |= 1 << (7 - (x & 7)); } }
        std::mem::swap(&mut line,&mut previous);
        if compression == 2 || compression == 32771 { let alignment = if compression == 32771 { 16 } else { 8 }; bits.bit = bits.bit.div_ceil(alignment) * alignment; }
    }
    Ok(output)
}
