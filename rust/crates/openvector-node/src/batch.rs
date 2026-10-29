use crate::engine_error;
use crate::snapshot::{self, ProgressOptions};
use napi::{Env, Result, bindgen_prelude::{Function, Object}};
use napi_derive::napi;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use openvector::batch_queue::{BatchQueue, OverwriteDecision};

#[napi]
pub struct BatchRunner {
    pub(crate) inner: openvector::batch_runner::BatchRunner,
}

#[napi]
impl BatchRunner {
    #[napi]
    pub fn add_files(&mut self, filenames: Vec<String>) -> Vec<String> {
        self.inner.queue.add_files(&filenames)
    }

    #[napi]
    pub fn select(&mut self, index: u32, selected: bool) -> Result<()> {
        self.inner.queue.select(index as usize, selected).map_err(engine_error)
    }

    #[napi]
    pub fn remove(&mut self, index: u32) -> Result<()> {
        self.inner.queue.remove(index as usize).map_err(engine_error)
    }

    #[napi]
    pub fn remove_selected(&mut self) { self.inner.queue.remove_selected(); }

    #[napi]
    pub fn clear(&mut self) { self.inner.queue.clear(); }

    #[napi]
    pub fn set_export(&mut self, directory: String, format: String) {
        self.inner.queue.export_directory = directory;
        self.inner.queue.export_format = format;
    }

    #[napi]
    pub fn snapshot<'env>(&self, env: Env) -> Result<Object<'env>> {
        let (running, queue, document) = self.inner.snapshot();
        let mut object = Object::new(&env)?;
        object.set("running", running)?;
        object.set("queue", snapshot::queue(env, queue)?)?;
        object.set("document", document.as_ref().map(snapshot::summary))?;
        Ok(object)
    }

    /// Calls are synchronous. Return false from progress to request cancellation.
    /// Overwrite decisions are explicit: overwrite, skip or cancel.
    #[napi]
    pub fn run<'env>(&mut self, env: Env, settings: Value, export_options: Value,
        overwrite: Option<Function<'_, Vec<String>, String>>,
        progress: Option<Object<'env>>) -> Result<Object<'env>>
    {
        let progress = progress.map(ProgressOptions::read).transpose()?;
        let cancellation = self.inner.cancellation_handle();
        cancellation.store(false, Ordering::Relaxed);
        let mut callback_error = None;
        let mut decide = |filenames: &[String]| {
            let Some(callback) = &overwrite else { return OverwriteDecision::Skip; };
            match callback.call(filenames.to_vec())
                .and_then(|decision| OverwriteDecision::try_from(decision.as_str()).map_err(engine_error)) {
                Ok(decision) => decision,
                Err(error) => { callback_error = Some(error); OverwriteDecision::Cancel }
            }
        };
        let mut observe = |event: &openvector::batch_runner::BatchEvent<'_>| {
            let options = progress.as_ref().expect("observer only installed with callback");
            let metadata = snapshot::summary(event.state);
            let checkpoint = snapshot::capture_with_summary(env, event.state, &options.capture, &metadata)
                .map_err(|error| error.to_string())?;
            let event = json!({"index":event.index,"filename":event.filename,"stage":event.stage,"paletteCount":event.palette_count,
                "state":metadata,"progress":event.progress.map(|p| json!({
                    "phase":p.phase,"substep":p.substep,"counter":p.counter,
                    "fraction":p.fraction,"percent":p.percent,"colorCount":p.color_count,"iteration":p.iteration
                }))});
            if !options.callback.call((event, checkpoint).into()).map_err(|error| error.to_string())? {
                cancellation.store(true, Ordering::Relaxed);
            }
            Ok(())
        };
        let observer = if progress.is_some() {
            Some(&mut observe as &mut openvector::batch_runner::BatchObserver<'_>)
        } else { None };
        let result = self.inner.run(&settings, &export_options, &mut decide, observer).map_err(engine_error)?;
        if let Some(error) = callback_error { return Err(error); }
        snapshot::batch_result(env, result)
    }
}

pub(crate) fn create(session: openvector::palette::CoreSession, application_directory: &str,
    working_directory: &str) -> BatchRunner
{
    BatchRunner { inner: openvector::batch_runner::BatchRunner::new(
        BatchQueue::new(application_directory, working_directory), session) }
}
