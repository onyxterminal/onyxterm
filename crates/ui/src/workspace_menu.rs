#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::Theme;
use gpui::{div, prelude::*, px, App, Div, MouseButton, SharedString, Window};
use std::rc::Rc;

const MENU_WIDTH_PX: f32 = 260.0;
const NEW_WORKSPACE_LABEL: &str = "New workspace\u{2026}";
const NAME_PLACEHOLDER: &str = "Workspace name";
pub const CARET: &str = "\u{258f}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuItem {
    pub id: SharedString,
    pub name: SharedString,
    pub is_active: bool,
    pub is_editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuField {
    pub renaming: Option<SharedString>,
    pub text: SharedString,
}

pub type SelectHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
pub type ClickHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub struct WorkspaceMenuActions {
    pub select: SelectHandler,
    pub rename: SelectHandler,
    pub delete: SelectHandler,
    pub create: ClickHandler,
}

pub fn workspace_menu(
    items: Vec<WorkspaceMenuItem>,
    field: Option<MenuField>,
    theme: &Theme,
    actions: WorkspaceMenuActions,
) -> Div {
    let is_creating = field.as_ref().is_some_and(|field| field.renaming.is_none());
    let rows: Vec<_> = items.into_iter().map(|item| {
        let editing = field.as_ref().filter(|field| field.renaming.as_ref() == Some(&item.id));
        match editing {
            Some(field) => name_field(&field.text, theme).into_any_element(),
            None => menu_row(item, theme, actions.clone()).into_any_element(),
        }
    }).collect();
    let footer = match field.filter(|_| is_creating) {
        Some(field) => name_field(&field.text, theme).into_any_element(),
        None => new_workspace_row(theme, actions.create.clone()).into_any_element(),
    };
    div()
        .w(px(MENU_WIDTH_PX))
        .py_1()
        .bg(theme.surface.raised)
        .border_1()
        .border_color(theme.border_default())
        .rounded(px(theme.layout.island_radius_px))
        .shadow_md()
        .flex()
        .flex_col()
        .children(rows)
        .child(div().my_1().h(px(1.0)).bg(theme.border_default()))
        .child(footer)
}

fn menu_row(item: WorkspaceMenuItem, theme: &Theme, actions: WorkspaceMenuActions) -> impl IntoElement {
    let is_editable = item.is_editable;
    let marker = if item.is_active { "\u{2713}" } else { "" };
    let hover_bg = theme.surface.subtle;
    let id = item.id.clone();
    let rename_id = item.id.clone();
    let delete_id = item.id.clone();
    let rename = actions.rename.clone();
    let delete = actions.delete.clone();
    div()
        .id(item.id.clone())
        .px_3()
        .py_1()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .text_color(theme.text_default())
        .hover(move |style| style.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| (actions.select)(&id, window, cx))
        .child(div().w(px(12.0)).text_color(theme.accent.primary).child(marker))
        .child(div().flex_1().child(item.name))
        .when(is_editable, |row| {
            row.child(row_button(("rename", rename_id.clone()), "Rename", theme, move |window, cx| {
                rename(&rename_id, window, cx)
            }))
            .child(row_button(("delete", delete_id.clone()), "\u{00d7}", theme, move |window, cx| {
                delete(&delete_id, window, cx)
            }))
        })
}

fn row_button(
    id: (&'static str, SharedString),
    label: &'static str,
    theme: &Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover_fg = theme.text_default();
    div()
        .id(SharedString::from(format!("{}-{}", id.0, id.1)))
        .px_1()
        .text_color(theme.text.muted)
        .hover(move |style| style.text_color(hover_fg))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(label)
}

fn new_workspace_row(theme: &Theme, create: ClickHandler) -> impl IntoElement {
    let hover_bg = theme.surface.subtle;
    div()
        .id("new-workspace")
        .px_3()
        .py_1()
        .cursor_pointer()
        .text_color(theme.accent.primary)
        .hover(move |style| style.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| create(window, cx))
        .child(NEW_WORKSPACE_LABEL)
}

fn name_field(text: &SharedString, theme: &Theme) -> impl IntoElement {
    let (content, color) = if text.is_empty() {
        (SharedString::from(NAME_PLACEHOLDER), theme.text.muted)
    } else {
        (text.clone(), theme.text_default())
    };
    div()
        .mx_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border_focus())
        .bg(theme.canvas())
        .flex()
        .flex_row()
        .child(div().text_color(color).child(content))
        .child(div().text_color(theme.accent.primary).child(CARET))
}
