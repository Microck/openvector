//! Source admission through the real engine worker and GPUI app lifecycle.
use super::*;
#[path = "../../openvector/tests/common/mod.rs"]
mod common;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-source-sizing-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
    fn source(&self, width: u32, height: u32) -> PathBuf {
        let path = self.0.join(format!("{width}x{height}.png"));
        image::RgbImage::from_fn(width, height, |x, _| image::Rgb(if x < width / 2 { [255; 3] } else { [0; 3] })).save(&path).unwrap();
        path
    }
    fn native_source(&self, id: &str) -> PathBuf {
        static INPUTS: std::sync::LazyLock<Vec<Value>> =
            std::sync::LazyLock::new(|| common::read_ndjson("fresh-conversion-20261020.ndjson.gz"));
        let input = &INPUTS.iter().find(|row| row["id"] == id).expect("retained native input")["input"];
        let path = self.0.join(input["name"].as_str().unwrap());
        std::fs::write(&path, common::b64(input["bytes"].as_str().unwrap())).unwrap();
        path
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn settle(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        app.poll(cx);
        if !app.busy && app.pending.is_empty() { break; }
        assert!(Instant::now() < deadline, "worker did not settle: {}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn finish(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    settle(app, cx); assert!(!app.error, "{}", app.status);
}
fn dimensions(app: &OpenVectorApp) -> (usize, usize) {
    let document = app.document.as_ref().unwrap(); (document.width, document.height)
}

#[gpui::test]
fn sizing_is_once_per_admission_and_preserves_file_controls_and_recent_metadata(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.source(1603, 1003);
    let original_bytes = std::fs::read(&source).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        for shrink in [false, true] {
            app.open(source.clone(), cx); finish(app, cx);
            assert_eq!(app.source_sizing, Some(true));
            let history = app.preferences.sources.snapshot();
            let preview = app.preferences.source_preview(&source).unwrap().clone();
            let committed = app.document.as_ref().unwrap().settings.to_json();
            app.settings.advanced = true; app.settings.complexity = 7; app.settings.palette = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
            let selected = app.settings.clone();
            app.dispatch(Action::SourceSizing(shrink), window, cx);
            // Advanced isolates sizing from Basic's separate classification commit.
            app.dispatch(Action::Advanced, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Palette);
            assert_eq!(dimensions(app), if shrink { (802, 502) } else { (1603, 1003) });
            assert_eq!(app.source_sizing, if shrink { None } else { Some(false) });
            assert!(app.source_sizing_consumed);
            assert_eq!(app.settings.to_json(), selected.to_json());
            assert_eq!(app.document.as_ref().unwrap().settings.to_json(), committed);
            assert_eq!(app.preferences.sources.snapshot(), history);
            assert!(Arc::ptr_eq(app.preferences.source_preview(&source).unwrap(), &preview));
            assert_eq!(app.document.as_ref().unwrap().filename, source);
            let raster = app.document.as_ref().unwrap().source.clone();
            app.dispatch(Action::Back, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Palette); // First Back restores changed advanced controls.
            app.dispatch(Action::Back, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Mode);
            app.dispatch(Action::SourceSizing(!shrink), window, cx);
            app.dispatch(Action::Advanced, window, cx); finish(app, cx);
            assert!(Arc::ptr_eq(&raster, &app.document.as_ref().unwrap().source));
            assert_eq!(app.source_sizing, if shrink { None } else { Some(true) });
            assert!(app.source_sizing_consumed);
        }
        assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
    });
}

