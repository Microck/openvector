use crate::engine_error;
use crate::snapshot::{self, ProgressOptions, SnapshotSelection};
use napi::{Env, Result, bindgen_prelude::{Buffer, Function, Object}};
use napi_derive::napi;
use serde_json::{Value, json};
use openvector::image_document::{DocumentProgress, DocumentSnapshot, DocumentObserver,
    AutomaticSelection, DocumentStageObserver, TaskResult};

fn notify(env: Env, options: &ProgressOptions<'_>,
    event: &DocumentProgress, state: &DocumentSnapshot) -> std::result::Result<(), String>
{
    let event = json!({
        "stage": event.stage, "phase": event.phase, "substep": event.substep,
        "counter": event.counter, "fraction": event.fraction, "percent": event.percent,
        "colorCount": event.color_count, "iteration": event.iteration,
    });
    let checkpoint = snapshot::capture(env, state, &options.capture).map_err(|error| error.to_string())?;
    if options.callback.call((event, checkpoint).into()).map_err(|error| error.to_string())? {
        Ok(())
    } else { Err(openvector::image_document::CANCEL_TASK.to_owned()) }
}

/// Keeps the fitting task's own status and work flag from the shared stage
/// observer; callers pass them to `RecentBasicProfiles.recordCompletion`.
fn observe_fitting(fitting: &mut Option<(u32, bool)>, stage: &str, task: &TaskResult)
    -> std::result::Result<(), String>
{
    if stage == "fitting" { *fitting = Some((task.state.status, task.did_work)); }
    Ok(())
}

/// Every successful automatic, batch-image and finish-editing run reports its
/// fitting stage, so a missing completion means the engine contract changed.
fn fitting_completion(fitting: Option<(u32, bool)>) -> Result<Value> {
    let (status, did_work) = fitting
        .ok_or_else(|| engine_error("Document processing finished without a fitting completion."))?;
    Ok(json!({"status": status, "didWork": did_work}))
}

