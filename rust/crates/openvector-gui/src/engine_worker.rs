use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc}, time::Duration};
use openvector::{batch_queue::OverwriteDecision, batch_runner::{BatchRunner, BatchEvent},
    export_options::{read_export_options, ExportOptions}, image_document::{ImageDocument, DocumentProgress, DocumentSnapshot, DocumentObserver, DocumentExportEnvironment, CANCEL_TASK},
    image_input::{decode_image, resolve_image_filename}, pixel_input::quantize_qcolor_channels,
    vector_output::VectorOutputInput};
use serde_json::{Value, json};
use crate::{jobs::{Bitmap, Command, DocumentView, Event, SourceOrigin}, viewer::paint::Scene};

#[cfg(test)]
#[path = "../../openvector/tests/common/mod.rs"]
mod common;

pub fn run(mut document: ImageDocument, mut committed_count: i32, commands: mpsc::Receiver<Command>, events: mpsc::Sender<Event>, cancelled: Arc<AtomicBool>, decisions: mpsc::Receiver<OverwriteDecision>) {
    let mut filename = PathBuf::new();
    let mut origin = SourceOrigin::Path;
    let mut has_alpha = false;
    let mut source = None;
    let mut display = DisplayScene::default();
    while let Ok(command) = commands.recv() {
        let source_may_change = matches!(&command, Command::Open { .. } | Command::PasteImage { .. } | Command::PrepareSource { .. } | Command::Batch { .. } | Command::Reset(_));
        let exported = matches!(&command, Command::Export { .. } | Command::BitmapExport { .. } | Command::PrepareDrag { .. });
        let refresh_vectors = matches!(&command, Command::Automatic(_) | Command::Process { .. } | Command::Profile { .. } | Command::Batch { .. } | Command::Edit { action: "finish", .. });
        let mut source_changed = false;
        // Status and work flag of this command's completed single-image fitting
        // task, set by the fitting stage observers. Batch work never sets it.
        let mut fitting = None;
        let mut source_recommendations = None;
        let mut progress = |progress: &DocumentProgress, _: &DocumentSnapshot| -> Result<(), String> {
            if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
            events.send(Event::Progress { stage: progress.stage.into(), percent: progress.percent as f64 }).map_err(|_| CANCEL_TASK.to_string())
        };
        let result = match command {
            Command::Reset(settings) => (|| {
                openvector::document_controls::read_document_settings(&settings).map_err(|error| error.to_string())?;
                document.reset(); document.commit_settings(&settings)?;
                committed_count = input_count(&settings, committed_count);
                source = None; filename.clear(); has_alpha = false; source_changed = true; Ok(())
            })(),
            Command::Commit(settings) => document.commit_settings(&settings).map(|()| {
                committed_count = input_count(&settings, committed_count);
            }),
            Command::PrepareSource { shrink } => (|| {
                if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
                source_changed = shrink && document.shrink_source();
                // Once admitted, even Leave unchanged is final for this source.
                events.send(Event::SourcePrepared).map_err(|_| CANCEL_TASK.to_string())
            })(),
            Command::AnalyzeSource(input) => (|| {
                if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
                let result = document.run_task("classification", &input, None)?;
                if result.state.status == 2 { return Err(CANCEL_TASK.into()); }
                let classification = result.state.classification;
                source_recommendations = Some([classification.category, classification.quality, classification.palette_mode]);
                Ok(())
            })(),
            Command::Open { path, settings } => (|| {
                let bytes = std::fs::read(&path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
                let retained = document.state().original.width != 0;
                has_alpha = admit_source(&mut document, &bytes, &path, &settings, &cancelled)?;
                if retained { committed_count = input_count(&settings, committed_count); }
                filename = path; origin = SourceOrigin::Path; source_changed = true;
                Ok(())
            })(),
            Command::PasteImage { bytes, directory, settings } => (|| {
                let suggested = directory.join("clipboard.png");
                let retained = document.state().original.width != 0;
                let alpha = admit_source(&mut document, &bytes, &suggested, &settings, &cancelled)
                    .map_err(|error| if error == CANCEL_TASK { error } else { format!("Cannot paste image: {error}") })?;
                if retained { committed_count = input_count(&settings, committed_count); }
                filename = suggested; origin = SourceOrigin::Clipboard; has_alpha = alpha; source_changed = true;
                Ok(())
            })(),
            Command::Automatic(input) => {
                let mut selected = |stage: &str, task: &openvector::image_document::TaskResult, selection: &openvector::image_document::AutomaticSelection| {
                    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
                    if matches!(stage, "segmentation" | "fitting") {
                        committed_count = selection.palette_count.map_or_else(|| input_count(&input, committed_count), |count| count as i32);
                    }
                    if stage == "fitting" { fitting = Some((task.state.status, task.did_work)); }
                    if matches!(stage, "classification" | "palette") {
                        events.send(Event::Selection { settings: selection.settings.clone(), color_count: selection.palette_count, recommended_background_mode: selection.recommended_background_mode, palette_ready: stage == "palette", classified: stage == "classification" }).map_err(|_| CANCEL_TASK.to_string())?;
                    }
                    Ok(())
                };
                document.run_automatic_with_observers(&input, Some(&mut progress), Some(&mut selected)).map(|_| ())
            },
            Command::Process { stage, input } => process(&mut document, stage, input, &mut committed_count, &mut fitting, &mut progress)
                .and_then(|selection| events.send(Event::Selection { settings: selection.settings, color_count: Some(selection.count as usize),
                    recommended_background_mode: selection.recommended_background_mode, palette_ready: selection.palette_ready, classified: false }).map_err(|_| CANCEL_TASK.to_string())),
            Command::Edit { action, input } => match action {
                "begin" => document.begin_editing_for_output().map(|_| ()), "gesture" => document.gesture(&input).map(|_| ()),
                "apply" => document.edit(&input).map(|_| ()), "undo" => document.undo_edit().map(|_| ()),
                "redo" => document.redo_edit().map(|_| ()), "revert" => document.revert_edits().map(|_| ()),
                "restore" => document.restore_committed_editing().map(|_| ()),
                "leave" => document.leave_editing(),
                "commit" => document.commit_editing().map(|_| ()),
                "apply-editing" => document.apply_editing().map(|_| ()),
                "finish" => {
                    let mut on_stage = |stage: &str, task: &openvector::image_document::TaskResult| {
                        if task.did_work { committed_count = input_count(&input, committed_count); }
                        if stage == "fitting" { fitting = Some((task.state.status, task.did_work)); }
                        Ok(())
                    };
                    document.finish_editing_with_observers(&input, Some(&mut progress), Some(&mut on_stage)).map(|_| ())
                },
                _ => Err(format!("Unknown segmentation editing action: {action}")),
            },
            Command::Background { action, input } => match action {
                "begin" => document.begin_background_editing().map(|_| ()), "gesture" => document.background_gesture(&input).map(|_| ()),
                "undo" => { document.undo_background(); Ok(()) }, "redo" => { document.redo_background(); Ok(()) },
                "revert" => { document.revert_background(); Ok(()) }, "finish" => { document.finish_background_editing(); Ok(()) },
                _ => Err(format!("Unknown background editing action: {action}")),
            },
            Command::Export { path, options } => export(&mut document, &path, &options, &cancelled, &events),
            Command::BitmapExport { path, format, scale, flatten } => bitmap_export(&mut document, &path, &format, scale, flatten, &cancelled),
            Command::PrepareDrag { request, directory, format, options } => {
                let prepared = (|| {
                    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
                    let file = crate::drag_result::DragFile::create(&directory, &filename, format)?;
                    export(&mut document, file.path(), &options, &cancelled, &events)?;
                    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
                    Ok(Arc::new(file))
                })();
                let completed = prepared.as_ref().map(|_| ()).map_err(Clone::clone);
                if events.send(Event::DragPrepared { request, result: prepared }).is_err() { return; }
                completed
            },
            Command::Profile { profile, basic } => {
                let mut on_stage = |stage: &str, task: &openvector::image_document::TaskResult, selected: &openvector::settings_profile::SettingsProfile, _: &openvector::profile_palette::PaletteRuntime| {
                    if stage != "palette" && task.did_work { committed_count = selected.color_count; }
                    if stage == "fitting" { fitting = Some((task.state.status, task.did_work)); }
                    Ok(())
                };
                // The GUI owns the palette runtime: it adopts a computed palette
                // through Selection, never the runner's returned runtime.
                let runtime = openvector::profile_palette::PaletteRuntime::default();
                let replay = if basic {
                    openvector::settings_profile::run_basic_profile(&mut document, &profile, &runtime, Some(&mut progress), Some(&mut on_stage))
                } else {
                    openvector::settings_profile::run_advanced_profile(&mut document, &profile, &runtime, Some(&mut progress), Some(&mut on_stage))
                };
                match replay {
                    Ok(Some(result)) => events.send(Event::Selection { settings: result.profile.settings, color_count: Some(result.profile.color_count as usize),
                        recommended_background_mode: result.profile.recommended_background_mode, palette_ready: result.palette_requested, classified: false }).map_err(|_| CANCEL_TASK.to_string()),
                    // The shared runner processes nothing for other palette modes;
                    // a basic replay then keeps its page and reports no error.
                    Ok(None) if basic => events.send(Event::ReplayUnprocessed).map_err(|_| CANCEL_TASK.to_string()),
                    Ok(None) => Err("Choose a color mode before running this profile.".into()),
                    Err(error) => Err(error),
                }
            },
            Command::Batch { queue, profile, options } => {
                let settings = openvector::settings_profile::batch_profile_controls(&profile);
                let mut selected_profile = profile.clone();
                let mut last_was_load = false;
                let mut runner = BatchRunner::from_document(queue, document);
                let cancel_handle = runner.cancellation_handle();
                let mut observer = |event: &BatchEvent<'_>| -> Result<(), String> {
                    last_was_load = event.stage == "load";
                    filename = event.filename.into();
                    origin = SourceOrigin::Path;
                    if event.progress.is_none() {
                        if last_was_load {
                            source_changed = true;
                            committed_count = profile.color_count;
                            selected_profile = profile.clone(); selected_profile.settings.background = [1.0; 4];
                            if let Some((_, alpha)) = event.source {
                                has_alpha = alpha;
                                // This load is the newest read of the file; the GUI reuses it instead of decoding again.
                                let original = &event.state.original;
                                events.send(Event::BatchPreview { filename: event.filename.into(), preview: Bitmap::preview(original.width, original.height, &original.bgra) }).map_err(|_| CANCEL_TASK.to_string())?;
                            }
                            events.send(Event::BatchSelection { profile: selected_profile.clone(), palette_ready: false }).map_err(|_| CANCEL_TASK.to_string())?;
                        } else if matches!(event.stage, "segmentation" | "smoothing" | "fitting") {
                            selected_profile.settings = event.state.settings.clone();
                            if let Some(count) = event.palette_count { committed_count = count as i32; selected_profile.color_count = count as i32; selected_profile.palette_color_count = count as i32; }
                            events.send(Event::BatchSelection { profile: selected_profile.clone(), palette_ready: event.palette_count.is_some() }).map_err(|_| CANCEL_TASK.to_string())?;
                        }
                    }
                    if cancelled.load(Ordering::Acquire) { cancel_handle.store(true, Ordering::Release); return Err(CANCEL_TASK.into()); }
                    events.send(Event::Progress { stage: format!("{} · {}", event.index + 1, event.stage), percent: event.progress.map_or(0.0, |p| p.percent as f64) }).map_err(|_| CANCEL_TASK.to_string())?;
                    Ok(())
                };
                let result = runner.run(&settings, &options, |files| {
                    if events.send(Event::Overwrite(files.to_vec())).is_err() { return OverwriteDecision::Cancel; }
                    loop {
                        if cancelled.load(Ordering::Acquire) { cancel_handle.store(true, Ordering::Release); return OverwriteDecision::Cancel; }
                        match decisions.recv_timeout(Duration::from_millis(50)) {
                            Ok(decision) => return decision,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return OverwriteDecision::Cancel,
                            Err(mpsc::RecvTimeoutError::Timeout) => {},
                        }
                    }
                }, Some(&mut observer));
                let result = result.and_then(|mut result| {
                    if result.status == "cancelled" && source_changed && last_was_load {
                        selected_profile.settings.palette.clear(); selected_profile.settings.background_mode = -1; selected_profile.settings.background = [1.0, 1.0, 1.0, 0.0];
                        selected_profile.palette_color_count = 2; selected_profile.recommended_background_mode = -1;
                        filename.clear();
                        events.send(Event::BatchSelection { profile: selected_profile.clone(), palette_ready: false }).map_err(|_| CANCEL_TASK.to_string())?;
                    }
                    events.send(Event::BatchState(result.queue.clone())).map_err(|_| CANCEL_TASK.to_string())?;
                    events.send(Event::BatchLog(std::mem::take(&mut result.log))).map_err(|_| CANCEL_TASK.to_string())?;
                    let was_cancelled = result.status == "cancelled";
                    events.send(Event::BatchResults(result)).map_err(|_| CANCEL_TASK.to_string())?;
                    if was_cancelled { Err("Cancelled".into()) } else { Ok(()) }
                });
                document = runner.into_document();
                result
            }
        };
        // After the command's final Selection, so the GUI records the selected
        // settings that produced this fitting rather than stale controls.
        if let (Ok(()), Some((status, did_work))) = (&result, fitting) {
            if events.send(Event::Fitting { status, did_work }).is_err() { return; }
        }
        let result = if !exported && document.state().original.width != 0 && !filename.as_os_str().is_empty() {
            if source.is_none() || source_changed {
                let original = &document.state().original;
                display = DisplayScene::default();
                source = Some(Bitmap { width: original.width, height: original.height, bgra: original.bgra.clone() }.into_render_image());
            }
            let alpha_type = document.source_alpha_type();
            match presentation(&document, &filename, origin, has_alpha, alpha_type, committed_count, source.as_ref().expect("loaded source"), &mut display, refresh_vectors) {
                Ok(view) => { if events.send(Event::Document(view)).is_err() { return; } result }
                Err(error) => result.and(Err(error)),
            }
        } else {
            if source_may_change && (document.state().original.width == 0 || filename.as_os_str().is_empty()) {
                source = None;
                display = DisplayScene::default();
                if events.send(Event::DocumentCleared).is_err() { return; }
            }
            result
        };
        let result = result.map_err(|error| if error == CANCEL_TASK { "Cancelled".to_string() } else { error });
        if events.send(Event::Finished(result.map(|()| source_recommendations))).is_err() { return; }
    }
}