#[gpui::test]
fn basic_shrinks_before_native_recommendation_admission_and_revisit_keeps_the_raster(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.0.join("1603x1003.png");
    // Same RGBA pixels as the guarded default-1603x1003 native admission.
    image::RgbaImage::from_fn(1603, 1003, |x, y| image::Rgba([
        ((x * 17 + y * 31) % 256) as u8, ((x * 73 + y * 7) % 256) as u8,
        ((x * 3 + y * 97) % 256) as u8, ((x * 29 + y * 43) % 256) as u8,
    ])).save(&source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); finish(app, cx);
        assert_eq!(app.source_sizing, Some(true));
        let history = app.preferences.sources.snapshot();
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(dimensions(app), (802, 502));
        assert_eq!(app.wizard.page, Page::ImageType);
        assert_eq!(app.source_sizing, None);
        assert_eq!(app.source_recommendations, Some([1, 2, 2]));
        assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], [1, 2, 2]);
        assert_eq!(app.settings.background_mode, -1);
        assert_eq!(app.color_count, 2);
        assert_eq!(app.preferences.sources.snapshot(), history);
        let reduced = app.document.as_ref().unwrap().source.clone();
        app.dispatch(Action::Back, window, cx);
        assert_eq!(app.wizard.page, Page::Mode);
        app.dispatch(Action::SourceSizing(false), window, cx);
        app.dispatch(Action::Basic, window, cx);
        assert!(!app.busy);
        assert_eq!(app.wizard.page, Page::ImageType);
        assert_eq!(app.source_sizing, None);
        assert!(Arc::ptr_eq(&reduced, &app.document.as_ref().unwrap().source));
        assert_eq!(app.source_recommendations, Some([1, 2, 2]));
        assert_eq!(std::fs::read(&source).unwrap(), original);
    });
}

#[gpui::test]
fn mode_exits_and_profile_replays_prepare_the_source(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.source(1601, 1001);
    let mut profile = openvector::settings_profile::default_selected_profile();
    profile.settings.category = 2; profile.settings.palette_mode = 2; profile.settings.background_mode = 0;
    let mut profiles = openvector::saved_profiles::SavedAdvancedProfiles::default();
    profiles.put("Sizing", profile.clone());
    profiles.write_file(&directory.0.join("profiles.json")).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        assert!(app.preferences.recent_basic.record_completion(&profile, 0, true, false));
        for action in [Action::Automatic, Action::Advanced,
            Action::Workflow(WorkflowAction::ApplyProfile("Sizing".into(), false)),
            Action::Workflow(WorkflowAction::ApplyProfile("Sizing".into(), true)),
            Action::Workflow(WorkflowAction::ReplayRecentBasic(0))] {
            app.open(source.clone(), cx); finish(app, cx);
            app.dispatch(action, window, cx); finish(app, cx);
            assert_eq!(dimensions(app), (801, 501));
            assert_eq!(app.source_sizing, None);
            assert_ne!(app.wizard.page, Page::Mode);
        }
    });
}

#[gpui::test]
fn failed_replacement_retains_choice_and_threshold_sources_offer_none(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.source(1603, 1003);
    let threshold = directory.source(1600, 1000);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        app.dispatch(Action::SourceSizing(false), window, cx);
        app.open(directory.0.join("missing.png"), cx); settle(app, cx);
        assert!(app.error); assert_eq!(app.source_sizing, Some(false));
        assert_eq!(dimensions(app), (1603, 1003));
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(dimensions(app), (1603, 1003)); assert_eq!(app.source_sizing, Some(false));
        assert!(app.source_sizing_consumed);
        // A failed replacement must not rearm or resize the retained, consumed source,
        // whether it starts on Mode or on a later page.
        // `settle`, not `finish`: synchronous page changes keep the failed load's error.
        fn back_to_mode(app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>) {
            while app.wizard.page != Page::Mode { app.dispatch(Action::Back, window, cx); settle(app, cx); }
        }
        for from_mode in [true, false] {
            back_to_mode(app, window, cx);
            app.dispatch(Action::SourceSizing(true), window, cx);
            if !from_mode { app.dispatch(Action::Advanced, window, cx); settle(app, cx); }
            app.open(directory.0.join("missing.png"), cx); settle(app, cx);
            assert!(app.error);
            assert_eq!(dimensions(app), (1603, 1003));
            assert!(app.source_sizing_consumed);
            back_to_mode(app, window, cx);
            app.dispatch(Action::Advanced, window, cx); settle(app, cx);
            assert_eq!(dimensions(app), (1603, 1003));
        }
        app.open(threshold, cx); finish(app, cx);
        assert_eq!(app.source_sizing, None);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(dimensions(app), (1600, 1000));
        app.enqueue(Command::Reset(app.task_input()), Some(Page::Load), cx); finish(app, cx);
        assert!(app.document.is_none()); assert_eq!(app.source_sizing, None);
    });
}

