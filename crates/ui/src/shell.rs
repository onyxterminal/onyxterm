#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::{open_palette, CommandId};
use crate::input::{encode_keystroke, paste_bytes};
use crate::keymap::Combo;
use crate::palette as palette_render;
use crate::tabs::Tabs;
use crate::terminal_view::{terminal_canvas, TerminalStyle};
use domain::{Axis, LayoutNode, PanelId, TerminalSessionId};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, FocusHandle, Hsla, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Render, Styled, Task, Window, WindowOptions,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use terminal_core::broadcast::{error::RecvError, Receiver};
use terminal_core::{GridSize, InMemoryTerminalService, ShellCommand, TerminalEvent, TerminalHandle};

const DEFAULT_SHELL: &str = "/bin/zsh";
const LOGIN_FLAG: &str = "-l";
const FALLBACK_CWD: &str = "/";
const INITIAL_GRID: GridSize = GridSize { cols: 80, rows: 24 };
const PASTE_KEY: &str = "v";
const ESCAPE_KEY: &str = "escape";

struct Pane {
    session: TerminalSessionId,
    handle: TerminalHandle,
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

pub struct Shell {
    state: AppState,
    terminals: Arc<InMemoryTerminalService>,
    style: TerminalStyle,
    focus_handle: FocusHandle,
    tabs: Tabs,
    panes: HashMap<PanelId, Pane>,
}

impl Shell {
    pub fn new(state: AppState, terminals: Arc<InMemoryTerminalService>, cx: &mut Context<Self>) -> Self {
        let mut shell = Self {
            style: TerminalStyle::from_theme(&state.theme),
            state,
            terminals,
            focus_handle: cx.focus_handle(),
            tabs: Tabs::default(),
            panes: HashMap::new(),
        };
        shell.open_tab(cx);
        shell
    }

    fn resolve_combo(&self, event: &KeyDownEvent) -> Option<CommandId> {
        let combo = Combo::new(event.keystroke.modifiers, event.keystroke.key.as_str());
        self.state.keymap.resolve(&combo)
    }

    fn run_command(&mut self, id: CommandId, cx: &mut Context<Self>) {
        match id {
            CommandId::NewTab => self.open_tab(cx),
            CommandId::NextTab => self.tabs.select_next(),
            CommandId::PrevTab => self.tabs.select_previous(),
            CommandId::SplitRight => self.split_focused(Axis::Horizontal, cx),
            CommandId::SplitDown => self.split_focused(Axis::Vertical, cx),
            CommandId::FocusAdjacent => self.focus_next_pane(),
            CommandId::CloseActive => self.close_focused_pane(cx),
            CommandId::Quit => cx.quit(),
            _ => crate::command::CommandRegistry::dispatch(&mut self.state, id),
        }
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if let Some(id) = self.resolve_combo(event) {
            self.run_command(id, cx);
            return;
        }
        let keystroke = &event.keystroke;
        if self.state.palette_visible {
            if keystroke.key == ESCAPE_KEY {
                self.state.palette_visible = false;
                cx.notify();
            }
            return;
        }
        if keystroke.modifiers.platform && keystroke.key == PASTE_KEY {
            self.paste_clipboard(cx);
            return;
        }
        let Some(handle) = self.focused_handle() else {
            return;
        };
        if let Some(bytes) = encode_keystroke(keystroke, handle.input_modes()) {
            write_to(handle, &bytes);
        }
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
        let panel = self.tabs.active()?.focused();
        self.panes.get(&panel).map(|pane| &pane.handle)
    }

    fn open_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.spawn_pane(cx) {
            self.tabs.open(panel);
        }
    }

    fn split_focused(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(panel) = self.spawn_pane(cx) else {
            return;
        };
        let Some(tab) = self.tabs.active_mut() else {
            return;
        };
        if let Err(err) = tab.split_focused(axis, panel) {
            tracing::warn!(?err, "split failed");
            self.discard_pane(panel, cx);
        }
    }

    fn focus_next_pane(&mut self) {
        if let Some(tab) = self.tabs.active_mut() {
            tab.focus_next();
        }
    }

    fn focus_pane(&mut self, panel: PanelId) {
        if let Some(tab) = self.tabs.active_mut() {
            tab.focus(panel);
        }
    }

