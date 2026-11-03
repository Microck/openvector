use super::*;
use base64::Engine;
use std::io::Read;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-reset-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn fixture(name: &str) -> PathBuf {
    let base = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    base.join(name)
}
fn references(name: &str) -> Vec<Value> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(fixture(name)).unwrap()).read_to_string(&mut text).unwrap();
    text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}
fn inputs(directory: &Directory) -> HashMap<String, PathBuf> {
    let encoded: Value = serde_json::from_slice(&std::fs::read(fixture("application-model-reset-inputs.json")).unwrap()).unwrap();
    encoded.as_object().unwrap().iter().enumerate().map(|(index, (name, bytes))| {
        let path = directory.0.join(format!("source-{index}.bmp"));
        std::fs::write(&path, base64::engine::general_purpose::STANDARD.decode(bytes.as_str().unwrap()).unwrap()).unwrap();
        (name.clone(), path)
    }).collect()
}
fn hex(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    (0..text.len()).step_by(2).map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).unwrap()).collect()
}
// Native 92-byte settings frame, as decoded by test/helpers/native-gui.mjs.
fn profile(frame: &Value) -> SettingsProfile {
    let bits = hex(&frame["settingsBits"]); let palette = hex(&frame["paletteArgbBits"]);
    let integer = |offset| i32::from_le_bytes(bits[offset..offset + 4].try_into().unwrap());
    let float = |offset| f32::from_le_bytes(bits[offset..offset + 4].try_into().unwrap());
    SettingsProfile {
        settings: DocumentSettings {
            category: integer(0), quality: integer(4), sensitivity: integer(12), anti_alias: integer(16) != 0,
            complexity: integer(20), minimum_pixels: integer(24), artifact_rejection: integer(28), cluster_colors: integer(32) != 0,
            smoothness: integer(36), detect_corners: integer(40) != 0, curve_complexity: integer(44), smoothing_anti_alias: integer(48) != 0,
            advanced: integer(52) != 0, palette_mode: integer(60), background_mode: integer(68), background: [float(84), float(80), float(76), float(72)],
            palette: palette.chunks_exact(16).flat_map(|color| color.chunks_exact(4).rev().map(|channel| f32::from_le_bytes(channel.try_into().unwrap()))).collect(),
        },
        color_count: integer(8), basic_color_count: integer(64), palette_color_count: integer(56), recommended_background_mode: integer(88),
    }
}
fn settle(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        app.poll(cx);
        if !app.busy && app.pending.is_empty() { break; }
        assert!(Instant::now() < deadline, "worker did not settle: {}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn confirm_reset(app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>) {
    app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
    assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::ResetApplicationAccepted)))));
    assert!(app.confirmation_cancel.is_focused(window));
    app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx);
    settle(app, cx);
}
fn assert_lists(app: &OpenVectorApp, expected: &Value) {
    assert_eq!(app.workflows.profiles.snapshot().iter().map(|entry| (entry.name.clone(), entry.profile.clone())).collect::<Vec<_>>(),
        expected["profiles"].as_array().unwrap().iter().map(|entry| (entry["name"].as_str().unwrap().to_owned(), profile(entry))).collect::<Vec<_>>());
    assert_eq!(app.preferences.sources.snapshot()["files"], expected["sourceHistory"]["files"]);
    assert_eq!(app.preferences.directories.snapshot(), expected["sourceDirectories"]);
}

// The legacy corpus captures model resets, not the full-window handler. Its
// altered-widget cases below retain static predictions from handler 0x451750.
// The decoded full-window test also covers genuine broader nondefault captures.