#[gpui::test]
fn source_back_consumes_recommended_and_replacement_page_controls_fresh_gate(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.source(1603, 1003); let replacement = directory.source(1601, 1001);
    let bytes = std::fs::read(&replacement).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); finish(app, cx);
        let history = app.preferences.sources.snapshot();
        app.dispatch(Action::Back, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Load);
        assert_eq!(dimensions(app), (802, 502));
        assert!(app.source_sizing_consumed);
        assert_eq!(app.preferences.sources.snapshot(), history);
        app.dispatch(Action::Next, window, cx);
        app.dispatch(Action::Advanced, window, cx); finish(app, cx);

        // A later-page replacement and same-path reopen each get a fresh gate.
        for _ in 0..2 {
            app.open(replacement.clone(), cx); finish(app, cx);
            assert_eq!(dimensions(app), (1601, 1001));
            assert_eq!(app.source_sizing, Some(true));
            assert!(!app.source_sizing_consumed);
            app.dispatch(Action::Advanced, window, cx); finish(app, cx);
            assert_eq!(dimensions(app), (801, 501));
        }

        // A reopen started on Source consumes the new gate when the new raster is admitted.
        app.open(replacement.clone(), cx); finish(app, cx);
        app.open(replacement.clone(), cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Mode);
        assert_eq!(app.source_sizing, Some(true));
        assert!(app.source_sizing_consumed);
        assert_eq!(dimensions(app), (1601, 1001));
        for shrink in [false, true] {
            app.dispatch(Action::SourceSizing(shrink), window, cx);
            app.dispatch(Action::Advanced, window, cx); finish(app, cx);
            assert_eq!(dimensions(app), (1601, 1001));
            app.dispatch(Action::Back, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Mode);
        }
        app.import_clipboard(Arc::new(bytes), cx); finish(app, cx);
        assert_eq!(app.document.as_ref().unwrap().origin, SourceOrigin::Clipboard);
        assert!(app.source_sizing_consumed);
        app.dispatch(Action::Advanced, window, cx); finish(app, cx);
        assert_eq!(dimensions(app), (1601, 1001));
        assert_eq!(app.source_recommendations, None);
    });
}

#[gpui::test]
fn cancellation_keeps_pending_choice_until_real_preparation_acknowledgment(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = directory.source(1603, 1003);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        app.dispatch(Action::SourceSizing(false), window, cx);
        app.dispatch(Action::Back, window, cx);
        let mut completed = Vec::new();
        loop {
            let event = app.worker.as_ref().unwrap().events.recv_timeout(Duration::from_secs(120)).unwrap();
            let finished = matches!(event, Event::Finished(_));
            completed.push(event);
            if finished { break; }
        }
        let (replay, receiver) = std::sync::mpsc::channel();
        let original = std::mem::replace(&mut app.worker.as_mut().unwrap().events, receiver);
        app.dispatch(Action::Cancel, window, cx);
        app.poll(cx);
        assert_eq!(app.source_sizing, Some(false));
        assert!(!app.source_sizing_consumed);
        assert_eq!(app.wizard.page, Page::Mode);
        for event in completed { replay.send(event).unwrap(); }
        app.poll(cx);
        app.worker.as_mut().unwrap().events = original;
        assert_eq!(app.status, "Cancelled");
        assert_eq!(app.wizard.page, Page::Mode);
        assert!(app.source_sizing_consumed);
        app.dispatch(Action::SourceSizing(true), window, cx);
        app.dispatch(Action::Advanced, window, cx);
        assert_eq!(app.wizard.page, Page::Palette);
        assert!(!app.busy);
        assert_eq!(app.status, "Cancelled");
        assert_eq!(dimensions(app), (1603, 1003));
    });
}

