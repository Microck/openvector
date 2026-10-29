use super::*;
use base64::Engine;
use std::{io::Read, time::{Duration, Instant}};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-controls-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn fixtures(name: &str) -> Vec<Value> {
    // Same lookup as openvector/tests/common: build boxes hold fixtures outside the tree.
    let base = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let path = base.join(format!("{name}.ndjson.gz"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap()).read_to_string(&mut text).unwrap();
    text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}
fn uniform_bytes() -> Vec<u8> {
    let reference = fixtures("batch-runner-reference").into_iter().find(|row| row["id"] == "uniform-pair").unwrap();
    base64::engine::general_purpose::STANDARD.decode(reference["sources"][0]["bytes"].as_str().unwrap()).unwrap()
}
fn settle(app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.poll(cx);
        if matches!(app.dialog, Some(Dialog::BatchOverwrite(_))) { app.dispatch(Action::BatchDecision(OverwriteDecision::Skip), window, cx); }
        if !app.busy && app.pending.is_empty() { break; }
        assert!(Instant::now() < deadline, "worker did not complete: {}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn finish(app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>) {
    settle(app, window, cx);
    assert!(!app.error, "{}", app.status);
}

#[gpui::test]
fn batch_selection_results_and_retry_use_real_runner(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let valid = directory.0.join("valid.png"); let invalid = directory.0.join("invalid.png"); let skipped = directory.0.join("skipped.png");
    let bytes = uniform_bytes();
    std::fs::write(&valid, &bytes).unwrap(); std::fs::write(&invalid, b"invalid image").unwrap(); std::fs::write(&skipped, &bytes).unwrap();
    let skipped_output = directory.0.join("skipped.svg"); std::fs::write(&skipped_output, b"keep existing").unwrap();
    let names: Vec<String> = [&valid, &invalid, &skipped].into_iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Batch, window, cx);
        app.batch_queue.add_files(&names); app.batch = Some(app.batch_queue.snapshot());
        app.workflow_action(WorkflowAction::BatchSelect(0, true), window, cx);
        app.workflow_action(WorkflowAction::BatchSelect(2, true), window, cx);
        assert_eq!(app.batch.as_ref().unwrap().queue.iter().map(|record| record.selected).collect::<Vec<_>>(), [true, false, true]);
        app.dialog = Some(Dialog::BatchOverwrite(vec![]));
        app.workflow_action(WorkflowAction::BatchClear, window, cx);
        assert_eq!(app.batch_queue.snapshot().queue.len(), 3);
        app.dialog = None;
        app.workflow_action(WorkflowAction::BatchRemoveSelected, window, cx);
        assert_eq!(app.batch_queue.snapshot().queue[0].filename, names[1]);
        app.workflow_action(WorkflowAction::BatchClear, window, cx);
        assert!(app.batch_queue.snapshot().queue.is_empty());
        app.batch_queue.add_files(&names); app.batch = Some(app.batch_queue.snapshot());
        app.preferences.format = "svg".into();
        app.dispatch(Action::BatchStart, window, cx);
        app.workflow_action(WorkflowAction::BatchClear, window, cx);
        assert_eq!(app.batch_queue.snapshot().queue.len(), 3);
        finish(app, window, cx);
        let result = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((result.converted, result.failed, result.skipped), (1, 1, 1));
        assert_eq!(result.files.iter().map(|file| file.status.as_str()).collect::<Vec<_>>(), ["written", "failed-load", "skipped"]);
        assert!(result.files[1].error.is_some());
        assert_eq!(result.files.iter().map(|file| &file.filename).collect::<Vec<_>>(), names.iter().collect::<Vec<_>>());
        assert_eq!(app.batch_queue.snapshot().queue.iter().map(|file| &file.filename).collect::<Vec<_>>(), names.iter().collect::<Vec<_>>());
        assert!(std::fs::read(directory.0.join("valid.svg")).unwrap().starts_with(b"<?xml"));
        assert_eq!(std::fs::read(&skipped_output).unwrap(), b"keep existing");
        std::fs::write(&invalid, &bytes).unwrap();
        app.dispatch(Action::BatchStart, window, cx);
        assert!(app.workflows.batch_result.is_none());
        finish(app, window, cx);
        let result = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((result.converted, result.failed, result.skipped), (1, 0, 2));
        assert_eq!(result.files.iter().map(|file| file.status.as_str()).collect::<Vec<_>>(), ["skipped", "written", "skipped"]);
        assert!(std::fs::read(directory.0.join("invalid.svg")).unwrap().starts_with(b"<?xml"));
        assert_eq!(std::fs::read(&skipped_output).unwrap(), b"keep existing");
    });
}