/// Stored owners after a confirmed reset. Both owners of each native pair share
/// one prediction, so the unmapped basic/advanced order of their offsets cannot
/// matter. The empty folder argument selects the canonical working directory.
fn predicted_window_preferences() -> Value {
    let directory = std::env::current_dir().unwrap().canonicalize().unwrap();
    let save = |format: &str| { let choice = json!({"format":format,"directory":directory,"expanded":false}); json!({"basic":choice,"advanced":choice}) };
    let drag = json!({"format":"ai","minimize":true});
    json!({"skipReview":false,"autoUpdate":[true,true,true],"quickSave":save("ai"),"bitmapSave":save("png"),"bitmapScale":1.0,"dragResult":{"basic":drag,"advanced":drag}})
}
/// The stored owners that `predicted_window_preferences` covers.
fn window_preferences(preferences: &Preferences) -> Value {
    json!({"skipReview":preferences.skip_review,"autoUpdate":[preferences.auto_palette,preferences.auto_segmentation,preferences.auto_vectorization],
        "quickSave":preferences.quick_save.preferences(),"bitmapSave":preferences.bitmap_save.preferences(),
        "bitmapScale":preferences.bitmap_scale,"dragResult":preferences.drag_result.preferences()})
}
/// Stored owners plus live Wizard, BitmapSize, Paths and disclosure state.
fn window_state(app: &OpenVectorApp) -> Value {
    let (wizard, bitmap) = (&app.wizard, &app.workflows.bitmap);
    json!({"stored":window_preferences(&app.preferences),"paths":app.show_paths,
        "disclosures":app.workflows.disclosures,
        "wizard":[wizard.skip_review,wizard.auto_palette,wizard.auto_segmentation,wizard.auto_vectorization],
        "bitmapSize":[bitmap.scale,bitmap.pane_scales,bitmap.panes,bitmap.original]})
}
/// Checks the stored predictions and their live Wizard and BitmapSize copies.
/// Paths is checked only where one window lived through the reset, because a
/// new window always starts without it.
fn assert_predicted_window(app: &OpenVectorApp) {
    assert_eq!(window_preferences(&app.preferences), predicted_window_preferences(), "static prediction, not a native capture");
    let (wizard, bitmap) = (&app.wizard, &app.workflows.bitmap);
    assert_eq!([wizard.skip_review, wizard.auto_palette, wizard.auto_segmentation, wizard.auto_vectorization], [false, true, true, true]);
    // At scale 1 both live panes settle at the original size.
    assert_eq!((bitmap.scale, bitmap.pane_scales, bitmap.panes), (1.0, [1.0; 2], [bitmap.original; 2]));
    assert_eq!(app.workflows.disclosures, [false; 5]);
}
/// Moves each predicted owner, and Paths, off its reset value. The folders do
/// not exist, so a preference reload keeps them verbatim.
fn alter_window(app: &mut OpenVectorApp, directory: &Directory) {
    app.wizard.skip_review = true; app.wizard.auto_palette = false; app.wizard.auto_segmentation = false; app.wizard.auto_vectorization = false;
    let preferences = &mut app.preferences;
    preferences.skip_review = true; preferences.auto_palette = false; preferences.auto_segmentation = false; preferences.auto_vectorization = false;
    for (save, formats) in [(&mut preferences.quick_save, ["svg", "pdf"]), (&mut preferences.bitmap_save, ["bmp", "tif"])] {
        for (choice, format) in save.choices.iter_mut().zip(formats) {
            choice.format = format.into(); choice.expanded = true; choice.directory = directory.0.join(format);
        }
    }
    for (choice, format) in preferences.drag_result.choices.iter_mut().zip(["svg", "pdf"]) { choice.format = format; choice.minimize = false; }
    // Both live panes leave scale 1; the stored scale follows the last commit.
    app.workflows.bitmap.commit(0, 2, 2.5).unwrap(); app.workflows.bitmap.commit(1, 2, 0.5).unwrap();
    app.preferences.bitmap_scale = app.workflows.bitmap.scale;
    app.show_paths = true;
    app.workflows.disclosures = [true; 5];
}

// Decode paths by role, not by the host's spelling. Never change process cwd:
// GPUI tests share a process and reset resolves save folders against that cwd.
fn capture_path(value: &Value, before: &Value, directory: &Directory) -> PathBuf {
    let text = value.as_str().unwrap().replace('\\', "/");
    if text.is_empty() { return PathBuf::new(); }
    if text == before["currentDirectory"].as_str().unwrap().replace('\\', "/") {
        return std::env::current_dir().unwrap().canonicalize().unwrap();
    }
    let history = &before["sourceHistory"];
    if text == history["filename"].as_str().unwrap().replace('\\', "/") {
        return directory.0.join(before["sourceName"].as_str().unwrap());
    }
    if text == history["sourceDirectory"].as_str().unwrap().replace('\\', "/") {
        return directory.0.clone();
    }
    assert_eq!(text, before["batchQueue"]["exportDirectory"].as_str().unwrap().replace('\\', "/"));
    directory.0.join("export")
}

fn capture_page(expected: &Value) -> Page {
    match expected["wizardPage"].as_u64().unwrap() {
        4 => Page::Mode,
        5 => Page::ImageType,
        6 => Page::Transparency,
        18 => Page::Export,
        19 => Page::Review,
        page => panic!("unmapped native Reset page {page}"),
    }
}

