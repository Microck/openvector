//! Prepared drag files belong to the app until offered to an external receiver.
use std::{ffi::OsStr, fs, io, path::{Path, PathBuf}, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use serde_json::{Value, json};
use crate::quick_save::VECTOR_FORMATS;

#[derive(Clone, Copy, Debug)]
pub struct DragChoice { pub format: &'static str, pub minimize: bool }
#[derive(Clone, Debug)]
pub struct DragPreferences { pub choices: [DragChoice; 2] }
impl Default for DragPreferences {
    fn default() -> Self { Self { choices: [DragChoice { format: "ai", minimize: true }; 2] } }
}
impl DragPreferences {
    pub fn preferences(&self) -> Value {
        let choice = |index: usize| json!({"format":self.choices[index].format,"minimize":self.choices[index].minimize});
        json!({"basic":choice(0),"advanced":choice(1)})
    }
    pub fn restore(&mut self, value: &Value) -> Result<(), String> {
        let mut choices = self.choices;
        for (mode, key) in ["basic", "advanced"].into_iter().enumerate() {
            let choice = &value[key];
            choices[mode].format = VECTOR_FORMATS.iter().copied().find(|format| choice["format"].as_str() == Some(*format))
                .ok_or("Unknown Drag result format in preferences.")?;
            choices[mode].minimize = choice["minimize"].as_bool().ok_or("Drag result minimize preference must be boolean.")?;
        }
        self.choices = choices;
        Ok(())
    }
}

#[derive(Debug)]
pub struct DragFile { directory: PathBuf, path: PathBuf, offered: AtomicBool }
impl DragFile {
    /// Reserve a private directory. The worker must finish exporting before publishing this value.
    pub fn create(root: &Path, source: &Path, format: &str) -> Result<Self, String> {
        if !VECTOR_FORMATS.contains(&format) { return Err("Unknown Drag result format.".into()); }
        let root = std::path::absolute(root).map_err(|error| format!("Cannot resolve drag export directory: {error}"))?;
        fs::create_dir_all(&root).map_err(|error| format!("Cannot create drag export directory: {error}"))?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = loop {
            let directory = root.join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let created = {
                #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; fs::DirBuilder::new().mode(0o700).create(&directory) }
                #[cfg(not(unix))] { fs::create_dir(&directory) }
            };
            match created {
                Ok(()) => break directory,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Cannot prepare drag export: {error}")),
            }
        };
        let mut path = directory.join(source.file_stem().unwrap_or(OsStr::new("vector")));
        path.add_extension(format);
        Ok(Self { directory, path, offered: AtomicBool::new(false) })
    }
    pub fn path(&self) -> &Path { &self.path }
    /// A drop acknowledgement does not prove the receiver has finished reading.
    /// Keep any file handed to the OS, including cancelled sessions, rather than race its reader.
    pub fn retain_for_receiver(&self) { self.offered.store(true, Ordering::Release); }
}
impl Drop for DragFile {
    fn drop(&mut self) {
        if !self.offered.load(Ordering::Acquire) {
            if let Err(error) = fs::remove_dir_all(&self.directory) {
                if error.kind() != io::ErrorKind::NotFound { eprintln!("Cannot remove unused drag export {}: {error}", self.directory.display()); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unpublished_files_are_removed_but_offered_files_survive_the_source() {
        let root = std::env::temp_dir().join(format!("openvector-drag-lifetime-{}", std::process::id()));
        let unused = DragFile::create(&root, Path::new("logo.source.png"), "svg").unwrap();
        let unused_path = unused.path().to_owned();
        fs::write(&unused_path, b"unpublished").unwrap();
        drop(unused);
        assert!(!unused_path.exists());
        let offered = DragFile::create(&root, Path::new("logo.source.png"), "svg").unwrap();
        let offered_path = offered.path().to_owned();
        fs::write(&offered_path, b"complete receiver data").unwrap();
        offered.retain_for_receiver();
        drop(offered);
        assert_eq!(fs::read(&offered_path).unwrap(), b"complete receiver data");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn basic_and_advanced_choices_round_trip_independently() {
        let mut prefs = DragPreferences::default();
        prefs.choices[0] = DragChoice { format: "svg", minimize: false };
        prefs.choices[1] = DragChoice { format: "pdf", minimize: true };
        let mut restored = DragPreferences::default();
        restored.restore(&prefs.preferences()).unwrap();
        assert_eq!(restored.preferences(), prefs.preferences());
        assert!(restored.restore(&json!({"basic":{"format":"svg","minimize":false},"advanced":{"format":"unknown","minimize":true}})).is_err());
        assert_eq!(restored.preferences(), prefs.preferences());
    }
}
