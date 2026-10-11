#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::app::PaletteChoice;
use crate::command::{
    close_palette, open_choice_palette, open_palette, palette_backspace, palette_move, palette_selected,
    palette_selected_choice, palette_type, CommandId,
};
use crate::confirm::{close_pane_copy, close_tab_copy, delete_workspace_copy, quit_copy, ConfirmCopy, Confirmation};
use crate::menus::RunCommand;
use crate::motion::fade_in;
use pane_drag::DraggedPane;
use crate::input::{dropped_paths_text, encode_keystroke, is_composed_text, page_scroll_lines, paste_bytes};
use crate::palette as palette_render;
use crate::terminal_view::{terminal_canvas, PaneFocus, TerminalPane, TerminalStyle};
use crate::workspace_menu::MenuField;
use crate::workspaces::{restore_notice, WorkspaceModel};
use domain::{Axis, LayoutNode, Panel, PanelId, TabId, TerminalSessionId, WorkspaceId};
use gpui::{
    div, point, prelude::*, px, AnyElement, ClipboardItem, Context, ElementInputHandler, ExternalPaths,
    DragMoveEvent, FocusHandle,
    IntoElement, KeyDownEvent, Keystroke, MouseButton, ParentElement, Render, SharedString, Styled, Subscription, Task,
    TitlebarOptions, Window, WindowOptions,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use terminal_core::broadcast::{error::RecvError, Receiver};
use terminal_core::{GridSize, InMemoryTerminalService, ShellCommand, TerminalEvent, TerminalHandle};
use workspace_service::{PanelSnapshot, RestoredState, WorkspaceStore};

mod dialogs;
mod pane_header;
mod pane_drag;
mod splits;
mod tab_strip;
mod text_input;

use tab_strip::TabField;

const DEFAULT_SHELL: &str = "/bin/zsh";
const LOGIN_FLAG: &str = "-l";
const INITIAL_GRID: GridSize = GridSize { cols: 80, rows: 24 };
const ESCAPE_KEY: &str = "escape";
const ENTER_KEY: &str = "enter";
const BACKSPACE_KEY: &str = "backspace";
const UP_KEY: &str = "up";
const DOWN_KEY: &str = "down";
const CWD_TAIL_SEGMENTS: usize = 2;
const HOME_MARK: &str = "~";
const ABOUT_TITLE: &str = "Onyxterm";
const ABOUT_OK: &str = "OK";
const ELLIPSIS: &str = "\u{2026}";

struct Pane {
    session: TerminalSessionId,
    terminal: TerminalPane,
    title: String,
    _events: Task<()>,
}

#[derive(Default)]
struct EventBatch {
    title: Option<String>,
    exited: bool,
}

impl EventBatch {
    fn absorb(&mut self, event: TerminalEvent) {
        match event {
            TerminalEvent::TitleChanged(title) => self.title = Some(title),
            TerminalEvent::Exited { .. } => self.exited = true,
            TerminalEvent::Output(_) | TerminalEvent::Resized { .. } => {}
        }
    }
}

pub struct Startup {
    pub terminals: Arc<InMemoryTerminalService>,
    pub store: WorkspaceStore,
    pub restored: RestoredState,
    pub home: PathBuf,
}

pub struct Shell {
    state: AppState,
    terminals: Arc<InMemoryTerminalService>,
    store: WorkspaceStore,
    style: TerminalStyle,
    focus_handle: FocusHandle,
    model: WorkspaceModel,
    panes: HashMap<PanelId, Pane>,
    pending: Option<Confirmation>,
    is_workspace_menu_open: bool,
    menu_field: Option<MenuField>,
    tab_field: Option<TabField>,
    notice: Option<String>,
    marked_text: Option<String>,
    split_drag: Option<splits::SplitDrag>,
    drop_target: Option<(PanelId, domain::DropZone)>,
    split_bounds: splits::SplitBounds,
    _quit_hook: Subscription,
}

impl Shell {
    pub fn new(state: AppState, startup: Startup, cx: &mut Context<Self>) -> Self {
        let font_family = state.theme.font.resolve_family(&cx.text_system().all_font_names());
        tracing::info!(%font_family, "terminal font resolved");
        let Startup { terminals, store, restored, home } = startup;
        let mut shell = Self {
            style: TerminalStyle::from_theme(&state.theme, font_family),
            state,
            terminals,
            store,
            focus_handle: cx.focus_handle(),
            model: WorkspaceModel::from_snapshot(restored.snapshot, &home),
            panes: HashMap::new(),
            pending: None,
            is_workspace_menu_open: false,
            menu_field: None,
            tab_field: None,
            notice: restore_notice(&restored.warnings),
            marked_text: None,
            split_drag: None,
            drop_target: None,
            split_bounds: splits::SplitBounds::default(),
            _quit_hook: cx.on_app_quit(|shell, _cx| {
                shell.flush();
                async {}
            }),
        };
        shell.show_active_workspace(cx);
        shell
    }

    fn persist(&mut self) {
        self.sync_live_cwds();
        self.store.save(self.model.snapshot());
    }

    fn flush(&mut self) {
        self.persist();
        if let Err(err) = self.store.flush() {
            tracing::error!(%err, "failed to flush workspace snapshot");
        }
    }

    fn sync_live_cwds(&mut self) {
        for (panel, pane) in &self.panes {
            let Some(cwd) = pane.terminal.handle.current_dir() else {
                continue;
            };
            if let Some(record) = self.model.panel_mut(*panel) {
                record.cwd = cwd;
            }
        }
    }

    fn show_active_workspace(&mut self, cx: &mut Context<Self>) {
        if self.model.tabs().is_empty() {
            self.open_tab(cx);
        }
        for panel in self.model.panels_to_spawn(|panel| self.panes.contains_key(&panel)) {
            self.spawn_restored(panel, cx);
        }
    }

    fn spawn_restored(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(record) = self.model.panel(panel).cloned() else {
            return;
        };
        let command = login_shell_command(record.cwd.clone());
        if let Some(session) = self.spawn_session(panel, command, record.panel.title.clone(), cx) {
            self.set_panel_session(panel, session);
        }
    }

    fn set_panel_session(&mut self, panel: PanelId, session: TerminalSessionId) {
        if let Some(record) = self.model.panel_mut(panel) {
            record.panel.resource = domain::ResourceRef::Terminal(session);
        }
    }

    fn resolve_combo(&self, event: &KeyDownEvent) -> Option<CommandId> {
        self.state.keymap.resolve_keystroke(&event.keystroke)
    }

    fn run_command(&mut self, id: CommandId, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            CommandId::NewTab => self.open_tab(cx),
            CommandId::NextTab => self.model.tabs_mut().select_next(),
            CommandId::PrevTab => self.model.tabs_mut().select_previous(),
            CommandId::SplitRight => self.split_focused(Axis::Horizontal, cx),
            CommandId::SplitDown => self.split_focused(Axis::Vertical, cx),
            CommandId::FocusAdjacent => self.focus_next_pane(),
            CommandId::MaximizePane => self.toggle_maximized(),
            CommandId::CloseActive => self.request_close_focused(cx),
            CommandId::Copy => self.copy_selection(cx),
            CommandId::Paste => self.paste_clipboard(cx),
            CommandId::SwitchWorkspace => self.open_workspace_picker(),
            CommandId::NewWorkspace => self.start_new_workspace(cx),
            CommandId::RenameTab => self.start_rename_tab(self.model.tabs().active_index()),
            CommandId::Quit => self.request_quit(cx),
            CommandId::ToggleFullScreen => window.toggle_fullscreen(),
            CommandId::Minimize => window.minimize_window(),
            CommandId::Zoom => window.zoom_window(),
            CommandId::Hide => cx.hide(),
            CommandId::HideOthers => cx.hide_other_apps(),
            CommandId::ShowAll => cx.unhide_other_apps(),
            CommandId::About => show_about(window, cx),
            CommandId::CloseTab => self.request_close_tab(cx),
            CommandId::SelectAll => self.select_all(),
            _ => crate::command::CommandRegistry::dispatch(&mut self.state, id),
        }
        if id.is_structural() {
            self.persist();
        }
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.consume_key(event, window, cx) {
            cx.stop_propagation();
        }
    }

    fn consume_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        if keystroke.key == ESCAPE_KEY && cx.has_active_drag() {
            return self.cancel_pane_drag(window, cx);
        }
        if self.pending.is_some() {
            self.handle_confirmation_key(keystroke, cx);
            return true;
        }
        if self.is_workspace_menu_open {
            self.handle_menu_key(keystroke, cx);
            return true;
        }
        if self.tab_field.is_some() {
            self.handle_tab_field_key(keystroke, cx);
            return true;
        }
        if let Some(id) = self.resolve_combo(event) {
            self.run_command(id, window, cx);
            return true;
        }
        if self.state.palette_visible {
            self.handle_palette_key(keystroke, window, cx);
            return true;
        }
        self.send_key_to_terminal(event, cx)
    }

    fn send_key_to_terminal(&self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let Some(handle) = self.focused_handle() else {
            return false;
        };
        if is_composed_text(keystroke, self.state.theme.input.option_as_meta) {
            return event.is_held && self.repeat_text(handle, keystroke);
        }
        if let Some(lines) = page_scroll_lines(keystroke, handle.grid_size().rows) {
            handle.terminal().scroll_lines(lines);
            cx.notify();
            return true;
        }
        let Some(bytes) = encode_keystroke(keystroke, handle.input_modes()) else {
            return false;
        };
        write_to(handle, &bytes);
        cx.notify();
        true
    }

    fn repeat_text(&self, handle: &TerminalHandle, keystroke: &Keystroke) -> bool {
        let Some(text) = keystroke.key_char.as_deref().filter(|text| !text.is_empty()) else {
            return false;
        };
        write_to(handle, text.as_bytes());
        true
    }

    fn drop_paths(&mut self, panel: PanelId, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_pane(panel);
        window.focus(&self.focus_handle);
        if let Some(pane) = self.panes.get(&panel) {
            write_to(&pane.terminal.handle, dropped_paths_text(paths.paths()).as_bytes());
        }
        cx.notify();
    }

    fn handle_palette_key(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = keystroke.modifiers;
        match keystroke.key.as_str() {
            ESCAPE_KEY => close_palette(&mut self.state),
            ENTER_KEY => self.run_palette_selection(window, cx),
            BACKSPACE_KEY => palette_backspace(&mut self.state),
            UP_KEY => palette_move(&mut self.state, -1),
            DOWN_KEY => palette_move(&mut self.state, 1),
            _ if modifiers.control || modifiers.platform => {}
            _ => {
                if let Some(text) = keystroke.key_char.as_deref() {
                    palette_type(&mut self.state, text);
                }
            }
        }
        cx.notify();
    }

    fn run_palette_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = palette_selected(&self.state);
        let choice = palette_selected_choice(&self.state);
        close_palette(&mut self.state);
        if let Some(key) = choice {
            self.select_workspace_by_key(&key, cx);
        }
        if let Some(id) = selected {
            self.run_command(id, window, cx);
        }
    }

    fn handle_confirmation_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.cancel_confirmation(cx),
            ENTER_KEY => self.accept_confirmation(cx),
            _ => {}
        }
    }

    fn handle_menu_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some(field) = self.menu_field.as_mut() else {
            if keystroke.key == ESCAPE_KEY {
                self.close_workspace_menu(cx);
            }
            return;
        };
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.menu_field = None,
            ENTER_KEY => self.commit_menu_field(cx),
            _ => edit_field(&mut field.text, keystroke),
        }
        cx.notify();
    }

    fn commit_menu_field(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.menu_field.take() else {
            return;
        };
        let name = field.text.trim().to_string();
        if name.is_empty() {
            return;
        }
        match field.renaming.and_then(|key| self.workspace_by_key(&key)) {
            Some(id) => {
                self.model.rename_workspace(id, name);
                self.persist();
            }
            None => {
                let id = self.model.create_workspace(name);
                self.select_workspace(id, cx);
            }
        }
    }

    fn open_workspace_picker(&mut self) {
        let choices = self
            .model
            .entries()
            .iter()
            .map(|entry| PaletteChoice { key: entry.workspace.id.to_string(), title: entry.workspace.name.clone() })
            .collect();
        open_choice_palette(&mut self.state, choices);
    }

    fn start_new_workspace(&mut self, cx: &mut Context<Self>) {
        self.is_workspace_menu_open = true;
        self.menu_field = Some(MenuField { renaming: None, text: SharedString::default() });
        cx.notify();
    }

    fn start_rename_workspace(&mut self, key: &SharedString, cx: &mut Context<Self>) {
        let Some(name) = self.workspace_by_key(key).and_then(|id| self.model.workspace_name(id)) else {
            return;
        };
        self.menu_field = Some(MenuField { renaming: Some(key.clone()), text: name.to_string().into() });
        cx.notify();
    }

    fn request_delete_workspace(&mut self, key: &SharedString, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace_by_key(key) {
            self.pending = Some(Confirmation::DeleteWorkspace(id));
            self.close_workspace_menu(cx);
        }
    }

    fn delete_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        for panel in self.model.workspace_panels(id) {
            self.discard_pane(panel, cx);
        }
        self.model.delete_workspace(id);
        self.show_active_workspace(cx);
        self.persist();
    }

    fn workspace_by_key(&self, key: &str) -> Option<WorkspaceId> {
        self.model.entries().iter().map(|entry| entry.workspace.id).find(|id| id.to_string() == key)
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let Some(text) = self.focused_handle().and_then(|handle| handle.terminal().selection_text()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn paste_clipboard(&self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some(handle) = self.focused_handle() else {
            return;
        };
        write_to(handle, &paste_bytes(&text, handle.input_modes()));
    }

    fn select_all(&self) {
        if let Some(handle) = self.focused_handle() {
            handle.terminal().select_all();
        }
    }

    fn focused_handle(&self) -> Option<&TerminalHandle> {
        let panel = self.model.tabs().active()?.focused();
        self.panes.get(&panel).map(|pane| &pane.terminal.handle)
    }

    fn request_close_focused(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.model.tabs().active().map(|tab| tab.focused()) else {
            return;
        };
        let is_busy = self.panes.get(&panel).is_some_and(|pane| pane.terminal.handle.has_foreground_job());
        if is_busy {
            self.pending = Some(Confirmation::ClosePane(panel));
            return;
        }
        self.close_pane(panel, cx);
    }

    fn request_close_tab(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.model.tabs().active() else {
            return;
        };
        let id = tab.id();
        let is_busy = tab
            .layout()
            .leaves()
            .iter()
            .any(|panel| self.panes.get(panel).is_some_and(|pane| pane.terminal.handle.has_foreground_job()));
        if is_busy {
            self.pending = Some(Confirmation::CloseTab(id));
            return;
        }
        self.close_tab(id, cx);
    }

    fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(panels) = self.model.tabs().iter().find(|tab| tab.id() == id).map(|tab| tab.layout().leaves()) else {
            return;
        };
        for panel in panels {
            self.close_pane(panel, cx);
        }
    }

    fn cancel_confirmation(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        cx.notify();
    }

    fn accept_confirmation(&mut self, cx: &mut Context<Self>) {
        match self.pending.take() {
            Some(Confirmation::ClosePane(panel)) => self.close_pane(panel, cx),
            Some(Confirmation::CloseTab(id)) => self.close_tab(id, cx),
            Some(Confirmation::DeleteWorkspace(id)) => self.delete_workspace(id, cx),
            Some(Confirmation::Quit { .. }) => cx.quit(),
            None => {}
        }
        cx.notify();
    }

    fn confirmation_copy(&self, confirmation: Confirmation) -> ConfirmCopy {
        match confirmation {
            Confirmation::ClosePane(_) => close_pane_copy(),
            Confirmation::CloseTab(_) => close_tab_copy(),
            Confirmation::Quit { sessions } => quit_copy(sessions),
            Confirmation::DeleteWorkspace(id) => {
                let live = self.model.workspace_panels(id).iter().filter(|p| self.panes.contains_key(p)).count();
                delete_workspace_copy(self.model.workspace_name(id).unwrap_or_default(), live)
            }
        }
    }

    fn live_session_count(&self) -> usize {
        self.panes.len()
    }

    fn request_quit(&mut self, cx: &mut Context<Self>) {
        match self.live_session_count() {
            0 => cx.quit(),
            sessions => self.pending = Some(Confirmation::Quit { sessions }),
        }
    }

    fn toggle_workspace_menu(&mut self, cx: &mut Context<Self>) {
        self.is_workspace_menu_open = !self.is_workspace_menu_open;
        self.menu_field = None;
        cx.notify();
    }

    fn close_workspace_menu(&mut self, cx: &mut Context<Self>) {
        self.is_workspace_menu_open = false;
        self.menu_field = None;
        cx.notify();
    }

    fn select_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.tab_field = None;
        self.model.switch_to(id);
        self.show_active_workspace(cx);
        self.persist();
        self.close_workspace_menu(cx);
    }

    fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        cx.notify();
    }

    fn open_tab(&mut self, cx: &mut Context<Self>) {
        let cwd = self.model.active_root();
        if let Some(panel) = self.spawn_shell_panel(cwd, cx) {
            self.model.tabs_mut().open(panel);
        }
    }

    fn split_focused(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let Some(focused) = self.model.tabs().active().map(|tab| tab.focused()) else {
            return;
        };
        let cwd = self.panel_cwd(focused);
        let Some(panel) = self.spawn_shell_panel(cwd, cx) else {
            return;
        };
        let Some(tab) = self.model.tabs_mut().active_mut() else {
            return;
        };
        if let Err(err) = tab.split_focused(axis, panel) {
            tracing::warn!(?err, "split failed");
            self.discard_pane(panel, cx);
        }
    }

    fn panel_cwd(&self, panel: PanelId) -> PathBuf {
        self.panes
            .get(&panel)
            .and_then(|pane| pane.terminal.handle.current_dir())
            .or_else(|| self.model.panel(panel).map(|record| record.cwd.clone()))
            .unwrap_or_else(|| self.model.active_root())
    }

    fn focus_next_pane(&mut self) {
        if let Some(tab) = self.model.tabs_mut().active_mut() {
            tab.focus_next();
        }
    }

    fn toggle_maximized(&mut self) {
        if let Some(tab) = self.model.tabs_mut().active_mut() {
            tab.toggle_maximized();
        }
    }

    fn focus_pane(&mut self, panel: PanelId) {
        if let Some(tab) = self.model.tabs_mut().active_mut() {
            tab.focus(panel);
        }
        self.persist();
    }

    fn close_pane(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        if self.pending == Some(Confirmation::ClosePane(panel)) {
            self.pending = None;
        }
        self.tab_field = None;
        if let Err(err) = self.model.remove_panel(panel) {
            tracing::warn!(?err, "close pane failed");
        }
        self.discard_pane(panel, cx);
        self.show_active_workspace(cx);
        self.persist();
    }

    fn discard_pane(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.remove(&panel) else {
            return;
        };
        let terminals = self.terminals.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = terminals.terminate_session(pane.session) {
                    tracing::debug!(?err, "terminate session");
                }
            })
            .detach();
    }

    fn spawn_shell_panel(&mut self, cwd: PathBuf, cx: &mut Context<Self>) -> Option<PanelId> {
        let command = login_shell_command(cwd.clone());
        let title = shell_title(&command);
        let panel = PanelId::new();
        let session = self.spawn_session(panel, command, title.clone(), cx)?;
        self.model.add_panel(PanelSnapshot { panel: Panel::new_terminal(panel, title, session), cwd, name: None });
        Some(panel)
    }

    fn spawn_session(
        &mut self,
        panel: PanelId,
        command: ShellCommand,
        title: String,
        cx: &mut Context<Self>,
    ) -> Option<TerminalSessionId> {
        let session = self
            .terminals
            .spawn_command(command, INITIAL_GRID)
            .inspect_err(|err| tracing::error!(?err, "failed to spawn session"))
            .ok()?;
        let handle = self.terminals.handle_for(session)?;
        let events = watch_session(panel, handle.subscribe(), cx);
        self.panes.insert(panel, Pane { session, terminal: TerminalPane::new(handle), title, _events: events });
        Some(session)
    }

    fn apply_batch(&mut self, panel: PanelId, batch: EventBatch, cx: &mut Context<Self>) {
        if let Some(title) = batch.title {
            if let Some(pane) = self.panes.get_mut(&panel) {
                pane.title = title.clone();
            }
            if let Some(record) = self.model.panel_mut(panel) {
                record.panel.title = title;
            }
        }
        if batch.exited {
            self.close_pane(panel, cx);
        }
        cx.notify();
    }

    fn select_workspace_by_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace_by_key(key) {
            self.select_workspace(id, cx);
        }
    }

    fn pane_focus(&self, cx: &mut Context<Self>) -> PaneFocus {
        let focus_handle = self.focus_handle.clone();
        let shell = cx.entity();
        PaneFocus {
            marked_text: self.marked_text.clone(),
            register_input: Box::new(move |bounds, window, cx| {
                window.handle_input(&focus_handle, ElementInputHandler::new(bounds, shell), cx)
            }),
        }
    }

    fn render_pane(&self, panel: PanelId, cx: &mut Context<Self>) -> AnyElement {
        let Some(pane) = self.panes.get(&panel) else {
            return div().into_any_element();
        };
        let is_focused = self.model.tabs().active().is_some_and(|tab| tab.focused() == panel);
        let header = self.render_pane_header(panel, is_focused, cx);
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(self.state.theme.layout.island_radius_px))
            .border_1()
            .border_color(self.pane_border(is_focused))
            .bg(self.state.theme.canvas())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, window, cx| {
                    shell.focus_pane(panel);
                    window.focus(&shell.focus_handle);
                    cx.notify();
                }),
            )
            .on_drop(cx.listener(move |shell, paths: &ExternalPaths, window, cx| shell.drop_paths(panel, paths, window, cx)))
            .on_drag_move(cx.listener(move |shell, event: &DragMoveEvent<DraggedPane>, _window, cx| {
                shell.track_pane_drag(panel, event, cx)
            }))
            .on_drop(cx.listener(|shell, dragged: &DraggedPane, _window, cx| shell.drop_pane(dragged.panel, cx)))
            .child(terminal_canvas(pane.terminal.clone(), self.style.clone(), is_focused.then(|| self.pane_focus(cx))))
            .child(header)
            .children(self.render_drop_highlight(panel))
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let palette_overlay = palette_render::build_overlay(&self.state, theme);
        let palette_visible = self.state.palette_visible;
        let layout = self.model.tabs().active().map(|tab| match tab.is_maximized() {
            true => LayoutNode::leaf(tab.focused()),
            false => tab.layout().clone(),
        });
        let notice = self.notice.clone().map(|notice| self.render_notice(&notice, cx));
        let pane_area = layout.map(|layout| self.render_layout(&layout, Vec::new(), cx));
        let workspace_menu = self.is_workspace_menu_open.then(|| self.render_workspace_menu(cx));
        let close_prompt = self.pending.map(|pending| self.render_confirmation(self.confirmation_copy(pending), cx));
        let tab_strip = self.render_tab_strip(window.is_fullscreen(), cx);
        let theme = &self.state.theme;
        let margin = px(theme.layout.window_margin_px);
        let animation_ms = theme.layout.animation_ms;

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.surface.subtle)
            .text_color(theme.text_default())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|shell, event, window, cx| shell.handle_key(event, window, cx)))
            .on_action(cx.listener(|shell, action: &RunCommand, window, cx| shell.run_command(action.0, window, cx)))
            .on_mouse_move(cx.listener(|shell, event: &gpui::MouseMoveEvent, _window, cx| shell.drag_split(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|shell, _event, _window, cx| {
                shell.end_split_drag(cx);
                shell.clear_drop_target(cx);
            }))
            .when_some(self.split_drag_cursor(), |el, cursor| el.cursor(cursor))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.commit_tab_field_on_click(cx)))
            .child(tab_strip)
            .children(notice)
            .child(div().flex_1().min_h(px(0.0)).w_full().px(margin).pb(margin).children(pane_area))
            .when(palette_visible, |el| el.child(fade_in(palette_overlay, "palette", animation_ms)))
            .children(workspace_menu.map(|menu| fade_in(menu, "workspace-menu", animation_ms)))
            .children(close_prompt.map(|prompt| fade_in(prompt, "confirmation", animation_ms)))
    }
}

