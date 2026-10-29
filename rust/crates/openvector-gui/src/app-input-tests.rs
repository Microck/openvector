use super::*;
use gpui::{Keystroke, Modifiers, ScrollDelta, ScrollEvent, VisualTestContext, point};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-input-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }

fn key_down(visual: &mut VisualTestContext, keystroke: &str) {
    visual.simulate_event(KeyDownEvent { keystroke: Keystroke::parse(keystroke).unwrap(), is_held: false, prefer_character_input: false });
}
fn loaded_viewer(cx: &mut gpui::TestAppContext) -> (Directory, gpui::Entity<OpenVectorApp>, &mut VisualTestContext) {
    let directory = Directory::new();
    let source = directory.0.join("source.png");
    image::RgbImage::from_fn(256, 256, |x, y| image::Rgb([x as u8, y as u8, 200])).save(&source).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx);
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.busy || !app.pending.is_empty() || app.document.is_none() {
            app.poll(cx);
            assert!(Instant::now() < deadline, "source did not load: {}", app.status);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.error, "{}", app.status);
        app.focus.focus(window, cx);
    });
    // Any dispatched event draws first; the viewport canvas measures and fits on that frame.
    visual.simulate_mouse_move(point(px(0.0), px(0.0)), None, Modifiers::default());
    (directory, app, visual)
}

// Native: scrollbar singleStep = trunc(available/20); one wheel notch is Qt angle 120,
// slider + trunc(120/12). GPUI Linux reports a notch as 3 lines.
#[gpui::test]
fn focused_view_arrows_and_wheel_notches_use_native_units(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    let view = |visual: &mut VisualTestContext| app.read_with(visual, |app, _| app.view.clone());
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(100), window, cx));
    let zoomed = view(visual);
    assert!(zoomed.area.height > 0.0 && zoomed.bar(Axis::Vertical).maximum > zoomed.bar(Axis::Vertical).value);

    key_down(visual, "down");
    let stepped = view(visual);
    assert_eq!(stepped.bar(Axis::Vertical).value, zoomed.bar(Axis::Vertical).value + (zoomed.area.height / 20.0).trunc());
    assert_eq!(stepped.bar(Axis::Horizontal).value, zoomed.bar(Axis::Horizontal).value);
    key_down(visual, "up");
    assert_eq!(view(visual).bar(Axis::Vertical).value, zoomed.bar(Axis::Vertical).value);
    key_down(visual, "alt-down");
    assert_eq!(view(visual).bar(Axis::Vertical).value, zoomed.bar(Axis::Vertical).value);

    let viewport = app.read_with(visual, |app, _| app.viewport);
    let scroll = |delta| ScrollEvent { position: viewport.center(), delta, ..ScrollEvent::default() };
    visual.simulate_event(scroll(ScrollDelta::Pixels(point(px(0.0), px(120.0)))));
    assert_eq!(view(visual).slider, zoomed.slider + 10);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(-10), window, cx));
    visual.simulate_event(scroll(ScrollDelta::Lines(point(0.0, 3.0))));
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    assert_eq!(view(visual).slider, zoomed.slider + 10);
}

