mod support;
use support::{FixtureDirectory, bitmap};
use std::{path::{Path, PathBuf}, sync::Arc, time::Duration};
use openvector::{document_controls::DocumentSettings, image_document::ImageDocument, image_input::{decode_image, resolve_image_filename}, palette::CoreSession, export_options::ExportOptions};
use openvector_gui::{drag_result::DragFile, jobs::{Command, DocumentView, Event, SourceOrigin, Worker}};
use serde_json::{Value, json};

fn finish(worker: &Worker) -> Result<Option<DocumentView>, String> {
    let mut document = None;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).expect("worker must settle every command") {
            Event::Document(view) => document = Some(view),
            Event::DocumentCleared => document = None,
            Event::Finished(result) => return result.map(|_| document),
            Event::Overwrite(_) => panic!("Unexpected overwrite prompt"),
            Event::DragPrepared { .. } => panic!("Unexpected drag preparation"),
            Event::SourcePrepared => {},
            Event::Progress { .. } | Event::BatchState(_) | Event::BatchLog(_) | Event::BatchResults(_) | Event::BatchPreview { .. } | Event::Selection { .. } | Event::Fitting { .. } | Event::ReplayUnprocessed | Event::BatchSelection { .. } => {},
        }
    }
}

/// Runs one drag preparation to completion. Preparation is export-only work: it answers its
/// own request once, only after the export is complete, and never replaces the presented document.
fn prepare_drag(worker: &Worker, request: u64, root: &Path, format: &'static str, options: Value) -> Result<Arc<DragFile>, String> {
    worker.submit(Command::PrepareDrag { request, directory: root.to_owned(), format, options }).unwrap();
    let mut answer = None;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).expect("worker must settle every command") {
            // Export progress after the answer would mean the file was offered while still being written.
            Event::Progress { .. } => assert!(answer.is_none(), "drag file published before its export finished"),
            Event::DragPrepared { request: answered, result } => {
                assert_eq!(answered, request);
                assert!(answer.replace(result).is_none(), "drag request answered twice");
            }
            Event::Finished(finished) => {
                let answer = answer.expect("drag request answered before its command finished");
                assert!(answer.is_err() || finished.is_ok(), "published drag file failed its command: {finished:?}");
                return answer;
            }
            Event::Document(_) | Event::DocumentCleared => panic!("drag preparation must not replace the presented document"),
            _ => panic!("drag preparation emitted an unrelated event"),
        }
    }
}

/// Prepares a drag file under `root`, then writes an ordinary Export of the same document state
/// beside `root`. Both must hold the same bytes; returns the drag file and its bytes.
fn drag_matching_export(worker: &Worker, request: u64, root: &Path, format: &'static str, options: Value) -> (Arc<DragFile>, Vec<u8>) {
    let file = prepare_drag(worker, request, root, format, options.clone()).unwrap();
    let bytes = std::fs::read(file.path()).unwrap();
    let exported = root.with_file_name(format!("export-{request}.{format}"));
    worker.submit(Command::Export { path: exported.clone(), options: options.clone() }).unwrap();
    finish(worker).unwrap();
    assert_eq!(without_creation_date(&bytes), without_creation_date(&std::fs::read(exported).unwrap()), "{format} {options}");
    (file, bytes)
}

/// EPS stamps the local time it was written; every other exported byte is deterministic.
fn without_creation_date(bytes: &[u8]) -> Vec<u8> {
    bytes.split_inclusive(|&byte| byte == b'\n').filter(|line| !line.starts_with(b"%%CreationDate:")).flatten().copied().collect()
}

/// Every file and directory below `root`, sorted; a missing root holds nothing.
fn entries_below(root: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        let children = match std::fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("Cannot list {}: {error}", directory.display()),
        };
        for child in children {
            let path = child.unwrap().path();
            if path.is_dir() { directories.push(path.clone()); }
            entries.push(path);
        }
    }
    entries.sort();
    entries
}

