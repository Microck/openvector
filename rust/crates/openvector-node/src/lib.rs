//! Node-API owns only transport conversion. All behavior lives in `openvector`.
use napi::{Error, Result};
use napi_derive::napi;
use serde_json::Value;
mod document;
mod batch;
mod snapshot;

fn engine_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}

#[napi]
pub fn format_file_size(bytes: f64) -> Result<String> {
    if !bytes.is_finite() || bytes.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&bytes) {
        return Err(engine_error("File size must be a nonnegative safe integer."));
    }
    openvector::source_history::format_file_size(bytes as u64).map_err(engine_error)
}

#[napi]
pub struct SourceHistory {
    inner: openvector::source_history::SourceHistory,
}

#[napi]
impl SourceHistory {
    #[napi(constructor)]
    pub fn new(snapshot: Option<Value>) -> Result<Self> {
        Ok(Self { inner: match snapshot {
            Some(snapshot) => openvector::source_history::SourceHistory::from_snapshot(&snapshot).map_err(engine_error)?,
            None => openvector::source_history::SourceHistory::default(),
        } })
    }

    #[napi]
    pub fn add(&mut self, filename: String, width: f64, height: f64, bytes: f64) -> Result<Value> {
        for dimension in [width, height] {
            if dimension.fract() != 0.0 || !(1.0..=i32::MAX as f64).contains(&dimension) {
                return Err(engine_error("Image dimensions must be positive signed 32-bit integers."));
            }
        }
        if !bytes.is_finite() || bytes.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&bytes) {
            return Err(engine_error("File size must be a nonnegative safe integer."));
        }
        self.inner.add(&filename, width as u32, height as u32, bytes as u64).map_err(engine_error)
    }

    #[napi]
    pub fn snapshot(&self) -> Value { self.inner.snapshot() }

    #[napi]
    pub fn remove(&mut self, filename: String) -> Value { self.inner.remove(&filename) }

    #[napi]
    pub fn clear(&mut self) -> Value { self.inner.clear() }
}

#[napi]
pub struct SourceDirectories {
    inner: openvector::source_history::SourceDirectories,
}

#[napi]
impl SourceDirectories {
    #[napi(constructor)]
    pub fn new(snapshot: Option<Value>) -> Result<Self> {
        Ok(Self { inner: match snapshot {
            Some(snapshot) => openvector::source_history::SourceDirectories::from_snapshot(&snapshot).map_err(engine_error)?,
            None => openvector::source_history::SourceDirectories::default(),
        } })
    }

    #[napi]
    pub fn add(&mut self, directory: String) -> Result<Value> {
        self.inner.add(&directory).map_err(engine_error)
    }

    #[napi]
    pub fn snapshot(&self) -> Value { self.inner.snapshot() }

    #[napi]
    pub fn clear(&mut self) -> Value { self.inner.clear() }
}

#[napi]
pub fn default_document_settings() -> Value {
    Value::Object(openvector::document_controls::DocumentSettings::default().to_json())
}

#[napi]
pub fn default_advanced_profile() -> Value {
    openvector::settings_profile::default_advanced_profile().to_json()
}

#[napi]
pub fn resolve_parameters(options: Value) -> Result<Value> {
    openvector::settings::resolve_parameters(&options).map_err(engine_error)
}

#[napi]
pub struct SavedAdvancedProfiles {
    inner: openvector::saved_profiles::SavedAdvancedProfiles,
}

#[napi]
impl SavedAdvancedProfiles {
    #[napi(constructor)]
    pub fn new(entries: Option<Value>) -> Result<Self> {
        Ok(Self { inner: match entries {
            Some(entries) => openvector::saved_profiles::SavedAdvancedProfiles::new(&entries).map_err(engine_error)?,
            None => openvector::saved_profiles::SavedAdvancedProfiles::default(),
        } })
    }

    #[napi]
    pub fn snapshot(&self) -> Value {
        Value::Array(self.inner.snapshot().into_iter()
            .map(|entry| serde_json::json!({"name":entry.name,"profile":entry.profile.to_json()})).collect())
    }

    #[napi]
    pub fn put(&mut self, name: String, profile: Value) -> Result<()> {
        self.inner.put_json(&name, &profile).map_err(engine_error)
    }

    #[napi]
    pub fn select(&mut self, name: String) -> Value {
        let (index, profile) = self.inner.select(&name);
        serde_json::json!({"index":index,"profile":profile.map(|profile|profile.to_json())})
    }

    #[napi]
    pub fn remove(&mut self, name: String) -> i32 { self.inner.remove(&name) as i32 }

    #[napi]
    pub fn rename(&mut self, name: String, new_name: String, force: bool) -> Result<Value> {
        let renamed = self.inner.rename(&name, &new_name, force).map_err(engine_error)?;
        Ok(serde_json::json!({"index":renamed.index,"changed":renamed.changed,"replacedIndex":renamed.replaced_index}))
    }

    #[napi]
    pub fn reset(&mut self) { self.inner.reset(); }

    #[napi]
    pub fn load(&mut self, filename: String) -> Result<()> {
        self.inner = openvector::saved_profiles::SavedAdvancedProfiles::read_file(filename).map_err(engine_error)?;
        Ok(())
    }

