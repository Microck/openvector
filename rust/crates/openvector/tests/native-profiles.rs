mod common;
mod editor_support;

use openvector::{document_controls::read_document_settings, image_document::{AutomaticSelection, DocumentProgress, DocumentSnapshot, ImageDocument, TaskResult, CANCEL_TASK}, palette::CoreSession, profile_palette::PaletteRuntime, saved_profiles::{RecentBasicProfile, RecentBasicProfiles, SavedAdvancedProfiles}, settings_profile::*};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-native-profiles-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }

fn compare_profile(actual: &SettingsProfile, frame: &Value) {
    let bits = frame["settingsBits"].as_str().unwrap();
    editor_support::compare_settings(&actual.settings, bits, frame["paletteArgbBits"].as_str().unwrap());
    let bytes = editor_support::hex(bits);
    for (value, offset) in [(actual.color_count, 8), (actual.basic_color_count, 64), (actual.palette_color_count, 56), (actual.recommended_background_mode, 88)] {
        assert_eq!(value, i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()));
    }
}
fn set(profile: &mut SettingsProfile, signature: &str, value: &Value) {
    let number = value.as_i64().unwrap() as i32;
    match signature {
        "setAdvNumColors(int)" => profile.color_count = number,
        "setBatchNumColors(int)" => profile.basic_color_count = number,
        "setImageCategory(int)" => profile.settings.category = number,
        "setImageQuality(int)" => profile.settings.quality = number,
        "setPaletteMode(int)" => profile.settings.palette_mode = number,
        "setUseAdvancedMode(bool)" => profile.settings.advanced = number != 0,
        "setAdvColorSensitivity(int)" => profile.settings.sensitivity = number,
        "setAdvUsePaletteAa(bool)" => profile.settings.anti_alias = number != 0,
        "setAdvSegmentationComplexity(int)" => profile.settings.complexity = number,
        "setAdvMinNumPixels(int)" => profile.settings.minimum_pixels = number,
        "setAdvAntiAliasingRejection(int)" => profile.settings.artifact_rejection = number,
        "setAdvClusterColors(bool)" => profile.settings.cluster_colors = number != 0,
        "setAdvContourSmoothness(int)" => profile.settings.smoothness = number,
        "setAdvDetectSharpCorners(bool)" => profile.settings.detect_corners = number != 0,
        "setAdvCurveComplexity(int)" => profile.settings.curve_complexity = number,
        "setAdvUseContourAa(bool)" => profile.settings.smoothing_anti_alias = number != 0,
        "setFlattenImageMode(int)" => profile.settings.background_mode = number,
        _ => panic!("unhandled native setter {signature}"),
    }
}
fn load(document: &mut ImageDocument, reference: &Value, name: &str) -> String {
    let decoded = openvector::image_input::decode_image(&common::b64(reference["inputs"][name].as_str().unwrap())).unwrap();
    let filename = openvector::image_input::resolve_image_filename(name);
    document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, filename.shadow_control as f64).unwrap();
    filename.extension
}
/// Run the real automatic chain into `selected` and return its fitting
/// completion (status, did work), which decides recent-list eligibility.
fn automatic(document: &mut ImageDocument, selected: &mut SettingsProfile, extension: String) -> (u32, bool) {
    let mut controls = Value::Object(selected.settings.to_json()); controls["extension"] = json!(extension);
    let mut fitting = None;
    let mut on_stage = |stage: &str, task: &TaskResult, _: &AutomaticSelection| { if stage == "fitting" { fitting = Some((task.state.status, task.did_work)); } Ok(()) };
    let result = document.run_automatic_with_observers(&controls, None, Some(&mut on_stage)).unwrap(); selected.settings = result.settings;
    if let Some(count) = result.palette_count { selected.color_count = count as i32; selected.palette_color_count = count as i32; }
    selected.recommended_background_mode = result.recommended_background_mode;
    fitting.unwrap()
}
/// A selected profile with the four inputs of the recent summary label.
fn summary_profile(category: i32, quality: i32, palette_mode: i32, palette_color_count: i32) -> SettingsProfile {
    let mut profile = default_selected_profile(); profile.settings.category = category; profile.settings.quality = quality;
    profile.settings.palette_mode = palette_mode; profile.palette_color_count = palette_color_count; profile
}
/// Native VS::save record `VmQt::VectorizationSettings::basic::<index>` (21
/// keys) in the stored SettingsProfile projection. Registry integers arrive as
/// unsigned 32-bit values and booleans as text; QRgb reloads as opaque.
fn native_basic_entry(values: &Value, index: usize) -> Value {
    let value = |name: &str| &values[format!("/VmQt::VectorizationSettings::basic::{index}/{name}").as_str()]["value"];
    let int = |name: &str| value(name).as_i64().unwrap() as u32 as i32;
    let flag = |name: &str| match value(name).as_str().unwrap() { "true" => true, "false" => false, other => panic!("{name} is not a boolean: {other}") };
    let rgb: u32 = value("flatten_image_color").as_str().unwrap().parse().unwrap();
    let background = [rgb & 255, rgb >> 8 & 255, rgb >> 16 & 255, 255].map(|byte| byte as f64 / 255.0);
    assert_eq!(value("name"), "not named");
    json!({
        "settings": {
            "category": int("image_category"), "quality": int("image_quality"), "sensitivity": int("adv_color_sensitivity"),
            "advanced": flag("use_advanced_mode"), "antiAlias": flag("adv_use_palette_aa"), "backgroundMode": int("flatten_image_mode"),
            "background": background, "complexity": int("adv_segmentation_complexity"), "minimumPixels": int("adv_min_num_pixels"),
            "artifactRejection": int("adv_anti_aliasing_rejection"), "clusterColors": flag("adv_cluster_colors"), "paletteMode": int("palette_mode"),
            "palette": [], "smoothness": int("adv_contour_smoothness"), "detectCorners": int("adv_detect_sharp_corners") != 0,
            "curveComplexity": int("adv_curve_complexity"), "smoothingAntiAlias": flag("adv_use_contour_aa"),
        },
        "colorCount": int("adv_num_colors"), "basicColorCount": int("batch_num_colors"), "paletteColorCount": int("num_palette_colors"),
        "recommendedBackgroundMode": int("recommended_flatten_image_mode"),
    })
}
/// Recent Basic replay with the stages it published.
fn replay_basic(document: &mut ImageDocument, stored: &SettingsProfile, runtime: &PaletteRuntime) -> (Result<Option<ProfileResult>, String>, Vec<String>) {
    let mut stages = Vec::new();
    let mut observer = |stage: &str, _: &TaskResult, _: &SettingsProfile, _: &PaletteRuntime| { stages.push(stage.to_owned()); Ok(()) };
    let result = run_basic_profile(document, stored, runtime, None, Some(&mut observer)); (result, stages)
}
/// Recent Basic replay, optionally cancelled during fitting, with the fitting
/// completion (status, did work) it published.
fn replay_fitting(document: &mut ImageDocument, stored: &SettingsProfile, cancel_fitting: bool) -> (Result<Option<ProfileResult>, String>, Option<(u32, bool)>) {
    let cancel = document.cancellation_handle(); let mut fitting = None;
    let mut progress = |event: &DocumentProgress, _: &DocumentSnapshot| { if cancel_fitting && event.stage == "fitting" { cancel.store(true, Ordering::Relaxed); } Ok(()) };
    let mut stage = |name: &str, task: &TaskResult, _: &SettingsProfile, _: &PaletteRuntime| { if name == "fitting" { fitting = Some((task.state.status, task.did_work)); } Ok(()) };
    let result = run_basic_profile(document, stored, &PaletteRuntime::default(), Some(&mut progress), Some(&mut stage)); (result, fitting)
}