fn admit_source(document: &mut ImageDocument, bytes: &[u8], name: &Path, settings: &Value, cancelled: &AtomicBool) -> Result<bool, String> {
    openvector::document_controls::read_document_settings(settings).map_err(|error| error.to_string())?;
    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
    let decoded = decode_image(bytes).map_err(|error| error.to_string())?;
    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
    let resolved = resolve_image_filename(&name.to_string_lossy());
    let retained = document.state().original.width != 0;
    document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, resolved.shadow_control as f64)?;
    if retained { document.commit_settings(settings)?; }
    document.clear_committed_palette();
    Ok(decoded.has_alpha)
}

fn input_count(input: &Value, fallback: i32) -> i32 {
    input["count"].as_i64().filter(|count| (2..=24).contains(count)).map_or(fallback, |count| count as i32)
}

struct ProcessSelection {
    settings: openvector::document_controls::DocumentSettings,
    count: i32,
    recommended_background_mode: i32,
    palette_ready: bool,
}

fn process(document: &mut ImageDocument, stage: &str, mut input: Value, committed_count: &mut i32,
    fitting: &mut Option<(u32, bool)>, observer: &mut DocumentObserver<'_>) -> Result<ProcessSelection, String> {
    let mode = input["paletteMode"].as_i64().ok_or("Choose a color mode.")?;
    let advanced = input["advanced"].as_bool().unwrap_or(false);
    let mut selected_count = input_count(&input, *committed_count);
    let mut palette_ready = false;
    // Smoothing operates on the committed segmentation, even when its cache
    // key is stale. Running segmentation here silently discards native edits.
    // Basic mode 0 selects the two-color candidate, not a retained fixed palette.
    // Advanced stage updates keep their explicit palette until palette analysis;
    // reselecting it here would refresh native stage keys and discard edits.
    if stage == "palette" || stage != "vectors" && matches!(mode, 0 | 1)
        && (mode == 0 && !advanced || input["palette"].as_array().is_none_or(Vec::is_empty)) {
        let mut count = if mode == 0 || !advanced { 2 } else { input["count"].as_u64().filter(|count| (2..=24).contains(count)).ok_or("Choose a color count from 2 to 24.")? as usize };
        let upper = if !advanced && mode != 0 { 12 } else { count };
        input["lower"] = json!(count); input["upper"] = json!(upper);
        let result = document.run_task("palette", &input, Some(&mut *observer))?;
        if result.did_work && result.state.status == 2 { return Err(CANCEL_TASK.into()); }
        *committed_count = selected_count;
        if !advanced { for candidate in 3..=upper { if document.state().scores[candidate] < document.state().scores[count] { count = candidate; } } }
        let palette = document.state().palettes.get(count).filter(|palette| !palette.is_empty()).ok_or("Color analysis did not produce the selected palette.")?;
        let bgra: Vec<f32> = palette.chunks_exact(4).flat_map(|color| color.iter().rev().copied()).collect();
        input["palette"] = json!(quantize_qcolor_channels(&bgra));
        let generated = openvector::document_controls::read_document_settings(&input).map_err(|error| error.to_string())?;
        document.remember_generated_palette(generated.palette_mode, &generated.palette);
        if !advanced { selected_count = count as i32; }
        palette_ready = true;
    }
    if stage != "palette" {
        let stages: &[&str] = match stage {
            "segmentation" => &["segmentation"],
            "fitting" => &["segmentation", "smoothing", "fitting"],
            "vectors" => &["smoothing", "fitting"],
            _ => return Err(format!("Unknown processing stage: {stage}")),
        };
        for current in stages {
            let result = document.run_task(current, &input, Some(&mut *observer))?;
            if result.did_work && result.state.status == 2 { return Err(CANCEL_TASK.into()); }
            if result.did_work { *committed_count = selected_count; }
            if *current == "fitting" { *fitting = Some((result.state.status, result.did_work)); }
        }
    }
    let settings = openvector::document_controls::read_document_settings(&input).map_err(|error| error.to_string())?;
    let recommended_background_mode = input["recommendedBackgroundMode"].as_i64().unwrap_or(-1) as i32;
    Ok(ProcessSelection { settings, count: selected_count, recommended_background_mode, palette_ready })
}