#[gpui::test]
fn basic_adopts_native_recommendations_once_without_changing_color_count(cx: &mut gpui::TestAppContext) {
    for (id, expected) in [
        ("fresh-conversion-20261020-00-logo-5x7", [1, 0, 1]),
        ("fresh-conversion-20261020-08-transparent-19x23", [1, 1, 0]),
        ("fresh-conversion-20261020-10-ring-41x37", [2, 0, 0]),
    ] {
        let directory = Directory::new(); let source = directory.native_source(id);
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source, cx); finish(app, cx);
            assert_eq!(app.source_recommendations, None);
            if id.ends_with("logo-5x7") {
                app.settings.category = 2; app.settings.quality = 2; app.settings.palette_mode = 2;
                app.color_count = 9;
            }
            let before = app.settings.clone(); let count = app.color_count;
            app.dispatch(Action::Basic, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::ImageType);
            assert_eq!(app.source_recommendations, Some(expected));
            assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], expected);
            assert_eq!(app.color_count, count);
            assert_eq!(app.settings.background_mode, before.background_mode);
            assert_eq!(app.document.as_ref().unwrap().settings.to_json(), before.to_json());

            app.settings.category = 0; app.settings.quality = 2; app.settings.palette_mode = 2;
            let edited = app.settings.to_json();
            app.dispatch(Action::Back, window, cx);
            app.dispatch(Action::Basic, window, cx);
            assert!(!app.busy, "acknowledged recommendations must not submit another analysis");
            assert_eq!(app.wizard.page, Page::ImageType);
            assert_eq!(app.settings.to_json(), edited);
            assert_eq!(app.source_recommendations, Some(expected));

            app.open(directory.0.join("missing.png"), cx); settle(app, cx);
            assert!(app.error); assert_eq!(app.source_recommendations, Some(expected));
            let replacement = directory.native_source("fresh-conversion-20261020-10-ring-41x37");
            app.open(replacement, cx); finish(app, cx);
            assert_eq!(app.source_recommendations, None);
            assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], [0, 2, 2]);
            assert_eq!(app.color_count, count);
            app.dispatch(Action::Basic, window, cx); finish(app, cx);
            assert_eq!(app.source_recommendations, Some([2, 0, 0]));
            assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], [2, 0, 0]);
            assert_eq!(app.color_count, count);
        });
    }
}

#[gpui::test]
fn cancelled_completion_does_not_admit_recommendations_and_retry_reanalyzes(cx: &mut gpui::TestAppContext) {
    for deliver_before_cancel in [false, true] {
        let directory = Directory::new();
        let source = directory.native_source("fresh-conversion-20261020-00-logo-5x7");
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source, cx); finish(app, cx);
            let selected = app.settings.to_json();
            app.dispatch(Action::Basic, window, cx);
            // Delay actual worker events, without fabricating results. Exercise
            // cancellation both before any dispatch and between presentation
            // delivery and the command's final completion.
            let mut completed = Vec::new();
            let completion = loop {
                let event = app.worker.as_ref().unwrap().events.recv_timeout(Duration::from_secs(120)).unwrap();
                if let Event::Finished(result) = &event {
                    assert_eq!(result, &Ok(Some([1, 0, 1])));
                    break event;
                }
                completed.push(event);
            };
            let (replay, receiver) = std::sync::mpsc::channel();
            let original = std::mem::replace(&mut app.worker.as_mut().unwrap().events, receiver);
            if !deliver_before_cancel { app.dispatch(Action::Cancel, window, cx); }
            for event in completed { replay.send(event).unwrap(); }
            app.poll(cx);
            assert!(app.busy);
            assert_eq!(app.wizard.page, Page::Mode);
            assert_eq!(app.source_recommendations, None);
            assert_eq!(app.settings.to_json(), selected);
            if deliver_before_cancel { app.dispatch(Action::Cancel, window, cx); }
            replay.send(completion).unwrap();
            app.poll(cx);
            app.worker.as_mut().unwrap().events = original;
            assert_eq!(app.wizard.page, Page::Mode);
            assert_eq!(app.status, "Cancelled");
            assert_eq!(app.source_recommendations, None);
            assert_eq!(app.settings.to_json(), selected);
            app.dispatch(Action::Basic, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::ImageType);
            assert_eq!(app.source_recommendations, Some([1, 0, 1]));
            assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], [1, 0, 1]);
        });
    }
}

