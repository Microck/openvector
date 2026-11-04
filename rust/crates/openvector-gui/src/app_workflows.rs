use super::*;

pub(super) const SVG_SIZE_NOTICE: &str = "SVG dimensions use points (72 per inch). At 96 DPI, 48 pt displays as 64 pixels; physical print size is preserved.";
use std::collections::{HashMap, HashSet};
use gpui::Entity;
use openvector::{profile_palette::{PaletteCommand, PaletteProfile, PaletteRuntime, apply_palette_command}, saved_profiles::SavedAdvancedProfiles, settings_profile::{SettingsProfile, apply_advanced_profile, apply_basic_profile}};
use crate::{bitmap_size::BitmapSize, color_picker::{Hsv, parse_hex, parse_percent, to_hex}, numeric_text::NumericLocale, text_field::TextField};

#[derive(Clone)]
pub(super) enum WorkflowAction {
    Numeric(&'static str, i32), CommitNumeric, SaveProfile(bool), Rename(String), RenameProfile(String, bool), DeleteProfile(String, bool), ApplyProfile(String, bool),
    Color(usize), CommitColor, DeleteColor(usize), Palette(PaletteCommand), Eyedropper,
    ToggleSave(bool), DirectSave(bool), SaveDialog(bool), SaveFormat(String), SaveFolder, Save(bool), BitmapScale(f64), BitmapCommit(usize), BitmapStep(usize, i32, bool), Flatten,
    Confirm(Command), CreateDirectory(Command), Preview(PathBuf),
    ToggleDisclosure(Disclosure), ClearSources, ClearDirectories, BrowseDirectory(PathBuf), OpenDirectory(PathBuf), OpenSource(PathBuf), HandPick,
    ReplayRecentBasic(usize),
    RunProcess(&'static str, Page, Value), ApplyProfileAccepted(String, bool),
    Candidate(usize),
    PaintColor, CommitPaintColor,
    BatchSelect(usize, bool), BatchRemoveSelected, BatchClear,
    ResetApplication, ResetApplicationAccepted, OpenAccepted(PathBuf), StartBatchAccepted,
}
#[derive(Clone)]
pub(super) enum Form {
    Numeric(&'static str), Color(usize), PaintColor, Save(bool), Rename(String), Confirm(String, WorkflowAction), Preview(PathBuf),
    Directory(PathBuf, Vec<(PathBuf, bool)>),
}
/// Independent window disclosures, indexing `Workflows::disclosures`. Native
/// owners: Recent images window+0xb4 and Recent folders +0xb0 on Load, Recent
/// Basic Settings +0x130 and Saved Advanced Settings +0x134 on Mode (constructor
/// 0x455e24 attaches both to wizard page 4), and Save Settings +0x1d0 on
/// advanced Export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Disclosure { RecentImages, RecentFolders, RecentBasicSettings, SavedAdvancedSettings, SaveSettings }
/// Native profile list row modes: Mode replays, Save Settings renames and
/// deletes, and batch settings apply a profile to the batch, rename and delete.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProfileRows { Replay, Manage, Batch }
pub(super) struct Workflows {
    profiles: SavedAdvancedProfiles,
    profile_path: PathBuf,
    /// Exact-name draft beside Save, the native SaveWidget line edit, shared by
    /// advanced Export and batch settings across pages and workspaces.
    pub profile_name: Entity<TextField>,
    pub fields: Vec<Entity<TextField>>,
    pub bitmap: BitmapSize,
    pub palette: PaletteRuntime,
    pub palette_count: f64,
    pub recommended_background_mode: i32,
    pub inactive_palette: PaletteRuntime,
    pub eyedropper: Option<usize>,
    /// Colour being edited in the picker dialog, and the screen bounds of its
    /// saturation/value square and hue strip (captured during paint).
    pub picker: Hsv,
    pub picker_bounds: [Bounds<gpui::Pixels>; 2],
    /// Which picker surface is being dragged: 0 square, 1 hue strip.
    pub picker_drag: Option<usize>,
    /// Session-only queue previews. A batch's newest read replaces its own
    /// preview, never the separately persisted recent-source thumbnail.
    /// Skipped batch files are not read, so they keep the earlier queue preview.
    pub thumbnails: HashMap<PathBuf, Result<Arc<RenderImage>, String>>,
    /// Images removed from the recent cache, `thumbnails`, or the viewer. GPUI keeps a texture in
    /// each window's sprite atlas until `App::drop_image`, and removals happen
    /// in callbacks with and without a window, so `render` releases them all.
    pub released_images: Vec<Arc<RenderImage>>,
    pub batch_result: Option<openvector::batch_runner::BatchResult>,
    /// In-flight decode per path, tagged with its request number. A newer read
    /// or a membership drop replaces or removes the tag, so a reset followed by
    /// a re-request of the same path cannot accept the older completion.
    thumbnail_pending: HashMap<PathBuf, u64>,
    thumbnail_request: u64,
    flatten: bool,
    /// Session-only disclosure states, all collapsed at startup as observed
    /// natively. The window stage of a confirmed Reset application (0x414790)
    /// collapses all five. Native persistence of these states is not
    /// established, so none is stored.
    disclosures: [bool; 5],
}
impl Workflows {
    pub(super) fn new(preferences: &Preferences, cx: &mut Context<OpenVectorApp>) -> Result<Self, String> {
        let profile_path = preferences.path.with_file_name("profiles.json");
        let profiles = SavedAdvancedProfiles::read_file(&profile_path).map_err(|error| format!("Cannot load saved profiles {}: {error} The file was not changed. Repair it, or move it aside to start with defaults, then restart OpenVector.", profile_path.display()))?;
        let bitmap = BitmapSize::new(preferences.bitmap_scale)?;
        let suggestion = suggested_profile_name(&profiles);
        Ok(Self { profiles, profile_path, profile_name: cx.new(|cx| TextField::new("Settings name", suggestion, cx)), fields: Vec::new(), bitmap,
            palette: PaletteRuntime::default(), palette_count: preferences.selected.palette_color_count as f64, recommended_background_mode: preferences.selected.recommended_background_mode,
            inactive_palette: PaletteRuntime::default(), eyedropper: None, picker: Hsv { h: 0.0, s: 0.0, v: 0.0 }, picker_bounds: [Bounds::default(); 2], picker_drag: None,
            thumbnails: HashMap::new(), released_images: Vec::new(), batch_result: None, thumbnail_pending: HashMap::new(), thumbnail_request: 0, flatten: false, disclosures: [false; 5] })
    }
    pub(super) fn expanded(&self, disclosure: Disclosure) -> bool { self.disclosures[disclosure as usize] }
    /// Each native full model reset (0x430ed0) emits mainModelReset, connected to
    /// SaveWidget::initializeName: the draft becomes the current suggestion,
    /// replacing typed text. The window constructor makes the startup call.
    pub(super) fn suggest_profile_name(&self, cx: &mut Context<OpenVectorApp>) {
        let name = suggested_profile_name(&self.profiles);
        self.profile_name.update(cx, |field, cx| field.set(name, cx));
    }
    /// Drops previews and in-flight decodes outside `wanted`, then claims every
    /// uncached, unrequested path for one new decode request.
    fn claim_previews(&mut self, wanted: HashSet<PathBuf>) -> (u64, Vec<PathBuf>) {
        self.released_images.extend(self.thumbnails.extract_if(|path, _| !wanted.contains(path)).filter_map(|(_, preview)| preview.ok()));
        self.thumbnail_pending.retain(|path, _| wanted.contains(path));
        self.thumbnail_request += 1;
        let request = self.thumbnail_request;
        let paths: Vec<PathBuf> = wanted.into_iter().filter(|path| !self.thumbnails.contains_key(path) && !self.thumbnail_pending.contains_key(path)).collect();
        for path in &paths { self.thumbnail_pending.insert(path.clone(), request); }
        (request, paths)
    }
    /// Publishes a decode only while `request` still owns `path`; renders nothing otherwise.
    fn finish_preview(&mut self, request: u64, path: PathBuf, preview: Result<Bitmap, String>) {
        if self.thumbnail_pending.get(&path) != Some(&request) { return; }
        self.thumbnail_pending.remove(&path);
        self.cache_preview(path, preview.map(Bitmap::into_render_image));
    }
    /// Records a newer read of `path` and discards any older in-flight decode of it.
    pub(super) fn set_preview(&mut self, path: PathBuf, preview: Result<Arc<RenderImage>, String>) {
        self.thumbnail_pending.remove(&path);
        self.cache_preview(path, preview);
    }
    fn cache_preview(&mut self, path: PathBuf, preview: Result<Arc<RenderImage>, String>) {
        if let Some(Ok(replaced)) = self.thumbnails.insert(path, preview) { self.released_images.push(replaced); }
    }
    /// Stores a batch outcome. A failed load is the newest read of that file, so
    /// its error replaces the cached preview; loaded files arrived as BatchPreview.
    pub(super) fn finish_batch(&mut self, result: openvector::batch_runner::BatchResult) {
        for file in &result.files {
            if let ("failed-load", Some(error)) = (file.status.as_str(), &file.error) { self.set_preview(PathBuf::from(&file.filename), Err(error.clone())); }
        }
        self.batch_result = Some(result);
    }
}
/// Native SaveWidget::initializeName (0x41c130): "Settings %1" for N = 1, 2, ...
/// until no saved advanced profile has that exact, case-sensitive name.
fn suggested_profile_name(profiles: &SavedAdvancedProfiles) -> String {
    let names = profiles.snapshot();
    (1u32..).map(|number| format!("Settings {number}")).find(|name| names.iter().all(|entry| &entry.name != name)).expect("a free settings name")
}
impl OpenVectorApp {
    fn form(&mut self, form: Form, fields: &[(&str, String)], window: &mut Window, cx: &mut Context<Self>) {
        self.workflows.fields = fields.iter().map(|(label, value)| cx.new(|cx| TextField::new(*label, value.clone(), cx))).collect();
        self.dialog = Some(Dialog::Form(form));
        if self.restore_focus.is_none() { self.restore_focus = window.focused(cx); }
        if let Some(field) = self.workflows.fields.first() { field.update(cx, |field, cx| field.select_all(window, cx)); }
        else { self.dialog_focus.focus(window, cx); }
        cx.notify();
    }
    fn field(&self, index: usize, cx: &Context<Self>) -> String { self.workflows.fields[index].read(cx).text.clone() }
    /// Opens the colour picker for a stored BGRA colour. Field 0 is hex,
    /// field 1 opacity as a whole percentage.
    fn open_picker(&mut self, form: Form, [b, g, r, a]: [f32; 4], window: &mut Window, cx: &mut Context<Self>) {
        self.workflows.picker = Hsv::from_rgb([r, g, b]);
        self.workflows.picker_drag = None;
        self.form(form, &[("Hex", to_hex([r, g, b])), ("Opacity", format!("{:.0}", a * 100.0))], window, cx);
        // Typing only repaints the field; repaint the swatch and square with it.
        // The subscription ends when the dialog drops the field entity.
        if let Some(field) = self.workflows.fields.first() { cx.observe(field, |_, _, cx| cx.notify()).detach(); }
    }
    /// The picker's colour as RGBA, validated from its two fields.
    fn picked(&self, cx: &Context<Self>) -> Result<[f32; 4], String> {
        let [r, g, b] = parse_hex(&self.field(0, cx)).ok_or("Enter a hex color such as #1C4FD8.")?;
        let a = parse_percent(&self.field(1, cx)).ok_or("Enter an opacity from 0 to 100.")?;
        Ok([r, g, b, a])
    }
    /// The colour the picker shows: the typed hex when valid, keeping the
    /// dragged hue for greys (which have no hue of their own).
    pub(super) fn picker_display(&self, cx: &Context<Self>) -> Hsv {
        match self.workflows.fields.first().and_then(|field| parse_hex(&field.read(cx).text)) {
            Some(rgb) => { let mut hsv = Hsv::from_rgb(rgb); if hsv.s == 0.0 { hsv.h = self.workflows.picker.h; } hsv }
            None => self.workflows.picker,
        }
    }
    /// Moves the picker to a pointer position on surface 0 (square) or 1 (hue).
    pub(super) fn picker_drag(&mut self, surface: usize, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let bounds = self.workflows.picker_bounds[surface];
        let fraction = |offset: gpui::Pixels, extent: gpui::Pixels| (f32::from(offset) / f32::from(extent).max(1.0)).clamp(0.0, 1.0);
        let x = fraction(position.x - bounds.origin.x, bounds.size.width);
        let mut hsv = self.picker_display(cx);
        if surface == 0 { hsv.s = x; hsv.v = 1.0 - fraction(position.y - bounds.origin.y, bounds.size.height); } else { hsv.h = x * 359.999; }
        self.workflows.picker = hsv;
        let hex = to_hex(hsv.to_rgb());
        if let Some(field) = self.workflows.fields.first() { field.update(cx, |field, cx| field.set(hex, cx)); }
        cx.notify();
    }
    pub(super) fn current_profile(&self) -> SettingsProfile {
        let mut profile = if self.batch.is_some() { self.preferences.batch.clone() } else { self.preferences.selected.clone() };
        profile.settings = self.settings.clone();
        if self.batch.is_some() && !self.settings.advanced { profile.basic_color_count = self.color_count; } else { profile.color_count = self.color_count; }
        profile.palette_color_count = self.workflows.palette_count as i32;
        profile.recommended_background_mode = self.workflows.recommended_background_mode;
        profile
    }
    pub(super) fn retain_processing_preferences(&mut self) {
        let profile = self.current_profile();
        if self.batch.is_some() { self.preferences.batch = profile; } else { self.preferences.selected = profile; }
        self.preferences.batch_directory = self.batch_queue.export_directory.clone().into();
        self.preferences.source_directory = self.source_directory.clone();
    }
    /// One disclosure header in the compact Quick Save style.
    fn disclosure(&self, id: &'static str, disclosure: Disclosure, label: &str, cx: &Context<Self>) -> Stateful<Div> {
        let sign = if self.workflows.expanded(disclosure) { "−" } else { "+" };
        self.control(id, format!("{sign} {label}"), !self.busy, false, Action::Workflow(WorkflowAction::ToggleDisclosure(disclosure)), cx)
    }
    /// Renders Recent images and Recent folders as independent disclosures in the
    /// compact Quick Save style. A collapsed group neither snapshots its history
    /// nor builds rows; the preview cache follows history membership regardless.
    pub(super) fn recent_panel(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_2()
            .child(self.disclosure("recent-images-disclosure", Disclosure::RecentImages, "Recent images", cx))
            .when(self.workflows.expanded(Disclosure::RecentImages), |panel| {
                let files = self.preferences.sources.snapshot();
                panel.children(files["files"].as_array().expect("source-history files").iter().enumerate().map(|(index, file)| {
                    let path = PathBuf::from(file["filename"].as_str().expect("source filename"));
                    div().flex().flex_col().gap_1()
                        .when_some(self.preferences.source_preview(&path), |row, image| row.child(img(image.clone()).w(px(72.0)).h(px(54.0)).object_fit(gpui::ObjectFit::Contain)))
                        .child(self.control(format!("recent-{index}"), path.file_name().unwrap_or_default().to_string_lossy().into_owned(), !self.busy, false, Action::Recent(path), cx))
                        .child(div().text_color(rgb(theme::MUTED)).child(file["description"].as_str().unwrap_or("").to_string()))
                }))
                .child(self.control("clear-recent", "Clear recent images", !self.busy, false, Action::Workflow(WorkflowAction::ClearSources), cx))
            })
            .child(self.disclosure("recent-folders-disclosure", Disclosure::RecentFolders, "Recent folders", cx))
            .when(self.workflows.expanded(Disclosure::RecentFolders), |panel| {
                let directories = self.preferences.directories.snapshot();
                panel.children(directories["directories"].as_array().expect("source-history directories").iter().enumerate().map(|(index, directory)| {
                    let path = PathBuf::from(directory.as_str().expect("source directory"));
                    div().flex().flex_col().gap_1().child(path.display().to_string())
                        .child(div().flex().gap_1()
                            .child(self.control(format!("browse-directory-{index}"), "Browse", !self.busy, false, Action::Workflow(WorkflowAction::BrowseDirectory(path.clone())), cx))
                            .child(self.control(format!("open-directory-{index}"), "Open folder", !self.busy, false, Action::Workflow(WorkflowAction::OpenDirectory(path)), cx)))
                }))
                .child(self.control("clear-directories", "Clear recent folders", !self.busy, false, Action::Workflow(WorkflowAction::ClearDirectories), cx))
            })
    }
    /// Mode page groups in native order: Recent Basic Settings (window+0x130),
    /// then Saved Advanced Settings (+0x134). The recent group links the first
    /// four labels for replay and, as natively, has no Clear control; the fifth
    /// stored entry stays hidden.
    pub(super) fn mode_settings_panel(&self, cx: &Context<Self>) -> Div {
        let recent = self.preferences.recent_basic.visible_entries();
        div().flex().flex_col().gap_2()
            .child(self.disclosure("recent-basic-disclosure", Disclosure::RecentBasicSettings, "Recent Basic Settings", cx))
            .when(self.workflows.expanded(Disclosure::RecentBasicSettings), |panel| if recent.is_empty() {
                panel.child(div().text_color(rgb(theme::MUTED)).child("No items in list"))
            } else {
                panel.children(recent.iter().enumerate().map(|(index, entry)| self.control(format!("recent-basic-{index}"), entry.label.clone(),
                    !self.busy, false, Action::Workflow(WorkflowAction::ReplayRecentBasic(index)), cx).justify_start().truncate()))
            })
            .child(self.disclosure("saved-advanced-disclosure", Disclosure::SavedAdvancedSettings, "Saved Advanced Settings", cx))
            .when(self.workflows.expanded(Disclosure::SavedAdvancedSettings), |panel| panel.child(self.profile_rows(ProfileRows::Replay, cx)))
    }
    /// Save Settings (window+0x1d0) on advanced Export: the exact-name field
    /// with Save, then Rename and Delete per saved profile.
    pub(super) fn save_settings_panel(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_2()
            .child(self.disclosure("save-settings-disclosure", Disclosure::SaveSettings, "Save Settings", cx))
            .when(self.workflows.expanded(Disclosure::SaveSettings), |panel| panel.child(self.profile_save_row(cx)).child(self.profile_rows(ProfileRows::Manage, cx)))
    }
    /// Batch settings save, apply (Use), rename and delete profiles with the
    /// same draft and list as advanced Export.
    pub(super) fn batch_profiles_panel(&self, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_2().child(section("Saved settings")).child(self.profile_save_row(cx)).child(self.profile_rows(ProfileRows::Batch, cx))
    }
    /// Native SaveWidget: Save is enabled only for a nonempty name (checkNewName
    /// 0x41ada0); Enter in the field saves too.
    fn profile_save_row(&self, cx: &Context<Self>) -> Div {
        let named = !self.workflows.profile_name.read(cx).text.is_empty();
        // The text canvas has no intrinsic width; the row sizes it.
        div().flex().items_center().gap_1().child(div().flex_1().min_w_0().child(self.workflows.profile_name.clone()))
            .child(self.control("profile-save", "Save", !self.busy && named, false, Action::Workflow(WorkflowAction::SaveProfile(false)), cx))
    }
    fn profile_rows(&self, rows: ProfileRows, cx: &Context<Self>) -> Div {
        div().flex().flex_col().gap_2().children(self.workflows.profiles.snapshot().into_iter().enumerate().map(|(index, entry)| {
            let button = |id: &str, label: &'static str, action| self.control(format!("profile-{id}-{index}"), label, !self.busy, false, Action::Workflow(action), cx);
            let buttons = div().flex().flex_wrap().gap_1();
            let manage = |buttons: Div| buttons.child(button("rename", "Rename", WorkflowAction::Rename(entry.name.clone())))
                .child(button("delete", "Delete", WorkflowAction::DeleteProfile(entry.name.clone(), false)));
            let buttons = match rows {
                ProfileRows::Replay => buttons.child(button("play", "Play", WorkflowAction::ApplyProfile(entry.name.clone(), false)))
                    .child(button("run", "Fast forward", WorkflowAction::ApplyProfile(entry.name.clone(), true))),
                ProfileRows::Manage => manage(buttons),
                ProfileRows::Batch => manage(buttons.child(button("use", "Use", WorkflowAction::ApplyProfile(entry.name.clone(), false)))),
            };
            div().flex().flex_col().gap_1().border_b_1().border_color(rgb(theme::BORDER)).pb_2().child(entry.name).child(buttons)
        }))
    }
    fn save_profiles(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(parent) = self.workflows.profile_path.parent() { std::fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
        self.workflows.profiles.write_file(&self.workflows.profile_path)?; cx.notify(); Ok(())
    }
    /// Confirmed Reset application: native list, model and window stages of
    /// handler 0x451750. Unlike Start, this preserves the wizard page even when
    /// closing the source (captured native No/Yes reference). Batch advanced
    /// requests, workspace, vector export options and source/export folders
    /// survive. Both stores are
    /// attempted after the whole session reset, even if either write fails.
    fn reset_application(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        if self.busy || self.loading_source || !self.pending.is_empty() {
            return Err("Wait for processing to finish before resetting the application.".into());
        }
        let mut selected = openvector::settings_profile::default_selected_profile();
        // The blank document commits zero RGBA before selected source metadata
        // becomes transparent white. Do not commit the active batch settings.
        selected.settings.background = [0.0; 4];
        let mut committed = Value::Object(selected.settings.to_json());
        committed["count"] = json!(selected.color_count);
        // Window owners whose updates can fail are prepared on copies first, so a
        // rejected reset changes nothing. Each Quick Save and bitmap pane collapses
        // and selects AI or PNG; the empty folder argument selects the working
        // directory and the empty filename drops the closed source's name. Scale 1
        // settles both bitmap panes against the retained original size.
        let (mut quick_save, mut bitmap_save, mut bitmap) = (self.preferences.quick_save.clone(), self.preferences.bitmap_save.clone(), self.workflows.bitmap.clone());
        for mode in 0..2 {
            quick_save.configure(mode, "ai", "", "")?; quick_save.choices[mode].expanded = false;
            bitmap_save.configure(mode, "png", "", "")?; bitmap_save.choices[mode].expanded = false;
            bitmap.commit(mode, 2, 1.0)?;
        }
        self.close_dialog(window, cx);
        self.enqueue(Command::Reset(committed), None, cx);
        if !self.busy { return Err(self.status.clone()); }

        self.retain_processing_preferences();
        selected.settings.background = [1.0, 1.0, 1.0, 0.0];
        selected.palette_color_count = 2;
        selected.recommended_background_mode = -1;
        self.preferences.selected = selected;
        let batch = &mut self.preferences.batch;
        batch.settings.category = 3; batch.settings.quality = 3; batch.settings.palette_mode = 3;
        batch.settings.background_mode = 1; batch.settings.background = [1.0; 4];
        batch.basic_color_count = 0;
        self.batch_queue.clear(); self.batch_queue.processing_index = 0;
        let profile = if self.batch.is_some() {
            self.batch = Some(self.batch_queue.snapshot());
            self.workflows.inactive_palette = PaletteRuntime::default();
            &self.preferences.batch
        } else {
            // Full-window Reset closes the source without navigating.
            self.workflows.palette = PaletteRuntime::default();
            &self.preferences.selected
        };
        self.settings = profile.settings.clone();
        self.color_count = if self.batch.is_some() && !self.settings.advanced { profile.basic_color_count } else { profile.color_count };
        self.workflows.palette_count = profile.palette_color_count as f64;
        self.workflows.recommended_background_mode = profile.recommended_background_mode;
        self.close_document();
        // The canvas keeps its measured bounds when resetting in the same window.
        self.view = ViewerView::default().resized(self.view.area); self.dragging = None; self.pointer_buttons = 0; self.zoom_dragging = false;
        self.workflows.eyedropper = None;
        self.workflows.profiles.reset();
        self.workflows.released_images.extend(self.preferences.clear_sources()); self.preferences.directories.clear(); self.preferences.recent_basic.clear();
        // 0x451750 runs the full model reset 0x430ed0 after these lists; its
        // mainModelReset suggests a name against the rebuilt default list.
        self.workflows.suggest_profile_name(cx);
        // Both preview owners are now empty, so this drops every preview and in-flight decode.
        self.load_previews(cx);
        // Native window stage, after both model stages and before persistence;
        // 0x414790 collapses all five disclosures GPUI owns.
        self.wizard.auto_palette = true; self.wizard.auto_segmentation = true; self.wizard.auto_vectorization = true; self.wizard.skip_review = false;
        let preferences = &mut self.preferences;
        preferences.auto_palette = true; preferences.auto_segmentation = true; preferences.auto_vectorization = true; preferences.skip_review = false;
        preferences.quick_save = quick_save; preferences.bitmap_save = bitmap_save; preferences.bitmap_scale = bitmap.scale;
        preferences.drag_result = Default::default();
        self.workflows.bitmap = bitmap; self.workflows.disclosures = [false; 5]; self.show_paths = false;
        self.page_changed(cx);
        let mut failures = Vec::new();
        if let Err(error) = self.save_profiles(cx) { failures.push(format!("saved profiles: {error}")); }
        if let Err(error) = self.preferences.save() { failures.push(format!("preferences and source history: {error}")); }
        if failures.is_empty() { Ok(()) } else {
            Err(format!("The session was reset, but reset settings could not be saved. {}. Check the storage paths and permissions, then retry Reset application.", failures.join("; ")))
        }
    }
    pub(super) fn palette_candidates(&self, cx: &Context<Self>) -> Div {
        let mut body = div().flex().flex_col().gap_1();
        if let Some(document) = &self.document {
            // A candidate is only offered while it adds a colour: once the image
            // runs out of distinct colours, larger counts repeat the same palette.
            let mut shown = 0;
            for (count, palette) in document.palettes.iter().enumerate().skip(2).filter(|(_, palette)| !palette.is_empty()) {
                // Candidate colours are ARGB. Near-identical shades (antialiasing
                // leftovers) and transparent entries do not count as new colours:
                // compare opaque entries at 16 levels per channel.
                let mut distinct = palette.chunks_exact(4).filter(|color| color[0] > 0.5)
                    .map(|color| color[1..].iter().map(|value| (*value as f64 * 15.0).round() as u8).collect::<Vec<_>>()).collect::<Vec<_>>();
                distinct.sort_unstable(); distinct.dedup();
                if distinct.len() <= shown { continue; }
                shown = distinct.len();
                let selected = self.settings.palette.iter().copied().eq(palette.chunks_exact(4).flat_map(|color| color.iter().rev().copied()).map(|value| ((value as f64 * 65535.0).round() / 65535.0) as f32));
                // Fixed-width labels keep every swatch strip on one vertical line.
                body = body.child(self.control(format!("candidate-{count}"), "", !self.busy, selected, Action::Workflow(WorkflowAction::Candidate(count)), cx)
                    .justify_start().gap_2().aria_label(format!("{count} colors"))
                    .child(div().w(px(64.0)).flex_none().child(format!("{count} colors")))
                    .children(palette.chunks_exact(4).map(|color| {
                        let rgba = ((color[1] * 255.0).round() as u32) << 24 | ((color[2] * 255.0).round() as u32) << 16 | ((color[3] * 255.0).round() as u32) << 8 | (color[0] * 255.0).round() as u32;
                        div().size(px(14.0)).rounded(px(2.0)).border_1().border_color(rgb(theme::BORDER)).bg(gpui::rgba(rgba))
                    })));
            }
        }
        body
    }
    pub(super) fn palette_command(&mut self, command: PaletteCommand, cx: &mut Context<Self>) -> Result<(), String> {
        let mut count = self.color_count as f64;
        apply_palette_command(&mut PaletteProfile { color_count: &mut count, palette_color_count: &mut self.workflows.palette_count, palette: &mut self.settings.palette }, &mut self.workflows.palette, &command)?;
        self.color_count = count as i32; self.persist(cx); cx.notify(); Ok(())
    }
    fn update_bitmap_fields(&mut self, cx: &mut Context<Self>) {
        let mode = usize::from(self.settings.advanced); let dimensions = self.workflows.bitmap.panes[mode];
        for (index, value) in [(2, dimensions[0].to_string()), (3, dimensions[1].to_string()), (4, format!("{:.1}", self.workflows.bitmap.pane_scales[mode] * 100.0))] {
            if let Some(field) = self.workflows.fields.get(index) { field.update(cx, |field, cx| field.set(value, cx)); }
        }
        self.preferences.bitmap_scale = self.workflows.bitmap.scale; self.persist(cx);
    }
    pub(super) fn prepare_save(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let path = match &command { Command::Export { path, .. } | Command::BitmapExport { path, .. } => path, _ => return Err("Choose a vector or bitmap export.".into()) };
        if let Some(document) = self.document.as_ref().filter(|document| document.origin == SourceOrigin::Path) {
            // Canonical paths resolve symlinks but do not identify hardlinks.
            let aliases_source = path == &document.filename || match openvector::vector_file::same_file_identity(path, &document.filename) {
                Ok(same) => same,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.to_string()),
            };
            if aliases_source {
                return Err("Cannot overwrite the source file. Choose another filename or directory.".into());
            }
        }
        let directory = path.parent().unwrap_or(std::path::Path::new("."));
        if !directory.exists() {
            self.dialog = Some(Dialog::Form(Form::Confirm(format!("Create directory {}?", directory.display()), WorkflowAction::CreateDirectory(command))));
        } else if !directory.is_dir() { return Err(format!("The save directory is a file: {}", directory.display())); }
        else if path.exists() { self.dialog = Some(Dialog::Form(Form::Confirm(format!("Replace {}?", path.display()), WorkflowAction::Confirm(command)))); }
        else { self.close_dialog(window, cx); self.enqueue(command, None, cx); }
        Ok(())
    }
    pub(super) fn workflow_action(&mut self, action: WorkflowAction, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed_profile = matches!(&action, WorkflowAction::ApplyProfileAccepted(..));
        let result: Result<(), String> = (|| {
            if self.error {
                self.error = false;
                self.status.clear();
                self.status.push_str("Ready");
            }
            match action {
                WorkflowAction::OpenAccepted(path) => { self.close_dialog(window, cx); self.open_accepted(path, cx); },
                WorkflowAction::StartBatchAccepted => { self.close_dialog(window, cx); self.start_batch(cx); },
                WorkflowAction::ResetApplication => {
                    if self.busy || self.loading_source || !self.pending.is_empty() || self.dialog.is_some() { return Ok(()); }
                    self.form(Form::Confirm(
                        "Close the current image, clear the batch queue, recent images, recent folders and recent basic settings, and replace saved settings with the default list? Selected image settings, basic batch choices, Auto-update, Skip this page next time, Paths, Quick Save, Bitmap export and Drag result will return to defaults. Advanced batch settings, vector export options, the batch export folder and files on disk will stay unchanged.".into(),
                        WorkflowAction::ResetApplicationAccepted), &[], window, cx);
                    self.confirmation_cancel.focus(window, cx);
                },
                WorkflowAction::ResetApplicationAccepted => {
                    if !matches!(self.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::ResetApplicationAccepted)))) { return Ok(()); }
                    self.reset_application(window, cx)?;
                },
                WorkflowAction::BatchSelect(index, selected) => {
                    if self.busy || self.dialog.is_some() { return Ok(()); }
                    self.batch_queue.select(index, selected)?;
                    self.batch = Some(self.batch_queue.snapshot());
                },
                WorkflowAction::BatchRemoveSelected | WorkflowAction::BatchClear => {
                    if self.busy || self.dialog.is_some() { return Ok(()); }
                    if matches!(action, WorkflowAction::BatchClear) { self.batch_queue.clear(); }
                    else { self.batch_queue.remove_selected(); }
                    self.batch = Some(self.batch_queue.snapshot());
                    self.load_previews(cx);
                },
                // Colour and opacity share one picker dialog: hex, drag surfaces, opacity %.
                WorkflowAction::PaintColor => {
                    let color = self.document.as_ref().ok_or("Load an image before editing.")?.edit_color;
                    self.open_picker(Form::PaintColor, color, window, cx);
                },
                WorkflowAction::CommitPaintColor => {
                    if !matches!(self.dialog, Some(Dialog::Form(Form::PaintColor))) { return Ok(()); }
                    let [r, g, b, a] = self.picked(cx)?;
                    self.close_dialog(window, cx);
                    self.enqueue(Command::Edit { action: "gesture", input: json!({"phase":"color","color":[b, g, r, a]}) }, None, cx);
                },
                WorkflowAction::Candidate(count) => {
                    let palette = self.document.as_ref().and_then(|document| document.palettes.get(count)).filter(|palette| !palette.is_empty()).ok_or("Analyze the image before choosing a palette.")?;
                    let colors = palette.chunks_exact(4).flat_map(|color| color.iter().rev().map(|channel| *channel as f64)).collect();
                    self.palette_command(PaletteCommand::Choose(colors), cx)?;
                    // Show the choice: segment with the new palette and stay here.
                    // page_changed then displays the region layer for Palette.
                    if self.batch.is_none() { self.process("segmentation", Page::Palette, cx); }
                },
                WorkflowAction::RunProcess(stage, destination, input) => {
                    self.close_dialog(window, cx);
                    if stage == "palette" { self.workflows.palette.customized = false; self.workflows.palette.backup = self.settings.palette.clone(); }
                    self.pending_processing += 1;
                    self.pending.push_front((Command::Process { stage, input }, Some(destination))); self.start_pending(cx);
                },
                WorkflowAction::Numeric(key, value) => self.form(Form::Numeric(key), &[(key, value.to_string())], window, cx),
                WorkflowAction::CommitNumeric => {
                    let Some(Dialog::Form(Form::Numeric(key))) = self.dialog.clone() else { return Ok(()); };
                    let text = self.field(0, cx); let value = NumericLocale::default().integer(&text).ok_or("Enter a whole number.")?;
                    let (low, high) = if key == "colorCount" { (2, if self.batch.is_some() && !self.settings.advanced { 12 } else { 24 }) } else if key == "minimumPixels" { (0, i32::MAX) } else { (1, 12) };
                    if !(low..=high).contains(&value) { return Err(format!("Enter a number from {low} to {high}.")); }
                    self.close_dialog(window, cx);
                    if key == "colorCount" { self.palette_command(PaletteCommand::Count(value as f64), cx)?; }
                    else { self.change_setting(key, json!(value), cx); }
                },
                // Native SaveWidget::onSaveButtonClicked (0x41cb10): an empty name
                // does nothing, an exact existing name asks before overwriting and
                // a completed save clears the field; a declined overwrite keeps it.
                WorkflowAction::SaveProfile(force) => {
                    let name = self.workflows.profile_name.read(cx).text.clone(); if name.is_empty() { return Ok(()); }
                    if !force && self.workflows.profiles.snapshot().iter().any(|entry| entry.name == name) {
                        self.dialog = Some(Dialog::Form(Form::Confirm(format!("Replace profile {name}?"), WorkflowAction::SaveProfile(true)))); return Ok(());
                    }
                    let profile = self.current_profile();
                    self.workflows.profiles.put(&name, profile); self.save_profiles(cx)?;
                    self.workflows.profile_name.update(cx, |field, cx| field.set("", cx));
                    // A confirmed replacement returns focus to the control that asked.
                    if self.dialog.is_some() { self.close_dialog(window, cx); }
                },
                // Native rename (0x42b620) asks in a "Change Settings Label" input
                // dialog that starts from the exact current label.
                WorkflowAction::Rename(name) => self.form(Form::Rename(name.clone()), &[("Setting name", name)], window, cx),
                WorkflowAction::RenameProfile(name, force) => {
                    // The prompt's field, kept while its overwrite confirmation is open.
                    if !matches!(&self.dialog, Some(Dialog::Form(Form::Rename(_) | Form::Confirm(_, WorkflowAction::RenameProfile(..))))) { return Ok(()); }
                    let replacement = self.field(0, cx);
                    // An empty or unchanged label renames and writes nothing.
                    if replacement.is_empty() || replacement == name { self.close_dialog(window, cx); return Ok(()); }
                    if !force && self.workflows.profiles.snapshot().iter().any(|entry| entry.name == replacement) {
                        self.dialog = Some(Dialog::Form(Form::Confirm(format!("Replace profile {replacement}?"), WorkflowAction::RenameProfile(name, true)))); return Ok(());
                    }
                    self.workflows.profiles.rename(&name, &replacement, force)?; self.save_profiles(cx)?; self.close_dialog(window, cx);
                },
                WorkflowAction::DeleteProfile(name, confirmed) => {
                    if !confirmed { self.dialog = Some(Dialog::Form(Form::Confirm(format!("Delete profile {name}?"), WorkflowAction::DeleteProfile(name, true)))); }
                    else { self.workflows.profiles.remove(&name); self.save_profiles(cx)?; self.close_dialog(window, cx); }
                },
                WorkflowAction::ApplyProfile(name, run) | WorkflowAction::ApplyProfileAccepted(name, run) => {
                    let selected = self.workflows.profiles.snapshot().into_iter().find(|entry| entry.name == name).ok_or("The selected profile no longer exists.")?.profile;
                    let profile = apply_advanced_profile(Some(&self.current_profile()), &selected, self.batch.is_some())?;
                    if !confirmed_profile && self.batch.is_none() {
                        let palette_changes = profile.settings.palette_mode != 2 && self.workflows.palette.customized;
                        let edits_change = run && self.document.as_ref().is_some_and(|document| document.has_applied_edits && document.stage_keys.segmentation != openvector::document_controls::document_segmentation_key(&profile.settings));
                        if palette_changes || edits_change {
                            self.dialog = Some(Dialog::Form(Form::Confirm("Applying this profile will recalculate the image and replace customized colors or applied segmentation edits.".into(), WorkflowAction::ApplyProfileAccepted(name, run))));
                            return Ok(());
                        }
                    }
                    self.workflows.profiles.select(&name);
                    self.settings = profile.settings.clone(); self.color_count = profile.color_count; self.workflows.palette_count = profile.palette_color_count as f64;
                    self.workflows.recommended_background_mode = profile.recommended_background_mode;
                    self.save_profiles(cx)?; self.close_dialog(window, cx);
                    self.persist(cx);
                    if self.batch.is_none() {
                        // Native Play seeds only the Advanced guard, preserving
                        // saved background controls and any prior Basic guard.
                        if !run { self.transparency_initialized[1] = true; }
                        let page = if self.document.as_ref().is_some_and(|document| document.source_alpha_type != 0) { Page::Transparency } else { Page::Palette };
                        if self.wizard.page != Page::Mode { self.wizard.page = page; self.page_changed(cx); }
                        if run { self.leave_mode(Some(Command::Profile { profile, basic: false }), Page::Export, cx); }
                        else { self.leave_mode(Some(Command::Commit(self.task_input())), page, cx); }
                    }
                },
                // Native vectorizeImageWithMruSettings: select the whole stored
                // profile with advanced off, keeping the live palette runtime and
                // the document's candidates, then replay it. Success shows the
                // completed page; failure, Cancel or a palette mode the runner
                // does not process leaves the replayed settings on Mode.
                WorkflowAction::ReplayRecentBasic(index) => {
                    let profile = apply_basic_profile(&self.preferences.recent_basic.visible_entries().get(index).ok_or("The selected recent settings no longer exist.")?.profile);
                    self.settings = profile.settings.clone(); self.color_count = profile.color_count;
                    self.workflows.palette_count = profile.palette_color_count as f64;
                    self.workflows.recommended_background_mode = profile.recommended_background_mode;
                    self.preferences.selected = profile.clone(); self.persist(cx);
                    self.leave_mode(Some(Command::Profile { profile, basic: true }), self.wizard.completed_page(&self.settings), cx);
                },
                WorkflowAction::Color(index) => {
                    let color = self.settings.palette.get(index * 4..index * 4 + 4).ok_or("Choose an existing palette color.")?;
                    let color = [color[0], color[1], color[2], color[3]];
                    self.open_picker(Form::Color(index), color, window, cx);
                },
                WorkflowAction::CommitColor => {
                    let Some(Dialog::Form(Form::Color(index))) = self.dialog else { return Ok(()); };
                    let [r, g, b, a] = self.picked(cx)?;
                    self.palette_command(PaletteCommand::Color { index: index as f64, color: vec![b as f64, g as f64, r as f64, a as f64] }, cx)?;
                    self.close_dialog(window, cx);
                },
                WorkflowAction::DeleteColor(index) => { self.palette_command(PaletteCommand::Delete(index as f64), cx)?; self.close_dialog(window, cx); },
                WorkflowAction::Palette(command) => { self.palette_command(command, cx)?; },
                WorkflowAction::Eyedropper => {
                    let Some(Dialog::Form(Form::Color(index))) = self.dialog else { return Ok(()); };
                    self.workflows.eyedropper = Some(index); self.close_dialog(window, cx); self.set_layer(0); self.status = "Click the source image to choose this palette color. Escape cancels.".into();
                },
                WorkflowAction::ToggleSave(bitmap) => {
                    let mode = usize::from(self.settings.advanced);
                    let choice = if bitmap { &mut self.preferences.bitmap_save.choices[mode] } else { &mut self.preferences.quick_save.choices[mode] };
                    choice.expanded = !choice.expanded; self.persist(cx);
                },
                WorkflowAction::DirectSave(bitmap) => {
                    self.workflow_action(WorkflowAction::SaveDialog(bitmap), window, cx);
                    self.workflow_action(WorkflowAction::Save(bitmap), window, cx);
                },
                WorkflowAction::SaveDialog(bitmap) => {
                    let mode = usize::from(self.settings.advanced);
                    let choice = if bitmap { &mut self.preferences.bitmap_save.choices[mode] } else { &mut self.preferences.quick_save.choices[mode] };
                    choice.expanded = true;
                    let mut fields = vec![("Directory", choice.directory.to_string_lossy().into_owned()), ("Filename", choice.filename.clone())];
                    if bitmap { let dimensions = self.workflows.bitmap.panes[mode]; fields.extend([("Width", dimensions[0].to_string()), ("Height", dimensions[1].to_string()), ("Percent", format!("{:.1}", self.workflows.bitmap.pane_scales[mode] * 100.0))]); }
                    self.workflows.flatten = false; self.form(Form::Save(bitmap), &fields, window, cx); self.persist(cx);
                },
                WorkflowAction::SaveFormat(format) => {
                    let Some(Dialog::Form(Form::Save(bitmap))) = self.dialog else { return Ok(()); };
                    let mode = usize::from(self.settings.advanced);
                    if bitmap { self.preferences.bitmap_save.choices[mode].format = format; self.workflows.flatten = false; } else { self.preferences.quick_save.choices[mode].format = format; }
                    self.persist(cx);
                },
                WorkflowAction::SaveFolder => {
                    let field = self.workflows.fields.first().cloned().ok_or("Open the save dialog first.")?;
                    let prompt = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Save folder".into()) });
                    cx.spawn(async move |this, cx| {
                        let result = prompt.await;
                        let _ = this.update(cx, |app, cx| match result {
                            Ok(Ok(Some(paths))) => if let Some(path) = paths.first() { field.update(cx, |field, cx| field.set(path.to_string_lossy(), cx)); },
                            Ok(Ok(None)) => {}, Ok(Err(error)) => app.fail(error.to_string(), cx), Err(error) => app.fail(error.to_string(), cx),
                        });
                    }).detach();
                },
                WorkflowAction::BitmapScale(scale) => {
                    let scale = if scale == 0.0 { zoom::export_scale(&self.view) } else { scale };
                    self.workflows.bitmap.commit(usize::from(self.settings.advanced), 2, scale)?; self.update_bitmap_fields(cx);
                },
                WorkflowAction::BitmapCommit(axis) => {
                    let text = self.field(axis + 2, cx); let parser = NumericLocale::host()?; let value = if axis < 2 { parser.integer(&text).unwrap_or(0) as f64 } else { parser.percent(&text).unwrap_or(0.0) };
                    self.workflows.bitmap.commit(usize::from(self.settings.advanced), axis, value)?; self.update_bitmap_fields(cx);
                },
                WorkflowAction::BitmapStep(axis, direction, large) => {
                    self.workflows.bitmap.step(usize::from(self.settings.advanced), axis, direction, large)?; self.update_bitmap_fields(cx);
                },
                WorkflowAction::Flatten => self.workflows.flatten = !self.workflows.flatten,
                WorkflowAction::Save(bitmap) => {
                    let mode = usize::from(self.settings.advanced); let directory = self.field(0, cx); let filename = self.field(1, cx);
                    if bitmap {
                        let dimensions = self.workflows.bitmap.panes[mode];
                        let previous = [dimensions[0].to_string(), dimensions[1].to_string(), format!("{:.1}", self.workflows.bitmap.pane_scales[mode] * 100.0)];
                        if let Some(axis) = (0..3).rev().find(|axis| self.field(axis + 2, cx).trim() != previous[*axis]) {
                            let text = self.field(axis + 2, cx);
                            let parser = NumericLocale::host()?; let value = if axis < 2 { parser.integer(&text).unwrap_or(0) as f64 } else { parser.percent(&text).unwrap_or(0.0) };
                            self.workflows.bitmap.commit(mode, axis, value)?; self.update_bitmap_fields(cx);
                        }
                    }
                    let choice = if bitmap { &mut self.preferences.bitmap_save } else { &mut self.preferences.quick_save };
                    let format = choice.choices[mode].format.clone(); choice.configure(mode, &format, &directory, &filename)?;
                    let path = choice.target(mode)?; let format = choice.choices[mode].format.clone();
                    let command = if bitmap {
                        let document = self.document.as_ref().ok_or("Open and vectorize an image before saving.")?;
                        if format != "png" && document.source_alpha_type != 0 && self.settings.background_mode == 0 && !self.workflows.flatten { return Err("This format may discard transparency. Select Confirm flattening before saving, or choose PNG.".into()); }
                        Command::BitmapExport { path: path.clone(), format: format.clone(), scale: self.workflows.bitmap.pane_scales[mode], flatten: format != "png" && self.workflows.flatten }
                    } else { Command::Export { path: path.clone(), options: self.export_options() } };
                    self.persist(cx);
                    self.prepare_save(command, window, cx)?;
                },
                WorkflowAction::Confirm(command) => { self.close_dialog(window, cx); self.enqueue(command, None, cx); },
                WorkflowAction::CreateDirectory(command) => {
                    let path = match &command { Command::Export { path, .. } | Command::BitmapExport { path, .. } => path, _ => return Err("Choose a vector or bitmap export.".into()) };
                    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new("."))).map_err(|error| error.to_string())?;
                    self.prepare_save(command, window, cx)?;
                },
                WorkflowAction::Preview(path) => { self.dialog = Some(Dialog::Form(Form::Preview(path))); },
                // Session-only, unlike ToggleSave; see `Workflows::disclosures`.
                WorkflowAction::ToggleDisclosure(disclosure) => { let expanded = &mut self.workflows.disclosures[disclosure as usize]; *expanded = !*expanded; },
                WorkflowAction::ClearSources => { self.workflows.released_images.extend(self.preferences.clear_sources()); self.persist(cx); },
                WorkflowAction::ClearDirectories => { self.preferences.directories.clear(); self.persist(cx); },
                WorkflowAction::BrowseDirectory(path) => {
                    let mut entries = Vec::new();
                    for entry in std::fs::read_dir(&path).map_err(|error| error.to_string())? {
                        let entry = entry.map_err(|error| error.to_string())?;
                        entries.push((entry.path(), entry.file_type().map_err(|error| error.to_string())?.is_dir()));
                    }
                    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
                    self.dialog = Some(Dialog::Form(Form::Directory(path, entries)));
                },
                WorkflowAction::OpenDirectory(path) => {
                    let path = path.canonicalize().map_err(|error| error.to_string())?;
                    cx.open_with_system(&path);
                    // A dispatched retry replaces the previous path error in the footer.
                    self.status = "Ready".into();
                },
                WorkflowAction::OpenSource(path) => { self.close_dialog(window, cx); self.open(path, cx); },
                WorkflowAction::HandPick => { self.wizard.page = Page::ImageType; self.page_changed(cx); },
            }
            Ok(())
        })();
        if let Err(error) = result { self.fail(error, cx); }
        cx.notify();
    }
    pub(super) fn retain_save_choices(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(Dialog::Form(Form::Save(bitmap))) = self.dialog else { return Ok(()); };
        let directory = self.field(0, cx); let filename = self.field(1, cx); let mode = usize::from(self.settings.advanced);
        let save = if bitmap { &mut self.preferences.bitmap_save } else { &mut self.preferences.quick_save };
        let format = save.choices[mode].format.clone(); save.configure(mode, &format, &directory, &filename)?;
        self.persist(cx); Ok(())
    }
    pub(super) fn save_disclosure(&self, bitmap: bool, cx: &Context<Self>) -> Div {
        let mode = usize::from(self.settings.advanced); let save = if bitmap { &self.preferences.bitmap_save } else { &self.preferences.quick_save };
        let choice = &save.choices[mode]; let name = if bitmap { "Bitmap export" } else { "Quick Save" }; let id = if bitmap { "bitmap" } else { "quick" };
        div().flex().flex_col().gap_2()
            .child(self.control(format!("{id}-disclosure"), format!("{} {name}", if choice.expanded { "−" } else { "+" }), !self.busy, false, Action::Workflow(WorkflowAction::ToggleSave(bitmap)), cx))
            .when(choice.expanded, |body| body
                .child(div().text_color(rgb(theme::MUTED)).child(choice.directory.display().to_string()))
                .child(div().child(format!("{}.{}", choice.filename, choice.format)))
                .when(!bitmap && choice.format == "svg", |body| body.child(div().text_color(rgb(theme::MUTED)).child(SVG_SIZE_NOTICE)))
                .child(div().flex().gap_1()
                    .child(self.control(format!("{id}-configure"), "Configure", !self.busy, false, Action::Workflow(WorkflowAction::SaveDialog(bitmap)), cx))
                    .child(self.control(format!("{id}-save"), "Save", !self.busy, false, Action::Workflow(WorkflowAction::DirectSave(bitmap)), cx))))
    }
    pub(super) fn workflow_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = match &self.dialog {
            Some(Dialog::Form(Form::Numeric(_))) => Some(WorkflowAction::CommitNumeric),
            Some(Dialog::Form(Form::Color(_))) => Some(WorkflowAction::CommitColor),
            Some(Dialog::Form(Form::PaintColor)) => Some(WorkflowAction::CommitPaintColor),
            Some(Dialog::Form(Form::Rename(name))) => Some(WorkflowAction::RenameProfile(name.clone(), false)),
            Some(Dialog::Form(Form::Save(bitmap))) => {
                let axis = self.workflows.fields.iter().enumerate().skip(2).find(|(_, field)| field.read(cx).focus_handle(cx).is_focused(window)).map(|(index, _)| index - 2);
                Some(axis.map_or(WorkflowAction::Save(*bitmap), WorkflowAction::BitmapCommit))
            },
            Some(Dialog::Form(Form::Confirm(_, _))) => { self.dispatch(Action::Dismiss, window, cx); return; }, _ => None,
        };
        if let Some(action) = action { self.workflow_action(action, window, cx); }
    }
    /// The dialog's confirming action, in the accent colour.
    fn workflow_primary(&self, id: &str, label: &'static str, action: WorkflowAction, cx: &Context<Self>) -> Stateful<Div> {
        primary_button(format!("dialog-{id}"), label, !self.busy).px_4()
            .when(!self.busy, |button| button.on_click(cx.listener(move |app, _, window, cx| app.dispatch(Action::Workflow(action.clone()), window, cx))))
    }
    fn workflow_button(&self, id: impl Into<String>, label: impl Into<String>, action: WorkflowAction, cx: &Context<Self>) -> Stateful<Div> {
        let id = format!("dialog-{}", id.into());
        button(id, label.into(), !self.busy, false).when(!self.busy, |button| button.on_click(cx.listener(move |app, _, window, cx| app.dispatch(Action::Workflow(action.clone()), window, cx))))
    }
    /// Saturation/value square, hue strip, then hex and opacity inputs with a
    /// preview swatch. Dragging either surface rewrites the hex field.
    fn picker(&self, cx: &Context<Self>) -> Div {
        use gpui::{linear_gradient, linear_color_stop};
        let hsv = self.picker_display(cx);
        let pack = |[r, g, b]: [f32; 3]| ((r * 255.0).round() as u32) << 16 | ((g * 255.0).round() as u32) << 8 | (b * 255.0).round() as u32;
        let hue = pack(Hsv { h: hsv.h, s: 1.0, v: 1.0 }.to_rgb());
        let current = pack(hsv.to_rgb());
        let entity = cx.entity().downgrade();
        let surface = |index: usize| {
            let entity = entity.clone();
            div().id(("picker-surface", index)).relative().w_full().cursor_pointer()
                .child(canvas(move |bounds, _, cx| { let _ = entity.update(cx, |app, _| app.workflows.picker_bounds[index] = bounds); }, |_, _, _, _| {}).absolute().size_full())
                .on_mouse_down(MouseButton::Left, cx.listener(move |app, event: &gpui::MouseDownEvent, _, cx| { app.workflows.picker_drag = Some(index); app.picker_drag(index, event.position, cx); }))
                .on_mouse_move(cx.listener(move |app, event: &gpui::MouseMoveEvent, _, cx| { if app.workflows.picker_drag == Some(index) { app.picker_drag(index, event.position, cx); } }))
                .on_mouse_up(MouseButton::Left, cx.listener(|app, _, _, _| app.workflows.picker_drag = None))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|app, _, _, _| app.workflows.picker_drag = None))
        };
        // Hue strip as six gradient segments, red through red.
        let stops = [0xff0000, 0xffff00, 0x00ff00, 0x00ffff, 0x0000ff, 0xff00ff, 0xff0000];
        // White ring inside a dark ring stays visible on both light and dark colours.
        let marker = |left: f32, top: f32| div().absolute().left(gpui::relative(left)).top(gpui::relative(top)).ml(px(-7.0)).mt(px(-7.0))
            .size(px(14.0)).rounded_full().border_1().border_color(rgb(0x000000))
            .child(div().size_full().rounded_full().border_2().border_color(rgb(0xffffff)));
        div().flex().flex_col().gap_3()
            .child(surface(0).h(px(180.0)).rounded(px(6.0)).overflow_hidden().bg(rgb(hue))
                .child(div().absolute().inset_0().bg(linear_gradient(90.0, linear_color_stop(rgb(0xffffff), 0.0), linear_color_stop(gpui::rgba(0xffffff00), 1.0))))
                .child(div().absolute().inset_0().bg(linear_gradient(180.0, linear_color_stop(gpui::rgba(0x00000000), 0.0), linear_color_stop(rgb(0x000000), 1.0))))
                .child(marker(hsv.s, 1.0 - hsv.v)))
            .child(surface(1).h(px(14.0)).rounded(px(7.0)).flex()
                .children(stops.windows(2).map(|pair| div().flex_1().h_full().bg(linear_gradient(90.0, linear_color_stop(rgb(pair[0]), 0.0), linear_color_stop(rgb(pair[1]), 1.0)))))
                .child(marker(hsv.h / 360.0, 0.5)))
            .child(div().flex().items_end().gap_2()
                .child(div().size(px(32.0)).flex_none().rounded(px(6.0)).border_1().border_color(rgb(theme::BORDER)).bg(rgb(current)))
                .child(div().flex_1().flex().flex_col().gap_1().child(div().text_color(rgb(theme::MUTED)).child("Hex")).child(self.workflows.fields[0].clone()))
                .child(div().w(px(96.0)).flex().flex_col().gap_1().child(div().text_color(rgb(theme::MUTED)).child("Opacity %")).child(self.workflows.fields[1].clone())))
    }
    pub(super) fn form_modal(&self, form: &Form, cx: &Context<Self>) -> Div {
        let mut body = div().id("form-body").flex().flex_col().gap_3().max_h(px(600.0)).overflow_y_scroll();
        let title = match form {
            Form::Numeric(key) => { body = body.child(self.workflows.fields[0].clone()).child(self.workflow_button("numeric-apply", "Apply", WorkflowAction::CommitNumeric, cx)); format!("Set {key}") },
            Form::Rename(name) => {
                body = body.child(self.workflows.fields[0].clone()).child(self.workflow_button("rename-apply", "Rename", WorkflowAction::RenameProfile(name.clone(), false), cx));
                "Change Settings Label".into()
            },
            Form::Color(index) => {
                body = body.child(self.picker(cx))
                    .child(div().flex().gap_2()
                        .child(self.workflow_button("color-pick", "Pick from image", WorkflowAction::Eyedropper, cx))
                        .child(self.workflow_button("color-delete", "Delete color", WorkflowAction::DeleteColor(*index), cx))
                        .child(div().flex_1())
                        .child(self.workflow_primary("color-apply", "Apply", WorkflowAction::CommitColor, cx)));
                format!("Palette color {}", index + 1)
            },
            Form::PaintColor => {
                body = body.child(self.picker(cx))
                    .child(div().flex().child(div().flex_1()).child(self.workflow_primary("paint-color-apply", "Apply", WorkflowAction::CommitPaintColor, cx)));
                "Paint color".into()
            },
            Form::Save(bitmap) => {
                let mode = usize::from(self.settings.advanced); let save = if *bitmap { &self.preferences.bitmap_save } else { &self.preferences.quick_save };
                body = body.child(div().flex().flex_wrap().gap_1().children(save.formats().iter().map(|format| self.workflow_button(format!("save-format-{format}"), format.to_uppercase(), WorkflowAction::SaveFormat((*format).into()), cx).bg(rgb(if save.choices[mode].format == *format { theme::ACCENT } else { theme::BACKGROUND })))))
                    .child(section("Directory")).child(self.workflows.fields[0].clone()).child(self.workflow_button("save-folder", "Choose folder", WorkflowAction::SaveFolder, cx))
                    .child(section("Filename")).child(self.workflows.fields[1].clone());
                if !*bitmap && save.choices[mode].format == "svg" { body = body.child(div().text_color(rgb(theme::MUTED)).child(SVG_SIZE_NOTICE)); }
                if *bitmap {
                    // The text canvas has no intrinsic width; size its input/button group.
                    for (axis, name) in ["Width", "Height", "Percent"].into_iter().enumerate() {
                        body = body.child(row(name, div().flex().w(px(180.0)).flex_shrink_0().gap_1().child(self.workflows.fields[axis + 2].clone()).child(self.workflow_button(format!("size-{axis}"), "Apply", WorkflowAction::BitmapCommit(axis), cx))));
                        if axis < 2 { body = body.child(div().flex().gap_1().children([(-1, true, "−25"), (-1, false, "−5"), (1, false, "+5"), (1, true, "+25")].into_iter().map(|(direction, large, label)| self.workflow_button(format!("size-step-{axis}-{direction}-{large}"), label, WorkflowAction::BitmapStep(axis, direction, large), cx)))); }
                    }
                    body = body.child(div().flex().flex_wrap().gap_1().children([1, 2, 4, 5, 6, 8, 10, 12, 16, 20, 24].into_iter().map(|scale| self.workflow_button(format!("scale-{scale}"), format!("{scale}×"), WorkflowAction::BitmapScale(scale as f64), cx))))
                        .child(self.workflow_button("view-scale", "Use view scale", WorkflowAction::BitmapScale(0.0), cx));
                    if save.choices[mode].format != "png" { body = body.child(self.workflow_button("flatten", if self.workflows.flatten { "Flattening confirmed" } else { "Confirm flattening transparency" }, WorkflowAction::Flatten, cx)); }
                }
                if *bitmap { "Bitmap export".into() } else { "Quick Save".into() }
            },
            Form::Confirm(message, action) => {
                let reset = matches!(action, WorkflowAction::ResetApplicationAccepted);
                body = body.child(message.clone()).child(self.workflow_button("confirm", if reset { "Reset application" } else { "Continue" }, action.clone(), cx));
                if reset { "Reset application?".into() } else { "Confirm action".into() }
            },
            Form::Directory(path, entries) => {
                if let Some(parent) = path.parent() { body = body.child(self.workflow_button("directory-parent", "Parent folder", WorkflowAction::BrowseDirectory(parent.into()), cx)); }
                for (index, (path, directory)) in entries.iter().enumerate() {
                    body = body.child(self.workflow_button(format!("directory-file-{index}"), path.file_name().unwrap_or_default().to_string_lossy(),
                        if *directory { WorkflowAction::BrowseDirectory(path.clone()) } else { WorkflowAction::OpenSource(path.clone()) }, cx));
                }
                path.display().to_string()
            },
            Form::Preview(path) => {
                body = body.child(path.display().to_string());
                if let Some(preview) = self.workflows.thumbnails.get(path) { body = match preview { Ok(image) => body.child(img(image.clone()).w(px(360.0)).h(px(300.0)).object_fit(gpui::ObjectFit::Contain)), Err(error) => body.child(error.clone()) }; }
                "Image preview".into()
            },
        };
        div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000099)).occlude().child(
            div().id("workflow-dialog").role(Role::Dialog).aria_label(title.clone()).track_focus(&self.dialog_focus).tab_index(0).tab_stop(false).occlude()
                .w(px(520.0)).max_h_full().p_4().flex().flex_col().gap_3().rounded(px(theme::DIALOG_RADIUS)).border_1().border_color(rgb(theme::BORDER)).bg(rgb(theme::RAISED))
                .child(div().text_size(px(18.0)).child(title)).child(body)
                .when(self.error, |body| body.child(div().id("workflow-error").role(Role::Alert).text_color(rgb(theme::DANGER)).child(self.status.clone())))
                .children(match form {
                    Form::Save(bitmap) => Some(self.workflow_button("save-now", "Save", WorkflowAction::Save(*bitmap), cx)),
                    _ => None,
                })
                .child(self.control("dialog-close", if matches!(form, Form::Confirm(_, _)) { "Cancel" } else { "Close" }, true, false, Action::Dismiss, cx)
                    .when(matches!(form, Form::Confirm(_, _)), |button| button.track_focus(&self.confirmation_cancel))))
    }
    /// Synchronizes only queue previews. Recent thumbnails belong to the
    /// persisted history and must never trigger reads of their source paths.
    pub(super) fn load_previews(&mut self, cx: &mut Context<Self>) {
        let wanted: HashSet<PathBuf> = self.batch_queue.snapshot().queue.iter().map(|entry| PathBuf::from(&entry.filename)).collect();
        let (request, paths) = self.workflows.claim_previews(wanted);
        if paths.is_empty() { return; }
        let work = cx.background_executor().spawn(async move {
            paths.into_iter().map(|path| {
                let preview = std::fs::read(&path).map_err(|error| error.to_string())
                    .and_then(|bytes| openvector::image_input::decode_image(&bytes).map_err(|error| error.to_string()))
                    .map(|source| Bitmap::preview(source.width, source.height, &source.bgra));
                (path, preview)
            }).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let previews = work.await;
            let _ = this.update(cx, |app, cx| {
                for (path, preview) in previews { app.workflows.finish_preview(request, path, preview); }
                cx.notify();
            });
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-forms-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); path
    }
    /// The native SaveWidget draft: startup suggests the first unused exact
    /// "Settings N"; typing there must not reach the viewer's held-layer keys,
    /// whose releases the field keeps; Enter saves and a completed save clears
    /// the field. An empty name does nothing, an exact existing name asks before
    /// overwriting and a declined overwrite keeps the draft. Rename starts from
    /// the exact label in its own prompt.
    #[gpui::test]
    fn save_settings_draft_follows_the_native_save_widget(cx: &mut gpui::TestAppContext) {
        let directory = directory(); let path = directory.join("gui.json");
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(path, cx).unwrap());
        // The field accepts text once a frame has painted it on advanced Export.
        app.update_in(cx, |app, window, cx| {
            assert_eq!(app.workflows.profile_name.read(cx).text, "Settings 1");
            app.settings.advanced = true; app.wizard.page = Page::Export;
            app.workflow_action(WorkflowAction::ToggleDisclosure(Disclosure::SaveSettings), window, cx);
        });
        app.update_in(cx, |app, window, cx| app.workflows.profile_name.update(cx, |field, cx| field.select_all(window, cx)));
        cx.simulate_keystrokes("b v");
        app.read_with(cx, |app, cx| {
            let input = app.input.snapshot();
            assert!(!input.held_bitmap && !input.held_vector, "typing a name must not hold a viewer layer");
            assert_eq!(app.workflows.profile_name.read(cx).text, "bv");
        });
        cx.simulate_keystrokes("enter");
        app.update_in(cx, |app, window, cx| {
            let draft = |app: &OpenVectorApp, cx: &App| app.workflows.profile_name.read(cx).text.clone();
            assert_eq!(app.workflows.profiles.snapshot()[0].name, "bv");
            assert_eq!(draft(app, cx), "");
            let count = app.workflows.profiles.len();
            app.workflow_action(WorkflowAction::SaveProfile(false), window, cx);
            assert_eq!(app.workflows.profiles.len(), count); assert!(!app.error);
            app.workflows.profile_name.update(cx, |field, cx| field.set("  Fine detail  ", cx));
            app.workflow_action(WorkflowAction::SaveProfile(false), window, cx);
            assert_eq!(app.workflows.profiles.snapshot()[0].name, "  Fine detail  ");
            app.workflows.profile_name.update(cx, |field, cx| field.set("  Fine detail  ", cx));
            app.workflow_action(WorkflowAction::SaveProfile(false), window, cx);
            assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::SaveProfile(true))))));
            app.dispatch(Action::Dismiss, window, cx);
            assert_eq!(draft(app, cx), "  Fine detail  ");
            app.workflow_action(WorkflowAction::SaveProfile(false), window, cx);
            app.workflow_action(WorkflowAction::SaveProfile(true), window, cx);
            assert!(app.dialog.is_none()); assert_eq!(draft(app, cx), "");
            let names = |app: &OpenVectorApp| app.workflows.profiles.snapshot().into_iter().map(|entry| entry.name).collect::<Vec<_>>();
            let before = names(app);
            app.workflow_action(WorkflowAction::Rename("bv".into()), window, cx);
            app.workflow_enter(window, cx);
            assert!(app.dialog.is_none()); assert_eq!(names(app), before, "an unchanged label renames nothing");
            app.workflow_action(WorkflowAction::Rename("  Fine detail  ".into()), window, cx);
            assert_eq!(app.field(0, cx), "  Fine detail  ");
            app.workflows.fields[0].update(cx, |field, cx| field.set("Renamed", cx));
            app.workflow_enter(window, cx);
            assert!(app.dialog.is_none());
            app.workflow_action(WorkflowAction::ApplyProfile("Renamed".into(), false), window, cx);
            assert!(app.settings.advanced);
            app.workflow_action(WorkflowAction::DeleteProfile("Renamed".into(), false), window, cx);
            assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, _)))));
            app.workflow_action(WorkflowAction::DeleteProfile("Renamed".into(), true), window, cx);
            assert!(!app.workflows.profiles.snapshot().iter().any(|entry| entry.name == "Renamed"));
        });
        let profiles = SavedAdvancedProfiles::read_file(directory.join("profiles.json")).unwrap();
        assert!(!profiles.snapshot().iter().any(|entry| entry.name == "Renamed"));
        std::fs::remove_dir_all(directory).unwrap();
    }
    /// Batch settings keep named profiles through the same draft and list: Save
    /// stores the batch request, Use applies a profile's advanced controls to it,
    /// and Rename and Delete keep their prompts.
    #[gpui::test]
    fn batch_settings_save_use_rename_and_delete_profiles(cx: &mut gpui::TestAppContext) {
        let directory = directory();
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.join("gui.json"), cx).unwrap());
        app.update_in(cx, |app, window, cx| {
            app.dispatch(Action::Batch, window, cx);
            app.dispatch(Action::Toggle("advanced"), window, cx);
            app.settings.complexity = 9;
            app.workflows.profile_name.update(cx, |field, cx| field.set("batch preset", cx));
            app.workflow_action(WorkflowAction::SaveProfile(false), window, cx);
            let saved = app.workflows.profiles.snapshot().remove(0);
            assert_eq!((saved.name.as_str(), &saved.profile), ("batch preset", &app.current_profile()));
            assert_eq!(app.workflows.profile_name.read(cx).text, "");
            app.settings.complexity = 3;
            app.workflow_action(WorkflowAction::ApplyProfile("batch preset".into(), false), window, cx);
            assert_eq!(app.settings.complexity, 9); assert!(app.batch.is_some());
            app.workflow_action(WorkflowAction::Rename("batch preset".into()), window, cx);
            app.workflows.fields[0].update(cx, |field, cx| field.set("renamed preset", cx));
            app.workflow_enter(window, cx);
            assert_eq!(app.workflows.profiles.snapshot()[0].name, "renamed preset");
            app.workflow_action(WorkflowAction::DeleteProfile("renamed preset".into(), false), window, cx);
            app.workflow_action(WorkflowAction::DeleteProfile("renamed preset".into(), true), window, cx);
            assert!(!app.workflows.profiles.snapshot().iter().any(|entry| entry.name == "renamed preset"));
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[gpui::test]
    fn palette_numeric_and_bitmap_dialogs_apply_real_controls(cx: &mut gpui::TestAppContext) {
        let directory = directory(); let path = directory.join("gui.json");
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(path, cx).unwrap());
        app.update_in(cx, |app, window, cx| {
            app.settings.palette = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
            app.workflows.palette.backup = app.settings.palette.clone();
            app.workflow_action(WorkflowAction::Color(0), window, cx);
            app.workflows.fields[0].update(cx, |field, cx| field.set("#FF0000", cx));
            app.workflow_action(WorkflowAction::CommitColor, window, cx);
            assert_eq!(&app.settings.palette[..4], &[0.0, 0.0, 1.0, 1.0]);
            app.workflow_action(WorkflowAction::Palette(PaletteCommand::Revert), window, cx);
            assert_eq!(&app.settings.palette[..4], &[0.0, 0.0, 0.0, 1.0]);
            app.workflow_action(WorkflowAction::Numeric("minimumPixels", 0), window, cx);
            app.workflows.fields[0].update(cx, |field, cx| field.set("125", cx));
            app.workflow_action(WorkflowAction::CommitNumeric, window, cx);
            assert_eq!(app.settings.minimum_pixels, 125);
            app.workflows.bitmap.source_changed(16, 12).unwrap();
            app.workflow_action(WorkflowAction::SaveDialog(true), window, cx);
            app.workflows.fields[2].update(cx, |field, cx| field.set("32", cx));
            app.workflow_action(WorkflowAction::BitmapCommit(0), window, cx);
            assert_eq!(app.workflows.bitmap.panes[0], [32, 24]);
            app.workflow_action(WorkflowAction::SaveFormat("tif".into()), window, cx);
            app.workflow_action(WorkflowAction::Flatten, window, cx);
            assert!(app.workflows.flatten);
            assert_eq!(app.preferences.bitmap_save.choices[0].format, "tif");
            assert_eq!(app.preferences.bitmap_save.choices[1].format, "png");
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[gpui::test]
    fn recalculation_confirmation_keeps_palette_and_defaults_to_cancel(cx: &mut gpui::TestAppContext) {
        let directory = directory();
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.join("gui.json"), cx).unwrap());
        app.update_in(cx, |app, window, cx| {
            app.settings.palette_mode = 1;
            app.palette_command(PaletteCommand::Choose(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]), cx).unwrap();
            app.palette_command(PaletteCommand::Color { index: 0.0, color: vec![0.0, 0.0, 1.0, 1.0] }, cx).unwrap();
            let palette = app.settings.palette.clone(); let runtime = app.workflows.palette.clone();
            app.process("palette", Page::Palette, cx);
            assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::RunProcess(..))))));
            app.enqueue(Command::Edit { action: "begin", input: Value::Null }, Some(Page::Editing), cx);
            assert!(!app.busy); assert_eq!(app.pending.len(), 1);
            app.workflow_enter(window, cx);
            assert!(app.dialog.is_none()); assert!(app.pending.is_empty()); assert!(!app.busy);
            assert_eq!(app.settings.palette, palette); assert_eq!(app.workflows.palette, runtime);
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[gpui::test]
    fn selected_and_batch_profiles_retain_independent_counts(cx: &mut gpui::TestAppContext) {
        let directory = directory();
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.join("gui.json"), cx).unwrap());
        app.update_in(cx, |app, window, cx| {
            app.color_count = 7; app.workflows.palette_count = 6.0; app.workflows.recommended_background_mode = -1;
            app.dispatch(Action::Batch, window, cx);
            assert_eq!(app.color_count, 0);
            app.color_count = 5;
            app.dispatch(Action::Toggle("advanced"), window, cx);
            assert_eq!(app.color_count, 2);
            app.color_count = 11;
            app.dispatch(Action::Toggle("advanced"), window, cx);
            assert_eq!(app.color_count, 5);
            app.dispatch(Action::Batch, window, cx);
            assert_eq!(app.color_count, 7); assert_eq!(app.workflows.palette_count, 6.0);
            assert_eq!(app.workflows.recommended_background_mode, -1);
            let restored = Preferences::load(directory.join("gui.json")).unwrap();
            assert_eq!(restored.selected.color_count, 7);
            assert_eq!(restored.batch.basic_color_count, 5); assert_eq!(restored.batch.color_count, 11);
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[gpui::test]
    fn preview_completion_requires_its_current_request(cx: &mut gpui::TestAppContext) {
        let directory = directory(); let path = directory.join("input.png");
        let (app, cx) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.join("gui.json"), cx).unwrap());
        app.update_in(cx, |app, _, _| {
            let wanted = || HashSet::from([path.clone()]);
            let pixel = || Ok(Bitmap { width: 1, height: 1, bgra: vec![0, 0, 0, 255] });
            let error = |workflows: &Workflows| workflows.thumbnails[&path].as_ref().err().cloned();
            let workflows = &mut app.workflows;
            let (stale, claimed) = workflows.claim_previews(wanted());
            assert_eq!(claimed, [path.clone()]);
            // Reset drops membership while that decode is in flight, then the same path returns.
            assert!(workflows.claim_previews(HashSet::new()).1.is_empty());
            let (current, claimed) = workflows.claim_previews(wanted());
            assert_eq!(claimed, [path.clone()]);
            workflows.finish_preview(stale, path.clone(), Err("stale".into()));
            assert!(!workflows.thumbnails.contains_key(&path));
            assert!(workflows.claim_previews(wanted()).1.is_empty(), "the current request still owns the path");
            workflows.finish_preview(current, path.clone(), Err("current".into()));
            assert_eq!(error(workflows).as_deref(), Some("current"));
            // A cached failure is not permanent: a newer engine read replaces it and
            // also supersedes a decode requested after a remove and re-add.
            workflows.claim_previews(HashSet::new());
            let (superseded, _) = workflows.claim_previews(wanted());
            workflows.set_preview(path.clone(), Err("engine".into()));
            workflows.finish_preview(superseded, path.clone(), pixel());
            assert_eq!(error(workflows).as_deref(), Some("engine"));
            workflows.set_preview(path.clone(), Ok(pixel().unwrap().into_render_image()));
            assert!(workflows.thumbnails[&path].is_ok());
            // Removed paths keep neither previews nor requests.
            workflows.claim_previews(HashSet::new());
            assert!(workflows.thumbnails.is_empty() && workflows.thumbnail_pending.is_empty());
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
#[path = "app_workflows_controls_tests.rs"]
mod control_tests;

#[cfg(test)]
#[path = "app-reset-tests.rs"]
mod reset_tests;

#[cfg(test)]
#[path = "app-recent-basic-tests.rs"]
mod recent_basic_tests;