/// Worker-owned display snapshot. Geometry is copied only when a completed fitting
/// is admitted, never for a pixel/background gesture. Scene Arcs survive commits;
/// visibility changes rebuild only the display projection, not engine state.
#[derive(Default)]
struct DisplayScene {
    geometry: Option<openvector::contour_fitting::FittedGeometry>,
    current_available: bool,
    hidden: Vec<i64>,
    vectors: Option<Arc<Scene>>,
    removed: Option<Arc<Scene>>,
}

impl DisplayScene {
    fn update(&mut self, document: &ImageDocument, hidden_regions: &[usize], refresh: bool) -> Result<(), String> {
        let state = document.state();
        let fitting = state.fitting.as_ref().filter(|fitting| fitting.complete);
        let replace = fitting.is_some() && (refresh || !self.current_available);
        if replace {
            self.geometry = document.presentation_geometry().map(|geometry| geometry.into_owned());
        }
        self.current_available = fitting.is_some();
        let hidden: Vec<i64> = hidden_regions.iter().map(|&region| region as i64).collect();
        if !replace && hidden == self.hidden { return Ok(()); }
        let Some(geometry) = &self.geometry else { return Ok(()); };
        let mut input = VectorOutputInput::new(geometry, state.original.width as i64, state.original.height as i64);
        input.layout = openvector::export_options::Layout::Cutouts;
        input.hidden_regions = &hidden;
        let vectors = Arc::new(Scene::from_output(&input).map_err(|error| error.to_string())?);
        let removed = if hidden.is_empty() { None } else {
            let visible: Vec<i64> = (0..geometry.regions.len()).map(|region| region as i64).filter(|region| !hidden.contains(region)).collect();
            input.hidden_regions = &visible;
            Some(Arc::new(Scene::from_removed_output(&input).map_err(|error| error.to_string())?))
        };
        self.hidden = hidden;
        self.vectors = Some(vectors);
        self.removed = removed;
        Ok(())
    }
}