#[gpui::test]
fn advanced_bypasses_analysis_but_later_basic_still_admits_it(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.native_source("fresh-conversion-20261020-08-transparent-19x23");
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        let committed = app.document.as_ref().unwrap().settings.to_json();
        app.dispatch(Action::Advanced, window, cx);
        assert!(!app.busy);
        assert_eq!(app.wizard.page, Page::Transparency);
        assert_eq!(app.source_recommendations, None);
        assert_eq!(app.settings.background_mode, 0);
        assert_eq!(app.document.as_ref().unwrap().settings.to_json(), committed);
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::ImageType);
        assert_eq!(app.source_recommendations, Some([1, 1, 0]));
        assert_eq!(app.settings.background_mode, 0, "analysis must preserve the previously selected background");
        app.enqueue(Command::Reset(app.task_input()), Some(Page::Load), cx); finish(app, cx);
        assert_eq!(app.source_recommendations, None);
    });
}

#[gpui::test]
fn first_transparency_entry_overrides_known_background_but_revisit_and_category_changes_do_not(cx: &mut gpui::TestAppContext) {
    for selected_mode in [1, 2] {
        let directory = Directory::new();
        let source = directory.native_source("fresh-conversion-20261020-08-transparent-19x23");
        let preferences = directory.0.join("gui.json");
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(preferences.clone(), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source, cx); finish(app, cx);
            app.dispatch(Action::Basic, window, cx); finish(app, cx);
            assert_eq!(app.settings.background_mode, -1);
            assert_eq!(app.transparency_initialized, [false, false]);
            app.change_setting("backgroundMode", json!(selected_mode), cx);
            let committed = app.document.as_ref().unwrap().settings.to_json();
            let raster = app.document.as_ref().unwrap().source.clone();
            app.dispatch(Action::Next, window, cx);
            assert_eq!(app.wizard.page, Page::Transparency);
            assert_eq!(app.settings.background_mode, 0);
            assert_eq!(app.settings.background, [1.0, 1.0, 1.0, 0.0]);
            assert_eq!(app.workflows.recommended_background_mode, 0);
            assert_eq!(app.transparency_initialized, [true, false]);
            assert_eq!(app.document.as_ref().unwrap().settings.to_json(), committed);
            assert!(Arc::ptr_eq(&raster, &app.document.as_ref().unwrap().source));
            let stored: Value = serde_json::from_slice(&std::fs::read(&preferences).unwrap()).unwrap();
            assert_eq!(stored["selected"]["settings"]["backgroundMode"], 0);
            app.dispatch(Action::Set("backgroundMode", json!(selected_mode)), window, cx);
            let chosen = (app.settings.background_mode, app.settings.background);
            if app.wizard.page != Page::Transparency { app.dispatch(Action::Back, window, cx); }
            assert_eq!(app.wizard.page, Page::Transparency);
            assert_eq!((app.settings.background_mode, app.settings.background), chosen);
            app.dispatch(Action::Back, window, cx);
            app.dispatch(Action::Set("category", json!(0)), window, cx);
            assert_eq!(app.wizard.page, Page::Transparency);
            assert_eq!((app.settings.background_mode, app.settings.background), chosen);
            assert_eq!(app.transparency_initialized, [true, false]);
        });
    }
}

