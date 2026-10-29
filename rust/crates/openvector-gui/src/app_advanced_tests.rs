use super::*;
use base64::Engine;
use std::io::Read;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-advanced-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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
    text.lines().skip(1).map(|line| serde_json::from_str(line).unwrap()).collect()
}
fn page_id(page: Page) -> i64 {
    match page { Page::Load => 0, Page::Mode => 4, Page::Transparency => 6, Page::Palette => 12,
        Page::Segmentation => 13, Page::Editing => 14, Page::Background => 15, Page::Smoothing => 16, Page::Export => 17,
        _ => panic!("unexpected advanced page {page:?}") }
}
fn compare_keys(document: &DocumentView, expected: &Value) {
    let keys = &document.stage_keys;
    for (name, actual) in [("palette", &keys.palette), ("reservedC0", &keys.reserved_c0), ("segmentation", &keys.segmentation),
        ("segmentationEdits", &keys.segmentation_edits), ("smoothing", &keys.smoothing), ("fitting", &keys.fitting)] {
        if let Some(expected) = expected.get(name) { assert_eq!(actual, expected.as_str().unwrap(), "{name}"); }
    }
}
fn finish(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        app.poll(cx);
        if !app.busy && app.pending.is_empty() { break; }
        assert!(Instant::now() < deadline, "worker did not settle: {}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!app.error, "{}", app.status);
}

#[gpui::test]
fn native_guided_dirty_next_updates_dirty_back_reverts_and_clean_navigation_advances(cx: &mut gpui::TestAppContext) {
    let cases = fixtures("profile-guided-reference").into_iter().filter(|row| matches!(row["id"].as_str().unwrap(),
        "guided-default-to-export" | "guided-dirty-segmentation-update" | "guided-dirty-smoothing-update"
        | "backward-dirty-segmentation-revert" | "backward-dirty-smoothing-revert"))
        .chain(fixtures("profile-history-reference").into_iter().filter(|row| row["id"] == "guided-fill-back-revert-leave"));
    for reference in cases {
        let directory = Directory::new();
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            for (index, action) in reference["actions"].as_array().unwrap().iter().enumerate() {
                match action[0].as_str().unwrap() {
                    "open" => {
                        let source = directory.0.join("patchwork.bmp");
                        std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["inputs"][action[1].as_str().unwrap()].as_str().unwrap()).unwrap()).unwrap();
                        app.open(source, cx); finish(app, cx);
                    }
                    "replay" => { app.dispatch(Action::Workflow(WorkflowAction::ApplyProfile(action[2].as_str().unwrap().into(), false)), window, cx); finish(app, cx); }
                    "wait" => {
                        if let Some((ticket, deadline)) = app.automatic.scheduled() {
                            assert!(action[1].as_u64().unwrap() >= AutoUpdate::DELAY.as_millis() as u64);
                            app.automatic_due(ticket, deadline, cx); finish(app, cx);
                        }
                    }
                    "next" => { app.dispatch(Action::Next, window, cx); finish(app, cx); }
                    "set" => {
                        match action[2].as_str().unwrap() {
                            "processBackPress()" => app.dispatch(Action::Back, window, cx),
                            "setAdvSegmentationComplexity(int)" => app.dispatch(Action::Set("complexity", action[3].clone()), window, cx),
                            "setAdvContourSmoothness(int)" => app.dispatch(Action::Set("smoothness", action[3].clone()), window, cx),
                            slot => panic!("unhandled native control {slot}"),
                        }
                        finish(app, cx);
                    }
                    "edit" => {
                        let color: Vec<Value> = action[5].as_array().unwrap().iter().rev().cloned().collect();
                        app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"fill","x":action[2],"y":action[3],"color":color,"commit":action[6]}) }, None, cx);
                        finish(app, cx);
                    }
                    "document" => {
                        let expected = &reference["expected"][index]["document"]["state"];
                        compare_keys(app.document.as_ref().unwrap(), &expected["stageKeys"]);
                        let bits = expected["settingsBits"].as_str().unwrap();
                        let bytes: Vec<u8> = (0..bits.len()).step_by(2).map(|offset| u8::from_str_radix(&bits[offset..offset + 2], 16).unwrap()).collect();
                        assert_eq!(app.document.as_ref().unwrap().committed_count, i32::from_le_bytes(bytes[8..12].try_into().unwrap()));
                    }
                    "load" | "show" | "snapshot" => {},
                    action => panic!("unhandled native action {action}"),
                }
                assert_eq!(page_id(app.wizard.page), reference["expected"][index]["processing"]["wizardPage"].as_i64().unwrap(), "{} action {index}: {action}", reference["id"]);
                if let Some(history) = reference["expected"][index].get("history") {
                    let document = app.document.as_ref().unwrap();
                    if let Some(key) = history["editor"]["key"].as_str() { assert_eq!(document.editor_key.as_deref(), Some(key)); }
                    assert_eq!(document.undo, !history["undo"].as_array().unwrap().is_empty());
                    assert_eq!(document.redo, !history["redo"].as_array().unwrap().is_empty());
                }
            }
        });
    }
}