#[test]
fn worker_commits_startup_profile_before_first_source_and_later_processing() {
    let directory = FixtureDirectory::new();
    let input = directory.0.join("startup.bmp");
    std::fs::write(&input, bitmap()).unwrap();
    let mut startup = openvector::settings_profile::default_selected_profile();
    startup.color_count = 7;
    startup.settings.sensitivity = 2;
    let worker = Worker::start(startup.clone()).unwrap();
    let selected = DocumentSettings { category: 2, quality: 0, sensitivity: 0,
        palette_mode: 2, background_mode: 0, ..startup.settings.clone() };
    worker.submit(Command::Open { path: input, settings: Value::Object(selected.to_json()) }).unwrap();
    let loaded = finish(&worker).unwrap().unwrap();
    assert_eq!(loaded.settings, startup.settings, "first Open preserves the startup commit, not later selected edits");
    assert_eq!(loaded.committed_count, 7);
    let mut controls = Value::Object(selected.to_json()); controls["count"] = json!(9);
    worker.submit(Command::Process { stage: "fitting", input: controls }).unwrap();
    let processed = finish(&worker).unwrap().unwrap();
    assert_eq!(processed.settings, selected);
    assert_eq!(processed.committed_count, 9);
    assert!(processed.vectors.is_some());
}

#[test]
fn invalid_startup_profile_returns_before_worker_readiness() {
    let mut invalid = openvector::settings_profile::default_selected_profile();
    invalid.settings.background[3] = f32::NAN;
    assert!(Worker::start(invalid).is_err());
}