// Compare semantic owners, not Qt storage layout: phase/click counters, widget
// addresses/kinds/labels, selector entries/indices, raw scaleBits, control
// headers and the registration flag describe the recorder/platform. GPUI has
// no activation store or QPixmap pool; source/preview release is checked below.
fn assert_window_capture(app: &OpenVectorApp, expected: &Value, before: &Value, directory: &Directory) {
    let dimensions = app.document.as_ref().map_or((0, 0), |doc| (doc.width, doc.height));
    assert_eq!(dimensions, (expected["sourceWidth"].as_u64().unwrap() as usize, expected["sourceHeight"].as_u64().unwrap() as usize));
    assert_eq!(expected["stackedWidgetIndex"], expected["wizardPage"]);
    assert_eq!(app.wizard.page, capture_page(expected));
    if let Some(owner) = expected.get("ownership") {
        assert_eq!(app.settings.advanced, owner["advanced"] == 1);
        assert_eq!(app.transparency_initialized, [owner["basicEntryGuard"] == 1, owner["advancedEntryGuard"] == 1]);
    }
    assert_eq!(app.preferences.selected, profile(&expected["selected"]));
    assert_eq!(app.current_profile(), profile(&expected["selected"]));
    assert_eq!(app.preferences.batch, profile(&expected["batch"]));
    assert_eq!(SavedAdvancedProfiles::default().snapshot()[0].profile, profile(&expected["defaultAdvanced"]));
    assert_eq!(app.workflows.profiles.snapshot().iter().map(|entry| (entry.name.clone(), entry.profile.clone())).collect::<Vec<_>>(),
        expected["savedProfiles"].as_array().unwrap().iter().map(|entry| (entry["name"].as_str().unwrap().to_owned(), profile(entry))).collect::<Vec<_>>());
    let widgets = expected["resetWidgets"].as_array().unwrap();
    for (index, (live, stored)) in [
        (app.wizard.auto_palette, app.preferences.auto_palette),
        (app.wizard.auto_segmentation, app.preferences.auto_segmentation),
        (app.wizard.auto_vectorization, app.preferences.auto_vectorization),
        (app.wizard.skip_review, app.preferences.skip_review),
    ].into_iter().enumerate() {
        let flag = widgets[index]["flag"] == 1;
        assert_eq!((live, stored), (flag, flag));
    }
    for (index, choice) in app.preferences.quick_save.choices.iter().chain(&app.preferences.bitmap_save.choices).enumerate() {
        let widget = &widgets[index + 4];
        assert_eq!(choice.format, widget["format"]["selected"].as_str().unwrap());
        assert_eq!(choice.expanded, widget["flag"] == 1);
        assert_eq!(choice.directory, capture_path(&widget["folder"], before, directory));
    }
    for mode in 0..2 {
        assert_eq!(app.workflows.bitmap.pane_scales[mode], widgets[6 + mode]["scale"].as_f64().unwrap());
        let scale = widgets[6 + mode]["scale"].as_f64().unwrap();
        let original = [before["sourceWidth"].as_i64().unwrap() as i32, before["sourceHeight"].as_i64().unwrap() as i32];
        assert_eq!(app.workflows.bitmap.original, original);
        assert_eq!(app.workflows.bitmap.panes[mode], original.map(|size| (size as f64 * scale + 0.5).trunc() as i32));
        let drag = &app.preferences.drag_result.choices[mode];
        assert_eq!(drag.format, widgets[13 + mode]["format"]["selected"].as_str().unwrap());
        assert_eq!(drag.minimize, widgets[13 + mode]["flag"] == 1);
    }
    assert_eq!(app.workflows.bitmap.scale, widgets[7]["scale"].as_f64().unwrap());
    assert_eq!(app.preferences.bitmap_scale, widgets[7]["scale"].as_f64().unwrap());
    for (index, disclosure) in [Disclosure::RecentFolders, Disclosure::RecentImages, Disclosure::RecentBasicSettings,
        Disclosure::SavedAdvancedSettings, Disclosure::SaveSettings].into_iter().enumerate() {
        assert_eq!(app.workflows.disclosures[disclosure as usize], widgets[8 + index]["flag"] == 1);
    }
    assert_eq!(app.show_paths, widgets[15]["flag"] == 1);
    let exports = &expected["independentExporterValues"];
    assert_eq!(app.preferences.layout, ["stacked", "cutouts", "grouped-cutouts"][exports[0].as_u64().unwrap() as usize]);
    assert_eq!(app.preferences.compact_eps, exports[1] == 0);
    assert_eq!(app.preferences.thin_stroke, exports[2] == 0);
    assert_eq!(app.preferences.curve_mode, ["splines", "sparse-lines", "dense-lines"][exports[3].as_u64().unwrap() as usize]);
    let history = &expected["sourceHistory"];
    let mut images = history["images"].clone();
    for image in images.as_array_mut().unwrap() { image["filename"] = json!(capture_path(&image["filename"], before, directory)); }
    assert_eq!(app.preferences.sources.snapshot(), json!({"capacity":history["capacity"], "files":images}));
    let folders = history["directories"].as_array().unwrap().iter().map(|path| capture_path(path, before, directory)).collect::<Vec<_>>();
    assert_eq!(app.preferences.directories.snapshot(), json!({"capacity":history["directoryCapacity"], "directories":folders}));
    // R3 decodes model+0xcc as filename and model+0xc4 as sourceDirectory.
    assert_eq!(app.document.as_ref().map(|doc| doc.filename.clone()).unwrap_or_default(), capture_path(&history["filename"], before, directory));
    assert_eq!(app.source_directory, capture_path(&history["sourceDirectory"], before, directory));
    assert_eq!(app.document.is_some(), history["sourcePresent"] == 1);
    if history["pixmaps"] == 0 {
        assert!(app.source_image.is_none()); assert!(app.workflows.thumbnails.is_empty());
        assert!(app.source_recommendations.is_none());
    }
    let queue = &expected["batchQueue"];
    assert_eq!(app.batch.is_some(), queue["isBatchMode"] == 1);
    assert_eq!(app.batch_queue.processing_index as i64, queue["processingIndex"].as_i64().unwrap());
    assert_eq!(app.batch_queue.export_directory, capture_path(&queue["exportDirectory"], before, directory).to_string_lossy());
    assert_eq!(app.batch_queue.export_format, queue["exportFormat"].as_str().unwrap());
    let records = app.batch_queue.snapshot().queue;
    let native_records = queue["records"].as_array().unwrap();
    let outputs = queue["outputs"].as_array().unwrap();
    assert_eq!(records.len(), native_records.len());
    assert_eq!(outputs.len(), native_records.len());
    // The wire stores index/eligibility separately from output filenames.
    // Other flags and the queue-card HTML are native presentation state,
    // not the QGraphicsItem selection or Rust BatchRecord fields.
    for ((record, native), output) in records.iter().zip(native_records).zip(outputs) {
        assert_eq!(record.filename, capture_path(&native["filename"], before, directory).to_string_lossy());
        assert_eq!(record.button_index as u64, native["flags"][0].as_u64().unwrap());
        assert_eq!(record.should_process, native["flags"][1] == 1);
        assert_eq!(record.output_filename, capture_path(&output[0], before, directory).to_string_lossy());
    }
    assert_eq!(queue["wizardPage"], expected["wizardPage"]);
    assert_eq!(app.dialog.is_some(), queue["dialogSeen"] == 1);
    let recommendations = app.source_recommendations.unwrap_or([-1; 3]).map(|value| value as u32);
    assert_eq!(json!(recommendations), queue["controls"]["recommendedControls"]);
    assert!(app.workflows.batch_result.is_none());
    assert_eq!(profile(&json!({"settingsBits":queue["controls"]["selectedSettingsBits"],"paletteArgbBits":expected["selected"]["paletteArgbBits"]})), app.preferences.selected);
    assert_eq!(profile(&json!({"settingsBits":queue["controls"]["requestedSettingsBits"],"paletteArgbBits":expected["batch"]["paletteArgbBits"]})), app.preferences.batch);
}