#[test]
fn native_saved_profile_actions_preserve_selected_batch_default_and_restart_order() {
    let rows = common::read_ndjson("saved-profiles-reference.ndjson.gz"); assert_eq!(rows.len(), 8);
    for reference in rows {
        let directory = Directory::new(); let path = directory.0.join("profiles.json");
        let mut profiles = SavedAdvancedProfiles::default();
        let mut selected = Some(default_selected_profile()); let mut batch = default_batch_profile();
        for (phase, actions) in reference["phases"].as_array().unwrap().iter().enumerate() {
            let mut batch_mode = false;
            if phase != 0 { profiles = SavedAdvancedProfiles::read_file(&path).unwrap(); selected = None; }
            for (index, action) in actions.as_array().unwrap().iter().enumerate() {
                let mut selected_index = -2;
                match action[0].as_str().unwrap() {
                    "set" => set(if action[1] == 1 { &mut batch } else { selected.as_mut().unwrap() }, action[2].as_str().unwrap(), &action[3]),
                    "batch" => batch_mode = action[1].as_bool().unwrap(),
                    "white" => selected.as_mut().unwrap().settings.background = [1.0; 4],
                    "color" => {
                        let channels: Vec<f32> = action[1].as_array().unwrap().iter().rev().map(|v| v.as_f64().unwrap() as f32).collect();
                        selected.as_mut().unwrap().settings.background = openvector::pixel_input::quantize_qcolor_channels(&channels).try_into().unwrap();
                    }
                    "automatic" => {
                        let mut document = CoreSession::default().create_document();
                        let extension = load(&mut document, &reference, action[1].as_str().unwrap());
                        let mut profile = default_selected_profile(); automatic(&mut document, &mut profile, extension); selected = Some(profile);
                    }
                    "save" => profiles.put(action[1].as_str().unwrap(), selected.as_ref().unwrap().clone()),
                    "save-many" => for name in action[1].as_array().unwrap() { profiles.put(name.as_str().unwrap(), selected.as_ref().unwrap().clone()); },
                    "load" => {
                        let (index, profile) = profiles.select(action[1].as_str().unwrap()); selected_index = index;
                        if let Some(profile) = profile {
                            if batch_mode { batch = apply_advanced_profile(Some(&batch), &profile, true).unwrap(); }
                            else { selected = Some(apply_advanced_profile(None, &profile, false).unwrap()); }
                        }
                    }
                    "persist" => profiles.write_file(&path).unwrap(),
                    "snapshot" => (),
                    other => panic!("unknown saved profile action {other}"),
                }
                let expected = &reference["expected"][phase]["frames"][index];
                if expected["action"] != 28 { continue; }
                assert_eq!(selected_index, expected["selectedIndex"].as_i64().unwrap() as isize, "{} {phase}/{index}", reference["id"]);
                let entries = profiles.snapshot(); let native = expected["profiles"].as_array().unwrap(); assert_eq!(entries.len(), native.len());
                for (entry, frame) in entries.iter().zip(native) { assert_eq!(entry.name, frame["name"].as_str().unwrap()); compare_profile(&entry.profile, frame); }
                if let Some(selected) = &selected { compare_profile(selected, &expected["selected"]); }
                compare_profile(&batch, &expected["batch"]); compare_profile(&default_advanced_profile(), &expected["defaultAdvanced"]);
            }
        }
    }
}

