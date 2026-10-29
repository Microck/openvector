//! Viewport geometry owned by one viewer state. Port of `gui/viewer-geometry.mjs`
//! (Qt 4.4.3 QGraphicsView scene bounds, integer scrollbars, retained centers).
//! Arithmetic order follows the reference exactly; every value is binary64.

use super::zoom_values::ZOOM_GETTER_SCALES;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
    fn axis(&self, index: usize) -> f64 {
        if index == 0 { self.width } else { self.height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One Qt scrollbar range. Values are integers stored as binary64 so the
/// reference's mixed integer/floating arithmetic is reproduced bit for bit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBar {
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub single_step: f64,
    pub page_step: f64,
}

impl ScrollBar {
    const fn new() -> Self {
        Self { value: 0.0, minimum: 0.0, maximum: 0.0, single_step: 1.0, page_step: 10.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Axis::Horizontal => 0,
            Axis::Vertical => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewerView {
    /// Displayed transform scale.
    pub zoom: f64,
    /// Integer slider position 0..=300 (separate native state from `zoom`).
    pub slider: usize,
    pub image: Size,
    pub area: Size,
    /// Native retained center.
    pub center: Point,
    /// Qt requested center.
    pub qt_center: Point,
    pub bars: [ScrollBar; 2],
    pub indent: Point,
    pub scene: SceneRect,
    pub origin: Point,
    pub retain_center_on_resize: bool,
}

impl Default for ViewerView {
    fn default() -> Self {
        Self::initial()
    }
}

fn clamp(value: f64, minimum: f64, maximum: f64) -> f64 {
    // Math.max(minimum, Math.min(maximum, value))
    let upper = if value < maximum { value } else { maximum };
    if minimum > upper { minimum } else { upper }
}

fn round_bound(value: f64) -> f64 {
    if value <= -2147483648.0 {
        -2147483648.0
    } else if value >= 2147483647.0 {
        2147483647.0
    } else {
        (value + 0.5).floor()
    }
}

/// JavaScript truthiness of a number.
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

impl ViewerView {
    pub fn initial() -> Self {
        Self {
            zoom: 1.0,
            slider: 100,
            image: Size::ZERO,
            area: Size::ZERO,
            center: Point::ZERO,
            qt_center: Point::ZERO,
            bars: [ScrollBar::new(), ScrollBar::new()],
            indent: Point::ZERO,
            scene: SceneRect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            origin: Point::ZERO,
            retain_center_on_resize: true,
        }
    }

    pub fn bar(&self, axis: Axis) -> &ScrollBar {
        &self.bars[axis.index()]
    }

    fn presented(mut self) -> Self {
        self.origin = Point {
            x: -(-self.indent.x).trunc() - self.bars[0].value,
            y: -(-self.indent.y).trunc() - self.bars[1].value,
        };
        self
    }

    /// Qt 4.4.3 inverts the transform through adjoint/determinant. Direct
    /// division changes observed point coordinates by one bit.
    pub fn image_point(&self, point: Point) -> Point {
        let inverse = self.zoom * (1.0 / (self.zoom * self.zoom));
        Point {
            x: (point.x + self.bars[0].value + (-self.indent.x).trunc()) * inverse,
            y: (point.y + self.bars[1].value + (-self.indent.y).trunc()) * inverse,
        }
    }

    fn scrolled(&mut self, x: f64, y: f64, notify: bool) {
        let next = [x, y]
            .map(|v| v.trunc())
            .iter()
            .enumerate()
            .map(|(i, v)| clamp(*v, self.bars[i].minimum, self.bars[i].maximum))
            .collect::<Vec<_>>();
        if next[0] != self.bars[0].value || next[1] != self.bars[1].value {
            self.bars[0].value = next[0];
            self.bars[1].value = next[1];
            self.qt_center = self.image_point(Point {
                x: ((self.area.width - 1.0) / 2.0).trunc() + 0.0,
                y: ((self.area.height - 1.0) / 2.0).trunc() + 0.0,
            });
            if notify {
                self.center = self.image_point(Point {
                    x: (self.area.width / 2.0 + 0.5).trunc(),
                    y: (self.area.height / 2.0 + 0.5).trunc(),
                });
            }
        }
    }

    fn scene_bounds(&mut self, notify: bool) {
        if self.image.width == 0.0 || self.image.height == 0.0 {
            self.scene = SceneRect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
            self.indent = Point::ZERO;
            for bar in &mut self.bars {
                bar.value = 0.0;
                bar.minimum = 0.0;
                bar.maximum = 0.0;
            }
            return;
        }
        let inverse = 1.0 / ZOOM_GETTER_SCALES[self.slider];
        let half_width = (self.area.width * 0.5) * inverse;
        let half_height = (self.area.height * 0.5) * inverse;
        self.scene = SceneRect {
            x: -half_width,
            y: -half_height,
            width: half_width * 2.0 + self.image.width,
            height: half_height * 2.0 + self.image.height,
        };
        let saved = self.qt_center;
        for index in 0..2 {
            let (scene_start, scene_extent) = if index == 0 {
                (self.scene.x, self.scene.width)
            } else {
                (self.scene.y, self.scene.height)
            };
            let area = self.area.axis(index);
            let left = scene_start * self.zoom;
            let width = scene_extent * self.zoom;
            let minimum = round_bound(left);
            let maximum = round_bound(left + width - area);
            let indent = if minimum >= maximum {
                self.bars[index].minimum = 0.0;
                self.bars[index].maximum = 0.0;
                (area / 2.0).trunc() - (left + (left + width)) / 2.0
            } else {
                let bar = &mut self.bars[index];
                bar.minimum = minimum;
                bar.maximum = maximum;
                bar.page_step = area;
                bar.single_step = (area / 20.0).trunc();
                0.0
            };
            if index == 0 {
                self.indent.x = indent;
            } else {
                self.indent.y = indent;
            }
        }
        let (x, y) = (self.bars[0].value, self.bars[1].value);
        self.scrolled(x, y, notify);
        self.qt_center = saved;
    }

    fn center_on(&mut self, point: Point, notify: bool) {
        let center = Point {
            x: clamp(point.x, 0.0, self.image.width),
            y: clamp(point.y, 0.0, self.image.height),
        };
        self.center = center;
        let x = if truthy(self.indent.x) {
            self.bars[0].value
        } else {
            (center.x * self.zoom - self.area.width / 2.0).trunc()
        };
        let y = if truthy(self.indent.y) {
            self.bars[1].value
        } else {
            (center.y * self.zoom - self.area.height / 2.0).trunc()
        };
        self.scrolled(x, y, notify);
        self.qt_center = center;
    }

    /// Scaling suppresses native updateCenter callbacks. The requested center
    /// stays distinct from integer scrollbar placement until a later action.
    pub fn scaled(&self, zoom: f64, slider: usize) -> Self {
        let mut next = self.clone();
        next.zoom = zoom;
        next.slider = slider;
        next.scene_bounds(false);
        let center = next.center;
        next.center_on(center, false);
        next.presented()
    }

    pub fn fitted(&self, image: Size, area: Size, zoom: f64, slider: usize) -> Self {
        let mut source = self.clone();
        source.image = image;
        source.area = area;
        let mut next = source.scaled(zoom, slider);
        next.center_on(Point { x: image.width / 2.0, y: image.height / 2.0 }, true);
        next.presented()
    }

    pub fn centered(&self) -> Self {
        let mut next = self.clone();
        let center = Point { x: next.image.width / 2.0, y: next.image.height / 2.0 };
        next.center_on(center, true);
        next.presented()
    }

    pub fn wheel_centered(&self, center: Point) -> Self {
        let mut next = self.clone();
        next.center_on(center, false);
        next.presented()
    }

    /// Returns `None` when the requested position equals the current one.
    pub fn scroll(&self, axis: Axis, value: f64) -> Option<Self> {
        let current = self.bars[axis.index()];
        if clamp(value, current.minimum, current.maximum) == current.value {
            return None;
        }
        let mut next = self.clone();
        let mut positions = [next.bars[0].value, next.bars[1].value];
        positions[axis.index()] = value;
        next.scrolled(positions[0], positions[1], true);
        Some(next.presented())
    }

    /// Moves one axis by `steps` scrollbar single steps (trunc(area/20)).
    /// Native toolbar: left -1, right +1, up +1, down -1. Focused-view arrow
    /// keys follow Qt's scroll area instead: Up -1, Down +1.
    pub fn step(&self, axis: Axis, steps: f64) -> Option<Self> {
        let bar = self.bars[axis.index()];
        self.scroll(axis, bar.value + steps * bar.single_step)
    }

    pub fn pan(&self, delta: Point) -> Self {
        let mut next = self.clone();
        let (x, y) = (next.bars[0].value - delta.x.trunc(), next.bars[1].value - delta.y.trunc());
        next.scrolled(x, y, true);
        next.presented()
    }

    pub fn resized(&self, area: Size) -> Self {
        let mut next = self.clone();
        next.area = area;
        next.scene_bounds(true);
        if !next.retain_center_on_resize {
            next.qt_center = next.image_point(Point {
                x: ((area.width - 1.0) / 2.0).trunc() + 0.0,
                y: ((area.height - 1.0) / 2.0).trunc() + 0.0,
            });
        }
        next.retain_center_on_resize = false;
        next.presented()
    }
}
