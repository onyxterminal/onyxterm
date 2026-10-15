#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::text_field::TextField;
use crate::theme::icons::Icon;
use crate::theme::menu_chrome::{
    field_text, menu_row, menu_section_label, menu_separator, menu_shortcut, popover_panel, MenuRowStyle,
};
use crate::theme::Theme;
use crate::workspaces::SCRATCH_NAME;
use domain::SshHost;
use gpui::{div, prelude::*, px, App, Div, MouseButton, SharedString, Window};
use std::rc::Rc;

const SECTION_LABEL: &str = "Workspaces";
const SSH_SECTION_LABEL: &str = "SSH hosts";
const NEW_WORKSPACE_LABEL: &str = "New Workspace\u{2026}";
const SETTINGS_LABEL: &str = "Settings\u{2026}";
const NAME_PLACEHOLDER: &str = "Workspace name";
const SEARCH_PLACEHOLDER: &str = "Filter workspaces";
const COMMIT_HINT: &str = "\u{21a9}";
const CHECK_MARK: &str = "\u{2713}";
const META_SEPARATOR: &str = " \u{00b7} ";
const CHECK_COLUMN_PX: f32 = 14.0;
const ROW_GAP_PX: f32 = 2.0;
const ROW_LINE_GAP_PX: f32 = 1.0;
const ROW_ICON_BUTTON_PX: f32 = 26.0;
const ROW_ICON_PX: f32 = 12.0;
const SEARCH_HEIGHT_PX: f32 = 30.0;
const SEARCH_PADDING_X_PX: f32 = 9.0;
const SEARCH_MARGIN_X_PX: f32 = 2.0;
const SEARCH_MARGIN_TOP_PX: f32 = 2.0;
const SEARCH_MARGIN_BOTTOM_PX: f32 = 4.0;
const SEARCH_RADIUS_PX: f32 = 7.0;
const SEARCH_GAP_PX: f32 = 7.0;
const SEARCH_ICON_PX: f32 = 13.0;
const NAME_FIELD_HEIGHT_PX: f32 = 32.0;
const NAME_FIELD_MARGIN_Y_PX: f32 = 2.0;
const NAME_FIELD_RADIUS_PX: f32 = 6.0;
const NAME_FIELD_GAP_PX: f32 = 8.0;
const NEW_WORKSPACE_GROUP: &str = "new-workspace";
const SETTINGS_GROUP: &str = "workspace-settings";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuItem {
    pub id: SharedString,
    pub name: SharedString,
    pub path: SharedString,
    pub tab_count: usize,
    pub shortcut: Option<String>,
    pub is_active: bool,
    pub can_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuField {
    pub renaming: Option<SharedString>,
    pub input: TextField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenuModel {
    pub items: Vec<WorkspaceMenuItem>,
    pub ssh_hosts: Vec<SshHost>,
    pub field: Option<MenuField>,
    pub search: TextField,
    pub new_workspace_shortcut: Option<String>,
    pub settings_shortcut: Option<String>,
}

pub type SelectHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
pub type ClickHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub struct WorkspaceMenuActions {
    pub select: SelectHandler,
    pub delete: SelectHandler,
    pub create: ClickHandler,
    pub connect: SelectHandler,
    pub open_settings: ClickHandler,
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

pub fn workspace_meta(tab_count: usize, path: &str) -> String {
    match tab_count {
        0 => path.to_string(),
        count => format!("{}{META_SEPARATOR}{path}", tab_count_label(count)),
    }
}

pub fn ssh_target(host: &SshHost) -> String {
    let Some(hostname) = host.hostname.as_deref() else {
        return String::new();
    };
    let user = host.user.as_deref().map(|user| format!("{user}@")).unwrap_or_default();
    let port = host.port.map(|port| format!(":{port}")).unwrap_or_default();
    format!("{user}{hostname}{port}")
}

pub fn workspace_menu(model: WorkspaceMenuModel, theme: &Theme, actions: WorkspaceMenuActions) -> Div {
    let WorkspaceMenuModel { items, ssh_hosts, field, search, new_workspace_shortcut, settings_shortcut } = model;
    let is_creating = field.as_ref().is_some_and(|field| field.renaming.is_none());
    let needle = search.text().trim().to_ascii_lowercase();
    let rows: Vec<_> = filter_workspaces(items, &needle)
        .into_iter()
        .map(|item| {
            let editing = field.as_ref().filter(|field| field.renaming.as_ref() == Some(&item.id));
            match editing {
                Some(field) => name_field(&field.input, theme).into_any_element(),
                None => workspace_row(item, theme, actions.clone()).into_any_element(),
            }
        })
        .collect();
    let ssh_hosts = filter_ssh_hosts(ssh_hosts, &needle);
    let ssh_section = (!ssh_hosts.is_empty()).then(|| ssh_section(ssh_hosts, theme, &actions));
    let new_workspace = match field.filter(|_| is_creating) {
        Some(field) => name_field(&field.input, theme).into_any_element(),
        None => new_workspace_row(new_workspace_shortcut, theme, actions.create.clone()).into_any_element(),
    };
    popover_panel(theme)
        .child(search_field(&search, theme))
        .child(menu_section_label(SECTION_LABEL, theme))
        .children(rows)
        .child(new_workspace)
        .children(ssh_section)
        .child(menu_separator(theme))
        .child(settings_row(settings_shortcut, theme, actions.open_settings.clone()))
}

fn filter_workspaces(items: Vec<WorkspaceMenuItem>, needle: &str) -> Vec<WorkspaceMenuItem> {
    if needle.is_empty() {
        return items;
    }
    items.into_iter().filter(|item| item.name.to_ascii_lowercase().contains(needle)).collect()
}

fn filter_ssh_hosts(hosts: Vec<SshHost>, needle: &str) -> Vec<SshHost> {
    if needle.is_empty() {
        return hosts;
    }
    hosts.into_iter().filter(|host| ssh_matches(host, needle)).collect()
}

fn ssh_matches(host: &SshHost, needle: &str) -> bool {
    host.alias.to_ascii_lowercase().contains(needle)
        || host
            .hostname
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
        || host
            .user
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
}

fn search_field(search: &TextField, theme: &Theme) -> impl IntoElement {
    div()
        .h(px(SEARCH_HEIGHT_PX))
        .flex_none()
        .mx(px(SEARCH_MARGIN_X_PX))
        .mt(px(SEARCH_MARGIN_TOP_PX))
        .mb(px(SEARCH_MARGIN_BOTTOM_PX))
        .px(px(SEARCH_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SEARCH_GAP_PX))
        .rounded(px(SEARCH_RADIUS_PX))
        .bg(theme.chrome.control_fill)
        .child(Icon::MagnifierBold.sized(SEARCH_ICON_PX, theme.text.subtle))
        .child(field_text(search, SEARCH_PLACEHOLDER, theme))
}

fn row_style(group: SharedString) -> MenuRowStyle {
    MenuRowStyle { group, is_highlighted: false, is_destructive: false }
}

fn clipped_line(text: impl Into<SharedString>) -> Div {
    div().min_w(px(0.0)).overflow_hidden().whitespace_nowrap().child(text.into())
}

fn meta_text(text: impl Into<SharedString>, theme: &Theme) -> Div {
    clipped_line(text).text_size(px(theme.ui_font.hint_px)).text_color(theme.text.subtle)
}

fn ssh_section(hosts: Vec<SshHost>, theme: &Theme, actions: &WorkspaceMenuActions) -> Div {
    div()
        .flex()
        .flex_col()
        .child(menu_separator(theme))
        .child(menu_section_label(SSH_SECTION_LABEL, theme))
        .children(hosts.into_iter().map(|host| ssh_row(host, theme, actions.connect.clone())))
}

fn ssh_row(host: SshHost, theme: &Theme, connect: SelectHandler) -> impl IntoElement {
    let group = SharedString::from(format!("ssh-row-{}", host.alias));
    let id = SharedString::from(format!("ssh-host-{}", host.alias));
    let alias = SharedString::from(host.alias.clone());
    let target = ssh_target(&host);
    menu_row(id, row_style(group), theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| connect(&alias, window, cx))
        .child(div().flex_none().child(host.alias))
        .child(meta_text(target, theme))
}

fn workspace_row(item: WorkspaceMenuItem, theme: &Theme, actions: WorkspaceMenuActions) -> impl IntoElement {
    let group = SharedString::from(format!("workspace-row-{}", item.id));
    let (select, select_id) = (actions.select.clone(), item.id.clone());
    let check = div()
        .w(px(CHECK_COLUMN_PX))
        .flex_none()
        .text_color(theme.accent.primary)
        .when(item.is_active, |check| check.child(CHECK_MARK));
    let lines = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(ROW_LINE_GAP_PX))
        .child(clipped_line(item.name.clone()))
        .child(meta_text(workspace_meta(item.tab_count, &item.path), theme));
    let row = menu_row(item.id.clone(), row_style(group), theme)
        .flex_1()
        .min_w(px(0.0))
        .h(px(theme.menu.popover_row_height_px))
        .justify_start()
        .when(item.is_active, |row| row.bg(theme.chrome.control_fill))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| select(&select_id, window, cx))
        .child(check)
        .child(lines)
        .children(item.shortcut.map(|shortcut| menu_shortcut(shortcut, theme)));
    let delete = match item.can_delete {
        true => delete_button(&item.id, theme, actions.delete.clone()).into_any_element(),
        false => div().flex_none().size(px(ROW_ICON_BUTTON_PX)).into_any_element(),
    };
    div().flex().flex_row().items_center().gap(px(ROW_GAP_PX)).child(row).child(delete)
}

fn delete_button(id: &SharedString, theme: &Theme, delete: SelectHandler) -> impl IntoElement {
    let delete_id = id.clone();
    div()
        .id(SharedString::from(format!("delete-{id}")))
        .flex_none()
        .size(px(ROW_ICON_BUTTON_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(theme.menu.row_radius_px))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            cx.stop_propagation();
            delete(&delete_id, window, cx);
        })
        .child(Icon::Trash.sized(ROW_ICON_PX, theme.text.subtle))
}

