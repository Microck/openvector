use std::time::{Duration, Instant};
use openvector::{document_controls::{DocumentSettings, document_segmentation_key, document_smoothing_key, document_fitting_key}, image_document::StageKeys};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Load, Mode, ImageType, Transparency, Quality, Colors, Palette,
    Segmentation, Editing, Smoothing, Background, Review, Export,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Self::Load => "Load image", Self::Mode => "Wizard mode",
            Self::ImageType => "Image type", Self::Transparency => "Transparency",
            Self::Quality => "Input quality", Self::Colors => "Color mode",
            Self::Palette => "Palette selection", Self::Segmentation => "Image segmentation",
            Self::Editing => "Edit segmentation", Self::Smoothing => "Path smoothing",
            Self::Background => "Remove background", Self::Review => "Review result",
            Self::Export => "Export result",
        }
    }

    pub fn update_stage(self) -> Option<&'static str> {
        match self { Self::Palette => Some("palette"), Self::Segmentation => Some("segmentation"),
            Self::Smoothing => Some("vectors"), _ => None }
    }
}

/// Navigation is view state. Processing and editing completion are acknowledged
/// by the document worker before a destination page becomes visible.
#[derive(Clone, Debug)]
pub struct Wizard {
    pub page: Page,
    pub skip_review: bool,
    pub auto_palette: bool,
    pub auto_segmentation: bool,
    pub auto_vectorization: bool,
    pub edit_origin: Page,
    /// Pages visited before the current one, oldest first. Back returns along
    /// this path, so it always reaches the page the user actually came from.
    /// Public only so `..Wizard::default()` works; change it through `record`.
    pub history: Vec<Page>,
    /// The page last acknowledged by `record`.
    pub shown: Page,
    /// Set by Back; the matching arrival pops history instead of pushing.
    pub back_target: Option<Page>,
}
impl Default for Wizard {
    fn default() -> Self {
        Self { page: Page::Load, skip_review: false, auto_palette: true,
            auto_segmentation: true, auto_vectorization: true, edit_origin: Page::Segmentation,
            history: Vec::new(), shown: Page::Load, back_target: None }
    }
}
impl Wizard {
    pub fn completed_page(&self, settings: &DocumentSettings) -> Page {
        if settings.advanced || self.skip_review { Page::Export } else { Page::Review }
    }
    /// Records an arrival on `self.page`. Call once after every page change.
    pub fn record(&mut self) {
        let target = self.back_target.take();
        if self.page == self.shown { return; }
        if self.page == Page::Load {
            self.history.clear();
        } else if let Some(index) = self.history.iter().position(|page| *page == self.page) {
            // Back, or a forward route to an earlier page, trims the loop.
            self.history.truncate(index);
        } else if target != Some(self.page) {
            self.history.push(self.shown);
        }
        self.shown = self.page;
    }
    /// Where Back goes. The native predecessor wins when the user actually
    /// came through it (or nothing is recorded, e.g. right after a reset);
    /// otherwise Back returns to the last page really visited, so skipped
    /// pages (Automatic goes Mode -> Export) are never shown on the way back.
    pub fn back_page(&mut self, settings: &DocumentSettings, has_alpha: bool) -> Page {
        let fixed = self.fixed_back_page(settings, has_alpha);
        let page = match self.history.last() {
            // Editing records where it began (edit_origin); that is its history.
            Some(last) if self.page != Page::Editing && !self.history.contains(&fixed) => *last,
            _ => fixed,
        };
        self.back_target = Some(page);
        page
    }
    fn fixed_back_page(&self, settings: &DocumentSettings, has_alpha: bool) -> Page {
        use Page::*;
        match self.page {
            Load | Mode => Load,
            ImageType => Mode,
            Transparency => if settings.advanced { Mode } else { ImageType },
            Quality => if has_alpha { Transparency } else { ImageType },
            Colors => Quality,
            Palette => if settings.advanced { if has_alpha { Transparency } else { Mode } } else { Colors },
            Segmentation => Palette,
            Editing => self.edit_origin,
            Smoothing => Editing,
            Background => if settings.advanced { Smoothing } else { Review },
            Review => if self.skip_review { Export } else if settings.palette_mode == 1 { Palette } else { Colors },
            Export => if settings.advanced { Background } else if self.skip_review {
                if settings.palette_mode == 1 { Palette } else { Colors }
            } else { Review },
        }
    }
}

