#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::icons::Icon;
pub use crate::theme::menu_chrome::CARET;
use crate::theme::menu_chrome::{
    caret_text, focused_field, menu_panel, menu_row, menu_section_label, menu_separator, menu_shortcut, MenuRowStyle,
};
use crate::theme::Theme;
use crate::workspaces::SCRATCH_NAME;
use gpui::{div, prelude::*, px, App, Div, MouseButton, SharedString, Window};
use std::rc::Rc;

const SECTION_LABEL: &str = "Workspaces";
const NEW_WORKSPACE_LABEL: &str = "New Workspace\u{2026}";
const NAME_PLACEHOLDER: &str = "Workspace name";
const CHECK_MARK: &str = "\u{2713}";
const CHECK_COLUMN_PX: f32 = 18.0;
const ROW_ACTION_GAP_PX: f32 = 2.0;
const ROW_ACTION_ICON_PX: f32 = 13.0;
const NEW_WORKSPACE_GROUP: &str = "new-workspace";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuItem {
    pub id: SharedString,
    pub name: SharedString,
    pub tab_count: usize,
    pub is_active: bool,
    pub is_editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuField {
    pub renaming: Option<SharedString>,
    pub text: SharedString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuModel {
    pub items: Vec<WorkspaceMenuItem>,
    pub field: Option<MenuField>,
    pub new_workspace_shortcut: Option<String>,
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

pub fn workspace_button_label(name: Option<&str>) -> &str {
    name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(SCRATCH_NAME)
}

pub fn tab_count_label(count: usize) -> String {
    match count {
        1 => "1 tab".to_string(),
        n => format!("{n} tabs"),
    }
}

pub fn workspace_menu(model: WorkspaceMenuModel, theme: &Theme, actions: WorkspaceMenuActions) -> Div {
    let WorkspaceMenuModel { items, field, new_workspace_shortcut } = model;
    let is_creating = field.as_ref().is_some_and(|field| field.renaming.is_none());
    let rows: Vec<_> = items
        .into_iter()
        .map(|item| {
            let editing = field.as_ref().filter(|field| field.renaming.as_ref() == Some(&item.id));
            match editing {
                Some(field) => name_field(&field.text, theme).into_any_element(),
                None => workspace_row(item, theme, actions.clone()).into_any_element(),
            }
        })
        .collect();
    let footer = match field.filter(|_| is_creating) {
        Some(field) => name_field(&field.text, theme).into_any_element(),
        None => new_workspace_row(new_workspace_shortcut, theme, actions.create.clone()).into_any_element(),
    };
    menu_panel(theme)
        .child(menu_section_label(SECTION_LABEL, theme))
        .children(rows)
        .child(menu_separator(theme))
        .child(footer)
}

fn row_style(group: SharedString) -> MenuRowStyle {
    MenuRowStyle { group, is_highlighted: false, is_destructive: false }
}

fn workspace_row(item: WorkspaceMenuItem, theme: &Theme, actions: WorkspaceMenuActions) -> impl IntoElement {
    let group = SharedString::from(format!("workspace-row-{}", item.id));
    let style = row_style(group.clone());
    let count = menu_shortcut(tab_count_label(item.tab_count), &style, theme)
        .when(item.is_editable, |count| count.group_hover(group.clone(), |count| count.hidden()));
    let (select, select_id) = (actions.select.clone(), item.id.clone());
    menu_row(item.id.clone(), style, theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| select(&select_id, window, cx))
        .child(div().w(px(CHECK_COLUMN_PX)).flex_none().child(if item.is_active { CHECK_MARK } else { "" }))
        .child(div().flex_1().min_w(px(0.0)).truncate().child(item.name))
        .child(count)
        .when(item.is_editable, |row| row.child(row_actions(&item.id, group, theme, &actions)))
}

fn row_actions(id: &SharedString, group: SharedString, theme: &Theme, actions: &WorkspaceMenuActions) -> Div {
    let (rename, delete) = (actions.rename.clone(), actions.delete.clone());
    let (rename_id, delete_id) = (id.clone(), id.clone());
    div()
        .hidden()
        .flex_none()
        .flex_row()
        .items_center()
        .gap(px(ROW_ACTION_GAP_PX))
        .group_hover(group, |actions| actions.flex())
        .child(row_button(format!("rename-{id}"), Icon::Pencil, theme, move |window, cx| {
            rename(&rename_id, window, cx)
        }))
        .child(row_button(format!("delete-{id}"), Icon::Trash, theme, move |window, cx| {
            delete(&delete_id, window, cx)
        }))
}

fn row_button(
    id: String,
    icon: Icon,
    theme: &Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let header = theme.pane_header;
    let hover_fill = theme.chrome.icon_hover_fill;
    div()
        .id(SharedString::from(id))
        .size(px(header.icon_button_px))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(header.icon_button_radius_px))
        .hover(move |button| button.bg(hover_fill))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(icon.sized(ROW_ACTION_ICON_PX))
}

fn new_workspace_row(shortcut: Option<String>, theme: &Theme, create: ClickHandler) -> impl IntoElement {
    let style = row_style(NEW_WORKSPACE_GROUP.into());
    let shortcut = shortcut.map(|shortcut| menu_shortcut(shortcut, &style, theme));
    menu_row(NEW_WORKSPACE_GROUP, style, theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| create(window, cx))
        .child(div().w(px(CHECK_COLUMN_PX)).flex_none())
        .child(div().flex_1().child(NEW_WORKSPACE_LABEL))
        .children(shortcut)
}

fn name_field(text: &SharedString, theme: &Theme) -> impl IntoElement {
    focused_field(theme)
        .h(px(theme.menu.row_height_px))
        .flex_none()
        .px(px(theme.menu.row_padding_x_px))
        .rounded(px(theme.menu.row_radius_px))
        .flex()
        .items_center()
        .child(caret_text(text, NAME_PLACEHOLDER, theme))
}
