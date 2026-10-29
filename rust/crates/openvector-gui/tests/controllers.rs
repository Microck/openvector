mod support;
use support::FixtureDirectory;
use gpui::{EntityInputHandler, Focusable};
use openvector_gui::{bitmap_size::BitmapSize, preferences::Preferences, quick_save::QuickSave, text_field::TextField};

#[test]
fn bitmap_size_preserves_aspect_ratio_separate_panes_and_correction_rollback() {
    let mut size = BitmapSize::new(1.0).unwrap();
    assert_eq!(size.panes, [[150, 250]; 2]);
    size.source_changed(17, 31).unwrap();
    size.commit(0, 0, 34.0).unwrap();
    assert_eq!(size.scale, 2.0); assert_eq!(size.panes, [[34, 62], [17, 31]]);
    size.commit(1, 2, 0.5).unwrap(); assert_eq!(size.panes, [[34, 62], [9, 16]]);
    size.commit(0, 1, 100000.0).unwrap(); assert_eq!(size.panes[0][1], 10000);
    let before = size.clone();
    assert!(size.source_changed(1, 100_000_000).is_err()); assert_eq!(size, before);
}

#[test]
fn quick_save_normalizes_names_without_merging_basic_advanced_choices() {
    let directory = FixtureDirectory::new(); let source = directory.0.join("multi.part.bmp");
    let mut save = QuickSave::new(false); save.source_changed(&source);
    assert_eq!(save.choices[0].filename, "multi");
    save.configure(0, "svg", directory.0.to_str().unwrap(), "new.output.eps").unwrap();
    assert_eq!(save.choices[0].format, "eps"); assert_eq!(save.choices[0].filename, "new");
    assert_eq!(save.choices[1].format, "ai"); assert_eq!(save.choices[1].filename, "multi");
    assert_eq!(save.target(0).unwrap(), directory.0.canonicalize().unwrap().join("new.eps"));
    save.configure(0, "svg", directory.0.to_str().unwrap(), "").unwrap(); assert!(save.target(0).is_err());
    assert!(save.configure(0, "svg", directory.0.to_str().unwrap(), "bad\0name").is_err());
}

#[test]
fn quick_save_and_bitmap_preferences_round_trip_independently() {
    let directory = FixtureDirectory::new(); let path = directory.0.join("gui.json");
    let mut preferences = Preferences::new(path.clone());
    preferences.quick_save.configure(0, "svg", directory.0.to_str().unwrap(), "first").unwrap();
    preferences.quick_save.configure(1, "pdf", directory.0.to_str().unwrap(), "second").unwrap();
    preferences.bitmap_save.configure(1, "tif", directory.0.to_str().unwrap(), "third").unwrap();
    preferences.bitmap_scale = 1.25; preferences.save().unwrap();
    let restored = Preferences::load(path).unwrap();
    assert_eq!(restored.quick_save.choices[0].format, "svg"); assert_eq!(restored.quick_save.choices[1].format, "pdf");
    assert_eq!(restored.bitmap_save.choices[0].format, "png"); assert_eq!(restored.bitmap_save.choices[1].format, "tif");
    assert_eq!(restored.bitmap_scale, 1.25); assert!(restored.quick_save.choices[0].filename.is_empty());
}

#[test]
fn recent_cache_is_bounded_coupled_to_history_and_lossless_across_storage() {
    let directory = FixtureDirectory::new(); let path = directory.0.join("gui.json");
    let mut preferences = Preferences::new(path.clone());
    let mut released = Vec::new();
    let paths = ["one.png", "two.png", "three.png", "four.png"].map(|name| directory.0.join(name));
    let bgra = [12, 34, 56, 137].repeat(720 * 360);
    let png = openvector::bitmap_codecs::encode_png(720, 360, &bgra, true).unwrap();
    for source in &paths {
        std::fs::write(source, &png).unwrap();
        preferences.remember(source, 720, 360, &bgra, &mut released).unwrap();
    }
    assert!(preferences.source_preview(&paths[0]).is_none(), "eviction drops the coupled thumbnail");
    assert_eq!(released.len(), 1);
    assert_eq!(preferences.source_preview(&paths[1]).unwrap().as_bytes(0).unwrap(), [12, 34, 56, 137].repeat(360 * 180));
    preferences.save().unwrap();
    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["sources"]["files"].as_array().unwrap().iter().map(|file| file["filename"].as_str().unwrap()).collect::<Vec<_>>(),
        paths[1..].iter().rev().map(|path| path.to_str().unwrap()).collect::<Vec<_>>());
    for source in &paths { std::fs::remove_file(source).unwrap(); }
    let mut restored = Preferences::load(path.clone()).unwrap();
    for source in &paths[1..] {
        assert_eq!(restored.source_preview(source).unwrap().as_bytes(0).unwrap(), preferences.source_preview(source).unwrap().as_bytes(0).unwrap());
    }
    let directory_history = restored.directories.snapshot();
    released.extend(restored.clear_sources());
    restored.save().unwrap();
    let cleared = Preferences::load(path).unwrap();
    assert!(cleared.sources.snapshot()["files"].as_array().unwrap().is_empty());
    assert_eq!(cleared.directories.snapshot(), directory_history, "clearing recent images keeps folder history");
}