#[gpui::test]
fn native_full_window_reset_no_yes_matches_decoded_capture(cx: &mut gpui::TestAppContext) {
    let reference: Value = serde_json::from_slice(&std::fs::read(fixture("application-window-reset-reference.json")).unwrap()).unwrap();
    for case in reference["cases"].as_array().unwrap() {
      for escape_no in [false, true].into_iter().take(if case["answer"] == 65536 { 2 } else { 1 }) {
        let directory = Directory::new();
        let source = directory.0.join(case["sourceName"].as_str().unwrap());
        std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(case["sourcePng"].as_str().unwrap()).unwrap()).unwrap();
        let before = &case["before"];
        let entry = case.get("entryBefore").unwrap_or(before);
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
            app.preferences.selected = profile(&entry["selected"]); app.preferences.batch = profile(&before["batch"]);
            app.settings = app.preferences.selected.settings.clone(); app.color_count = app.preferences.selected.color_count;
            app.workflows.palette_count = app.preferences.selected.palette_color_count as f64;
            app.workflows.recommended_background_mode = app.preferences.selected.recommended_background_mode;
            app.source_recommendations = Some(std::array::from_fn(|index| before["batchQueue"]["controls"]["recommendedControls"][index].as_u64().unwrap() as i32));
            app.wizard.page = capture_page(entry);
            if let Some(owner) = entry.get("ownership") {
                app.transparency_initialized = [owner["basicEntryGuard"] == 1, owner["advancedEntryGuard"] == 1];
            }
            let widgets = before["resetWidgets"].as_array().unwrap();
            app.wizard.auto_palette = widgets[0]["flag"] == 1; app.preferences.auto_palette = app.wizard.auto_palette;
            app.wizard.auto_segmentation = widgets[1]["flag"] == 1; app.preferences.auto_segmentation = app.wizard.auto_segmentation;
            app.wizard.auto_vectorization = widgets[2]["flag"] == 1; app.preferences.auto_vectorization = app.wizard.auto_vectorization;
            app.wizard.skip_review = widgets[3]["flag"] == 1; app.preferences.skip_review = app.wizard.skip_review;
            for mode in 0..2 {
                app.workflows.bitmap.commit(mode, 2, widgets[6 + mode]["scale"].as_f64().unwrap()).unwrap();
                let drag = &mut app.preferences.drag_result.choices[mode];
                drag.format = match widgets[13 + mode]["format"]["selected"].as_str().unwrap() {
                    "ai" => "ai", "dxf" => "dxf", "emf" => "emf", "eps" => "eps", "pdf" => "pdf", "svg" => "svg",
                    format => panic!("unmapped drag format {format}"),
                };
                drag.minimize = widgets[13 + mode]["flag"] == 1;
            }
            app.preferences.bitmap_scale = app.workflows.bitmap.scale;
            for (index, disclosure) in [Disclosure::RecentFolders, Disclosure::RecentImages, Disclosure::RecentBasicSettings,
                Disclosure::SavedAdvancedSettings, Disclosure::SaveSettings].into_iter().enumerate() {
                app.workflows.disclosures[disclosure as usize] = widgets[8 + index]["flag"] == 1;
            }
            app.show_paths = widgets[15]["flag"] == 1;
            for entry in before["savedProfiles"].as_array().unwrap().iter().rev() {
                app.workflows.profiles.put(entry["name"].as_str().unwrap(), profile(entry));
            }
            for (index, choice) in app.preferences.quick_save.choices.iter_mut().chain(&mut app.preferences.bitmap_save.choices).enumerate() {
                let widget = &before["resetWidgets"][index + 4];
                choice.directory = capture_path(&widget["folder"], before, &directory);
                choice.format = widget["format"]["selected"].as_str().unwrap().into();
                choice.expanded = widget["flag"] == 1;
            }
            let exports = &before["independentExporterValues"];
            app.preferences.layout = ["stacked", "cutouts", "grouped-cutouts"][exports[0].as_u64().unwrap() as usize].into();
            app.preferences.compact_eps = exports[1] == 0; app.preferences.thin_stroke = exports[2] == 0;
            app.preferences.curve_mode = ["splines", "sparse-lines", "dense-lines"][exports[3].as_u64().unwrap() as usize].into();
            let filenames = before["batchQueue"]["records"].as_array().unwrap().iter()
                .map(|record| capture_path(&record["filename"], before, &directory).to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(app.batch_queue.add_files(&filenames).is_empty());
            app.batch_queue.export_directory = capture_path(&before["batchQueue"]["exportDirectory"], before, &directory).to_string_lossy().into_owned();
            app.batch_queue.export_format = before["batchQueue"]["exportFormat"].as_str().unwrap().into();
            if case.get("entryBefore").is_some() {
                // Fresh native histories retain controls before the first
                // Transparency entry. Exercise initialization, do not seed its result.
                assert_window_capture(app, entry, before, &directory);
                let action = match app.wizard.page {
                    Page::ImageType => Action::Next,
                    Page::Mode => Action::Advanced,
                    page => panic!("unmapped native Transparency entry {page:?}"),
                };
                app.dispatch(action, window, cx);
                settle(app, cx); assert!(!app.error, "{}", app.status);
            }
            assert_window_capture(app, before, before, &directory);
            app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
            assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::ResetApplicationAccepted)))));
            assert!(app.confirmation_cancel.is_focused(window));
            if case["answer"] == 65536 {
                if escape_no { app.dispatch(Action::Dismiss, window, cx); }
                else { app.workflow_enter(window, cx); }
            } else { app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx); }
            assert!(app.dialog.is_none()); assert!(!app.error, "{}", app.status);
            assert_window_capture(app, &case["returned"], before, &directory);
            settle(app, cx); assert!(!app.error, "{}", app.status);
            assert_window_capture(app, &case["drained"], before, &directory);
        });
      }
    }
}