#[test]
fn real_worker_load_process_edit_background_export_and_failed_load() {
    let directory = FixtureDirectory::new();
    let input = directory.0.join("source.bmp"); let output = directory.0.join("worker.svg");
    let bytes = bitmap(); std::fs::write(&input, &bytes).unwrap();
    let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
    worker.submit(Command::Open { path: input.clone(), settings: Value::Object(DocumentSettings::default().to_json()) }).unwrap();
    let opened = finish(&worker).unwrap().unwrap();
    assert_eq!((opened.width, opened.height), (16, 16)); assert!(!opened.has_alpha); assert!(opened.vectors.is_none());
    let settings = DocumentSettings { category: 2, quality: 0, palette_mode: 2, background_mode: 0, ..DocumentSettings::default() };
    let controls = Value::Object(settings.to_json());
    worker.submit(Command::Process { stage: "fitting", input: controls.clone() }).unwrap();
    let processed = finish(&worker).unwrap().unwrap();
    assert!(processed.vectors.as_ref().is_some_and(|scene| !scene.shapes.is_empty()));
    assert!(std::sync::Arc::ptr_eq(&opened.source, &processed.source));
    worker.submit(Command::Export { path: output.clone(), options: json!({}) }).unwrap(); finish(&worker).unwrap();
    let mut reference = ImageDocument::new(CoreSession::default());
    let decoded = decode_image(&bytes).unwrap();
    let name = resolve_image_filename(input.to_str().unwrap());
    reference.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, name.shadow_control as f64).unwrap();
    for stage in ["segmentation", "smoothing", "fitting"] { reference.run_task(stage, &controls, None).unwrap(); }
    let reference_path = directory.0.join("reference.svg");
    let saved = reference.save_vector_file(&reference_path, &ExportOptions::default(), None).unwrap();
    assert_eq!(saved.status, 0);
    assert_eq!(std::fs::read(output).unwrap(), std::fs::read(reference_path).unwrap());
    let alpha = reference.source_alpha_type();
    let state = reference.state();
    let bitmap_input = openvector::bitmap_export::BitmapExportInput {
        geometry: state.fitting.as_ref().map(|fitting| &fitting.geometry), width: 16, height: 16,
        source_alpha_type: alpha, background_mode: state.settings.background_mode, background_undo_count: 0, hidden_regions: &[],
    };
    for format in ["png", "bmp", "jpg", "tif"] {
        let target = directory.0.join(format!("worker.{format}"));
        worker.submit(Command::BitmapExport { path: target.clone(), format: format.into(), scale: 1.25, flatten: false }).unwrap();
        finish(&worker).unwrap();
        let expected = openvector::bitmap_export::export_bitmap(&bitmap_input, &openvector::bitmap_export::BitmapExportOptions {
            format: format.parse().unwrap(), scale: 1.25, flatten: false,
        }).unwrap();
        assert_eq!(std::fs::read(target).unwrap(), expected, "{format}");
    }

    // Drag preparation is an ordinary export into an app-owned file. Cover every headless vector
    // format (EMF reads the host display) and each export option the app sends.
    let drag_root = directory.0.join("drag-exports");
    let app_options = |layout: &str, curve_mode: &str, thin_stroke: bool, compact: bool|
        json!({"layout":layout,"curveMode":curve_mode,"thinStroke":thin_stroke,"compact":compact});
    let export_defaults = app_options("grouped-cutouts", "dense-lines", false, false);
    let mut prepared = Vec::new();
    for (request, (format, options)) in (1..).zip([
        ("svg", export_defaults.clone()), ("svg", app_options("stacked", "dense-lines", true, false)),
        ("pdf", app_options("cutouts", "dense-lines", false, false)), ("ai", export_defaults.clone()),
        ("eps", export_defaults.clone()), ("eps", app_options("grouped-cutouts", "dense-lines", false, true)),
        ("dxf", app_options("grouped-cutouts", "splines", false, false)),
    ]) {
        let (file, bytes) = drag_matching_export(&worker, request, &drag_root, format, options);
        assert!(file.path().ends_with(format!("source.{format}")), "{}", file.path().display());
        prepared.push((file, bytes));
    }
    let mut published: Vec<_> = prepared.iter().map(|(file, _)| file.path().to_owned()).collect();
    published.sort();
    assert_eq!(entries_below(&drag_root).into_iter().filter(|path| path.is_file()).collect::<Vec<_>>(), published,
        "drag preparation leaves nothing but its published files");
    assert_ne!(prepared[0].1, prepared[1].1, "layout and thin stroke reach the drag file");
    assert_ne!(without_creation_date(&prepared[4].1), without_creation_date(&prepared[5].1), "compact reaches the EPS drag file");
    // Failed preparations publish nothing, remove nothing they did not create, and leave the document exportable.
    let before = entries_below(&drag_root);
    assert!(prepare_drag(&worker, 8, &drag_root, "svg", json!({"layout":"unknown"})).is_err());
    assert_eq!(entries_below(&drag_root), before);
    let blocked = directory.0.join("blocked");
    std::fs::write(&blocked, b"foreign file").unwrap();
    assert!(prepare_drag(&worker, 9, &blocked, "svg", export_defaults.clone()).is_err());
    assert_eq!(std::fs::read(&blocked).unwrap(), b"foreign file");
    assert_eq!(drag_matching_export(&worker, 10, &drag_root, "svg", export_defaults.clone()).1, prepared[0].1);

    worker.submit(Command::Edit { action: "begin", input: Value::Null }).unwrap();
    let began = finish(&worker).unwrap().unwrap();
    // Export-only commands, drag preparation included, keep the presented source and scene.
    assert!(Arc::ptr_eq(&began.source, &processed.source));
    assert!(Arc::ptr_eq(began.vectors.as_ref().unwrap(), processed.vectors.as_ref().unwrap()));
    worker.submit(Command::Edit { action: "apply", input: json!({"tool":"fill","x":6,"y":6,"color":[0,0,1,1]}) }).unwrap();
    assert!(finish(&worker).unwrap().unwrap().undo);
    worker.submit(Command::Edit { action: "undo", input: Value::Null }).unwrap(); assert!(finish(&worker).unwrap().unwrap().redo);
    worker.submit(Command::Edit { action: "redo", input: Value::Null }).unwrap(); assert!(finish(&worker).unwrap().unwrap().undo);
    worker.submit(Command::Edit { action: "finish", input: controls }).unwrap();
    assert!(finish(&worker).unwrap().unwrap().has_committed_edits);
    let (_, edited) = drag_matching_export(&worker, 11, &drag_root, "svg", export_defaults.clone());
    assert_ne!(edited, prepared[0].1, "drag preparation exports the current fitting");
    worker.submit(Command::Background { action: "begin", input: Value::Null }).unwrap(); finish(&worker).unwrap();
    worker.submit(Command::Background { action: "gesture", input: json!({"phase":"press","point":[6,6],"buttons":1,"button":1,"modifiers":0}) }).unwrap();
    let hidden = finish(&worker).unwrap().unwrap(); assert!(hidden.undo); assert!(hidden.removed.is_some());
    let (_, without_region) = drag_matching_export(&worker, 12, &drag_root, "svg", export_defaults.clone());
    assert_ne!(without_region, edited, "drag preparation omits hidden regions");
    worker.submit(Command::Background { action: "undo", input: Value::Null }).unwrap();
    let restored = finish(&worker).unwrap().unwrap(); assert!(restored.redo); assert!(restored.removed.is_none());
    assert_eq!(drag_matching_export(&worker, 13, &drag_root, "svg", export_defaults).1, edited, "restored visibility restores the drag file");
    for (file, bytes) in &prepared { assert_eq!(&std::fs::read(file.path()).unwrap(), bytes, "later work rewrote a published drag file"); }

    let corrupt = directory.0.join("corrupt.bmp"); std::fs::write(&corrupt, b"not an image").unwrap();
    worker.submit(Command::Open { path: corrupt, settings: Value::Object(DocumentSettings::default().to_json()) }).unwrap();
    let mut retained = None;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
            Event::Document(view) => retained = Some(view),
            Event::Finished(result) => { assert!(result.is_err()); break; }
            _ => {},
        }
    }
    let retained = retained.expect("failed load retains the prior document");
    assert_eq!(retained.filename, input); assert!(retained.vectors.is_some());
    assert_eq!(retained.display_vectors.as_ref().unwrap().shapes, restored.display_vectors.as_ref().unwrap().shapes);
}