#[gpui::test]
fn scrollbar_corner_cannot_start_image_pan_or_zoom(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(100), window, cx));
    visual.simulate_mouse_move(point(px(0.0), px(0.0)), None, Modifiers::default());
    let (before, corner, inside) = app.read_with(visual, |app, _| (
        app.view.clone(),
        app.viewport.origin + point(px(app.view.area.width as f32 + 8.0), px(app.view.area.height as f32 + 8.0)),
        app.viewport.center(),
    ));
    visual.simulate_mouse_down(corner, MouseButton::Left, Modifiers::default());
    app.read_with(visual, |app, _| {
        assert!(app.dragging.is_none(), "the reserved corner must not seed an image pan");
        assert_eq!(app.pointer_buttons, 0);
    });
    visual.simulate_mouse_move(inside, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
    visual.simulate_event(ScrollEvent { position: corner, delta: ScrollDelta::Pixels(point(px(0.0), px(120.0))), ..Default::default() });
    app.read_with(visual, |app, _| assert_eq!(app.view, before, "corner input must not move or zoom the image"));

    visual.simulate_mouse_down(inside, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(inside + point(px(-40.0), px(-30.0)), MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(inside, MouseButton::Left, Modifiers::default());
    app.read_with(visual, |app, _| assert_ne!(app.view, before, "image-content dragging must still pan"));
}

fn scrollbar_point(app: &OpenVectorApp, axis: Axis, along: f64) -> gpui::Point<gpui::Pixels> {
    match axis {
        Axis::Horizontal => app.viewport.origin + point(px(along as f32), px(app.view.area.height as f32 + 8.0)),
        Axis::Vertical => app.viewport.origin + point(px(app.view.area.width as f32 + 8.0), px(along as f32)),
    }
}

fn thumb_grab(app: &OpenVectorApp, axis: Axis) -> gpui::Point<gpui::Pixels> {
    let (extent, thumb, offset) = app.scrollbar_geometry(axis);
    assert!(extent > thumb && thumb >= 17.0);
    scrollbar_point(app, axis, offset + thumb / 4.0)
}

#[gpui::test]
fn scrollbar_thumb_drag_preserves_grab_and_clamps_both_axes(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(100), window, cx));
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let (before, grab, travel) = app.read_with(visual, |app, _| {
            let (extent, thumb, _) = app.scrollbar_geometry(axis);
            (app.view.clone(), thumb_grab(app, axis), extent - thumb)
        });
        visual.simulate_mouse_down(grab, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(grab, MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| assert_eq!(app.view, before, "thumb press and stationary move must not jump"));

        // Moving across the image as well as along the track must remain owned
        // by the scrollbar, not seed a pan or editing gesture.
        let mut moved = grab;
        match axis {
            Axis::Horizontal => { moved.x += px(37.25); moved.y -= px(60.0); },
            Axis::Vertical => { moved.y += px(37.25); moved.x -= px(60.0); },
        }
        visual.simulate_mouse_move(moved, MouseButton::Left, Modifiers::default());
        let bar = before.bar(axis);
        let expected = before.scroll(axis, bar.value + 37.25 * ((bar.maximum - bar.minimum) / travel)).unwrap();
        app.read_with(visual, |app, _| {
            assert_eq!(app.view, expected);
            assert_eq!(app.pointer_buttons, 0, "scrollbar must not enter the image pointer owner");
            assert!(app.dragging.is_none());
        });

        for (delta, value) in [(100_000.0, bar.maximum), (-100_000.0, bar.minimum), (0.0, bar.value)] {
            let mut moved = grab;
            match axis {
                Axis::Horizontal => moved.x += px(delta),
                Axis::Vertical => moved.y += px(delta),
            }
            visual.simulate_mouse_move(moved, MouseButton::Left, Modifiers::default());
            app.read_with(visual, |app, _| assert_eq!(app.view.bar(axis).value, value));
        }
        // Release outside the window, then move back over the old thumb.
        visual.simulate_mouse_up(point(px(-100.0), px(-100.0)), MouseButton::Left, Modifiers::default());
        let released = app.read_with(visual, |app, _| app.view.clone());
        visual.simulate_mouse_move(moved, None, Modifiers::default());
        app.read_with(visual, |app, _| {
            assert_eq!(app.view, released);
            assert!(app.scrollbar_interaction.is_none());
        });
    }
}

