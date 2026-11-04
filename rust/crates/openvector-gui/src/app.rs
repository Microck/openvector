use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::{Duration, Instant}};
use gpui::{prelude::*, App, Bounds, Context, Div, ExternalPaths, FocusHandle, Focusable, KeyDownEvent, KeyUpEvent, MouseButton, PathPromptOptions, RenderImage, Role, Stateful, Window, canvas, div, img, px, rgb};
use openvector::{batch_queue::{BatchQueue, BatchSnapshot, OverwriteDecision}, document_controls::DocumentSettings};
use serde_json::{Value, json};
use crate::{controls::{button, icon_button, unfilled_button, primary_button, group, segmented, hint, row, section}, jobs::{Bitmap, Command, DocumentView, Event, SourceOrigin, Worker}, preferences::Preferences, theme, viewer::{geometry::{Axis, Point, Size, ViewerView}, input::{self, LayerAvailability, ViewerInput, ViewerKey, KeyPhase, tool}, zoom}, wizard::{Page, Wizard, AdvancedControls, AutoUpdate, page_controls_changed, page_needs_update}};
#[path = "app_workflows.rs"]
mod workflows;
use workflows::{Form, WorkflowAction, Workflows};
#[path = "app-drag-result.rs"]
mod drag_result;

#[derive(Clone)]
enum Action {
    Open, PasteImage, ConfirmPaste(Arc<Vec<u8>>), Batch, BatchAdd, BatchRemove(usize), BatchStart, BatchFolder, BatchAutoColors,
    Next, Back, Start, Automatic, Basic, Advanced,
    SourceSizing(bool),
    Set(&'static str, Value), Number(&'static str, i32), Toggle(&'static str),
    BackgroundChannel(usize, i32),
    Process(&'static str), Tool(i32), Edit(&'static str), Background, EditResult, ImproveResult,
    Layer(i32), Zoom(i64), Fit, One, Step(Axis, f64), Paths,
    Export, Format(&'static str), Layout(&'static str), CurveMode(&'static str), ThinStroke, CompactEps,
    DragFormat(&'static str), DragMinimize, DragRetry,
    SkipReview, Auto(&'static str), Recent(PathBuf), About, Dismiss,
    BatchDecision(OverwriteDecision), Cancel,
    Workflow(WorkflowAction),
}
#[derive(Clone)]
enum Dialog { About, PasteImage(Arc<Vec<u8>>), BatchOverwrite(Vec<String>), Form(Form) }
/// Export shape layouts: stored preference value, button label, hint for the
/// selected choice. Values are the CLI/API names and stay unchanged.
const SHAPE_LAYOUTS: [(&str, &str, &str); 3] = [
    ("stacked", "Overlapping shapes", "Each color is one solid shape stacked on the ones below. Easiest to edit."),
    ("cutouts", "Cut-out shapes", "Shapes have holes where other colors sit, so nothing overlaps. Best for cutters."),
    ("grouped-cutouts", "Cut-out, grouped by color", "No overlaps, and each color is grouped into its own layer."),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkKind { Gesture, Processing }
impl WorkKind {
    fn of(command: &Command) -> Self {
        match command {
            Command::Edit { action: "gesture", .. } | Command::Background { action: "gesture", .. } => Self::Gesture,
            _ => Self::Processing,
        }
    }
}

#[derive(Clone, Copy)]
struct ScrollbarDrag {
    axis: Axis,
    position: f64,
    value: f64,
    units_per_pixel: f64,
}

#[derive(Clone, Copy)]
struct ScrollbarPage {
    axis: Axis,
    position: f64,
    forward: bool,
}

enum ScrollbarInteraction {
    Drag(ScrollbarDrag),
    Page { page: ScrollbarPage, _repeat: gpui::Task<()> },
}

// These timings fit the retained native input measurements; their sampling
// bounds do not establish exact native timer constants.
const SCROLLBAR_REPEAT_DELAY: Duration = Duration::from_millis(500);
const SCROLLBAR_REPEAT_INTERVAL: Duration = Duration::from_millis(50);

/// How long a worker command may run before the UI shows it as busy.
const BUSY_GRACE: Duration = Duration::from_millis(250);

pub struct OpenVectorApp {
    /// When the current busy period began, and the status shown before it.
    busy_since: Option<(Instant, String)>,
    pub wizard: Wizard,
    pub settings: DocumentSettings,
    pub document: Option<DocumentView>,
    pub view: ViewerView,
    pub input: ViewerInput,
    pub status: String,
    pub error: bool,
    pub busy: bool,
    pub batch: Option<BatchSnapshot>,
    pub batch_log: Vec<String>,
    preferences: Preferences,
    workflows: Workflows,
    drag_result: drag_result::DragResult,
    worker: Option<Worker>,
    pending: VecDeque<(Command, Option<Page>)>,
    // Avoid scanning the gesture backlog on each incoming pointer sample.
    pending_processing: usize,
    in_flight: Option<WorkKind>,
    destination: Option<Page>,
    automatic: AutoUpdate,
    automatic_task: Option<(u64, gpui::Task<()>)>,
    /// Source layer, converted once per loaded source. The segmentation layer
    /// is owned by `document`.
    source_image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
    dialog_focus: FocusHandle,
    restore_focus: Option<FocusHandle>,
    dialog: Option<Dialog>,
    viewport: Bounds<gpui::Pixels>,
    zoom_bounds: Bounds<gpui::Pixels>,
    zoom_dragging: bool,
    dragging: Option<Point>,
    scrollbar_interaction: Option<ScrollbarInteraction>,
    pointer_buttons: u32,
    pub show_paths: bool,
    pub color_count: i32,
    progress: f64,
    batch_queue: BatchQueue,
    loading_source: bool,
    source_sizing: Option<bool>,
    /// Native consumption is independent of the visible sizing offer/choice.
    source_sizing_consumed: bool,
    /// Acknowledged UI recommendations, not the engine's possibly cancelled result.
    source_recommendations: Option<[i32; 3]>,
    /// Native Basic/Advanced entry guards. Saved profile Play can set both.
    transparency_initialized: [bool; 2],
    source_directory: PathBuf,
    confirmation_cancel: FocusHandle,
    confirmation_message: Option<String>,
    smoke: Option<PathBuf>,
    smoke_exported: bool,
    _poll: gpui::Task<()>,
    activation: Option<gpui::Subscription>,
}

impl OpenVectorApp {
    pub fn new(cx: &mut Context<Self>) -> Result<Self, String> {
        Self::with_preferences(Preferences::default_path(), cx)
    }
    pub fn with_preferences(path: PathBuf, cx: &mut Context<Self>) -> Result<Self, String> {
        let preferences = Preferences::load(path)?;
        let workflows = Workflows::new(&preferences, cx)?;
        let (worker, initial_error) = match Worker::start(preferences.selected.clone()) {
            Ok(worker) => (Some(worker), None), Err(error) => (None, Some(error)),
        };
        let wizard = Wizard { skip_review: preferences.skip_review, auto_palette: preferences.auto_palette,
            auto_segmentation: preferences.auto_segmentation, auto_vectorization: preferences.auto_vectorization,
            ..Wizard::default() };
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let directory = cwd.to_string_lossy();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(30)).await;
                if this.update(cx, |app, cx| app.poll(cx)).is_err() { break; }
            }
        });
        let selected = preferences.selected.clone();
        let mut batch_queue = BatchQueue::new(&directory, &directory);
        batch_queue.export_directory = preferences.batch_directory.to_string_lossy().into_owned();
        let source_directory = preferences.source_directory.clone();
        let app = Self { wizard, settings: selected.settings, document: None, view: ViewerView::default(),
            input: ViewerInput::default(), status: initial_error.clone().unwrap_or_else(|| "Ready".into()),
            error: initial_error.is_some(), busy: false, batch: None, batch_log: Vec::new(),
            preferences, workflows, worker, pending: VecDeque::new(), pending_processing: 0, in_flight: None, destination: None, busy_since: None,
            automatic: AutoUpdate::default(), automatic_task: None,
            drag_result: drag_result::DragResult::default(),
            source_image: None, focus: cx.focus_handle(), dialog_focus: cx.focus_handle(),
            confirmation_cancel: cx.focus_handle().tab_stop(true), confirmation_message: None,
            restore_focus: None, dialog: None, viewport: Bounds::default(), dragging: None, pointer_buttons: 0,
            zoom_bounds: Bounds::default(), zoom_dragging: false,
            scrollbar_interaction: None,
            show_paths: false, color_count: selected.color_count, progress: 0.0,
            batch_queue, loading_source: false, source_sizing: None, source_sizing_consumed: false, source_recommendations: None, transparency_initialized: [false; 2], source_directory, smoke: None, smoke_exported: false, _poll: poll, activation: None };
        Ok(app)
    }
    pub fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.busy || self.dialog.is_some() { return; }
        let path = match std::path::absolute(path) { Ok(path) => path, Err(error) => { self.fail(format!("Cannot resolve the source path: {error}"), cx); return; } };
        if self.has_manual_edits() {
            self.dialog = Some(Dialog::Form(Form::Confirm(
                "Opening another image will discard your palette, segmentation and background edits. Cancel to keep the current image.".into(),
                WorkflowAction::OpenAccepted(path))));
            cx.notify();
        } else { self.open_accepted(path, cx); }
    }
    fn has_manual_edits(&self) -> bool {
        (if self.batch.is_some() { self.workflows.inactive_palette.customized } else { self.workflows.palette.customized }) || self.document.as_ref().is_some_and(|document|
            document.has_applied_edits || document.removed.is_some()
            || document.editing_changed)
    }
    fn open_accepted(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.enqueue(Command::Open { path, settings: self.task_input() }, Some(Page::Mode), cx);
    }
    fn paste_image(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.dialog.is_some() { return; }
        if self.batch.is_some() { self.fail("Return to Single image to paste an image.", cx); return; }
        let image = cx.read_from_clipboard().and_then(|item| item.into_entries().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => Some(image),
            _ => None,
        }));
        let Some(image) = image else {
            self.fail("No image on the clipboard. Copy an image, then choose Paste image.", cx); return;
        };
        if image.bytes.is_empty() {
            self.fail("The clipboard image is empty. Copy the image again, then choose Paste image.", cx); return;
        }
        let bytes = Arc::new(image.bytes);
        if self.has_manual_edits() {
            self.dialog = Some(Dialog::PasteImage(bytes)); cx.notify();
        } else {
            self.import_clipboard(bytes, cx);
        }
    }
    fn import_clipboard(&mut self, bytes: Arc<Vec<u8>>, cx: &mut Context<Self>) {
        if self.busy || self.batch.is_some() || self.dialog.is_some() { return; }
        self.enqueue(Command::PasteImage { bytes, directory: self.source_directory.clone(), settings: self.task_input() }, Some(Page::Mode), cx);
    }
    pub fn start_smoke(&mut self, input: PathBuf, output: PathBuf, cx: &mut Context<Self>) {
        self.smoke = Some(output);
        self.open(input, cx);
    }
    fn fail(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = true; self.status = message.into(); cx.notify();
    }
    fn persist(&mut self, cx: &mut Context<Self>) {
        self.retain_processing_preferences();
        if let Err(error) = self.preferences.save() { self.fail(error, cx); }
    }
    /// All Mode exits prepare the source before their desired command/page.
    /// Keep the pending gate until the worker acknowledges preparation.
    fn leave_mode(&mut self, command: Option<Command>, destination: Page, cx: &mut Context<Self>) {
        if self.wizard.page == Page::Mode && self.batch.is_none() && !self.source_sizing_consumed {
            if let Some(shrink) = self.source_sizing {
                self.enqueue(Command::PrepareSource { shrink }, command.is_none().then_some(destination), cx);
                if command.is_none() || !self.busy { return; }
            } else {
                // No pending choice or raster work needs worker acknowledgment.
                self.source_sizing_consumed = true;
            }
        }
        if let Some(command) = command { self.enqueue(command, Some(destination), cx); }
        else { self.wizard.page = destination; self.page_changed(cx); }
    }
    fn enqueue(&mut self, command: Command, destination: Option<Page>, cx: &mut Context<Self>) {
        self.automatic_task = None;
        if matches!(&command, Command::Open { .. } | Command::PasteImage { .. } | Command::Reset(_) | Command::Batch { .. }) {
            self.automatic.reset();
            self.scrollbar_interaction = None;
        } else { self.automatic.suspend(); }
        self.pending_processing += usize::from(WorkKind::of(&command) == WorkKind::Processing);
        self.pending.push_back((command, destination));
        self.start_pending(cx);
    }
    fn start_pending(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.dialog.is_some() { return; }
        let Some((command, destination)) = self.pending.pop_front() else { return; };
        let kind = WorkKind::of(&command);
        self.pending_processing -= usize::from(kind == WorkKind::Processing);
        let Some(worker) = self.worker.as_ref() else { self.fail("The image worker is unavailable.", cx); return; };
        self.loading_source = matches!(&command, Command::Open { .. } | Command::PasteImage { .. });
        match worker.submit(command) {
            Ok(()) => {
                if !self.busy { self.busy_since = Some((Instant::now(), self.status.clone())); }
                self.in_flight = Some(kind); self.busy = true; self.error = false; self.progress = 0.0; self.status = "Processing".into(); self.destination = destination;
                // Repaint once the grace period ends, in case no worker event arrives first.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(BUSY_GRACE).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                }).detach();
            }
            Err(error) => self.fail(error, cx),
        }
        cx.notify();
    }
    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_processing = 0;
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        loop {
            let event = self.worker.as_ref().map(|worker| worker.events.try_recv());
            let event = match event {
                Some(Ok(event)) => event,
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    self.worker = None; self.busy = false; self.in_flight = None; self.clear_pending();
                    self.fail("The image worker stopped unexpectedly. Restart OpenVector.", cx); break;
                }
                _ => break,
            };
            let cancelled = self.worker.as_ref().is_some_and(Worker::cancellation_requested);
            match event {
                // Native taskFinished checks the UI cancellation latch before
                // dispatching completion, even if the worker already returned.
                Event::Progress { .. } | Event::Selection { .. } | Event::Fitting { .. } if cancelled => continue,
                Event::Progress { stage, percent } => { self.status = stage; self.progress = percent; }
                Event::SourcePrepared => self.source_sizing_consumed = true,
                Event::DocumentCleared => {
                    self.close_document();
                    self.view = ViewerView::default().resized(self.view.area);
                    self.automatic.reset(); self.automatic_task = None;
                    self.workflows.eyedropper = None;
                },
                Event::Selection { settings, color_count, recommended_background_mode, palette_ready, classified } => {
                    if classified {
                        self.source_recommendations = Some([settings.category, settings.quality, settings.palette_mode]);
                        self.transparency_initialized = [true, false];
                    }
                    self.settings = settings;
                    if let Some(count) = color_count { self.color_count = count as i32; }
                    self.workflows.recommended_background_mode = recommended_background_mode;
                    if palette_ready {
                        self.workflows.palette.backup = self.settings.palette.clone(); self.workflows.palette.customized = false;
                        self.workflows.palette_count = (self.settings.palette.len() / 4) as f64;
                    }
                    self.persist(cx);
                },
                // Native addMruSettings after a completed fitting: record the
                // selected settings the preceding Selection adopted. The shared
                // list decides eligibility, order and duplicates.
                Event::Fitting { status, did_work } => {
                    if self.preferences.recent_basic.record_completion(&self.current_profile(), status, did_work, self.batch.is_some()) { self.persist(cx); }
                },
                Event::ReplayUnprocessed => self.destination = None,
                Event::BatchSelection { profile, palette_ready } => {
                    self.workflows.inactive_palette.customized = false;
                    if palette_ready { self.workflows.inactive_palette.backup = profile.settings.palette.clone(); }
                    self.preferences.selected = profile; self.persist(cx);
                },
                Event::Document(document) => {
                    let new_source = self.document.as_ref().is_none_or(|previous| !Arc::ptr_eq(&previous.source, &document.source));
                    let retained_source = self.document.is_some();
                    if new_source {
                        self.drag_result.invalidate();
                        if self.batch.is_some() { self.source_sizing = None; self.source_sizing_consumed = false; self.source_recommendations = None; self.transparency_initialized = [false; 2]; self.preferences.bitmap_save.batch_source_changed(&document.filename); }
                        if self.loading_source && document.origin == SourceOrigin::Path {
                            if let Some(directory) = document.filename.parent() { self.source_directory = directory.into(); }
                        }
                        self.workflows.released_images.extend(self.source_image.replace(document.source.clone()));
                        self.view.image = Size::new(document.width as f64, document.height as f64);
                        if self.batch.is_none() {
                            let recommended = openvector::image_resize::source_shrink_dimensions(document.width, document.height);
                            self.source_sizing = (recommended != [document.width, document.height]).then_some(true);
                            if let Err(error) = self.workflows.bitmap.source_changed(document.width, document.height) { self.fail(error, cx); }
                        }
                        if self.batch.is_none() && self.loading_source {
                            // Native load resets the sizing gate; a load started on Mode also
                            // passes the Source-exit boundary, which consumes the new gate.
                            // Apply both only once the worker admits the replacement, so a
                            // failed or cancelled load leaves the retained document and its gate
                            // untouched, and the soon-discarded old raster is never prepared.
                            // The page is still the origin page: destinations apply on Finished.
                            self.source_sizing_consumed = self.wizard.page == Page::Mode;
                            self.source_recommendations = None;
                            self.transparency_initialized = [false; 2];
                            self.preferences.quick_save.source_changed(&document.filename);
                            let bitmap_name = document.filename.with_file_name(format!("{}_vectorized", document.filename.file_stem().unwrap_or_default().to_string_lossy()));
                            self.preferences.bitmap_save.source_changed(&bitmap_name);
                            self.workflows.palette = Default::default();
                            if retained_source {
                                self.settings.palette.clear(); self.settings.background_mode = -1; self.settings.background = [1.0, 1.0, 1.0, 0.0];
                                self.workflows.palette_count = 2.0; self.workflows.recommended_background_mode = -1;
                                // Native loadImage (0x431330) and clipboard import (0x4363a0)
                                // fully reset a loaded model, whose mainModelReset suggests a
                                // settings name. A failed GPUI admission keeps the old
                                // document, so only the successful replacement suggests.
                                self.workflows.suggest_profile_name(cx);
                            }
                            if document.origin == SourceOrigin::Path {
                                if let Err(error) = self.preferences.remember(
                                    &document.filename, document.width, document.height,
                                    document.source.as_bytes(0).expect("source image has one frame"),
                                    &mut self.workflows.released_images,
                                ) { self.fail(error, cx); }
                            }
                            self.persist(cx);
                        }
                        self.fit();
                    }
                    if !self.input.snapshot().shift_held && matches!(self.wizard.page, Page::Editing | Page::Background) {
                        self.input.set_tool(document.edit_tool);
                    }
                    // Each document carries a fresh segmentation image; release the one it replaces.
                    if let Some(previous) = self.document.replace(document) { self.workflows.released_images.extend(previous.segments); }
                }
                Event::BatchState(state) => self.batch = Some(state),
                Event::BatchPreview { filename, preview } => self.workflows.set_preview(filename.into(), Ok(preview.into_render_image())),
                Event::BatchResults(result) => self.workflows.finish_batch(result),
                Event::BatchLog(line) => self.batch_log.push(line),
                Event::Overwrite(files) => self.dialog = Some(Dialog::BatchOverwrite(files)),
                Event::DragPrepared { request, result } => self.drag_prepared(request, result),
                Event::Finished(result) => {
                    let result = if cancelled { Err("Cancelled".into()) } else { result };
                    self.loading_source = false;
                    self.busy = false;
                    self.in_flight = None;
                    if let Some(batch) = self.batch.as_mut() { self.batch_queue.set_accepting_edits(true); batch.accepting_edits = true; }
                    match result {
                        Ok(recommendations) => {
                            if let Some(recommendations) = recommendations {
                                [self.settings.category, self.settings.quality, self.settings.palette_mode] = recommendations;
                                self.source_recommendations = Some(recommendations);
                                self.persist(cx);
                            }
                            if !self.error { self.status = "Ready".into(); }
                            if let Some(page) = self.destination.take() { self.wizard.page = page; self.page_changed(cx); }
                            self.sync_automatic(false, cx);
                            if let Some(output) = self.smoke.clone() {
                                if self.document.as_ref().is_some_and(|doc| doc.vectors.is_some()) {
                                    if self.smoke_exported { cx.quit(); }
                                    else { self.smoke_exported = true; self.enqueue(Command::Export { path: output, options: self.export_options() }, None, cx); }
                                } else { self.enqueue(Command::Automatic(self.task_input()), Some(Page::Review), cx); }
                            }
                        }
                        Err(error) => {
                            self.destination = None; self.clear_pending();
                            self.sync_automatic(false, cx);
                            self.automatic.cancel(); self.automatic_task = None;
                            self.fail(error, cx);
                            if self.smoke.is_some() { eprintln!("{}", self.status); std::process::exit(1); }
                        }
                    }
                    self.start_pending(cx);
                    self.prepare_drag(cx);
                }
            }
            cx.notify();
        }
    }
    fn task_input(&self) -> Value {
        let mut value = Value::Object(self.settings.to_json());
        value["lower"] = json!(self.color_count); value["upper"] = json!(self.color_count);
        value["count"] = json!(self.color_count);
        if self.workflows.palette.customized { value["paletteEncoding"] = json!("straight"); }
        value["recommendedBackgroundMode"] = json!(self.workflows.recommended_background_mode);
        value["extension"] = json!(self.document.as_ref().and_then(|doc| doc.filename.extension()).and_then(|ext| ext.to_str()).unwrap_or(""));
        value
    }
    fn export_options(&self) -> Value {
        json!({"layout":self.preferences.layout,"curveMode":self.preferences.curve_mode,"thinStroke":self.preferences.thin_stroke,"compact":self.preferences.compact_eps})
    }
    fn fit(&mut self) {
        self.scrollbar_interaction = None;
        if let Some(view) = zoom::fit(&self.view, self.view.image, self.view.area) { self.view = view; }
    }
    /// Drops the document and queues its viewer images for atlas release.
    fn close_document(&mut self) {
        self.source_sizing = None;
        self.source_sizing_consumed = false;
        self.source_recommendations = None;
        self.transparency_initialized = [false; 2];
        self.scrollbar_interaction = None;
        self.drag_result.invalidate();
        self.workflows.released_images.extend(self.source_image.take());
        self.workflows.released_images.extend(self.document.take().and_then(|document| document.segments));
    }
    fn page_changed(&mut self, cx: &mut Context<Self>) {
        self.wizard.record();
        // Reset can retain this page after closing the source; that is not a new entry.
        if self.wizard.page == Page::Transparency && self.document.is_some() {
            let mode = usize::from(self.settings.advanced);
            if !self.transparency_initialized[mode] {
                self.transparency_initialized[mode] = true;
                self.transparency_initialized[1 - mode] = false;
                let flatten = !self.settings.advanced && self.settings.category == 1
                    && self.document.as_ref().is_some_and(|document| document.source_alpha_type == 1);
                self.settings.background_mode = i32::from(flatten);
                self.settings.background = [1.0, 1.0, 1.0, i32::from(flatten) as f32];
                self.workflows.recommended_background_mode = if self.settings.advanced { -1 } else { i32::from(flatten) };
                self.persist(cx);
            }
        }
        let layer = match self.wizard.page { Page::Segmentation | Page::Editing => 1,
            Page::Smoothing | Page::Background | Page::Review | Page::Export => 2,
            // Basic palette choices segment immediately, so show their regions.
            Page::Palette if !self.settings.advanced && self.document.as_ref().is_some_and(|doc| doc.segments.is_some()) => 1,
            _ => 0 };
        self.set_layer(layer);
        self.input.set_tool(if matches!(self.wizard.page, Page::Editing | Page::Background) {
            self.document.as_ref().map_or(tool::PAN, |document| document.edit_tool)
        } else { tool::PAN });
    }
    fn set_layer(&mut self, layer: i32) {
        let segments = self.document.as_ref().is_some_and(|doc| doc.segments.is_some());
        let vectors = self.document.as_ref().is_some_and(|doc| doc.display_vectors.is_some());
        self.input.set_layer(layer, segments, vectors);
    }
    fn process(&mut self, stage: &'static str, destination: Page, cx: &mut Context<Self>) {
        self.automatic.consume_current(); self.automatic_task = None;
        let stage = if stage == "fitting" && self.settings.advanced && self.wizard.page == Page::Smoothing { "vectors" } else { stage };
        let input = self.task_input();
        let warning = if stage == "palette" && self.workflows.palette.customized {
            Some("Recalculating the palette will replace your customized colors.")
        } else if self.document.as_ref().is_some_and(|document| stage == "segmentation" && document.undo
            || stage == "fitting" && document.has_applied_edits && document.stage_keys.segmentation != openvector::document_controls::document_segmentation_key(&self.settings)) {
            Some("Recalculating the segmentation will discard your applied edits.")
        } else { None };
        if let Some(warning) = warning {
            self.dialog = Some(Dialog::Form(workflows::Form::Confirm(warning.into(), WorkflowAction::RunProcess(stage, destination, input))));
        } else {
            if stage == "palette" { self.workflows.palette.customized = false; self.workflows.palette.backup = self.settings.palette.clone(); }
            self.enqueue(Command::Process { stage, input }, Some(destination), cx);
        }
    }
    fn next(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.document.is_none() { return; }
        if self.dirty_page() {
            if let Some(stage) = self.wizard.page.update_stage() { self.process(stage, self.wizard.page, cx); }
            return;
        }
        use Page::*;
        match self.wizard.page {
            Load => self.wizard.page = Mode,
            ImageType => self.wizard.page = if self.document.as_ref().is_some_and(|doc| doc.source_alpha_type != 0) { Transparency } else { Quality },
            Transparency => self.wizard.page = if self.settings.advanced { Palette } else { Quality },
            Quality => self.wizard.page = Colors,
            Colors if self.settings.palette_mode == 1 => self.process("palette", Palette, cx),
            Colors | Palette if !self.settings.advanced => self.process("fitting", self.wizard.completed_page(&self.settings), cx),
            Palette => self.enqueue(Command::Commit(self.task_input()), Some(Segmentation), cx),
            Segmentation => {
                self.wizard.edit_origin = Segmentation;
                self.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Editing), cx);
            }
            Editing => self.enqueue(Command::Edit { action: if self.settings.advanced { "apply-editing" } else { "finish" }, input: self.task_input() },
                Some(if self.settings.advanced { Smoothing } else { self.wizard.completed_page(&self.settings) }), cx),
            Smoothing => self.enqueue(Command::Background { action: "begin", input: Value::Null }, Some(Background), cx),
            Background => self.enqueue(Command::Background { action: "finish", input: Value::Null }, Some(if self.settings.advanced { Export } else { Review }), cx),
            Review => self.wizard.page = Export,
            _ => {},
        }
        self.sync_automatic(false, cx);
        self.page_changed(cx); cx.notify();
    }
    fn back(&mut self, cx: &mut Context<Self>) {
        if self.busy { return; }
        if self.can_revert() {
            let document = self.document.as_ref().expect("dirty document");
            AdvancedControls::new(&document.settings, document.committed_count).restore(&mut self.settings, &mut self.color_count);
            self.automatic.consume_current(); self.automatic_task = None;
            self.persist(cx);
            self.enqueue(Command::Edit { action: "restore", input: Value::Null }, None, cx);
            self.sync_automatic(true, cx);
            return;
        }
        let destination = self.wizard.back_page(&self.settings, self.document.as_ref().is_some_and(|doc| doc.source_alpha_type != 0));
        match self.wizard.page {
            Page::Mode => self.leave_mode(None, destination, cx),
            Page::Background => self.enqueue(Command::Background { action: "finish", input: Value::Null }, Some(destination), cx),
            Page::Editing => self.enqueue(Command::Edit { action: "leave", input: Value::Null }, Some(destination), cx),
            Page::Palette => self.enqueue(Command::Commit(self.task_input()), Some(destination), cx),
            _ if destination == Page::Background => self.enqueue(Command::Background { action: "begin", input: Value::Null }, Some(destination), cx),
            _ if destination == Page::Editing => self.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(destination), cx),
            _ => { self.wizard.page = destination; self.page_changed(cx); },
        }
        self.sync_automatic(false, cx);
    }
    fn dirty_page(&self) -> bool {
        self.settings.advanced && self.wizard.page.update_stage().is_some() &&
            self.document.as_ref().is_some_and(|document|
                page_controls_changed(self.wizard.page, &self.settings, self.color_count,
                    &document.settings, document.committed_count)
                || page_needs_update(self.wizard.page, &self.settings, &document.stage_keys))
    }
    fn can_revert(&self) -> bool {
        self.changed_editing() || self.document.as_ref().is_some_and(|document|
            page_controls_changed(self.wizard.page, &self.settings, self.color_count,
                &document.settings, document.committed_count))
    }
    fn changed_editing(&self) -> bool {
        self.wizard.page == Page::Editing && self.document.as_ref().is_some_and(|document|
            document.editing_changed)
    }
    fn sync_automatic(&mut self, changed: bool, cx: &mut Context<Self>) {
        let active = self.settings.advanced && self.batch.is_none() && self.document.is_some() && !self.loading_source;
        if active && self.wizard.page == Page::Palette && self.settings.palette_mode == 0 {
            self.settings.palette_mode = 1; self.persist(cx);
        }
        let page = active.then_some(self.wizard.page).filter(|page| page.update_stage().is_some());
        let checked = match page { Some(Page::Palette) => self.wizard.auto_palette,
            Some(Page::Segmentation) => self.wizard.auto_segmentation, Some(Page::Smoothing) => self.wizard.auto_vectorization, _ => false };
        let stale = page != self.automatic.page() && page.is_some_and(|page|
            self.document.as_ref().is_some_and(|document| page_needs_update(page, &self.settings, &document.stage_keys)));
        self.automatic.synchronize(page, checked, stale, changed, Instant::now());
        if self.busy || !self.pending.is_empty() || self.dialog.is_some() {
            self.automatic.suspend(); self.automatic_task = None; return;
        }
        let Some((ticket, deadline)) = self.automatic.scheduled() else { self.automatic_task = None; return; };
        if self.automatic_task.as_ref().is_some_and(|(current, _)| *current == ticket) { return; }
        self.automatic_task = Some((ticket, cx.spawn(async move |this, cx| {
            cx.background_executor().timer(deadline.saturating_duration_since(Instant::now())).await;
            let _ = this.update(cx, |app, cx| app.automatic_due(ticket, Instant::now(), cx));
        })));
    }
    fn automatic_due(&mut self, ticket: u64, now: Instant, cx: &mut Context<Self>) {
        if self.automatic.scheduled().is_none_or(|(current, _)| current != ticket) { return; }
        if self.busy || self.dialog.is_some() || !self.pending.is_empty() { self.automatic.suspend(); return; }
        if let Some(page) = self.automatic.fire(ticket, now) {
            if self.wizard.page == page && self.settings.advanced && self.batch.is_none() && self.document.is_some() {
                if let Some(stage) = page.update_stage() { self.process(stage, page, cx); cx.notify(); }
            }
        }
    }
    fn add_batch_files(&mut self, paths: &[String], cx: &mut Context<Self>) {
        // The queue model follows reference source-folder defaults. The desktop
        // export-folder choice is explicit and must survive adding more inputs.
        let export_directory = self.batch_queue.export_directory.clone();
        let duplicates = self.batch_queue.add_files(paths);
        self.batch_queue.export_directory = export_directory;
        self.batch = Some(self.batch_queue.snapshot());
        self.load_previews(cx);
        if !duplicates.is_empty() { self.fail(format!("Already in queue: {}", duplicates.join(", ")), cx); }
        cx.notify();
    }
    fn choose_files(&mut self, batch: bool, cx: &mut Context<Self>) {
        if self.busy { return; }
        self.automatic.cancel(); self.automatic_task = None;
        let prompt = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: batch, prompt: Some("Open image".into()) });
        cx.spawn(async move |this, cx| {
            let result = prompt.await;
            let _ = this.update(cx, |app, cx| match result {
                Ok(Ok(Some(paths))) if batch => {
                    let paths = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>();
                    app.add_batch_files(&paths, cx);
                }
                Ok(Ok(Some(paths))) => if let Some(path) = paths.into_iter().next() { app.open(path, cx); },
                Ok(Ok(None)) => {}, Ok(Err(error)) => app.fail(error.to_string(), cx), Err(error) => app.fail(error.to_string(), cx),
            });
        }).detach();
    }
    fn choose_export(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.busy || !self.document.as_ref().is_some_and(|doc| doc.vectors.is_some()) { return; }
        let source = &self.document.as_ref().expect("loaded document").filename;
        let parent = source.parent().unwrap_or(std::path::Path::new("."));
        let suggested = format!("{}.{}", source.file_stem().unwrap_or_default().to_string_lossy(), self.preferences.format);
        let prompt = cx.prompt_for_new_path(parent, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let result = prompt.await;
            let _ = this.update_in(cx, |app, window, cx| match result {
                Ok(Ok(Some(path))) => app.export_to(path, window, cx),
                Ok(Ok(None)) => {}, Ok(Err(error)) => app.fail(error.to_string(), cx), Err(error) => app.fail(error.to_string(), cx),
            });
        }).detach();
    }
    fn export_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.prepare_save(Command::Export { path, options: self.export_options() }, window, cx) {
            self.fail(error, cx);
        }
        cx.notify();
    }
    fn start_batch(&mut self, cx: &mut Context<Self>) {
        self.batch_queue.export_format = self.preferences.format.clone(); self.batch_log.clear();
        self.workflows.batch_result = None; self.batch_queue.set_accepting_edits(false); self.batch = Some(self.batch_queue.snapshot());
        self.enqueue(Command::Batch { queue: self.batch_queue.clone(), profile: self.current_profile(), options: self.export_options() }, None, cx);
    }
    fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag_result.active() { return; }
        // The only busy gate. Controls paint as enabled during BUSY_GRACE, so
        // clicks rely on this check; keep their handlers free of a second list.
        if self.busy && !matches!(action, Action::Cancel | Action::BatchDecision(_) | Action::Dismiss | Action::Layer(_) | Action::Zoom(_) | Action::Fit | Action::One | Action::Step(_, _) | Action::Paths) { return; }
        if matches!(action, Action::Start | Action::Batch | Action::Zoom(_) | Action::Fit | Action::One | Action::Step(_, _)) {
            self.scrollbar_interaction = None;
        }
        let mut persist_choice = matches!(&action, Action::Basic | Action::Advanced | Action::BackgroundChannel(_, _) | Action::Number(_, _) | Action::Toggle(_) | Action::BatchAutoColors | Action::BatchStart);
        let previous_page = self.wizard.page;
        let previous_controls = (AdvancedControls::new(&self.settings, self.color_count), self.settings.advanced,
            self.settings.palette_mode, self.settings.background_mode, self.settings.background);
        match action {
            Action::Workflow(action) => self.workflow_action(action, window, cx),
            Action::Open => self.choose_files(false, cx),
            Action::PasteImage => self.paste_image(cx),
            Action::ConfirmPaste(bytes) => { self.close_dialog(window, cx); self.import_clipboard(bytes, cx); }
            Action::Recent(path) => self.open(path, cx),
            Action::Batch => {
                self.automatic.reset(); self.automatic_task = None;
                self.retain_processing_preferences();
                if self.batch.is_some() {
                    self.batch = None; self.input.set_batch_mode(false);
                    self.settings = self.preferences.selected.settings.clone(); self.color_count = self.preferences.selected.color_count;
                    self.workflows.palette_count = self.preferences.selected.palette_color_count as f64;
                    self.workflows.recommended_background_mode = self.preferences.selected.recommended_background_mode;
                } else {
                    self.batch = Some(self.batch_queue.snapshot()); self.input.set_batch_mode(true);
                    self.settings = self.preferences.batch.settings.clone();
                    self.color_count = if self.settings.advanced { self.preferences.batch.color_count } else { self.preferences.batch.basic_color_count };
                    self.workflows.palette_count = self.preferences.batch.palette_color_count as f64;
                    self.workflows.recommended_background_mode = self.preferences.batch.recommended_background_mode;
                    if self.preferences.format == "emf" { self.preferences.format = "svg".into(); }
                }
                std::mem::swap(&mut self.workflows.palette, &mut self.workflows.inactive_palette);
                self.persist(cx);
            }
            Action::BatchAdd => self.choose_files(true, cx),
            Action::BatchRemove(index) => match self.batch_queue.remove(index) {
                Ok(()) => { self.batch = Some(self.batch_queue.snapshot()); self.load_previews(cx); }
                Err(error) => self.fail(error, cx),
            },
            Action::BatchFolder => {
                let prompt = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Export folder".into()) });
                cx.spawn(async move |this, cx| {
                    let result = prompt.await;
                    let _ = this.update(cx, |app, cx| {
                        match result {
                            Ok(Ok(Some(paths))) => if let Some(path) = paths.first() { app.batch_queue.export_directory = path.to_string_lossy().into_owned(); app.batch = Some(app.batch_queue.snapshot()); app.persist(cx); },
                            Ok(Ok(None)) => {}, Ok(Err(error)) => app.fail(error.to_string(), cx), Err(error) => app.fail(error.to_string(), cx),
                        }
                        cx.notify();
                    });
                }).detach();
            }
            Action::BatchStart => {
                if self.has_manual_edits() {
                    self.dialog = Some(Dialog::Form(Form::Confirm(
                        "Starting this batch will replace the current image and discard its manual edits. Cancel to return and export the current result first.".into(),
                        WorkflowAction::StartBatchAccepted)));
                } else { self.start_batch(cx); }
            }
            Action::BatchDecision(decision) => {
                if let Some(worker) = &self.worker { if let Err(error) = worker.decide(decision) { self.fail(error, cx); } }
                self.close_dialog(window, cx);
            }
            Action::Cancel => {
                self.drag_result.cancel_preparation();
                self.automatic.cancel(); self.automatic_task = None; self.clear_pending(); self.destination = None;
                if self.busy { self.in_flight = Some(WorkKind::Processing); }
                if let Some(worker) = &self.worker { worker.cancel(); } self.status = "Cancelling".into();
            }
            Action::Next => self.next(cx),
            Action::Back => self.back(cx),
            Action::Start => match self.wizard.page {
                Page::Load => {
                    let mut settings = self.task_input(); settings["palette"] = json!([]);
                    self.enqueue(Command::Reset(settings), Some(Page::Load), cx);
                    if let Err(error) = self.palette_command(openvector::profile_palette::PaletteCommand::Source, cx) { self.fail(error, cx); }
                    self.settings.background_mode = -1; self.settings.background = [1.0, 1.0, 1.0, 0.0]; self.workflows.recommended_background_mode = -1;
                    self.persist(cx);
                    // Native Start Over on Load (0x432560) always runs the full model
                    // reset 0x430ed0, whose mainModelReset suggests a settings name.
                    self.workflows.suggest_profile_name(cx);
                },
                Page::Background => self.enqueue(Command::Background { action: "finish", input: Value::Null }, Some(Page::Load), cx),
                Page::Palette => self.enqueue(Command::Commit(self.task_input()), Some(Page::Load), cx),
                Page::Editing => self.enqueue(Command::Edit { action: "leave", input: Value::Null }, Some(Page::Load), cx),
                _ => { self.wizard.page = Page::Load; self.page_changed(cx); },
            },
            Action::SourceSizing(shrink) => {
                if self.wizard.page == Page::Mode && self.source_sizing.is_some() { self.source_sizing = Some(shrink); }
            }
            Action::Automatic => { self.settings.advanced = false; self.leave_mode(Some(Command::Automatic(self.task_input())), self.wizard.completed_page(&self.settings), cx); }
            Action::Basic => {
                self.settings.advanced = false;
                let command = (self.batch.is_none() && self.source_recommendations.is_none())
                    .then(|| Command::AnalyzeSource(self.task_input()));
                self.leave_mode(command, Page::ImageType, cx);
            }
            Action::Advanced => {
                self.settings.advanced = true;
                let initializing = !self.transparency_initialized[1];
                let page = if self.document.as_ref().is_some_and(|doc| doc.source_alpha_type != 0) { Page::Transparency } else { Page::Palette };
                self.leave_mode(None, page, cx);
                // Synchronous initialization already saved the whole selection.
                // Queued source preparation still needs this mode choice saved now.
                if initializing && self.transparency_initialized[1] { persist_choice = false; }
            }
            Action::Set(key, value) => {
                if self.wizard.page == Page::Review {
                    if (key == "paletteMode" && value == 1) || (key == "category" && value == 1) {
                        self.settings.advanced = false; self.settings.palette_mode = 1;
                        if key == "category" { self.settings.category = 1; }
                        self.persist(cx); self.process("palette", Page::Palette, cx); return;
                    }
                    if key == "quality" || key == "paletteMode" {
                        self.change_setting(key, value, cx); return;
                    }
                }
                self.change_setting(key, value, cx);
                let custom_background = key == "backgroundMode" && self.settings.background_mode == 2;
                if !custom_background && matches!(self.wizard.page, Page::ImageType | Page::Transparency | Page::Quality | Page::Colors) && self.batch.is_none() { self.next(cx); }
            }
            Action::BackgroundChannel(channel, delta) => {
                let value = (self.settings.background[channel] * 255.0).round() as i32;
                self.settings.background[channel] = (value + delta).clamp(0, 255) as f32 / 255.0;
                self.settings.background[3] = 1.0;
            }
            Action::Number(key, delta) => {
                if key == "colorCount" {
                    self.color_count = (self.color_count + delta).clamp(2, if self.batch.is_some() && !self.settings.advanced { 12 } else { 24 });
                    if self.batch.is_none() && !self.settings.advanced && self.wizard.page == Page::Palette && self.wizard.auto_palette {
                        self.process("palette", Page::Palette, cx);
                    }
                }
                else { let value = Value::Object(self.settings.to_json())[key].as_i64().unwrap_or(0); self.change_setting(key, json!((value + delta as i64).clamp(if key == "minimumPixels" { 0 } else { 1 }, if key == "minimumPixels" { i32::MAX as i64 } else { 12 })), cx); }
            }
            Action::Toggle(key) => {
                if key == "advanced" && self.batch.is_some() {
                    self.retain_processing_preferences(); self.settings.advanced = !self.settings.advanced;
                    self.color_count = if self.settings.advanced { self.preferences.batch.color_count } else { self.preferences.batch.basic_color_count };
                } else {
                    let value = Value::Object(self.settings.to_json())[key].as_bool().unwrap_or(false);
                    self.change_setting(key, json!(!value), cx);
                }
            }
            Action::BatchAutoColors => self.color_count = 0,
            Action::ImproveResult => { self.wizard.page = Page::Review; self.page_changed(cx); }
            Action::Process(stage) => self.process(stage, self.wizard.page, cx),
            Action::Tool(tool_id) => {
                self.input.select_tool(tool_id);
                let tool = match tool_id { tool::PENCIL => "pencil", tool::FILL => "fill", tool::ZAP => "zap", tool::PICKER => "picker", tool::BACKGROUND => "background", _ => "pan" };
                let input = json!({"phase":"select","tool":tool});
                let command = if self.wizard.page == Page::Background { Command::Background { action: "gesture", input } } else { Command::Edit { action: "gesture", input } };
                self.enqueue(command, None, cx);
            }
            Action::Edit(action) => {
                let command = if self.wizard.page == Page::Background { Command::Background { action, input: Value::Null } } else { Command::Edit { action, input: Value::Null } };
                self.enqueue(command, None, cx);
            }
            Action::Background => self.enqueue(Command::Background { action: "begin", input: Value::Null }, Some(Page::Background), cx),
            Action::EditResult => { self.wizard.edit_origin = Page::Review; self.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Page::Editing), cx); }
            Action::Layer(layer) => self.set_layer(layer), Action::Zoom(delta) => self.view = zoom::slider(&self.view, self.view.slider as i64 + delta),
            Action::Fit => self.fit(), Action::One => self.view = zoom::one(&self.view),
            Action::Step(axis, steps) => if let Some(view) = self.view.step(axis, steps) { self.view = view },
            Action::Paths => self.show_paths = !self.show_paths,
            Action::Export => self.choose_export(window, cx),
            Action::CurveMode(mode) => { self.preferences.curve_mode = mode.into(); self.persist(cx); },
            Action::Format(format) => { self.preferences.format = format.into(); self.persist(cx); },
            Action::Layout(layout) => { self.preferences.layout = layout.into(); self.persist(cx); },
            Action::ThinStroke => { self.preferences.thin_stroke = !self.preferences.thin_stroke; self.persist(cx); },
            Action::CompactEps => { self.preferences.compact_eps = !self.preferences.compact_eps; self.persist(cx); },
            Action::DragFormat(format) => { self.preferences.drag_result.choices[usize::from(self.settings.advanced)].format = format; self.persist(cx); }
            Action::DragMinimize => {
                let choice = &mut self.preferences.drag_result.choices[usize::from(self.settings.advanced)];
                choice.minimize = !choice.minimize; self.persist(cx);
            }
            Action::DragRetry => self.drag_result.invalidate(),
            Action::SkipReview => { self.wizard.skip_review = !self.wizard.skip_review; self.preferences.skip_review = self.wizard.skip_review; self.persist(cx); },
            Action::Auto(stage) => {
                match stage {
                    "palette" => { self.wizard.auto_palette = !self.wizard.auto_palette; self.preferences.auto_palette = self.wizard.auto_palette; },
                    "segmentation" => { self.wizard.auto_segmentation = !self.wizard.auto_segmentation; self.preferences.auto_segmentation = self.wizard.auto_segmentation; },
                    _ => { self.wizard.auto_vectorization = !self.wizard.auto_vectorization; self.preferences.auto_vectorization = self.wizard.auto_vectorization; },
                }
                self.persist(cx);
            }
            Action::About => self.dialog = Some(Dialog::About),
            Action::Dismiss => {
                if matches!(self.dialog, Some(Dialog::PasteImage(_) | Dialog::Form(Form::Confirm(_, WorkflowAction::RunProcess(..))))) {
                    self.automatic.cancel(); self.automatic_task = None;
                }
                if matches!(self.dialog, Some(Dialog::Form(workflows::Form::Confirm(_, WorkflowAction::RunProcess(..))))) { self.clear_pending(); }
                if matches!(self.dialog, Some(Dialog::BatchOverwrite(_))) { if let Some(worker) = &self.worker { let _ = worker.decide(OverwriteDecision::Cancel); } }
                self.close_dialog(window, cx);
            }
        }
        if persist_choice { self.persist(cx); }
        let changed = previous_controls != (AdvancedControls::new(&self.settings, self.color_count), self.settings.advanced,
            self.settings.palette_mode, self.settings.background_mode, self.settings.background);
        self.sync_automatic(changed && previous_page == self.wizard.page, cx);
        self.prepare_drag(cx);
        cx.notify();
    }
    fn change_setting(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let mut settings = Value::Object(self.settings.to_json()); settings[key] = value;
        if key == "backgroundMode" && settings[key] == 1 { settings["background"] = json!([1, 1, 1, 1]); }
        if key == "backgroundMode" && settings[key] == 2 { settings["background"][3] = json!(1); }
        match openvector::document_controls::read_document_settings(&settings) {
            Ok(settings) => self.settings = settings, Err(error) => { self.fail(error.to_string(), cx); return; }
        }
        self.persist(cx);
        if self.batch.is_none() && self.wizard.page == Page::Review { self.process("fitting", Page::Review, cx); }
    }
    fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.retain_save_choices(cx) { self.fail(error, cx); }
        self.dialog = None;
        if let Some(focus) = self.restore_focus.take() { focus.focus(window, cx); }
        else { self.focus.focus(window, cx); }
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.key == "escape" && self.cancel_internal_drag(window, cx) { cx.stop_propagation(); return; }
        if key.key == "tab" {
            loop {
                if key.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); }
                if self.dialog.is_none() || self.dialog_focus.contains_focused(window, cx) { break; }
            }
            cx.stop_propagation(); return;
        }
        if self.dialog.is_some() {
            if key.key == "escape" { self.dispatch(Action::Dismiss, window, cx); }
            if key.key == "enter" && (self.dialog_focus.is_focused(window) || self.workflows.fields.iter().any(|field| field.read(cx).focus_handle(cx).is_focused(window))) {
                self.workflow_enter(window, cx); cx.stop_propagation();
            }
            return;
        }
        if (key.modifiers.control || key.modifiers.platform) && !key.modifiers.alt {
            let action = match key.key.as_str() { "o" => Some(Action::Open), "s" if self.batch.is_none() => Some(Action::Export), _ => None };
            if let Some(action) = action { self.dispatch(action, window, cx); cx.stop_propagation(); return; }
        }
        // The inline settings name owns its keys while it is on screen: Enter
        // saves, and typed text must reach neither the viewer's held-layer keys
        // nor wizard shortcuts. GPUI keeps focus on a field that a page change
        // hid; containment in the rendered frame excludes that stale focus.
        if self.workflows.profile_name.read(cx).focus_handle(cx).is_focused(window) && self.focus.contains_focused(window, cx) {
            if key.key == "enter" { self.dispatch(Action::Workflow(WorkflowAction::SaveProfile(false)), window, cx); cx.stop_propagation(); }
            return;
        }
        if key.key == "v" && (key.modifiers.control || key.modifiers.platform) && !key.modifiers.alt {
            self.dispatch(Action::PasteImage, window, cx); cx.stop_propagation(); return;
        }
        if key.key == "escape" && self.workflows.eyedropper.take().is_some() { self.status = "Color selection cancelled".into(); cx.notify(); return; }
        self.viewer_key(input::qt_key_code(&key.key), key.modifiers, KeyPhase::Press, cx);
        let action = if (key.modifiers.control || key.modifiers.platform) && matches!(key.key.as_str(), "z" | "y") && matches!(self.wizard.page, Page::Editing | Page::Background) {
            Some(Action::Edit(if key.key == "y" || key.modifiers.shift { "redo" } else { "undo" }))
        } else if key.modifiers.alt && key.key == "r" { Some(Action::Start) }
        else if key.key == "pageup" || key.modifiers.alt && matches!(key.key.as_str(), "b" | "left") { Some(Action::Back) }
        else if key.key == "pagedown" || key.modifiers.alt && matches!(key.key.as_str(), "n" | "right") || key.key == "enter" && self.focus.is_focused(window) { Some(Action::Next) }
        // Focused view (the root handle, which a viewport press focuses): Qt's
        // QAbstractScrollArea arrows step the scrollbars by singleStep, Up
        // subtracting. Alt arrows stay wizard shortcuts, as in the browser port.
        else if !key.modifiers.alt && self.focus.is_focused(window) && self.document.is_some() && self.batch.is_none() {
            match key.key.as_str() {
                "left" => Some(Action::Step(Axis::Horizontal, -1.0)), "right" => Some(Action::Step(Axis::Horizontal, 1.0)),
                "up" => Some(Action::Step(Axis::Vertical, -1.0)), "down" => Some(Action::Step(Axis::Vertical, 1.0)),
                _ => None,
            }
        }
        else { None };
        if let Some(action) = action { self.dispatch(action, window, cx); cx.stop_propagation(); }
    }
    fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "v" && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform) {
            cx.stop_propagation(); return;
        }
        self.viewer_key(input::qt_key_code(&event.keystroke.key), event.keystroke.modifiers, KeyPhase::Release, cx);
    }
    fn viewer_key(&mut self, key: i64, modifiers: gpui::Modifiers, phase: KeyPhase, cx: &mut Context<Self>) {
        let availability = LayerAvailability { bitmap_enabled: self.document.is_some(), vector_enabled: self.document.as_ref().is_some_and(|doc| doc.display_vectors.is_some()) };
        let event = ViewerKey { phase, key, modifiers: input::qt_modifiers(modifiers.shift, modifiers.control, modifiers.alt, modifiers.platform) };
        if let Err(error) = self.input.key(event, availability) { self.fail(error.to_string(), cx); }
        cx.notify();
    }
    fn pointer_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        // GPUI reports one held button, not a chord mask. Retain locally
        // observed chord members until release; recover outside presses.
        match event.pressed_button {
            Some(MouseButton::Left) => self.pointer_buttons |= 1,
            Some(MouseButton::Right) => self.pointer_buttons |= 2,
            Some(MouseButton::Middle) => self.pointer_buttons |= 4,
            None => { self.pointer_buttons = 0; self.dragging = None; },
            _ => {},
        }
        if self.pointer_buttons != 0 { self.pointer("move", event.position, 0, cx); }
    }
    fn pointer(&mut self, phase: &'static str, position: gpui::Point<gpui::Pixels>, button: u32, cx: &mut Context<Self>) {
        let point = Point::new(f32::from(position.x - self.viewport.origin.x) as f64, f32::from(position.y - self.viewport.origin.y) as f64);
        if phase == "press" && button == 1 {
            if let Some(index) = self.workflows.eyedropper {
                let point = self.view.image_point(point);
                if let Some(document) = &self.document {
                    if point.x >= 0.0 && point.y >= 0.0 && point.x < document.width as f64 && point.y < document.height as f64 {
                        let offset = (point.y as usize * document.width + point.x as usize) * 4;
                        let color = document.source.as_bytes(0).expect("source image has one frame")[offset..offset + 4].iter().map(|byte| *byte as f64 / 255.0).collect();
                        match self.palette_command(openvector::profile_palette::PaletteCommand::Color { index: index as f64, color }, cx) {
                            Ok(()) => { self.workflows.eyedropper = None; self.status = "Palette color selected".into(); },
                            Err(error) => self.fail(error, cx),
                        }
                    }
                }
                return;
            }
        }
        if phase == "press" { self.pointer_buttons |= button; } else if phase == "release" { self.pointer_buttons &= !button; }
        if self.input.snapshot().tool == tool::PAN {
            if phase == "press" && button == 1 { self.dragging = Some(point); }
            else if phase == "release" { self.dragging = None; }
            else if let Some(previous) = self.dragging { self.view = self.view.pan(Point::new(point.x - previous.x, point.y - previous.y)); self.dragging = Some(point); }
        } else if matches!(self.wizard.page, Page::Editing | Page::Background) {
            // Samples may follow another gesture, but never cross a processing,
            // navigation or source-replacement command already in the queue.
            if self.dialog.is_some() || self.in_flight == Some(WorkKind::Processing)
                || self.pending_processing != 0 { return; }
            let world = self.view.image_point(point);
            let input = json!({"phase":phase,"point":[world.x,world.y],"buttons":self.pointer_buttons,"button":button,"modifiers":0});
            let command = if self.wizard.page == Page::Background { Command::Background { action: "gesture", input } } else { Command::Edit { action: "gesture", input } };
            self.enqueue(command, None, cx);
        }
        cx.notify();
    }
    /// While a dialog is open only its own actions stay enabled.
    fn control_enabled(&self, enabled: bool, action: &Action) -> bool {
        enabled && (self.dialog.is_none() || matches!(action, Action::Dismiss | Action::ConfirmPaste(_) | Action::BatchDecision(_)))
    }
    fn control(&self, id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool, selected: bool, action: Action, cx: &Context<Self>) -> Stateful<Div> {
        let enabled = self.control_enabled(enabled, &action);
        // Text-only buttons: the user preferred the cleaner icon-free preview.
        icon_button(id, label, enabled, selected, None).when(enabled, |d| d.on_click(cx.listener(move |app, _, window, cx| app.dispatch(action.clone(), window, cx))))
    }
    fn choice(&self, key: &'static str, options: &[(&'static str, i32)], cx: &Context<Self>) -> Div {
        let current = Value::Object(self.settings.to_json())[key].as_i64().unwrap_or(-1);
        div().flex().flex_col().gap_1().children(options.iter().map(|(label, value)| self.control(format!("{key}-{value}"), *label, !self.busy, current == *value as i64, Action::Set(key, json!(value)), cx)))
    }
    /// `choice` laid out horizontally, for short option sets (High / Medium / Low).
    fn choice_row(&self, key: &'static str, options: &[(&'static str, i32)], cx: &Context<Self>) -> Div {
        let current = Value::Object(self.settings.to_json())[key].as_i64().unwrap_or(-1);
        segmented(options.iter().map(|(label, value)| self.control(format!("{key}-{value}"), *label, !self.busy, current == *value as i64, Action::Set(key, json!(value)), cx)))
    }
    /// The page's main action, filled with the accent colour.
    fn primary(&self, id: &'static str, label: &'static str, enabled: bool, action: Action, cx: &Context<Self>) -> Stateful<Div> {
        let enabled = self.control_enabled(enabled, &action);
        primary_button(id, label, enabled).when(enabled, |d| d.on_click(cx.listener(move |app, _, window, cx| app.dispatch(action.clone(), window, cx))))
    }
    fn number(&self, key: &'static str, label: &'static str, value: i32, cx: &Context<Self>) -> Div {
        row(label, div().flex().items_center().gap_1()
            .child(self.control(format!("{key}-down"), "−", !self.busy, false, Action::Number(key, -1), cx).aria_label(format!("Decrease {label}")))
            .child(self.control(format!("{key}-value"), if key == "colorCount" && value == 0 { "Auto".into() } else { value.to_string() }, !self.busy, false, Action::Workflow(WorkflowAction::Numeric(key, value)), cx).font_family("Geist Mono").aria_label(format!("Edit {label}")))
            .child(self.control(format!("{key}-up"), "+", !self.busy, false, Action::Number(key, 1), cx).aria_label(format!("Increase {label}"))))
    }
    fn check(&self, key: &'static str, label: &'static str, value: bool, cx: &Context<Self>) -> Stateful<Div> {
        self.checkbox(key, label, value, Action::Toggle(key), cx)
    }
    /// A left-aligned row with a drawn 14px box. State lives in the box, not in
    /// the selected-button tint, so a checked option never looks like a choice.
    fn checkbox(&self, id: &'static str, label: &'static str, value: bool, action: Action, cx: &Context<Self>) -> Stateful<Div> {
        self.control(id, "", !self.busy, false, action, cx).justify_start().gap_2().aria_label(label)
            .aria_toggled(if value { gpui::Toggled::True } else { gpui::Toggled::False })
            .child(div().size(px(14.0)).flex_none().rounded(px(3.0)).border_1().flex().items_center().justify_center()
                .border_color(rgb(if value { theme::ACCENT } else { theme::MUTED })).bg(rgb(if value { theme::ACCENT } else { theme::CONTROL }))
                .text_size(px(10.0)).text_color(rgb(0xffffff)).when(value, |d| d.child("✓")))
            .child(label)
    }
    fn settings_panel(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_1()
            .child(section("Image type")).child(self.choice("category", &[("Photograph", 0), ("Artwork · blended edges", 1), ("Artwork · unblended edges", 2), ("Auto-detect", 3)], cx))
            .child(section("Input quality")).child(self.choice("quality", &[("High", 0), ("Medium", 1), ("Low", 2), ("Auto-detect", 3)], cx))
            .child(section("Color mode")).child(self.choice("paletteMode", &[("Two colors", 0), ("Fixed palette", 1), ("Flexible palette", 2), ("Auto-detect", 3)], cx))
            .child(self.number("colorCount", "Number of colors", self.color_count, cx))
            .when(!self.settings.advanced, |body| body.child(self.checkbox("batch-auto-colors", "Auto-detect number of colors", self.color_count == 0, Action::BatchAutoColors, cx)))
            .child(section("Transparency")).child(self.choice("backgroundMode", &[("Leave transparent", 0), ("White", 1), ("Auto-detect", 3)], cx))
            .when(self.settings.advanced, |d| d.child(self.processing_controls(cx)))
    }
    fn processing_controls(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_1()
            .child(section("Color sensitivity")).child(self.choice("sensitivity", &[("High", 0), ("Medium", 1), ("Low", 2)], cx))
            .child(self.check("antiAlias", "Blended edges", self.settings.anti_alias, cx))
            .child(self.number("complexity", "Complexity", self.settings.complexity, cx))
            .child(self.number("minimumPixels", "Minimum pixels", self.settings.minimum_pixels, cx))
            .child(section("Artifact rejection")).child(self.choice("artifactRejection", &[("None", 0), ("Moderate", 1), ("Aggressive", 2)], cx))
            .child(self.check("clusterColors", "Cluster colors", self.settings.cluster_colors, cx))
            .child(self.number("smoothness", "Smoothness", self.settings.smoothness, cx))
            .child(self.number("curveComplexity", "Curve complexity", self.settings.curve_complexity, cx))
            .child(self.check("detectCorners", "Detect sharp corners", self.settings.detect_corners, cx))
            .child(self.check("smoothingAntiAlias", "Anti-alias contours", self.settings.smoothing_anti_alias, cx))
    }
    fn background_color(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_1().children([(2, "Red"), (1, "Green"), (0, "Blue")].into_iter().map(|(channel, label)| {
            row(label, div().flex().gap_1().items_center()
                .child(self.control(format!("background-{channel}-down"), "−", !self.busy, false, Action::BackgroundChannel(channel, -1), cx).aria_label(format!("Decrease background {label}")))
                .child(div().w(px(36.0)).text_center().child(format!("{:.0}", self.settings.background[channel] * 255.0)))
                .child(self.control(format!("background-{channel}-up"), "+", !self.busy, false, Action::BackgroundChannel(channel, 1), cx).aria_label(format!("Increase background {label}"))))
        }))
    }
    fn export_panel(&self, cx: &Context<Self>) -> Div {
        let formats = ["ai", "dxf", "emf", "eps", "pdf", "svg"].into_iter().filter(|format| self.batch.is_none() || *format != "emf").collect::<Vec<_>>();
        let format = |format: &'static str| self.control(format, format.to_uppercase(), !self.busy, self.preferences.format == format, Action::Format(format), cx);
        div().flex().flex_col()
            // Three per row keeps the formats in an even grid.
            .child(group("Format", div().flex().flex_col().gap_1().children(formats.chunks(3).map(|row| segmented(row.iter().map(|name| format(name)))))))
            .when(self.preferences.format == "svg", |body| body.child(div().text_color(rgb(theme::MUTED)).child(workflows::SVG_SIZE_NOTICE)))
            // Plain-language names for stacked / cutouts / grouped-cutouts; one hint
            // line explains the selected choice.
            .child(group("Shape layout", div().flex().flex_col().gap_1()
                .children(SHAPE_LAYOUTS.iter().map(|(value, label, _)| self.control(*value, *label, !self.busy, self.preferences.layout == *value, Action::Layout(*value), cx)))
                .children(SHAPE_LAYOUTS.iter().find(|(value, _, _)| self.preferences.layout == *value)
                    .map(|(_, _, hint)| div().text_color(rgb(theme::MUTED)).child(*hint)))))
            .when(self.preferences.format == "dxf", |body| body.child(group("DXF curves", segmented(
                [("splines", "Splines"), ("sparse-lines", "Sparse lines"), ("dense-lines", "Dense lines")].into_iter().map(|(mode, label)| self.control(mode, label, !self.busy, self.preferences.curve_mode == mode, Action::CurveMode(mode), cx))))))
            .child(group("Options", div().flex().flex_col().gap_1()
                .child(self.checkbox("thin-stroke", "Thin outline", self.preferences.thin_stroke, Action::ThinStroke, cx))
                .when(self.preferences.format == "eps", |d| d.child(self.checkbox("compact-eps", "Compact EPS", self.preferences.compact_eps, Action::CompactEps, cx)))))
    }
    fn source_sizing_panel(&self, cx: &Context<Self>) -> Div {
        let mut panel = div().flex().flex_col().gap_1();
        if let (Some(shrink), Some(document)) = (self.source_sizing, &self.document) {
            let [width, height] = openvector::image_resize::source_shrink_dimensions(document.width, document.height);
            panel = panel.child(section("Source size"))
                .child(format!("Your image is {} × {} pixels.", document.width, document.height))
                .child(self.control("source-recommended", format!("{width} × {height} (Recommended)"), !self.busy, shrink, Action::SourceSizing(true), cx))
                .child(self.control("source-unchanged", "Leave unchanged", !self.busy, !shrink, Action::SourceSizing(false), cx));
        }
        panel
    }
    fn wizard_panel(&self, cx: &Context<Self>) -> Div {
        let mut body = div().flex().flex_col().gap_2();
        use Page::*;
        body = match self.wizard.page {
            Load => body.child(self.primary("browse", "Open image", !self.busy, Action::Open, cx).w_full())
                .child(div().text_color(rgb(theme::MUTED)).child("PNG, JPEG, BMP, GIF or TIFF"))
                .child(div().mt_2().child(self.recent_panel(cx))),
            Mode => body
                .child(hint(self.control("automatic", "Fully automatic", !self.busy, false, Action::Automatic, cx), "Convert with recommended settings."))
                .child(hint(self.control("basic", "Basic", !self.busy, false, Action::Basic, cx), "Answer a few questions about the image."))
                .child(hint(self.control("advanced", "Advanced", !self.busy, false, Action::Advanced, cx), "Tune palette, regions and curves."))
                .child(self.source_sizing_panel(cx)),
            ImageType => body.child(self.choice("category", &[("Photograph", 0), ("Artwork with blended edges", 1), ("Artwork without blending", 2)], cx)),
            Transparency => body.child(self.choice("backgroundMode", &[("Leave transparent", 0), ("White", 1), ("Custom color", 2)], cx))
                .when(self.settings.background_mode == 2, |body| body.child(self.background_color(cx))),
            Quality => body.child(self.choice_row("quality", &[("High", 0), ("Medium", 1), ("Low", 2)], cx)),
            Colors => body.child(self.choice("paletteMode", &[("Two colors", 0), ("Fixed palette", 1), ("Flexible palette", 2)], cx)),
            Palette => body
                .child(group("Number of colors", div().flex().flex_col().gap_2()
                    .when(self.settings.advanced, |body| body.child(self.number("colorCount", "Count", self.color_count, cx)))
                    .child(self.palette_candidates(cx))))
                .when(self.settings.advanced, |body| body
                    .child(group("Color mode", self.choice("paletteMode", &[("Two colors", 0), ("Fixed palette", 1), ("Flexible palette", 2)], cx)))
                    .child(group("Color sensitivity", div().flex().flex_col().gap_1()
                        .child(self.choice_row("sensitivity", &[("High", 0), ("Medium", 1), ("Low", 2)], cx))
                        .child(self.check("antiAlias", "Blended edges", self.settings.anti_alias, cx)))))
                .child(group("Palette", div().flex().flex_col().gap_2()
                    .child(div().flex().flex_wrap().gap_1().children(self.settings.palette.chunks_exact(4).enumerate().map(|(index, color)| {
                        let rgba = ((color[2] * 255.0).round() as u32) << 24 | ((color[1] * 255.0).round() as u32) << 16 | ((color[0] * 255.0).round() as u32) << 8 | 255;
                        // Built without control()'s hover fill, which would cover the
                        // colour: hovering keeps the colour and only lifts the border.
                        let action = Action::Workflow(WorkflowAction::Color(index));
                        let enabled = self.control_enabled(!self.busy, &action);
                        unfilled_button(format!("swatch-{index}"), "", enabled, false, None).w(px(28.0)).bg(gpui::rgba(rgba))
                            .aria_label(format!("Edit palette color {}", index + 1))
                            .when(enabled, |d| d.hover(|s| s.border_color(rgb(theme::ACCENT)))
                                .on_click(cx.listener(move |app, _, window, cx| app.dispatch(action.clone(), window, cx))))
                    })))
                    // Public Add is disabled at 24; the direct slot separately caps at 25.
                    .child(segmented([
                        self.control("palette-add", "Add", !self.busy && self.settings.palette.len() / 4 != 24, false, Action::Workflow(WorkflowAction::Palette(openvector::profile_palette::PaletteCommand::Add)), cx)
                            .debug_selector(|| "palette-add".into()),
                        self.control("palette-delete", "Remove last", !self.busy && !self.settings.palette.is_empty(), false, Action::Workflow(WorkflowAction::Palette(openvector::profile_palette::PaletteCommand::Delete((self.settings.palette.len() / 4).saturating_sub(1) as f64))), cx),
                    ]))
                    .child(segmented([
                        self.control("palette-source", "From image", !self.busy, false, Action::Workflow(WorkflowAction::Palette(openvector::profile_palette::PaletteCommand::Source)), cx),
                        self.control("palette-revert", "Revert", !self.busy, false, Action::Workflow(WorkflowAction::Palette(openvector::profile_palette::PaletteCommand::Revert)), cx),
                    ]))
                    .child(self.control("analyze", "Analyze colors", !self.busy, false, Action::Process("palette"), cx)))),
            Segmentation => body
                .child(group("Regions", div().flex().flex_col()
                    .child(self.number("complexity", "Complexity", self.settings.complexity, cx))
                    .child(self.number("minimumPixels", "Minimum pixels per region", self.settings.minimum_pixels, cx))))
                .child(group("Artifact rejection", self.choice_row("artifactRejection", &[("None", 0), ("Moderate", 1), ("Aggressive", 2)], cx)))
                .child(group("Options", self.check("clusterColors", "Cluster colors", self.settings.cluster_colors, cx)))
                .child(self.control("segment-update", "Update segmentation", !self.busy, false, Action::Process("segmentation"), cx)),
            Smoothing => body
                .child(group("Curves", div().flex().flex_col()
                    .child(self.number("smoothness", "Smoothness", self.settings.smoothness, cx))
                    .child(self.number("curveComplexity", "Curve complexity", self.settings.curve_complexity, cx))))
                .child(group("Options", div().flex().flex_col().gap_1()
                    .child(self.check("detectCorners", "Detect sharp corners", self.settings.detect_corners, cx))
                    .child(self.check("smoothingAntiAlias", "Anti-alias contours", self.settings.smoothing_anti_alias, cx))))
                .child(self.control("fitting-update", "Update vectorization", !self.busy, false, Action::Process("fitting"), cx)),
            Editing | Background => {
                let tools: &[(i32, &str)] = if self.wizard.page == Editing { &[(tool::PAN, "Pan"), (tool::PENCIL, "Pencil"), (tool::FILL, "Fill"), (tool::ZAP, "Zap"), (tool::PICKER, "Pick")] } else { &[(tool::PAN, "Pan"), (tool::BACKGROUND, "Erase")] };
                let active = self.input.snapshot().tool;
                let help = |tool: i32| match tool {
                    tool::PENCIL => "Draw with the paint color.",
                    tool::FILL => "Replace a connected region with the paint color.",
                    tool::ZAP => "Merge a region into its neighboring colors.",
                    tool::PICKER => "Click the image to take its color.",
                    tool::BACKGROUND => "Click a region to hide it. Click again to restore it.",
                    // Pan keeps the image movable while editing; it paints nothing.
                    _ => "Drag to move the image without changing it.",
                };
                body.child(group("Tool", div().flex().flex_col().gap_2()
                    .child(segmented(tools.iter().map(|(selected_tool, label)|
                        hint(self.control(format!("tool-{selected_tool}"), label.to_string(), !self.busy, active == *selected_tool, Action::Tool(*selected_tool), cx), help(*selected_tool)))))
                    // The active tool's explanation stays visible, not only on hover.
                    .child(div().text_color(rgb(theme::MUTED)).child(help(active)))))
                    .when(self.wizard.page == Editing, |body| {
                        let color = self.document.as_ref().map_or([0.0, 0.0, 0.0, 1.0], |document| document.edit_color);
                        let rgba = ((color[2] * 255.0).round() as u32) << 24 | ((color[1] * 255.0).round() as u32) << 16 | ((color[0] * 255.0).round() as u32) << 8 | 255;
                        let hex = format!("#{:02X}{:02X}{:02X}", (color[2] * 255.0).round() as u8, (color[1] * 255.0).round() as u8, (color[0] * 255.0).round() as u8);
                        body.child(group("Paint color", div().flex().gap_1()
                            .child(self.control("edit-color", hex, !self.busy, false, Action::Workflow(WorkflowAction::PaintColor), cx).flex_1().justify_start()
                                .child(div().ml_auto().size(px(16.0)).rounded(px(3.0)).border_1().border_color(rgb(theme::BORDER)).bg(gpui::rgba(rgba))))
                            .child(self.control("edit-opacity", format!("{:.0}%", color[3] * 100.0), !self.busy, false, Action::Workflow(WorkflowAction::PaintColor), cx).w(px(64.0)).aria_label("Opacity"))))
                    })
                    .child(group("History", segmented([
                        self.control("undo", "Undo", !self.busy && self.document.as_ref().is_some_and(|doc| doc.undo), false, Action::Edit("undo"), cx),
                        self.control("redo", "Redo", !self.busy && self.document.as_ref().is_some_and(|doc| doc.redo), false, Action::Edit("redo"), cx),
                        self.control("revert", "Undo all", !self.busy, false, Action::Edit("revert"), cx),
                    ])))
            }
            // Done reviewing lives in the footer as the primary action.
            Review => body
                .child(group("Input quality", self.choice_row("quality", &[("High", 0), ("Medium", 1), ("Low", 2)], cx)))
                .child(group("Colors", div().flex().flex_col().gap_2()
                    .child(segmented([
                        self.control("review-unlimited", "Unlimited", !self.busy, self.settings.palette_mode == 2, Action::Set("paletteMode", json!(2)), cx),
                        self.control("review-custom", "Custom", !self.busy, matches!(self.settings.palette_mode, 0 | 1), Action::Set("paletteMode", json!(1)), cx),
                    ]))
                    .when(matches!(self.settings.palette_mode, 0 | 1), |body| body
                        .child(div().flex().flex_wrap().gap_1().children(self.settings.palette.chunks_exact(4).map(|color| {
                            let rgba = ((color[2] * 255.0).round() as u32) << 24 | ((color[1] * 255.0).round() as u32) << 16 | ((color[0] * 255.0).round() as u32) << 8 | (color[3] * 255.0).round() as u32;
                            div().size(px(18.0)).rounded(px(3.0)).border_1().border_color(rgb(theme::BORDER)).bg(gpui::rgba(rgba))
                        }))))))
                .child(group("Refine", div().flex().flex_col().gap_1()
                    .child(hint(self.control("remove-background", "Remove background", !self.busy, false, Action::Background, cx), "Hide the regions you click."))
                    .child(hint(self.control("edit-result", "Edit result", !self.busy, false, Action::EditResult, cx), "Paint, fill or merge regions by hand."))
                    .child(hint(self.control("hand-pick", "Hand-pick settings", !self.busy, false, Action::Workflow(WorkflowAction::HandPick), cx), "Go back and answer the Basic questions."))))
                .child(self.checkbox("skip-review", "Skip this page next time", self.wizard.skip_review, Action::SkipReview, cx)),
            Export => body.child(self.export_panel(cx)).child(self.primary("save-export", "Save vector", !self.busy, Action::Export, cx))
                .child(self.save_disclosure(false, cx)).child(self.save_disclosure(true, cx)).child(self.drag_panel(cx))
                .when(self.settings.advanced, |body| body.child(self.save_settings_panel(cx)))
                .when(!self.settings.advanced, |body| body.child(self.control("improve-result", "Improve result", !self.busy, false, Action::ImproveResult, cx))),
        };
        let auto = match self.wizard.page { Palette => Some(("palette", self.wizard.auto_palette)), Segmentation => Some(("segmentation", self.wizard.auto_segmentation)), Smoothing => Some(("fitting", self.wizard.auto_vectorization)), _ => None };
        let dirty = self.dirty_page();
        div().w(px(theme::WIZARD_WIDTH)).flex_none().h_full().flex().flex_col().border_l_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::SURFACE))
            .child(div().p_4().border_b_1().border_color(rgb(theme::BORDER)).child(div().text_color(rgb(theme::MUTED)).child(if self.settings.advanced { "Advanced wizard" } else { "Basic wizard" }))
                .child(div().mt_1().text_size(px(18.0)).child(self.wizard.page.title())))
            .child(div().id("wizard-body").flex_1().min_h_0().overflow_y_scroll().p_4().child(body))
            // Saved-settings shortcuts sit at the bottom of Mode, apart from the three
            // conversion modes. Their own scroll keeps expanded lists from crowding them.
            .when(self.wizard.page == Mode, |d| d.child(div().id("mode-saved-settings").flex_none().max_h(px(240.0)).overflow_y_scroll()
                .px_4().py_3().border_t_1().border_color(rgb(theme::BORDER)).child(self.mode_settings_panel(cx))))
            .when(self.settings.advanced && auto.is_some(), |d| { let (stage, checked) = auto.expect("auto control"); d.child(div().px_4().pb_2().child(self.checkbox("auto-update", "Auto-update preview", checked, Action::Auto(stage), cx))) })
            .child(div().p_3().flex().gap_2().border_t_1().border_color(rgb(theme::BORDER))
                .child(self.control("start", "Start over", !self.busy && self.document.is_some(), false, Action::Start, cx))
                .child(self.control("back", if self.can_revert() { "Discard changes" } else { "Back" }, !self.busy && self.wizard.page != Load, false, Action::Back, cx))
                .child(div().flex_1())
                // Mode picks a route and Export saves, so neither has a Next step.
                .when(!matches!(self.wizard.page, Mode | Export), |footer| footer
                    .child(self.primary(if self.wizard.page == Review && !dirty { "done-reviewing" } else { "next" },
                        if dirty { "Update" } else if self.wizard.page == Review { "Done reviewing" } else { "Next" },
                        !self.busy && self.document.is_some(), Action::Next, cx).px_4())))
    }
    fn batch_panel(&self, cx: &Context<Self>) -> Div {
        let snapshot = self.batch.as_ref().expect("batch page");
        let editable = !self.busy && snapshot.accepting_edits;
        div().flex().size_full().min_h_0()
            .child(div().flex_1().flex().flex_col().min_w_0().p_4().gap_3()
                .child(div().flex().flex_wrap().items_center().gap_2().child("Batch files")
                    .child(self.control("batch-add", "Add images", editable, false, Action::BatchAdd, cx))
                    .child(self.control("batch-remove-selected", "Remove selected", editable && snapshot.queue.iter().any(|record| record.selected), false, Action::Workflow(WorkflowAction::BatchRemoveSelected), cx))
                    .child(self.control("batch-clear", "Clear all", editable && !snapshot.queue.is_empty(), false, Action::Workflow(WorkflowAction::BatchClear), cx)))
                .child(div().id("batch-files").flex_1().min_h_0().overflow_y_scroll().children(snapshot.queue.iter().enumerate().map(|(index, record)| {
                    let outcome = self.workflows.batch_result.as_ref().and_then(|result| result.files.iter().find(|file| file.filename == record.filename));
                    let preview = self.workflows.thumbnails.get(&PathBuf::from(&record.filename));
                    let error = outcome.and_then(|file| file.error.as_ref())
                        .or_else(|| preview.and_then(|image| image.as_ref().err()));
                    div().flex().items_center().gap_2().min_h(px(36.0)).border_b_1().border_color(rgb(theme::BORDER))
                        .child(self.control(format!("select-{index}"), if record.selected { "Selected" } else { "Select" }, editable, record.selected, Action::Workflow(WorkflowAction::BatchSelect(index, !record.selected)), cx).aria_label(format!("Select {}", record.filename)))
                        .when_some(preview.and_then(|image| image.as_ref().ok()), |row, image|
                            row.child(img(image.clone()).w(px(48.0)).h(px(48.0)).object_fit(gpui::ObjectFit::Contain)))
                        .child(div().flex_1().min_w_0().flex().flex_col().child(record.filename.clone())
                            .when_some(outcome, |row, file| row.child(div().text_color(rgb(if file.error.is_some() { theme::DANGER } else { theme::MUTED }))
                                .child(format!("{} · {}", file.status, file.output_filename))))
                            .when_some(error, |row, error| row.child(div().text_color(rgb(theme::DANGER)).child(error.clone()))))
                        .child(self.control(format!("preview-{index}"), "Preview", !self.busy && preview.is_some(), false, Action::Workflow(WorkflowAction::Preview(record.filename.clone().into())), cx))
                        .child(self.control(format!("remove-{index}"), "Remove", editable, false, Action::BatchRemove(index), cx))
                })))
                .when_some(self.workflows.batch_result.as_ref(), |body, result| body
                    .child(div().child(format!("{} converted · {} failed · {} skipped", result.converted, result.failed, result.skipped)))
                    .child(self.control("batch-show-folder", "Show Export Folder", !self.busy, false,
                        Action::Workflow(WorkflowAction::OpenDirectory(result.queue.export_directory.clone().into())), cx)))
                .child(div().id("batch-log").max_h(px(180.0)).overflow_y_scroll().children(self.batch_log.iter().map(|line| div().child(line.clone())))))
            .child(div().w(px(theme::WIZARD_WIDTH)).h_full().flex().flex_col().border_l_1().border_color(rgb(theme::BORDER))
                .child(div().id("batch-settings").flex_1().min_h_0().overflow_y_scroll().p_4()
                    .child(self.check("advanced", "Advanced controls", self.settings.advanced, cx)).child(self.settings_panel(cx))
                    .child(self.batch_profiles_panel(cx))
                    .child(section("Export folder")).child(div().child(snapshot.export_directory.clone()))
                    .child(self.control("batch-folder", "Choose folder", !self.busy, false, Action::BatchFolder, cx))
                    .child(self.export_panel(cx)))
                .child(div().p_3().child(self.control("batch-start", "Start batch", !self.busy && !snapshot.queue.is_empty(), false, Action::BatchStart, cx))))
    }
    fn zoom_control(&self, cx: &Context<Self>) -> Stateful<Div> {
        let entity = cx.entity().downgrade();
        div().id("zoom-slider").role(Role::Slider).aria_label("Zoom")
            .aria_orientation(gpui::Orientation::Vertical).aria_min_numeric_value(0.0)
            .aria_max_numeric_value(300.0).aria_numeric_value(self.view.slider as f64).aria_numeric_value_step(1.0)
            .tab_index(0).tab_stop(self.document.is_some() && self.dialog.is_none())
            .relative().h(px(160.0)).w_full().flex_none().border_1().border_color(rgb(theme::SURFACE))
            .focus_visible(|style| style.border_color(rgb(theme::TEXT)))
            // Track and thumb sit in full-width rows centred by flex, so they stay
            // on the rail's centre line at any rail width.
            .child(div().absolute().left_0().right_0().top(px(4.0)).bottom(px(4.0)).flex().justify_center().child(div().w(px(2.0)).h_full().bg(rgb(theme::BORDER))))
            .child(div().absolute().left_0().right_0().top(px(4.0 + (300 - self.view.slider) as f32 / 300.0 * 144.0)).flex().justify_center()
                .child(div().w(px(14.0)).h(px(8.0)).rounded(px(2.0)).bg(rgb(theme::TEXT))))
            .child(canvas(move |bounds, _, cx| { let _ = entity.update(cx, |app, _| app.zoom_bounds = bounds); }, |_, _, _, _| {}).absolute().size_full())
            .on_mouse_down(MouseButton::Left, cx.listener(|app, event: &gpui::MouseDownEvent, _, cx| {
                if app.document.is_some() && app.dialog.is_none() { app.zoom_dragging = true; app.zoom_at(event.position.y); cx.notify(); }
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|app, event: &gpui::MouseMoveEvent, _, cx| { if app.zoom_dragging { app.zoom_at(event.position.y); cx.notify(); } }))
            .on_mouse_up(MouseButton::Left, cx.listener(|app, _, _, _| app.zoom_dragging = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|app, _, _, _| app.zoom_dragging = false))
            .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                let position = match event.keystroke.key.as_str() {
                    "up" | "right" => app.view.slider as i64 + 1, "down" | "left" => app.view.slider as i64 - 1,
                    "pageup" => app.view.slider as i64 + 10, "pagedown" => app.view.slider as i64 - 10,
                    "home" => 0, "end" => 300, _ => return,
                };
                app.view = zoom::slider(&app.view, position); cx.notify(); cx.stop_propagation();
            }))
    }
    fn zoom_at(&mut self, y: gpui::Pixels) {
        self.scrollbar_interaction = None;
        let ratio = (f32::from(y - self.zoom_bounds.origin.y) - 8.0) / 144.0;
        self.view = zoom::slider(&self.view, ((1.0 - ratio) * 300.0).round() as i64);
    }
    fn scrollbar_geometry(&self, axis: Axis) -> (f64, f64, f64) {
        let bar = self.view.bar(axis);
        let extent = if axis == Axis::Vertical { self.view.area.height } else { self.view.area.width };
        let range = bar.maximum - bar.minimum;
        let thumb = if range == 0.0 { extent } else { (extent * bar.page_step / (range + bar.page_step)).max(17.0).min(extent) };
        let offset = if range == 0.0 { 0.0 } else { (bar.value - bar.minimum) / range * (extent - thumb) };
        (extent, thumb, offset)
    }
    fn scrollbar_position(&self, axis: Axis, position: gpui::Point<gpui::Pixels>) -> f64 {
        match axis {
            Axis::Horizontal => f32::from(position.x - self.viewport.origin.x) as f64,
            Axis::Vertical => f32::from(position.y - self.viewport.origin.y) as f64,
        }
    }
    fn scrollbar_press(&mut self, axis: Axis, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.document.is_none() || self.dialog.is_some() || self.loading_source || self.batch.is_some() { return; }
        let position = self.scrollbar_position(axis, position);
        let (extent, thumb, offset) = self.scrollbar_geometry(axis);
        let bar = *self.view.bar(axis);
        self.scrollbar_interaction = None;
        if extent <= thumb { return; }
        self.dragging = None;
        self.pointer_buttons = 0;
        if position >= offset && position <= offset + thumb {
            // Anchor to the original value, not the rounded thumb pixels. This
            // preserves any grab offset and a zero-distance move exactly.
            self.scrollbar_interaction = Some(ScrollbarInteraction::Drag(ScrollbarDrag {
                axis, position, value: bar.value,
                units_per_pixel: (bar.maximum - bar.minimum) / (extent - thumb),
            }));
        } else {
            let page = ScrollbarPage { axis, position, forward: position > offset + thumb };
            self.scrollbar_page_step(page, cx);
            let repeat = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SCROLLBAR_REPEAT_DELAY).await;
                loop {
                    let moved = this.update(cx, |app, cx| {
                        let Some(ScrollbarInteraction::Page { page, .. }) = app.scrollbar_interaction.as_ref() else { return false; };
                        app.scrollbar_page_step(*page, cx)
                    }).unwrap_or(false);
                    if !moved { break; }
                    cx.background_executor().timer(SCROLLBAR_REPEAT_INTERVAL).await;
                }
            });
            // Retain ownership even after repeat stops at the pointer, so moving
            // the still-held button cannot become an image pan or edit.
            self.scrollbar_interaction = Some(ScrollbarInteraction::Page { page, _repeat: repeat });
        }
        cx.notify();
    }
    fn scrollbar_page_step(&mut self, page: ScrollbarPage, cx: &mut Context<Self>) -> bool {
        let (_, thumb, offset) = self.scrollbar_geometry(page.axis);
        if (page.forward && page.position <= offset + thumb) || (!page.forward && page.position >= offset) { return false; }
        let bar = *self.view.bar(page.axis);
        let value = bar.value + if page.forward { bar.page_step } else { -bar.page_step };
        let Some(view) = self.view.scroll(page.axis, value) else { return false; };
        if view.bar(page.axis).value == bar.value { return false; }
        self.view = view;
        cx.notify();
        true
    }
    fn scrollbar_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let axis = match self.scrollbar_interaction.as_ref() {
            Some(ScrollbarInteraction::Drag(drag)) => drag.axis,
            Some(ScrollbarInteraction::Page { page, .. }) => page.axis,
            None => return false,
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.scrollbar_interaction = None;
            return true;
        }
        let position = self.scrollbar_position(axis, event.position);
        match self.scrollbar_interaction.as_mut() {
            Some(ScrollbarInteraction::Drag(drag)) => {
                let value = drag.value + (position - drag.position) * drag.units_per_pixel;
                if let Some(view) = self.view.scroll(axis, value) { self.view = view; cx.notify(); }
            }
            Some(ScrollbarInteraction::Page { page, .. }) => page.position = position,
            None => {},
        }
        true
    }
    fn scrollbar(&self, vertical: bool, cx: &Context<Self>) -> Stateful<Div> {
        let axis = if vertical { Axis::Vertical } else { Axis::Horizontal };
        let bar = *self.view.bar(axis);
        let (_, thumb, offset) = self.scrollbar_geometry(axis);
        let mut track = div().id(if vertical { "vertical-scrollbar" } else { "horizontal-scrollbar" })
            .role(Role::ScrollBar).aria_label(if vertical { "Vertical image position" } else { "Horizontal image position" })
            .aria_numeric_value(bar.value).aria_min_numeric_value(bar.minimum).aria_max_numeric_value(bar.maximum)
            .aria_orientation(if vertical { gpui::Orientation::Vertical } else { gpui::Orientation::Horizontal })
            .tab_index(0).tab_stop(self.document.is_some() && self.dialog.is_none()).absolute().bg(rgb(theme::RAISED))
            .focus_visible(|style| style.bg(rgb(theme::HOVER)));
        track = if vertical {
            track.right_0().top_0().bottom(px(17.0)).w(px(17.0))
                .child(div().absolute().top(px(offset as f32)).left(px(5.0)).w(px(7.0)).h(px(thumb as f32)).bg(rgb(theme::BORDER)))
        } else {
            track.bottom_0().left_0().right(px(17.0)).h(px(17.0))
                .child(div().absolute().left(px(offset as f32)).top(px(5.0)).h(px(7.0)).w(px(thumb as f32)).bg(rgb(theme::BORDER)))
        };
        track.on_mouse_down(MouseButton::Left, cx.listener(move |app, event: &gpui::MouseDownEvent, _, cx| {
            app.scrollbar_press(axis, event.position, cx);
            cx.stop_propagation();
        })).on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            app.scrollbar_interaction = None;
            let bar = app.view.bar(axis);
            let value = match event.keystroke.key.as_str() {
                "left" | "up" => bar.value - bar.single_step, "right" | "down" => bar.value + bar.single_step,
                "pageup" => bar.value - bar.page_step, "pagedown" => bar.value + bar.page_step,
                "home" => bar.minimum, "end" => bar.maximum, _ => return,
            };
            if let Some(view) = app.view.scroll(axis, value) { app.view = view; cx.notify(); }
            cx.stop_propagation();
        }))
    }
    fn viewer(&mut self, cx: &Context<Self>) -> Div {
        let layer = self.input.snapshot().current_layer;
        let scene = self.document.as_ref().and_then(|doc| doc.display_vectors.clone());
        let removed = if self.wizard.page == Page::Background { self.document.as_ref().and_then(|doc| doc.display_removed.clone()) } else { None };
        let view = self.view.clone(); let paths = self.show_paths;
        let image = if layer == 1 { self.document.as_ref().and_then(|doc| doc.segments.clone()) } else { self.source_image.clone() };
        let measure = cx.entity().downgrade();
        let scrollbar_move = cx.entity().downgrade();
        let scrollbar_release = scrollbar_move.clone();
        let paint = canvas(move |bounds, _, cx| {
            let _ = measure.update(cx, |app, cx| {
                if app.viewport != bounds {
                    app.scrollbar_interaction = None;
                    app.viewport = bounds;
                    let area = Size::new(f32::from(bounds.size.width) as f64, f32::from(bounds.size.height) as f64);
                    let first = app.view.area.width == 0.0;
                    app.view = app.view.resized(area);
                    if first { app.fit(); }
                    cx.notify();
                }
            });
        }, move |bounds, _, window, _| {
            // Capture before canvas input, without hit-testing the track. The
            // held pointer may cross the image, sidebar or window boundary.
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                if phase == gpui::DispatchPhase::Capture {
                    let _ = scrollbar_move.update(cx, |app, cx| {
                        if app.scrollbar_move(event, cx) { cx.stop_propagation(); }
                    });
                }
            });
            window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                if phase == gpui::DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = scrollbar_release.update(cx, |app, cx| {
                        if app.scrollbar_interaction.take().is_some() { cx.stop_propagation(); }
                    });
                }
            });
            if layer == 2 {
                if let Some(scene) = scene { scene.paint(bounds, &view, paths, false, window); }
                if let Some(scene) = removed { scene.paint(bounds, &view, false, true, window); }
            }
        }).absolute().size_full();
        // Compact navigator: a D-pad for panning, then zoom and fit rows, sized for
        // the 84px rail (68px inside padding). Zero horizontal padding keeps the
        // icons centred in 20px cells.
        let enabled = self.document.is_some();
        let pad = |id: &'static str, icon: &'static str, name: &'static str, action: Action| {
            hint(self.control(id, "", enabled, false, action, cx).w(px(20.0)).h(px(20.0)).min_h(px(20.0)).px(px(0.0))
                .aria_label(name).debug_selector(move || id.into()).child(gpui::svg().path(icon).size(px(14.0)).text_color(rgb(theme::TEXT))), name)
        };
        let wide = |id: &'static str, label: &'static str, name: &'static str, action: Action| {
            hint(self.control(id, label, enabled, false, action, cx).flex_1().h(px(22.0)).min_h(px(22.0)).px(px(0.0))
                .aria_label(name).debug_selector(move || id.into()), name)
        };
        let cell = || div().w(px(20.0)).h(px(20.0));
        let navigator = div().w_full().flex().flex_col().items_center().gap(px(2.0))
            .child(div().flex().gap(px(2.0)).child(cell()).child(pad("pan-up", "arrow-up", "Pan up", Action::Step(Axis::Vertical, -1.0))).child(cell()))
            .child(div().flex().gap(px(2.0))
                .child(pad("pan-left", "arrow-left", "Pan left", Action::Step(Axis::Horizontal, -1.0)))
                .child(cell().flex().items_center().justify_center().child(div().size(px(4.0)).rounded_full().bg(rgb(theme::BORDER))))
                .child(pad("pan-right", "arrow-right", "Pan right", Action::Step(Axis::Horizontal, 1.0))))
            .child(div().flex().gap(px(2.0)).child(cell()).child(pad("pan-down", "arrow-down", "Pan down", Action::Step(Axis::Vertical, 1.0))).child(cell()))
            .child(div().mt_2().w_full().flex().gap(px(2.0))
                .child(wide("zoom-out", "−", "Zoom out", Action::Zoom(-10))).child(wide("zoom-in", "+", "Zoom in", Action::Zoom(10))))
            .child(div().w_full().flex().gap(px(2.0))
                // The zoom label is relative to Fit, so Fit is 100% and 1:1 is actual pixels.
                .child(wide("fit", "Fit", "Fit to window (100%)", Action::Fit)).child(wide("one", "1:1", "Actual pixels (1:1)", Action::One)));
        div().flex().flex_1().min_w_0().h_full()
            .child(div().id("viewer-tools").w(px(84.0)).flex_none().p_2().flex().flex_col().gap_1().overflow_y_scroll().border_r_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::SURFACE))
                .child(navigator)
                .child(self.zoom_control(cx))
                .child(div().flex_1())
                .child(hint(self.control("bitmap-layer", "Bitmap", self.document.is_some(), layer == 0, Action::Layer(0), cx).aria_label("Bitmap (hold B)"), "Show the source image. Hold B to compare."))
                .child(hint(self.control("segments-layer", "Regions", self.document.as_ref().is_some_and(|doc| doc.segments.is_some()), layer == 1, Action::Layer(1), cx).aria_label("Segmentation"), "Show the colored regions used for tracing."))
                .child(hint(self.control("vector-layer", "Vector", self.document.as_ref().is_some_and(|doc| doc.display_vectors.is_some()), layer == 2, Action::Layer(2), cx).aria_label("Vector (hold V)"), "Show the vector result. Hold V to compare."))
                .child(self.control("paths", "Paths", self.document.as_ref().is_some_and(|doc| doc.display_vectors.is_some()), paths, Action::Paths, cx)))
            .child(div().id("viewport").relative().flex_1().min_w_0().h_full().overflow_hidden().bg(rgb(theme::VIEWPORT))
                // Image paint and input share the content bounds, not the scrollbar strips.
                .child(div().id("image-viewport").absolute().inset_0().right(px(zoom::SCROLLBAR_EXTENT as f32)).bottom(px(zoom::SCROLLBAR_EXTENT as f32)).overflow_hidden()
                .on_mouse_down(MouseButton::Left, cx.listener(|app, event: &gpui::MouseDownEvent, window, cx| { app.focus.focus(window, cx); app.pointer("press", event.position, 1, cx); }))
                .on_mouse_down(MouseButton::Right, cx.listener(|app, event: &gpui::MouseDownEvent, _, cx| app.pointer("press", event.position, 2, cx)))
                .on_mouse_down(MouseButton::Middle, cx.listener(|app, event: &gpui::MouseDownEvent, _, cx| app.pointer("press", event.position, 4, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(|app, event: &gpui::MouseUpEvent, _, cx| app.pointer("release", event.position, 1, cx)))
                .on_mouse_up(MouseButton::Right, cx.listener(|app, event: &gpui::MouseUpEvent, _, cx| app.pointer("release", event.position, 2, cx)))
                .on_mouse_up(MouseButton::Middle, cx.listener(|app, event: &gpui::MouseUpEvent, _, cx| app.pointer("release", event.position, 4, cx)))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|app, event: &gpui::MouseUpEvent, _, cx| {
                    if app.pointer_buttons & 1 != 0 { app.pointer("release", event.position, 1, cx); }
                }))
                .on_mouse_move(cx.listener(|app, event: &gpui::MouseMoveEvent, _, cx| app.pointer_move(event, cx)))
                .on_scroll(cx.listener(|app, event: &gpui::ScrollEvent, _, cx| {
                    if app.document.is_none() { return; }
                    app.scrollbar_interaction = None;
                    let angle = zoom::wheel_angle(&event.delta);
                    let anchor = Point::new(f32::from(event.position.x - app.viewport.origin.x) as f64 - app.view.area.width / 2.0,
                        f32::from(event.position.y - app.viewport.origin.y) as f64 - app.view.area.height / 2.0);
                    app.view = zoom::wheel(&app.view, angle, anchor); cx.notify();
                }))
                .when(layer != 2, |d| d.children(image.map(|image| img(image).absolute().left(px(self.view.origin.x as f32)).top(px(self.view.origin.y as f32))
                    .w(px((self.view.image.width * self.view.zoom) as f32)).h(px((self.view.image.height * self.view.zoom) as f32)))))
                .child(paint)
                .when(self.document.is_none(), |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().text_color(rgb(theme::MUTED)).flex_col().gap_3().child(gpui::svg().path("image").size(px(40.0)).text_color(rgb(theme::MUTED)))
                    .child("Turn an image into editable vectors")
                    .child(self.control("empty-open", "Open image", !self.busy, false, Action::Open, cx))
                    .child("or drop an image here · Ctrl+O to browse"))))
                .child(self.scrollbar(false, cx)).child(self.scrollbar(true, cx))
                // 100% means the image fills the view (Fit); actual pixels use 1:1.
                .when(self.document.is_some(), |d| d.child(div().absolute().bottom(px(20.0)).right(px(20.0)).px_2().bg(rgb(theme::VIEWPORT)).text_color(rgb(theme::MUTED))
                    .font_family("Geist Mono").child(format!("{:.0}%", self.fit_relative_zoom() * 100.0)))))
    }
    /// Displayed zoom divided by the scale Fit would use for the current view.
    fn fit_relative_zoom(&self) -> f64 {
        let fit = (self.view.area.width / self.view.image.width).min(self.view.area.height / self.view.image.height);
        if fit.is_finite() && fit > 0.0 { self.view.zoom / fit } else { self.view.zoom }
    }
    fn modal(&self, cx: &Context<Self>) -> Option<Div> {
        let dialog = self.dialog.as_ref()?;
        if let Dialog::Form(form) = dialog { return Some(self.form_modal(form, cx)); }
        let (title, description) = match dialog {
            Dialog::About => ("About OpenVector", "OpenVector traces bitmap images into vector shapes you can edit, then saves them as SVG, PDF, EPS, AI, DXF or EMF.\n\nFree and open source under the MIT License.".to_string()),
            Dialog::PasteImage(_) => ("Replace image?", "Pasting a new image will discard your segmentation and background edits. Cancel to keep the current image.".to_string()),
            Dialog::BatchOverwrite(files) => ("Replace existing batch outputs?", files.join("\n")),
            Dialog::Form(_) => unreachable!("workflow dialogs return above"),
        };
        let actions = match dialog {
            // Actions sit on the right, the confirming one filled with the accent.
            Dialog::About => div().flex().justify_end().child(self.control("dialog-close", "Close", true, false, Action::Dismiss, cx).px_4()),
            Dialog::PasteImage(bytes) => div().flex().justify_end().gap_2()
                .child(self.control("paste-cancel", "Cancel", true, false, Action::Dismiss, cx).track_focus(&self.confirmation_cancel))
                .child(self.primary("paste-confirm", "Paste image", true, Action::ConfirmPaste(bytes.clone()), cx).px_4()),
            Dialog::BatchOverwrite(_) => div().flex().justify_end().gap_2().child(self.control("dialog-cancel", "Cancel", true, false, Action::BatchDecision(OverwriteDecision::Cancel), cx))
                .child(self.control("dialog-skip", "Skip existing", true, false, Action::BatchDecision(OverwriteDecision::Skip), cx).debug_selector(|| "dialog-skip".into()))
                .child(self.control("dialog-overwrite", "Overwrite", true, false, Action::BatchDecision(OverwriteDecision::Overwrite), cx)),
            Dialog::Form(_) => unreachable!("workflow dialogs return above"),
        };
        Some(div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000099)).occlude().child(
            div().id("dialog").role(Role::Dialog).aria_label(title).track_focus(&self.dialog_focus).tab_index(0).tab_stop(false).occlude()
                .w(px(440.0)).max_h(px(480.0)).p_4().rounded(px(theme::DIALOG_RADIUS)).border_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::RAISED))
                .child(div().text_size(px(18.0)).child(title)).child(div().id("dialog-description").my_4().max_h(px(300.0)).overflow_y_scroll().child(description)).child(actions)))
    }
}