#[gpui::test]
fn native_application_reset_transitions_preserve_batch_requests_and_main_export_choices(cx: &mut gpui::TestAppContext) {
    for reference in references("application-model-reset-reference.ndjson.gz") {
        for (index, action) in reference["phases"][0].as_array().unwrap().iter().enumerate() {
            if action[0] != "reset-application-model" { continue; }
            let before = &reference["expected"][0][index - 1]; let after = &reference["expected"][0][index];
            // Page setters and document captures omit profile/list payloads.
            // Use the preceding full capture, but prove its controls are still
            // current rather than silently seeding a stale pre-reset profile.
            let profile_frame = reference["expected"][0].as_array().unwrap()[..index].iter().rev()
                .find(|frame| frame.get("selected").is_some()).expect("a full profile capture before reset");
            assert_eq!(profile_frame["selected"]["settingsBits"], before["processing"]["controls"]["selectedSettingsBits"], "{} / {index}: selected precondition", reference["id"]);
            assert_eq!(profile_frame["batch"]["settingsBits"], before["processing"]["controls"]["requestedSettingsBits"], "{} / {index}: batch precondition", reference["id"]);
            let directory = Directory::new(); let paths = inputs(&directory);
            let source = paths[reference["phases"][0][0][1].as_str().unwrap()].clone();
            let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
            app.update_in(visual, |app, window, cx| {
                app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
                app.preferences.selected = profile(&profile_frame["selected"]); app.preferences.batch = profile(&profile_frame["batch"]);
                app.workflows.profiles.reset();
                for entry in profile_frame["profiles"].as_array().unwrap().iter().rev() {
                    app.workflows.profiles.put(entry["name"].as_str().unwrap(), profile(entry));
                }
                let batch = before["processing"]["isBatchMode"].as_bool().unwrap();
                let names = before["queue"].as_array().unwrap().iter().map(|entry| paths[entry["filename"].as_str().unwrap()].to_string_lossy().into_owned()).collect::<Vec<_>>();
                app.batch_queue.add_files(&names);
                // A nonzero cursor catches clear() alone, which only clears records.
                app.batch_queue.processing_index = 7;
                app.batch_queue.export_format = before["exportFormat"].as_str().unwrap().into();
                app.batch_queue.export_directory = directory.0.join("outputs").to_string_lossy().into_owned();
                app.batch = batch.then(|| app.batch_queue.snapshot()); app.input.set_batch_mode(batch);
                let selected = if batch { &app.preferences.batch } else { &app.preferences.selected };
                app.settings = selected.settings.clone();
                app.color_count = if batch && !selected.settings.advanced { selected.basic_color_count } else { selected.color_count };
                app.workflows.palette_count = selected.palette_color_count as f64;
                app.workflows.recommended_background_mode = selected.recommended_background_mode;
                // GPUI shows batch files/settings together. Neither native page case
                // may switch workspace or change the underlying single-image page.
                app.wizard.page = Page::Palette;
                app.preferences.format = app.batch_queue.export_format.clone();
                app.preferences.layout = "grouped-cutouts".into(); app.preferences.curve_mode = "dense-lines".into();
                app.preferences.thin_stroke = true; app.preferences.compact_eps = true;
                alter_window(app, &directory);
                // Static prediction: the native list stage (0x42f380) also clears Recent Basic Settings.
                assert!(app.preferences.recent_basic.record_completion(&app.preferences.selected, 0, true, false));
                // Names derived from the closed source must not survive it.
                app.preferences.quick_save.choices[0].filename = "old-source".into();
                app.preferences.bitmap_save.choices[1].filename = "old-vectorized".into();
                let exports = app.export_options();
                let viewport_area = Size::new(800.0, 600.0);
                app.view = app.view.resized(viewport_area);
                let chooser = app.source_directory.clone(); let export_directory = app.batch_queue.export_directory.clone();
                confirm_reset(app, window, cx);
                assert!(!app.error, "{} / {index}: {}", reference["id"], app.status);
                assert!(app.document.is_none()); assert!(app.source_image.is_none());
                assert_eq!(app.view.area, viewport_area, "reset must retain the unchanged canvas size");
                assert_eq!(app.preferences.selected, profile(&after["selected"]));
                assert_eq!(app.preferences.batch, profile(&after["batch"]));
                assert_eq!(app.current_profile(), profile(&after[if batch { "batch" } else { "selected" }]));
                assert_lists(app, after);
                assert!(app.preferences.recent_basic.entries().is_empty(), "static prediction, not a native capture");
                assert_eq!(app.batch.is_some(), batch); assert_eq!(app.wizard.page, Page::Palette);
                assert!(app.batch_queue.snapshot().queue.is_empty()); assert_eq!(app.batch_queue.processing_index as i64, after["processingIndex"].as_i64().unwrap());
                assert_eq!(app.batch_queue.export_directory, export_directory); assert_eq!(app.batch_queue.export_format, after["exportFormat"].as_str().unwrap());
                assert_eq!(app.export_options(), exports);
                assert!(app.preferences.quick_save.choices.iter().all(|choice| choice.filename.is_empty()));
                assert!(app.preferences.bitmap_save.choices.iter().all(|choice| choice.filename.is_empty()));
                assert_eq!(app.source_directory, chooser);
                // Model state above follows the native capture; window owners follow the static prediction.
                assert_predicted_window(app); assert!(!app.show_paths);
                assert!(app.automatic.scheduled().is_none()); assert!(app.workflows.thumbnails.is_empty());
                let restored = Preferences::load(directory.0.join("gui.json")).unwrap();
                assert_eq!(restored.source_directory, chooser); assert_eq!(restored.sources.snapshot(), app.preferences.sources.snapshot());
                assert_eq!(restored.directories.snapshot(), app.preferences.directories.snapshot());
                assert!(restored.recent_basic.entries().is_empty());
                assert_eq!(openvector::settings_profile::stored_settings_profile(&restored.selected), openvector::settings_profile::stored_settings_profile(&app.preferences.selected));
                assert_eq!(openvector::settings_profile::stored_settings_profile(&restored.batch), openvector::settings_profile::stored_settings_profile(&app.preferences.batch));
                assert_eq!(restored.format, app.preferences.format); assert_eq!(restored.batch_directory.to_string_lossy(), export_directory);
                assert_eq!(window_preferences(&restored), predicted_window_preferences());
                assert_eq!(SavedAdvancedProfiles::read_file(directory.0.join("profiles.json")).unwrap().snapshot(), app.workflows.profiles.snapshot());
                // Reopening exercises the real worker's blank committed settings,
                // which differ from the selected profile's white background.
                if batch { app.dispatch(Action::Batch, window, cx); }
                app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
                assert_eq!(app.view.area, viewport_area);
                assert!(app.view.zoom > 1.0, "reopening a small source must fit the retained viewport, not clamp to 20%");
                let document_frame = reference["expected"][0].as_array().unwrap().iter().skip(index + 1).find_map(|frame| frame.get("document")).unwrap();
                let expected = profile(&json!({"settingsBits":document_frame["state"]["settingsBits"],"paletteArgbBits":""}));
                assert_eq!(app.document.as_ref().unwrap().settings, expected.settings);
                assert_eq!(app.document.as_ref().unwrap().committed_count, expected.color_count);
            });
        }
    }
}