    #[napi]
    pub fn save(&self, filename: String) -> Result<()> {
        self.inner.write_file(filename).map_err(engine_error)
    }
}

/// Recent Basic Settings: five stored profiles behind four visible links.
/// Rust owns labels, completion eligibility, duplicates and capacity.
#[napi]
pub struct RecentBasicProfiles {
    inner: openvector::saved_profiles::RecentBasicProfiles,
}

fn recent_entries(entries: &[openvector::saved_profiles::RecentBasicProfile]) -> Value {
    Value::Array(entries.iter()
        .map(|entry| serde_json::json!({"label":entry.label,"profile":entry.profile.to_json()})).collect())
}

#[napi]
impl RecentBasicProfiles {
    /// Accepts a `storedProfiles()` array and rebuilds it through the record rule.
    #[napi(constructor)]
    pub fn new(stored_profiles: Option<Value>) -> Result<Self> {
        Ok(Self { inner: match stored_profiles {
            Some(stored) => openvector::saved_profiles::RecentBasicProfiles::from_stored_profiles(&stored).map_err(engine_error)?,
            None => openvector::saved_profiles::RecentBasicProfiles::default(),
        } })
    }

    /// Newest first, with complete in-memory profiles.
    #[napi]
    pub fn entries(&self) -> Value { recent_entries(self.inner.entries()) }

    #[napi]
    pub fn visible_entries(&self) -> Value { recent_entries(self.inner.visible_entries()) }

    /// Offers one fitting completion and returns whether Rust found it eligible.
    #[napi]
    pub fn record_completion(&mut self, profile: Value, status: f64, did_work: bool, batch: bool) -> Result<bool> {
        // A u32 parameter would wrap (2**32 arrives as the eligible status 0) and
        // truncate fractions, so take the double and require an exact u32.
        if status.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&status) {
            return Err(engine_error("Completion status must be an unsigned 32-bit integer."));
        }
        let profile = openvector::settings_profile::read_settings_profile(&profile).map_err(engine_error)?;
        Ok(self.inner.record_completion(&profile, status as u32, did_work, batch))
    }

    #[napi]
    pub fn clear(&mut self) { self.inner.clear(); }

    /// The persistence projection the constructor and collection files accept.
    #[napi]
    pub fn stored_profiles(&self) -> Value { self.inner.stored_profiles() }

    /// Replaces the list only after a successful read; a missing file is empty.
    #[napi]
    pub fn load(&mut self, filename: String) -> Result<()> {
        self.inner = openvector::saved_profiles::RecentBasicProfiles::read_file(filename).map_err(engine_error)?;
        Ok(())
    }

    #[napi]
    pub fn save(&self, filename: String) -> Result<()> {
        self.inner.write_file(filename).map_err(engine_error)
    }
}

#[napi]
pub fn default_batch_profile() -> Value {
    openvector::settings_profile::default_batch_profile().to_json()
}

#[napi]
pub fn batch_profile_controls(profile: Value) -> Result<Value> {
    let profile = openvector::settings_profile::read_settings_profile(&profile).map_err(engine_error)?;
    Ok(openvector::settings_profile::batch_profile_controls(&profile))
}

#[napi]
pub fn apply_advanced_profile(current: Option<Value>, selected: Value, batch: bool) -> Result<Value> {
    let current = current.as_ref().map(openvector::settings_profile::read_settings_profile).transpose().map_err(engine_error)?;
    let selected = openvector::settings_profile::read_settings_profile(&selected).map_err(engine_error)?;
    openvector::settings_profile::apply_advanced_profile(current.as_ref(), &selected, batch)
        .map(|profile| profile.to_json()).map_err(engine_error)
}

#[napi]
pub fn apply_basic_profile(selected: Value) -> Result<Value> {
    let selected = openvector::settings_profile::read_settings_profile(&selected).map_err(engine_error)?;
    Ok(openvector::settings_profile::apply_basic_profile(&selected).to_json())
}

#[napi]
pub fn basic_profile_label(profile: Value) -> Result<String> {
    let profile = openvector::settings_profile::read_settings_profile(&profile).map_err(engine_error)?;
    Ok(openvector::settings_profile::basic_profile_label(&profile))
}

#[napi]
pub fn apply_palette_command(profile: Value, runtime: Value, command: Value) -> Result<Value> {
    use openvector::profile_palette::{read_palette_runtime, PaletteCommand, PaletteProfile};
    let mut profile = openvector::settings_profile::read_settings_profile(&profile).map_err(engine_error)?;
    let mut runtime = read_palette_runtime(&runtime).map_err(engine_error)?;
    let command = PaletteCommand::from_json(&command).map_err(engine_error)?;
    let mut color_count = profile.color_count as f64;
    let mut palette_count = profile.palette_color_count as f64;
    let changed = openvector::profile_palette::apply_palette_command(&mut PaletteProfile {
        color_count: &mut color_count, palette_color_count: &mut palette_count,
        palette: &mut profile.settings.palette,
    }, &mut runtime, &command).map_err(engine_error)?;
    profile.color_count = color_count as i32;
    profile.palette_color_count = palette_count as i32;
    Ok(serde_json::json!({"profile":profile.to_json(),"runtime":{"backup":runtime.backup,"customized":runtime.customized},"changed":changed}))
}