#[gpui::test]
fn scrollbar_track_pages_and_repeats_until_reaching_pointer(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(200), window, cx));
    for axis in [Axis::Horizontal, Axis::Vertical] {
        // Start at the track edge so the held click has room for multiple pages,
        // independent of the test window's aspect ratio and initial centering.
        app.update(visual, |app, cx| {
            app.view = app.view.scroll(axis, app.view.bar(axis).minimum).unwrap();
            cx.notify();
        });
        let (before, click) = app.read_with(visual, |app, _| {
            let (extent, _, _) = app.scrollbar_geometry(axis);
            let along = extent * 0.8;
            (app.view.clone(), scrollbar_point(app, axis, along))
        });
        let bar = before.bar(axis);
        let first_page = before.scroll(axis, bar.value + bar.page_step).unwrap();
        visual.simulate_mouse_down(click, MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| assert_eq!(app.view, first_page));
        visual.executor().advance_clock(SCROLLBAR_REPEAT_DELAY - Duration::from_millis(1));
        visual.run_until_parked();
        app.read_with(visual, |app, _| assert_eq!(app.view, first_page, "holding must preserve the initial repeat delay"));
        visual.executor().advance_clock(Duration::from_millis(1));
        visual.run_until_parked();
        let second_page = first_page.scroll(axis, first_page.bar(axis).value + bar.page_step).unwrap();
        app.read_with(visual, |app, _| assert_eq!(app.view, second_page));

        for _ in 0..10 {
            visual.executor().advance_clock(SCROLLBAR_REPEAT_INTERVAL);
            visual.run_until_parked();
        }
        let stopped = app.read_with(visual, |app, _| {
            let (_, thumb, offset) = app.scrollbar_geometry(axis);
            let pointer = app.scrollbar_position(axis, click);
            assert!(pointer >= offset && pointer <= offset + thumb, "paging must stop at the reached thumb");
            assert!(matches!(app.scrollbar_interaction, Some(ScrollbarInteraction::Page { .. })));
            app.view.clone()
        });
        visual.executor().advance_clock(Duration::from_secs(1));
        visual.run_until_parked();
        app.read_with(visual, |app, _| assert_eq!(app.view, stopped));
        visual.simulate_mouse_up(point(px(-100.0), px(-100.0)), MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| assert!(app.scrollbar_interaction.is_none()));
    }
}

#[gpui::test]
fn scrollbar_track_repeat_cancels_on_release_missing_button_and_modal(cx: &mut gpui::TestAppContext) {
    for cancellation in ["release", "missing-button", "modal"] {
        let (_directory, app, visual) = loaded_viewer(cx);
        app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(200), window, cx));
        let click = app.read_with(visual, |app, _| scrollbar_point(app, Axis::Vertical, 1.0));
        visual.simulate_mouse_down(click, MouseButton::Left, Modifiers::default());
        match cancellation {
            "release" => visual.simulate_mouse_up(point(px(-100.0), px(-100.0)), MouseButton::Left, Modifiers::default()),
            "missing-button" => visual.simulate_mouse_move(click, None, Modifiers::default()),
            "modal" => {
                app.update_in(visual, |app, window, cx| app.dispatch(Action::About, window, cx));
                visual.simulate_mouse_move(click, MouseButton::Left, Modifiers::default());
            }
            _ => unreachable!(),
        }
        let stopped = app.read_with(visual, |app, _| {
            assert!(app.scrollbar_interaction.is_none(), "{cancellation}");
            app.view.clone()
        });
        visual.executor().advance_clock(SCROLLBAR_REPEAT_DELAY + SCROLLBAR_REPEAT_INTERVAL);
        visual.run_until_parked();
        app.read_with(visual, |app, _| assert_eq!(app.view, stopped, "{cancellation} must cancel queued paging"));
    }
}

#[gpui::test]
fn scrollbar_track_repeat_survives_keyboard_deactivation_until_release(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(200), window, cx));
    let click = app.read_with(visual, |app, _| scrollbar_point(app, Axis::Vertical, 1.0));
    visual.simulate_mouse_down(click, MouseButton::Left, Modifiers::default());
    let first_page = app.read_with(visual, |app, _| app.view.clone());
    visual.deactivate_window();
    visual.executor().advance_clock(SCROLLBAR_REPEAT_DELAY);
    visual.run_until_parked();
    let bar = first_page.bar(Axis::Vertical);
    let second_page = first_page.scroll(Axis::Vertical, bar.value - bar.page_step).unwrap();
    app.read_with(visual, |app, _| assert_eq!(app.view, second_page));
    visual.simulate_mouse_up(point(px(-100.0), px(-100.0)), MouseButton::Left, Modifiers::default());
    visual.executor().advance_clock(SCROLLBAR_REPEAT_INTERVAL);
    visual.run_until_parked();
    app.read_with(visual, |app, _| {
        assert_eq!(app.view, second_page);
        assert!(app.scrollbar_interaction.is_none());
    });
}