impl Focusable for OpenVectorApp { fn focus_handle(&self, _: &App) -> FocusHandle { self.focus.clone() } }
impl Render for OpenVectorApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Quick operations (most option changes) finish well inside BUSY_GRACE.
        // Painting them as idle avoids the whole UI dimming and the status line
        // flashing "Processing" for one frame. Clicks are still refused while busy.
        let grace = self.busy && self.busy_since.as_ref().is_some_and(|(since, _)| since.elapsed() < BUSY_GRACE);
        let held = grace.then(|| {
            let status = self.busy_since.as_ref().map(|(_, status)| status.clone()).unwrap_or_default();
            self.busy = false;
            std::mem::replace(&mut self.status, status)
        });
        let element = self.render_frame(window, cx);
        if let Some(status) = held { self.busy = true; self.status = status; }
        element
    }
}
impl OpenVectorApp {
    fn render_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.activation.is_none() {
            self.activation = Some(cx.observe_window_activation(window, |app, window, cx| {
                if !window.is_window_active() { app.input.release_keys(); cx.notify(); }
            }));
        }
        // Images removed since the last frame cannot be painted by this one.
        // Some(window) also reaches this window, which is leased during render.
        for image in self.workflows.released_images.drain(..) { cx.drop_image(image, Some(window)); }
        if self.dialog.is_some() || self.batch.is_some() || self.loading_source || self.document.is_none() {
            self.scrollbar_interaction = None;
        }
        if self.dialog.is_some() && self.restore_focus.is_none() {
            self.restore_focus = window.focused(cx); self.dialog_focus.focus(window, cx);
        }
        let confirmation_message = match &self.dialog {
            Some(Dialog::Form(workflows::Form::Confirm(message, _))) => Some(message.clone()),
            Some(Dialog::PasteImage(_)) => Some("Replace image with clipboard image".into()),
            _ => None,
        };
        if confirmation_message.is_some() && confirmation_message != self.confirmation_message { self.confirmation_cancel.focus(window, cx); }
        self.confirmation_message = confirmation_message;
        let filename = self.document.as_ref().map(|doc| {
            let title = if doc.origin == SourceOrigin::Clipboard { "[From clipboard]".into() } else { doc.filename.file_name().unwrap_or_default().to_string_lossy() };
            format!("{title}  ·  {} × {}", doc.width, doc.height)
        }).unwrap_or_default();
        let content = if self.batch.is_some() { self.batch_panel(cx) } else { div().flex().size_full().min_h_0().child(self.viewer(cx)).child(self.wizard_panel(cx)) };
        div().id("openvector").track_focus(&self.focus).key_context("OpenVector").size_full().relative().flex().flex_col()
            .font_family("Geist").text_size(px(theme::UI_FONT_SIZE)).text_color(rgb(theme::TEXT)).bg(rgb(theme::BACKGROUND))
            .on_key_down(cx.listener(Self::key_down)).on_key_up(cx.listener(Self::key_up))
            .on_external_drag_event(cx.listener(Self::drag_event))
            .on_mouse_up(MouseButton::Left, cx.listener(|app, _, _, cx| { app.internal_drag_released(cx); }))
            .on_modifiers_changed(cx.listener(|app, event: &gpui::ModifiersChangedEvent, _, cx| {
                if event.modifiers.shift != app.input.snapshot().shift_held {
                    app.viewer_key(input::KEY_SHIFT, event.modifiers, if event.modifiers.shift { KeyPhase::Press } else { KeyPhase::Release }, cx);
                }
            }))
            .on_drop(cx.listener(|app, paths: &ExternalPaths, _, cx| {
                if app.busy { return; }
                if app.batch.is_some() {
                    let paths = paths.0.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>();
                    app.add_batch_files(&paths, cx);
                } else if let Some(path) = paths.0.first() { app.open(path.clone(), cx); }
                cx.notify();
            }))
            // The logo is the brand mark; file actions sit beside it. Paste stays on
            // Ctrl+V and drag-and-drop, so it no longer needs a header button.
            .child(div().h(px(theme::HEADER_HEIGHT)).flex_none().px_3().flex().items_center().gap_2().border_b_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::SURFACE))
                .child(gpui::svg().path("logo").size(px(20.0)).flex_none().mr_3().text_color(rgb(theme::TEXT)))
                .child(self.control("open", "Open", !self.busy, false, Action::Open, cx))
                .child(self.control("batch", if self.batch.is_some() { "Single image" } else { "Batch" }, !self.busy, self.batch.is_some(), Action::Batch, cx))
                .child(self.control("export", "Export", !self.busy && self.batch.is_none() && self.document.as_ref().is_some_and(|doc| doc.vectors.is_some()), false, Action::Export, cx))
                .child(div().flex_1())
                .child(self.control("reset-application", "Reset application", !self.busy, false, Action::Workflow(WorkflowAction::ResetApplication), cx))
                .child(self.control("about", "About", !self.busy, false, Action::About, cx)))
            .child(div().flex_1().min_h_0().child(content))
            // Status bar: the open file, then what the app is doing.
            .child(div().h(px(theme::FOOTER_HEIGHT)).flex_none().px_3().flex().items_center().gap_3().border_t_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::SURFACE))
                .when(!filename.is_empty(), |d| d.child(div().flex_none().max_w(px(360.0)).truncate().child(filename)).child(div().text_color(rgb(theme::MUTED)).child("·")))
                .child(div().id("status").role(if self.error { Role::Alert } else { Role::Status }).aria_label(self.status.clone()).flex_1().min_w_0().truncate().text_color(rgb(if self.error { theme::DANGER } else { theme::MUTED })).child(self.status.clone()))
                .when(self.busy, |d| d.child(div().w(px(120.0)).h(px(2.0)).bg(rgb(theme::BORDER)).child(div().w(px((self.progress.clamp(0.0, 100.0) * 1.2) as f32)).h_full().bg(rgb(theme::ACCENT))))
                    .child(self.control("cancel", "Cancel", true, false, Action::Cancel, cx))))
            .children(self.modal(cx))
    }
}

#[cfg(test)]
#[path = "app_advanced_tests.rs"]
mod advanced_tests;

#[cfg(test)]
#[path = "app-save-tests.rs"]
mod save_tests;

#[cfg(test)]
#[path = "app-input-tests.rs"]
mod input_tests;

#[cfg(test)]
#[path = "app-palette-transition-tests.rs"]
mod palette_transition_tests;

#[cfg(test)]
#[path = "app-source-sizing-tests.rs"]
mod source_sizing_tests;