#[gpui::test]
fn repaired_batch_input_replaces_its_cached_preview(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let valid = directory.0.join("valid.png"); let invalid = directory.0.join("invalid.png");
    let bytes = uniform_bytes();
    std::fs::write(&valid, &bytes).unwrap(); std::fs::write(&invalid, b"invalid image").unwrap();
    let names: Vec<String> = [&valid, &invalid].into_iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Batch, window, cx);
        app.batch_queue.add_files(&names); app.batch = Some(app.batch_queue.snapshot());
        app.load_previews(cx);
    });
    // The add-time decode settles and caches the failure before the user repairs the file.
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let decode_error = app.workflows.thumbnails[&invalid].as_ref().err().cloned().unwrap();
        assert!(app.workflows.thumbnails[&valid].is_ok());
        app.preferences.format = "svg".into();
        app.dispatch(Action::BatchStart, window, cx);
        finish(app, window, cx);
        let result = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!(result.files.iter().map(|file| file.status.as_str()).collect::<Vec<_>>(), ["written", "failed-load"]);
        assert_eq!(app.workflows.thumbnails[&invalid].as_ref().err(), result.files[1].error.as_ref());
        assert_eq!(result.files[1].error.as_ref(), Some(&decode_error));
        std::fs::write(&invalid, &bytes).unwrap();
        // settle answers the valid.svg overwrite prompt with Skip.
        app.dispatch(Action::BatchStart, window, cx);
        finish(app, window, cx);
        let result = app.workflows.batch_result.as_ref().unwrap();
        assert_eq!((result.converted, result.failed, result.skipped), (1, 0, 1));
        assert_eq!(result.files.iter().map(|file| file.status.as_str()).collect::<Vec<_>>(), ["skipped", "written"]);
        assert!(app.workflows.thumbnails[&invalid].is_ok(), "the repaired load must replace the cached decode failure");
        app.dispatch(Action::BatchRemove(1), window, cx);
        assert!(!app.workflows.thumbnails.contains_key(&invalid));
        assert!(app.workflows.thumbnails[&valid].is_ok());
    });
}