#[gpui::test]
fn application_reset_cancel_and_start_do_not_clear_application_lists(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let paths = inputs(&directory); let source = paths.values().min().unwrap().clone();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); settle(app, cx); assert!(!app.error, "{}", app.status);
        alter_window(app, &directory);
        app.settings.complexity = 11; app.color_count = 9;
        assert!(app.preferences.recent_basic.record_completion(&app.current_profile(), 0, true, false)); app.persist(cx);
        app.workflows.profiles.put("custom", app.current_profile()); app.save_profiles(cx).unwrap();
        let profiles = app.workflows.profiles.snapshot(); let sources = app.preferences.sources.snapshot(); let owners = window_state(app);
        let recent = app.preferences.recent_basic.stored_profiles();
        let draft = |app: &OpenVectorApp, cx: &App| app.workflows.profile_name.read(cx).text.clone();
        app.workflows.profile_name.update(cx, |field, cx| field.set("draft", cx));
        let persisted = std::fs::read(&app.preferences.path).unwrap();
        app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx);
        assert!(app.document.is_some()); assert_eq!(window_state(app), owners);
        for escape in [false, true] {
            app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
            assert!(app.confirmation_cancel.is_focused(window));
            if escape { app.dispatch(Action::Dismiss, window, cx); } else { app.workflow_enter(window, cx); }
            assert!(app.dialog.is_none()); assert!(app.document.is_some()); assert!(!app.busy);
            assert_eq!(app.settings.complexity, 11); assert_eq!(app.color_count, 9);
            assert_eq!(app.workflows.profiles.snapshot(), profiles); assert_eq!(app.preferences.sources.snapshot(), sources);
            assert_eq!(app.preferences.recent_basic.stored_profiles(), recent); assert_eq!(draft(app, cx), "draft");
            assert_eq!(window_state(app), owners);
            assert_eq!(std::fs::read(&app.preferences.path).unwrap(), persisted);
        }
        app.dispatch(Action::Start, window, cx); app.dispatch(Action::Start, window, cx); settle(app, cx);
        assert!(app.document.is_none()); assert_eq!(app.settings.complexity, 11); assert_eq!(app.color_count, 9);
        assert_eq!(app.workflows.profiles.snapshot(), profiles); assert_eq!(app.preferences.sources.snapshot(), sources);
        assert_eq!(app.preferences.recent_basic.stored_profiles(), recent);
        // The second Start runs the full model reset, whose mainModelReset suggests a name.
        assert_eq!(draft(app, cx), "Settings 1");
        assert_eq!(window_state(app), owners);
        // List reset must recreate the built-in entry, not retain its renamed copy.
        app.workflows.profiles.rename("Default Advanced Settings", "renamed default", false).unwrap();
        app.workflows.profile_name.update(cx, |field, cx| field.set("draft", cx));
        confirm_reset(app, window, cx); assert!(!app.error, "{}", app.status);
        let native = references("factory-reset-reference.ndjson.gz").into_iter().find(|row| row["id"] == "fresh-business-reset").unwrap();
        assert_lists(app, &native["expected"][0][1]);
        assert!(app.preferences.recent_basic.entries().is_empty());
        // Static prediction: 0x451750 reaches the same full model reset.
        assert_eq!(draft(app, cx), "Settings 1");
    });
}

