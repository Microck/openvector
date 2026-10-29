//! Byte-level codec checks and decoder round trips from `test/bitmap-codecs.test.mjs`.
//! Fitted-scene export tests live in `bitmap_export.rs`.
mod common;
use openvector::bitmap_codecs::{encode_bmp, encode_jpeg, encode_png, encode_tiff, BitmapError};
use openvector::image_input::decode_image;
use serde_json::Value;
use std::io::Read;

type Encoder = fn(i64, i64, &[u8], bool) -> Result<Vec<u8>, BitmapError>;

fn encoder(format: &str) -> Encoder {
    match format {
        "bmp" => |w, h, p, _| encode_bmp(w, h, p),
        "tif" => |w, h, p, _| encode_tiff(w, h, p),
        "png" => encode_png,
        "jpg" => |w, h, p, _| encode_jpeg(w, h, p),
        _ => unreachable!(),
    }
}

fn references() -> Vec<Value> {
    ["bitmap-codecs-reference", "bitmap-jpeg-reference"].iter()
        .flat_map(|name| common::read_ndjson(&format!("{name}.ndjson.gz"))).collect()
}

fn inflate(text: &str) -> Vec<u8> {
    let mut bgra = Vec::new();
    flate2::read::ZlibDecoder::new(&common::b64(text)[..]).read_to_end(&mut bgra).unwrap();
    bgra
}

fn check_bytes(format: &str, count: usize) {
    let encode = encoder(format);
    let mut seen = 0;
    let mut failures = Vec::new();
    for reference in references().iter().filter(|row| row["format"] == format) {
        seen += 1;
        let id = reference["id"].as_str().unwrap();
        let (width, height) = (reference["width"].as_i64().unwrap(), reference["height"].as_i64().unwrap());
        let has_alpha = reference["hasAlpha"].as_bool().unwrap_or(false);
        let bgra = inflate(reference["bgraZlib"].as_str().unwrap());
        let before = bgra.clone();
        let encoded = encode(width, height, &bgra, has_alpha).unwrap_or_else(|error| panic!("{id}: {error}"));
        if encoded != common::b64(reference["encoded"].as_str().unwrap()) {
            failures.push(format!("encoded bytes differ for {id} (alpha {has_alpha})"));
        }
        assert_eq!(bgra, before, "{id}");
        let decoded = decode_image(&encoded).unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!((decoded.width, decoded.height), (width as usize, height as usize), "{id}");
        if format != "jpg" {
            assert!(decoded.bgra == bgra, "{id}: decoded pixels differ");
        }
    }
    assert_eq!(seen, count, "{format} fixture count");
    assert!(failures.is_empty(), "{failures:?}");
}

fn check_rejections(format: &str) {
    let encode = encoder(format);
    assert!(encode(2, 1, &[0; 4], false).unwrap_err().0.contains("one BGRA pixel"));
    assert!(encode(0, 1, &[], false).unwrap_err().0.contains("dimensions must be positive"));
    // The JavaScript `channels must be integers` case (a 256 channel) cannot
    // be expressed: `&[u8]` input enforces byte channels at compile time.
}

#[test]
fn native_bitmap_bmp_bytes() { check_bytes("bmp", 2); }
#[test]
fn native_bitmap_tif_bytes() { check_bytes("tif", 2); }
#[test]
fn native_bitmap_png_bytes() { check_bytes("png", 9); }
#[test]
fn native_bitmap_jpg_bytes() { check_bytes("jpg", 9); }

#[test]
fn bmp_rejects_incomplete_pixels_before_encoding() { check_rejections("bmp"); }
#[test]
fn tif_rejects_incomplete_pixels_before_encoding() { check_rejections("tif"); }
#[test]
fn png_rejects_incomplete_pixels_before_encoding() { check_rejections("png"); }
#[test]
fn jpg_rejects_incomplete_pixels_before_encoding() { check_rejections("jpg"); }

#[test]
fn png_preserves_an_explicit_rgb_rgba_storage_choice() {
    let bgra = [3, 7, 11, 255];
    assert_eq!(encode_png(1, 1, &bgra, false).unwrap()[25], 2);
    assert_eq!(encode_png(1, 1, &bgra, true).unwrap()[25], 6);
    // `hasAlpha must be a boolean` is enforced by the `bool` parameter type.
}

#[test]
fn jpeg_ignores_supplied_alpha_and_returns_owned_bytes() {
    let references = references();
    let reference = references.iter().find(|row| row["id"] == "noise-large").unwrap();
    let (width, height) = (reference["width"].as_i64().unwrap(), reference["height"].as_i64().unwrap());
    let mut bgra = inflate(reference["bgraZlib"].as_str().unwrap());
    let first = encode_jpeg(width, height, &bgra).unwrap();
    let saved = first.clone();
    for index in (3..bgra.len()).step_by(4) { bgra[index] = (index & 255) as u8; }
    assert_eq!(encode_jpeg(width, height, &bgra).unwrap(), saved);
    encode_jpeg(1, 1, &[255, 255, 255, 255]).unwrap();
    assert_eq!(first, saved);
    assert!(encode_jpeg(65501, 1, &vec![0; 65501 * 4]).unwrap_err().0.contains("cannot exceed 65500"));
}
