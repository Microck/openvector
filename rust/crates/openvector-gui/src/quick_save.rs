use std::path::{Path, PathBuf};
use serde_json::{Value, json};

pub const VECTOR_FORMATS: [&str; 6] = ["ai", "dxf", "emf", "eps", "pdf", "svg"];
pub const BITMAP_FORMATS: [&str; 4] = ["bmp", "jpg", "png", "tif"];
#[derive(Clone, Debug)]
pub struct SaveChoice { pub format: String, pub directory: PathBuf, pub expanded: bool, pub filename: String, auto_filename: bool, stale_source: bool }
#[derive(Clone, Debug)]
pub struct QuickSave { pub choices: [SaveChoice; 2], bitmap: bool, working_directory: PathBuf }
impl QuickSave {
    pub fn new(bitmap: bool) -> Self {
        let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let choice = SaveChoice { format: if bitmap { "png" } else { "ai" }.into(), directory: working_directory.clone(), expanded: false, filename: String::new(), auto_filename: true, stale_source: false };
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
        // An unchanged field is the stored stem, whose dots belong to the name
        // (`drawing.svg` from `drawing.svg.png`). Only a typed name may carry an
        // extension that selects the format.
        if filename != self.choices[mode].filename {
            self.set_filename(mode, filename);
            self.choices[mode].auto_filename = false;
            self.choices[mode].stale_source = false;
        }
        Ok(())
    }
    fn set_filename(&mut self, mode: usize, filename: &str) {
        if filename.is_empty() { self.choices[mode].filename.clear(); return; }
        let base = self.working_directory.join(&self.choices[mode].directory);
        let absolute = base.join(filename);
        let directory = filename == ".." || filename.ends_with(['/', '\\']) || absolute.is_dir();
        self.choices[mode].directory = self.directory(if directory { &absolute } else { absolute.parent().unwrap_or(&base) }.to_string_lossy().as_ref());
        let leaf = if directory { "" } else { absolute.file_name().and_then(|name| name.to_str()).unwrap_or("") };
        let extension = Path::new(leaf).extension().and_then(|ext| ext.to_str()).map(str::to_lowercase);
        self.choices[mode].filename = if let Some(extension) = extension.filter(|extension| self.formats().contains(&extension.as_str())) {
            self.choices[mode].format = extension;
            Path::new(leaf).file_stem().and_then(|stem| stem.to_str()).unwrap_or(leaf).into()
        } else { leaf.into() };
    }
    pub fn source_changed(&mut self, filename: &Path) {
        let absolute = self.working_directory.join(filename);
        let directory = self.directory(absolute.parent().unwrap_or(&self.working_directory).to_string_lossy().as_ref());
        self.choices[0].directory = directory.clone();
        self.choices[1].directory = directory;
        // The source suffix is an input format, not part of the suggested output
        // name. Strip it once; dots inside the stem must survive unchanged.
        let stem = absolute.file_stem().and_then(|name| name.to_str()).unwrap_or("");
        let extension = absolute.extension().and_then(|ext| ext.to_str()).map(str::to_lowercase);
        let format = extension.filter(|extension| self.formats().contains(&extension.as_str()));
        for choice in &mut self.choices {
            choice.filename = stem.into();
            choice.auto_filename = true; choice.stale_source = false;
            if let Some(format) = &format { choice.format = format.clone(); }
        }
    }
    /// Batch processing can replace the current document without a normal load.
    /// Refresh automatic names, but require confirmation before reusing a custom
    /// name that belongs to a previous source.
    pub fn batch_source_changed(&mut self, filename: &Path) {
        let stem = filename.file_stem().and_then(|name| name.to_str()).unwrap_or("");
        let suggested = format!("{stem}_vectorized");
        for choice in &mut self.choices {
            if choice.auto_filename { choice.filename = suggested.clone(); choice.stale_source = false; }
            else { choice.stale_source = true; }
        }
    }
    pub fn needs_source_confirmation(&self, mode: usize) -> bool { self.choices[mode].stale_source }
    pub fn confirm_source(&mut self, mode: usize) { self.choices[mode].stale_source = false; }
    pub fn target(&self, mode: usize) -> Result<PathBuf, String> {
        let choice = &self.choices[mode]; if choice.filename.is_empty() { return Err("Enter a filename before saving.".into()); }
        Ok(self.working_directory.join(&choice.directory).join(format!("{}.{}", choice.filename, choice.format)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_names_keep_the_full_stem_and_only_recognized_output_suffixes_select_format() {
        let mut save = QuickSave::new(false);
        save.configure(0, "pdf", ".", "logo.final.SVG").unwrap();
        assert_eq!(save.target(0).unwrap().file_name().unwrap(), "logo.final.svg");
        save.configure(0, "pdf", ".", "logo.final").unwrap();
        assert_eq!(save.target(0).unwrap().file_name().unwrap(), "logo.final.pdf");
        save.source_changed(Path::new("drawing.svg.png"));
        for mode in 0..2 { assert_eq!(save.choices[mode].filename, "drawing.svg"); }
    }

    #[test]
    fn batch_source_refreshes_automatic_names_and_flags_custom_names() {
        let mut save = QuickSave::new(true);
        save.source_changed(Path::new("alpha_vectorized"));
        save.configure(1, "png", ".", "custom-name").unwrap();
        save.batch_source_changed(Path::new("tiny.png"));
        assert_eq!(save.choices[0].filename, "tiny_vectorized");
        assert_eq!(save.choices[1].filename, "custom-name");
        assert!(!save.needs_source_confirmation(0));
        assert!(save.needs_source_confirmation(1));
        save.confirm_source(1);
        assert!(!save.needs_source_confirmation(1));
        save.batch_source_changed(Path::new("another.png"));
        assert!(save.needs_source_confirmation(1));
        save.configure(1, "png", ".", "another-name").unwrap();
        assert!(!save.needs_source_confirmation(1));
    }

    // The dialog hands the suggested stem back unchanged. Its dots are part of
    // the name, so the format the user picked must still apply.
    #[test]
    fn saving_an_unchanged_suggested_stem_keeps_the_selected_format() {
        let mut save = QuickSave::new(false);
        save.source_changed(Path::new("drawing.svg.png"));
        let suggested = save.choices[0].filename.clone();
        save.configure(0, "pdf", ".", &suggested).unwrap();
        assert_eq!(save.target(0).unwrap().file_name().unwrap(), "drawing.svg.pdf");
    }
}
