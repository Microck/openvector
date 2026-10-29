#[path = "../../openvector/tests/common/mod.rs"] mod common;
use openvector::{document_controls::DocumentSettings, saved_profiles::{RecentBasicProfiles, SavedAdvancedProfiles}, settings_profile::*};
use serde_json::{json, Value};
use std::{io::Write, path::{Path, PathBuf}, process::{Command, Output, Stdio}, sync::atomic::{AtomicU64, Ordering}, time::Instant};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-cli-native-profiles-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn invoke(root: &Path, args: &[String], input: Option<&[u8]>, early_rejection: bool) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openvector")).current_dir(root).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let written = if let Some(input) = input { child.stdin.take().unwrap().write_all(input) } else { drop(child.stdin.take()); Ok(()) };
    let output = child.wait_with_output().unwrap();
    if let Err(error) = written {
        assert!(early_rejection, "stdin failure: {error}; stderr {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    output
}
fn args(values: &[&str]) -> Vec<String> { values.iter().map(|s| (*s).to_owned()).collect() }
fn success(output: &Output) { assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr)); assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr)); }
fn failure(output: &Output) { assert_eq!(output.status.code(), Some(1)); assert!(output.stdout.is_empty()); assert!(!output.stderr.is_empty()); }
fn corpus(name: &str) -> Vec<Value> { common::read_ndjson(&format!("{name}-reference.ndjson.gz")) }
fn input(reference: &Value) -> Vec<u8> {
    let open = reference["actions"].as_array().unwrap().iter().find(|a| a[0] == "open").unwrap();
    common::b64(reference["inputs"][open[1].as_str().unwrap()].as_str().unwrap())
}
fn expected_svg(reference: &Value) -> Vec<u8> {
    let state = &reference["expected"].as_array().unwrap().iter().rev().find(|f| !f["document"].is_null()).unwrap()["document"]["state"];
    let geometry = openvector::contour_fitting::FittedGeometry::from_json(&state["fittedGeometry"]).unwrap();
    let dimensions = openvector::image_input::decode_image(&input(reference)).unwrap();
    let mut vector = openvector::vector_output::VectorOutputInput::new(&geometry, dimensions.width as i64, dimensions.height as i64);
    vector.layout = openvector::export_options::Layout::GroupedCutouts;
    openvector::svg::export_svg(&vector).unwrap().into_bytes()
}
fn stored_names(path: &Path) -> Vec<String> {
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    value["profiles"].as_array().unwrap().iter().map(|entry| entry["name"].as_str().unwrap().to_owned()).collect()
}

#[test]
fn profile_vectorize_matches_complete_native_fast_forward_svg_and_persisted_order() {
    let rows = corpus("profile-replay-window");
    for (id, name, mode, selected) in [
        ("visible-window-default-fast-forward", DEFAULT_ADVANCED_PROFILE_NAME, 1, false),
        ("two-count-seven-fast-forward", "two-seven", 0, false),
        ("fixed-empty-count-fast-forward", "empty-count", 1, true),
        ("flexible-count-seven-fast-forward", "flexible", 2, false),
    ] {
        let directory = Directory::new(); let root = &directory.0; let path = root.join("profiles.json");
        let reference = rows.iter().find(|r| r["id"] == id).unwrap(); std::fs::write(root.join("patchwork.bmp"), input(reference)).unwrap();
        let mut profile = if selected { default_selected_profile() } else { default_advanced_profile() };
        profile.settings.advanced = true; profile.settings.palette_mode = mode;
        if name != DEFAULT_ADVANCED_PROFILE_NAME { profile.color_count = 7; }
        let mut profiles = SavedAdvancedProfiles::default(); profiles.put(name, profile); profiles.write_file(&path).unwrap();
        let output = invoke(root, &args(&["vectorize", "patchwork.bmp", "--profile", name, "--profiles-file", "profiles.json"]), None, false);
        success(&output); assert_eq!(output.stdout, expected_svg(reference), "{id}");
        let expected = if name == DEFAULT_ADVANCED_PROFILE_NAME { vec![name] } else { vec![name, DEFAULT_ADVANCED_PROFILE_NAME] };
        assert_eq!(stored_names(&path), expected, "persisted MRU and remaining order");
    }
}