fn presentation(document: &ImageDocument, filename: &Path, origin: SourceOrigin, has_alpha: bool, source_alpha_type: u8, committed_count: i32, source: &Arc<gpui::RenderImage>, display: &mut DisplayScene, refresh_vectors: bool) -> Result<DocumentView, String> {
    let state = document.state();
    let mut editor = document.editing_state();
    let background = document.background_state();
    let segments = if let Some(editor) = editor.as_mut() { Some(Bitmap { width: editor.width, height: editor.height, bgra: std::mem::take(&mut editor.bgra) }.into_render_image()) }
        else if let Some(segmentation) = &state.segmentation {
            segmentation.indices.as_ref().map(|indices| {
                let mut bgra = Vec::with_capacity(indices.len() * 4);
                for &region in indices {
                    let color = segmentation.color_indices.get(region as usize).copied().unwrap_or(region) as usize;
                    let channels = segmentation.palette.get(color * 4..color * 4 + 4).ok_or("Segmentation palette does not contain its region color.")?;
                    bgra.extend(channels.iter().map(|channel| (*channel as f64 * 255.0 + 0.5).floor() as u8));
                }
                Ok::<_, String>(Bitmap { width: segmentation.width, height: segmentation.height, bgra }.into_render_image())
            }).transpose()?
        } else { None };
    display.update(document, &background.hidden_regions, refresh_vectors)?;
    let (vectors, removed) = if display.current_available {
        (display.vectors.clone(), display.removed.clone())
    } else { (None, None) };
    Ok(DocumentView { filename: filename.into(), origin, width: state.original.width, height: state.original.height, has_alpha, source_alpha_type,
        stage_keys: state.stage_keys.clone(), editor_key: editor.as_ref().map(|editor| editor.key.clone()),
        committed_count, has_committed_edits: document.has_committed_edits(),
        has_applied_edits: document.has_applied_edits(), editing_changed: document.editing_changed(),
        palettes: state.palettes.clone(),
        settings: state.settings.clone(), source: source.clone(),
        segments, vectors, removed, display_vectors: display.vectors.clone(), display_removed: display.removed.clone(),
        undo: if background.active { !background.undo.is_empty() } else { editor.as_ref().is_some_and(|editor| !editor.undo.is_empty()) },
        redo: if background.active { !background.redo.is_empty() } else { editor.as_ref().is_some_and(|editor| !editor.redo.is_empty()) },
        edit_color: editor.as_ref().map_or([0.0, 0.0, 0.0, 1.0], |editor| editor.color),
        edit_tool: if background.active {
            if background.tool == "background" { 7 } else { 0 }
        } else {
            match editor.as_ref().map(|editor| editor.tool.as_str()) {
                Some("pencil") => 3, Some("picker") => 4, Some("fill") => 5, Some("zap") => 6, _ => 0,
            }
        } })
}