#[gpui::test]
fn scrollbar_drag_stops_on_missing_button_and_view_transitions(cx: &mut gpui::TestAppContext) {
    let (directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(100), window, cx));
    for button in [None, Some(MouseButton::Right), Some(MouseButton::Middle)] {
        let grab = app.read_with(visual, |app, _| thumb_grab(app, Axis::Horizontal));
        visual.simulate_mouse_down(grab, MouseButton::Left, Modifiers::default());
        let stopped = app.read_with(visual, |app, _| app.view.clone());
        visual.simulate_mouse_move(grab + point(px(50.0), px(-60.0)), button, Modifiers::default());
        visual.simulate_mouse_move(grab + point(px(70.0), px(-60.0)), MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| {
            assert_eq!(app.view, stopped);
            assert!(app.scrollbar_interaction.is_none());
        });
        visual.simulate_mouse_up(grab, MouseButton::Left, Modifiers::default());
    }

    for action in [Action::About, Action::Batch, Action::Start, Action::Fit] {
        let grab = app.read_with(visual, |app, _| thumb_grab(app, Axis::Vertical));
        visual.simulate_mouse_down(grab, MouseButton::Left, Modifiers::default());
        app.update_in(visual, |app, window, cx| app.dispatch(action.clone(), window, cx));
        // Draw the modal/batch transition before dismissing it.
        visual.simulate_mouse_move(point(px(0.0), px(0.0)), MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| assert!(app.scrollbar_interaction.is_none()));
        app.update_in(visual, |app, window, cx| match action {
            Action::About => app.dispatch(Action::Dismiss, window, cx),
            Action::Batch => app.dispatch(Action::Batch, window, cx),
            _ => {},
        });
        let stopped = app.read_with(visual, |app, _| app.view.clone());
        visual.simulate_mouse_move(grab + point(px(-60.0), px(70.0)), MouseButton::Left, Modifiers::default());
        app.read_with(visual, |app, _| assert_eq!(app.view, stopped));
        visual.simulate_mouse_up(grab, MouseButton::Left, Modifiers::default());
    }

    // Source admission must clear the drag before the worker replaces geometry.
    let grab = app.read_with(visual, |app, _| thumb_grab(app, Axis::Horizontal));
    visual.simulate_mouse_down(grab, MouseButton::Left, Modifiers::default());
    app.update_in(visual, |app, _, cx| {
        app.open(directory.0.join("source.png"), cx);
        assert!(app.scrollbar_interaction.is_none());
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.busy || !app.pending.is_empty() {
            app.poll(cx);
            assert!(Instant::now() < deadline, "source did not reload: {}", app.status);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.error, "{}", app.status);
    });
    visual.simulate_mouse_up(grab, MouseButton::Left, Modifiers::default());
    let grab = app.read_with(visual, |app, _| thumb_grab(app, Axis::Horizontal));
    visual.simulate_mouse_down(grab, MouseButton::Left, Modifiers::default());
    app.update_in(visual, |app, window, cx| {
        app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
        app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx);
        assert!(app.scrollbar_interaction.is_none());
        assert!(app.document.is_none());
    });
    visual.simulate_mouse_move(grab + point(px(70.0), px(0.0)), MouseButton::Left, Modifiers::default());
    app.read_with(visual, |app, _| assert_eq!(app.view.bar(Axis::Horizontal).value, 0.0));
}

#[gpui::test]
fn pan_buttons_match_focused_arrow_keys(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| app.dispatch(Action::Zoom(100), window, cx));
    visual.simulate_mouse_move(point(px(0.0), px(0.0)), None, Modifiers::default());
    let before = app.read_with(visual, |app, _| app.view.clone());
    let up = visual.debug_bounds("pan-up").unwrap().center();
    visual.simulate_click(up, Modifiers::default());
    let button_up = app.read_with(visual, |app, _| app.view.clone());
    app.update_in(visual, |app, window, cx| { app.view = before.clone(); app.focus.focus(window, cx); });
    key_down(visual, "up");
    app.read_with(visual, |app, _| assert_eq!(app.view, button_up));
    let down = visual.debug_bounds("pan-down").unwrap().center();
    visual.simulate_click(down, Modifiers::default());
    let button_down = app.read_with(visual, |app, _| app.view.clone());
    app.update_in(visual, |app, window, cx| { app.view = button_up.clone(); app.focus.focus(window, cx); });
    key_down(visual, "down");
    app.read_with(visual, |app, _| {
        assert_eq!(app.view, button_down);
        assert_eq!(app.view.bar(Axis::Vertical).value, before.bar(Axis::Vertical).value);
    });
}

