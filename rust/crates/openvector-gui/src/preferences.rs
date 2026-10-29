use std::{collections::HashMap, fs, io::{self, Cursor, Write}, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicU64, Ordering}}};
use serde_json::{Value, json};
use openvector::{saved_profiles::RecentBasicProfiles, source_history::{SourceHistory, SourceDirectories}, settings_profile::{SettingsProfile, default_selected_profile, default_batch_profile, read_settings_profile, stored_settings_profile}};
use gpui::RenderImage;
use crate::jobs::Bitmap;

/// A recent entry owns its encoded cache and one paintable image. Encoding runs
/// only after a successful source load, not on every preference save.
#[derive(Clone, Debug)]
struct SourcePreview {
    png: Arc<Vec<u8>>,
    image: Arc<RenderImage>,
}
impl SourcePreview {
    fn new(bitmap: Bitmap) -> Result<Self, String> {
        let png = openvector::bitmap_codecs::encode_png(bitmap.width as i64, bitmap.height as i64, &bitmap.bgra, true).map_err(|error| error.to_string())?;
        Ok(Self { png: Arc::new(png), image: bitmap.into_render_image() })
    }
    fn restore(png: Vec<u8>) -> Result<Self, String> {
        // This bound matches Bitmap::preview and the 360px Preview dialog.
        // Reject oversized stored images before the decoder allocates pixels.
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(360); limits.max_image_height = Some(360);
        limits.max_alloc = Some(4 * 1024 * 1024);
        let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(&png), limits).map_err(|error| error.to_string())?;
        let rgba = image::DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?.into_rgba8();
        let (width, height) = rgba.dimensions();
        let mut bgra = rgba.into_raw();
        for pixel in bgra.chunks_exact_mut(4) { pixel.swap(0, 2); }
        Ok(Self { png: Arc::new(png), image: Bitmap { width: width as usize, height: height as usize, bgra }.into_render_image() })
    }
}