fn show_about(window: &mut Window, cx: &mut gpui::App) {
    let detail = format!("Version {}", crate::version());
    let _dismissed = window.prompt(gpui::PromptLevel::Info, ABOUT_TITLE, Some(&detail), &[ABOUT_OK], cx);
}

fn watch_session(panel: PanelId, mut events: Receiver<TerminalEvent>, cx: &mut Context<Shell>) -> Task<()> {
    cx.spawn(async move |shell, cx| loop {
        let batch = next_batch(&mut events).await;
        let exited = batch.exited;
        let updated = shell.update(cx, |shell, cx| shell.apply_batch(panel, batch, cx));
        if exited || updated.is_err() {
            break;
        }
    })
}

async fn next_batch(events: &mut Receiver<TerminalEvent>) -> EventBatch {
    let mut batch = EventBatch::default();
    match events.recv().await {
        Ok(event) => batch.absorb(event),
        Err(RecvError::Lagged(_)) => {}
        Err(RecvError::Closed) => batch.exited = true,
    }
    while let Ok(event) = events.try_recv() {
        batch.absorb(event);
    }
    batch
}

pub fn short_cwd(path: &Path, home: &Path) -> String {
    let (prefix, relative) = match path.strip_prefix(home) {
        Ok(relative) => (HOME_MARK.to_string(), relative),
        Err(_) => (String::new(), path.strip_prefix("/").unwrap_or(path)),
    };
    let segments: Vec<String> =
        relative.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let tail_start = segments.len().saturating_sub(CWD_TAIL_SEGMENTS);
    let shown = segments[tail_start..].join("/");
    match (prefix.is_empty(), tail_start > 0, shown.is_empty()) {
        (false, _, true) => prefix,
        (false, false, false) => format!("{prefix}/{shown}"),
        (false, true, false) => format!("{prefix}/{ELLIPSIS}/{shown}"),
        (true, false, _) => format!("/{shown}"),
        (true, true, _) => format!("/{ELLIPSIS}/{shown}"),
    }
}