#[test]
fn invalid_cached_thumbnails_are_rejected_without_reading_sources_or_changing_storage() {
    let directory = FixtureDirectory::new(); let path = directory.0.join("gui.json");
    let mut preferences = Preferences::new(path.clone());
    preferences.sources.add("/missing/source.png", 1, 1, 4).unwrap();
    preferences.save().unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let oversized = openvector::bitmap_codecs::encode_png(361, 1, &[1, 2, 3, 255].repeat(361), true).unwrap();
    for cached in [serde_json::json!([0, 1, 2]), serde_json::json!(oversized)] {
        let mut invalid = valid.clone();
        invalid["sources"]["files"][0]["previewPng"] = cached;
        let bytes = serde_json::to_vec(&invalid).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(Preferences::load(path.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let mut old_shape = valid;
    old_shape["sources"]["files"][0].as_object_mut().unwrap().remove("previewPng");
    let bytes = serde_json::to_vec(&old_shape).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(Preferences::load(path.clone()).is_err(), "old metadata-only stores require explicit recovery, not automatic migration");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn wrong_export_preference_types_are_rejected_without_overwriting_storage() {
    let directory = FixtureDirectory::new(); let path = directory.0.join("gui.json");
    Preferences::new(path.clone()).save().unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for key in ["format", "layout"] {
        let mut invalid = valid.clone();
        invalid[key] = serde_json::json!(42);
        let bytes = serde_json::to_vec(&invalid).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(Preferences::load(path.clone()).is_err(), "{key} must not silently become a default");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[gpui::test]
fn native_text_field_supports_utf16_ime_and_grapheme_keyboard_navigation(cx: &mut gpui::TestAppContext) {
    let (field, cx) = cx.add_window_view(|window, cx| {
        let field = TextField::new("Profile name", "A😀e\u{301}", cx); field.focus_handle(cx).focus(window, cx); field
    });
    field.update_in(cx, |field, window, cx| {
        field.set_selected_text_range(1..3, window, cx);
        field.replace_and_mark_text_in_range(None, "漢字", Some(2..2), window, cx);
        assert_eq!(field.marked_text_range(window, cx), Some(1..3));
        field.replace_text_in_range(None, "漢", window, cx);
        assert_eq!(field.text, "A漢e\u{301}");
        field.set_selected_text_range(4..4, window, cx);
    });
    cx.simulate_keystrokes("backspace");
    field.read_with(cx, |field, _| assert_eq!(field.text, "A漢"));
    cx.simulate_keystrokes("ctrl-a ctrl-c end ctrl-v");
    field.read_with(cx, |field, _| assert_eq!(field.text, "A漢A漢"));
}

#[gpui::test]
fn text_field_inserts_printable_keys_and_replaces_selected_text(cx: &mut gpui::TestAppContext) {
    let (field, cx) = cx.add_window_view(|window, cx| {
        let field = TextField::new("Opacity", "1", cx);
        field.focus_handle(cx).focus(window, cx);
        field
    });
    cx.simulate_keystrokes("ctrl-a 0 . 5");
    field.read_with(cx, |field, _| assert_eq!(field.text, "0.5"));
    cx.simulate_keystrokes("ctrl-a 2 5 5");
    field.read_with(cx, |field, _| assert_eq!(field.text, "255"));
}
