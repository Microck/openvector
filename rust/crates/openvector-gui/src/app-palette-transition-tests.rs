//! Basic color-mode transitions through the real worker and AI exporter.
use super::*;
use std::collections::BTreeSet;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-palette-transition-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }

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

fn export_fills(app: &mut OpenVectorApp, path: PathBuf, window: &mut Window, cx: &mut Context<OpenVectorApp>) -> BTreeSet<String> {
    app.dispatch(Action::Next, window, cx);
    assert_eq!(app.wizard.page, Page::Export);
    app.export_to(path.clone(), window, cx); finish(app, cx);
    let fills = std::fs::read_to_string(path).unwrap().lines()
        .filter_map(|line| line.strip_suffix(" Xa").map(str::to_owned)).collect();
    app.dispatch(Action::Back, window, cx);
    assert_eq!(app.wizard.page, Page::Review);
    fills
}

#[gpui::test]
fn two_colors_after_fixed_palette_replaces_selection_and_export(cx: &mut gpui::TestAppContext) {
    // Both observed paths matter: a clean transition and the same transition
    // after committed edits, where dismissing the warning must preserve them.
    for edited in [false, true] {
        let directory = Directory::new();
        let source = directory.0.join("artwork.png");
        let colors = [[255, 255, 255], [220, 28, 42], [22, 180, 80], [20, 75, 210]];
        image::RgbImage::from_fn(48, 48, |x, y| image::Rgb(colors[((y / 24) * 2 + x / 24) as usize]))
            .save(&source).unwrap();
        let store = directory.0.join("gui.json");
        let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(store.clone(), cx).unwrap());
        app.update_in(visual, |app, window, cx| {
            app.open(source, cx); finish(app, cx);
            for action in [Action::Basic, Action::Set("category", json!(2)), Action::Set("quality", json!(0)), Action::Set("paletteMode", json!(0))] {
                app.dispatch(action, window, cx); finish(app, cx);
            }
            assert_eq!(app.wizard.page, Page::Review);
            let two_colors = app.settings.palette.clone();
            assert_eq!(two_colors.len() / 4, 2);
            let two_color_fills = export_fills(app, directory.0.join("fresh-two.ai"), window, cx);
            assert_eq!(two_color_fills.len(), 2);

            app.dispatch(Action::Back, window, cx);
            assert_eq!(app.wizard.page, Page::Colors);
            app.dispatch(Action::Set("paletteMode", json!(1)), window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Palette);
            // Choosing a candidate now segments it for the preview; wait for that.
            app.dispatch(Action::Workflow(WorkflowAction::Candidate(4)), window, cx); finish(app, cx);
            let fixed_colors = app.settings.palette.clone();
            assert_eq!(fixed_colors.len() / 4, 4);
            app.dispatch(Action::Next, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Review);
            assert_eq!(app.settings.palette, fixed_colors, "fixed mode must keep the chosen candidate");
            let four_color_fills = export_fills(app, directory.0.join("fixed-four.ai"), window, cx);
            assert_eq!(four_color_fills.len(), 4);

            if edited {
                app.dispatch(Action::EditResult, window, cx); finish(app, cx);
                app.enqueue(Command::Edit { action: "apply", input: json!({"tool":"fill","x":4,"y":4,"color":[0,0,0,1],"commit":true}) }, None, cx);
                finish(app, cx);
                app.dispatch(Action::Next, window, cx); finish(app, cx);
                assert_eq!(app.wizard.page, Page::Review);
                assert!(app.document.as_ref().unwrap().has_committed_edits);
            }
            app.dispatch(Action::Back, window, cx);
            assert_eq!(app.wizard.page, Page::Palette);
            app.dispatch(Action::Back, window, cx); finish(app, cx);
            assert_eq!(app.wizard.page, Page::Colors);
            app.dispatch(Action::Set("paletteMode", json!(0)), window, cx);
            if edited {
                assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::RunProcess("fitting", Page::Review, _))))));
                let committed = app.document.as_ref().unwrap().stage_keys.segmentation_edits.clone();
                app.dispatch(Action::Dismiss, window, cx); finish(app, cx);
                assert_eq!(app.wizard.page, Page::Colors);
                assert_eq!(app.document.as_ref().unwrap().stage_keys.segmentation_edits, committed);
                assert!(app.document.as_ref().unwrap().has_committed_edits);
                assert_eq!(app.settings.palette, fixed_colors);
                app.dispatch(Action::Set("paletteMode", json!(0)), window, cx);
                let Some(Dialog::Form(Form::Confirm(_, action))) = &app.dialog else { panic!("missing discard confirmation"); };
                let action = action.clone();
                app.dispatch(Action::Workflow(action), window, cx);
            } else {
                assert!(app.dialog.is_none());
            }
            finish(app, cx);
            assert_eq!(app.wizard.page, Page::Review);
            assert_eq!(app.settings.palette_mode, 0);
            assert_eq!(app.settings.palette, two_colors, "Review must show the recomputed two-color selection");
            assert_eq!(app.color_count, 2);
            assert_eq!(app.workflows.palette_count, 2.0);
            assert_eq!(app.workflows.palette.backup, two_colors);
            assert!(!app.workflows.palette.customized);
            let document = app.document.as_ref().unwrap();
            assert_eq!(document.settings.palette, two_colors);
            assert_eq!(document.committed_count, 2);
            assert!(!document.has_committed_edits);
            assert_eq!(export_fills(app, directory.0.join("two-after-fixed.ai"), window, cx), two_color_fills);
            let restored = Preferences::load(store.clone()).unwrap();
            assert_eq!(restored.selected.settings.palette_mode, 0);
            assert_eq!(restored.selected.color_count, 2);
            assert_eq!(restored.selected.palette_color_count, 2);
        });
    }
}
