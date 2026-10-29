//! Recent Basic Settings through the real worker: recording after completed
//! single-image fitting, no record for cached or batch work, and replay
//! navigation on success, failure and Cancel. Expectations beyond the retained
//! persistence capture are static predictions (docs/recent-basic-settings-contract.json).
use super::*;
use base64::Engine;
use openvector::settings_profile::stored_settings_profile;
use std::io::Read;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-recent-basic-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
/// The processing-random-reuse capture: automatic(alpha), model reset,
/// automatic(bravo), persist.
fn reference() -> Value {
    // Same lookup as openvector/tests/common: build boxes hold fixtures outside the tree.
    let base = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(base.join("processing-reset-reference.ndjson.gz")).unwrap()).read_to_string(&mut text).unwrap();
    text.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|row| row["id"] == "processing-random-reuse").unwrap()
}
/// Writes a retained input under its native directory name, keeping the native
/// image.bmp name because classification reads the extension.
fn native_source(reference: &Value, native: &str, directory: &Directory) -> PathBuf {
    let source = directory.0.join(native.rsplit('\\').nth(1).unwrap()).join("image.bmp");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, base64::engine::general_purpose::STANDARD.decode(reference["inputs"][native].as_str().unwrap()).unwrap()).unwrap();
    source
}
/// Fully automatic classifies this input as "Artwork, High, Two colors".
fn bravo(directory: &Directory) -> PathBuf {
    let reference = reference();
    let native = reference["inputs"].as_object().unwrap().keys().find(|name| name.ends_with("\\bravo\\image.bmp")).unwrap().clone();
    native_source(&reference, &native, directory)
}
/// Native VS::load of basic::<index>: the stored record with opaque flattening
/// color and no palette colors. Registry integers are unsigned 32-bit words.
fn native_basic_entry(values: &Value, index: usize) -> SettingsProfile {
    let value = |name: &str| &values[format!("/VmQt::VectorizationSettings::basic::{index}/{name}")]["value"];
    let int = |name: &str| value(name).as_u64().unwrap() as u32 as i32;
    let flag = |name: &str| value(name) == "true";
    let color: u32 = value("flatten_image_color").as_str().unwrap().parse().unwrap();
    let channel = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0;
    SettingsProfile {
        settings: DocumentSettings {
            category: int("image_category"), quality: int("image_quality"), sensitivity: int("adv_color_sensitivity"), anti_alias: flag("adv_use_palette_aa"),
            complexity: int("adv_segmentation_complexity"), minimum_pixels: int("adv_min_num_pixels"), artifact_rejection: int("adv_anti_aliasing_rejection"),
            cluster_colors: flag("adv_cluster_colors"), smoothness: int("adv_contour_smoothness"), detect_corners: int("adv_detect_sharp_corners") != 0,
            curve_complexity: int("adv_curve_complexity"), smoothing_anti_alias: flag("adv_use_contour_aa"), advanced: flag("use_advanced_mode"),
            palette_mode: int("palette_mode"), background_mode: int("flatten_image_mode"),
            background: [channel(0), channel(8), channel(16), 1.0], palette: Vec::new(),
        },
        color_count: int("adv_num_colors"), basic_color_count: int("batch_num_colors"), palette_color_count: int("num_palette_colors"),
        recommended_background_mode: int("recommended_flatten_image_mode"),
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
fn finish(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    settle(app, cx);
    assert!(!app.error, "{}", app.status);
}
fn labels(app: &OpenVectorApp) -> Vec<String> {
    app.preferences.recent_basic.entries().iter().map(|entry| entry.label.clone()).collect()
}

/// Retained cross-check: each automatic run records the selected settings,
/// Reset application clears them, and the store finally holds exactly the
/// native basic::0 record with its derived label.
#[gpui::test]
fn automatic_runs_record_native_basic_settings_and_reset_clears_them(cx: &mut gpui::TestAppContext) {
    let reference = reference();
    let directory = Directory::new(); let path = directory.0.join("gui.json");
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(path.clone(), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        for action in reference["phases"][0].as_array().unwrap() {
            match action[0].as_str().unwrap() {
                "automatic" => {
                    app.open(native_source(&reference, action[1].as_str().unwrap(), &directory), cx); finish(app, cx);
                    app.dispatch(Action::Automatic, window, cx); finish(app, cx);
                    assert_eq!(labels(app).len(), 1);
                }
                "reset-model" => {
                    app.dispatch(Action::Workflow(WorkflowAction::ResetApplication), window, cx);
                    app.dispatch(Action::Workflow(WorkflowAction::ResetApplicationAccepted), window, cx); finish(app, cx);
                    assert!(labels(app).is_empty());
                }
                "document" | "snapshot" | "persist" => {},
                action => panic!("unhandled native action {action}"),
            }
        }
    });
    let values = &reference["privateValues"][0];
    assert_eq!(values["/VmQt::MainModel/num_mcu_settings"]["value"], 1);
    let restored = Preferences::load(path).unwrap();
    assert_eq!(restored.recent_basic.entries().iter().map(|entry| entry.label.as_str()).collect::<Vec<_>>(), ["Artwork, High, Two colors"]);
    assert_eq!(restored.recent_basic.stored_profiles(), json!([stored_settings_profile(&native_basic_entry(values, 0))]));
}

/// A fitting that finds every stage cached, or any batch fitting, records
/// nothing; a changed basic choice that fits again records its label.
#[gpui::test]
fn cached_and_batch_fitting_do_not_record(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = bravo(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); finish(app, cx);
        app.dispatch(Action::Automatic, window, cx); finish(app, cx);
        assert_eq!(labels(app), ["Artwork, High, Two colors"]);
        // The native clear slot has no control; clearing here makes a cached record visible.
        app.preferences.recent_basic.clear();
        app.process("fitting", Page::Review, cx); finish(app, cx);
        assert!(labels(app).is_empty(), "a cached fitting records nothing");
        app.dispatch(Action::Set("quality", json!(1)), window, cx); finish(app, cx);
        assert_eq!(labels(app), ["Artwork, Medium, Two colors"]);
        app.dispatch(Action::Batch, window, cx);
        app.batch_queue.add_files(&[source.to_string_lossy().into_owned()]); app.batch = Some(app.batch_queue.snapshot());
        app.dispatch(Action::BatchStart, window, cx); finish(app, cx);
        assert_eq!(app.workflows.batch_result.as_ref().unwrap().converted, 1);
        assert_eq!(labels(app), ["Artwork, Medium, Two colors"], "batch fitting records nothing");
    });
}

