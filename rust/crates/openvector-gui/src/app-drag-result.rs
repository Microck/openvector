use super::*;
use crate::{drag_result::DragFile, viewer::paint::Scene};
use gpui::{ExternalDragOutcome, ExternalDragPayload, FileDropEvent};

struct Key { scene: std::sync::Weak<Scene>, owner: usize, format: &'static str, options: Value }
impl Key {
    fn matches(&self, app: &OpenVectorApp) -> bool {
        let owner = usize::from(app.settings.advanced);
        app.document.as_ref().and_then(|doc| doc.vectors.as_ref()).is_some_and(|scene| self.scene.as_ptr() == Arc::as_ptr(scene))
            && self.owner == owner && self.format == app.preferences.drag_result.choices[owner].format
            && self.options["layout"].as_str() == Some(app.preferences.layout.as_str())
            && self.options["curveMode"].as_str() == Some(app.preferences.curve_mode.as_str())
            && self.options["thinStroke"].as_bool() == Some(app.preferences.thin_stroke)
            && self.options["compact"].as_bool() == Some(app.preferences.compact_eps)
    }
}
#[derive(Default)]
enum Preparation {
    #[default]
    Empty,
    Pending { key: Key, request: u64 },
    Ready { key: Key, file: Arc<DragFile> },
    Failed { key: Key, error: String },
}
impl Preparation {
    fn key(&self) -> Option<&Key> {
        match self { Self::Empty => None, Self::Pending { key, .. } | Self::Ready { key, .. } | Self::Failed { key, .. } => Some(key) }
    }
}
struct Active { file: Arc<DragFile>, handed_off: bool, minimize: bool }
#[derive(Default)]
pub(super) struct DragResult { sequence: u64, preparation: Preparation, active: Option<Active> }
impl DragResult {
    pub(super) fn invalidate(&mut self) { self.preparation = Preparation::Empty; }
    pub(super) fn cancel_preparation(&mut self) {
        self.preparation = match std::mem::take(&mut self.preparation) {
            Preparation::Pending { key, .. } => Preparation::Failed { key, error: "Preparation cancelled".into() },
            other => other,
        };
    }
    pub(super) fn active(&self) -> bool { self.active.is_some() }
}
struct DragPreview;
impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px_2().py_1().bg(rgb(theme::BACKGROUND)).text_color(rgb(theme::TEXT)).child("Vector file")
    }
}
impl OpenVectorApp {
    pub(super) fn prepare_drag(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.batch.is_some() || self.dialog.is_some() || self.wizard.page != Page::Export || self.drag_result.active() { return; }
        let Some(scene) = self.document.as_ref().and_then(|doc| doc.vectors.as_ref()) else { self.drag_result.invalidate(); return; };
        let owner = usize::from(self.settings.advanced);
        let format = self.preferences.drag_result.choices[owner].format;
        if self.drag_result.preparation.key().is_some_and(|key| key.matches(self)) { return; }
        let options = self.export_options();
        // Keep identity without retaining a replaced document's potentially large geometry.
        let key = Key { scene: Arc::downgrade(scene), owner, format, options: options.clone() };
        self.drag_result.sequence += 1;
        let request = self.drag_result.sequence;
        self.drag_result.preparation = Preparation::Pending { key, request };
        self.enqueue(Command::PrepareDrag { request, directory: self.preferences.path.with_file_name("drag-exports"), format, options }, None, cx);
    }
    pub(super) fn drag_prepared(&mut self, request: u64, result: Result<Arc<DragFile>, String>) {
        self.drag_result.preparation = match std::mem::take(&mut self.drag_result.preparation) {
            Preparation::Pending { key, request: pending } if pending == request => match result {
                Ok(file) => Preparation::Ready { key, file }, Err(error) => Preparation::Failed { key, error },
            },
            other => other,
        };
    }
    pub(super) fn drag_event(&mut self, event: &FileDropEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            FileDropEvent::Started => {
                if let Some(active) = &self.drag_result.active {
                    active.file.retain_for_receiver();
                    if active.minimize { window.minimize_window(); }
                    self.status = "Dragging vector file".into();
                }
            }
            FileDropEvent::Ended { outcome } => {
                let Some(active) = self.drag_result.active.take() else { return; };
                // Accepted platforms may terminate before Started. They can already have
                // exposed the path; only an explicit startup rejection proves otherwise.
                if !matches!(outcome, ExternalDragOutcome::Rejected(_)) { active.file.retain_for_receiver(); }
                cx.stop_active_drag(window);
                match outcome {
                    ExternalDragOutcome::Dropped => { self.status = "Vector file dropped".into(); self.error = false; }
                    ExternalDragOutcome::Cancelled => { self.status = "Drag cancelled".into(); self.error = false; }
                    ExternalDragOutcome::Rejected(error) | ExternalDragOutcome::Failed(error) => self.fail(error.clone(), cx),
                }
            }
            _ => return,
        }
        cx.notify();
    }
    pub(super) fn internal_drag_released(&mut self, cx: &mut Context<Self>) -> bool {
        let released = self.drag_result.active.take_if(|active| !active.handed_off).is_some();
        if released { cx.notify(); }
        released
    }
    pub(super) fn cancel_internal_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.internal_drag_released(cx) { return false; }
        cx.stop_active_drag(window);
        self.status = "Drag cancelled".into();
        self.error = false;
        true
    }
    fn current_drag_file(&self, file: &Arc<DragFile>) -> bool {
        !self.busy && self.batch.is_none() && self.dialog.is_none() && self.wizard.page == Page::Export
            && matches!(&self.drag_result.preparation, Preparation::Ready { key, file: ready }
                if Arc::ptr_eq(ready, file) && key.matches(self))
    }
    pub(super) fn begin_result_drag(&mut self, file: &Arc<DragFile>, cx: &mut Context<Self>) -> bool {
        if self.drag_result.active.is_some() || !self.current_drag_file(file) { return false; }
        let minimize = self.preferences.drag_result.choices[usize::from(self.settings.advanced)].minimize;
        self.drag_result.active = Some(Active { file: file.clone(), handed_off: false, minimize });
        cx.notify();
        true
    }
    pub(super) fn result_drag_payload(&mut self, file: &Arc<DragFile>) -> Option<ExternalDragPayload> {
        if !self.current_drag_file(file) { return None; }
        let active = self.drag_result.active.as_mut().filter(|active| Arc::ptr_eq(&active.file, file))?;
        active.handed_off = true;
        Some(ExternalDragPayload::Files(gpui::FileDragPaths::new([(file.path().to_owned(), false)])))
    }
    pub(super) fn drag_panel(&self, cx: &Context<Self>) -> Div {
        let owner = usize::from(self.settings.advanced);
        let choice = self.preferences.drag_result.choices[owner];
        let mut panel = div().flex().flex_col().gap_2().child(section("Drag result"))
            .child(div().flex().flex_wrap().gap_1().children(crate::quick_save::VECTOR_FORMATS.iter().map(|format|
                self.control(format!("drag-format-{format}"), format.to_uppercase(), !self.busy, *format == choice.format, Action::DragFormat(format), cx))))
            .child(self.control("drag-minimize", "Minimize on drag", !self.busy, choice.minimize, Action::DragMinimize, cx));
        match &self.drag_result.preparation {
            Preparation::Ready { file, .. } if !self.busy && !self.drag_result.active() => {
                let app = cx.entity().downgrade();
                let payload_app = app.clone();
                panel = panel.child(button("drag-result", format!("Drag {} file", choice.format.to_uppercase()), true, false)
                    .on_drag(file.clone(), move |file: &Arc<DragFile>, _, _, cx| {
                        let _ = app.update(cx, |app, cx| app.begin_result_drag(file, cx));
                        cx.new(|_| DragPreview)
                    })
                    .external_drag_payload(move |file: &Arc<DragFile>, _, cx| {
                        payload_app.update(cx, |app, _| app.result_drag_payload(file)).ok().flatten()
                    }));
            }
            Preparation::Failed { error, .. } => {
                panel = panel.child(div().child(error.clone())).child(self.control("drag-retry", "Retry preparation", !self.busy, false, Action::DragRetry, cx));
            }
            _ => {
                let label = if self.drag_result.active() { "Dragging vector file" }
                    else if self.document.as_ref().is_none_or(|document| document.vectors.is_none()) { "Update vectors before dragging" }
                    else { "Preparing drag file" };
                panel = panel.child(button("drag-result", label, false, false));
            }
        }
        panel
    }
}

#[cfg(test)]
#[path = "app-drag-tests.rs"]
mod tests;