fn fixture_path(root: &Path, relative: &str) -> PathBuf {
    // Fixture names use '/', but captured inputs use native separators. Queue identity preserves the supplied string.
    root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
}
fn batch_context(reference: &Value) -> (Directory, Vec<String>, PathBuf) {
    let directory = Directory::new(); let mut filenames = Vec::new();
    for source in reference["sources"].as_array().unwrap() {
        let path = fixture_path(&directory.0, source["name"].as_str().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap(); std::fs::write(&path, common::b64(source["bytes"].as_str().unwrap())).unwrap();
        filenames.push(path.to_str().unwrap().to_owned());
    }
    let output = fixture_path(&directory.0, reference["exportDirectory"].as_str().unwrap());
    if reference["directoryExists"].as_bool().unwrap() { std::fs::create_dir_all(&output).unwrap(); }
    for (name, bytes) in reference["initialOutputs"].as_object().unwrap() { std::fs::write(output.join(name), common::b64(bytes.as_str().unwrap())).unwrap(); }
    (directory, filenames, output)
}
fn normalized_log(log: &str, elapsed: Option<u128>) -> String {
    log.split('\n').map(|line| {
        if let Some((label, value)) = line.split_once(": ") {
            if ["Start time", "End time"].contains(&label) {
                let parts: Vec<_> = value.split(':').collect(); assert_eq!(parts.len(), 3);
                for (i, part) in parts.iter().enumerate() { assert_eq!(part.len(), 2); assert!(part.bytes().all(|b| b.is_ascii_digit())); assert!(part.parse::<u32>().unwrap() < if i == 0 { 24 } else { 60 }); }
                return format!("{label}: <clock>");
            }
            if ["Done classifying", "Done finding palette", "Done segmenting", "Done smoothing", "Done fitting", "Elapsed time"].contains(&label) {
                let (minutes, rest) = value.split_once(':').unwrap(); let (seconds, millis) = rest.split_once('.').unwrap();
                assert!(!minutes.is_empty() && minutes.bytes().all(|b| b.is_ascii_digit())); assert_eq!(seconds.len(), 2); assert_eq!(millis.len(), 3);
                assert!(seconds.bytes().all(|b| b.is_ascii_digit()) && millis.bytes().all(|b| b.is_ascii_digit()));
                let seconds = seconds.parse::<u128>().unwrap(); assert!(seconds < 60);
                let duration = minutes.parse::<u128>().unwrap() * 60000 + seconds * 1000 + millis.parse::<u128>().unwrap();
                if let Some(elapsed) = elapsed { assert!(duration <= elapsed + 1000, "reported duration exceeds execution wall time"); }
                return format!("{label}: <duration>");
            }
        }
        line.to_owned()
    }).collect::<Vec<_>>().join("\n")
}
fn assert_batch(actual: &Value, reference: &Value, prefix: &str, root: &Path, output: &Path, elapsed: u128) {
    let native = &reference["expected"]; let log = native["processing"]["log"].as_str().unwrap();
    let relocate = |path: &str| path.replace(prefix, root.to_str().unwrap()).replace(&prefix.replace('\\', "/"), &root.to_str().unwrap().replace('\\', "/")).replace('\\', std::path::MAIN_SEPARATOR_STR);
    let status = match native["processing"]["wizardPage"].as_i64().unwrap() { 1 => "empty", 2 => "cancelled", 0 => "no-files", 3 => if reference["decision"] == "cancel" { "cancelled" } else { "complete" }, other => panic!("unsettled native wizard page {other}") };
    assert_eq!(actual["status"], status);
    assert_eq!(actual["queue"]["processingIndex"].as_i64().unwrap() as u32 as u64, native["processingIndex"].as_u64().unwrap());
    let queue = native["queue"].as_array().unwrap();
    let flags: Vec<_> = actual["queue"]["queue"].as_array().unwrap().iter().map(|r| r["shouldProcess"].as_bool().unwrap()).collect();
    assert_eq!(flags, queue.iter().map(|r| r["initializedFlags"][0].as_i64().unwrap() != 0).collect::<Vec<_>>());
    assert_eq!(normalized_log(actual["log"].as_str().unwrap(), Some(elapsed)), normalized_log(&relocate(log), None), "complete native processing log");
    let failed = log.split('\n').filter(|line| ["Failed To Open File.", "Failed To Save File."].contains(line)).count();
    assert_eq!(actual["failed"], json!(failed)); assert_eq!(actual["converted"], reference["expectedConverted"]);
    assert_eq!(actual["skipped"], json!(queue.iter().filter(|r| r["initializedFlags"][0] == 0).count()));
    let files = actual["files"].as_array().unwrap();
    assert_eq!(files.iter().map(|f| f["outputFilename"].as_str().unwrap().to_owned()).collect::<Vec<_>>(), native["processing"]["outputs"].as_array().unwrap().iter().map(|o| relocate(o[0].as_str().unwrap())).collect::<Vec<_>>());
    assert_eq!(files.iter().map(|f| f["filename"].as_str().unwrap().to_owned()).collect::<Vec<_>>(), queue.iter().map(|r| relocate(r["filename"].as_str().unwrap())).collect::<Vec<_>>());
    if reference["directoryExists"].as_bool().unwrap() {
        let mut listed: Vec<_> = std::fs::read_dir(output).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect(); listed.sort();
        let mut expected: Vec<_> = reference["expectedOutputs"].as_object().unwrap().keys().cloned().collect(); expected.sort(); assert_eq!(listed, expected);
        for (name, bytes) in reference["expectedOutputs"].as_object().unwrap() { assert_eq!(std::fs::read(output.join(name)).unwrap(), common::b64(bytes.as_str().unwrap()), "output {name}"); }
    } else { assert!(!output.exists()); }
}

#[test]
fn saved_profile_batch_matches_all_native_cold_files_logs_and_storage() {
    let rows = corpus("profile-conversion-batch"); assert_eq!(rows.len(), 5);
    for reference in &rows[1..] {
        let (directory, filenames, output) = batch_context(reference); let root = &directory.0;
        let mut profiles = SavedAdvancedProfiles::default(); let mut selected = default_selected_profile();
        for action in reference["before"].as_array().unwrap() {
            match action[0].as_str().unwrap() {
                "set" => { assert_eq!(action[1], 2); let value = action[3].as_i64().unwrap() as i32; match action[2].as_str().unwrap() {
                    "setAdvNumColors(int)" => selected.color_count = value,
                    "setPaletteMode(int)" => selected.settings.palette_mode = value,
                    "setAdvSegmentationComplexity(int)" => selected.settings.complexity = value,
                    "setAdvContourSmoothness(int)" => selected.settings.smoothness = value,
                    other => panic!("unknown profile input {other}"),
                } }
                "save" => profiles.put(action[1].as_str().unwrap(), selected.clone()),
                "batch" | "load" => (),
                other => panic!("unknown native batch setup {other}"),
            }
        }
        profiles.write_file(root.join("profiles.json")).unwrap();
        let settings = br#"{"advanced":false,"category":3,"quality":3,"paletteMode":3,"colorCount":0,"backgroundMode":1,"complexity":1,"smoothness":1}"#;
        std::fs::write(root.join("batch.json"), settings).unwrap();
        let mut command = args(&["batch"]); command.extend(filenames); command.extend(args(&["--output-dir", output.to_str().unwrap(), "--format", "svg", "--settings", "batch.json", "--profile", "saved", "--profiles-file", "profiles.json"]));
        let began = Instant::now(); let completed = invoke(root, &command, None, false); let elapsed = began.elapsed().as_millis(); success(&completed);
        assert_batch(&serde_json::from_slice(&completed.stdout).unwrap(), reference, rows[0]["evidencePrefix"].as_str().unwrap(), root, &output, elapsed);
        assert_eq!(std::fs::read(root.join("batch.json")).unwrap(), settings);
        assert_eq!(stored_names(&root.join("profiles.json")), ["saved", DEFAULT_ADVANCED_PROFILE_NAME]);
    }
}

#[test]
fn supplied_batch_profile_colors_follow_real_automatic_controller_and_native_outputs() {
    let rows = corpus("batch-profiles");
    for (id, mode) in [("custom-two", 0), ("custom-fixed", 1), ("custom-flexible", 2)] {
        let reference = rows.iter().find(|r| r["id"] == id).unwrap(); let (directory, filenames, output) = batch_context(reference); let root = &directory.0;
        let source = reference["before"][0][1].as_str().unwrap(); let decoded = openvector::image_input::decode_image(&common::b64(reference["inputs"][source].as_str().unwrap())).unwrap();
        let filename = openvector::image_input::resolve_image_filename(source); let mut document = openvector::palette::CoreSession::default().create_document();
        document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, filename.shadow_control as f64).unwrap();
        let mut profile = default_selected_profile(); let mut controls = Value::Object(profile.settings.to_json()); controls["extension"] = json!(filename.extension);
        let automatic = document.run_automatic(&controls).unwrap(); assert_eq!(automatic.palette_count, Some(2));
        profile.settings = automatic.settings; profile.settings.complexity = 9; profile.settings.smoothness = 10; profile.settings.palette_mode = mode; profile.color_count = 7; profile.palette_color_count = 2;
        let requested = apply_advanced_profile(Some(&default_batch_profile()), &profile, true).unwrap();
        let bytes = serde_json::to_vec(&batch_profile_controls(&requested)).unwrap(); std::fs::write(root.join("settings.json"), &bytes).unwrap();
        let mut command = args(&["batch"]); command.extend(filenames); command.extend(args(&["--settings", "settings.json", "--format", "svg", "--output-dir", output.to_str().unwrap()]));
        let began = Instant::now(); let completed = invoke(root, &command, None, false); let elapsed = began.elapsed().as_millis(); success(&completed);
        assert_batch(&serde_json::from_slice(&completed.stdout).unwrap(), reference, rows[0]["evidencePrefix"].as_str().unwrap(), root, &output, elapsed);
        assert_eq!(std::fs::read(root.join("settings.json")).unwrap(), bytes);
    }
}