fn finish_worker(worker: &Worker, settings: &mut DocumentSettings) -> DocumentView {
    let mut document = None;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
            Event::Document(view) => document = Some(view),
            Event::Selection { settings: selected, .. } => *settings = selected,
            Event::Finished(result) => { result.unwrap(); return document.unwrap(); }
            _ => {},
        }
    }
}

#[test]
fn native_auto_update_fixtures_use_stage_keys_coalesce_and_reenter() {
    for reference in fixtures("auto-update-reference").into_iter().chain(fixtures("auto-update-reentry-reference")) {
        let directory = Directory::new(); let source = directory.0.join(reference["input"]["name"].as_str().unwrap());
        std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap()).unwrap();
        let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
        let mut settings = openvector::settings_profile::default_selected_profile().settings;
        worker.submit(Command::Open { path: source, settings: Value::Object(settings.to_json()) }).unwrap();
        finish_worker(&worker, &mut settings);
        let mut input = Value::Object(settings.to_json()); input["extension"] = json!("bmp"); input["count"] = json!(2);
        worker.submit(Command::Automatic(input)).unwrap(); let mut document = finish_worker(&worker, &mut settings);
        compare_keys(&document, &reference["expectedAutomatic"]["stageKeys"]);
        let mut automatic = AutoUpdate::default(); let mut checked = [true; 3]; let mut page = None;
        let mut now = Instant::now(); let mut completed = 0;
        for action in reference["actions"].as_array().unwrap() {
            let index = |page| match page { Some(Page::Palette) => 0, Some(Page::Segmentation) => 1, _ => 2 };
            match action["action"].as_i64().unwrap() {
                19 => {
                    let key = match action["slot"].as_str().unwrap() { "setUseAdvancedMode(bool)" => "advanced",
                        "setAdvContourSmoothness(int)" => "smoothness", "setAdvCurveComplexity(int)" => "curveComplexity",
                        "setAdvColorSensitivity(int)" => "sensitivity", "setAdvSegmentationComplexity(int)" => "complexity", slot => panic!("{slot}") };
                    let mut input = Value::Object(settings.to_json());
                    let value = if key == "advanced" { json!(action["value"] == 1) } else { action["value"].clone() };
                    let changed = input[key] != value; input[key] = value;
                    settings = openvector::document_controls::read_document_settings(&input).unwrap();
                    automatic.synchronize(page, checked[index(page)], false, changed, now);
                }
                29 => {
                    page = match action["value"].as_i64().unwrap() { 12 => Some(Page::Palette), 13 => Some(Page::Segmentation), 16 => Some(Page::Smoothing), _ => None };
                    if page == Some(Page::Palette) && settings.palette_mode == 0 { settings.palette_mode = 1; }
                    automatic.synchronize(page, checked[index(page)], page.is_some_and(|page| page_needs_update(page, &settings, &document.stage_keys)), false, now);
                }
                33 => { checked[index(page)] = !checked[index(page)]; automatic.synchronize(page, checked[index(page)], false, false, now); }
                32 => {
                    now += Duration::from_millis(action["milliseconds"].as_u64().unwrap());
                    if let Some((ticket, _)) = automatic.scheduled() {
                        if let Some(page) = automatic.fire(ticket, now) {
                            let mut input = Value::Object(settings.to_json()); input["count"] = json!(2);
                            worker.submit(Command::Process { stage: page.update_stage().unwrap(), input }).unwrap();
                            // Native captures keep their separately selected settings while
                            // observing the committed worker document after each timeout.
                            document = finish_worker(&worker, &mut settings.clone());
                        }
                    }
                    compare_keys(&document, &reference["expectedCompleted"][completed]["stageKeys"]); completed += 1;
                }
                24 | 31 => {},
                action => panic!("unhandled native action {action}"),
            }
        }
        assert_eq!(completed, reference["expectedCompleted"].as_array().unwrap().len());
    }
}

