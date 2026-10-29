use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

pub struct FixtureDirectory(pub PathBuf);
impl FixtureDirectory {
    pub fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        // The clock can return the same timestamp to concurrent tests.
        let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("openvector-gui-workflow-{}-{nonce}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap(); Self(path)
    }
}
impl Drop for FixtureDirectory { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

// A real encoded 16x16 24-bit BMP, decoded exclusively by the shared engine.
pub fn bitmap() -> Vec<u8> {
    let mut bytes = vec![0u8; 54 + 16 * 16 * 3];
    let length = bytes.len() as u32;
    bytes[..2].copy_from_slice(b"BM"); bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes()); bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&16i32.to_le_bytes()); bytes[22..26].copy_from_slice(&16i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes()); bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    for y in 0..16 { for x in 0..16 {
        let color = if (4..12).contains(&x) && (4..12).contains(&y) { 0 } else { 255 };
        bytes[54 + (y * 16 + x) * 3..54 + (y * 16 + x + 1) * 3].fill(color);
    } }
    bytes
}
