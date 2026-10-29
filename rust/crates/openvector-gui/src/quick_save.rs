use std::path::{Path, PathBuf};
use serde_json::{Value, json};

pub const VECTOR_FORMATS: [&str; 6] = ["ai", "dxf", "emf", "eps", "pdf", "svg"];
pub const BITMAP_FORMATS: [&str; 4] = ["bmp", "jpg", "png", "tif"];
#[derive(Clone, Debug)]
pub struct SaveChoice { pub format: String, pub directory: PathBuf, pub expanded: bool, pub filename: String }
#[derive(Clone, Debug)]
pub struct QuickSave { pub choices: [SaveChoice; 2], bitmap: bool, working_directory: PathBuf }
impl QuickSave {
    pub fn new(bitmap: bool) -> Self {
        let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let choice = SaveChoice { format: if bitmap { "png" } else { "ai" }.into(), directory: working_directory.clone(), expanded: false, filename: String::new() };
        Self { choices: [choice.clone(), choice], bitmap, working_directory }
    }
    pub fn formats(&self) -> &[&'static str] { if self.bitmap { &BITMAP_FORMATS } else { &VECTOR_FORMATS } }
    pub fn preferences(&self) -> Value {
        let choice = |i: usize| json!({"format":self.choices[i].format,"directory":self.choices[i].directory,"expanded":self.choices[i].expanded});
        json!({"basic":choice(0),"advanced":choice(1)})
    }
    pub fn restore(&mut self, value: &Value) -> Result<(), String> {
        if value.as_object().is_none_or(|object| object.len() != 2) { return Err("Quick Save preferences require basic and advanced choices.".into()); }
        for (mode, key) in ["basic", "advanced"].into_iter().enumerate() {
            let choice = &value[key];
            let format = choice["format"].as_str().filter(|format| self.formats().contains(format)).ok_or("Unknown Quick Save format.")?;
            let directory = choice["directory"].as_str().filter(|text| !text.contains('\0')).ok_or("Quick Save directory must be text without NUL characters.")?;
            let expanded = choice["expanded"].as_bool().ok_or("Quick Save expanded must be boolean.")?;
            self.choices[mode].format = format.into(); self.choices[mode].directory = self.directory(directory); self.choices[mode].expanded = expanded;
        }
        Ok(())
    }
    fn directory(&self, directory: &str) -> PathBuf {
        let selected = if directory.is_empty() { self.working_directory.clone() } else { PathBuf::from(directory) };
        let absolute = if selected.is_absolute() { selected.clone() } else { self.working_directory.join(&selected) };
        if absolute.is_dir() { absolute.canonicalize().unwrap_or(selected) } else { selected }
    }
    pub fn configure(&mut self, mode: usize, format: &str, directory: &str, filename: &str) -> Result<(), String> {
        if !self.formats().contains(&format) || directory.contains('\0') || filename.contains('\0') { return Err("Choose a supported format and enter a filename without NUL characters.".into()); }
        self.choices[mode].format = format.into(); self.choices[mode].directory = self.directory(directory);
        self.set_filename(mode, filename); Ok(())
    }
    fn set_filename(&mut self, mode: usize, filename: &str) {
        if filename.is_empty() { self.choices[mode].filename.clear(); return; }
        let base = self.working_directory.join(&self.choices[mode].directory);
        let absolute = base.join(filename);
        let directory = filename == ".." || filename.ends_with(['/', '\\']) || absolute.is_dir();
        self.choices[mode].directory = self.directory(if directory { &absolute } else { absolute.parent().unwrap_or(&base) }.to_string_lossy().as_ref());
        let leaf = if directory { "" } else { absolute.file_name().and_then(|name| name.to_str()).unwrap_or("") };
        if let Some(extension) = Path::new(leaf).extension().and_then(|ext| ext.to_str()) {
            let extension = extension.to_lowercase(); if self.formats().contains(&extension.as_str()) { self.choices[mode].format = extension; }
        }
        self.choices[mode].filename = leaf.split('.').next().unwrap_or("").into();
    }
    pub fn source_changed(&mut self, filename: &Path) {
        let absolute = self.working_directory.join(filename);
        let directory = self.directory(absolute.parent().unwrap_or(&self.working_directory).to_string_lossy().as_ref());
        self.choices[0].directory = directory.clone();
        self.choices[1].directory = directory;
        for mode in 0..2 { self.set_filename(mode, absolute.file_name().and_then(|name| name.to_str()).unwrap_or("")); }
    }
    pub fn target(&self, mode: usize) -> Result<PathBuf, String> {
        let choice = &self.choices[mode]; if choice.filename.is_empty() { return Err("Enter a filename before saving.".into()); }
        Ok(self.working_directory.join(&choice.directory).join(format!("{}.{}", choice.filename, choice.format)))
    }
}
