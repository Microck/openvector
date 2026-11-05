//! Shared fixture access for differential tests. Fixtures live in the
//! repository's `test/fixtures`, or under `OPENVECTOR_FIXTURES` when the
//! crate is built away from the full tree (for example in a build box).
#![allow(dead_code)]
use std::io::Read;
use std::path::PathBuf;

pub fn fixture_path(name: &str) -> PathBuf {
    let base = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    base.join(name)
}

pub fn read_bytes(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if name.ends_with(".gz") {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(&raw[..]).read_to_end(&mut out).unwrap();
        out
    } else { raw }
}

pub fn read_json(name: &str) -> serde_json::Value {
    serde_json::from_slice(&read_bytes(name)).unwrap()
}

pub fn read_ndjson(name: &str) -> Vec<serde_json::Value> {
    read_bytes(name).split(|b| *b == b'\n').filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap()).collect()
}

pub fn b64(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).unwrap()
}

/// Adapt only the producer field in original PDF captures. Padding keeps all
/// original object offsets and startxref values unchanged.
pub fn public_pdf_fixture(bytes: &[u8]) -> Vec<u8> {
    const ORIGINAL: &[u8] = b"/Producer(VectorMagic.com)";
    const PUBLIC: &[u8] = b"/Producer(OpenVector)     ";
    assert_eq!(ORIGINAL.len(), PUBLIC.len());
    let mut output = bytes.to_vec();
    if let Some(index) = output.windows(ORIGINAL.len()).position(|window| window == ORIGINAL) {
        output[index..index + ORIGINAL.len()].copy_from_slice(PUBLIC);
    }
    output
}

/// Adapt only the fixed UTF-16 description in original EMF captures, including
/// cancelled outputs with a header. Preserve all record bytes and header fields.
pub fn public_emf_fixture(mut output: Vec<u8>) -> Vec<u8> {
    if output.len() >= 236 && output.starts_with(&1u32.to_le_bytes()) && &output[40..44] == b" EMF" {
        assert_eq!(&output[4..8], &236u32.to_le_bytes());
        assert_eq!(&output[60..64], &64u32.to_le_bytes());
        assert_eq!(&output[64..68], &108u32.to_le_bytes());
        let description = &mut output[108..236];
        description.fill(0);
        for (index, unit) in "OpenVector\0OpenVector\0\0".encode_utf16().enumerate() {
            description[index * 2..index * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
    }
    output
}
