//! Strict native comparisons for defined behavior, and sample-domain
//! correctness oracles for the approved 16-bit RGB/RGBA Adam7 contract.
mod common;
use common::{b64,read_bytes,read_json,read_ndjson};
use openvector::image_input::{decode_image,resolve_image_filename};
use openvector::image_pipeline::prepare_image_state;
use openvector::jpeg_input::decode_jpeg_pixels;
use openvector::tiff_input::decode_tiff_pixels;
use serde_json::{json,Value};

fn n(v: &Value, key: &str) -> usize { v[key].as_u64().unwrap_or_else(|| panic!("missing {key}")) as usize }
fn pixels(actual: &[u8], expected: &Value, label: &str) {
    let expected = b64(expected.as_str().unwrap());
    assert_eq!(actual.len(),expected.len(),"{label} byte count");
    if let Some(index) = actual.iter().zip(&expected).position(|(a,b)| a != b) { panic!("{label}: byte {index}: actual {}, native {}",actual[index],expected[index]); }
}
fn image(actual: &openvector::image_resize::OwnedImage, expected: &Value, label: &str) {
    assert_eq!((actual.width,actual.height),(n(expected,"width"),n(expected,"height")),"{label} dimensions"); pixels(&actual.bgra,&expected["bgra"],label);
}
fn native_input(corpus: &str, preparation: bool) {
    native_input_rows(corpus, read_ndjson(&format!("{corpus}-input-reference.ndjson.gz")), preparation);
}

fn native_input_rows(corpus: &str, rows: Vec<Value>, preparation: bool) {
    let metadata = &rows[0];
    assert_eq!(metadata["wholeNativeResponseRepeatIdentical"],true);
    assert_eq!(rows.len()-1,n(metadata,"cases"));
    if corpus == "image" {
        assert_eq!(rows.len()-1,337);
        assert_eq!(rows[1..].iter().filter(|frame| correct_adam7(&b64(frame["encoded"].as_str().unwrap()))).count(), 16);
    }
    for frame in &rows[1..] { native_frame(corpus, frame, preparation); }
    if corpus == "image" {
        assert_eq!(rows[1..].iter().filter(|r| n(&r["decoded"],"matte") == 0).count(),138);
        assert_eq!(rows[1..].iter().filter(|r| n(&r["decoded"],"matte") == 1).count(),199);
    }
}