#[test]
fn native_retained_profile_selection_keeps_committed_and_requested_controls_distinct() {
    let rows = common::read_ndjson("profile-document-reference.ndjson.gz"); assert_eq!(rows.len(), 5);
    for reference in &rows[1..] {
        let mut document = CoreSession::default().create_document(); let mut loaded = false;
        let mut profiles = SavedAdvancedProfiles::default(); let mut selected = default_selected_profile(); let mut batch_mode = false;
        for (index, action) in reference["actions"].as_array().unwrap().iter().enumerate() {
            match action[0].as_str().unwrap() {
                "automatic" => {
                    let extension = load(&mut document, reference, action[1].as_str().unwrap());
                    if loaded { selected.settings.palette.clear(); selected.settings.background_mode = -1; selected.settings.background = [1.0, 1.0, 1.0, 0.0]; }
                    automatic(&mut document, &mut selected, extension); loaded = true;
                }
                "set" => if action[1] != 1 { set(&mut selected, action[2].as_str().unwrap(), &action[3]); },
                "save" => profiles.put(action[1].as_str().unwrap(), selected.clone()),
                "batch" => batch_mode = action[1].as_bool().unwrap(),
                "load" => {
                    let (_, choice) = profiles.select(action[1].as_str().unwrap());
                    if let Some(choice) = choice { if !batch_mode {
                        document.commit_settings(&Value::Object(choice.settings.to_json())).unwrap();
                        selected = apply_advanced_profile(None, &choice, false).unwrap();
                    } }
                }
                "document" => {
                    let expected = &reference["expected"][index]["document"];
                    editor_support::compare_document_state(&document.snapshot(), &expected["state"], &rows[0]["parameters"]);
                    editor_support::compare_settings(&selected.settings, expected["controls"]["selectedSettingsBits"].as_str().unwrap(), expected["selectedPaletteArgbBits"].as_str().unwrap());
                }
                other => panic!("unknown retained profile action {other}"),
            }
        }
    }
}

#[test]
fn native_advanced_replay_cold_cache_hit_and_observer_ownership() {
    let rows = common::read_ndjson("profile-replay-window-reference.ndjson.gz");
    let reference = rows.iter().find(|r| r["id"] == "default-fast-forward-cache-hit").unwrap();
    let mut document = CoreSession::default().create_document(); load(&mut document, reference, reference["actions"][0][1].as_str().unwrap());
    let profile = default_advanced_profile(); let runtime = PaletteRuntime { backup: vec![0.2, 0.3, 0.4, 1.0], customized: true };
    let before = (profile.clone(), runtime.clone());
    let expected: Vec<_> = reference["expected"].as_array().unwrap().iter().filter(|f| !f["document"].is_null()).collect(); assert_eq!(expected.len(), 2);
    for (repeat, frame) in expected.iter().enumerate() {
        document.commit_settings(&Value::Object(profile.settings.to_json())).unwrap();
        let mut stages = Vec::new();
        let mut observer = |stage: &str, task: &openvector::image_document::TaskResult, profile: &SettingsProfile, runtime: &PaletteRuntime| {
            stages.push(stage.to_owned());
            let mut owned = (task.clone(), profile.clone(), runtime.clone()); owned.0.state.working.bgra.fill(0); owned.1.settings.background.fill(1.0); owned.2.backup.fill(0.0); Ok(())
        };
        let result = run_advanced_profile(&mut document, &profile, &runtime, None, Some(&mut observer)).unwrap().unwrap();
        assert_eq!(stages, ["palette", "segmentation", "smoothing", "fitting"]);
        editor_support::compare_document_state(&document.snapshot(), &frame["document"]["state"], &rows[0]["parameters"]);
        editor_support::compare_json(&result.state.to_json(), &document.snapshot().to_json(), "returned state");
        assert_eq!((&profile, &runtime), (&before.0, &before.1)); assert_eq!(result.profile.color_count, 2); assert!(!result.palette_runtime.customized);
        if repeat == 1 { assert_eq!(result.did_work, ["palette", "segmentation", "smoothing", "fitting"].into_iter().map(|s| (s.to_owned(), false)).collect()); }
    }
    // Cancellation can arrive after the last stage snapshot was produced.
    // Reusing that snapshot must still expose the live shared flag to callers.
    let cancel = document.cancellation_handle();
    let mut observer = |stage: &str, _: &openvector::image_document::TaskResult, _: &SettingsProfile, _: &PaletteRuntime| {
        if stage == "fitting" { cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
        Ok(())
    };
    let result = run_advanced_profile(&mut document, &profile, &runtime, None, Some(&mut observer)).unwrap().unwrap();
    assert!(!result.did_work["fitting"]);
    assert!(result.state.cancelled);
    assert_eq!(result.state.to_json(), document.snapshot().to_json());
}

#[test]
fn native_advanced_replay_preserves_enhance_size_boundary() {
    let rows = common::read_ndjson("tiny-profile-20261020.ndjson.gz");
    for reference in &rows[1..] {
        let input = &reference["input"];
        let decoded = openvector::image_input::decode_image(&common::b64(input["bytes"].as_str().unwrap())).unwrap();
        let filename = openvector::image_input::resolve_image_filename(input["name"].as_str().unwrap());
        let mut document = CoreSession::default().create_document();
        document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, filename.shadow_control as f64).unwrap();
        let original = document.state().original.clone();
        let profile = default_advanced_profile();
        let native_failed = !reference["nativeFailure"].is_null();
        // Repeating a rejected request must not treat its segmentation key as
        // completed. Replaying the saved default reloads its empty palette;
        // native cache hits retain that committed palette, not the selected one.
        for repeat in 0..2 {
            document.commit_settings(&Value::Object(profile.settings.to_json())).unwrap();
            let mut stages = Vec::new();
            let mut observer = |stage: &str, _: &openvector::image_document::TaskResult, _: &SettingsProfile, _: &PaletteRuntime| {
                stages.push(stage.to_owned()); Ok(())
            };
            let result = run_advanced_profile(&mut document, &profile, &PaletteRuntime::default(), None, Some(&mut observer));
            assert_eq!(document.state().original, original);
            if native_failed {
                // Native faults at 0x48ac2e after EnhanceImage produces 0x0.
                // Preserve rejection instead of clamping or skipping the filter.
                assert!(result.is_err(), "{} repeat {repeat}", reference["id"]);
                assert_eq!(stages, ["palette"]);
                let state = document.state();
                assert_eq!(state.source, openvector::image_resize::OwnedImage::default());
                assert_eq!(state.working, openvector::image_resize::OwnedImage::default());
                assert!(state.stage_keys.segmentation.is_empty());
                assert!(state.stage_keys.smoothing.is_empty());
                assert!(state.stage_keys.fitting.is_empty());
                assert!(state.contours.is_none());
                assert!(state.fitting.is_none());
            } else {
                let completed = result.unwrap().unwrap();
                assert_eq!(stages, ["palette", "segmentation", "smoothing", "fitting"]);
                let expected = if repeat == 0 { &reference["expected"] } else { &reference["expectedReplay"] };
                editor_support::compare_document_state(&completed.state, &expected["state"], &rows[0]["parameters"]);
                if repeat == 1 { assert!(completed.did_work.values().all(|worked| !worked)); }
            }
        }
    }
}

