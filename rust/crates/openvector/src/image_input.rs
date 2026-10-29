//! Byte-signature dispatch, legacy Q8 PNG import, and native filename metadata.
use std::io::Read;
use std::sync::LazyLock;
use crate::bmp_input::{ascii_pair, decode_bmp};
use crate::gif_input::decode_gif;
use crate::jpeg_input::decode_jpeg;
use crate::tiff_input::decode_tiff;
use crate::pixel_input::{fail, image_size, DecodedImage, InputError};

const PNG_SIGNATURE: &[u8] = &[137,80,78,71,13,10,26,10];
const ADAM7: [[usize;4];7] = [[0,0,8,8],[4,0,8,8],[0,4,4,8],[2,0,4,4],[0,2,2,4],[1,0,2,2],[0,1,1,2]];
static CRC_TABLE: LazyLock<[u32;256]> = LazyLock::new(|| std::array::from_fn(|byte| {
    let mut value = byte as u32; for _ in 0..8 { value = if value & 1 != 0 { 0xedb88320 ^ (value >> 1) } else { value >> 1 }; } value
}));
fn crc32(bytes: &[u8]) -> u32 { bytes.iter().fold(0xffffffff, |crc,&b| CRC_TABLE[((crc ^ b as u32)&255) as usize] ^ (crc >> 8)) ^ 0xffffffff }
fn word(bytes: &[u8], offset: usize) -> u32 { u32::from_be_bytes(bytes[offset..offset+4].try_into().unwrap()) }
fn short(bytes: &[u8], offset: usize) -> u16 { u16::from_be_bytes([bytes[offset],bytes[offset+1]]) }
fn sample_at(row: &[u8], index: usize, depth: usize) -> u16 {
    if depth == 8 { row[index] as u16 } else if depth == 16 { short(row,index*2) }
    else { let bit = index*depth; ((row[bit/8] >> (8-depth-bit%8)) & ((1 << depth)-1)) as u16 }
}
fn unfilter(encoded: &[u8], offset: &mut usize, row: &mut [u8], previous: &[u8], bpp: usize) -> Result<(), InputError> {
    let filter = encoded[*offset]; *offset += 1;
    if filter > 4 { return fail("PNG row filter must be from 0 through 4."); }
    for index in 0..row.len() {
        let left = if index >= bpp { row[index-bpp] as i32 } else { 0 };
        let above = previous[index] as i32; let upper_left = if index >= bpp { previous[index-bpp] as i32 } else { 0 };
        let predictor = match filter { 1 => left, 2 => above, 3 => (left+above) >> 1, 4 => {
            let estimate = left+above-upper_left; let a = (estimate-left).abs(); let b = (estimate-above).abs(); let c = (estimate-upper_left).abs();
            if a <= b && a <= c { left } else if b <= c { above } else { upper_left }
        }, _ => 0 };
        row[index] = encoded[*offset+index].wrapping_add(predictor as u8);
    }
    *offset += row.len(); Ok(())
}
fn import_legacy_rows(storage: &mut [u8], bgra: &mut [u8], width: usize, height: usize, kind: u8, transparency: Option<&[u8]>) {
    let channels = if kind == 2 { 3 } else { 4 }; let stride = width*channels*2;
    let keys = transparency.map(|t| [short(t,0)&255,short(t,2)&255,short(t,4)&255]);
    for y in 0..height {
        let row = &mut storage[y*stride..(y+1)*stride];
        if kind == 6 { for index in 0..width*4 { row[index] = row[index*2]; } }
        else {
            let (mut read,mut write) = (0,0);
            for _ in 0..width {
                for _ in 0..3 { row[write] = row[read]; write += 1; read += 2; }
                let transparent = keys.is_some_and(|keys| short(row,read-6) == keys[0] && short(row,read-4) == keys[1] && short(row,read-2) == keys[2]);
                row[write] = if transparent { 255 } else { 0 }; write += 1;
            }
        }
        for x in 0..width { let source = x*4; let target = (y*width+x)*4; bgra[target..target+4].copy_from_slice(&[row[source+2],row[source+1],row[source],row[source+3]]); }
    }
}

