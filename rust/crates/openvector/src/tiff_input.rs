//! Classic TIFF directory, bounded block decoding, and original Q8 import.
use std::collections::BTreeMap;
use std::io::Read;
use crate::jpeg_input::{read_jpeg_samples, convert_jpeg_ycbcr};
use crate::pixel_input::{fail, normalize_cmyk_pixels, DecodedImage, InputError};
use crate::tiff_fax::decode_fax;
use crate::tiff_legacy_codecs::{decode_next, decode_thunder};

struct Directory<'a> { data: &'a [u8], little: bool, fields: BTreeMap<u16, Vec<f64>> }
fn bounded(data: &[u8], offset: usize, count: usize) -> Result<&[u8], InputError> {
    let end = offset.checked_add(count).filter(|&n| n <= data.len()).ok_or_else(|| InputError::new("TIFF field data is incomplete."))?;
    Ok(&data[offset..end])
}
fn uint(data: &[u8], offset: usize, size: usize, little: bool) -> Result<u64, InputError> {
    let bytes = bounded(data,offset,size)?;
    Ok(if little { bytes.iter().rev().fold(0u64, |n,&b| (n << 8) | b as u64) } else { bytes.iter().fold(0u64, |n,&b| (n << 8) | b as u64) })
}
impl<'a> Directory<'a> {
    fn read(data: &'a [u8]) -> Result<Self, InputError> {
        bounded(data,0,8)?; let little = data[0] == 0x49;
        if uint(data,2,2,little)? != 42 { return fail("Only classic TIFF directories are currently implemented."); }
        let directory = uint(data,4,4,little)? as usize; let count = uint(data,directory,2,little)? as usize;
        bounded(data,directory+2,count*12+4)?;
        let mut fields = BTreeMap::new();
        for index in 0..count {
            let entry = directory+2+index*12; let tag = uint(data,entry,2,little)? as u16; let kind = uint(data,entry+2,2,little)? as usize;
            let items = uint(data,entry+4,4,little)? as usize;
            let size = match kind { 1|2|6|7 => 1, 3|8 => 2, 4|9|11 => 4, 5|10|12 => 8, _ => return fail("Unsupported TIFF field type.") };
            let bytes = items*size; let offset = if bytes <= 4 { entry+8 } else { uint(data,entry+8,4,little)? as usize };
            bounded(data,offset,bytes)?; let mut values = Vec::with_capacity(items);
            for item in 0..items {
                let at = offset+item*size; let n = uint(data,at,size,little)?;
                values.push(match kind {
                    6 => n as i8 as f64, 8 => n as i16 as f64, 9 => n as i32 as f64,
                    11 => f32::from_bits(n as u32) as f64, 12 => f64::from_bits(n),
                    5 => uint(data,at,4,little)? as f64 / uint(data,at+4,4,little)? as f64,
                    10 => uint(data,at,4,little)? as i32 as f64 / uint(data,at+4,4,little)? as i32 as f64,
                    _ => n as f64,
                });
            }
            fields.insert(tag,values);
        }
        Ok(Self { data, little, fields })
    }
    fn value(&self, tag: u16, fallback: f64) -> f64 { self.fields.get(&tag).and_then(|v| v.first()).copied().unwrap_or(fallback) }
    fn n(&self, tag: u16, fallback: usize) -> usize { self.value(tag,fallback as f64) as usize }
}
fn decode_lzw(encoded: &[u8], count: usize) -> Result<Vec<u8>, InputError> {
    let mut prefix = [0usize;4096]; let mut suffix = [0u8;4096]; let mut stack = [0u8;4096];
    for (i,b) in suffix.iter_mut().enumerate().take(256) { *b = i as u8; }
    let (mut next,mut bits,mut bit) = (258usize,9usize,0usize); let mut previous = None;
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        if bit+bits > encoded.len()*8 { return fail("TIFF LZW pixels are incomplete."); }
        let mut code = 0usize; for _ in 0..bits { code = code*2 + ((encoded[bit/8] >> (7-(bit&7))) & 1) as usize; bit += 1; }
        if code == 256 { next = 258; bits = 9; previous = None; continue; }
        if code == 257 { return fail("TIFF LZW ended before all strip pixels."); }
        let special = code == next && previous.is_some();
        if !(code < next || special) { return fail("TIFF LZW code is outside the dictionary."); }
        let mut cursor = if special { previous.unwrap() } else { code }; let mut size = 0;
        while cursor >= 258 { stack[size] = suffix[cursor]; size += 1; cursor = prefix[cursor]; }
        if cursor >= 256 { return fail("TIFF LZW code is outside the dictionary."); }
        stack[size] = suffix[cursor]; size += 1; let first = stack[size-1];
        if size+usize::from(special) > count-out.len() { return fail("TIFF LZW phrase exceeds the strip."); }
        out.extend(stack[..size].iter().rev()); if special { out.push(first); }
        if let Some(prev) = previous { if next < 4096 { prefix[next] = prev; suffix[next] = first; next += 1; if next == (1 << bits)-1 && bits < 12 { bits += 1; } } }
        previous = Some(code);
    }
    Ok(out)
}
struct Layout { width: usize, rows: usize, fill_order: u32, t4_options: u32, read_unit_bytes: usize }
fn decode_strip(encoded: &[u8], compression: usize, count: usize, layout: &Layout, scanline: &mut [u8]) -> Result<Vec<u8>, InputError> {
    match compression {
        32766 => return decode_next(encoded,layout.width,layout.rows),
        32809 => return decode_thunder(encoded,layout.width,layout.rows,scanline),
        2|3|4|32771 => return decode_fax(encoded,layout.width,layout.rows,compression as u32,layout.fill_order,layout.t4_options),
        1 => { if encoded.len() < count { return fail("TIFF uncompressed strip is incomplete."); } return Ok(encoded[..count].to_vec()); }
        5 => return decode_lzw(encoded,count),
        8|32946 => {
            let mut out = Vec::new(); flate2::read::ZlibDecoder::new(encoded).read_to_end(&mut out).map_err(|e| InputError::new(e.to_string()))?;
            if out.len() < count { return fail("TIFF deflate strip is incomplete."); } out.truncate(count); return Ok(out);
        }
        32773 => {},
        _ => return fail("Unsupported TIFF compression."),
    }
    let mut out = vec![0;count]; let (mut read,mut written) = (0usize,0usize);
    while written < count {
        let control = *encoded.get(read).ok_or_else(|| InputError::new("TIFF PackBits strip is incomplete."))?; read += 1;
        if control == 128 { continue; } let length = if control < 128 { control as usize+1 } else { 257-control as usize };
        let end = count.min((written/layout.read_unit_bytes+1)*layout.read_unit_bytes);
        if written+length > end { return fail("TIFF PackBits packet crosses an image row or block boundary."); }
        if control < 128 { if read+length > encoded.len() { return fail("TIFF PackBits literal is incomplete."); } out[written..written+length].copy_from_slice(&encoded[read..read+length]); read += length; }
        else { let value = *encoded.get(read).ok_or_else(|| InputError::new("TIFF PackBits repeat byte is missing."))?; read += 1; out[written..written+length].fill(value); }
        written += length;
    }
    Ok(out)
}
fn decode_jpeg_block(encoded: &[u8], tables: Option<&[u8]>, columns: usize, rows: usize, samples: usize, photometric: usize) -> Result<Vec<u8>, InputError> {
    if !encoded.starts_with(&[255,216]) { return fail("TIFF JPEG block requires an SOI marker."); }
    let mut merged = Vec::new();
    let encoded = if let Some(tables) = tables { merged.reserve(tables.len()+encoded.len()-4); merged.extend_from_slice(&tables[..tables.len()-2]); merged.extend_from_slice(&encoded[2..]); &merged[..] } else { encoded };
    let mut decoded = read_jpeg_samples(encoded)?;
    if decoded.width != columns || decoded.height != rows || decoded.planes.len() != samples { return fail("TIFF JPEG frame dimensions or components disagree with its block."); }
    if photometric == 6 { convert_jpeg_ycbcr(&mut decoded.planes); }
    let mut out = vec![0;columns*rows*samples];
    for pixel in 0..columns*rows { for sample in 0..samples { out[pixel*samples+sample] = decoded.planes[sample][pixel]; } }
    Ok(out)
}
fn undo_horizontal(bytes: &mut [u8], columns: usize, rows: usize, depth: usize, samples: usize, little: bool) -> Result<(), InputError> {
    let size = depth/8; let stride = columns*samples*size;
    for row in 0..rows { for index in samples..columns*samples {
        let offset = row*stride+index*size; let sum = uint(bytes,offset,size,little)?.wrapping_add(uint(bytes,offset-samples*size,size,little)?);
        for byte in 0..size { bytes[offset+byte] = (sum >> (8*if little { byte } else { size-1-byte })) as u8; }
    } }
    Ok(())
}
fn undo_floating(bytes: &mut [u8], columns: usize, rows: usize, depth: usize, samples: usize) {
    let size = depth/8; let row_samples = columns*samples; let row_bytes = row_samples*size; let mut planes = vec![0;row_bytes];
    for row in 0..rows {
        let start = row*row_bytes;
        for index in samples..row_bytes { bytes[start+index] = bytes[start+index].wrapping_add(bytes[start+index-samples]); }
        planes.copy_from_slice(&bytes[start..start+row_bytes]);
        for sample in 0..row_samples { for byte in 0..size { bytes[start+sample*size+byte] = planes[(size-1-byte)*row_samples+sample]; } }
    }
}
fn js_u32(value: f64) -> u32 { if !value.is_finite() { 0 } else { value.trunc().rem_euclid(4294967296.0) as u32 } }
fn sample_quantum(number: f64, depth: usize, format: usize, gray16: bool) -> f64 {
    if gray16 { return (js_u32(number) >> 8) as f64; }
    if format != 3 && depth == 32 { return (js_u32(number+0x808080 as f64) / 0x1010101) as f64; }
    let scaled = number*255.0/if format == 3 { 1.0 } else { 2f64.powi(depth as i32)-1.0 };
    let rounded = ((if format == 3 { scaled as f32 as f64 } else { scaled })+0.5).floor();
    if rounded.is_nan() { rounded } else { rounded.clamp(0.0,255.0) }
}
enum Samples { Bytes(Vec<u8>), Indices(Vec<u32>), Floats(Vec<f32>) }
impl Samples {
    fn get(&self, index: usize) -> f64 { match self { Self::Bytes(v) => v[index] as f64, Self::Indices(v) => v[index] as f64, Self::Floats(v) => v[index] as f64 } }
    fn set(&mut self, index: usize, n: f64) { match self { Self::Bytes(v) => v[index] = js_u32(n) as u8, Self::Indices(v) => v[index] = js_u32(n), Self::Floats(v) => v[index] = n as f32 } }
}
struct ImageLayout { width: usize, height: usize, samples: usize, depth: usize, format: usize, has_alpha: bool }
fn read_samples(d: &Directory<'_>, image: &ImageLayout) -> Result<(Samples,bool), InputError> {
    let ImageLayout { width, height, samples, depth, format, has_alpha } = *image;
    let tiled = d.fields.contains_key(&322); let planar = d.n(284,1); let compression = d.n(259,1); let predictor = d.n(317,1);
    if compression == 7 && depth != 8 { return fail("TIFF JPEG requires eight-bit samples."); }
    if d.fields.contains_key(&317) && ![5,8,32946].contains(&compression) { return fail("TIFF predictor tag is not registered for this compression."); }
    if ![1,2].contains(&planar) || ![1,2,3].contains(&predictor) { return fail("Unsupported TIFF planar configuration or predictor."); }
    if predictor == 2 && ![8,16].contains(&depth) { return fail("Unsupported TIFF horizontal predictor sample type."); }
    if predictor == 3 && (format != 3 || ![32,64].contains(&depth)) { return fail("Unsupported TIFF floating predictor sample type."); }
    if tiled && ![1,2,4,8,16].contains(&depth) { return fail("Original TIFF tile delegate does not support this sample depth."); }
    let columns = if tiled { d.n(322,0) } else { width }; let block_height = if tiled { d.n(323,0) } else { d.n(278,height) };
    if columns == 0 || block_height == 0 { return fail("TIFF block dimensions must be positive."); }
    let photometric = d.n(262,1); let single_sample = !tiled && [0,1,3].contains(&photometric);
    if !tiled && !single_sample && planar == 2 && samples > 1 && compression != 1 && block_height > 1 && height > 1 { return fail("TIFF compressed planar strips do not support native row access."); }
    let across = width.div_ceil(columns); let down = height.div_ceil(block_height); let planes = if planar == 2 { samples } else { 1 }; let blocks = across*down;
    let offsets = d.fields.get(&if tiled { 324 } else { 273 }); let counts = d.fields.get(&if tiled { 325 } else { 279 });
    let (offsets,counts) = match (offsets,counts) { (Some(o),Some(c)) if o.len() >= blocks*planes && c.len() >= blocks*planes => (o,c), _ => return fail("TIFF block directory is incomplete.") };
    let gray_scanline = !tiled && [0,1].contains(&photometric) && samples > 1 && depth >= 8; let gray_channels = if has_alpha { 2 } else { 1 };
    let jpeg_tables = if compression == 7 { d.fields.get(&347).map(|v| v.iter().map(|&x| js_u32(x) as u8).collect::<Vec<_>>()) } else { None };
    if jpeg_tables.as_ref().is_some_and(|v| v.len() < 4 || !v.starts_with(&[255,216]) || !v.ends_with(&[255,217])) { return fail("TIFF JPEG tables require SOI and EOI markers."); }
    let mut output = if photometric == 3 { Samples::Indices(vec![0;width*height*samples]) } else if format == 3 { Samples::Floats(vec![0.0;width*height*samples]) } else { Samples::Bytes(vec![0;width*height*samples]) };
    let block_samples = if planar == 2 { 1 } else { samples }; let orientation = d.n(274,1).wrapping_sub(1)&3; let delegate = tiled || photometric == 6;
    let fill_order = d.n(266,1);
    if ![1,2].contains(&fill_order) { return fail("Unsupported TIFF fill order."); }
    if [32766,32809].contains(&compression) && (samples != 1 || depth != if compression == 32766 { 2 } else { 4 }) { return fail("TIFF NeXT/ThunderScan requires one sample at its codec depth."); }
    let mut scanline = if compression == 32809 { vec![0;columns.div_ceil(2)] } else { Vec::new() };
    let fax = [2,3,4,32771].contains(&compression);
    if fax && (depth != 1 || samples != 1) { return fail("TIFF CCITT requires one bilevel sample per pixel."); }
    for plane in 0..if single_sample { 1 } else { planes } { for block in 0..blocks {
        let top = block/across*block_height; let left = block%across*columns;
        let rows = if tiled { block_height } else { block_height.min(height-top) }; let row_bytes = (columns*block_samples*depth).div_ceil(8); let index = plane*blocks+block;
        let encoded = bounded(d.data,offsets[index] as usize,counts[index] as usize)?;
        let reversed;
        let encoded = if fill_order == 2 && !fax && compression != 7 { reversed = encoded.iter().map(|b| b.reverse_bits()).collect::<Vec<_>>(); &reversed[..] } else { encoded };
        let mut bytes = if compression == 7 { decode_jpeg_block(encoded,jpeg_tables.as_deref(),columns,rows,block_samples,photometric)? }
            else { decode_strip(encoded,compression,rows*row_bytes,&Layout { width: columns, rows, fill_order: fill_order as u32, t4_options: d.n(292,0) as u32, read_unit_bytes: if tiled { rows*row_bytes } else { row_bytes } }, &mut scanline)? };
        if predictor == 2 { undo_horizontal(&mut bytes,columns,rows,depth,block_samples,d.little)?; }
        if predictor == 3 { undo_floating(&mut bytes,columns,rows,depth,block_samples); }
        if !d.little && depth >= 16 && predictor != 3 { for word in bytes.chunks_exact_mut(depth/8) { word.reverse(); } }
        let gray_stride = gray_channels*depth/8+samples-gray_channels;
        let mut gray_row = if gray_scanline { vec![0;row_bytes.max(width*gray_stride)] } else { Vec::new() };
        let visible_columns = columns.min(width-left); let visible_rows = block_height.min(height-top);
        let stride = if tiled && depth == 16 && samples == 1 && [0,1].contains(&photometric) { visible_columns*2+(columns-visible_columns) } else { row_bytes };
        let flip_x = delegate && (orientation == 1 || orientation == 2); let flip_y = delegate && (orientation == 2 || orientation == 3);
        let gray16 = tiled && depth == 16 && [0,1].contains(&photometric); let imported = if gray_scanline { gray_channels } else { block_samples };
        for row in 0..rows {
            if gray_scanline { gray_row[..row_bytes].copy_from_slice(&bytes[row*row_bytes..(row+1)*row_bytes]); }
            let view = if gray_scanline { &gray_row[..] } else { &bytes[..] };
            for index in 0..columns*imported {
                let bit = index*depth; let channel = index%imported; let column = index/imported;
                let offset = if gray_scanline { column*gray_stride+channel*depth/8 } else { row*stride+bit/8 };
                let sample = if depth < 8 { ((bytes[offset] >> (8-depth-bit%8)) & ((1 << depth)-1)) as f64 }
                    else if format == 3 { let n = uint(view,offset,depth/8,true)?; if depth == 32 { f32::from_bits(n as u32) as f64 } else { f64::from_bits(n) } }
                    else if [8,16,32].contains(&depth) { uint(view,offset,depth/8,true)? as f64 } else { return fail("Unsupported TIFF integer sample depth."); };
                let x = left as isize + if flip_x { visible_columns as isize-1-column as isize } else { column as isize };
                let y = top as isize + if flip_y { visible_rows as isize-1-row as isize } else { row as isize };
                let component = if planar == 2 && !gray_scanline { plane } else { channel };
                if x >= left as isize && y >= top as isize && x < width as isize && y < height as isize { output.set((y as usize*width+x as usize)*samples+component,if photometric == 3 && component == 0 { sample } else { sample_quantum(sample,depth,format,gray16) }); }
            }
        }
    } }
    Ok((output,tiled))
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiffPixels { pub width: usize, pub height: usize, pub bgra: Vec<u8>, pub has_alpha: bool, pub color_space: u8, pub black: Option<Vec<u8>> }
pub fn decode_tiff_pixels(encoded: &[u8]) -> Result<TiffPixels, InputError> {
    let d = Directory::read(encoded)?; let w = d.value(256,0.0); let h = d.value(257,0.0);
    if !w.is_finite() || !h.is_finite() || w.fract() != 0.0 || h.fract() != 0.0 || w < 1.0 || h < 1.0 || w*h > 2147483647.0 { return fail("TIFF requires positive dimensions with an int32 pixel count."); }
    let (width,height) = (w as usize,h as usize); let pixels = width*height; let samples = d.n(277,1);
    let depths = d.fields.get(&258).map(Vec::as_slice).unwrap_or(&[1.0]); let depth = depths.first().copied().unwrap_or(0.0) as usize; let format = d.n(339,1);
    if !depths.iter().all(|&v| v == depth as f64) || ![1,2,4,8,16,32,64].contains(&depth) { return fail("Unsupported TIFF sample depths."); }
    if ![1,2,3].contains(&format) || format == 3 && ![32,64].contains(&depth) { return fail("Unsupported TIFF sample format."); }
    let photometric = d.n(262,1); let cmyk = photometric == 5;
    if ![0,1,2,3,5].contains(&photometric) && !(photometric == 6 && d.n(259,1) == 7) { return fail("Unsupported TIFF photometric interpretation."); }
    if photometric == 6 && samples != 3 { return fail("TIFF JPEG YCbCr requires three samples."); }
    let generic = photometric == 6 && !d.fields.contains_key(&322);
    if generic && pixels*4 > width*5+1 { return fail("Original TIFF generic reader overruns its quantum buffer for this raster."); }
    let extras = d.fields.get(&338).map(Vec::as_slice).unwrap_or(&[]); let extra = extras.first().copied();
    let has_alpha = if !extras.is_empty() { samples > 3 || extra == Some(1.0) || extra == Some(2.0) } else { photometric == 2 && samples == 4 };
    let components = if cmyk { 4 } else if photometric == 2 || photometric == 6 { 3 } else { 1 };
    if samples < components+usize::from(has_alpha) { return fail("TIFF image has too few samples for its channels."); }
    let mut bgra = vec![0;pixels*4]; let mut black = if cmyk { Some(vec![0;pixels]) } else { None };
    let palette = d.fields.get(&320); let palette_colors = 2f64.powi(depth as i32);
    if photometric == 3 && palette.is_none_or(|p| p.len() as f64 != palette_colors*3.0) { return fail("TIFF palette is incomplete."); }
    let maximum = if palette.is_some_and(|p| p.iter().any(|&v| v > 255.0)) { 65535.0 } else { 255.0 };
    let (values,tiled) = read_samples(&d,&ImageLayout { width,height,samples,depth,format,has_alpha })?;
    let indexed_gray = !tiled && samples == 2 && depth <= 8;
    let invert = photometric == 0 && (tiled || !has_alpha && (depth == 1 || !indexed_gray && [8,16].contains(&depth)));
    for pixel in 0..pixels {
        let index = pixel*samples; let alpha = if has_alpha { values.get(index+components) } else { 255.0 };
        let mut rgb = if photometric == 3 {
            let color = values.get(index) as usize; let p = palette.unwrap(); let n = palette_colors as usize;
            std::array::from_fn(|channel| (p.get(color+channel*n).copied().unwrap_or(f64::NAN)*255.0/maximum+0.5).floor())
        } else if photometric == 2 || photometric == 6 || cmyk {
            if let Some(black) = &mut black { black[pixel] = js_u32(values.get(index+3)) as u8; }
            [values.get(index),values.get(index+1),values.get(index+2)]
        } else { [if invert { 255.0-values.get(index) } else { values.get(index) };3] };
        if tiled && cmyk {
            let black = black.as_mut().unwrap(); let light = 255.0-black[pixel] as f64;
            for value in &mut rgb { *value = ((255.0-*value)*light/255.0).floor(); } black[pixel] = 0;
        } else if tiled && has_alpha && extra == Some(2.0) { for value in &mut rgb { *value = ((*value*alpha+127.0)/255.0).floor(); } }
        else if !tiled && extra == Some(1.0) { let scale = if alpha != 0.0 && !alpha.is_nan() { 1.0/(alpha*(1.0/255.0)) } else { 1.0 }; for value in &mut rgb { let n = (scale * *value+0.5).floor(); *value = if n.is_nan() { n } else { n.min(255.0) }; } }
        bgra[pixel*4..pixel*4+4].copy_from_slice(&[js_u32(rgb[2]) as u8,js_u32(rgb[1]) as u8,js_u32(rgb[0]) as u8,js_u32(alpha) as u8]);
    }
    if generic {
        let mut rgba = vec![0;pixels*4]; for pixel in 0..pixels { let o = pixel*4; rgba[o..o+4].copy_from_slice(&[bgra[o+2],bgra[o+1],bgra[o],255]); }
        for pixel in 0..pixels { bgra[pixel*4..pixel*4+4].copy_from_slice(&[0,0,rgba[pixel],255]); }
    }
    Ok(TiffPixels { width,height,bgra,has_alpha,color_space: if cmyk { 12 } else { 1 },black })
}
pub fn decode_tiff(encoded: &[u8]) -> Result<DecodedImage, InputError> {
    let mut p = decode_tiff_pixels(encoded)?; if let Some(black) = &p.black { normalize_cmyk_pixels(&mut p.bgra,black); }
    Ok(DecodedImage { width:p.width,height:p.height,bgra:p.bgra,has_alpha:p.has_alpha,format:"tiff" })
}