#[test]
fn advanced_replay_cancels_each_stage_without_publishing_it_and_retries_retained_source() {
    let rows = common::read_ndjson("profile-replay-window-reference.ndjson.gz");
    let reference = rows.iter().find(|r| r["id"] == "default-fast-forward-cache-hit").unwrap();
    let stages = ["palette", "segmentation", "smoothing", "fitting"];
    for (index, target) in stages.iter().enumerate() {
        let mut document = CoreSession::default().create_document(); load(&mut document, reference, reference["actions"][0][1].as_str().unwrap());
        let original = document.snapshot().original.bgra; let profile = default_advanced_profile();
        document.commit_settings(&Value::Object(profile.settings.to_json())).unwrap();
        let cancel = document.cancellation_handle(); let mut observed = Vec::new(); let mut completed = Vec::new();
        let mut progress = |event: &openvector::image_document::DocumentProgress, _: &openvector::image_document::DocumentSnapshot| { observed.push(event.stage); if event.stage == *target { cancel.store(true, Ordering::Relaxed); } Ok(()) };
        let mut stage = |name: &str, _: &openvector::image_document::TaskResult, _: &SettingsProfile, _: &PaletteRuntime| { completed.push(name.to_owned()); Ok(()) };
        let error = run_advanced_profile(&mut document, &profile, &PaletteRuntime::default(), Some(&mut progress), Some(&mut stage)).unwrap_err();
        assert_eq!(error, CANCEL_TASK); assert!(observed.contains(target)); assert_eq!(completed, stages[..index]);
        assert_eq!(document.snapshot().original.bgra, original);
        let retried = run_advanced_profile(&mut document, &profile, &PaletteRuntime::default(), None, None).unwrap().unwrap();
        assert_eq!(retried.state.status, 0); assert!(!retried.state.stage_keys.fitting.is_empty());
    }
}