pub fn decode_png(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    let (mut width,mut height,mut depth,mut kind,mut interlace,mut channels) = (0usize,0usize,0usize,0u8,0u8,0usize);
    let (mut header,mut ended) = (false,false); let (mut palette,mut transparency) = (None::<&[u8]>,None::<&[u8]>);
    let mut compressed = Vec::new(); let mut data_chunks = 0; let mut offset = 8usize;
    while offset < encoded.len() {
        if offset+12 > encoded.len() { return fail("PNG chunk header is incomplete."); }
        let length = word(encoded,offset) as usize; let end = offset+8+length;
        if end+4 > encoded.len() { return fail("PNG chunk payload is incomplete."); }
        let name: [u8;4] = std::array::from_fn(|i| encoded[offset+4+i]&127);
        let payload = &encoded[offset+8..end];
        if crc32(&encoded[offset+4..end]) != word(encoded,end) { return Err(InputError::new(format!("PNG {} chunk CRC does not match.",String::from_utf8_lossy(&name)))); }
        match &name {
            b"IHDR" => {
                if header || offset != 8 || length != 13 { return fail("PNG requires one initial 13-byte IHDR."); }
                width = word(payload,0) as usize; height = word(payload,4) as usize; depth = payload[8] as usize; kind = payload[9]; interlace = payload[12];
                channels = match kind { 0|3 => 1, 2 => 3, 4 => 2, 6 => 4, _ => 0 };
                let valid_depth = match kind { 0 => [1,2,4,8,16].contains(&depth), 3 => [1,2,4,8].contains(&depth), _ => [8,16].contains(&depth) };
                if channels == 0 || !valid_depth || payload[10] != 0 || payload[11] != 0 || interlace > 1 { return fail("Unsupported PNG color, depth, compression, filter or interlace method."); }
                image_size(width as i64,height as i64)?; header = true;
            }
            _ if !header => return fail("PNG image header is missing."),
            b"PLTE" => palette = Some(payload),
            b"tRNS" => transparency = Some(payload),
            b"IDAT" => { data_chunks += 1; compressed.extend_from_slice(payload); }
            b"IEND" => { if length != 0 { return fail("PNG IEND must be empty."); } ended = true; break; }
            _ => { if encoded[offset+4]&32 == 0 { return Err(InputError::new(format!("Unsupported PNG critical chunk {}.",String::from_utf8_lossy(&name)))); } }
        }
        offset = end+4;
    }
    if !ended || data_chunks == 0 { return fail("PNG image data or trailer is missing."); }
    if kind == 3 && palette.is_none_or(|p| p.is_empty() || p.len()%3 != 0 || p.len() > 768) { return fail("Indexed PNG requires a valid RGB palette."); }
    if transparency.is_some_and(|t| kind == 0 && t.len() != 2 || kind == 2 && t.len() != 6) { return fail("PNG transparency key has the wrong size."); }
    let passes: &[[usize;4]] = if interlace != 0 { &ADAM7 } else { &[[0,0,1,1]] };
    let expected: usize = passes.iter().map(|&[x,y,dx,dy]| { let w = width.saturating_sub(x).div_ceil(dx); let h = height.saturating_sub(y).div_ceil(dy); if w != 0 && h != 0 { h*((w*channels*depth).div_ceil(8)+1) } else { 0 } }).sum();
    let mut inflated = Vec::with_capacity(expected);
    flate2::read::ZlibDecoder::new(&compressed[..]).take(expected as u64+1).read_to_end(&mut inflated).map_err(|e| InputError::new(e.to_string()))?;
    if inflated.len() != expected { return fail("PNG decoded scanline size does not match its dimensions."); }
    let mut bgra = vec![0;width*height*4]; let bpp = (channels*depth).div_ceil(8).max(1);
    let legacy = depth == 16 && (kind == 2 || kind == 6); let mut storage = if legacy { vec![0;width*height*channels*2] } else { Vec::new() };
    let scale = |n: u16| -> u8 { if depth == 16 { (n >> 8) as u8 } else if depth == 8 { n as u8 } else { (n as f64*(255.0/((1usize << depth)-1) as f64)) as u8 } };
    offset = 0;
    for &[px,py,dx,dy] in passes {
        let pass_width = width.saturating_sub(px).div_ceil(dx); let pass_height = height.saturating_sub(py).div_ceil(dy); let row_bytes = (pass_width*channels*depth).div_ceil(8);
        let mut previous = vec![0;row_bytes]; let mut row = vec![0;row_bytes];
        if pass_width != 0 { for y in 0..pass_height {
            unfilter(&inflated,&mut offset,&mut row,&previous,bpp)?;
            for x in 0..pass_width {
                let sample = x*channels; let pixel = (py+y*dy)*width+px+x*dx;
                if legacy { let target = pixel*channels*2; storage[target..target+channels*2].copy_from_slice(&row[sample*2..sample*2+channels*2]); continue; }
                let (red,green,blue,alpha) = if kind == 3 {
                    let color = sample_at(&row,sample,depth) as usize; let palette = palette.unwrap();
                    if color*3+2 >= palette.len() { return fail("PNG palette index is outside its color table."); }
                    (palette[color*3],palette[color*3+1],palette[color*3+2],transparency.and_then(|t| t.get(color)).copied().unwrap_or(255))
                } else if kind == 0 || kind == 4 { let gray = scale(sample_at(&row,sample,depth)); (gray,gray,gray,if kind == 4 { scale(sample_at(&row,sample+1,depth)) } else { 255 }) }
                else { (scale(sample_at(&row,sample,depth)),scale(sample_at(&row,sample+1,depth)),scale(sample_at(&row,sample+2,depth)),if kind == 6 { scale(sample_at(&row,sample+3,depth)) } else { 255 }) };
                bgra[pixel*4..pixel*4+4].copy_from_slice(&[blue,green,red,alpha]);
            }
            std::mem::swap(&mut row,&mut previous);
        } }
        if legacy { import_legacy_rows(&mut storage,&mut bgra,width,height,kind,transparency); }
    }
    if let Some(t) = transparency { if kind == 0 || kind == 2 {
        let keys = if kind == 0 { [if depth < 8 { short(t,0) as f64*255.0/((1usize << depth)-1) as f64 } else { (short(t,0)&255) as f64 };3] }
            else { [(short(t,0)&255) as f64,(short(t,2)&255) as f64,(short(t,4)&255) as f64] };
        for y in 0..height { bgra[y*width*4+3] = 255; for x in 0..width { let p = (y*width+x)*4; if bgra[p+2] as f64 == keys[0] && bgra[p+1] as f64 == keys[1] && bgra[p] as f64 == keys[2] { bgra[p+3] = 0; } } }
    } }
    Ok(DecodedImage { width,height,bgra,has_alpha:kind == 4 || kind == 6 || transparency.is_some(),format:"png" })
}

