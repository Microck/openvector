//! Drag result workflow with the real worker, filesystem and GPUI test window.
//!
//! Direct production methods stand in for the Drag button's closures: `begin_result_drag`
//! (on_drag) and `result_drag_payload` (external_drag_payload). GPUI's harness cannot
//! pointer-drive that button from this crate: no public API reports a production element's
//! bounds (`debug_bounds` needs a `debug_selector`), and a TestWindow's native start result
//! and offered paths are reachable only through crate-private accessors. Native
//! Started/Ended events, left releases and Escape go through the window's drawn root listeners.
//!
//! A painted Drag button's drag listener holds a clone of the ready file until the next
//! frame replaces it, so removal checks follow an update that redraws without that button.
use super::*;
use base64::Engine;
use gpui::{Entity, FileDragPaths, Modifiers, VisualTestContext, point};
use std::io::Read;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-drag-tests-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
/// Writes the first auto-update reference input, which Automatic vectorizes.
fn fixture_source(directory: &Directory) -> PathBuf {
    // Same lookup as openvector/tests/common: build boxes hold fixtures outside the tree.
    let base = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(base.join("auto-update-reference.ndjson.gz")).unwrap()).read_to_string(&mut text).unwrap();
    // Line 0 describes the capture.
    let reference: Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    let source = directory.0.join(reference["input"]["name"].as_str().unwrap());
    std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap()).unwrap();
    source
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
fn ready(app: &OpenVectorApp) -> Arc<DragFile> {
    match &app.drag_result.preparation { Preparation::Ready { file, .. } => file.clone(), _ => panic!("no ready drag file: {}", app.status) }
}
fn pending_request(app: &OpenVectorApp) -> u64 {
    match &app.drag_result.preparation { Preparation::Pending { request, .. } => *request, _ => panic!("no pending drag preparation: {}", app.status) }
}
/// Vectorizes with Automatic and enters Export, where the worker prepares an SVG drag file.
/// Minimize on drag is turned off because TestWindow::minimize is unimplemented!().
fn export_ready(app: &mut OpenVectorApp, source: PathBuf, window: &mut Window, cx: &mut Context<OpenVectorApp>) -> Arc<DragFile> {
    app.open(source, cx); finish(app, cx);
    app.dispatch(Action::Automatic, window, cx); finish(app, cx);
    app.dispatch(Action::DragFormat("svg"), window, cx);
    app.dispatch(Action::DragMinimize, window, cx);
    app.dispatch(Action::Next, window, cx); finish(app, cx);
    assert_eq!(app.wizard.page, Page::Export);
    ready(app)
}
/// Ends an internal drag as a held pointer does, over the root mouse-up listener. The move
/// restores mouse modality: after a keystroke GPUI reports no hover until the pointer moves.
fn release(visual: &mut VisualTestContext) {
    let position = point(px(1.0), px(1.0));
    visual.simulate_mouse_move(position, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
}
/// Admits `file` and resolves its payload, then delivers `events` to the external-drag listener.
fn hand_off(app: &Entity<OpenVectorApp>, visual: &mut VisualTestContext, file: &Arc<DragFile>, events: impl IntoIterator<Item = FileDropEvent>) {
    app.update_in(visual, |app, _, cx| {
        assert!(app.begin_result_drag(file, cx), "the drag is admitted");
        assert!(app.result_drag_payload(file).is_some(), "the payload resolves");
    });
    for event in events { visual.simulate_event(event); }
}
/// Retry preparation replaces the ready file; returns the replacement.
fn retry_preparation(app: &Entity<OpenVectorApp>, visual: &mut VisualTestContext) -> Arc<DragFile> {
    app.update_in(visual, |app, window, cx| { app.dispatch(Action::DragRetry, window, cx); finish(app, cx); ready(app) })
}
/// GPUI reports this when start_external_drag returns false.
fn rejected() -> FileDropEvent {
    FileDropEvent::Ended { outcome: ExternalDragOutcome::Rejected("start refused".into()) }
}

#[gpui::test]
fn ready_file_is_the_current_export_offered_only_to_an_admitted_drag(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = fixture_source(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        let prepared = export_ready(app, source.clone(), window, cx);
        assert_eq!(prepared.path().file_name(), source.with_extension("svg").file_name());
        app.dispatch(Action::Back, window, cx); assert_eq!(app.wizard.page, Page::Review);
        assert!(!app.begin_result_drag(&prepared, cx), "only Export offers the result");
        app.dispatch(Action::Next, window, cx); finish(app, cx);
        let file = ready(app);
        let reference = directory.0.join("reference.svg");
        app.enqueue(Command::Export { path: reference.clone(), options: app.export_options() }, None, cx);
        assert!(!app.begin_result_drag(&file, cx), "a running worker command blocks admission");
        finish(app, cx);
        assert!(std::fs::read(file.path()).unwrap() == std::fs::read(&reference).unwrap(), "the drag file must equal Save vector output");
        app.dispatch(Action::About, window, cx);
        assert!(!app.begin_result_drag(&file, cx), "an open dialog blocks admission");
        app.dispatch(Action::Dismiss, window, cx);
        assert_eq!(app.result_drag_payload(&file), None, "handoff requires an admitted drag");
        assert!(app.begin_result_drag(&file, cx));
        assert_eq!(app.result_drag_payload(&file), Some(ExternalDragPayload::Files(FileDragPaths::new([(file.path().to_owned(), false)]))));
    });
}