#[test]
fn auto_update_tickets_are_one_shot_and_cancelled_work_cannot_run() {
    let mut automatic = AutoUpdate::default(); let now = Instant::now();
    automatic.synchronize(Some(Page::Smoothing), true, true, false, now);
    let (old, deadline) = automatic.scheduled().unwrap(); assert_eq!(deadline, now + Duration::from_millis(500));
    assert_eq!(automatic.fire(old, deadline - Duration::from_millis(1)), None);
    automatic.synchronize(Some(Page::Smoothing), true, false, true, now + Duration::from_millis(200));
    let (latest, deadline) = automatic.scheduled().unwrap(); assert_ne!(latest, old);
    assert_eq!(deadline, now + Duration::from_millis(700)); assert_eq!(automatic.fire(old, deadline), None);
    automatic.synchronize(None, false, false, false, now + Duration::from_millis(300));
    assert_eq!(automatic.fire(latest, deadline), None);
    automatic.synchronize(Some(Page::Smoothing), false, false, false, deadline);
    assert!(automatic.scheduled().is_none());
    automatic.synchronize(Some(Page::Smoothing), true, false, false, deadline);
    let (ticket, deadline) = automatic.scheduled().unwrap();
    assert_eq!(automatic.fire(ticket, deadline), Some(Page::Smoothing)); assert_eq!(automatic.fire(ticket, deadline), None);
    automatic.synchronize(Some(Page::Smoothing), true, false, true, deadline);
    let (ticket, deadline) = automatic.scheduled().unwrap(); automatic.suspend();
    assert_eq!(automatic.fire(ticket, deadline), None); automatic.resume(deadline);
    let (ticket, deadline) = automatic.scheduled().unwrap(); automatic.cancel();
    assert_eq!(automatic.fire(ticket, deadline), None); automatic.resume(deadline); assert!(automatic.scheduled().is_none());
    automatic.synchronize(Some(Page::Palette), true, true, false, deadline);
    let (ticket, deadline) = automatic.scheduled().unwrap(); automatic.reset(); assert_eq!(automatic.fire(ticket, deadline), None);
}

#[test]
fn advanced_revert_owns_only_native_control_fields_and_count() {
    let committed = openvector::settings_profile::default_advanced_profile().settings;
    let mut selected = committed.clone();
    selected.category = 2; selected.quality = 0; selected.palette_mode = 2;
    selected.palette = vec![0.0, 0.5, 1.0, 1.0]; selected.background_mode = 1; selected.background = [1.0; 4];
    assert_eq!(AdvancedControls::new(&selected, 2), AdvancedControls::new(&committed, 2));
    let before = selected.clone(); let mut count = 9;
    selected.complexity = 11; selected.curve_complexity = 12; selected.anti_alias = false;
    AdvancedControls::new(&committed, 2).restore(&mut selected, &mut count);
    assert_eq!(selected, before); assert_eq!(count, 2);
    let keys = openvector::image_document::StageKeys {
        segmentation: openvector::document_controls::document_segmentation_key(&selected),
        smoothing: openvector::document_controls::document_smoothing_key(&selected, ""),
        fitting: openvector::document_controls::document_fitting_key(&selected, ""), ..Default::default()
    };
    assert!(!page_needs_update(Page::Segmentation, &selected, &keys));
    assert!(!page_needs_update(Page::Smoothing, &selected, &keys));
    let edited_keys = openvector::image_document::StageKeys { segmentation_edits: "committed edit".into(), ..keys };
    assert!(page_needs_update(Page::Smoothing, &selected, &edited_keys));
    assert!(!page_needs_update(Page::Palette, &selected, &edited_keys));
    selected.palette.clear();
    assert!(!page_needs_update(Page::Palette, &selected, &edited_keys));
    selected.palette_mode = 1;
    assert!(page_needs_update(Page::Palette, &selected, &edited_keys));
}