pub fn decode_image(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    if encoded.starts_with(PNG_SIGNATURE) { return decode_png(encoded); }
    if encoded.len() >= 2 && [*b"BM",*b"BA",*b"CI"].contains(&ascii_pair(encoded,0)) { return decode_bmp(encoded); }
    if encoded.starts_with(&[255,216]) { return decode_jpeg(encoded); }
    if encoded.starts_with(b"GIF87a") || encoded.starts_with(b"GIF89a") { return decode_gif(encoded); }
    if encoded.len() >= 4 && [*b"II*\0",*b"MM\0*",*b"II+\0",*b"MM\0+"].contains(&encoded[..4].try_into().unwrap()) { return decode_tiff(encoded); }
    fail("Unsupported image format. PNG, BMP, JPEG, GIF and TIFF are currently implemented.")
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilename { pub extension: String, pub shadow_control: i32 }
pub fn resolve_image_filename(filename: &str) -> ImageFilename {
    let separator = filename.rfind('/').or_else(|| filename.rfind('\\'));
    let mut extension = filename.rfind('.').filter(|&dot| separator.is_none_or(|separator| dot >= separator)).map(|dot| &filename[dot..]).unwrap_or("");
    if extension == ".jpg" { extension = ".jpeg"; } else if extension == ".tif" { extension = ".tiff"; }
    let shadow_control = match extension { ".gif" => 0, ".jpeg" => 1, ".png" => 2, _ => -1 };
    ImageFilename { extension: extension.into(), shadow_control }
}
