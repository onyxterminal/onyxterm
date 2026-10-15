use super::{FieldTarget, Shell, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::motion::fade_in;
use crate::pane_menu::{
    pane_menu_entries, selectable_commands, step_selection, PaneMenuEntry, PaneMenuItem, PaneMenuState,
};
use crate::text_field::TextField;
use crate::theme::menu_chrome::{menu_panel, menu_row, menu_separator, menu_shortcut, MenuRowStyle};
use domain::PanelId;
use gpui::{div, prelude::*, px, AnyElement, Context, Keystroke, MouseButton, SharedString, Window};

pub(super) struct PaneMenu {
    panel: PanelId,
    selected: usize,
}

pub(super) struct PaneField {
    pub(super) panel: PanelId,
    pub(super) input: TextField,
}

impl Shell {
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
        selectable_commands(&pane_menu_entries(self.pane_menu_state(), &self.state.keymap))
    }

    fn pane_menu_state(&self) -> PaneMenuState {
        PaneMenuState {
            is_maximized: self.model.tabs().active().is_some_and(|tab| tab.is_maximized()),
            is_copy_mode: self.is_copy_mode_active(),
        }
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
        let input = TextField::new(self.pane_display_title(panel));
        self.pane_field = Some(PaneField { panel, input });
    }

    pub(super) fn handle_pane_field_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.pane_field = None,
            ENTER_KEY => self.commit_pane_field(),
            _ => return self.type_into_field(FieldTarget::PaneName, keystroke, cx),
        }
        cx.notify();
        true
    }

    pub(super) fn commit_pane_field(&mut self) {
        let Some(field) = self.pane_field.take() else {
            return;
        };
        let name = Some(field.input.text().trim().to_string()).filter(|name| !name.is_empty());
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
        let theme = &self.state.theme;
        let island = self.stage.island_bounds().into_iter().find(|(panel, _)| *panel == menu.panel)?.1;
        let entries = pane_menu_entries(self.pane_menu_state(), &self.state.keymap);
        let mut index = 0;
        let rows: Vec<AnyElement> = entries
            .into_iter()
            .map(|entry| match entry {
                PaneMenuEntry::Divider => menu_separator(theme).into_any_element(),
                PaneMenuEntry::Item(item) => {
                    let row = self.pane_menu_row(item, index, index == menu.selected, cx);
                    index += 1;
                    row
                }
            })
            .collect();
        let left = (island.right() - px(theme.pane_header.padding_right_px + theme.menu.width_px)).max(px(0.0));
        let top = island.top() + px(theme.layout.pane_header_height_px);
        let popover = div().absolute().left(left).top(top).occlude().child(menu_panel(theme).children(rows));
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
        let style = MenuRowStyle {
            group: SharedString::from(format!("pane-menu-item-{index}")),
            is_highlighted: is_selected,
            is_destructive: item.is_destructive(),
        };
        let shortcut = item.shortcut.map(|shortcut| menu_shortcut(shortcut, theme));
        menu_row(("pane-menu-item", index), style, theme)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    cx.stop_propagation();
                    shell.run_pane_menu_item(index, window, cx);
                }),
            )
            .child(div().min_w(px(0.0)).truncate().child(item.title))
            .children(shortcut)
            .into_any_element()
    }
}