#[test]
fn profile_validation_and_atomic_storage_failures_preserve_input_and_owned_entries() {
    let directory = Directory::new(); let path = directory.0.join("profiles.json");
    let mut profiles = SavedAdvancedProfiles::default(); let before = profiles.snapshot();
    assert_eq!(SavedAdvancedProfiles::read_file(&path).unwrap().snapshot(), before);
    for bad in [Value::Null, json!({}), { let mut p = default_advanced_profile().to_json(); p["colorCount"] = Value::Null; p }, { let mut p = default_advanced_profile().to_json(); p["settings"]["complexity"] = json!(0); p }] {
        assert!(profiles.put_json("invalid", &bad).is_err()); assert_eq!(profiles.snapshot(), before);
    }
    for contents in ["{", "null", "[]", "{}", "{\"profiles\":{},\"oldProfiles\":[]}", "{\"profiles\":[{\"name\":\"invalid\",\"profile\":null}]}"] {
        std::fs::write(&path, contents).unwrap(); assert!(SavedAdvancedProfiles::read_file(&path).is_err()); assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
    }
    profiles.write_file(&path).unwrap(); profiles.put("saved", default_advanced_profile()); profiles.write_file(&path).unwrap(); assert_eq!(SavedAdvancedProfiles::read_file(&path).unwrap().len(), 2);
    let target = directory.0.join("directory-target"); std::fs::create_dir(&target).unwrap(); std::fs::write(target.join("keep.txt"), b"keep").unwrap();
    assert!(profiles.write_file(&target).is_err()); assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"keep");
    let mut names: Vec<_> = std::fs::read_dir(&directory.0).unwrap().map(|e| e.unwrap().file_name()).collect(); names.sort();
    assert_eq!(names, ["directory-target", "profiles.json"].map(std::ffi::OsString::from));
    let mut supplied = default_advanced_profile(); supplied.settings.palette = vec![0.1, 0.2, 0.3, 1.0]; profiles.put("owned", supplied.clone());
    let before = profiles.snapshot(); supplied.settings.palette.fill(0.0); supplied.settings.background.fill(1.0);
    let (_, selected) = profiles.select("owned"); selected.unwrap().settings.palette.fill(1.0);
    let mut snapshot = profiles.snapshot(); snapshot[0].name = "changed".into(); snapshot[0].profile.settings.background.fill(1.0); assert_eq!(profiles.snapshot(), before);
    assert_eq!(profiles.select("absent"), (-1, None)); assert_eq!(profiles.snapshot(), before);
    profiles.put(DEFAULT_ADVANCED_PROFILE_NAME, supplied); assert_eq!(profiles.snapshot()[0].profile, default_advanced_profile());
    // Invalid controls must be rejected before a replay can start.
    assert!(read_document_settings(&json!({"complexity":0})).is_err());
}

#[test]
fn profile_rename_collisions_exact_labels_and_builtin_startup_restoration() {
    for (source, destination, expected) in [("a", "c", vec!["b", "c"]), ("c", "a", vec!["a", "b"])] {
        let mut profiles = SavedAdvancedProfiles::default(); profiles.remove(DEFAULT_ADVANCED_PROFILE_NAME);
        for (name, count) in [("a", 3), ("b", 5), ("c", 7)] { let mut profile = default_advanced_profile(); profile.color_count = count; profiles.put(name, profile); }
        let before = profiles.snapshot();
        let index = before.iter().position(|p| p.name == source).unwrap(); let replaced = before.iter().position(|p| p.name == destination).unwrap();
        assert!(profiles.rename(source, destination, false).unwrap_err().contains("Confirm replacement")); assert_eq!(profiles.snapshot(), before);
        let result = profiles.rename(source, destination, true).unwrap();
        assert_eq!((result.index, result.changed, result.replaced_index), (index as isize, true, replaced as isize));
        let after = profiles.snapshot(); assert_eq!(after.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), expected);
        assert_eq!(after.iter().find(|p| p.name == destination).unwrap().profile, before[index].profile);
        assert_eq!(after.iter().find(|p| p.name == "b").unwrap().profile.color_count, 5);
    }
    let mut profiles = SavedAdvancedProfiles::default();
    for name in ["", " ", "Logo", "logo", "é", "e\u{301}"] { profiles.put(name, default_advanced_profile()); }
    let before = profiles.snapshot(); let index = before.iter().position(|p| p.name == "Logo").unwrap();
    for name in ["", "Logo"] { let result = profiles.rename("Logo", name, false).unwrap(); assert_eq!((result.index, result.changed, result.replaced_index), (index as isize, false, -1)); }
    let result = profiles.rename("missing", "renamed", false).unwrap(); assert_eq!((result.index, result.changed, result.replaced_index), (-1, false, -1));
    assert_eq!(profiles.remove("missing"), -1); assert_eq!(profiles.snapshot(), before);
    assert!(profiles.rename("", "not empty", false).unwrap().changed); assert!(profiles.rename("é", "é ", false).unwrap().changed); profiles.remove("e\u{301}");
    assert_eq!(profiles.snapshot().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["é ", "logo", "Logo", " ", "not empty", DEFAULT_ADVANCED_PROFILE_NAME]);

    let directory = Directory::new(); let path = directory.0.join("profiles.json"); let mut profiles = SavedAdvancedProfiles::default();
    let mut custom = default_advanced_profile(); custom.color_count = 17; profiles.put("custom", custom.clone());
    profiles.rename("custom", DEFAULT_ADVANCED_PROFILE_NAME, true).unwrap();
    assert_eq!(profiles.snapshot(), vec![openvector::saved_profiles::SavedProfile { name: DEFAULT_ADVANCED_PROFILE_NAME.into(), profile: custom }]);
    profiles.write_file(&path).unwrap(); assert_eq!(SavedAdvancedProfiles::read_file(&path).unwrap().snapshot(), SavedAdvancedProfiles::default().snapshot());
    profiles.rename(DEFAULT_ADVANCED_PROFILE_NAME, "renamed default", false).unwrap(); profiles.write_file(&path).unwrap();
    let restored = SavedAdvancedProfiles::read_file(&path).unwrap().snapshot();
    assert_eq!(restored.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), [DEFAULT_ADVANCED_PROFILE_NAME, "renamed default"]); assert_eq!(restored[1].profile.color_count, 17);
    assert_eq!(profiles.remove("renamed default"), 0); assert!(profiles.is_empty()); profiles.write_file(&path).unwrap(); assert_eq!(SavedAdvancedProfiles::read_file(&path).unwrap().len(), 1);
}