#[gpui::test]
fn advanced_entry_checkbox_revert_and_confirmation_use_real_worker(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let reference = fixtures("auto-update-reference").remove(0);
    let source = directory.0.join("square.bmp");
    std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap()).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        app.wizard.auto_palette = false;
        app.dispatch(Action::Advanced, window, cx);
        assert_eq!(app.wizard.page, Page::Palette); assert!(!app.busy); assert!(app.automatic.scheduled().is_none());
        app.dispatch(Action::Auto("palette"), window, cx);
        let (ticket, deadline) = app.automatic.scheduled().unwrap();
        app.automatic_due(ticket, deadline - Duration::from_millis(1), cx); assert!(!app.busy);
        app.automatic_due(ticket, deadline, cx); finish(app, cx);
        assert!(!app.settings.palette.is_empty()); assert!(!app.dirty_page());
        app.dispatch(Action::Next, window, cx); finish(app, cx); assert_eq!(app.wizard.page, Page::Segmentation);
        app.dispatch(Action::Next, window, cx); finish(app, cx); assert_eq!(app.wizard.page, Page::Segmentation);
        app.dispatch(Action::Next, window, cx); finish(app, cx); assert_eq!(app.wizard.page, Page::Editing);
        app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"fill","x":6,"y":6,"color":[0,0,1,1]}) }, None, cx); finish(app, cx);
        assert!(app.changed_editing()); app.dispatch(Action::Back, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Editing); assert!(!app.changed_editing());
        app.wizard.auto_vectorization = false;
        let baseline = edited_pixels(app);
        app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"fill","x":6,"y":6,"color":[0,0,1,1]}) }, None, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx); finish(app, cx);
        app.dispatch(Action::Back, window, cx); finish(app, cx);
        app.dispatch(Action::Back, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Editing);
        assert_eq!(edited_pixels(app), baseline);
        assert!(!app.document.as_ref().unwrap().undo);
        assert!(!app.document.as_ref().unwrap().redo);
        assert!(app.document.as_ref().unwrap().has_applied_edits);
        assert!(!app.document.as_ref().unwrap().has_committed_edits);
        app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"fill","x":6,"y":6,"color":[0,0,1,1]}) }, None, cx); finish(app, cx);
        let edited = edited_pixels(app);
        app.dispatch(Action::Next, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx); finish(app, cx); // Explicit Update.
        assert!(!app.error, "{}", app.status);
        app.dispatch(Action::Back, window, cx); finish(app, cx);
        assert!(!app.changed_editing());
        app.dispatch(Action::Back, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Segmentation);
        assert_eq!(edited_pixels(app), edited);
        let key = app.document.as_ref().unwrap().stage_keys.segmentation_edits.clone(); assert!(!key.is_empty());
        app.dispatch(Action::Number("complexity", 1), window, cx); assert!(!app.busy);
        let (ticket, deadline) = app.automatic.scheduled().unwrap(); app.automatic_due(ticket, deadline, cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::RunProcess("segmentation", Page::Segmentation, _))))));
        assert!(!app.busy); assert!(app.pending.is_empty()); app.dispatch(Action::Dismiss, window, cx);
        assert!(app.automatic.scheduled().is_none()); assert_eq!(app.document.as_ref().unwrap().stage_keys.segmentation_edits, key);
        app.dispatch(Action::Back, window, cx); finish(app, cx); assert_eq!(app.wizard.page, Page::Segmentation);
        assert!(!app.dirty_page());
        app.dispatch(Action::Number("complexity", 1), window, cx);
        let (ticket, deadline) = app.automatic.scheduled().unwrap(); app.dispatch(Action::Cancel, window, cx);
        app.automatic_due(ticket, deadline, cx); assert!(!app.busy); assert!(app.automatic.scheduled().is_none());
        app.dispatch(Action::Number("complexity", 1), window, cx);
        let (ticket, deadline) = app.automatic.scheduled().unwrap();
        app.enqueue(Command::Commit(app.task_input()), None, cx);
        app.automatic_due(ticket, deadline, cx); assert!(app.pending.is_empty());
        finish(app, cx);
        let (ticket, deadline) = app.automatic.scheduled().expect("busy pipeline defers one pending update");
        let source = app.document.as_ref().unwrap().filename.clone();
        let old_source = app.document.as_ref().unwrap().source.clone();
        app.open(source.clone(), cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::OpenAccepted(_))))));
        app.dispatch(Action::Workflow(WorkflowAction::OpenAccepted(source)), window, cx);
        app.automatic_due(ticket, deadline, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Mode); assert!(app.automatic.scheduled().is_none());
        assert!(!Arc::ptr_eq(&old_source, &app.document.as_ref().unwrap().source));
    });
}