/// Automatic and batch-image processing share observers and result shape;
/// `batch` selects the engine path.
fn process_image(env: Env, document: &mut openvector::image_document::ImageDocument, batch: bool, controls: Value,
    progress: Option<Object<'_>>) -> Result<Value>
{
    let progress = progress.map(ProgressOptions::read).transpose()?;
    let mut observer = |event: &DocumentProgress, state: &DocumentSnapshot| {
        notify(env, progress.as_ref().expect("observer only installed with callback"), event, state)
    };
    let observer = if progress.is_some() {
        Some(&mut observer as &mut DocumentObserver<'_>)
    } else { None };
    let mut fitting = None;
    let mut on_stage = |stage: &str, task: &TaskResult, _: &AutomaticSelection| observe_fitting(&mut fitting, stage, task);
    let on_stage = Some(&mut on_stage as &mut DocumentStageObserver<'_>);
    let result = if batch {
        document.run_batch_image_with_observers(&controls, observer, on_stage)
    } else {
        document.run_automatic_with_observers(&controls, observer, on_stage)
    };
    let result = result.map_err(engine_error)?;
    let fitting = fitting_completion(fitting)?;
    Ok(json!({
        "settings": result.settings.to_json(), "paletteCount": result.palette_count,
        "sourceAlphaType": result.source_alpha_type,
        "recommendedBackgroundMode": result.recommended_background_mode,
        "state": snapshot::summary(&result.state), "fitting": fitting,
    }))
}

/// Advanced and recent-basic replay share the engine pipeline and its result
/// shape; `basic` selects the runner. `None` means the palette mode does not process.
fn replay_profile(env: Env, document: &mut openvector::image_document::ImageDocument, basic: bool, profile: Value,
    runtime: Value, progress: Option<Object<'_>>) -> Result<Option<Value>>
{
    use openvector::settings_profile::{read_settings_profile, run_advanced_profile, run_basic_profile};
    let profile = read_settings_profile(&profile).map_err(engine_error)?;
    let runtime = openvector::profile_palette::read_palette_runtime(&runtime).map_err(engine_error)?;
    let progress = progress.map(ProgressOptions::read).transpose()?;
    let mut observer = |event: &DocumentProgress, state: &DocumentSnapshot| {
        notify(env, progress.as_ref().expect("observer only installed with callback"), event, state)
    };
    let observer = if progress.is_some() {
        Some(&mut observer as &mut DocumentObserver<'_>)
    } else { None };
    let result = if basic {
        run_basic_profile(document, &profile, &runtime, observer, None)
    } else {
        run_advanced_profile(document, &profile, &runtime, observer, None)
    };
    Ok(result.map_err(engine_error)?.map(|result| json!({
        "profile": result.profile.to_json(),
        "paletteRuntime": {"backup": result.palette_runtime.backup, "customized": result.palette_runtime.customized},
        "state": snapshot::summary(&result.state), "paletteRequested": result.palette_requested,
        "didWork": result.did_work,
    })))
}

#[napi(object)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Buffer,
    pub has_alpha: bool,
    pub format: String,
}

#[napi]
pub fn decode_image(encoded: Buffer) -> Result<DecodedImage> {
    let image = openvector::image_input::decode_image(&encoded).map_err(engine_error)?;
    Ok(DecodedImage {
        width: image.width as u32, height: image.height as u32,
        bgra: image.bgra.into(), has_alpha: image.has_alpha, format: image.format.to_owned(),
    })
}

#[napi]
pub struct CoreSession {
    inner: openvector::palette::CoreSession,
}

#[napi]
impl CoreSession {
    #[napi(constructor)]
    pub fn new() -> Self { Self { inner: openvector::palette::CoreSession::default() } }

    #[napi]
    pub fn random_state(&self) -> u32 { self.inner.random_state() }

    #[napi]
    pub fn create_document(&self) -> ImageDocument {
        ImageDocument { inner: openvector::image_document::ImageDocument::new(self.inner.clone()),
            session: self.inner.clone() }
    }

    #[napi]
    pub fn create_batch(&self, application_directory: String, working_directory: String) -> crate::batch::BatchRunner {
        crate::batch::create(self.inner.clone(), &application_directory, &working_directory)
    }
}

#[napi]
pub struct ImageDocument {
    inner: openvector::image_document::ImageDocument,
    // Refresh the shared PRNG without cloning all rasters through core snapshot().
    session: openvector::palette::CoreSession,
}

#[napi]
impl ImageDocument {
    #[napi]
    pub fn set_source(&mut self, encoded: Buffer, filename: String) -> Result<()> {
        let image = openvector::image_input::decode_image(&encoded).map_err(engine_error)?;
        let name = openvector::image_input::resolve_image_filename(&filename);
        self.inner.set_source(image.width, image.height, &image.bgra, image.has_alpha,
            f64::from(name.shadow_control)).map_err(engine_error)
    }

    #[napi]
    pub fn shrink_source(&mut self) -> bool { self.inner.shrink_source() }

    #[napi]
    pub fn commit_settings(&mut self, settings: Value) -> Result<()> {
        self.inner.commit_settings(&settings).map_err(engine_error)
    }

    #[napi]
    pub fn snapshot<'env>(&self, env: Env, selection: Option<SnapshotSelection>) -> Result<Object<'env>> {
        let selection = selection.unwrap_or_else(SnapshotSelection::complete);
        selection.validate()?;
        let mut captured = snapshot::capture(env, self.inner.state(), &selection)?;
        captured.set("randomState", self.session.random_state())?;
        captured.set("cancelled", self.inner.cancellation_handle().load(std::sync::atomic::Ordering::Relaxed))?;
        Ok(captured)
    }

    /// A synchronous progress callback returns false to cancel at a native checkpoint.
    #[napi]
    pub fn run_task(&mut self, env: Env, stage: String, settings: Value,
        progress: Option<Object<'_>>) -> Result<Value>
    {
        let progress = progress.map(ProgressOptions::read).transpose()?;
        let mut observer = |event: &DocumentProgress, state: &DocumentSnapshot| {
            notify(env, progress.as_ref().expect("observer only installed with callback"), event, state)
        };
        let observer = if progress.is_some() {
            Some(&mut observer as &mut DocumentObserver<'_>)
        } else { None };
        let result = self.inner.run_task(&stage, &settings, observer).map_err(engine_error)?;
        Ok(json!({
            "didWork": result.did_work, "settings": result.settings.to_json(),
            "state": snapshot::summary(&result.state),
        }))
    }

    #[napi]
    pub fn run_automatic(&mut self, env: Env, controls: Value,
        progress: Option<Object<'_>>) -> Result<Value>
    {
        process_image(env, &mut self.inner, false, controls, progress)
    }

    #[napi]
    pub fn run_batch_image(&mut self, env: Env, controls: Value,
        progress: Option<Object<'_>>) -> Result<Value>
    {
        process_image(env, &mut self.inner, true, controls, progress)
    }

    #[napi]
    pub fn run_advanced_profile(&mut self, env: Env, profile: Value, runtime: Value,
        progress: Option<Object<'_>>) -> Result<Option<Value>>
    {
        replay_profile(env, &mut self.inner, false, profile, runtime, progress)
    }

    /// Replays a recent basic profile with exact palette bounds and no advanced
    /// zero-count fallback. Return false from progress to cancel.
    #[napi]
    pub fn run_basic_profile(&mut self, env: Env, profile: Value, runtime: Value,
        progress: Option<Object<'_>>) -> Result<Option<Value>>
    {
        replay_profile(env, &mut self.inner, true, profile, runtime, progress)
    }

    #[napi]
    pub fn begin_editing(&mut self) -> Result<Value> {
        self.inner.begin_editing().map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn editing<'env>(&self, env: Env) -> Result<Option<Object<'env>>> {
        self.inner.editing_state().map(|editor| snapshot::editor(env, editor)).transpose()
    }

    #[napi]
    pub fn editing_history(&self) -> Value {
        json!(self.inner.editing_history())
    }

    #[napi]
    pub fn leave_editing(&mut self) -> Result<()> {
        self.inner.leave_editing().map_err(engine_error)
    }

    #[napi]
    pub fn restore_committed_editing(&mut self) -> Result<Value> {
        self.inner.restore_committed_editing().map(|editor| editor.as_ref().map(snapshot::editor_summary).unwrap_or(Value::Null)).map_err(engine_error)
    }

    #[napi]
    pub fn edit(&mut self, command: Value) -> Result<Value> {
        self.inner.edit(&command).map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn gesture(&mut self, event: Value) -> Result<Value> {
        self.inner.gesture(&event).map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn undo_edit(&mut self) -> Result<Value> {
        self.inner.undo_edit().map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn redo_edit(&mut self) -> Result<Value> {
        self.inner.redo_edit().map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn revert_edits(&mut self) -> Result<Value> {
        self.inner.revert_edits().map(|editor| snapshot::editor_summary(&editor)).map_err(engine_error)
    }

    #[napi]
    pub fn commit_editing(&mut self) -> Result<Value> {
        self.inner.commit_editing().map(|state| snapshot::summary(&state)).map_err(engine_error)
    }

    /// Returns compact metadata plus the fitting task's own completion.
    #[napi]
    pub fn finish_editing(&mut self, env: Env, settings: Value,
        progress: Option<Object<'_>>) -> Result<Value>
    {
        let progress = progress.map(ProgressOptions::read).transpose()?;
        let mut observer = |event: &DocumentProgress, state: &DocumentSnapshot| {
            notify(env, progress.as_ref().expect("observer only installed with callback"), event, state)
        };
        let observer = if progress.is_some() {
            Some(&mut observer as &mut DocumentObserver<'_>)
        } else { None };
        let mut fitting = None;
        let mut on_stage = |stage: &str, task: &TaskResult| observe_fitting(&mut fitting, stage, task);
        let state = self.inner.finish_editing_with_observers(&settings, observer,
            Some(&mut on_stage as &mut dyn FnMut(&str, &TaskResult) -> std::result::Result<(), String>))
            .map_err(engine_error)?;
        let mut finished = snapshot::summary(&state);
        finished["fittingCompletion"] = fitting_completion(fitting)?;
        Ok(finished)
    }

    #[napi]
    pub fn begin_background_editing(&mut self) -> Result<Value> {
        self.inner.begin_background_editing().map(|state| snapshot::background_summary(&state)).map_err(engine_error)
    }

    #[napi]
    pub fn background<'env>(&self, env: Env) -> Result<Object<'env>> {
        snapshot::background(env, self.inner.background_state())
    }

    #[napi]
    pub fn toggle_background(&mut self, x: f64, y: f64) -> Result<Value> {
        self.inner.toggle_background([x, y]).map(|state| snapshot::background_summary(&state)).map_err(engine_error)
    }

    #[napi]
    pub fn background_gesture(&mut self, event: Value) -> Result<Value> {
        self.inner.background_gesture(&event).map(|state| snapshot::background_summary(&state)).map_err(engine_error)
    }

    #[napi]
    pub fn undo_background(&mut self) -> Result<Value> {
        Ok(snapshot::background_summary(&self.inner.undo_background()))
    }

    #[napi]
    pub fn redo_background(&mut self) -> Result<Value> {
        Ok(snapshot::background_summary(&self.inner.redo_background()))
    }

    #[napi]
    pub fn revert_background(&mut self) -> Result<Value> {
        Ok(snapshot::background_summary(&self.inner.revert_background()))
    }

    #[napi]
    pub fn finish_background_editing(&mut self) -> Result<Value> {
        Ok(snapshot::background_summary(&self.inner.finish_background_editing()))
    }

    #[napi]
    pub fn export_bitmap(&mut self, format: String, scale: Option<f64>, flatten: Option<bool>) -> Result<Buffer, String> {
        use openvector::bitmap_export::{BitmapExportError, BitmapExportInput, BitmapExportOptions, export_bitmap};
        let error = |error: BitmapExportError| napi::Error::new(
            error.code().unwrap_or("GenericFailure").to_owned(), error.to_string());
        let format = format.parse().map_err(error)?;
        let source_alpha_type = self.inner.source_alpha_type();
        let background = self.inner.background_state();
        let hidden_regions: Vec<i64> = background.hidden_regions.iter().map(|&region| region as i64).collect();
        let state = self.inner.state();
        let input = BitmapExportInput {
            geometry: state.fitting.as_ref().filter(|fitting| fitting.complete).map(|fitting| &fitting.geometry),
            width: state.original.width as i64, height: state.original.height as i64,
            source_alpha_type, background_mode: state.settings.background_mode,
            background_undo_count: background.undo.len(), hidden_regions: &hidden_regions,
        };
        export_bitmap(&input, &BitmapExportOptions { format, scale: scale.unwrap_or(1.0), flatten: flatten.unwrap_or(false) })
            .map(Buffer::from).map_err(error)
    }

    #[napi]
    pub fn save_vector(&mut self, filename: String, options: Value,
        progress: Option<Function<'_, i32, bool>>) -> Result<Value>
    {
        let options = openvector::export_options::read_export_options(
            &options, openvector::export_options::ExportOptions::default()).map_err(engine_error)?;
        let mut observer = |percent| {
            let callback = progress.as_ref().expect("observer only installed with callback");
            if callback.call(percent).map_err(|error| error.to_string())? {
                Ok(())
            } else { Err(openvector::image_document::CANCEL_TASK.to_owned()) }
        };
        let observer = if progress.is_some() {
            Some(&mut observer as &mut dyn FnMut(i32) -> std::result::Result<(), String>)
        } else { None };
        let result = self.inner.save_vector_file(std::path::Path::new(&filename), &options, observer)
            .map_err(engine_error)?;
        Ok(json!({"status": result.status, "error": result.error.map(|error|
            json!({"message": error.to_string(), "osError": error.raw_os_error()}))}))
    }

    #[napi]
    pub fn reset(&mut self) { self.inner.reset(); }
}