#[test]
fn native_recent_basic_persistence_matches_retained_registry_rows() {
    // Retained native evidence: each processing-reset row persists
    // num_mcu_settings and its VmQt::VectorizationSettings::basic::<i> records.
    // Real automatic fitting records, Reset Everything's model stage clears and
    // a restart rewrites the same records. Duplicates, the hidden fifth entry
    // and replay are outside this capture.
    let rows = common::read_ndjson("processing-reset-reference.ndjson.gz"); assert_eq!(rows.len(), 7);
    let mut persisted = 0;
    for reference in &rows {
        let id = reference["id"].as_str().unwrap(); let phases = reference["phases"].as_array().unwrap();
        assert!(phases[1..].iter().all(|phase| phase.as_array().unwrap().iter().all(|action| action[0] == "snapshot")), "{id}");
        let directory = Directory::new(); let path = directory.0.join("recent.json");
        let mut document = CoreSession::default().create_document(); let mut recent = RecentBasicProfiles::default();
        for action in phases[0].as_array().unwrap() {
            match action[0].as_str().unwrap() {
                "automatic" => {
                    let extension = load(&mut document, reference, action[1].as_str().unwrap());
                    let mut selected = default_selected_profile(); let (status, did_work) = automatic(&mut document, &mut selected, extension);
                    assert!(recent.record_completion(&selected, status, did_work, false), "{id}");
                }
                "reset-model" => recent.clear(),
                "persist" => recent.write_file(&path).unwrap(),
                // None of the other captured actions completes a single-image fitting.
                "set" | "color" | "open-wide" | "save" | "load" | "batch" | "add-files" | "export-format" | "export-folder" | "snapshot" | "document" => (),
                other => panic!("unknown processing reset action {other}"),
            }
        }
        for (phase, native) in reference["privateValues"].as_array().unwrap().iter().enumerate() {
            // A later phase restarts: the collection reloads and teardown writes it again.
            if phase != 0 { RecentBasicProfiles::read_file(&path).unwrap().write_file(&path).unwrap(); }
            let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let stored = file.as_object().filter(|o| o.len() == 1).and_then(|o| o.get("recentBasicSettings")).and_then(Value::as_array).unwrap();
            assert_eq!(stored.len() as u64, native["/VmQt::MainModel/num_mcu_settings"]["value"].as_u64().unwrap(), "{id} phase {phase}");
            for (index, entry) in stored.iter().enumerate() { editor_support::compare_json(entry, &native_basic_entry(native, index), &format!("{id} phase {phase} basic::{index}")); }
            persisted += stored.len();
        }
    }
    // Only processing-random-reuse completes a fitting after its last reset.
    assert_eq!(persisted, 2);
}

#[test]
fn recent_basic_labels_follow_native_summary_static_prediction() {
    // Static prediction from VectorizationSettings 0x440bb0; no capture holds a recent label.
    for (category, quality, mode, count, expected) in [
        (2, 0, 0, 7, "Artwork, High, Two colors"), (0, 1, 1, 8, "Photograph, Medium, 8 colors"),
        (1, 2, 2, 0, "Anti-aliased artwork, Low, Unlimited colors"), (2, 1, 1, 0, "Artwork, Medium, Fixed colors"),
        (2, 1, 1, -3, "Artwork, Medium, Fixed colors"), (-1, 1, 1, 2, "Medium, 2 colors"), (3, 3, 0, 2, "Two colors"), (2, 0, 3, 2, "Artwork, High, "),
    ] { assert_eq!(basic_profile_label(&summary_profile(category, quality, mode, count)), expected); }
    // Fields outside the summary never change the identity.
    let mut hidden = summary_profile(2, 0, 1, 3); hidden.settings.advanced = true; hidden.settings.complexity = 9; hidden.settings.background_mode = 1;
    hidden.settings.background = [0.5; 4]; hidden.settings.palette = vec![0.1, 0.2, 0.3, 1.0]; hidden.color_count = 9; hidden.basic_color_count = 4; hidden.recommended_background_mode = 1;
    assert_eq!(basic_profile_label(&hidden), "Artwork, High, 3 colors");
}

