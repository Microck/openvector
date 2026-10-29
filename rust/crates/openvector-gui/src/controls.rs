use gpui::{div, prelude::*, px, rgb, Div, Role, Stateful};
use crate::theme;

pub fn button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool, selected: bool) -> Stateful<Div> {
    icon_button(id, label, enabled, selected, None)
}

pub fn icon_button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool, selected: bool, icon: Option<&'static str>) -> Stateful<Div> {
    unfilled_button(id, label, enabled, selected, icon).when(enabled, |d| d.hover(|s| s.bg(rgb(theme::HOVER))))
}

/// A button without the hover fill, for controls whose fill carries meaning
/// (palette swatches). GPUI allows one hover style per element, so callers add
/// their own instead of overriding icon_button's.
pub fn unfilled_button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool, selected: bool, icon: Option<&'static str>) -> Stateful<Div> {
    let label = label.into();
    div().id(id).role(Role::Button).aria_label(label.clone()).tab_index(0).tab_stop(enabled)
        .h(px(theme::CONTROL_HEIGHT)).min_h(px(theme::CONTROL_HEIGHT)).px_2().flex().gap_1().items_center().justify_center()
        .rounded(px(theme::CONTROL_RADIUS)).border_1()
        .border_color(rgb(if selected { theme::ACCENT } else { theme::CONTROL_BORDER }))
        .text_color(rgb(if enabled { theme::TEXT } else { theme::MUTED }))
        .bg(rgb(if selected { theme::SELECTED } else { theme::CONTROL }))
        // Pressed: drop to the background fill so a click is visibly acknowledged.
        .when(enabled, |d| d.cursor_pointer().active(|s| s.bg(rgb(theme::BACKGROUND))))
        .when(!enabled, |d| d.opacity(0.45).aria_description("Unavailable"))
        // Neutral ring: blue is reserved for "selected", so focus must differ.
        .focus_visible(|s| s.border_color(rgb(theme::TEXT)))
        .children(icon.map(|path| gpui::svg().path(path).size(px(15.0)).flex_none().text_color(rgb(theme::TEXT))))
        // An empty label would still take a flex gap and push icons off centre.
        .when(!label.is_empty(), |d| d.child(label))
}

pub fn row(label: impl Into<gpui::SharedString>, control: impl IntoElement) -> Div {
    div().flex().items_center().justify_between().gap_2().min_h(px(32.0))
        .child(div().flex_1().child(label.into())).child(control)
}

pub fn section(label: &'static str) -> Div {
    div().mt_3().mb_1().text_color(rgb(theme::MUTED)).child(label)
}

/// A titled block of related controls. Groups are separated by a rule so
/// a long page reads as a few distinct decisions instead of one list.
pub fn group(label: &'static str, body: impl IntoElement) -> Div {
    div().flex().flex_col().gap_2().pb_3().mb_3().border_b_1().border_color(rgb(theme::BORDER))
        .child(div().text_size(px(11.0)).text_color(rgb(theme::MUTED)).child(label.to_uppercase()))
        .child(body)
}

/// Lays out choice buttons side by side with equal widths.
pub fn segmented(children: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    div().flex().gap_1().children(children.into_iter().map(|child| child.flex_1().min_w_0()))
}

/// Filled accent button for the single main action on a page.
pub fn primary_button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool) -> Stateful<Div> {
    unfilled_button(id, label, enabled, false, None).bg(rgb(theme::ACCENT)).border_color(rgb(theme::ACCENT))
        .text_color(rgb(0xffffff)).when(enabled, |d| d.hover(|s| s.opacity(0.88)))
}

/// A text tooltip keeps compact viewer controls discoverable with a pointer.
struct Hint(gpui::SharedString);
impl gpui::Render for Hint {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().px_2().py_1().rounded(px(4.0)).border_1().border_color(rgb(theme::BORDER))
            .bg(rgb(theme::RAISED)).text_color(rgb(theme::TEXT)).text_size(px(theme::UI_FONT_SIZE)).child(self.0.clone())
    }
}
pub fn hint(control: Stateful<Div>, text: impl Into<gpui::SharedString>) -> Stateful<Div> {
    let text = text.into();
    control.tooltip(move |_, cx| cx.new(|_| Hint(text.clone())).into())
}