#[gpui::test]
fn paint_controls_round_trip_worker_color_and_picker(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.0.join("source.png");
    std::fs::write(&source, uniform_bytes()).unwrap();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let alpha = openvector::pixel_input::quantize_qcolor_channels(&[0.37])[0];
    app.update_in(cx, |app, window, cx| {
        app.open(source, cx); finish(app, window, cx);
        app.dispatch(Action::Automatic, window, cx); finish(app, window, cx);
        app.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Page::Editing), cx); finish(app, window, cx);
        app.workflow_action(WorkflowAction::PaintOpacity, window, cx);
        app.workflows.fields[0].update(cx, |field, cx| field.set("0.37", cx));
        app.workflow_action(WorkflowAction::CommitPaintOpacity, window, cx); finish(app, window, cx);
        assert_eq!(app.document.as_ref().unwrap().edit_color[3], alpha);
        app.workflow_action(WorkflowAction::PaintColor, window, cx);
    });
    cx.simulate_keystrokes("ctrl-a 2 5 5 tab ctrl-a 0 tab ctrl-a 0 enter");
    app.update_in(cx, |app, window, cx| {
        finish(app, window, cx);
        assert_eq!(app.document.as_ref().unwrap().edit_color, [0.0, 0.0, 1.0, alpha]);
        app.workflow_action(WorkflowAction::PaintOpacity, window, cx);
        app.workflows.fields[0].update(cx, |field, cx| field.set("NaN", cx));
        app.workflow_action(WorkflowAction::CommitPaintOpacity, window, cx);
        assert!(app.error); assert!(app.dialog.is_some()); assert!(!app.busy);
        app.dispatch(Action::Dismiss, window, cx);
        app.enqueue(Command::Edit { action: "gesture", input: json!({"phase":"select","tool":"picker"}) }, None, cx); finish(app, window, cx);
        let pixel = app.document.as_ref().unwrap().segments.as_ref().unwrap().as_bytes(0).unwrap()[..4].to_vec();
        app.enqueue(Command::Edit { action: "gesture", input: json!({"phase":"release","point":[0.0,0.0],"buttons":0,"button":1}) }, None, cx); finish(app, window, cx);
        let picked: [f32; 4] = std::array::from_fn(|index| (pixel[index] as f64 / 255.0) as f32);
        assert_eq!(app.document.as_ref().unwrap().edit_color, picked);
        app.workflow_action(WorkflowAction::PaintColor, window, cx);
        assert_eq!(app.field(0, cx), pixel[2].to_string());
        assert_eq!(app.field(1, cx), pixel[1].to_string());
        assert_eq!(app.field(2, cx), pixel[0].to_string());
    });
}

fn in_atlas(cx: &mut gpui::VisualTestContext, image: &RenderImage) -> bool {
    cx.update(|window, _| window.has_image_atlas_entry(image))
}

/// GPUI frees an image texture only through drop_image, so each replaced source,
/// segmentation layer or preview must leave the window atlas once it is redrawn.
#[gpui::test]
fn replaced_images_leave_the_window_atlas(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.0.join("source.png");
    std::fs::write(&source, uniform_bytes()).unwrap();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    // Every update_in ends with a test-mode draw. The Load page paints the
    // source layer beside its recent-image preview once Recent images is expanded.
    let load = |app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>| {
        app.open(source.clone(), cx); finish(app, window, cx);
        app.wizard.page = Page::Load; cx.notify();
        (app.source_image.clone().unwrap(), app.preferences.source_preview(&source).unwrap().clone())
    };
    let (first_source, first_preview) = app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::RecentImages)), window, cx);
        load(app, window, cx)
    });
    assert!(in_atlas(cx, &first_source) && in_atlas(cx, &first_preview));
    let (source_image, preview) = app.update_in(cx, |app, window, cx| load(app, window, cx));
    assert!(!in_atlas(cx, &first_source) && !in_atlas(cx, &first_preview), "a reload releases the replaced source and preview");
    assert!(in_atlas(cx, &source_image) && in_atlas(cx, &preview));

    let first_segments = app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Automatic, window, cx); finish(app, window, cx);
        app.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Page::Editing), cx); finish(app, window, cx);
        app.document.as_ref().unwrap().segments.clone().unwrap()
    });
    assert!(in_atlas(cx, &first_segments));
    let segments = app.update_in(cx, |app, window, cx| {
        app.enqueue(Command::Edit { action: "gesture", input: json!({"phase":"select","tool":"picker"}) }, None, cx); finish(app, window, cx);
        app.document.as_ref().unwrap().segments.clone().unwrap()
    });
    assert!(!in_atlas(cx, &first_segments) && in_atlas(cx, &segments), "an edit gesture releases the replaced segmentation layer");

    app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
        app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx); finish(app, window, cx);
        assert!(app.document.is_none());
    });
    for image in [&source_image, &preview, &segments] { assert!(!in_atlas(cx, image), "reset releases every viewer image and preview"); }
}