#[test]
fn recent_basic_list_keeps_five_shows_four_and_keeps_old_duplicates_static_prediction() {
    // Static prediction from addMruSettings 0x42d890 and LinkListWidget::updateView
    // 0x439320; no retained capture shows the hidden fifth entry or a duplicate.
    let completions: Vec<_> = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)].into_iter().map(|(category, quality)| summary_profile(category, quality, 2, 0)).collect();
    let mut recent = RecentBasicProfiles::default();
    for profile in &completions { assert!(recent.record_completion(profile, 0, true, false)); }
    let newest: Vec<_> = completions[1..].iter().rev().map(|profile| RecentBasicProfile { label: basic_profile_label(profile), profile: profile.clone() }).collect();
    assert_eq!(recent.entries(), newest); assert_eq!(recent.visible_entries(), &newest[..4]);
    // The hidden fifth label returns to the front with its OLD complete profile;
    // the incoming fields outside the label are discarded.
    let mut incoming = completions[1].clone(); incoming.settings.advanced = true; incoming.settings.complexity = 9;
    incoming.settings.palette = vec![0.1, 0.2, 0.3, 1.0]; incoming.color_count = 7; incoming.basic_color_count = 4;
    assert!(recent.record_completion(&incoming, 0, true, false));
    let promoted: Vec<_> = [&newest[4]].into_iter().chain(&newest[..4]).cloned().collect();
    assert_eq!(recent.entries(), promoted); assert_eq!(recent.visible_entries()[0].profile, completions[1]);
    // Re-recording the first label is eligible and changes nothing.
    assert!(recent.record_completion(&incoming, 0, true, false)); assert_eq!(recent.entries(), promoted);
    // Cached, failed, cancelled and batch fitting completions are not eligible.
    for (status, did_work, batch) in [(0, false, false), (1, true, false), (2, true, false), (0, true, true)] {
        assert!(!recent.record_completion(&completions[0], status, did_work, batch)); assert_eq!(recent.entries(), promoted);
    }
    recent.clear(); assert!(recent.entries().is_empty() && recent.visible_entries().is_empty());
}

#[test]
fn recent_basic_storage_projects_profiles_and_rebuilds_through_recording_rule_static_prediction() {
    // The per-entry projection is the retained native record (see the registry
    // test). Rebuild order, duplicate handling and trimming are static
    // predictions from readSettings, which replays entries oldest to newest.
    let directory = Directory::new(); let path = directory.0.join("recent.json");
    assert!(RecentBasicProfiles::read_file(&path).unwrap().entries().is_empty());
    let mut fixed = summary_profile(2, 0, 1, 2); fixed.settings.palette = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    fixed.settings.background_mode = 1; fixed.settings.background = [0.25, 0.5, 0.75, 0.0];
    let unlimited = summary_profile(0, 1, 2, 0); let mut recent = RecentBasicProfiles::default();
    for profile in [&fixed, &unlimited] { assert!(recent.record_completion(profile, 0, true, false)); }
    recent.write_file(&path).unwrap();
    let stored = json!([stored_settings_profile(&unlimited), stored_settings_profile(&fixed)]);
    assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(), json!({"recentBasicSettings": stored.clone()}));
    assert_eq!(recent.stored_profiles(), stored);
    // Reloading keeps order and labels; palette colors are gone and the background is opaque.
    let restored = RecentBasicProfiles::read_file(&path).unwrap();
    let projected: Vec<_> = stored.as_array().unwrap().iter().map(|value| { let profile = read_settings_profile(value).unwrap(); RecentBasicProfile { label: basic_profile_label(&profile), profile } }).collect();
    assert_eq!(restored.entries(), projected); assert_eq!(restored.entries()[1].label, recent.entries()[1].label);
    assert!(restored.entries()[1].profile.settings.palette.is_empty()); assert_eq!(restored.entries()[1].profile.settings.background[3], 1.0);
    // Index zero is newest. The older duplicate keeps its profile, and the
    // lowest five indices survive the trim.
    let mut older = summary_profile(2, 0, 0, 2); older.settings.complexity = 9;
    let others: Vec<_> = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1)].into_iter().map(|(category, quality)| summary_profile(category, quality, 2, 0)).collect();
    let stored = [&summary_profile(2, 0, 0, 2), &others[0], &others[1], &others[2], &others[3], &older, &others[4]].map(stored_settings_profile);
    let rebuilt = RecentBasicProfiles::from_stored_profiles(&Value::Array(Vec::from(stored))).unwrap();
    assert_eq!(rebuilt.entries().iter().map(|entry| entry.label.as_str()).collect::<Vec<_>>(), [
        "Artwork, High, Two colors", "Photograph, High, Unlimited colors", "Photograph, Medium, Unlimited colors",
        "Photograph, Low, Unlimited colors", "Anti-aliased artwork, High, Unlimited colors",
    ]);
    assert_eq!(rebuilt.entries()[0].profile.settings.complexity, 9);
    // Invalid content fails without changing the file; a saved advanced profile file is not a recent file.
    for contents in ["{", "null", "[]", "{}", "{\"profiles\":[]}", "{\"recentBasicSettings\":{}}", "{\"recentBasicSettings\":[],\"profiles\":[]}", "{\"recentBasicSettings\":[null]}"] {
        std::fs::write(&path, contents).unwrap(); assert!(RecentBasicProfiles::read_file(&path).is_err()); assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
    }
}