#[derive(Clone, Debug)]
pub struct Preferences {
    pub path: PathBuf,
    pub skip_review: bool,
    pub auto_palette: bool,
    pub auto_segmentation: bool,
    pub auto_vectorization: bool,
    pub sources: SourceHistory,
    source_previews: HashMap<PathBuf, SourcePreview>,
    pub directories: SourceDirectories,
    pub source_directory: PathBuf,
    pub selected: SettingsProfile,
    pub batch: SettingsProfile,
    /// Recent Basic Settings, newest first. Stored as the shared projected
    /// profiles; labels are derived again on load.
    pub recent_basic: RecentBasicProfiles,
    pub batch_directory: PathBuf,
    pub format: String,
    pub layout: String,
    pub curve_mode: String,
    pub thin_stroke: bool,
    pub compact_eps: bool,
    pub quick_save: crate::quick_save::QuickSave,
    pub bitmap_save: crate::quick_save::QuickSave,
    pub bitmap_scale: f64,
    pub drag_result: crate::drag_result::DragPreferences,
}
impl Preferences {
    pub fn new(path: PathBuf) -> Self {
        Self { path, skip_review: false, auto_palette: true, auto_segmentation: true,
            auto_vectorization: true, sources: SourceHistory::default(), source_previews: HashMap::new(), directories: SourceDirectories::default(),
            source_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected: default_selected_profile(), batch: default_batch_profile(), recent_basic: RecentBasicProfiles::default(),
            batch_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), format: "svg".into(),
            layout: openvector::export_options::ExportOptions::default().layout.name().into(), thin_stroke: false, compact_eps: false,
            curve_mode: openvector::export_options::ExportOptions::default().curve_mode.name().into(),
            quick_save: crate::quick_save::QuickSave::new(false), bitmap_save: crate::quick_save::QuickSave::new(true), bitmap_scale: 1.0,
            drag_result: crate::drag_result::DragPreferences::default() }
    }
    pub fn default_path() -> PathBuf {
        if let Some(path) = std::env::var_os("OPENVECTOR_PREFERENCES") { return path.into(); }
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("openvector/gui.json")
    }
    pub fn load(path: PathBuf) -> Result<Self, String> {
        Self::read(&path).map_err(|error| format!("Cannot load preferences {}: {error} The file was not changed. Repair it, or move it aside to start with defaults, then restart OpenVector.", path.display()))
    }
    fn read(path: &Path) -> Result<Self, String> {
        let mut prefs = Self::new(path.to_owned());
        let bytes = match fs::read(&prefs.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(prefs),
            Err(error) => return Err(format!("Cannot read preferences: {error}")),
        };
        let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| format!("Cannot read preferences: {error}"))?;
        if !value.is_object() { return Err("Preferences must be a JSON object.".into()); }
        for (key, target) in [("skipReview", &mut prefs.skip_review), ("autoPalette", &mut prefs.auto_palette),
            ("autoSegmentation", &mut prefs.auto_segmentation), ("autoVectorization", &mut prefs.auto_vectorization),
            ("thinStroke", &mut prefs.thin_stroke), ("compactEps", &mut prefs.compact_eps)] {
            if let Some(value) = value.get(key) {
                *target = value.as_bool().ok_or_else(|| format!("Preference {key} must be boolean."))?;
            }
        }
        if let Some(format) = value.get("format") {
            let format = format.as_str().ok_or("Export format must be text.")?;
            if !["ai", "dxf", "emf", "eps", "pdf", "svg"].contains(&format) { return Err("Unknown export format in preferences.".into()); }
            prefs.format = format.into();
        }
        if let Some(layout) = value.get("layout") {
            let layout = layout.as_str().ok_or("Shape mode must be text.")?;
            if !["stacked", "cutouts", "grouped-cutouts"].contains(&layout) { return Err("Unknown shape mode in preferences.".into()); }
            prefs.layout = layout.into();
        }
        if let Some(mode) = value.get("curveMode") {
            prefs.curve_mode = openvector::export_options::CurveMode::parse(mode.as_str().ok_or("DXF curve mode must be text.")?).ok_or("Unknown DXF curve mode in preferences.")?.name().into();
        }
        prefs.sources = SourceHistory::from_snapshot(&value["sources"]).map_err(str::to_string)?;
        for file in value["sources"]["files"].as_array_mut().expect("validated source-history files") {
            let path = PathBuf::from(file["filename"].as_str().expect("validated source filename"));
            let stored = file.as_object_mut().expect("validated source entry").remove("previewPng").ok_or("Recent image is missing its cached previewPng.")?;
            // Explicit null is a current-format entry without a thumbnail. Never
            // manufacture a cache by reading the original path during startup.
            if !stored.is_null() {
                let png: Vec<u8> = serde_json::from_value(stored).map_err(|error| format!("Invalid cached thumbnail: {error}"))?;
                prefs.source_previews.insert(path, SourcePreview::restore(png)?);
            }
        }
        if let Some(value) = value.get("directories") { prefs.directories = SourceDirectories::from_snapshot(value).map_err(str::to_string)?; }
        if let Some(value) = value.get("sourceDirectory") { prefs.source_directory = PathBuf::from(value.as_str().ok_or("Source directory must be text.")?); }
        if let Some(value) = value.get("selected") { prefs.selected = read_settings_profile(value)?; }
        if let Some(value) = value.get("batch") { prefs.batch = read_settings_profile(value)?; }
        if let Some(value) = value.get("recentBasicSettings") { prefs.recent_basic = RecentBasicProfiles::from_stored_profiles(value)?; }
        if let Some(value) = value.get("batchDirectory") { prefs.batch_directory = PathBuf::from(value.as_str().ok_or("Batch directory must be text.")?); }
        if let Some(value) = value.get("quickSave") { prefs.quick_save.restore(value)?; }
        if let Some(value) = value.get("bitmapSave") { prefs.bitmap_save.restore(value)?; }
        if let Some(value) = value.get("dragResult") { prefs.drag_result.restore(value)?; }
        if let Some(value) = value.get("bitmapScale") {
            prefs.bitmap_scale = value.as_f64().filter(|scale| scale.is_finite() && *scale > 0.0).ok_or("Bitmap scale must be positive and finite.")?;
        }
        Ok(prefs)
    }
    pub fn source_preview(&self, path: &Path) -> Option<&Arc<RenderImage>> {
        self.source_previews.get(path).map(|preview| &preview.image)
    }
    /// Called only for a successful single-image load. Batch decodes never
    /// update recent entries. Retired images must leave GPUI's window atlas.
    pub fn remember(&mut self, path: &Path, width: usize, height: usize, bgra: &[u8], released: &mut Vec<Arc<RenderImage>>) -> Result<(), String> {
        let bytes = fs::metadata(path).map_err(|error| format!("Cannot read source metadata: {error}"))?.len();
        let preview = SourcePreview::new(Bitmap::preview(width, height, bgra))?;
        let history = self.sources.add(&path.to_string_lossy(), width as u32, height as u32, bytes).map_err(str::to_string)?;
        let files = history["files"].as_array().expect("source-history files");
        released.extend(self.source_previews.extract_if(|path, _| !files.iter().any(|file| Path::new(file["filename"].as_str().expect("source filename")) == path)).map(|(_, preview)| preview.image));
        if let Some(replaced) = self.source_previews.insert(path.to_owned(), preview) { released.push(replaced.image); }
        if let Some(directory) = path.parent() { self.directories.add(&directory.to_string_lossy()).map_err(str::to_string)?; }
        Ok(())
    }
    pub fn clear_sources(&mut self) -> impl Iterator<Item = Arc<RenderImage>> + '_ {
        self.sources.clear();
        self.source_previews.drain().map(|(_, preview)| preview.image)
    }
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        let mut sources = self.sources.snapshot();
        for file in sources["files"].as_array_mut().expect("source-history files") {
            let path = Path::new(file["filename"].as_str().expect("source filename"));
            file["previewPng"] = self.source_previews.get(path).map_or(Value::Null, |preview| json!(preview.png.as_ref()));
        }
        let mut value = json!({"skipReview":self.skip_review,"autoPalette":self.auto_palette,
            "autoSegmentation":self.auto_segmentation,"autoVectorization":self.auto_vectorization,
            "directories":self.directories.snapshot(),
            "sourceDirectory":self.source_directory,
            "selected":stored_settings_profile(&self.selected),"batch":stored_settings_profile(&self.batch),"batchDirectory":self.batch_directory,
            "recentBasicSettings":self.recent_basic.stored_profiles(),
            "format":self.format,"layout":self.layout,"curveMode":self.curve_mode,
            "thinStroke":self.thin_stroke,"compactEps":self.compact_eps,
            "quickSave":self.quick_save.preferences(),"bitmapSave":self.bitmap_save.preferences(),"bitmapScale":self.bitmap_scale,
            "dragResult":self.drag_result.preferences()});
        // Move the cached payload into the document rather than serializing a
        // Value into another Value and duplicating every encoded thumbnail byte.
        value["sources"] = sources;
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        // Own the temporary file before writing or cleaning it up. A failed reset
        // must not delete a directory at the destination or another writer's file.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let (temporary, mut file) = loop {
            let path = self.path.with_extension(format!("{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Cannot save preferences: {error}")),
            }
        };
        let write = file.write_all(&bytes).map_err(|error| format!("Cannot save preferences: {error}"));
        drop(file);
        let result = write.and_then(|()| fs::rename(&temporary, &self.path).map_err(|error| format!("Cannot replace preferences: {error}")));
        if let Err(error) = result {
            return match fs::remove_file(&temporary) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; cannot remove temporary preferences {}: {cleanup}", temporary.display())),
            };
        }
        Ok(())
    }
}