/// The GUI records the selection a fitting produced, so a command reports its
/// fitting only after its final Selection, here one carrying the palette the
/// same command chose. A repeat finds every stage cached and reports no work.
#[test]
fn fitting_outcome_follows_the_final_selection() {
    let directory = FixtureDirectory::new();
    let input = directory.0.join("source.bmp"); std::fs::write(&input, bitmap()).unwrap();
    let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
    worker.submit(Command::Open { path: input, settings: Value::Object(DocumentSettings::default().to_json()) }).unwrap();
    finish(&worker).unwrap();
    let controls = Value::Object(DocumentSettings { category: 2, quality: 0, palette_mode: 0, background_mode: 0, ..DocumentSettings::default() }.to_json());
    for did_work in [true, false] {
        worker.submit(Command::Process { stage: "fitting", input: controls.clone() }).unwrap();
        let mut order = Vec::new();
        loop {
            match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                Event::Selection { palette_ready, .. } => { assert!(palette_ready); order.push("selection"); }
                Event::Fitting { status, did_work: worked } => { assert_eq!((status, worked), (0, did_work)); order.push("fitting"); }
                Event::Finished(result) => { result.unwrap(); break; }
                _ => {},
            }
        }
        assert_eq!(order, ["selection", "fitting"]);
    }
}