#[test]
fn conversion_profile_admission_failures_do_not_write_storage_or_outputs() {
    let directory = Directory::new(); let root = &directory.0; let path = root.join("profiles.json");
    let mut profiles = SavedAdvancedProfiles::default(); profiles.put("saved", default_advanced_profile()); profiles.write_file(&path).unwrap();
    std::fs::write(root.join("invalid.bmp"), b"invalid").unwrap(); let before = std::fs::read(&path).unwrap();
    for command in [
        args(&["vectorize", "invalid.bmp", "--profile", "saved"]),
        args(&["vectorize", "invalid.bmp", "--profiles-file", "profiles.json"]),
        args(&["vectorize", "invalid.bmp", "--profile", "saved", "--profiles-file", "-"]),
        args(&["batch", "invalid.bmp", "--profile", "saved"]),
        args(&["batch", "invalid.bmp", "--profiles-file", "profiles.json"]),
        args(&["vectorize", "invalid.bmp", "--profile", "missing", "--profiles-file", "profiles.json"]),
        args(&["vectorize", "invalid.bmp", "--profile", "saved", "--profiles-file", "profiles.json"]),
    ] {
        failure(&invoke(root, &command, None, false)); assert_eq!(std::fs::read(&path).unwrap(), before); assert!(!root.join("result.svg").exists());
    }
}