#[gpui::test]
fn application_reset_attempts_both_stores_and_keeps_failed_targets(cx: &mut gpui::TestAppContext) {
    for (fail_profiles, fail_preferences) in [(true, false), (false, true), (true, true)] {
        let directory = Directory::new(); let paths = inputs(&directory); let source = paths.values().min().unwrap().clone();
        let preference_path = directory.0.join("gui.json"); let profile_path = directory.0.join("profiles.json");
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preference_path.clone(), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
            let source_bytes = std::fs::read(&source).unwrap();
            app.workflows.profiles.put("custom", app.current_profile()); app.save_profiles(cx).unwrap();
            let chooser = app.source_directory.clone();
            alter_window(app, &directory);
            for (path, fail) in [(&profile_path, fail_profiles), (&preference_path, fail_preferences)] {
                if fail { std::fs::remove_file(path).unwrap(); std::fs::create_dir(path).unwrap(); std::fs::write(path.join("keep.txt"), b"keep").unwrap(); }
            }
            let output = directory.0.join("output.svg"); std::fs::write(&output, b"existing output").unwrap();
            let unrelated = preference_path.with_extension(format!("{}.tmp", std::process::id())); std::fs::write(&unrelated, b"unrelated").unwrap();
            confirm_reset(app, window, cx);
            assert!(app.error);
            assert_eq!(app.status.contains("saved profiles:"), fail_profiles);
            assert_eq!(app.status.contains("preferences and source history:"), fail_preferences);
            assert!(app.document.is_none()); assert_eq!(app.source_directory, chooser);
            // The window stage precedes both writes, so a failed write keeps it in memory.
            assert_predicted_window(app); assert!(!app.show_paths);
            assert!(app.preferences.sources.snapshot()["files"].as_array().unwrap().is_empty());
            assert_eq!(app.workflows.profiles.snapshot(), SavedAdvancedProfiles::default().snapshot());
            for (path, fail) in [(&profile_path, fail_profiles), (&preference_path, fail_preferences)] {
                if fail { assert_eq!(std::fs::read(path.join("keep.txt")).unwrap(), b"keep"); }
            }
            if !fail_profiles { assert_eq!(SavedAdvancedProfiles::read_file(&profile_path).unwrap().snapshot(), app.workflows.profiles.snapshot()); }
            if !fail_preferences {
                let restored = Preferences::load(preference_path.clone()).unwrap();
                assert!(restored.sources.snapshot()["files"].as_array().unwrap().is_empty());
                assert_eq!(window_preferences(&restored), predicted_window_preferences());
            }
            assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
            assert_eq!(std::fs::read(output).unwrap(), b"existing output"); assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");
            assert!(!std::fs::read_dir(&directory.0).unwrap().map(|entry| entry.unwrap().path()).any(|path| path != unrelated && path.extension().is_some_and(|extension| extension == "tmp")));
        });
    }
}