#[test]
fn retained_preview_tracks_visibility_without_restoring_current_vectors() {
    let directory = FixtureDirectory::new();
    let input = directory.0.join("preview.bmp");
    std::fs::write(&input, bitmap()).unwrap();
    let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
    let defaults = Value::Object(DocumentSettings::default().to_json());
    let controls = Value::Object(DocumentSettings {
        category: 2, quality: 0, palette_mode: 2, background_mode: 0,
        ..DocumentSettings::default()
    }.to_json());
    worker.submit(Command::Open { path: input.clone(), settings: defaults.clone() }).unwrap();
    assert!(finish(&worker).unwrap().unwrap().display_vectors.is_none());
    worker.submit(Command::Process { stage: "fitting", input: controls.clone() }).unwrap();
    let complete = finish(&worker).unwrap().unwrap();
    let original = complete.display_vectors.as_ref().unwrap();
    assert!(Arc::ptr_eq(original, complete.vectors.as_ref().unwrap()));

    worker.submit(Command::Background { action: "begin", input: Value::Null }).unwrap();
    finish(&worker).unwrap();
    worker.submit(Command::Background { action: "gesture", input: json!({"phase":"press","point":[6,6],"buttons":1,"button":1,"modifiers":0}) }).unwrap();
    let hidden = finish(&worker).unwrap().unwrap();
    assert!(hidden.undo);
    assert!(hidden.display_removed.is_some());
    worker.submit(Command::Edit { action: "begin", input: Value::Null }).unwrap();
    finish(&worker).unwrap();
    worker.submit(Command::Edit { action: "apply", input: json!({"tool":"fill","x":6,"y":6,"color":[0,0,1,1]}) }).unwrap();
    let edited = finish(&worker).unwrap().unwrap();
    assert!(Arc::ptr_eq(hidden.display_vectors.as_ref().unwrap(), edited.display_vectors.as_ref().unwrap()));
    worker.submit(Command::Edit { action: "commit", input: Value::Null }).unwrap();
    let committed = finish(&worker).unwrap().unwrap();
    assert!(committed.vectors.is_none());
    assert!(committed.removed.is_none());
    assert!(Arc::ptr_eq(hidden.display_vectors.as_ref().unwrap(), committed.display_vectors.as_ref().unwrap()));
    assert!(Arc::ptr_eq(hidden.display_removed.as_ref().unwrap(), committed.display_removed.as_ref().unwrap()));
    assert!(Arc::ptr_eq(&complete.source, &committed.source));
    let unavailable = directory.0.join("unavailable.svg");
    worker.submit(Command::Export { path: unavailable.clone(), options: json!({}) }).unwrap();
    assert!(finish(&worker).is_err());
    assert!(!unavailable.exists());
    // Retained display geometry never supplies a drag file; the failed preparation leaves nothing behind.
    let drag_root = directory.0.join("drag-exports");
    assert!(prepare_drag(&worker, 1, &drag_root, "svg", json!({})).is_err());
    assert_eq!(entries_below(&drag_root), Vec::<PathBuf>::new());

    worker.submit(Command::Background { action: "begin", input: Value::Null }).unwrap();
    let began = finish(&worker).unwrap().unwrap();
    assert!(Arc::ptr_eq(&committed.source, &began.source));
    assert!(Arc::ptr_eq(committed.display_vectors.as_ref().unwrap(), began.display_vectors.as_ref().unwrap()), "failed preparation keeps the retained preview");
    worker.submit(Command::Background { action: "gesture", input: json!({"phase":"press","point":[6,6],"buttons":1,"button":1,"modifiers":0}) }).unwrap();
    assert!(finish(&worker).is_err(), "retained preview must not supply hit-test boundaries");
    worker.submit(Command::Background { action: "undo", input: Value::Null }).unwrap();
    let visible = finish(&worker).unwrap().unwrap();
    assert!(visible.vectors.is_none());
    assert!(visible.redo);
    assert!(visible.display_removed.is_none());
    assert_eq!(visible.display_vectors.as_ref().unwrap().shapes, original.shapes);
    worker.submit(Command::Background { action: "redo", input: Value::Null }).unwrap();
    let rehidden = finish(&worker).unwrap().unwrap();
    assert!(rehidden.undo);
    assert_eq!(rehidden.display_vectors.as_ref().unwrap().shapes, hidden.display_vectors.as_ref().unwrap().shapes);
    assert_eq!(rehidden.display_removed.as_ref().unwrap().shapes, hidden.display_removed.as_ref().unwrap().shapes);

    worker.submit(Command::Process { stage: "vectors", input: controls.clone() }).unwrap();
    let renewed = finish(&worker).unwrap().unwrap();
    assert!(Arc::ptr_eq(renewed.display_vectors.as_ref().unwrap(), renewed.vectors.as_ref().unwrap()));
    assert_ne!(renewed.display_vectors.as_ref().unwrap().shapes, original.shapes);
    assert!(renewed.display_removed.is_none(), "new fitting clears active visibility");
    assert!(renewed.redo, "new fitting moves visibility actions to redo");
    worker.submit(Command::Process { stage: "vectors", input: controls.clone() }).unwrap();
    let cached = finish(&worker).unwrap().unwrap();
    assert_eq!(cached.display_vectors.as_ref().unwrap().shapes, renewed.display_vectors.as_ref().unwrap().shapes);
    assert!(cached.redo);
    // Preparation recovers once a current fitting exists again.
    drag_matching_export(&worker, 2, &drag_root, "svg", json!({}));

    worker.submit(Command::Open { path: input.clone(), settings: defaults.clone() }).unwrap();
    let replacement = finish(&worker).unwrap().unwrap();
    assert!(replacement.display_vectors.is_none());
    assert!(replacement.display_removed.is_none());
    assert!(!Arc::ptr_eq(&replacement.source, &complete.source));
    worker.submit(Command::Process { stage: "fitting", input: controls }).unwrap();
    assert!(finish(&worker).unwrap().unwrap().display_vectors.is_some());
    worker.submit(Command::Reset(defaults.clone())).unwrap();
    assert!(finish(&worker).unwrap().is_none());
    worker.submit(Command::Open { path: input, settings: defaults }).unwrap();
    assert!(finish(&worker).unwrap().unwrap().display_vectors.is_none());
}