fn edit_field(text: &mut SharedString, keystroke: &Keystroke) {
    let modifiers = keystroke.modifiers;
    if keystroke.key == BACKSPACE_KEY {
        let mut edited = text.to_string();
        edited.pop();
        *text = edited.into();
        return;
    }
    if modifiers.control || modifiers.platform {
        return;
    }
    if let Some(typed) = keystroke.key_char.as_deref() {
        *text = format!("{text}{typed}").into();
    }
}

fn write_to(handle: &TerminalHandle, bytes: &[u8]) {
    handle.terminal().scroll_to_bottom();
    if let Err(err) = handle.write(bytes) {
        tracing::warn!(?err, "pty write failed");
    }
}

fn login_shell_command(cwd: PathBuf) -> ShellCommand {
    let program = std::env::var_os("SHELL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_SHELL));
    let cwd = if Path::new(&cwd).is_dir() { cwd } else { PathBuf::from("/") };
    ShellCommand { program, args: vec![LOGIN_FLAG.to_string()], cwd }
}

fn shell_title(command: &ShellCommand) -> String {
    command
        .program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn open_shell_window(
    options: WindowOptions,
    startup: Startup,
    cx: &mut gpui::App,
) -> gpui::Result<gpui::WindowHandle<Shell>> {
    let state = AppState::new();
    let layout = state.theme.layout;
    let themed_options = WindowOptions {
        window_background: state.theme.window_background(),
        titlebar: Some(TitlebarOptions {
            title: options.titlebar.and_then(|titlebar| titlebar.title),
            appears_transparent: true,
            traffic_light_position: Some(point(px(layout.traffic_light_left_px), px(layout.traffic_light_top_px))),
        }),
        ..options
    };
    cx.open_window(themed_options, |window, cx| {
        let shell = cx.new(|cx| {
            cx.observe_window_bounds(window, |_shell, _window, cx| cx.notify()).detach();
            Shell::new(state, startup, cx)
        });
        window.focus(&shell.read(cx).focus_handle);
        let guarded = shell.downgrade();
        window.on_window_should_close(cx, move |_window, cx| {
            let Some(shell) = guarded.upgrade() else {
                return true;
            };
            let is_idle = shell.read(cx).live_session_count() == 0;
            if !is_idle {
                shell.update(cx, |shell, cx| {
                    shell.request_quit(cx);
                    cx.notify();
                });
            }
            is_idle
        });
        shell
    })
}

pub fn palette_visible(state: &AppState) -> bool {
    state.palette_visible
}

pub fn open_or_toggle_palette(state: &mut AppState) {
    if !state.palette_visible {
        open_palette(state);
    } else {
        state.palette_visible = false;
    }
}