/// Replay selects the whole stored entry with advanced off, shows Review (or
/// Export with Skip review) and moves the label to the front. A failed replay
/// stays on Mode with the replayed settings selected and the list unchanged; a
/// palette mode the runner does not process selects its controls and stays on
/// Mode without an error. Deterministic cancellation is proved on the worker
/// loop (engine_worker cancellation_tests): Worker::submit clears the flag, so
/// a GUI Cancel always races the running replay.
#[gpui::test]
fn replay_navigates_on_success_and_stays_on_mode_without_processing(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new(); let source = bravo(&directory);
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        let to_mode = |app: &mut OpenVectorApp, window: &mut Window, cx: &mut Context<OpenVectorApp>| {
            app.dispatch(Action::Start, window, cx); app.dispatch(Action::Next, window, cx);
            assert_eq!(app.wizard.page, Page::Mode);
        };
        let replay = |app: &mut OpenVectorApp, index, window: &mut Window, cx: &mut Context<OpenVectorApp>|
            app.dispatch(Action::Workflow(WorkflowAction::ReplayRecentBasic(index)), window, cx);
        app.open(source, cx); finish(app, cx);
        app.dispatch(Action::Automatic, window, cx); finish(app, cx);
        app.dispatch(Action::Set("quality", json!(1)), window, cx); finish(app, cx);
        assert_eq!(labels(app), ["Artwork, Medium, Two colors", "Artwork, High, Two colors"]);
        let high = app.preferences.recent_basic.entries()[1].profile.clone();
        to_mode(app, window, cx);
        app.dispatch(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::RecentBasicSettings)), window, cx);
        // An earlier advanced selection survives Start; the replay turns it off.
        app.settings.advanced = true;
        replay(app, 1, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Review);
        assert_eq!(labels(app), ["Artwork, High, Two colors", "Artwork, Medium, Two colors"]);
        assert!(!app.settings.advanced);
        assert_eq!(stored_settings_profile(&app.current_profile()), stored_settings_profile(&high));

        to_mode(app, window, cx);
        app.dispatch(Action::SkipReview, window, cx);
        replay(app, 1, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Export);
        assert_eq!(labels(app), ["Artwork, Medium, Two colors", "Artwork, High, Two colors"]);
        app.dispatch(Action::SkipReview, window, cx);

        // A fixed palette without a color count requests exactly 0..0, which the
        // palette task rejects; no fallback range runs.
        to_mode(app, window, cx);
        let mut fixed = high.clone(); fixed.settings.palette_mode = 1; fixed.palette_color_count = 0;
        assert!(app.preferences.recent_basic.record_completion(&fixed, 0, true, false));
        let before = app.preferences.recent_basic.stored_profiles();
        replay(app, 0, window, cx); settle(app, cx);
        assert!(app.error);
        assert_eq!(app.wizard.page, Page::Mode);
        assert_eq!((app.settings.palette_mode, app.settings.advanced, app.workflows.palette_count), (1, false, 0.0));
        assert_eq!(app.preferences.recent_basic.stored_profiles(), before);
        assert!(app.workflows.expanded(Disclosure::RecentBasicSettings));

        // Auto-detect colors processes nothing: its controls stay selected, the
        // list is unchanged and Mode stays without the advanced replay error.
        let mut detect = high.clone(); detect.settings.palette_mode = 3;
        assert!(app.preferences.recent_basic.record_completion(&detect, 0, true, false));
        let before = app.preferences.recent_basic.stored_profiles();
        replay(app, 0, window, cx); finish(app, cx);
        assert_eq!(app.wizard.page, Page::Mode);
        assert_eq!((app.settings.palette_mode, app.settings.advanced), (3, false));
        assert_eq!(app.preferences.recent_basic.stored_profiles(), before);
    });
}