fn edited_pixels(app: &OpenVectorApp) -> Vec<u8> {
    app.document.as_ref().unwrap().segments.as_ref().unwrap().as_bytes(0).unwrap().to_vec()
}

fn prepare_editor(app: &mut OpenVectorApp, source: PathBuf, cx: &mut Context<OpenVectorApp>) {
    app.open(source, cx); finish(app, cx);
    app.settings.category = 2; app.settings.quality = 0;
    app.settings.palette_mode = 2; app.settings.background_mode = 0;
    app.settings.advanced = true;
    app.wizard.auto_vectorization = false;
    app.enqueue(Command::Process { stage: "fitting", input: app.task_input() }, None, cx); finish(app, cx);
    app.wizard.edit_origin = Page::Segmentation;
    app.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Page::Editing), cx); finish(app, cx);
}

#[gpui::test]
fn editing_next_applies_before_separate_auto_update_and_basic_finishes(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.0.join("source.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([255, 255, 255])).save(&source).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        prepare_editor(app, source, cx);
        let original = edited_pixels(app);
        app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"pencil","from":[3,3],"to":[10,10],"color":[0,0,1,1]}) }, None, cx);
        finish(app, cx);
        let painted = edited_pixels(app); assert_ne!(painted, original);
        app.next(cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Smoothing);
        assert!(app.document.as_ref().unwrap().vectors.is_none(), "Advanced Next must not fit vectors");
        assert!(app.automatic.scheduled().is_none(), "unchecked Auto-update must not schedule work");
        assert_eq!(edited_pixels(app), painted);
        app.back(cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Editing);
        app.enqueue(Command::Edit { action: "undo", input: Value::Null }, None, cx); finish(app, cx);
        assert_eq!(edited_pixels(app), original);
        app.back(cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Segmentation, "empty active history ignores redo");
        assert_eq!(edited_pixels(app), original);
        app.next(cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Editing);
        app.enqueue(Command::Edit { action: "redo", input: Value::Null }, None, cx); finish(app, cx);
        assert_eq!(edited_pixels(app), painted);
        app.next(cx); finish(app, cx);
        app.dispatch(Action::Auto("fitting"), window, cx);
        let (ticket, deadline) = app.automatic.scheduled().unwrap();
        app.dispatch(Action::Cancel, window, cx);
        app.automatic_due(ticket, deadline, cx);
        assert!(app.document.as_ref().unwrap().vectors.is_none());
        assert_eq!(app.wizard.page, Page::Smoothing);
        app.back(cx); finish(app, cx);
        app.next(cx); finish(app, cx);
        let (ticket, deadline) = app.automatic.scheduled().unwrap();
        app.automatic_due(ticket, deadline, cx); finish(app, cx);
        assert!(app.document.as_ref().unwrap().vectors.is_some());
        app.back(cx); finish(app, cx);
        app.enqueue(Command::Edit { action: "undo", input: Value::Null }, None, cx); finish(app, cx);
        app.settings.advanced = false;
        app.next(cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Review);
        assert_eq!(edited_pixels(app), original);
        assert!(app.document.as_ref().unwrap().vectors.is_some(), "Basic Next must finish vectors");
    });
}