/// The ten advanced controls and color count share the native committed/revert
/// boundary. Palette, background, basic classification and export choices do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvancedControls {
    count: i32,
    sensitivity: i32,
    anti_alias: bool,
    complexity: i32,
    minimum_pixels: i32,
    artifact_rejection: i32,
    cluster_colors: bool,
    smoothness: i32,
    detect_corners: bool,
    curve_complexity: i32,
    smoothing_anti_alias: bool,
}
impl AdvancedControls {
    pub fn new(settings: &DocumentSettings, count: i32) -> Self {
        Self { count, sensitivity: settings.sensitivity, anti_alias: settings.anti_alias,
            complexity: settings.complexity, minimum_pixels: settings.minimum_pixels,
            artifact_rejection: settings.artifact_rejection, cluster_colors: settings.cluster_colors,
            smoothness: settings.smoothness, detect_corners: settings.detect_corners,
            curve_complexity: settings.curve_complexity, smoothing_anti_alias: settings.smoothing_anti_alias }
    }
    pub fn restore(self, settings: &mut DocumentSettings, count: &mut i32) {
        *count = self.count;
        settings.sensitivity = self.sensitivity; settings.anti_alias = self.anti_alias;
        settings.complexity = self.complexity; settings.minimum_pixels = self.minimum_pixels;
        settings.artifact_rejection = self.artifact_rejection; settings.cluster_colors = self.cluster_colors;
        settings.smoothness = self.smoothness; settings.detect_corners = self.detect_corners;
        settings.curve_complexity = self.curve_complexity; settings.smoothing_anti_alias = self.smoothing_anti_alias;
    }
}

pub fn page_needs_update(page: Page, settings: &DocumentSettings, keys: &StageKeys) -> bool {
    match page {
        Page::Palette => settings.palette_mode != 2 && settings.palette.is_empty(),
        Page::Segmentation => document_segmentation_key(settings) != keys.segmentation,
        Page::Smoothing => document_smoothing_key(settings, &keys.segmentation_edits) != keys.smoothing
            || document_fitting_key(settings, &keys.segmentation_edits) != keys.fitting,
        _ => false,
    }
}

/// Back reverts changed controls, not unfinished processing. A committed edit
/// can invalidate fitting while leaving every selected control unchanged.
pub fn page_controls_changed(page: Page, settings: &DocumentSettings, count: i32,
    committed: &DocumentSettings, committed_count: i32) -> bool {
    settings.advanced && matches!(page, Page::Palette | Page::Segmentation | Page::Editing | Page::Smoothing)
        && (page != Page::Palette || settings.palette_mode == 1)
        && AdvancedControls::new(settings, count) != AdvancedControls::new(committed, committed_count)
}

/// One active native 500 ms one-shot. Pending work survives leaving a page or
/// unchecking its box; a source change/cancel clears it. The host owns one GPUI
/// task, never a polling loop, and validates its ticket before dispatch.
#[derive(Clone, Debug, Default)]
pub struct AutoUpdate {
    page: Option<Page>,
    pending: [bool; 3],
    checked: bool,
    deadline: Option<Instant>,
    ticket: u64,
}
impl AutoUpdate {
    pub const DELAY: Duration = Duration::from_millis(500);
    pub fn page(&self) -> Option<Page> { self.page }
    fn index(page: Page) -> Option<usize> {
        match page { Page::Palette => Some(0), Page::Segmentation => Some(1), Page::Smoothing => Some(2), _ => None }
    }
    pub fn synchronize(&mut self, page: Option<Page>, checked: bool, stale: bool, changed: bool, now: Instant) {
        let page = page.filter(|page| Self::index(*page).is_some());
        let entering = self.page != page;
        if entering || self.checked != checked { self.suspend(); }
        self.page = page; self.checked = checked;
        if let Some(index) = page.and_then(Self::index) {
            if changed || entering && stale {
                self.pending[index] = true;
                self.suspend();
            }
        }
        self.resume(now);
    }
    pub fn suspend(&mut self) {
        self.deadline = None; self.ticket = self.ticket.wrapping_add(1);
    }
    pub fn resume(&mut self, now: Instant) {
        if self.deadline.is_none() && self.checked && self.page.and_then(Self::index).is_some_and(|index| self.pending[index]) {
            self.deadline = Some(now + Self::DELAY); self.ticket = self.ticket.wrapping_add(1);
        }
    }
    pub fn cancel(&mut self) {
        self.pending = [false; 3]; self.suspend();
    }
    pub fn reset(&mut self) {
        self.cancel(); self.page = None; self.checked = false;
    }
    pub fn consume_current(&mut self) {
        if let Some(index) = self.page.and_then(Self::index) { self.pending[index] = false; }
        self.suspend();
    }
    pub fn scheduled(&self) -> Option<(u64, Instant)> { self.deadline.map(|deadline| (self.ticket, deadline)) }
    pub fn fire(&mut self, ticket: u64, now: Instant) -> Option<Page> {
        if self.ticket != ticket || self.deadline.is_none_or(|deadline| deadline > now) { return None; }
        let page = self.page?;
        self.consume_current();
        self.checked.then_some(page)
    }
}