/// Public action order from the complete native Recent30 recording. Optional
/// output contains only this independent application's state for comparison on
/// the reference host, so the private native response never leaves that host.
#[gpui::test]
fn complete_recent30_public_history(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let source = directory.0.join("artwork.png");
    std::fs::write(&source, include_bytes!("../../../../test/fixtures/recent30-artwork.png")).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    let pages = [4,19,19,19,19,19,0,4,4,12,12,12,13,13,14,16,16,15,17,17,17,0,4,19,19,0,4,18,18,18];
    let mut frames = Vec::new();
    for sequence in 1..=30 {
        let before = app.read_with(visual, |app, cx| recent30_state(app, cx));
        app.update_in(visual, |app, window, cx| {
            let action = match sequence {
                1 => { app.open(source.clone(), cx); None },
                2 => Some(Action::Automatic),
                3 | 6 => Some(Action::Set("quality", json!(1))),
                4 => Some(Action::Set("quality", json!(2))),
                5 => Some(Action::Set("paletteMode", json!(2))),
                7 | 22 | 26 => Some(Action::Start),
                8 | 13 | 15 | 16 | 18 | 19 | 23 | 27 => Some(Action::Next),
                9 => Some(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::RecentBasicSettings))),
                10 => Some(Action::Advanced),
                11 => Some(Action::Set("paletteMode", json!(1))),
                12 => Some(Action::Set("sensitivity", json!(0))),
                14 => Some(Action::Number("complexity", 1)),
                17 => None,
                20 => Some(Action::Workflow(WorkflowAction::ToggleDisclosure(Disclosure::SaveSettings))),
                21 => Some(Action::Workflow(WorkflowAction::SaveProfile(false))),
                24 => Some(Action::Workflow(WorkflowAction::ReplayRecentBasic(2))),
                25 => Some(Action::SkipReview),
                28 => Some(Action::Workflow(WorkflowAction::ReplayRecentBasic(1))),
                29 => Some(Action::Workflow(WorkflowAction::ToggleSave(false))),
                30 => Some(Action::Workflow(WorkflowAction::DirectSave(false))),
                _ => unreachable!(),
            };
            if let Some(action) = action { app.dispatch(action, window, cx); }
            finish(app, cx);
        });
        // Exercise the real one-shot callback and its worker, rather than
        // submitting processing that the user did not request. AutoUpdate uses
        // wall time, while GPUI's test executor owns callback delivery.
        let timer = app.read_with(visual, |app, _| app.automatic.scheduled());
        if let Some((_, deadline)) = timer {
            let delay = deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1);
            std::thread::sleep(delay);
            visual.executor().advance_clock(delay);
            visual.run_until_parked();
            app.update_in(visual, |app, _, cx| finish(app, cx));
        }
        let after = app.read_with(visual, |app, cx| recent30_state(app, cx));
        frames.push(json!({"sequence":sequence,"before":before,"after":after}));
        assert_eq!(frames.last().unwrap()["after"]["page"], pages[sequence - 1], "native page after event {sequence}");
    }
    app.read_with(visual, |app, _| {
        assert_eq!(labels(app), ["Artwork, Medium, 4 colors", "Artwork, Low, Unlimited colors",
            "Artwork, Medium, Unlimited colors", "Artwork, Low, 4 colors", "Artwork, High, 4 colors"]);
        assert_eq!(app.preferences.recent_basic.visible_entries().len(), 4);
        assert!(!app.settings.advanced);
        assert!(app.wizard.skip_review);
    });
    let saved = std::fs::read(directory.0.join("artwork.ai")).unwrap();
    assert!(!saved.is_empty());
    if let Some(output) = std::env::var_os("OPENVECTOR_RECENT30_REPLAY_OUTPUT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output).unwrap();
        serde_json::to_writer(&mut file, &json!({"frames":frames,"saved":base64::engine::general_purpose::STANDARD.encode(saved)})).unwrap();
        file.flush().unwrap();
    }
}