#[gpui::test]
fn application_reset_rejects_pending_batch_and_preserves_completed_results(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let paths = inputs(&directory); let source = paths.values().min().unwrap().clone();
    let output = source.with_extension("svg"); std::fs::write(&output, b"existing output").unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.dispatch(Action::Batch, window, cx);
        alter_window(app, &directory);
        app.batch_queue.add_files(&[source.to_string_lossy().into_owned()]); app.batch = Some(app.batch_queue.snapshot());
        app.dispatch(Action::BatchStart, window, cx);
        let deadline = Instant::now() + Duration::from_secs(120);
        while !matches!(app.dialog, Some(Dialog::BatchOverwrite(_))) {
            app.poll(cx); assert!(Instant::now() < deadline, "overwrite dialog did not appear: {}", app.status);
            std::thread::sleep(Duration::from_millis(1));
        }
        let queue = app.batch_queue.snapshot(); let profiles = app.workflows.profiles.snapshot(); let owners = window_state(app);
        app.workflow_action(WorkflowAction::ResetApplication, window, cx);
        app.workflow_action(WorkflowAction::ResetApplicationAccepted, window, cx);
        assert!(matches!(app.dialog, Some(Dialog::BatchOverwrite(_)))); assert!(app.busy);
        assert_eq!(app.batch_queue.snapshot(), queue); assert_eq!(app.workflows.profiles.snapshot(), profiles);
        assert_eq!(window_state(app), owners);
        app.dispatch(Action::BatchDecision(OverwriteDecision::Skip), window, cx); settle(app, cx); assert!(!app.error, "{}", app.status);
        let outcome = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((outcome.converted, outcome.failed, outcome.skipped), (0, 0, 1)); assert_eq!(outcome.files[0].status, "skipped");
        let log = app.batch_log.clone();
        confirm_reset(app, window, cx); assert!(!app.error, "{}", app.status);
        assert!(app.batch.as_ref().unwrap().queue.is_empty());
        assert_predicted_window(app); assert!(!app.show_paths);
        let outcome = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((outcome.converted, outcome.failed, outcome.skipped), (0, 0, 1)); assert_eq!(outcome.files[0].status, "skipped");
        assert_eq!(outcome.files[0].filename, source.to_string_lossy()); assert_eq!(app.batch_log, log);
        assert_eq!(std::fs::read(output).unwrap(), b"existing output");
    });
}

// The overwrite question arrives while the batch is still busy, and the worker
// waits for the answer. A click on the rendered button must reach it.
#[gpui::test]
fn batch_overwrite_buttons_answer_while_busy(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let paths = inputs(&directory); let source = paths.values().min().unwrap().clone();
    let output = source.with_extension("svg"); std::fs::write(&output, b"existing output").unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.dispatch(Action::Batch, window, cx);
        app.batch_queue.add_files(&[source.to_string_lossy().into_owned()]); app.batch = Some(app.batch_queue.snapshot());
        app.dispatch(Action::BatchStart, window, cx);
        let deadline = Instant::now() + Duration::from_secs(120);
        while !matches!(app.dialog, Some(Dialog::BatchOverwrite(_))) {
            app.poll(cx); assert!(Instant::now() < deadline, "overwrite dialog did not appear: {}", app.status);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.busy);
        cx.notify();
    });
    visual.simulate_mouse_move(gpui::point(px(0.0), px(0.0)), None, gpui::Modifiers::default());
    let skip = visual.debug_bounds("dialog-skip").expect("rendered Skip existing control").center();
    visual.simulate_click(skip, gpui::Modifiers::default());
    app.update_in(visual, |app, _, cx| {
        assert!(app.dialog.is_none(), "the click must answer the dialog");
        settle(app, cx); assert!(!app.error, "{}", app.status);
        let outcome = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((outcome.converted, outcome.failed, outcome.skipped), (0, 0, 1));
        assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
    });
}

#[gpui::test]
fn application_reset_window_owners_persist_across_restart(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let paths = inputs(&directory); let source = paths.values().min().unwrap().clone();
    let preference_path = directory.0.join("gui.json");
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preference_path.clone(), cx).unwrap());
    let retained = app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
        alter_window(app, &directory);
        // Retained owners leave their defaults too, so the restart must read this store.
        app.preferences.format = "pdf".into(); app.preferences.layout = "cutouts".into(); app.preferences.curve_mode = "dense-lines".into();
        app.preferences.thin_stroke = true; app.preferences.compact_eps = true;
        app.batch_queue.export_directory = directory.0.join("outputs").to_string_lossy().into_owned();
        app.persist(cx); assert!(!app.error, "{}", app.status);
        // The altered owners reach the store, so the reset must rewrite them.
        assert_eq!(window_preferences(&Preferences::load(preference_path.clone()).unwrap()), window_preferences(&app.preferences));
        confirm_reset(app, window, cx); assert!(!app.error, "{}", app.status);
        assert_predicted_window(app); assert!(!app.show_paths);
        assert_eq!(window_preferences(&Preferences::load(preference_path.clone()).unwrap()), predicted_window_preferences());
        (app.preferences.format.clone(), app.export_options(), app.source_directory.clone(), app.batch_queue.export_directory.clone())
    });
    // A second window reads both stores and starts its own worker, as a restart does.
    let (restarted, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preference_path.clone(), cx).unwrap());
    restarted.update_in(visual, |app, _, cx| {
        assert!(!app.error, "{}", app.status);
        assert_predicted_window(app);
        assert_eq!((app.preferences.format.clone(), app.export_options(), app.source_directory.clone(), app.batch_queue.export_directory.clone()), retained);
        app.open(source.clone(), cx); settle(app, cx); assert!(!app.error, "{}", app.status);
        let document = app.document.as_ref().unwrap(); let size = [document.width as i32, document.height as i32];
        assert_eq!((app.workflows.bitmap.panes, app.workflows.bitmap.pane_scales), ([size; 2], [1.0; 2]), "scale 1 keeps the next image at its pixel size");
    });
}