#[test]
fn recent_basic_replay_requests_exact_palette_bounds_without_advanced_zero_fallback_static_prediction() {
    // Static prediction from vectorizeImageWithMruSettings 0x42b560; no retained
    // capture replays a recent entry. Analyzed palette counts expose the bounds.
    let rows = common::read_ndjson("profile-replay-window-reference.ndjson.gz");
    let reference = rows.iter().find(|r| r["id"] == "default-fast-forward-cache-hit").unwrap(); let source = reference["actions"][0][1].as_str().unwrap();
    let analyzed = |state: &DocumentSnapshot| (0..25).filter(|&count| state.completed[count] != 0).collect::<Vec<usize>>();
    let runtime = PaletteRuntime { backup: vec![0.2, 0.3, 0.4, 1.0], customized: true };
    for (mode, expected) in [(0, vec![2]), (1, vec![3]), (2, vec![])] {
        let mut document = CoreSession::default().create_document(); load(&mut document, reference, source);
        // An advanced completion may have recorded the entry; replay keeps every stored field except advanced mode.
        let mut stored = summary_profile(2, 0, mode, 3); stored.settings.advanced = true; stored.settings.complexity = 9; stored.color_count = 5; stored.basic_color_count = 4;
        let before = (stored.clone(), runtime.clone());
        let (result, stages) = replay_basic(&mut document, &stored, &runtime); let result = result.unwrap().unwrap();
        assert_eq!((&stored, &runtime), (&before.0, &before.1)); assert_eq!(analyzed(&result.state), expected, "palette mode {mode}");
        let mut applied = apply_basic_profile(&stored); assert!(!applied.settings.advanced);
        if let Some(&count) = expected.first() {
            // The searched palette is chosen exactly as advanced replay chooses it.
            assert_eq!(stages, ["palette", "segmentation", "smoothing", "fitting"]); assert_eq!(result.profile.settings.palette.len(), count * 4);
            applied.color_count = count as i32; applied.palette_color_count = count as i32; applied.settings.palette = result.profile.settings.palette.clone();
            assert_eq!(result.palette_runtime, PaletteRuntime { backup: applied.settings.palette.clone(), customized: false });
        } else {
            // Without a search, the caller's palette revert state is untouched.
            assert_eq!(stages, ["segmentation", "smoothing", "fitting"]); assert_eq!(result.palette_runtime, runtime);
        }
        assert_eq!(result.profile, applied); assert_eq!(result.palette_requested, mode != 2);
        assert_eq!(result.state.status, 0); assert!(result.did_work["fitting"]);
    }
    // Auto-detect applies settings but processes nothing. A fixed palette without
    // a count forwards 0..0, which the palette task rejects before changing the
    // document; advanced replay of the same profile keeps its 2..12 fallback.
    let mut document = CoreSession::default().create_document(); load(&mut document, reference, source);
    let (auto, stages) = replay_basic(&mut document, &summary_profile(2, 0, 3, 0), &runtime);
    assert!(auto.unwrap().is_none()); assert!(stages.is_empty());
    let (zero, stages) = replay_basic(&mut document, &summary_profile(2, 0, 1, 0), &runtime);
    assert_eq!(zero.unwrap_err(), "Document lower must be an integer from 2 through 24."); assert!(stages.is_empty());
    assert!(analyzed(document.state()).is_empty()); assert!(document.state().stage_keys.palette.is_empty() && document.state().stage_keys.segmentation.is_empty());
    let advanced = run_advanced_profile(&mut document, &summary_profile(2, 0, 1, 0), &runtime, None, None).unwrap().unwrap();
    assert_eq!(analyzed(&advanced.state), (2..=12).collect::<Vec<usize>>());
}

#[test]
fn recent_basic_replay_records_only_completed_fitting_and_keeps_the_old_entry_static_prediction() {
    // Static prediction: a replay's fitting completion records the current
    // settings, which reproduce the clicked label, so that entry returns to the
    // front with its stored object. Cancelled and cached fitting do not record.
    let rows = common::read_ndjson("profile-replay-window-reference.ndjson.gz");
    let reference = rows.iter().find(|r| r["id"] == "default-fast-forward-cache-hit").unwrap();
    let mut document = CoreSession::default().create_document(); load(&mut document, reference, reference["actions"][0][1].as_str().unwrap());
    // An advanced completion recorded this entry; its label ignores advanced controls.
    let mut stored = summary_profile(2, 0, 2, 0); stored.settings.advanced = true; stored.settings.complexity = 9;
    let mut recent = RecentBasicProfiles::default();
    for profile in [&stored, &summary_profile(0, 1, 2, 0)] { assert!(recent.record_completion(profile, 0, true, false)); }
    let before = recent.entries().to_vec();
    let (cancelled, completion) = replay_fitting(&mut document, &stored, true);
    assert_eq!(cancelled.unwrap_err(), CANCEL_TASK); assert_eq!(completion, None);
    let (completed, completion) = replay_fitting(&mut document, &stored, false);
    let completed = completed.unwrap().unwrap(); let (status, did_work) = completion.unwrap();
    assert!(!completed.profile.settings.advanced); assert!(recent.record_completion(&completed.profile, status, did_work, false));
    assert_eq!(recent.entries(), [before[1].clone(), before[0].clone()]); assert_eq!(recent.entries()[0].profile, stored);
    // Replaying again reuses every cached stage, so its fitting cannot record.
    let promoted = recent.entries().to_vec();
    let (cached, completion) = replay_fitting(&mut document, &stored, false);
    let cached = cached.unwrap().unwrap(); let (status, did_work) = completion.unwrap();
    assert_eq!((status, did_work), (0, false)); assert!(!recent.record_completion(&cached.profile, status, did_work, false)); assert_eq!(recent.entries(), promoted);
}