#[gpui::test]
fn changed_scene_options_or_source_reject_admission_and_handoff(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = fixture_source(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    // Scene: a refit publishes a new scene for the same source while a drag is admitted.
    let first = app.update_in(visual, |app, window, cx| {
        let file = export_ready(app, source.clone(), window, cx);
        assert!(app.begin_result_drag(&file, cx));
        let scene = app.document.as_ref().unwrap().vectors.clone().unwrap();
        app.enqueue(Command::Process { stage: "fitting", input: app.task_input() }, None, cx); finish(app, cx);
        assert!(!Arc::ptr_eq(&scene, app.document.as_ref().unwrap().vectors.as_ref().unwrap()), "the refit must publish a new scene");
        assert_eq!(app.result_drag_payload(&file), None, "a new scene rejects handoff");
        file
    });
    release(visual);
    // Export options: outside batch mode, dispatch re-prepares after an option change and is
    // blocked during a drag, so flip one in place.
    let second = app.update_in(visual, |app, window, cx| {
        assert!(!app.begin_result_drag(&first, cx), "a new scene rejects admission");
        app.dispatch(Action::DragRetry, window, cx); finish(app, cx);
        let file = ready(app);
        assert!(app.begin_result_drag(&file, cx));
        app.preferences.thin_stroke = !app.preferences.thin_stroke;
        assert_eq!(app.result_drag_payload(&file), None, "changed export options reject handoff");
        file
    });
    release(visual);
    app.update_in(visual, |app, _, cx| {
        assert!(!app.begin_result_drag(&second, cx), "changed export options reject admission");
        app.preferences.thin_stroke = !app.preferences.thin_stroke;
        // Source: reopening the image while a drag is admitted loads a new source.
        assert!(app.begin_result_drag(&second, cx));
        app.open(source, cx);
        assert_eq!(app.result_drag_payload(&second), None, "a running worker command rejects handoff");
        finish(app, cx);
        assert_eq!(app.result_drag_payload(&second), None, "a new source rejects handoff");
    });
}

#[gpui::test]
fn superseded_completion_cannot_replace_the_current_preparation(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = fixture_source(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let superseded = app.update_in(visual, |app, window, cx| {
        app.open(source, cx); finish(app, cx);
        app.dispatch(Action::Automatic, window, cx); finish(app, cx);
        app.dispatch(Action::Next, window, cx);
        let first_request = pending_request(app);
        finish(app, cx);
        let first = ready(app);
        app.dispatch(Action::ThinStroke, window, cx);
        let current = pending_request(app);
        // prepare_drag issues no request while the worker is busy, so the real worker cannot
        // deliver a superseded completion late; deliver one through the production method.
        app.drag_prepared(first_request, Ok(first.clone()));
        assert!(matches!(&app.drag_result.preparation, Preparation::Pending { request, .. } if *request == current),
            "a superseded completion replaced the current request");
        finish(app, cx);
        let second = ready(app);
        assert!(!app.begin_result_drag(&first, cx), "a superseded file cannot start a drag");
        assert!(app.begin_result_drag(&second, cx));
        first.path().to_owned()
    });
    assert!(!superseded.exists(), "nothing keeps a superseded file that was never offered");
}

#[gpui::test]
fn unoffered_attempts_allow_retry_and_leave_the_file_removable(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = fixture_source(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let file = app.update_in(visual, |app, window, cx| {
        let file = export_ready(app, source, window, cx);
        app.focus.focus(window, cx);
        assert!(app.begin_result_drag(&file, cx));
        file
    });
    release(visual);
    app.update_in(visual, |app, _, cx| assert!(app.begin_result_drag(&file, cx), "a released drag can be retried"));
    visual.simulate_keystrokes("escape");
    app.update_in(visual, |app, _, cx| {
        assert!(app.begin_result_drag(&file, cx), "an escaped drag can be retried");
        assert!(app.result_drag_payload(&file).is_some());
    });
    // The platform refuses to start a session for the resolved payload; the gesture then ends.
    visual.simulate_event(rejected());
    release(visual);
    let path = file.path().to_owned();
    retry_preparation(&app, visual);
    drop(file);
    assert!(!path.exists(), "no attempt offered the file, so replacing its preparation removes it");
}

#[gpui::test]
fn offered_files_outlive_their_session_and_a_later_rejected_attempt(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = fixture_source(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let mut file = app.update_in(visual, |app, window, cx| export_ready(app, source, window, cx));
    for outcome in [ExternalDragOutcome::Cancelled, ExternalDragOutcome::Failed("receiver failed".into()), ExternalDragOutcome::Dropped] {
        let (path, bytes) = (file.path().to_owned(), std::fs::read(file.path()).unwrap());
        hand_off(&app, visual, &file, [FileDropEvent::Started, FileDropEvent::Ended { outcome: outcome.clone() }]);
        // Retrying the same file, the platform refuses to start; the gesture then ends.
        hand_off(&app, visual, &file, [rejected()]);
        release(visual);
        file = retry_preparation(&app, visual);
        // Ended does not prove the receiver has finished reading.
        assert!(std::fs::read(&path).is_ok_and(|read| read == bytes), "{outcome:?}: the offered file must outlive a refused retry and its replacement");
    }
}