/// Recent images and Recent folders are independent session disclosures. A
/// collapsed group paints no rows; toggles keep both histories and the open image.
#[gpui::test]
fn recent_disclosures_toggle_independently(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.0.join("source.png");
    std::fs::write(&source, uniform_bytes()).unwrap();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    // Opening records the image and its folder; the closing draw shows the Load page.
    let (preview, history) = app.update_in(cx, |app, window, cx| {
        app.open(source.clone(), cx); finish(app, window, cx);
        app.wizard.page = Page::Load; cx.notify();
        (app.preferences.source_preview(&source).unwrap().clone(), (app.preferences.sources.snapshot(), app.preferences.directories.snapshot()))
    });
    assert!(!in_atlas(cx, &preview), "collapsed Recent images paints no rows");
    app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::RecentImages)), window, cx);
        assert_eq!(app.workflows.disclosures, [true, false, false, false, false]);
    });
    assert!(in_atlas(cx, &preview), "expanded Recent images paints its preview");
    app.update_in(cx, |app, window, cx| {
        app.dispatch(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::RecentFolders)), window, cx);
        assert_eq!(app.workflows.disclosures, [true, true, false, false, false]);
        assert_eq!((app.preferences.sources.snapshot(), app.preferences.directories.snapshot()), history, "toggles keep both histories");
        assert!(app.document.is_some());
    });
}

#[gpui::test]
fn profile_play_uses_native_source_alpha_routing(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let references = fixtures("profile-replay-window-reference");
    for reference in references.iter().skip(1).filter(|row| !row["id"].as_str().unwrap().starts_with("hidden-window")) {
        let Some(index) = reference["actions"].as_array().unwrap().iter().position(|action| action[0] == "replay" && action[1] == "play") else { continue; };
        let expected = reference["expected"][index]["processing"]["wizardPage"].as_u64().unwrap();
        let source_name = reference["actions"][0][1].as_str().unwrap();
        let source = directory.0.join("replay.png");
        std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["inputs"][source_name].as_str().unwrap()).unwrap()).unwrap();
        app.update_in(cx, |app, window, cx| {
            app.open(source, cx); finish(app, window, cx);
            app.workflow_action(WorkflowAction::ApplyProfile("Default Advanced Settings".into(), false), window, cx); finish(app, window, cx);
            assert_eq!(app.wizard.page, if expected == 6 { Page::Transparency } else { assert_eq!(expected, 12); Page::Palette }, "{}", reference["id"]);
        });
    }
    // Preserve the native EnhanceImage size boundary: 2x2, 4x5 and 5x4 fail;
    // 5x5 completes. The rejected cases must leave a usable source in the GUI.
    for reference in fixtures("tiny-profile-20261020").iter().skip(1) {
        let source = directory.0.join(reference["input"]["name"].as_str().unwrap());
        let bytes = base64::engine::general_purpose::STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap();
        let decoded = openvector::image_input::decode_image(&bytes).unwrap();
        std::fs::write(&source, bytes).unwrap();
        let native_failed = !reference["nativeFailure"].is_null();
        app.update_in(cx, |app, window, cx| {
            app.open(source.clone(), cx); finish(app, window, cx);
            let document = app.document.as_ref().unwrap();
            assert!(document.has_alpha); assert_eq!(document.source_alpha_type, 0);
            app.workflow_action(WorkflowAction::ApplyProfile("Default Advanced Settings".into(), false), window, cx); finish(app, window, cx);
            assert_eq!(app.wizard.page, Page::Palette);
            app.workflow_action(WorkflowAction::ApplyProfile("Default Advanced Settings".into(), true), window, cx); settle(app, window, cx);
            let document = app.document.as_ref().unwrap();
            assert_eq!(document.filename, source);
            assert_eq!((document.width, document.height), (decoded.width, decoded.height));
            if native_failed {
                assert!(app.error, "{}", reference["id"]);
                assert_eq!(app.wizard.page, Page::Palette);
                assert!(document.vectors.is_none());
            } else {
                assert!(!app.error, "{}", app.status);
                assert_eq!(app.wizard.page, Page::Export);
                assert!(document.vectors.is_some());
            }
        });
    }
}

