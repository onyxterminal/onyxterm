#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::Theme;
use gpui::{div, prelude::*, px, App, Div, MouseButton, SharedString, Window};
use std::rc::Rc;

const MENU_WIDTH_PX: f32 = 220.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuItem {
    pub id: SharedString,
    pub name: SharedString,
    pub is_active: bool,
}

pub type SelectHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

pub fn workspace_menu(items: Vec<WorkspaceMenuItem>, theme: &Theme, on_select: SelectHandler) -> Div {
    div()
        .w(px(MENU_WIDTH_PX))
        .py_1()
        .bg(theme.surface.raised)
        .border_1()
        .border_color(theme.border_default())
        .rounded_md()
        .shadow_md()
        .flex()
        .flex_col()
        .children(items.into_iter().map(|item| menu_row(item, theme, on_select.clone())))
}

fn menu_row(item: WorkspaceMenuItem, theme: &Theme, on_select: SelectHandler) -> impl IntoElement {
    let marker = if item.is_active { "\u{2713}" } else { "" };
    let hover_bg = theme.surface.subtle;
    let id = item.id.clone();
    div()
        .id(item.id.clone())
        .px_3()
        .py_1()
        .flex()
        .flex_row()
        .justify_between()
        .cursor_pointer()
        .text_color(theme.text_default())
        .hover(move |style| style.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| on_select(&id, window, cx))
        .child(item.name)
        .child(div().text_color(theme.accent.primary).child(marker))
}