#[test]
fn exact_empty_profile_name_binary_stdin_edit_plan_and_storage_output_protection() {
    let directory = Directory::new(); let root = &directory.0; let path = root.join("profiles.json");
    let mut profiles = SavedAdvancedProfiles::default(); profiles.put("", default_advanced_profile()); profiles.write_file(&path).unwrap();
    let rows = corpus("profile-replay-window"); let reference = rows.iter().find(|r| r["id"] == "visible-window-default-fast-forward").unwrap();
    let bytes = input(reference); std::fs::write(root.join("edits.json"), b"[]").unwrap();
    let completed = invoke(root, &args(&["vectorize", "-", "--filename", "patchwork.bmp", "--profile", "", "--profiles-file", "profiles.json", "--edits", "edits.json"]), Some(&bytes), false);
    success(&completed); assert_eq!(completed.stdout, expected_svg(reference)); assert_eq!(stored_names(&path), ["", DEFAULT_ADVANCED_PROFILE_NAME]);
    let before = std::fs::read(&path).unwrap();
    failure(&invoke(root, &args(&["vectorize", "-", "--filename", "patchwork.bmp", "--profile", "", "--profiles-file", "profiles.json", "--output", "profiles.json", "--format", "svg", "--force"]), Some(&bytes), true));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn auto_detect_profile_selects_and_persists_without_silently_running_automatic_conversion() {
    let directory = Directory::new(); let root = &directory.0; let path = root.join("profiles.json");
    let mut profiles = SavedAdvancedProfiles::default(); let mut profile = default_advanced_profile(); profile.settings.palette_mode = 3; profiles.put("auto", profile); profiles.write_file(&path).unwrap();
    let rows = corpus("profile-replay-window"); let reference = rows.iter().find(|r| r["id"] == "auto-mode-fast-forward").unwrap();
    let completed = invoke(root, &args(&["vectorize", "-", "--filename", "patchwork.bmp", "--profile", "auto", "--profiles-file", "profiles.json", "--output", "output.svg"]), Some(&input(reference)), true);
    failure(&completed); assert!(String::from_utf8_lossy(&completed.stderr).contains("Fast Forward does not process")); assert!(!root.join("output.svg").exists());
    assert_eq!(stored_names(&path), ["auto", DEFAULT_ADVANCED_PROFILE_NAME]);
}

fn reset_reference() -> Value { corpus("processing-reset").into_iter().find(|r| r["id"] == "processing-random-reuse").unwrap() }
/// Source of the last automatic run in the persisted native phase.
fn reset_input(reference: &Value) -> Vec<u8> {
    let source = reference["phases"][0].as_array().unwrap().iter().rev().find(|a| a[0] == "automatic").unwrap()[1].as_str().unwrap();
    common::b64(reference["inputs"][source].as_str().unwrap())
}
fn recent_list(root: &Path) -> Vec<Value> {
    let output = invoke(root, &args(&["recent-settings", "list", "--file", "recent.json"]), None, false); success(&output);
    serde_json::from_slice::<Value>(&output.stdout).unwrap().as_array().unwrap().clone()
}
fn labels(entries: &[Value]) -> Vec<&str> { entries.iter().map(|e| e["label"].as_str().unwrap()).collect() }

#[test]
fn recent_vectorize_records_the_native_persisted_automatic_entry() {
    // Native: automatic, Reset Everything, automatic on this source, clean teardown.
    let reference = reset_reference(); let registry = &reference["privateValues"][0];
    assert_eq!(registry["/VmQt::MainModel/num_mcu_settings"]["value"], 1);
    let value = |key: &str| registry[format!("/VmQt::VectorizationSettings::basic::0/{key}")]["value"].clone();
    let integer = |key: &str| value(key).as_u64().unwrap() as u32 as i32;
    let flag = |key: &str| value(key) == "true";
    // QColor::rgba() of white; loading forces alpha 255, as for saved profiles.
    assert_eq!(value("flatten_image_color"), "16777215");
    let native = SettingsProfile {
        settings: DocumentSettings { category: integer("image_category"), quality: integer("image_quality"), sensitivity: integer("adv_color_sensitivity"),
            advanced: flag("use_advanced_mode"), anti_alias: flag("adv_use_palette_aa"), background_mode: integer("flatten_image_mode"), background: [1.0; 4],
            complexity: integer("adv_segmentation_complexity"), minimum_pixels: integer("adv_min_num_pixels"), artifact_rejection: integer("adv_anti_aliasing_rejection"),
            cluster_colors: flag("adv_cluster_colors"), palette_mode: integer("palette_mode"), palette: Vec::new(), smoothness: integer("adv_contour_smoothness"),
            detect_corners: integer("adv_detect_sharp_corners") != 0, curve_complexity: integer("adv_curve_complexity"), smoothing_anti_alias: flag("adv_use_contour_aa") },
        color_count: integer("adv_num_colors"), basic_color_count: integer("batch_num_colors"), palette_color_count: integer("num_palette_colors"),
        recommended_background_mode: integer("recommended_flatten_image_mode"),
    };
    let directory = Directory::new(); let root = &directory.0; let path = root.join("recent.json");
    std::fs::write(root.join("image.bmp"), reset_input(&reference)).unwrap();
    assert!(recent_list(root).is_empty()); assert!(!path.exists());
    success(&invoke(root, &args(&["vectorize", "image.bmp", "--recent-file", "recent.json"]), None, false));
    let stored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored, json!({"recentBasicSettings": [stored_settings_profile(&native)]}), "native persisted record");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(recent_list(root), [json!({"label": "Artwork, High, Two colors", "visible": true, "profile": native.to_json()})]);
    let shown = invoke(root, &args(&["recent-settings", "show", "--file", "recent.json", "--index", "0"]), None, false); success(&shown);
    assert_eq!(serde_json::from_slice::<Value>(&shown.stdout).unwrap(), native.to_json()); assert_eq!(std::fs::read(&path).unwrap(), before);
    for removed in [1, 0] {
        let cleared = invoke(root, &args(&["recent-settings", "clear", "--file", "recent.json"]), None, false); success(&cleared);
        assert_eq!(serde_json::from_slice::<Value>(&cleared.stdout).unwrap(), json!({"removed": removed}));
        assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(), json!({"recentBasicSettings": []}));
    }
    let cleared = invoke(root, &args(&["recent-settings", "clear", "--file", "missing.json"]), None, false); success(&cleared);
    assert_eq!(serde_json::from_slice::<Value>(&cleared.stdout).unwrap(), json!({"removed": 0})); assert!(!root.join("missing.json").exists());
}

