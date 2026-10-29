use std::ops::Range;
use gpui::{prelude::*, App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle, Focusable, KeyDownEvent, MouseButton, Pixels, Point, Role, ShapedLine, TextRun, UTF16Selection, Window, canvas, div, fill, point, px, rgb, size};
use unicode_segmentation::UnicodeSegmentation;
use crate::theme;

pub struct TextField {
    pub text: String,
    label: String,
    focus: FocusHandle,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
}
impl TextField {
    pub fn new(label: impl Into<String>, text: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let text = text.into(); let end = text.len();
        // Tracked handles own tab navigation; Div flags only configure implicit handles.
        Self { text, label: label.into(), focus: cx.focus_handle().tab_stop(true), selected: end..end, reversed: false, marked: None, line: None, bounds: None, selecting: false }
    }
    pub fn set(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.text = text.into(); self.selected = self.text.len()..self.text.len(); self.marked = None; cx.notify();
    }
    pub fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx); self.selected = 0..self.text.len(); cx.notify();
    }
    fn byte(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (index, character) in self.text.char_indices() { if units >= utf16 { return index; } units += character.len_utf16(); }
        self.text.len()
    }
    fn utf16(&self, byte: usize) -> usize { self.text[..byte].encode_utf16().count() }
    fn byte_range(&self, range: Range<usize>) -> Range<usize> { self.byte(range.start)..self.byte(range.end) }
    fn cursor(&self) -> usize { if self.reversed { self.selected.start } else { self.selected.end } }
    fn move_cursor(&mut self, index: usize, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = if self.reversed { self.selected.end } else { self.selected.start };
        self.selected = if shift { anchor.min(index)..anchor.max(index) } else { index..index };
        self.reversed = shift && index < anchor;
        window.notify_text_selection_changed(&self.focus); cx.notify();
    }
    fn index(&self, position: Point<Pixels>) -> usize {
        self.line.as_ref().zip(self.bounds).map_or(0, |(line, bounds)| line.closest_index_for_x(position.x - bounds.left()))
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke; let shortcut = key.modifiers.control || key.modifiers.platform;
        let cursor = self.cursor();
        let previous = self.text[..cursor].grapheme_indices(true).next_back().map_or(0, |(i, _)| i);
        let next = self.text[cursor..].grapheme_indices(true).nth(1).map_or(self.text.len(), |(i, _)| cursor + i);
        match key.key.as_str() {
            "a" if shortcut => { self.selected = 0..self.text.len(); self.reversed = false; },
            "c" | "x" if shortcut => {
                if !self.selected.is_empty() { cx.write_to_clipboard(ClipboardItem::new_string(self.text[self.selected.clone()].into())); }
                if key.key == "x" { self.replace_text_in_range(None, "", window, cx); }
            },
            "v" if shortcut => if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) { self.replace_text_in_range(None, &text, window, cx); },
            "left" => self.move_cursor(if !key.modifiers.shift && !self.selected.is_empty() { self.selected.start } else { previous }, key.modifiers.shift, window, cx),
            "right" => self.move_cursor(if !key.modifiers.shift && !self.selected.is_empty() { self.selected.end } else { next }, key.modifiers.shift, window, cx),
            "home" => self.move_cursor(0, key.modifiers.shift, window, cx),
            "end" => self.move_cursor(self.text.len(), key.modifiers.shift, window, cx),
            "backspace" => { if self.selected.is_empty() { self.selected = previous..cursor; } self.replace_text_in_range(None, "", window, cx); },
            "delete" => { if self.selected.is_empty() { self.selected = cursor..next; } self.replace_text_in_range(None, "", window, cx); },
            // GPUI inserts printable text through EntityInputHandler after key dispatch.
            // Stopping an unhandled key here suppresses that insertion.
            _ => return,
        }
        window.notify_text_selection_changed(&self.focus); cx.notify(); cx.stop_propagation();
    }
}
impl Focusable for TextField { fn focus_handle(&self, _: &App) -> FocusHandle { self.focus.clone() } }
impl EntityInputHandler for TextField {
    fn text_for_range(&mut self, range: Range<usize>, actual: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let range = self.byte_range(range); *actual = Some(self.utf16(range.start)..self.utf16(range.end)); Some(self.text[range].into())
    }
    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.utf16(self.selected.start)..self.utf16(self.selected.end), reversed: self.reversed })
    }
    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.byte_range(range); self.reversed = false; window.notify_text_selection_changed(&self.focus); cx.notify();
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { Some(self.text.encode_utf16().count()) }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> { self.marked.as_ref().map(|r| self.utf16(r.start)..self.utf16(r.end)) }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.marked = None; window.notify_text_input_changed(&self.focus); cx.notify(); }
    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let range = range.map(|r| self.byte_range(r)).or_else(|| self.marked.clone()).unwrap_or_else(|| self.selected.clone());
        let text = text.replace(['\n', '\r'], " "); self.text.replace_range(range.clone(), &text);
        let end = range.start + text.len(); self.selected = end..end; self.marked = None; self.reversed = false;
        window.notify_text_input_changed(&self.focus); cx.notify();
    }
    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        let range = range.map(|r| self.byte_range(r)).or_else(|| self.marked.clone()).unwrap_or_else(|| self.selected.clone());
        self.text.replace_range(range.clone(), text); self.marked = (!text.is_empty()).then_some(range.start..range.start + text.len());
        let start = self.utf16(range.start); let length = text.encode_utf16().count();
        self.selected = selected.map(|r| self.byte_range(start + r.start.min(length)..start + r.end.min(length))).unwrap_or(range.start + text.len()..range.start + text.len());
        self.reversed = false; window.notify_text_input_changed(&self.focus); cx.notify();
    }
    fn bounds_for_range(&mut self, range: Range<usize>, bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let range = self.byte_range(range); let line = self.line.as_ref()?;
        let bounds = self.bounds.unwrap_or(bounds);
        Some(Bounds::from_corners(point(bounds.left() + line.x_for_index(range.start), bounds.top()), point(bounds.left() + line.x_for_index(range.end), bounds.bottom())))
    }
    fn character_index_for_point(&mut self, position: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { Some(self.utf16(self.index(position))) }
}
impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity(); let focus = self.focus.clone();
        div().id("text-field").track_focus(&focus).role(Role::TextInput).aria_label(self.label.clone())
            .h(px(28.0)).min_h(px(28.0)).w_full().px_2().bg(rgb(theme::RAISED)).border_1().border_color(rgb(theme::BORDER)).rounded(px(theme::CONTROL_RADIUS))
            .focus_visible(|style| style.border_color(rgb(theme::ACCENT))).overflow_hidden()
            .on_key_down(cx.listener(Self::key))
            .on_key_up(cx.listener(|_, _: &gpui::KeyUpEvent, _, cx| cx.stop_propagation()))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                this.focus.focus(window, cx); this.selecting = true; let index = this.index(event.position);
                this.move_cursor(index, event.modifiers.shift, window, cx); window.request_virtual_keyboard(); cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| { if this.selecting { this.move_cursor(this.index(event.position), true, window, cx); } }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.selecting = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.selecting = false))
            .child(canvas(|_, _, _| (), move |bounds, _, window, cx| {
                entity.update(cx, |field, cx| {
                    let style = window.text_style();
                    let line = window.text_system().shape_line(field.text.clone().into(), px(theme::UI_FONT_SIZE), &[TextRun { len: field.text.len(), font: style.font(), color: rgb(theme::TEXT).into(), background_color: None, underline: None, strikethrough: None }], None);
                    window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
                    let offset = (line.x_for_index(field.cursor()) - bounds.size.width + px(4.0)).max(px(0.0));
                    let origin = point(bounds.left() - offset, bounds.top() + px(5.0));
                    if focus.is_focused(window) {
                        let left = line.x_for_index(field.selected.start); let right = line.x_for_index(field.selected.end);
                        let color = if field.selected.is_empty() { rgb(theme::TEXT) } else { rgb(theme::ACCENT) };
                        window.paint_quad(fill(Bounds::new(point(origin.x + left, origin.y), size((right - left).max(px(1.0)), px(17.0))), color));
                    }
                    if let Some(marked) = &field.marked {
                        let left = line.x_for_index(marked.start); let right = line.x_for_index(marked.end);
                        window.paint_quad(fill(Bounds::new(point(origin.x + left, origin.y + px(16.0)), size(right - left, px(1.0))), rgb(theme::TEXT)));
                    }
                    if let Err(error) = line.paint(origin, px(18.0), gpui::TextAlign::Left, None, window, cx) { eprintln!("Text rendering failed: {error}"); }
                    field.line = Some(line); field.bounds = Some(Bounds::new(point(bounds.left() - offset, bounds.top()), bounds.size));
                });
            }).size_full())
    }
}
