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