#[gpui::test]
fn transparency_mode_entries_and_source_replacement_own_distinct_initialization(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.native_source("fresh-conversion-20261020-08-transparent-19x23");
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); finish(app, cx);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.transparency_initialized, [true, false]);
        app.dispatch(Action::Set("backgroundMode", json!(2)), window, cx);
        app.dispatch(Action::BackgroundChannel(0, -13), window, cx);
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Advanced, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Transparency);
        assert_eq!(app.transparency_initialized, [false, true]);
        assert_eq!(app.settings.background_mode, 0);
        assert_eq!(app.settings.background, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(app.workflows.recommended_background_mode, -1);
        app.dispatch(Action::Set("backgroundMode", json!(2)), window, cx);
        app.dispatch(Action::BackgroundChannel(0, -13), window, cx);
        let chosen = app.settings.background;
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Advanced, window, cx); finish(app, cx);
        assert_eq!(app.settings.background_mode, 2);
        assert_eq!(app.settings.background, chosen);
        assert_eq!(app.transparency_initialized, [false, true]);
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.transparency_initialized, [true, false]);
        assert_eq!(app.settings.background_mode, 0);
        assert_eq!(app.workflows.recommended_background_mode, 0);
        app.open(directory.0.join("missing.png"), cx); settle(app, cx);
        assert!(app.error);
        assert_eq!(app.transparency_initialized, [true, false]);
        app.open(source, cx); finish(app, cx);
        assert_eq!(app.transparency_initialized, [false, false]);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(app.transparency_initialized, [false, false]);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.transparency_initialized, [true, false]);
        app.enqueue(Command::Reset(app.task_input()), Some(Page::Load), cx); finish(app, cx);
        assert_eq!(app.transparency_initialized, [false, false]);
    });
}

#[gpui::test]
fn binary_alpha_artwork_defaults_to_white_but_opaque_sources_skip_transparency(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let binary = directory.0.join("binary-alpha-8x8.png");
    image::RgbaImage::from_fn(8, 8, |x, y| image::Rgba(
        if (2..6).contains(&x) && (2..6).contains(&y) { [0, 0, 255, 255] } else { [0; 4] }
    )).save(&binary).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(binary, cx); finish(app, cx);
        assert_eq!(app.document.as_ref().unwrap().source_alpha_type, 1);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        assert_eq!(app.settings.background_mode, -1);
        app.dispatch(Action::Set("category", json!(1)), window, cx);
        assert_eq!(app.wizard.page, Page::Transparency);
        assert_eq!(app.settings.background_mode, 1);
        assert_eq!(app.settings.background, [1.0; 4]);
        assert_eq!(app.workflows.recommended_background_mode, 1);
        assert_eq!(app.transparency_initialized, [true, false]);
        let opaque = directory.native_source("fresh-conversion-20261020-00-logo-5x7");
        app.open(opaque, cx); finish(app, cx);
        assert_eq!(app.document.as_ref().unwrap().source_alpha_type, 0);
        let background = (app.settings.background_mode, app.settings.background);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.wizard.page, Page::Quality);
        assert_eq!(app.transparency_initialized, [false, false]);
        assert_eq!((app.settings.background_mode, app.settings.background), background);
    });
}

