use gpui::{div, prelude::*, px, rgb, Div, Role, Stateful};
use crate::theme;

pub fn button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>, enabled: bool, selected: bool) -> Stateful<Div> {
    let label = label.into();
    div().id(id).role(Role::Button).aria_label(label.clone()).tab_index(0).tab_stop(enabled)
        .h(px(theme::CONTROL_HEIGHT)).min_h(px(theme::CONTROL_HEIGHT)).px_2().flex().items_center().justify_center()
        .rounded(px(theme::CONTROL_RADIUS)).border_1()
        .border_color(rgb(if selected { theme::ACCENT } else { theme::BORDER }))
        .text_color(rgb(if enabled { theme::TEXT } else { theme::MUTED }))
        .bg(rgb(if selected { theme::RAISED } else { theme::BACKGROUND }))
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(rgb(theme::HOVER))))
        .when(!enabled, |d| d.opacity(0.45).aria_description("Unavailable"))
        .focus_visible(|s| s.border_color(rgb(theme::ACCENT)))
        .child(label)
}

pub fn row(label: impl Into<gpui::SharedString>, control: impl IntoElement) -> Div {
    div().flex().items_center().justify_between().gap_2().min_h(px(32.0))
        .child(div().flex_1().child(label.into())).child(control)
}

pub fn section(label: &'static str) -> Div {
    div().mt_3().mb_1().text_color(rgb(theme::MUTED)).child(label)
}