    fn close_focused_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.tabs.active().map(|tab| tab.focused()) {
            self.close_pane(panel, cx);
        }
    }

    fn close_pane(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        if let Err(err) = self.tabs.close_panel(panel) {
            tracing::warn!(?err, "close pane failed");
        }
        self.discard_pane(panel, cx);
        if self.tabs.is_empty() {
            cx.quit();
        }
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

    fn spawn_pane(&mut self, cx: &mut Context<Self>) -> Option<PanelId> {
        let command = login_shell_command();
        let title = shell_title(&command);
        let session = self
            .terminals
            .spawn_command(command, INITIAL_GRID)
            .inspect_err(|err| tracing::error!(?err, "failed to spawn shell"))
            .ok()?;
        let handle = self.terminals.handle_for(session)?;
        let panel = PanelId::new();
        let events = watch_session(panel, handle.subscribe(), cx);
        self.panes.insert(panel, Pane { session, handle, title, _events: events });
        Some(panel)
    }

    fn apply_batch(&mut self, panel: PanelId, batch: EventBatch, cx: &mut Context<Self>) {
        if let Some(title) = batch.title {
            if let Some(pane) = self.panes.get_mut(&panel) {
                pane.title = title;
            }
        }
        if batch.exited {
            self.close_pane(panel, cx);
        }
        cx.notify();
    }

    fn render_tab_strip(&self) -> impl IntoElement {
        let theme = &self.state.theme;
        let active = self.tabs.active_index();
        let labels: Vec<(String, bool)> = self
            .tabs
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
            .children(labels.into_iter().map(|(label, is_active)| {
                let (bg, fg) = if is_active {
                    (theme.canvas(), theme.text_default())
                } else {
                    (theme.surface.subtle, theme.text.muted)
                };
                div()
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .bg(bg)
                    .text_color(fg)
                    .border_r_1()
                    .border_color(theme.border_default())
                    .child(label)
            }))
    }

    fn render_rail(&self) -> impl IntoElement {
        let theme = &self.state.theme;
        div()
            .w(px(theme.layout.workspace_rail_width_px))
            .flex_none()
            .h_full()
            .bg(theme.surface.subtle)
            .border_r_1()
            .border_color(theme.border_default())
            .flex_col()
            .py_2()
            .px_2()
            .child(div().text_color(theme.text.muted).py_1().child("Workspaces"))
            .children(
                self.state
                    .workspaces
                    .iter()
                    .map(|ws| div().py_1().text_color(theme.text.muted).child(ws.name.clone())),
            )
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
        let is_focused = self.tabs.active().is_some_and(|tab| tab.focused() == panel);
        div()
            .size_full()
            .p_1()
            .border_1()
            .border_color(self.pane_border(is_focused))
            .bg(self.state.theme.canvas())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event, _window, cx| {
                    shell.focus_pane(panel);
                    cx.notify();
                }),
            )
            .child(terminal_canvas(pane.handle.clone(), self.style, is_focused))
            .into_any_element()
    }

    fn pane_border(&self, is_focused: bool) -> Hsla {
        let theme = &self.state.theme;
        let is_split = self.tabs.active().is_some_and(|tab| !matches!(tab.layout(), LayoutNode::Leaf { .. }));
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
        let layout = self.tabs.active().map(|tab| tab.layout().clone());
        let pane_area = layout.map(|layout| self.render_layout(&layout, cx));

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.canvas())
            .text_color(theme.text_default())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|shell, event, _window, cx| shell.handle_key(event, cx)))
            .child(self.render_tab_strip())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_rail())
                    .child(div().flex_1().h_full().min_w(px(0.0)).children(pane_area)),
            )
            .when(palette_visible, |el| el.child(palette_overlay))
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
    if let Err(err) = handle.write(bytes) {
        tracing::warn!(?err, "pty write failed");
    }
}

fn login_shell_command() -> ShellCommand {
    let program = std::env::var_os("SHELL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_SHELL));
    let cwd = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(FALLBACK_CWD));
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
    terminals: Arc<InMemoryTerminalService>,
    cx: &mut gpui::App,
) -> gpui::Result<gpui::WindowHandle<Shell>> {
    let state = AppState::new();
    let themed_options = WindowOptions {
        window_background: state.theme.window_background(),
        ..options
    };
    cx.open_window(themed_options, |window, cx| {
        let shell = cx.new(|cx| Shell::new(state, terminals, cx));
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