#[test]
fn removed_preview_keeps_transparent_region_boundaries() {
    use openvector_gui::viewer::paint::Scene;
    use openvector::vector_output::VectorOutputInput;
    let decoded = decode_image(&bitmap()).unwrap();
    let mut document = ImageDocument::new(CoreSession::default());
    document.set_source(decoded.width, decoded.height, &decoded.bgra, false, 0.0).unwrap();
    let controls = Value::Object(DocumentSettings {
        category: 2, quality: 0, palette_mode: 2, background_mode: 0,
        ..DocumentSettings::default()
    }.to_json());
    for stage in ["segmentation", "smoothing", "fitting"] {
        document.run_task(stage, &controls, None).unwrap();
    }
    let mut geometry = document.state().fitting.as_ref().unwrap().geometry.clone();
    let input = VectorOutputInput::new(&geometry, 16, 16);
    let opaque = Scene::from_removed_output(&input).unwrap();
    for region in &mut geometry.regions { region.bgra[3] = 0; }
    let input = VectorOutputInput::new(&geometry, 16, 16);
    assert!(Scene::from_output(&input).unwrap().shapes.is_empty());
    let transparent = Scene::from_removed_output(&input).unwrap();
    assert_eq!(transparent.shapes.iter().map(|shape| &shape.segments).collect::<Vec<_>>(),
        opaque.shapes.iter().map(|shape| &shape.segments).collect::<Vec<_>>());
}

#[test]
fn real_batch_updates_selected_metadata_and_clears_failed_source() {
    use openvector::{batch_queue::BatchQueue, settings_profile::default_batch_profile};
    let directory = FixtureDirectory::new();
    let input = directory.0.join("batch.bmp"); std::fs::write(&input, bitmap()).unwrap();
    let root = directory.0.to_string_lossy();
    let mut queue = BatchQueue::new(&root, &root); queue.export_format = "svg".into();
    queue.add_files(&[input.to_string_lossy().into_owned()]);
    let mut profile = default_batch_profile();
    profile.settings.category = 2; profile.settings.quality = 0; profile.settings.palette_mode = 1; profile.basic_color_count = 3;
    let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
    worker.submit(Command::Batch { queue, profile: profile.clone(), options: json!({}) }).unwrap();
    let mut selection = None;
    let mut shown = None;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
            Event::BatchSelection { profile, .. } => selection = Some(profile),
            Event::Document(document) => shown = Some(document),
            Event::Finished(result) => { result.unwrap(); break; },
            Event::Overwrite(_) => panic!("Fresh batch directory must not require overwrite confirmation"),
            _ => {},
        }
    }
    let selected = selection.unwrap();
    assert_eq!(selected.color_count, 3); assert_eq!(selected.palette_color_count, 3);
    assert_eq!(selected.basic_color_count, 3);
    assert!(shown.unwrap().vectors.is_some()); assert!(directory.0.join("batch.svg").exists());
    let mut queue = BatchQueue::new(&root, &root); queue.export_format = "svg".into();
    queue.add_files(&[directory.0.join("missing.bmp").to_string_lossy().into_owned()]);
    worker.submit(Command::Batch { queue, profile, options: json!({}) }).unwrap();
    let mut cleared = false;
    loop {
        match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
            Event::DocumentCleared => cleared = true,
            Event::Finished(result) => { result.unwrap(); break; },
            _ => {},
        }
    }
    assert!(cleared, "A failed batch load must not keep the previous viewer image");
}