#[gpui::test]
fn saved_profile_play_seeds_advanced_guard_without_discarding_basic_guard(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let mut profile = openvector::settings_profile::default_selected_profile();
    profile.settings.advanced = true; profile.settings.category = 1; profile.settings.palette_mode = 2;
    profile.settings.background_mode = 2; profile.settings.background = [32.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0, 1.0];
    profile.recommended_background_mode = 1;
    let mut profiles = openvector::saved_profiles::SavedAdvancedProfiles::default();
    profiles.put("Custom background", profile.clone());
    profiles.write_file(&directory.0.join("profiles.json")).unwrap();
    let source = directory.native_source("fresh-conversion-20261020-08-transparent-19x23");
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.transparency_initialized, [true, false]);
        app.dispatch(Action::Back, window, cx); app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Workflow(WorkflowAction::ApplyProfile("Custom background".into(), false)), window, cx);
        finish(app, cx);
        assert_eq!(app.wizard.page, Page::Transparency);
        assert_eq!(app.transparency_initialized, [true, true]);
        assert_eq!(app.settings.background_mode, 2);
        assert_eq!(app.settings.background, profile.settings.background);
        assert_eq!(app.workflows.recommended_background_mode, 1);
        app.dispatch(Action::Back, window, cx);
        app.dispatch(Action::Basic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        assert_eq!(app.transparency_initialized, [true, true]);
        assert_eq!(app.settings.background_mode, 2);
        assert_eq!(app.settings.background, profile.settings.background);
        assert_eq!(app.workflows.recommended_background_mode, 1);
    });
}

#[gpui::test]
fn automatic_classification_adopts_controls_and_transparency_lifetime_in_one_stage(cx: &mut gpui::TestAppContext) {
    for cancel_before_classification in [true, false] {
        let directory = Directory::new();
        let source = directory.native_source("fresh-conversion-20261020-08-transparent-19x23");
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source, cx); finish(app, cx);
            let initial = app.settings.to_json();
            app.dispatch(Action::Automatic, window, cx);
            let mut events = Vec::new();
            loop {
                let event = app.worker.as_ref().unwrap().events.recv_timeout(Duration::from_secs(120)).unwrap();
                let finished = matches!(&event, Event::Finished(Ok(_)));
                assert!(!matches!(&event, Event::Finished(Err(_))), "{event:?}");
                events.push(event);
                if finished { break; }
            }
            let classification = events.iter().position(|event| matches!(event, Event::Selection { classified: true, .. })).unwrap();
            let suffix = events.split_off(classification + 1);
            let (replay, receiver) = std::sync::mpsc::channel();
            let original = std::mem::replace(&mut app.worker.as_mut().unwrap().events, receiver);
            if cancel_before_classification { app.dispatch(Action::Cancel, window, cx); }
            for event in events { replay.send(event).unwrap(); }
            app.poll(cx);
            assert!(app.busy);
            assert_eq!(app.wizard.page, Page::Mode);
            if cancel_before_classification {
                assert_eq!(app.source_recommendations, None);
                assert_eq!(app.transparency_initialized, [false, false]);
                assert_eq!(app.settings.to_json(), initial);
            } else {
                assert_eq!(app.source_recommendations, Some([1, 1, 0]));
                assert_eq!(app.transparency_initialized, [true, false]);
                assert_eq!([app.settings.category, app.settings.quality, app.settings.palette_mode], [1, 1, 0]);
                assert_eq!(app.settings.background_mode, 0);
                assert_eq!(app.settings.background, [1.0, 1.0, 1.0, 0.0]);
                assert_eq!(app.workflows.recommended_background_mode, 0);
                app.dispatch(Action::Cancel, window, cx);
            }
            for event in suffix { replay.send(event).unwrap(); }
            app.poll(cx);
            app.worker.as_mut().unwrap().events = original;
            assert_eq!(app.status, "Cancelled");
            assert_eq!(app.wizard.page, Page::Mode);
            let manual = [0.125, 0.25, 0.5, 1.0];
            if !cancel_before_classification {
                app.settings.background_mode = 2; app.settings.background = manual;
            }
            app.dispatch(Action::Basic, window, cx);
            if cancel_before_classification { finish(app, cx); }
            else { assert!(!app.busy); }
            app.dispatch(Action::Next, window, cx);
            assert_eq!(app.wizard.page, Page::Transparency);
            assert_eq!(app.transparency_initialized, [true, false]);
            assert_eq!(app.settings.background_mode, if cancel_before_classification { 0 } else { 2 });
            assert_eq!(app.settings.background, if cancel_before_classification { [1.0, 1.0, 1.0, 0.0] } else { manual });
        });
    }
}
