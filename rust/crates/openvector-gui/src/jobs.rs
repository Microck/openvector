use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc}, thread};
use openvector::{batch_queue::{BatchQueue, BatchSnapshot, OverwriteDecision}, document_controls::DocumentSettings, image_document::ImageDocument, palette::CoreSession};
use gpui::RenderImage;
use serde_json::Value;
use crate::viewer::paint::Scene;

#[derive(Clone, Debug)]
pub struct Bitmap { pub width: usize, pub height: usize, pub bgra: Vec<u8> }
impl Bitmap {
    /// Nearest-sampled display preview, at most 360 pixels on the long side.
    /// Queue rows, recent-source links and the Preview dialog share this one projection.
    pub fn preview(width: usize, height: usize, bgra: &[u8]) -> Self {
        let scale = (360.0 / width.max(height) as f64).min(1.0);
        let preview_width = (width as f64 * scale).round().max(1.0) as usize; let preview_height = (height as f64 * scale).round().max(1.0) as usize;
        let mut pixels = Vec::with_capacity(preview_width * preview_height * 4);
        for y in 0..preview_height { for x in 0..preview_width { let offset = ((y * height / preview_height) * width + x * width / preview_width) * 4; pixels.extend_from_slice(&bgra[offset..offset + 4]); } }
        Self { width: preview_width, height: preview_height, bgra: pixels }
    }
    /// Moves the pixels into a paintable GPUI image without copying them.
    /// The owner must hand a replaced image to `App::drop_image`; dropping the
    /// Arc alone leaves its texture in every window's sprite atlas.
    pub fn into_render_image(self) -> Arc<RenderImage> {
        // GPUI's RenderImage uses BGRA byte order despite image::RgbaImage's name.
        let image = image::RgbaImage::from_raw(self.width as u32, self.height as u32, self.bgra).expect("engine dimensions match BGRA bytes");
        Arc::new(RenderImage::new([image::Frame::new(image)]))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceOrigin { Path, Clipboard }
#[derive(Clone, Debug)]
pub struct DocumentView {
    pub origin: SourceOrigin,
    /// Disk source path for Path; export-name suggestion only for Clipboard.
    pub filename: PathBuf,
    pub width: usize,
    pub height: usize,
    pub has_alpha: bool,
    pub source_alpha_type: u8,
    /// Exact document-owned stage keys, including the applied segmentation edit key.
    pub stage_keys: openvector::image_document::StageKeys,
    /// Live editor key, distinct from the applied segmentation edit key.
    pub editor_key: Option<String>,
    pub has_committed_edits: bool,
    pub has_applied_edits: bool,
    /// Engine comparison against the owned Back checkpoint, ignoring redo.
    pub editing_changed: bool,
    pub palettes: Vec<Vec<f32>>,
    /// Last committed controller count; not part of engine DocumentSettings.
    pub committed_count: i32,
    /// Exact last committed engine controls, not the selected GUI controls.
    pub settings: DocumentSettings,
    /// Shared worker-built source image; the UI never copies its full pixel buffer.
    pub source: Arc<RenderImage>,
    /// Segmentation layer, converted on the worker so each edit gesture moves
    /// its pixels into the viewer instead of copying them on the UI thread.
    pub segments: Option<Arc<RenderImage>>,
    /// Last completed scene with current visibility, display-only while fitting is unavailable.
    pub display_vectors: Option<Arc<Scene>>,
    pub display_removed: Option<Arc<Scene>>,
    /// Current computation only; retained display geometry must never enable export or hit testing.
    pub vectors: Option<Arc<Scene>>,
    pub removed: Option<Arc<Scene>>,
    pub undo: bool,
    pub redo: bool,
    pub edit_color: [f32; 4],
    pub edit_tool: i32,
}
#[derive(Clone, Debug)]
pub enum Command {
    Open { path: PathBuf, settings: Value },
    PasteImage { bytes: Arc<Vec<u8>>, directory: PathBuf, settings: Value },
    Reset(Value),
    Commit(Value),
    /// One-time Mode admission, before the selected processing/navigation command.
    PrepareSource { shrink: bool },
    /// Basic source admission analyzes recommendations without running vectorization.
    AnalyzeSource(Value),
    Process { stage: &'static str, input: Value },
    Automatic(Value),
    Edit { action: &'static str, input: Value },
    Background { action: &'static str, input: Value },
    Export { path: PathBuf, options: Value },
    PrepareDrag { request: u64, directory: PathBuf, format: &'static str, options: Value },
    BitmapExport { path: PathBuf, format: String, scale: f64, flatten: bool },
    /// Replays a stored profile through the shared engine runner: a saved
    /// advanced profile (Fast forward) or a Recent Basic Settings entry.
    Profile { profile: openvector::settings_profile::SettingsProfile, basic: bool },
    Batch { queue: BatchQueue, profile: openvector::settings_profile::SettingsProfile, options: Value },
}
#[derive(Debug)]
pub enum Event {
    Progress { stage: String, percent: f64 },
    Document(DocumentView),
    DocumentCleared,
    SourcePrepared,
    Selection { settings: DocumentSettings, color_count: Option<usize>, recommended_background_mode: i32, palette_ready: bool, classified: bool },
    /// Status and work flag of the command's completed single-image fitting task.
    /// It follows the command's final Selection, so the GUI records the settings
    /// that produced the fitting. Batch processing never sends it.
    Fitting { status: u32, did_work: bool },
    /// A Recent Basic replay whose palette mode selects no processing. As in
    /// native vectorizeImageWithMruSettings, its controls stay selected and the
    /// requested page does not open.
    ReplayUnprocessed,
    BatchSelection { profile: openvector::settings_profile::SettingsProfile, palette_ready: bool },
    /// Preview of the source a batch load just admitted, keyed by its exact queue filename.
    BatchPreview { filename: String, preview: Bitmap },
    BatchState(BatchSnapshot),
    BatchLog(String),
    BatchResults(openvector::batch_runner::BatchResult),
    Overwrite(Vec<String>),
    DragPrepared { request: u64, result: Result<Arc<crate::drag_result::DragFile>, String> },
    /// Basic recommendations are admitted atomically with successful completion.
    /// Automatic classification is admitted atomically at its Selection stage.
    Finished(Result<Option<[i32; 3]>, String>),
}

/// The document and its Rc-owned random stream never leave this thread.
/// Only owned presentation snapshots cross into GPUI.
pub struct Worker {
    commands: mpsc::Sender<Command>,
    pub events: mpsc::Receiver<Event>,
    cancellation: Arc<AtomicBool>,
    document_cancellation: Arc<AtomicBool>,
    decisions: mpsc::Sender<OverwriteDecision>,
}
impl Worker {
    pub fn start(initial: openvector::settings_profile::SettingsProfile) -> Result<Self, String> {
        let (commands, incoming) = mpsc::channel();
        let (outgoing, events) = mpsc::channel();
        let (decisions, decision_rx) = mpsc::channel();
        let (ready, document_ready) = mpsc::sync_channel(1);
        let cancellation = Arc::new(AtomicBool::new(false));
        let cancelled = cancellation.clone();
        thread::Builder::new().name("openvector-document".into()).spawn(move || {
            let mut document = ImageDocument::new(CoreSession::default());
            // MainModel commits the startup selection before the first source.
            // Do this before readiness, so it cannot race a later Open command.
            let cancellation = match document.commit_settings(&Value::Object(initial.settings.to_json())) {
                Ok(()) => document.cancellation_handle(),
                Err(error) => { let _ = ready.send(Err(error)); return; }
            };
            if ready.send(Ok(cancellation)).is_err() { return; }
            crate::engine_worker::run(document, initial.color_count, incoming, outgoing, cancelled, decision_rx);
        }).map_err(|error| format!("Cannot start image worker: {error}"))?;
        let document_cancellation = document_ready.recv().map_err(|_| "The image worker stopped during startup.".to_string())??;
        Ok(Self { commands, events, cancellation, document_cancellation, decisions })
    }
    pub fn submit(&self, command: Command) -> Result<(), String> {
        self.cancellation.store(false, Ordering::Release);
        self.commands.send(command).map_err(|_| "Image worker stopped unexpectedly.".into())
    }
    pub fn cancellation_requested(&self) -> bool { self.cancellation.load(Ordering::Acquire) }
    pub fn cancel(&self) {
        // The completion latch survives the document's per-task cancellation resets.
        self.cancellation.store(true, Ordering::Release);
        self.document_cancellation.store(true, Ordering::Relaxed);
    }
    pub fn decide(&self, decision: OverwriteDecision) -> Result<(), String> {
        self.decisions.send(decision).map_err(|_| "Image worker stopped before the overwrite decision.".into())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        // Unblock a worker waiting for an overwrite answer before its channel closes.
        let _ = self.decisions.send(OverwriteDecision::Cancel);
    }
}