/// Retained native draft lifetime (profile-reset-reference, read through the
/// SaveWidget QLineEdit): startup shows the first unused exact Settings N;
/// direct model saves and a Start Over that only leaves its page keep the
/// draft; reloading over a loaded image and the full reset of Start Over on
/// Load suggest again. Replays, loads and control changes keep the draft in
/// every capture, so only the actions that own it run here.
#[gpui::test]
fn native_settings_name_draft_follows_reloads_and_full_resets(cx: &mut gpui::TestAppContext) {
    for reference in fixtures("profile-reset-reference").iter().skip(1) {
        let directory = Directory::new();
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            for (index, action) in reference["actions"].as_array().unwrap().iter().enumerate() {
                match action[0].as_str().unwrap() {
                    "open" => {
                        let source = directory.0.join("patchwork.bmp");
                        std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["inputs"][action[1].as_str().unwrap()].as_str().unwrap()).unwrap()).unwrap();
                        app.open(source, cx); finish(app, window, cx);
                    }
                    // The native model slot, not the SaveWidget button.
                    "save" => { app.workflows.profiles.put(action[1].as_str().unwrap(), app.current_profile()); app.save_profiles(cx).unwrap(); }
                    "start-over" => { app.dispatch(Action::Start, window, cx); finish(app, window, cx); }
                    _ => {},
                }
                if let Some(name) = reference["expected"][index]["suggestedName"].as_str() {
                    assert_eq!(app.workflows.profile_name.read(cx).text, name, "{} action {index}", reference["id"]);
                }
            }
        });
    }
}

#[gpui::test]
fn advanced_palette_candidates_keep_count_color_and_revert_ownership(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let reference = fixtures("profile-replay-window-reference").into_iter().find(|row| row["id"] == "two-count-seven-play").unwrap();
    let name = reference["actions"][0][1].as_str().unwrap();
    let source = directory.0.join("patchwork.bmp");
    std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["inputs"][name].as_str().unwrap()).unwrap()).unwrap();
    let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(cx, |app, window, cx| {
        app.open(source, cx); finish(app, window, cx);
        app.settings.advanced = true; app.settings.palette_mode = 1; app.color_count = 24;
        app.process("palette", Page::Palette, cx); finish(app, window, cx);
        let argb = &app.document.as_ref().unwrap().palettes[24];
        assert_eq!(argb.len(), 24 * 4);
        let bgra: Vec<f32> = argb.chunks_exact(4).flat_map(|color| color.iter().rev().copied()).collect();
        let selected = openvector::pixel_input::quantize_qcolor_channels(&bgra);
        app.workflow_action(WorkflowAction::Candidate(24), window, cx);
        assert_eq!(app.color_count, 24); assert_eq!(app.workflows.palette_count, 24.0);
        assert_eq!(app.settings.palette, selected);
        app.palette_command(PaletteCommand::Color { index: 0.0, color: vec![0.0, 0.0, 1.0, 1.0] }, cx).unwrap();
        assert!(app.workflows.palette.customized);
        app.palette_command(PaletteCommand::Count(13.0), cx).unwrap();
        app.palette_command(PaletteCommand::Revert, cx).unwrap();
        assert_eq!(app.color_count, 13); assert_eq!(app.workflows.palette_count, 24.0);
        assert_eq!(app.settings.palette, selected); assert!(!app.workflows.palette.customized);

        app.workflow_action(WorkflowAction::Color(0), window, cx);
        app.workflows.fields[0].update(cx, |field, cx| field.set("256", cx));
        app.workflow_action(WorkflowAction::CommitColor, window, cx);
        assert!(app.error);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Color(0)))));
        assert_eq!(app.settings.palette, selected);
        let validation_message = app.status.clone();
        for (field, value) in app.workflows.fields.iter().zip(["17", "84", "201", "128"]) {
            field.update(cx, |field, cx| field.set(value, cx));
        }
        app.workflow_action(WorkflowAction::CommitColor, window, cx);
        assert!(!app.error);
        assert!(app.dialog.is_none());
        assert_ne!(app.status, validation_message, "successful correction must remove the obsolete footer error");
        assert_eq!(&app.settings.palette[..4], &[201.0 / 255.0, 84.0 / 255.0, 17.0 / 255.0, 128.0 / 255.0]);
        assert!(app.workflows.palette.customized);
    });
}