#[gpui::test]
fn held_moves_queue_behind_gestures_not_processing_or_cancelled_navigation(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.0.join("source.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([255, 255, 255])).save(&source).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        prepare_editor(app, source.clone(), cx);
        app.enqueue(Command::Edit { action: "gesture", input: json!({"phase":"color","color":[0,0,1,1]}) }, None, cx); finish(app, cx);
        app.dispatch(Action::Tool(tool::PENCIL), window, cx); finish(app, cx);
        let original = edited_pixels(app);
        let position = |app: &OpenVectorApp, x: f64, y: f64| gpui::point(
            app.viewport.origin.x + px((app.view.origin.x + x * app.view.zoom) as f32),
            app.viewport.origin.y + px((app.view.origin.y + y * app.view.zoom) as f32));
        let movement = |app: &mut OpenVectorApp, x, y, cx: &mut Context<OpenVectorApp>| {
            app.pointer_move(&gpui::MouseMoveEvent { position: position(app, x, y), pressed_button: Some(MouseButton::Left), ..Default::default() }, cx);
        };
        // No local press: the first sample seeds the native previous point.
        // The next sample must survive while that gesture is still in flight.
        movement(app, 3.0, 3.0, cx);
        movement(app, 10.0, 10.0, cx);
        finish(app, cx);
        let painted = edited_pixels(app); assert_ne!(painted, original);
        app.enqueue(Command::Edit { action: "undo", input: Value::Null }, None, cx); finish(app, cx);
        assert_eq!(edited_pixels(app), original);
        app.enqueue(Command::Edit { action: "redo", input: Value::Null }, None, cx); finish(app, cx);
        assert_eq!(edited_pixels(app), painted);
        app.pointer_move(&gpui::MouseMoveEvent { position: position(app, 2.0, 12.0), ..Default::default() }, cx);
        app.pointer_move(&gpui::MouseMoveEvent { position: position(app, 2.0, 12.0), pressed_button: Some(MouseButton::Right), ..Default::default() }, cx);
        app.pointer("release", position(app, 2.0, 12.0), 2, cx);
        finish(app, cx);
        assert_eq!(edited_pixels(app), painted, "hover clears a left release missed outside the view");

        // A processing command behind a gesture is already a boundary.
        movement(app, 10.0, 10.0, cx);
        app.enqueue(Command::Commit(app.task_input()), None, cx);
        movement(app, 2.0, 12.0, cx);
        finish(app, cx);
        assert_eq!(edited_pixels(app), painted);
        app.enqueue(Command::Process { stage: "vectors", input: app.task_input() }, None, cx);
        movement(app, 2.0, 12.0, cx);
        finish(app, cx);
        assert_eq!(edited_pixels(app), painted);
        app.dialog = Some(Dialog::About);
        movement(app, 2.0, 12.0, cx);
        app.dialog = None;
        finish(app, cx);
        assert_eq!(edited_pixels(app), painted);

        // Cancel drops the queued navigation and blocks further samples until
        // the actual in-flight gesture finishes, even if it already completed.
        movement(app, 10.0, 10.0, cx);
        app.enqueue(Command::Edit { action: "commit", input: app.task_input() }, Some(Page::Smoothing), cx);
        app.dispatch(Action::Cancel, window, cx);
        movement(app, 2.0, 12.0, cx);
        let deadline = Instant::now() + Duration::from_secs(120);
        while app.busy || !app.pending.is_empty() {
            app.poll(cx);
            assert!(Instant::now() < deadline, "cancelled gesture did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.error);
        assert_eq!(app.status, "Cancelled");
        assert_eq!(app.wizard.page, Page::Editing);
        assert_eq!(edited_pixels(app), painted);
        app.open(source.clone(), cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::OpenAccepted(_))))));
        app.dispatch(Action::Workflow(WorkflowAction::OpenAccepted(source)), window, cx);
        movement(app, 2.0, 12.0, cx);
        finish(app, cx);
        assert_eq!(app.wizard.page, Page::Mode);
        assert!(app.document.as_ref().unwrap().segments.is_none());
        assert!(!app.document.as_ref().unwrap().undo);
    });
}