fn new_workspace_row(shortcut: Option<String>, theme: &Theme, create: ClickHandler) -> impl IntoElement {
    menu_row(NEW_WORKSPACE_GROUP, row_style(NEW_WORKSPACE_GROUP.into()), theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| create(window, cx))
        .child(div().child(NEW_WORKSPACE_LABEL))
        .children(shortcut.map(|shortcut| menu_shortcut(shortcut, theme)))
}

fn settings_row(shortcut: Option<String>, theme: &Theme, open_settings: ClickHandler) -> impl IntoElement {
    menu_row(SETTINGS_GROUP, row_style(SETTINGS_GROUP.into()), theme)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| open_settings(window, cx))
        .child(div().child(SETTINGS_LABEL))
        .children(shortcut.map(|shortcut| menu_shortcut(shortcut, theme)))
}

fn name_field(input: &TextField, theme: &Theme) -> impl IntoElement {
    div()
        .h(px(NAME_FIELD_HEIGHT_PX))
        .flex_none()
        .my(px(NAME_FIELD_MARGIN_Y_PX))
        .px(px(theme.menu.row_padding_x_px))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(NAME_FIELD_GAP_PX))
        .rounded(px(NAME_FIELD_RADIUS_PX))
        .border_1()
        .border_color(theme.border.focus)
        .bg(theme.chrome.control_fill)
        .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(field_text(input, NAME_PLACEHOLDER, theme)))
        .child(menu_shortcut(COMMIT_HINT, theme))
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
        let delete_count = Rc::new(Cell::new(0));
        let model = WorkspaceMenuModel {
            items: vec![WorkspaceMenuItem {
                id: "workspace-1".into(),
                name: "Project".into(),
                path: "~/project".into(),
                tab_count: 2,
                shortcut: None,
                is_active: true,
                can_delete: true,
            }],
            ssh_hosts: Vec::new(),
            field: None,
            search: TextField::default(),
            new_workspace_shortcut: None,
            settings_shortcut: None,
        };
        let actions = WorkspaceMenuActions {
            select: Rc::new(|_, _, _| {}),
            delete: {
                let delete_count = delete_count.clone();
                Rc::new(move |_, _, _| delete_count.set(delete_count.get() + 1))
            },
            create: Rc::new(|_, _| {}),
            connect: Rc::new(|_, _, _| {}),
            open_settings: Rc::new(|_, _| {}),
        };
        let row_y = theme.menu.popover_padding_px
            + SEARCH_MARGIN_TOP_PX
            + SEARCH_HEIGHT_PX
            + SEARCH_MARGIN_BOTTOM_PX
            + theme.menu.section_label_height_px
            + theme.menu.popover_row_height_px / 2.0;
        let action_x = theme.menu.popover_width_px - theme.menu.popover_padding_px - ROW_ICON_BUTTON_PX * 0.5;
        let (_menu, cx) = app.add_window_view(|_, _| TestMenu { model, theme, actions });
        cx.simulate_mouse_move(point(px(action_x), px(row_y)), None, Modifiers::default());
        cx.simulate_mouse_down(point(px(action_x), px(row_y)), MouseButton::Left, Modifiers::default());
        assert_eq!(delete_count.get(), 1);
        cx.simulate_mouse_move(point(px(390.0), px(290.0)), None, Modifiers::default());
    }
}