#[gpui::test]
fn public_palette_add_stops_at_twenty_four_selected_colors(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, _, cx| {
        app.wizard.page = Page::Palette;
        app.settings.advanced = true;
        app.color_count = 13;
        app.palette_command(PaletteCommand::Replace([0.25, 0.5, 0.75, 1.0].repeat(23)), cx).unwrap();
    });
    visual.simulate_mouse_move(gpui::point(px(0.0), px(0.0)), None, gpui::Modifiers::default());
    let add = visual.debug_bounds("palette-add").expect("rendered palette Add control").center();
    visual.simulate_click(add, gpui::Modifiers::default());
    let full = app.read_with(visual, |app, _| {
        assert_eq!(app.settings.palette.len(), 24 * 4);
        assert_eq!(&app.settings.palette[23 * 4..], &[1.0; 4]);
        assert_eq!(app.color_count, 13, "Add does not change the requested analysis count");
        assert_eq!(app.workflows.palette_count, 24.0);
        app.settings.palette.clone()
    });
    visual.simulate_click(add, gpui::Modifiers::default());
    app.read_with(visual, |app, _| assert_eq!(app.settings.palette, full, "the public button cannot add a 25th color"));

    app.update_in(visual, |app, window, cx| {
        app.workflow_action(WorkflowAction::Palette(PaletteCommand::Delete(23.0)), window, cx);
        assert_eq!(app.settings.palette.len(), 23 * 4);
        assert_eq!(app.workflows.palette_count, 24.0, "the delete slot leaves this separate owner unchanged");
    });
    visual.simulate_click(add, gpui::Modifiers::default());
    app.read_with(visual, |app, _| assert_eq!(app.settings.palette, full, "deletion re-enables Add using selected length, not paletteColorCount"));
}