#[test]
fn clipboard_bytes_share_file_decoder_without_reading_suggested_path() {
    let directory = FixtureDirectory::new();
    let input = directory.0.join("original.bmp");
    let bytes = bitmap();
    std::fs::write(&input, &bytes).unwrap();
    let settings = Value::Object(DocumentSettings::default().to_json());
    let worker = Worker::start(openvector::settings_profile::default_selected_profile()).unwrap();
    worker.submit(Command::Open { path: input.clone(), settings: settings.clone() }).unwrap();
    let file = finish(&worker).unwrap().unwrap();
    assert_eq!(file.origin, SourceOrigin::Path);

    // This path need not exist: it supplies the export suggestion, not input bytes.
    let clipboard_directory = directory.0.join("not-a-source-directory");
    worker.submit(Command::PasteImage { bytes: bytes.into(), directory: clipboard_directory.clone(), settings: settings.clone() }).unwrap();
    let pasted = finish(&worker).unwrap().unwrap();
    assert_eq!(pasted.origin, SourceOrigin::Clipboard);
    assert_eq!(pasted.filename, clipboard_directory.join("clipboard.png"));
    assert!(!clipboard_directory.exists());
    assert_eq!((pasted.width, pasted.height, pasted.has_alpha), (file.width, file.height, file.has_alpha));
    assert_eq!(pasted.source.as_bytes(0).unwrap(), file.source.as_bytes(0).unwrap());
    assert!(!std::sync::Arc::ptr_eq(&pasted.source, &file.source));

    let controls = Value::Object(DocumentSettings { category: 2, quality: 0, palette_mode: 2, background_mode: 0, ..DocumentSettings::default() }.to_json());
    worker.submit(Command::Process { stage: "fitting", input: controls }).unwrap();
    let converted = finish(&worker).unwrap().unwrap();
    assert_eq!(converted.origin, SourceOrigin::Clipboard);
    assert!(converted.vectors.is_some());
    let output = directory.0.join("before.svg");
    worker.submit(Command::Export { path: output.clone(), options: json!({}) }).unwrap();
    finish(&worker).unwrap();
    let expected = std::fs::read(output).unwrap();

    for invalid in [Vec::new(), b"not an image".to_vec(), b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec()] {
        worker.submit(Command::PasteImage { bytes: invalid.into(), directory: directory.0.clone(), settings: settings.clone() }).unwrap();
        let mut retained = None;
        loop {
            match worker.events.recv_timeout(Duration::from_secs(120)).unwrap() {
                Event::Document(view) => retained = Some(view),
                Event::DocumentCleared => panic!("Invalid clipboard must not clear the current image"),
                Event::Finished(result) => { assert!(result.unwrap_err().starts_with("Cannot paste image:")); break; },
                _ => {},
            }
        }
        let retained = retained.expect("Failed clipboard admission retains the document");
        assert_eq!(retained.origin, SourceOrigin::Clipboard);
        assert_eq!(retained.filename, pasted.filename);
        assert!(std::sync::Arc::ptr_eq(&retained.source, &pasted.source));
        assert!(retained.vectors.is_some());
        let output = directory.0.join("after.svg");
        worker.submit(Command::Export { path: output.clone(), options: json!({}) }).unwrap();
        finish(&worker).unwrap();
        assert_eq!(std::fs::read(output).unwrap(), expected);
    }
    worker.submit(Command::Open { path: input.clone(), settings }).unwrap();
    let reopened = finish(&worker).unwrap().unwrap();
    assert_eq!(reopened.origin, SourceOrigin::Path);
    assert_eq!(reopened.filename, input);
}
