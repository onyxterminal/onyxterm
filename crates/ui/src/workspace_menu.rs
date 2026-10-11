#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::text_field::TextField;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::{
    field_text, focused_field, menu_panel, menu_row, menu_section_label, menu_separator, menu_shortcut, MenuRowStyle,
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
    pub input: TextField,
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
                Some(field) => name_field(&field.input, theme).into_any_element(),
                None => workspace_row(item, theme, actions.clone()).into_any_element(),
            }
        })
        .collect();
    let footer = match field.filter(|_| is_creating) {
        Some(field) => name_field(&field.input, theme).into_any_element(),
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
    let count = menu_shortcut(tab_count_label(item.tab_count), &style, theme);
    let trailing = if item.is_editable {
        let width = theme.pane_header.icon_button_px * 2.0 + ROW_ACTION_GAP_PX;
        // Keep both states in the layout so hover changes cannot skip child prepaint.
        div()
            .relative()
            .w(px(width))
            .h(px(theme.pane_header.icon_button_px))
            .flex_none()
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(count.group_hover(group.clone(), |style| style.invisible())),
            )
            .child(row_actions(&item.id, group, theme, &actions))
            .into_any_element()
    } else {
        count.into_any_element()
    };
    let (select, select_id) = (actions.select.clone(), item.id.clone());
    menu_row(item.id.clone(), style, theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| select(&select_id, window, cx))
        .child(div().w(px(CHECK_COLUMN_PX)).flex_none().child(if item.is_active { CHECK_MARK } else { "" }))
        .child(div().flex_1().min_w(px(0.0)).truncate().child(item.name))
        .child(trailing)
}

fn row_actions(id: &SharedString, group: SharedString, theme: &Theme, actions: &WorkspaceMenuActions) -> Div {
    let (rename, delete) = (actions.rename.clone(), actions.delete.clone());
    let (rename_id, delete_id) = (id.clone(), id.clone());
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(ROW_ACTION_GAP_PX))
        .invisible()
        .group_hover(group, |style| style.visible())
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
        .hover(move |style| style.bg(hover_fill))
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

fn name_field(input: &TextField, theme: &Theme) -> impl IntoElement {
    focused_field(theme)
        .h(px(theme.menu.row_height_px))
        .flex_none()
        .px(px(theme.menu.row_padding_x_px))
        .rounded(px(theme.menu.row_radius_px))
        .flex()
        .items_center()
        .child(field_text(input, NAME_PLACEHOLDER, theme))
}

#[cfg(test)]
mod paint_tests {
    use super::*;
    use gpui::{point, Context, Modifiers, Render, TestAppContext};
    use std::cell::Cell;

    struct TestMenu {
        model: WorkspaceMenuModel,
        theme: Theme,
        actions: WorkspaceMenuActions,
    }

    impl Render for TestMenu {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            workspace_menu(self.model.clone(), &self.theme, self.actions.clone())
        }
    }

    #[test]
    fn workspace_actions_survive_hover_during_paint() {
        let mut app = TestAppContext::single();
        let theme = Theme::load_default().expect("theme");
        let rename_count = Rc::new(Cell::new(0));
        let model = WorkspaceMenuModel {
            items: vec![WorkspaceMenuItem {
                id: "workspace-1".into(),
                name: "Project".into(),
                tab_count: 2,
                is_active: true,
                is_editable: true,
            }],
            field: None,
            new_workspace_shortcut: None,
        };
        let actions = WorkspaceMenuActions {
            select: Rc::new(|_, _, _| {}),
            rename: {
                let rename_count = rename_count.clone();
                Rc::new(move |_, _, _| rename_count.set(rename_count.get() + 1))
            },
            delete: Rc::new(|_, _, _| {}),
            create: Rc::new(|_, _| {}),
        };
        let row_y = theme.menu.padding_px + theme.menu.section_label_height_px + theme.menu.row_height_px / 2.0;
        let action_x = theme.menu.width_px
            - theme.menu.padding_px
            - theme.menu.row_padding_x_px
            - theme.pane_header.icon_button_px * 1.5
            - ROW_ACTION_GAP_PX;
        let (_menu, cx) = app.add_window_view(|_, _| TestMenu { model, theme, actions });
        cx.simulate_mouse_move(point(px(action_x), px(row_y)), None, Modifiers::default());
        cx.simulate_mouse_down(point(px(action_x), px(row_y)), MouseButton::Left, Modifiers::default());
        assert_eq!(rename_count.get(), 1);
        cx.simulate_mouse_move(point(px(390.0), px(290.0)), None, Modifiers::default());
    }
}