#[gpui::test]
fn recent_previews_survive_restart_and_refresh_only_after_successful_reopen(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let preference_path = directory.0.join("gui.json");
    let paths = ["missing.png", "changed.png", "invalid.png"].map(|name| directory.0.join(name));
    for (index, path) in paths.iter().enumerate() {
        image::RgbaImage::from_pixel(32, 24, image::Rgba([30 + index as u8 * 50, 40, 120, 137])).save(path).unwrap();
    }
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preference_path.clone(), cx).unwrap());
    let (history, pixels) = app.update_in(visual, |app, window, cx| {
        for path in &paths { app.open(path.clone(), cx); finish(app, window, cx); }
        (app.preferences.sources.snapshot(), paths.iter().map(|path| app.preferences.source_preview(path).unwrap().as_bytes(0).unwrap().to_vec()).collect::<Vec<_>>())
    });
    std::fs::remove_file(&paths[0]).unwrap();
    image::RgbaImage::from_pixel(40, 20, image::Rgba([5, 200, 17, 255])).save(&paths[1]).unwrap();
    std::fs::write(&paths[2], b"invalid image").unwrap();
    let saved = std::fs::read(&preference_path).unwrap();
    let (restarted, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preference_path.clone(), cx).unwrap());
    visual.run_until_parked();
    restarted.update_in(visual, |app, window, cx| {
        assert!(app.document.is_none());
        assert_eq!(app.preferences.sources.snapshot(), history);
        for (path, expected) in paths.iter().zip(&pixels) {
            assert_eq!(app.preferences.source_preview(path).unwrap().as_bytes(0).unwrap(), expected);
        }
        assert_eq!(std::fs::read(&preference_path).unwrap(), saved, "startup must not rewrite history");
        app.open(paths[1].clone(), cx); finish(app, window, cx);
        let reopened = app.preferences.source_preview(&paths[1]).unwrap().clone();
        assert_eq!(reopened.as_bytes(0).unwrap(), [17, 200, 5, 255].repeat(40 * 20));
        assert_eq!(app.preferences.sources.snapshot()["files"][0]["filename"], paths[1].to_str().unwrap());
        let source = app.document.as_ref().unwrap().source.clone();
        let refreshed = app.preferences.sources.snapshot();
        for index in [0, 2] {
            app.open(paths[index].clone(), cx); settle(app, window, cx);
            assert!(app.error, "a missing or invalid source must fail to reopen");
            assert!(Arc::ptr_eq(&source, &app.document.as_ref().unwrap().source));
            assert_eq!(app.preferences.sources.snapshot(), refreshed);
            assert_eq!(app.preferences.source_preview(&paths[index]).unwrap().as_bytes(0).unwrap(), pixels[index]);
            assert!(Arc::ptr_eq(&reopened, app.preferences.source_preview(&paths[1]).unwrap()));
        }
        // A queue decode of the same path is independent of the recent cache.
        image::RgbaImage::from_pixel(40, 20, image::Rgba([200, 5, 17, 255])).save(&paths[1]).unwrap();
        app.batch_queue.add_files(&[paths[1].to_string_lossy().into_owned(), paths[2].to_string_lossy().into_owned()]);
        app.load_previews(cx);
    });
    visual.run_until_parked();
    restarted.update_in(visual, |app, window, cx| {
        let queue_preview = app.workflows.thumbnails[&paths[1]].as_ref().unwrap().clone();
        assert_eq!(queue_preview.as_bytes(0).unwrap(), [17, 5, 200, 255].repeat(40 * 20));
        assert_eq!(app.preferences.source_preview(&paths[1]).unwrap().as_bytes(0).unwrap(), [17, 200, 5, 255].repeat(40 * 20));
        assert!(app.workflows.thumbnails[&paths[2]].is_err());
        assert_eq!(app.preferences.source_preview(&paths[2]).unwrap().as_bytes(0).unwrap(), pixels[2]);
        app.workflow_action(WorkflowAction::ClearSources, window, cx);
        for path in &paths { assert!(app.preferences.source_preview(path).is_none()); }
        assert!(Arc::ptr_eq(&queue_preview, app.workflows.thumbnails[&paths[1]].as_ref().unwrap()), "clearing recents keeps queue previews");
        let restored = Preferences::load(preference_path.clone()).unwrap();
        assert!(restored.sources.snapshot()["files"].as_array().unwrap().is_empty());
    });
}

#[gpui::test]
fn rejected_stores_prevent_app_construction_without_changing_either_file(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("valid/gui.json"), cx).unwrap());
    for store in ["gui.json", "profiles.json"] {
        for bytes in [b"{invalid JSON\0\xff".as_slice(), b"null", b"[]"] {
            let rejected_directory = Directory::new();
            let preferences = rejected_directory.0.join("gui.json");
            let profiles = rejected_directory.0.join("profiles.json");
            Preferences::new(preferences.clone()).save().unwrap();
            SavedAdvancedProfiles::default().write_file(&profiles).unwrap();
            let rejected = rejected_directory.0.join(store);
            std::fs::write(&rejected, bytes).unwrap();
            let before = [std::fs::read(&preferences).unwrap(), std::fs::read(&profiles).unwrap()];
            app.update_in(visual, |_, _, cx| {
                let error = match OpenVectorApp::with_preferences(preferences.clone(), cx) {
                    Ok(_) => panic!("a rejected store must not create a writable application"),
                    Err(error) => error,
                };
                assert!(error.contains(rejected.to_str().unwrap()), "{error}");
            });
            assert_eq!([std::fs::read(&preferences).unwrap(), std::fs::read(&profiles).unwrap()], before);
        }
    }
}
