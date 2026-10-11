use super::{edit_field, Shell, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::motion::fade_in;
use crate::pane_menu::{pane_menu_entries, selectable_commands, step_selection, PaneMenuEntry, PaneMenuItem};
use domain::PanelId;
use gpui::{canvas, div, prelude::*, px, AnyElement, Bounds, Context, Keystroke, MouseButton, Pixels, SharedString, Window};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const MENU_WIDTH_PX: f32 = 240.0;
const MENU_OFFSET_PX: f32 = 4.0;

pub(super) type GearBounds = Rc<RefCell<HashMap<PanelId, Bounds<Pixels>>>>;

pub(super) struct PaneMenu {
    panel: PanelId,
    selected: usize,
}

pub(super) struct PaneField {
    pub(super) panel: PanelId,
    pub(super) text: SharedString,
}

impl Shell {
    pub(super) fn pane_display_title(&self, panel: PanelId) -> String {
        let name = self.model.panel(panel).and_then(|record| record.name.clone());
        let live = self.panes.get(&panel).map(|pane| pane.title.clone()).filter(|title| !title.is_empty());
        name.or(live).unwrap_or_else(|| workspace_service::DEFAULT_PANEL_TITLE.to_string())
    }

    pub(super) fn gear_bounds_probe(&self, panel: PanelId) -> impl IntoElement {
        let sink = self.gear_bounds.clone();
        canvas(
            move |bounds, _window, _cx| {
                sink.borrow_mut().insert(panel, bounds);
            },
            |_bounds, _state, _window, _cx| {},
        )
        .absolute()
        .size_full()
    }

    pub(super) fn toggle_pane_menu(&mut self, panel: PanelId, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_pane(panel);
        window.focus(&self.focus_handle);
        let is_open_here = self.pane_menu.as_ref().is_some_and(|menu| menu.panel == panel);
        self.pane_menu = (!is_open_here).then_some(PaneMenu { panel, selected: 0 });
        cx.notify();
    }

    pub(super) fn is_pane_menu_open(&self) -> bool {
        self.pane_menu.is_some()
    }

    fn close_pane_menu(&mut self, cx: &mut Context<Self>) {
        self.pane_menu = None;
        cx.notify();
    }

    fn menu_commands(&self) -> Vec<crate::command::CommandId> {
        selectable_commands(&pane_menu_entries(self.is_active_maximized(), &self.state.keymap))
    }

    fn is_active_maximized(&self) -> bool {
        self.model.tabs().active().is_some_and(|tab| tab.is_maximized())
    }

    pub(super) fn handle_pane_menu_key(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.menu_commands().len();
        let Some(selected) = self.pane_menu.as_ref().map(|menu| menu.selected) else {
            return;
        };
        let step = match keystroke.key.as_str() {
            ESCAPE_KEY => return self.close_pane_menu(cx),
            ENTER_KEY => return self.run_pane_menu_item(selected, window, cx),
            UP_KEY => -1,
            DOWN_KEY => 1,
            _ => return,
        };
        if let Some(menu) = self.pane_menu.as_mut() {
            menu.selected = step_selection(selected, step, count);
        }
        cx.notify();
    }

    fn run_pane_menu_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.pane_menu.take() else {
            return;
        };
        let Some(command) = self.menu_commands().get(index).copied() else {
            return;
        };
        self.focus_pane(menu.panel);
        window.focus(&self.focus_handle);
        self.run_command(command, window, cx);
    }

    pub(super) fn start_rename_pane(&mut self, panel: PanelId) {
        let text = self.pane_display_title(panel).into();
        self.pane_field = Some(PaneField { panel, text });
    }

    pub(super) fn handle_pane_field_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.pane_field = None,
            ENTER_KEY => self.commit_pane_field(),
            _ => {
                if let Some(field) = self.pane_field.as_mut() {
                    edit_field(&mut field.text, keystroke);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn commit_pane_field(&mut self) {
        let Some(field) = self.pane_field.take() else {
            return;
        };
        let name = Some(field.text.trim().to_string()).filter(|name| !name.is_empty());
        if let Some(record) = self.model.panel_mut(field.panel) {
            record.name = name;
        }
        self.persist();
    }

    pub(super) fn move_focused_to_new_tab(&mut self) {
        let Some(panel) = self.model.tabs().active().map(|tab| tab.focused()) else {
            return;
        };
        if let Err(err) = self.model.tabs_mut().move_panel_to_new_tab(panel) {
            tracing::debug!(?err, "move to new tab rejected");
        }
    }

    pub(super) fn render_pane_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.pane_menu.as_ref()?;
        let anchor = self.gear_bounds.borrow().get(&menu.panel).copied()?;
        let theme = &self.state.theme;
        let entries = pane_menu_entries(self.is_active_maximized(), &self.state.keymap);
        let mut index = 0;
        let rows: Vec<AnyElement> = entries
            .into_iter()
            .map(|entry| match entry {
                PaneMenuEntry::Divider => div().my_1().h(px(1.0)).bg(theme.border_default()).into_any_element(),
                PaneMenuEntry::Item(item) => {
                    let row = self.pane_menu_row(item, index, index == menu.selected, cx);
                    index += 1;
                    row
                }
            })
            .collect();
        let left = (anchor.right() - px(MENU_WIDTH_PX)).max(px(0.0));
        let top = anchor.bottom() + px(MENU_OFFSET_PX);
        let popover = div()
            .absolute()
            .left(left)
            .top(top)
            .w(px(MENU_WIDTH_PX))
            .py_1()
            .occlude()
            .bg(theme.surface.raised)
            .border_1()
            .border_color(theme.border_default())
            .rounded(px(theme.layout.island_radius_px))
            .shadow_md()
            .flex()
            .flex_col()
            .children(rows);
        let backdrop = div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.close_pane_menu(cx)))
            .child(popover);
        Some(fade_in(backdrop, "pane-menu", theme.layout.animation_ms))
    }

    fn pane_menu_row(&self, item: PaneMenuItem, index: usize, is_selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.state.theme;
        let hover_bg = theme.surface.subtle;
        div()
            .id(("pane-menu-item", index))
            .mx_1()
            .px_2()
            .py_1()
            .flex()
            .flex_row()
            .justify_between()
            .gap_3()
            .rounded(px(theme.layout.border_radius_px))
            .cursor_pointer()
            .text_color(theme.text_default())
            .when(is_selected, |row| row.bg(hover_bg))
            .hover(move |style| style.bg(hover_bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_pane_menu_item(index, window, cx);
                }),
            )
            .child(item.title)
            .children(item.shortcut.map(|shortcut| div().text_color(theme.text.muted).child(shortcut)))
            .into_any_element()
    }
}