fn export(document: &mut ImageDocument, path: &Path, options: &Value, cancelled: &AtomicBool, events: &mpsc::Sender<Event>) -> Result<(), String> {
    let options = read_export_options(options, ExportOptions::default()).map_err(|error| error.to_string())?;
    let mut environment = DocumentExportEnvironment::default();
    if path.extension().is_some_and(|extension| extension == "emf") {
        environment.reference_device = Some(openvector::emf::EmfDevice::current_display().map_err(|error| error.to_string())?);
    }
    let result = document.save_vector_file_with_environment(path, &options, &environment, Some(&mut |percent| {
        if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
        events.send(Event::Progress { stage: "Exporting".into(), percent: percent as f64 }).map_err(|_| CANCEL_TASK.to_string())
    }))?;
    if let Some(error) = result.error { return Err(format!("Cannot save {}: {error}", path.display())); }
    match result.status { 0 => Ok(()), 2 => Err(CANCEL_TASK.into()), status => Err(format!("Export failed with status {status}.")) }
}

fn bitmap_export(document: &mut ImageDocument, path: &Path, format: &str, scale: f64, flatten: bool, cancelled: &AtomicBool) -> Result<(), String> {
    use std::{fs::OpenOptions, io::Write};
    use openvector::bitmap_export::{BitmapExportInput, BitmapExportOptions};
    let alpha = document.source_alpha_type();
    let background = document.background_state();
    let hidden_regions: Vec<i64> = background.hidden_regions.iter().map(|&region| region as i64).collect();
    let state = document.state();
    let geometry = document.presentation_geometry();
    let input = BitmapExportInput { geometry: geometry.as_deref(),
        width: state.original.width as i64, height: state.original.height as i64, source_alpha_type: alpha,
        background_mode: state.settings.background_mode, background_undo_count: background.undo.len(), hidden_regions: &hidden_regions };
    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
    let bytes = openvector::bitmap_export::export_bitmap(&input, &BitmapExportOptions {
        format: format.parse().map_err(|error: openvector::bitmap_export::BitmapExportError| error.to_string())?, scale, flatten
    }).map_err(|error| error.to_string())?;
    if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut sequence = 0_u64;
    let (temporary, mut file) = loop {
        let temporary = directory.join(format!(".openvector-bitmap-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => sequence += 1,
            Err(error) => return Err(error.to_string()),
        }
    };
    let result = (|| {
        for chunk in bytes.chunks(65536) {
            if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.to_string()); }
            file.write_all(chunk).map_err(|error| error.to_string())?;
        }
        file.sync_all().map_err(|error| error.to_string())?; drop(file);
        if cancelled.load(Ordering::Acquire) { return Err(CANCEL_TASK.into()); }
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() { if let Err(error) = std::fs::remove_file(&temporary) { return Err(format!("{}; cannot remove temporary bitmap: {error}", result.unwrap_err())); } }
    result
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use openvector::palette::CoreSession;

    #[test]
    fn cancelled_source_admission_keeps_the_current_document() {
        let mut document = ImageDocument::new(CoreSession::default());
        let pixels = [20, 40, 60, 255];
        document.set_source(1, 1, &pixels, false, 2.0).unwrap();
        let settings = Value::Object(document.state().settings.to_json());
        let error = admit_source(&mut document, b"not decoded after cancellation", Path::new("clipboard.png"), &settings, &AtomicBool::new(true)).unwrap_err();
        assert_eq!(error, CANCEL_TASK);
        assert_eq!((document.state().original.width, document.state().original.height), (1, 1));
        assert_eq!(document.state().original.bgra, pixels);
        assert_eq!(Value::Object(document.state().settings.to_json()), settings);
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use openvector::{document_controls::DocumentSettings, palette::CoreSession, settings_profile::SettingsProfile};

    #[test]
    fn basic_admission_retry_and_replacement_preserve_native_classification_rng_lifetimes() {
        let rows = common::read_ndjson("fresh-conversion-20261020.ndjson.gz");
        let source = |id: &str| {
            let input = &rows.iter().find(|row| row["id"] == id).unwrap()["input"];
            (common::b64(input["bytes"].as_str().unwrap()), PathBuf::from(input["name"].as_str().unwrap()))
        };
        let (logo, logo_name) = source("fresh-conversion-20261020-00-logo-5x7");
        let (ring, ring_name) = source("fresh-conversion-20261020-10-ring-41x37");
        for retry in [false, true] {
            let mut document = ImageDocument::new(CoreSession::default());
            let mut controls = Value::Object(DocumentSettings::default().to_json());
            admit_source(&mut document, &logo, &logo_name, &controls, &AtomicBool::new(false)).unwrap();
            controls["extension"] = json!(".png");
            let classified = document.run_task("classification", &controls, None).unwrap();
            assert_eq!([classified.state.classification.category, classified.state.classification.quality,
                classified.state.classification.palette_mode, classified.state.classification.color_count], [1, 0, 1, 3]);
            assert_eq!(classified.state.random_state, 6863);
            if retry {
                // Native joined-worker cancellation retains the computed result;
                // a fresh Basic request must classify again, not reuse that cache.
                document.request_cancel();
                let cancelled = document.snapshot();
                assert_eq!([cancelled.classification.category, cancelled.classification.quality,
                    cancelled.classification.palette_mode, cancelled.classification.color_count], [1, 0, 1, 3]);
                assert_eq!(cancelled.random_state, 6863);
                let repeated = document.run_task("classification", &controls, None).unwrap();
                assert!(repeated.did_work);
                assert_eq!([repeated.state.classification.category, repeated.state.classification.quality,
                    repeated.state.classification.palette_mode, repeated.state.classification.color_count], [1, 0, 1, 3]);
                assert_eq!(repeated.state.random_state, 8263);
            } else {
                controls["category"] = json!(0); controls["quality"] = json!(2); controls["paletteMode"] = json!(2);
                admit_source(&mut document, &ring, &ring_name, &controls, &AtomicBool::new(false)).unwrap();
                assert_eq!(document.snapshot().classification.category, -1);
                assert_eq!(document.snapshot().random_state, 6863);
                assert_eq!([document.state().settings.category, document.state().settings.quality,
                    document.state().settings.palette_mode], [0, 2, 2]);
                let replaced = document.run_task("classification", &controls, None).unwrap();
                assert_eq!([replaced.state.classification.category, replaced.state.classification.quality,
                    replaced.state.classification.palette_mode, replaced.state.classification.color_count], [2, 0, 0, 2]);
                assert_eq!(replaced.state.random_state, 1334);
            }
        }
    }

    /// Sends one command to the worker loop and returns the Selection and
    /// Fitting events it published, in order, with its result.
    fn outcome(commands: &mpsc::Sender<Command>, events: &mpsc::Receiver<Event>, command: Command) -> (Vec<String>, Result<(), String>) {
        commands.send(command).unwrap();
        let mut published = Vec::new();
        loop {
            match events.recv_timeout(Duration::from_secs(120)).unwrap() {
                Event::Selection { .. } => published.push("selection".to_string()),
                Event::Fitting { status, did_work } => published.push(format!("fitting {status} {did_work}")),
                Event::Finished(result) => return (published, result.map(|_| ())),
                _ => {},
            }
        }
    }

    /// Worker::submit clears the cancellation flag, so a GUI Cancel races the
    /// running command. Holding the loop's own flag raises it before a Recent
    /// Basic replay starts: the replay stops at its first progress check, in
    /// segmentation, and publishes neither its selection nor a fitting. The same
    /// replay without the flag fits with work, which the GUI would record under
    /// its label, different from the fitting before it.
    #[test]
    fn cancelled_recent_basic_replay_publishes_no_fitting() {
        let (commands, incoming) = mpsc::channel();
        let (outgoing, events) = mpsc::channel();
        let (_decisions, decision_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || run(ImageDocument::new(CoreSession::default()), 2, incoming, outgoing, flag, decision_rx));
        let directory = std::env::temp_dir().join(format!("openvector-worker-cancellation-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.png");
        image::RgbImage::from_fn(16, 16, |x, y| image::Rgb(if (4..12).contains(&x) && (4..12).contains(&y) { [0; 3] } else { [255; 3] })).save(&source).unwrap();
        let artwork = |quality| DocumentSettings { category: 2, quality, palette_mode: 2, background_mode: 0, ..DocumentSettings::default() };
        let fitted = || -> (Vec<String>, Result<(), String>) { (vec!["selection".to_string(), "fitting 0 true".to_string()], Ok(())) };
        assert_eq!(outcome(&commands, &events, Command::Open { path: source, settings: Value::Object(artwork(0).to_json()) }).1, Ok(()));
        // "Artwork, High, Unlimited colors".
        assert_eq!(outcome(&commands, &events, Command::Process { stage: "fitting", input: Value::Object(artwork(0).to_json()) }), fitted());
        // "Artwork, Medium, Unlimited colors": unlimited colors skips the palette search.
        let replay = || Command::Profile { profile: SettingsProfile { settings: artwork(1), color_count: 2, basic_color_count: 0, palette_color_count: 0, recommended_background_mode: 0 }, basic: true };
        cancelled.store(true, Ordering::Release);
        assert_eq!(outcome(&commands, &events, replay()), (Vec::new(), Err("Cancelled".to_string())));
        cancelled.store(false, Ordering::Release);
        assert_eq!(outcome(&commands, &events, replay()), fitted());
        drop(commands);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cancelled_source_preparation_keeps_original_and_can_retry() {
        let (commands, incoming) = mpsc::channel();
        let (outgoing, events) = mpsc::channel();
        let (_decisions, decision_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let worker = std::thread::spawn(move || run(ImageDocument::new(CoreSession::default()), 2, incoming, outgoing, flag, decision_rx));
        let directory = std::env::temp_dir().join(format!("openvector-sizing-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.png");
        image::RgbImage::from_pixel(1603, 1003, image::Rgb([17, 71, 143])).save(&source).unwrap();
        assert_eq!(outcome(&commands, &events, Command::Open { path: source, settings: Value::Object(DocumentSettings::default().to_json()) }).1, Ok(()));
        for cancel in [true, false] {
            cancelled.store(cancel, Ordering::Release);
            commands.send(Command::PrepareSource { shrink: true }).unwrap();
            let mut admitted = false;
            let mut dimensions = None;
            loop {
                match events.recv_timeout(Duration::from_secs(120)).unwrap() {
                    Event::SourcePrepared => admitted = true,
                    Event::Document(view) => dimensions = Some((view.width, view.height)),
                    Event::Finished(result) => {
                        assert_eq!(result, if cancel { Err("Cancelled".into()) } else { Ok(None) });
                        break;
                    },
                    _ => {},
                }
            }
            assert_eq!(admitted, !cancel);
            assert_eq!(dimensions, Some(if cancel { (1603, 1003) } else { (802, 502) }));
        }
        drop(commands); worker.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
