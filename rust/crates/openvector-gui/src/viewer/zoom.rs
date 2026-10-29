//! Viewer zoom actions. Port of `gui/viewer-zoom.mjs`; contract
//! `docs/viewer-zoom-contract.json`.

use super::geometry::{Point, Size, ViewerView};
use super::zoom_values::{ZOOM_GETTER_SCALES, ZOOM_INVERSE_THRESHOLDS, ZOOM_SLIDER_SCALES};

pub const SLIDER_MAXIMUM: i64 = 300;
pub const SLIDER_PAGE_STEP: i64 = 10;
pub const MINIMUM_SCALE: f64 = 0.2;
pub const MAXIMUM_SCALE: f64 = 24.0;
/// Native always-on scrollbar extent subtracted from the widget for Fit.
pub const SCROLLBAR_EXTENT: f64 = 17.0;
/// Qt angle units per wheel notch.
pub const WHEEL_NOTCH_ANGLE: i32 = 120;
/// Lines per wheel notch in GPUI's Linux backends: X11 core buttons 4/5,
/// XInput2 smooth scrolling (one valuator increment) and Wayland discrete
/// axes all scale one notch to `SCROLL_LINES` (pinned gpui_linux
/// `linux/platform.rs`).
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const GPUI_LINES_PER_NOTCH: f32 = 3.0;

fn clamp_scale(scale: f64) -> f64 {
    MINIMUM_SCALE.max(MAXIMUM_SCALE.min(scale))
}

/// Monotone integer inverse whose generated transition boundaries preserve
/// the native truncation. Returns `None` for a non-finite scale.
pub fn scale_position(scale: f64) -> Option<usize> {
    if !scale.is_finite() {
        return None;
    }
    let scale = clamp_scale(scale);
    Some(ZOOM_INVERSE_THRESHOLDS.partition_point(|threshold| scale >= *threshold))
}

/// Bitmap "Use view scale" reads the logarithmic slider getter, not the matrix.
pub fn export_scale(view: &ViewerView) -> f64 {
    ZOOM_GETTER_SCALES[view.slider]
}

pub fn direct_scale(view: &ViewerView, scale: f64) -> Option<ViewerView> {
    let slider = scale_position(scale)?;
    Some(view.scaled(clamp_scale(scale), slider))
}

/// Fit: smaller available/image ratio, no margin, recentered.
pub fn fit(view: &ViewerView, image: Size, area: Size) -> Option<ViewerView> {
    let scale = (area.width / image.width).min(area.height / image.height);
    let slider = scale_position(scale)?;
    Some(view.fitted(image, area, clamp_scale(scale), slider))
}

/// 100%: displayed scale exactly one, slider synchronized, image centered.
pub fn one(view: &ViewerView) -> ViewerView {
    direct_scale(view, 1.0).expect("one is finite").centered()
}

/// A slider change keeps the retained logical center while quantizing its
/// displayed position through the integer scrollbars.
pub fn slider(view: &ViewerView, position: i64) -> ViewerView {
    let slider = position.clamp(0, SLIDER_MAXIMUM) as usize;
    view.scaled(clamp_scale(ZOOM_SLIDER_SCALES[slider]), slider)
}

/// Native wheel: slider advances by trunc(angle/12); `point` is the pointer
/// offset from half the available viewport.
pub fn wheel(view: &ViewerView, angle: i32, point: Point) -> ViewerView {
    let next = slider(view, view.slider as i64 + (angle / 12) as i64);
    let shift = 1.0 / export_scale(view) - 1.0 / export_scale(&next);
    next.wheel_centered(Point {
        x: view.center.x + point.x * shift,
        y: view.center.y + point.y * shift,
    })
}

/// Convert scrolling to viewer angle units. Linux line events have a pinned
/// notch size. Other platforms retain the existing logical-pixel conversion;
/// their native notch normalization still needs platform-specific verification.
pub fn wheel_angle(delta: &gpui::ScrollDelta) -> i32 {
    match delta {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        gpui::ScrollDelta::Lines(lines) => (lines.y * WHEEL_NOTCH_ANGLE as f32 / GPUI_LINES_PER_NOTCH).round() as i32,
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        gpui::ScrollDelta::Lines(lines) => (lines.y * 12.0) as i32,
        gpui::ScrollDelta::Pixels(pixels) => f32::from(pixels.y) as i32,
    }
}
