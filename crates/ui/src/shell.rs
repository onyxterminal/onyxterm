#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::{
    close_palette, open_palette, palette_backspace, palette_move, palette_selected, palette_type, CommandId,
};
use crate::input::{encode_keystroke, page_scroll_lines, paste_bytes};
use crate::keymap::Combo;
use crate::palette as palette_render;
use crate::terminal_view::{terminal_canvas, TerminalPane, TerminalStyle};
use crate::workspace_menu::{workspace_menu, WorkspaceMenuItem};
use crate::workspaces::{restore_notice, WorkspaceModel};
use domain::{Axis, LayoutNode, Panel, PanelId, TerminalSessionId, WorkspaceId};
use gpui::{
    div, img, prelude::*, px, relative, AnyElement, ClipboardItem, Context, FocusHandle, Hsla, Image, ImageFormat,
    IntoElement, KeyDownEvent, Keystroke, MouseButton, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window, WindowOptions,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use terminal_core::broadcast::{error::RecvError, Receiver};
use terminal_core::{GridSize, InMemoryTerminalService, ShellCommand, TerminalEvent, TerminalHandle};
use workspace_service::{PanelSnapshot, RestoredState, WorkspaceStore};

const DEFAULT_SHELL: &str = "/bin/zsh";
const LOGIN_FLAG: &str = "-l";
const INITIAL_GRID: GridSize = GridSize { cols: 80, rows: 24 };
const ESCAPE_KEY: &str = "escape";
const ENTER_KEY: &str = "enter";
const BACKSPACE_KEY: &str = "backspace";
const UP_KEY: &str = "up";
const DOWN_KEY: &str = "down";
const LOGO_SIZE_PX: f32 = 18.0;
const CLOSE_PROMPT_WIDTH_PX: f32 = 360.0;
const BACKDROP_ALPHA: f32 = 0.35;
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo.png");

static LOGO: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())));

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
    pending_close: Option<PanelId>,
    is_workspace_menu_open: bool,
    notice: Option<String>,
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
            pending_close: None,
            is_workspace_menu_open: false,
            notice: restore_notice(&restored.warnings),
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
        let combo = Combo::new(event.keystroke.modifiers, event.keystroke.key.as_str());
        self.state.keymap.resolve(&combo)
    }

    fn run_command(&mut self, id: CommandId, cx: &mut Context<Self>) {
        match id {
            CommandId::NewTab => self.open_tab(cx),
            CommandId::NextTab => self.model.tabs_mut().select_next(),
            CommandId::PrevTab => self.model.tabs_mut().select_previous(),
            CommandId::SplitRight => self.split_focused(Axis::Horizontal, cx),
            CommandId::SplitDown => self.split_focused(Axis::Vertical, cx),
            CommandId::FocusAdjacent => self.focus_next_pane(),
            CommandId::CloseActive => self.request_close_focused(cx),
            CommandId::Copy => self.copy_selection(cx),
            CommandId::Paste => self.paste_clipboard(cx),
            CommandId::Quit => cx.quit(),
            _ => crate::command::CommandRegistry::dispatch(&mut self.state, id),
        }
        if id.is_structural() {
            self.persist();
        }
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if self.pending_close.is_some() {
            self.handle_close_prompt_key(keystroke, cx);
            return;
        }
        if self.is_workspace_menu_open && keystroke.key == ESCAPE_KEY {
            self.close_workspace_menu(cx);
            return;
        }
        if let Some(id) = self.resolve_combo(event) {
            self.run_command(id, cx);
            return;
        }
        if self.state.palette_visible {
            self.handle_palette_key(keystroke, cx);
            return;
        }
        self.send_key_to_terminal(keystroke, cx);
    }

    fn send_key_to_terminal(&self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some(handle) = self.focused_handle() else {
            return;
        };
        if let Some(lines) = page_scroll_lines(keystroke, handle.grid_size().rows) {
            handle.terminal().scroll_lines(lines);
            cx.notify();
            return;
        }
        if let Some(bytes) = encode_keystroke(keystroke, handle.input_modes()) {
            write_to(handle, &bytes);
            cx.notify();
        }
    }

    fn handle_palette_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let modifiers = keystroke.modifiers;
        match keystroke.key.as_str() {
            ESCAPE_KEY => close_palette(&mut self.state),
            ENTER_KEY => self.run_palette_selection(cx),
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

    fn run_palette_selection(&mut self, cx: &mut Context<Self>) {
        let selected = palette_selected(&self.state);
        close_palette(&mut self.state);
        if let Some(id) = selected {
            self.run_command(id, cx);
        }
    }

    fn handle_close_prompt_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.cancel_close(cx),
            ENTER_KEY => self.confirm_close(cx),
            _ => {}
        }
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
            self.pending_close = Some(panel);
            return;
        }
        self.close_pane(panel, cx);
    }

    fn cancel_close(&mut self, cx: &mut Context<Self>) {
        self.pending_close = None;
        cx.notify();
    }

    fn confirm_close(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.pending_close.take() {
            self.close_pane(panel, cx);
        }
        cx.notify();
    }

    fn toggle_workspace_menu(&mut self, cx: &mut Context<Self>) {
        self.is_workspace_menu_open = !self.is_workspace_menu_open;
        cx.notify();
    }

    fn close_workspace_menu(&mut self, cx: &mut Context<Self>) {
        self.is_workspace_menu_open = false;
        cx.notify();
    }

    fn select_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
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

    fn focus_pane(&mut self, panel: PanelId) {
        if let Some(tab) = self.model.tabs_mut().active_mut() {
            tab.focus(panel);
        }
        self.persist();
    }

    fn close_pane(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        if self.pending_close == Some(panel) {
            self.pending_close = None;
        }
        if let Err(err) = self.model.remove_panel(panel) {
            tracing::warn!(?err, "close pane failed");
        }
        self.discard_pane(panel, cx);
        if self.model.tabs().is_empty() {
            self.open_tab(cx);
        }
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
        self.model.add_panel(PanelSnapshot { panel: Panel::new_terminal(panel, title, session), cwd });
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

    fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let active = self.model.tabs().active_index();
        let labels: Vec<(String, bool)> = self
            .model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let title = self.panes.get(&tab.focused()).map(|pane| pane.title.as_str()).unwrap_or_default();
                (format!("{}  {}", index + 1, title), index == active)
            })
            .collect();
        div()
            .h(px(theme.layout.tab_strip_height_px))
            .flex_none()
            .bg(theme.surface.subtle)
            .border_b_1()
            .border_color(theme.border_default())
            .flex()
            .flex_row()
            .child(self.render_logo_button(cx))
            .children(labels.into_iter().enumerate().map(|(index, (label, is_active))| {
                let (bg, fg) = if is_active {
                    (theme.canvas(), theme.text_default())
                } else {
                    (theme.surface.subtle, theme.text.muted)
                };
                let hover_bg = theme.border.muted;
                div()
                    .id(("tab", index))
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .bg(bg)
                    .text_color(fg)
                    .border_r_1()
                    .border_color(theme.border_default())
                    .when(!is_active, |el| el.hover(move |style| style.bg(hover_bg)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |shell, _event, window, cx| shell.activate_tab(index, window, cx)),
                    )
                    .child(label)
            }))
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.model.tabs_mut().activate(index);
        self.persist();
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn render_logo_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let hover_bg = theme.border.muted;
        div()
            .id("workspace-menu-button")
            .h_full()
            .pl_3()
            .pr_2()
            .flex()
            .items_center()
            .cursor_pointer()
            .border_r_1()
            .border_color(theme.border_default())
            .hover(move |style| style.bg(hover_bg))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.toggle_workspace_menu(cx)))
            .child(img(LOGO.clone()).size(px(LOGO_SIZE_PX)))
    }

    fn render_workspace_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let active = self.model.active_id();
        let items = self
            .model
            .entries()
            .iter()
            .map(|entry| WorkspaceMenuItem {
                id: entry.workspace.id.to_string().into(),
                name: entry.workspace.name.clone().into(),
                is_active: entry.workspace.id == active,
            })
            .collect();
        let shell = cx.entity().downgrade();
        let on_select = Rc::new(move |id: &SharedString, _window: &mut Window, cx: &mut gpui::App| {
            let id = id.clone();
            if let Err(err) = shell.update(cx, |shell, cx| shell.select_workspace_by_key(&id, cx)) {
                tracing::debug!(?err, "workspace select after shell dropped");
            }
        });
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.close_workspace_menu(cx)))
            .child(
                div()
                    .absolute()
                    .top(px(theme.layout.tab_strip_height_px))
                    .left(px(theme.layout.gap_px))
                    .occlude()
                    .child(workspace_menu(items, theme, on_select)),
            )
    }

    fn select_workspace_by_key(&mut self, key: &str, cx: &mut Context<Self>) {
        let id = self.model.entries().iter().map(|entry| entry.workspace.id).find(|id| id.to_string() == key);
        if let Some(id) = id {
            self.select_workspace(id, cx);
        }
    }

    fn render_notice(&self, notice: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        div()
            .flex_none()
            .px_3()
            .py_1()
            .flex()
            .flex_row()
            .justify_between()
            .gap_3()
            .bg(theme.surface.subtle)
            .border_b_1()
            .border_color(theme.border_default())
            .text_color(theme.text.muted)
            .child(notice.to_string())
            .child(
                div()
                    .id("notice-dismiss")
                    .cursor_pointer()
                    .text_color(theme.accent.primary)
                    .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.dismiss_notice(cx)))
                    .child("Dismiss"),
            )
    }

    fn render_close_prompt(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let backdrop = Hsla { a: BACKDROP_ALPHA, ..gpui::black() };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(backdrop)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(CLOSE_PROMPT_WIDTH_PX))
                    .p_4()
                    .bg(theme.surface.raised)
                    .border_1()
                    .border_color(theme.border_default())
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child("Terminate running process?")
                    .child(
                        div()
                            .text_color(theme.text.muted)
                            .child("A command is still running in this pane. Closing it will end the process."),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_2()
                            .child(
                                self.prompt_button("close-cancel", "Cancel", (theme.surface.subtle, theme.text_default()))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|shell, _event, _window, cx| shell.cancel_close(cx)),
                                    ),
                            )
                            .child(
                                self.prompt_button(
                                    "close-confirm",
                                    "Close and terminate",
                                    (theme.status.failed, theme.text.inverse),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|shell, _event, _window, cx| shell.confirm_close(cx)),
                                ),
                            ),
                    ),
            )
    }

    fn prompt_button(&self, id: &'static str, label: &'static str, (bg, fg): (Hsla, Hsla)) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(bg)
            .text_color(fg)
            .border_1()
            .border_color(self.state.theme.border_default())
            .child(label)
    }

    fn render_layout(&self, node: &LayoutNode, cx: &mut Context<Self>) -> AnyElement {
        match node {
            LayoutNode::Leaf { panel_id } => self.render_pane(*panel_id, cx),
            LayoutNode::Split { axis, ratio, first, second } => {
                let first = self.render_layout(first, cx);
                let second = self.render_layout(second, cx);
                let container = div().size_full().flex().min_w(px(0.0)).min_h(px(0.0));
                match axis {
                    Axis::Horizontal => container
                        .flex_row()
                        .child(div().h_full().w(relative(*ratio)).flex_none().child(first))
                        .child(div().h_full().flex_1().min_w(px(0.0)).child(second)),
                    Axis::Vertical => container
                        .flex_col()
                        .child(div().w_full().h(relative(*ratio)).flex_none().child(first))
                        .child(div().w_full().flex_1().min_h(px(0.0)).child(second)),
                }
                .into_any_element()
            }
        }
    }

    fn render_pane(&self, panel: PanelId, cx: &mut Context<Self>) -> AnyElement {
        let Some(pane) = self.panes.get(&panel) else {
            return div().into_any_element();
        };
        let is_focused = self.model.tabs().active().is_some_and(|tab| tab.focused() == panel);
        div()
            .size_full()
            .p_1()
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
            .child(terminal_canvas(pane.terminal.clone(), self.style.clone(), is_focused))
            .into_any_element()
    }

    fn pane_border(&self, is_focused: bool) -> Hsla {
        let theme = &self.state.theme;
        let is_split = self.model.tabs().active().is_some_and(|tab| !matches!(tab.layout(), LayoutNode::Leaf { .. }));
        match (is_split, is_focused) {
            (false, _) => theme.canvas(),
            (true, true) => theme.border_focus(),
            (true, false) => theme.border_default(),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let palette_overlay = palette_render::build_overlay(&self.state, theme);
        let palette_visible = self.state.palette_visible;
        let layout = self.model.tabs().active().map(|tab| tab.layout().clone());
        let notice = self.notice.clone().map(|notice| self.render_notice(&notice, cx));
        let pane_area = layout.map(|layout| self.render_layout(&layout, cx));
        let workspace_menu = self.is_workspace_menu_open.then(|| self.render_workspace_menu(cx));
        let close_prompt = self.pending_close.map(|_| self.render_close_prompt(cx));
        let tab_strip = self.render_tab_strip(cx);
        let theme = &self.state.theme;

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.canvas())
            .text_color(theme.text_default())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|shell, event, _window, cx| shell.handle_key(event, cx)))
            .child(tab_strip)
            .children(notice)
            .child(div().flex_1().min_h(px(0.0)).w_full().children(pane_area))
            .when(palette_visible, |el| el.child(palette_overlay))
            .children(workspace_menu)
            .children(close_prompt)
    }
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
    let themed_options = WindowOptions {
        window_background: state.theme.window_background(),
        ..options
    };
    cx.open_window(themed_options, |window, cx| {
        let shell = cx.new(|cx| Shell::new(state, startup, cx));
        window.focus(&shell.read(cx).focus_handle);
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