#[gpui::test]
fn replacing_manual_work_requires_confirmation_and_cancel_preserves_it(cx: &mut gpui::TestAppContext) {
    let (directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| {
        let source = app.document.as_ref().unwrap().source.clone();
        // Leaving the editor preserves its unapplied edits even on the Load page.
        app.document.as_mut().unwrap().editing_changed = true;
        app.wizard.page = Page::Load;
        app.open(directory.0.join("missing.png"), cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::OpenAccepted(_))))));
        app.dispatch(Action::Dismiss, window, cx);
        app.document.as_mut().unwrap().editing_changed = false;
        app.workflows.palette.customized = true;
        app.open(directory.0.join("missing.png"), cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::OpenAccepted(_))))));
        assert!(!app.busy);
        app.dispatch(Action::Dismiss, window, cx);
        assert!(Arc::ptr_eq(&source, &app.document.as_ref().unwrap().source));
        assert!(app.workflows.palette.customized);
        app.dispatch(Action::Batch, window, cx);
        app.dispatch(Action::BatchStart, window, cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::StartBatchAccepted)))));
        app.dispatch(Action::Dismiss, window, cx);
        assert!(!app.busy);
        assert!(Arc::ptr_eq(&source, &app.document.as_ref().unwrap().source));
    });
}

#[gpui::test]
fn adding_batch_inputs_preserves_the_explicit_export_folder(cx: &mut gpui::TestAppContext) {
    let (directory, app, visual) = loaded_viewer(cx);
    app.update_in(visual, |app, window, cx| {
        app.dispatch(Action::Batch, window, cx);
        let output = directory.0.join("chosen-output").to_string_lossy().into_owned();
        app.batch_queue.export_directory = output.clone();
        app.add_batch_files(&[directory.0.join("source.png").to_string_lossy().into_owned()], cx);
        assert_eq!(app.batch.as_ref().unwrap().export_directory, output);
        assert_eq!(app.batch.as_ref().unwrap().queue.len(), 1);
    });
}

#[gpui::test]
fn bitmap_save_uses_the_displayed_modes_scale(cx: &mut gpui::TestAppContext) {
    let (directory, app, visual) = loaded_viewer(cx);
    let output = directory.0.join("bitmap.png");
    std::fs::write(&output, b"existing output").unwrap();
    app.update_in(visual, |app, window, cx| {
        app.workflows.bitmap.commit(0, 2, 0.5).unwrap();
        app.workflows.bitmap.commit(1, 2, 2.0).unwrap();
        for (mode, scale) in [(0, 0.5), (1, 2.0)] {
            app.settings.advanced = mode == 1;
            app.preferences.bitmap_save.configure(mode, "png", directory.0.to_str().unwrap(), "bitmap").unwrap();
            app.dispatch(Action::Workflow(WorkflowAction::SaveDialog(true)), window, cx);
            app.dispatch(Action::Workflow(WorkflowAction::Save(true)), window, cx);
            match &app.dialog {
                Some(Dialog::Form(Form::Confirm(_, WorkflowAction::Confirm(Command::BitmapExport { scale: selected, .. })))) => assert_eq!(*selected, scale),
                _ => panic!("Expected overwrite confirmation: {}", app.status),
            }
            app.dispatch(Action::Dismiss, window, cx);
        }
    });
    assert_eq!(std::fs::read(output).unwrap(), b"existing output");
}

#[gpui::test]
fn comparison_keys_release_when_the_window_deactivates(cx: &mut gpui::TestAppContext) {
    let (_directory, app, visual) = loaded_viewer(cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    for key in ["b", "v"] {
        key_down(visual, key);
        app.read_with(visual, |app, _| {
            let held = app.input.snapshot();
            assert!(held.held_bitmap || held.held_vector);
        });
        visual.deactivate_window();
        visual.run_until_parked();
        app.read_with(visual, |app, _| {
            let released = app.input.snapshot();
            assert!(!released.held_bitmap && !released.held_vector);
            assert_eq!(released.current_layer, released.base_layer);
        });
        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
    }
}