#[test]
fn recent_records_each_fitting_that_works_and_replay_keeps_the_older_duplicate() {
    let directory = Directory::new(); let root = &directory.0;
    std::fs::write(root.join("image.bmp"), reset_input(&reset_reference())).unwrap();
    let mut saved = default_advanced_profile(); saved.settings.category = 2; saved.settings.quality = 0; saved.settings.complexity = 9; saved.settings.smoothness = 10;
    let mut profiles = SavedAdvancedProfiles::default(); profiles.put("saved", saved); profiles.write_file(root.join("profiles.json")).unwrap();
    // Advanced cache keys omit category, so this relabelled fitting is cached and records nothing.
    std::fs::write(root.join("cached.json"), br#"[{"settings":{"category":0}},{"action":"process","stage":"fitting"}]"#).unwrap();
    success(&invoke(root, &args(&["vectorize", "image.bmp", "--profile", "saved", "--profiles-file", "profiles.json", "--recent-file", "recent.json", "--edits", "cached.json"]), None, false));
    assert_eq!(labels(&recent_list(root)), ["Artwork, High, 2 colors"]);
    // Basic cache keys include category and quality, so the fitting action and edit finish both
    // record. Reopening the source records its automatic label again, moving it first.
    std::fs::write(root.join("refit.json"), br#"[{"settings":{"category":0}},{"action":"process","stage":"fitting"},{"settings":{"quality":2}},{"action":"finish"},{"source":"image.bmp"}]"#).unwrap();
    success(&invoke(root, &args(&["vectorize", "image.bmp", "--recent-file", "recent.json", "--edits", "refit.json"]), None, false));
    let entries = recent_list(root);
    assert_eq!(labels(&entries), ["Artwork, High, Two colors", "Photograph, Low, Two colors", "Photograph, High, Two colors", "Artwork, High, 2 colors"]);
    assert_eq!(entries[3]["profile"]["settings"]["advanced"], true);
    // Replay processes basic settings; its matching label moves the older advanced entry first unchanged.
    success(&invoke(root, &args(&["vectorize", "image.bmp", "--recent-file", "recent.json", "--recent-index", "3"]), None, false));
    let replayed = recent_list(root);
    assert_eq!(replayed[0], entries[3]); assert_eq!(replayed[1..], entries[..3]);
}

#[test]
fn recent_rejections_and_failed_replays_leave_storage_and_outputs_unchanged() {
    let directory = Directory::new(); let root = &directory.0; let path = root.join("recent.json");
    std::fs::write(root.join("image.bmp"), reset_input(&reset_reference())).unwrap();
    let mut profiles = SavedAdvancedProfiles::default(); profiles.put("saved", default_advanced_profile()); profiles.write_file(root.join("profiles.json")).unwrap();
    // Oldest first: three unlimited-color entries, an auto-detect palette mode that
    // replay does not process, and fixed colors without a stored count.
    let mut recent = RecentBasicProfiles::default();
    for (category, quality, mode) in [(0, 0, 2), (1, 1, 2), (2, 2, 2), (2, 0, 3), (2, 0, 1)] {
        let mut profile = default_selected_profile(); profile.settings.category = category; profile.settings.quality = quality; profile.settings.palette_mode = mode; profile.palette_color_count = 0;
        assert!(recent.record_completion(&profile, 0, true, false));
    }
    recent.write_file(&path).unwrap();
    let (stored, before) = (std::fs::read(root.join("profiles.json")).unwrap(), std::fs::read(&path).unwrap());
    let vectorize = |extra: &[&str]| { let mut command = args(&["vectorize", "image.bmp", "-o", "out.svg"]); command.extend(args(extra)); command };
    for (command, diagnostic) in [
        (vectorize(&["--recent-index", "0"]), "--recent-index requires --recent-file"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "0", "--profile", "saved", "--profiles-file", "profiles.json"]), "not both"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "-1"]), "zero-based integer"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "+1"]), "zero-based integer"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "1.0"]), "zero-based integer"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index="]), "zero-based integer"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "4"]), "hidden"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "5"]), "not found"),
        (vectorize(&["--recent-file", "-"]), "--recent-file must name"),
        (vectorize(&["--recent-file", "profiles.json", "--profile", "saved", "--profiles-file", "profiles.json"]), "different files"),
        (args(&["vectorize", "image.bmp", "--recent-file", "recent.json", "-o", "recent.json", "--format", "svg", "--force"]), "different files"),
        // Fixed bounds 0..0 reach the palette task unchanged and fail there.
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "0"]), "lower must be an integer from 2 through 24"),
        (vectorize(&["--recent-file", "recent.json", "--recent-index", "1"]), "does not process"),
        (args(&["recent-settings", "show", "--file", "recent.json"]), "requires --index"),
        (args(&["recent-settings", "list", "--file", "recent.json", "--index", "0"]), "only valid with recent-settings show"),
        (args(&["recent-settings", "show", "--file", "recent.json", "--index", "5"]), "not found"),
        (args(&["recent-settings", "clear", "--file", "-"]), "--file must name"),
    ] {
        let output = invoke(root, &command, None, false); failure(&output);
        assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic), "{command:?}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(std::fs::read(&path).unwrap(), before, "{command:?}"); assert_eq!(std::fs::read(root.join("profiles.json")).unwrap(), stored, "{command:?}");
        assert!(!root.join("out.svg").exists(), "{command:?}");
    }
    let entries = recent_list(root);
    assert_eq!(entries.iter().map(|e| e["visible"].as_bool().unwrap()).collect::<Vec<_>>(), [true, true, true, true, false]);
    let shown = invoke(root, &args(&["recent-settings", "show", "--file", "recent.json", "--index", "4"]), None, false); success(&shown);
    assert_eq!(serde_json::from_slice::<Value>(&shown.stdout).unwrap(), entries[4]["profile"]);
}