fn native_frame(corpus: &str, frame: &Value, preparation: bool) {
    let id = frame["id"].as_str().unwrap(); let encoded = b64(frame["encoded"].as_str().unwrap()); let before = encoded.clone();
    let decoded = decode_image(&encoded).unwrap_or_else(|e| panic!("{corpus}/{id}: {e}"));
    let expected = &frame["decoded"];
    assert_eq!(decoded.format,frame["format"].as_str().unwrap(),"{id} format");
    assert_eq!((decoded.width,decoded.height,decoded.has_alpha),(n(expected,"width"),n(expected,"height"),n(expected,"matte") != 0),"{id} dimensions/alpha");
    if correct_adam7(&encoded) {
        let expected = adam7_sample_pixels(frame, &encoded);
        assert_eq!(decoded.bgra, expected, "{id} sample-domain decode");
        if preparation { adam7_preparation(frame, &decoded, &expected); }
        assert_eq!(encoded, before, "{id} input ownership");
        return;
    }
    if !preparation {
        if decoded.format == "jpeg" { let raw = decode_jpeg_pixels(&encoded).unwrap(); assert_eq!(raw.color_space as usize,n(expected,"colorSpace"),"{id} colorspace"); pixels(&raw.bgra,&expected["bgra"],&format!("{id} raw JPEG")); }
        else if decoded.format == "tiff" { let raw = decode_tiff_pixels(&encoded).unwrap(); assert_eq!(raw.color_space as usize,n(expected,"colorSpace"),"{id} colorspace"); assert_eq!(raw.has_alpha,decoded.has_alpha,"{id} raw matte"); pixels(&raw.bgra,&expected["bgra"],&format!("{id} raw TIFF")); }
        else { pixels(&decoded.bgra,&expected["bgra"],&format!("{id} decoded")); }
    } else {
        let parameters = json!({"Shared::is_anti_aliased":frame["antiAlias"]});
        let initial = prepare_image_state(decoded.width,decoded.height,&decoded.bgra,decoded.has_alpha,0,0,&parameters).unwrap_or_else(|e| panic!("{id} initial: {e}"));
        image(&initial.source,&frame["source"],&format!("{id} source"));
        let prepared = prepare_image_state(decoded.width,decoded.height,&decoded.bgra,decoded.has_alpha,n(frame,"mode") as u32,n(frame,"amount") as u32,&parameters).unwrap_or_else(|e| panic!("{id} prepare: {e}"));
        image(&prepared.source,&frame["prepared"],&format!("{id} prepared")); image(&prepared.working,&frame["working"],&format!("{id} working"));
    }
    assert_eq!(encoded,before,"{id} input ownership");
}
macro_rules! corpus { ($name:ident,$fixture:literal) => { mod $name { #[test] fn native_pixels() { super::native_input($fixture,false); } #[test] fn native_preparation() { super::native_input($fixture,true); } } }; }
corpus!(image,"image");
corpus!(bmp_layout,"bmp-layout");
corpus!(bmp_bitfields,"bmp-bitfields");
corpus!(bmp_short_matrix,"bmp-short-matrix");
corpus!(bmp_header_variants,"bmp-header-variants");
corpus!(bmp_rle,"bmp-rle");
corpus!(bmp_rle_controls,"bmp-rle-controls");
corpus!(bmp_palettes,"bmp-palettes");
corpus!(bmp_wrappers,"bmp-wrappers");
corpus!(bmp_ignored_masks,"bmp-ignored-masks");
corpus!(gif,"gif");
corpus!(gif_opaque_gray,"gif-opaque-gray");
corpus!(gif_structure,"gif-structure-verified");
corpus!(jpeg,"jpeg");
corpus!(jpeg_expanded,"jpeg-expanded-verified");
corpus!(jpeg_markers,"jpeg-markers");
corpus!(tiff,"tiff");
corpus!(tiff_extended,"tiff-extended");
corpus!(tiff_quantum,"tiff-quantum-matrix");
corpus!(tiff_layout,"tiff-layout-success");
corpus!(tiff_tiles,"tiff-tile-variants");
corpus!(tiff_gray_tile_stride,"tiff-gray-tile-stride");
corpus!(tiff_sample_format,"tiff-sample-format");
corpus!(tiff_fax,"tiff-fax");
corpus!(tiff_storage_order,"tiff-storage-order");
corpus!(tiff_jpeg,"tiff-jpeg");
corpus!(tiff_jpeg_strips,"tiff-jpeg-strips");
corpus!(tiff_jpeg_layout,"tiff-jpeg-layout-success");
corpus!(tiff_jpeg_samples,"tiff-jpeg-samples");
corpus!(tiff_jpeg_single_row,"tiff-jpeg-single-row");
corpus!(tiff_jpeg_gray_alpha,"tiff-jpeg-gray-alpha");
corpus!(jpeg_alpha_modes,"jpeg-alpha-modes");
corpus!(tiff_legacy,"tiff-legacy-codecs");
corpus!(tiff_legacy_layout,"tiff-legacy-layout");
corpus!(tiff_thunder_commands,"tiff-thunder-commands");
corpus!(tiff_thunder_retained,"tiff-thunder-retained-row");
corpus!(tiff_thunder_initial,"tiff-thunder-initial-row");
corpus!(tiff_legacy_framing,"tiff-legacy-row-framing");
corpus!(tiff_predictor3,"tiff-predictor3");
corpus!(tiff_predictor3_layout,"tiff-predictor3-layout");
corpus!(tiff_predictor3_fill,"tiff-predictor3-fill-verified");
corpus!(tiff_fill_codecs,"tiff-fill-order-codecs-verified");
corpus!(tiff_predictor3_boundaries,"tiff-predictor3-boundaries");
corpus!(tiff_predictor3_white,"tiff-predictor3-white");
corpus!(tiff_gray_padding,"tiff-gray-padding");
corpus!(tiff_gray_padding32,"tiff-gray-padding32");
corpus!(tiff_gray_three,"tiff-gray-three");
corpus!(tiff_gray_planar_predictor3,"tiff-gray-planar-predictor3");
corpus!(tiff_packbits_tiles,"tiff-packbits-tile-framing-verified");

fn fresh_native_input(preparation: bool) {
    let rows = read_ndjson("fresh-image-input-20261020.ndjson.gz");
    assert_eq!(rows.len(), 121);
    native_input_rows("fresh-image-20261020", rows, preparation);
}

#[test]
fn fresh_native_pixels() { fresh_native_input(false); }

#[test]
fn fresh_native_preparation() { fresh_native_input(true); }

// Fresh seeded BMP/GIF/JPEG/TIFF inputs (evidence/fresh-multiformat-20261020).
// Every case ran in its own two original processes with byte-identical
// responses, so one test per case keeps a mismatch from hiding later cases.
const FRESH_MULTIFORMAT: &str = "fresh-multiformat-20261020";
fn fresh_multiformat(id: &str, preparation: bool) {
    let rows = read_ndjson(&format!("{FRESH_MULTIFORMAT}.ndjson.gz"));
    let id = format!("{FRESH_MULTIFORMAT}-{id}");
    let frame = rows[1..].iter().find(|row| row["id"] == id.as_str()).unwrap_or_else(|| panic!("{id} missing"));
    native_frame(FRESH_MULTIFORMAT, frame, preparation);
}
macro_rules! fresh_multiformat {
    ($($name:ident = $id:literal),* $(,)?) => {
        mod fresh_multiformat {
            $(mod $name { #[test] fn native_pixels() { super::super::fresh_multiformat($id,false); } #[test] fn native_preparation() { super::super::fresh_multiformat($id,true); } })*
        }
    };
}
fresh_multiformat! {
    bmp1_bottom = "00-bmp1-bottom-13x7", bmp4_top_partial_palette = "01-bmp4-top-partial-5x9", bmp8_bottom = "02-bmp8-bottom-31x3",
    bmp16_top = "03-bmp16-top-7x5", bmp24_top_narrow = "04-bmp24-top-1x17", bmp32_filler = "05-bmp32-bottom-filler-19x2",
    gif2_single_row = "06-gif2-9x1", gif16_transparent = "07-gif16-transparent-11x13", gif256_lzw_clear = "08-gif256-saturated-96x64",
    gif_first_frame_rect = "09-gif-two-frame-12x10", gif_local_table_transparent = "10-gif-local-transparent-7x29", gif_interlaced = "11-gif-interlaced-5x11",
    jpeg_gray = "12-jpeg-gray-17x9", jpeg_gray_progressive_narrow = "13-jpeg-gray-progressive-1x31", jpeg_444_optimized = "14-jpeg-444-optimized-13x11",
    jpeg_422 = "15-jpeg-422-21x7", jpeg_420_progressive = "16-jpeg-420-progressive-33x25", jpeg_420_single_row = "17-jpeg-420-3x1",
    jpeg_cmyk = "18-jpeg-cmyk-15x10", tiff_le_rgb_strips = "19-tiff-le-rgb8-strips-7x3", tiff_be_gray16 = "20-tiff-be-gray16-5x7",
    tiff_le_miniswhite_packbits = "21-tiff-le-miniswhite1-packbits-19x5", tiff_be_lzw_predictor = "22-tiff-be-rgb8-lzw-predictor-11x9",
    tiff_le_rgba_deflate = "23-tiff-le-rgba8-deflate-9x6", tiff_be_palette4 = "24-tiff-be-palette4-13x4", tiff_le_gray4_lzw = "25-tiff-le-gray4-lzw-3x17",
    tiff_be_packbits_narrow = "26-tiff-be-gray8-packbits-1x23", tiff_le_lzw_clear = "27-tiff-le-gray8-lzw-clear-160x96",
}

// This class replaces the original scratch-memory output, including bytes
// that happened to repeat in finite native observations. No masks apply.
fn correct_adam7(encoded: &[u8]) -> bool {
    encoded.starts_with(b"\x89PNG\r\n\x1a\n") && encoded[24] == 16
        && matches!(encoded[25], 2 | 6) && encoded[28] == 1
}

fn adam7_sample_pixels(frame: &Value, encoded: &[u8]) -> Vec<u8> {
    assert!(correct_adam7(encoded));
    let channels = if encoded[25] == 2 { 3 } else { 4 };
    let samples = frame["writer"]["samples"].as_array().expect("recorded writer samples");
    assert_eq!(samples.len(), n(frame, "width") * n(frame, "height") * channels);
    let mut key = None;
    let mut offset = 8;
    while offset < encoded.len() {
        let length = u32::from_be_bytes(encoded[offset..offset+4].try_into().unwrap()) as usize;
        if &encoded[offset+4..offset+8] == b"tRNS" {
            key = Some(std::array::from_fn::<_, 3, _>(|channel| {
                u16::from_be_bytes(encoded[offset+8+channel*2..offset+10+channel*2].try_into().unwrap()) as u64
            }));
        }
        offset += length + 12;
    }
    samples.chunks_exact(channels).flat_map(|pixel| {
        let rgb = [pixel[0].as_u64().unwrap(), pixel[1].as_u64().unwrap(), pixel[2].as_u64().unwrap()];
        let alpha = if channels == 4 { (pixel[3].as_u64().unwrap() >> 8) as u8 }
            else if key == Some(rgb) { 0 } else { 255 };
        [(rgb[2] >> 8) as u8, (rgb[1] >> 8) as u8, (rgb[0] >> 8) as u8, alpha]
    }).collect()
}

fn adam7_preparation(frame: &Value, decoded: &openvector::pixel_input::DecodedImage, expected: &[u8]) {
    // Every archived affected case uses mode zero and is too small to resize.
    // Compute the complete prepared buffer from authored samples, not decoder output.
    assert_eq!((n(frame, "mode"), n(frame, "amount")), (0, 0));
    let expected: Vec<u8> = expected.chunks_exact(4).flat_map(|pixel| {
        let alpha = pixel[3] as f64 * (1.0 / 255.0);
        [(pixel[0] as f64 * alpha) as u8, (pixel[1] as f64 * alpha) as u8,
            (pixel[2] as f64 * alpha) as u8, pixel[3]]
    }).collect();
    for anti_alias in [0, 1] {
        let prepared = prepare_image_state(decoded.width, decoded.height, &decoded.bgra,
            decoded.has_alpha, 0, 0, &json!({"Shared::is_anti_aliased":anti_alias})).unwrap();
        for actual in [&prepared.source, &prepared.working] {
            assert_eq!((actual.width, actual.height), (decoded.width, decoded.height));
            assert_eq!(actual.bgra, expected, "{} prepared samples", frame["id"]);
        }
    }
}

#[test]
fn authored_adam7_pixels_and_preparation() {
    let fixture = read_json("png-adam7-correctness.json");
    for frame in fixture["cases"].as_array().unwrap() {
        let encoded = b64(frame["encoded"].as_str().unwrap());
        let before = encoded.clone();
        let mut decoded = decode_image(&encoded).unwrap();
        assert_eq!((decoded.width, decoded.height), (n(frame, "width"), n(frame, "height")));
        assert_eq!(decoded.has_alpha, n(&frame["writer"], "colorType") == 6 || frame["writer"]["transparentKey"] == true);
        let expected = adam7_sample_pixels(frame, &encoded);
        assert_eq!(decoded.bgra, expected, "{} authored samples", frame["id"]);
        adam7_preparation(frame, &decoded, &expected);
        decoded.bgra.fill(0);
        assert_eq!(encoded, before);
        assert_eq!(decode_image(&encoded).unwrap().bgra, expected);
    }
}

fn failures(corpus: &str, count: usize, message: impl Fn(&str,&str)->bool, heap_failure: bool) {
    let rows = read_ndjson(&format!("{corpus}-errors-reference.ndjson.gz"));
    assert_eq!(n(&rows[0],"cases"),count); assert_eq!(rows.len()-1,count);
    for frame in &rows[1..] {
        let id = frame["id"].as_str().unwrap(); let runs = frame["expectedFailures"].as_array().unwrap(); assert_eq!(runs.len(),2,"{id}");
        for run in runs {
            if heap_failure { assert!(n(run,"exitCode") == 0xc0000374 || n(run,"exitCode") == 11 && n(run,"stage") == 921 && run["exceptionCode"] == "0xc0000005","{id} native heap failure"); }
            else { assert_eq!(n(run,"exitCode"),11,"{id}"); assert_eq!(n(run,"stage"),921,"{id}"); assert_eq!(run["exceptionCode"],"0xe06d7363","{id}"); }
        }
        let encoded = b64(frame["encoded"].as_str().unwrap()); let before = encoded.clone(); let error = decode_image(&encoded).unwrap_err();
        assert!(message(id,error.message()),"{id}: {error}"); assert_eq!(encoded,before,"{id} ownership");
    }
}
#[test] fn gif_interlace_errors() { failures("gif-interlace",3,|_,e| e == "GIF interlace selects a row outside the image.",false); }
#[test] fn tiff_planar_errors() { failures("tiff-planar",3,|_,e| e.contains("compressed planar strips"),false); }
#[test] fn tiff_jpeg_heap_errors() { failures("tiff-jpeg-process",2,|_,e| e.contains("generic reader overruns its quantum buffer"),true); }
#[test] fn tiff_jpeg_marker_errors() { failures("tiff-jpeg-malformed",2,|_,e| e.starts_with("TIFF JPEG ") && e.contains("marker"),false); }
#[test] fn tiff_old_jpeg_errors() { failures("tiff-old-jpeg-unavailable",18,|_,e| e == "Unsupported TIFF compression.",false); }
#[test] fn tiff_legacy_command_errors() { failures("tiff-legacy-row-framing",4,|_,e| (e.starts_with("TIFF NeXT") || e.starts_with("TIFF ThunderScan")) && (e.contains("incomplete") || e.contains("exceeds")),false); }
fn predictor_reason(id: &str,e: &str) -> bool {
    if id.contains("-tile-") { e.contains("tile delegate") && e.contains("sample depth") }
    else if id.contains("-planar-rows-") { e.contains("compressed planar strips") }
    else if id.contains("-unregistered-") { e.contains("predictor tag") && e.contains("not registered") }
    else if id.starts_with("tiff-predictor2-") { e.contains("horizontal predictor sample type") }
    else { e.contains("floating predictor sample type") }
}
#[test] fn tiff_predictor_errors() { failures("tiff-predictor3",70,predictor_reason,false); }
#[test] fn tiff_predictor_registration_errors() { failures("tiff-predictor-packbits-registration",16,predictor_reason,false); }
#[test] fn tiff_fill_encoding_errors() { failures("tiff-predictor3-fill-encoding",8,|_,_| true,false); }
#[test] fn tiff_packbits_row_errors() { failures("tiff-packbits-row-boundary",8,|_,e| e == "TIFF PackBits packet crosses an image row or block boundary.",false); }
#[test] fn standalone_two_component_jpeg_is_not_tiff_alpha() { assert_eq!(decode_image(&read_bytes("jpeg-two-component-input.jpg")).unwrap_err().message(),"Unsupported JPEG component count."); }
#[test] fn native_filename_metadata() {
    let v = read_json("image-filename-reference.json"); assert_eq!(v["wholeFreshProcessResponseRepeatIdentical"],true); let cases = v["cases"].as_array().unwrap(); assert_eq!(cases.len(),15);
    for c in cases { let filename = c["filename"].as_str().unwrap(); let actual = resolve_image_filename(filename); assert_eq!(actual.extension,c["extension"].as_str().unwrap(),"{filename}"); assert_eq!(actual.shadow_control as i64,c["shadowControl"].as_i64().unwrap(),"{filename}"); }
}
#[test] fn decoders_preserve_offset_views_and_own_storage() {
    for (fixture,short,missing) in [("bmp-layout",vec![4,14,25],1),("gif",vec![6,12],4),("jpeg",vec![2,4,12],8),("tiff",vec![4,8],4)] {
        let rows = read_ndjson(&format!("{fixture}-input-reference.ndjson.gz")); let encoded = b64(rows[1]["encoded"].as_str().unwrap());
        let mut container = vec![1,2]; container.extend_from_slice(&encoded); container.push(3); let view = &container[2..container.len()-1];
        let mut first = decode_image(view).unwrap(); let second = decode_image(view).unwrap(); first.bgra.fill(0); assert_ne!(first.bgra,second.bgra,"{fixture}"); assert_eq!(view,&encoded,"{fixture}");
        for length in short.into_iter().chain(std::iter::once(encoded.len()-missing)) { assert!(decode_image(&encoded[..length]).is_err(),"{fixture} truncated {length}"); }
        assert_eq!(view,&encoded,"{fixture}");
    }
}
#[test] fn unsupported_signatures_fail_explicitly() {
    for input in [&[][..],b"not an image",&[137,80,78,71]] { assert_eq!(decode_image(input).unwrap_err().message(),"Unsupported image format. PNG, BMP, JPEG, GIF and TIFF are currently implemented."); }
}