fn recent30_state(app: &OpenVectorApp, cx: &App) -> Value {
    use crate::viewer::paint::Segment;
    let scene = |scene: &Arc<crate::viewer::paint::Scene>| json!(scene.shapes.iter().map(|shape| {
        json!({"color":shape.color,"segments":shape.segments.iter().map(|segment| match segment {
            Segment::Move(x, y) => json!(["M", x, y]), Segment::Line(x, y) => json!(["L", x, y]),
            Segment::Cubic(points) => json!(["C", points]), Segment::Close => json!(["Z"]),
        }).collect::<Vec<_>>()})
    }).collect::<Vec<_>>());
    let document = app.document.as_ref().map(|document| json!({
        "settings":document.settings.to_json(),"colorCount":document.committed_count,
        "stageKeys":{"palette":document.stage_keys.palette,"segmentation":document.stage_keys.segmentation,
            "smoothing":document.stage_keys.smoothing,"fitting":document.stage_keys.fitting},
        "palettes":document.palettes,"vectors":document.vectors.as_ref().map(scene)
    }));
    let page = match app.wizard.page {
        Page::Load => 0, Page::Mode => 4, Page::Palette => 12, Page::Segmentation => 13,
        Page::Editing => 14, Page::Smoothing => 16, Page::Background => 15, Page::Review => 19,
        Page::Export => if app.settings.advanced { 17 } else { 18 },
        page => panic!("Unexpected Recent30 page {page:?}"),
    };
    json!({"page":page,"selected":app.current_profile().to_json(),
        "paletteRuntime":{"backup":app.workflows.palette.backup,"customized":app.workflows.palette.customized},
        "recent":app.preferences.recent_basic.entries().iter().map(|entry| json!({"label":entry.label,"profile":entry.profile.to_json()})).collect::<Vec<_>>(),
        "named":app.workflows.profiles.snapshot().iter().map(|entry| json!({"name":entry.name,"profile":entry.profile.to_json()})).collect::<Vec<_>>(),
        "saveName":app.workflows.profile_name.read(cx).text,"skipReview":app.wizard.skip_review,"document":document})
}
