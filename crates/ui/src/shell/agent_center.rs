use super::{edit_field, Confirmation, Shell, ENTER_KEY, ESCAPE_KEY};
use crate::agents::{
    known_exit_code, mark_terminated, prepare_launch, state_label, status_color, AgentRow, LaunchRequest,
};
use crate::tabs::Tab;
use agent_core::CommandSpec;
use chrono::Utc;
use domain::{Agent, AgentId, AgentState, Panel, PanelId};
use gpui::{div, prelude::*, px, Context, Keystroke, MouseButton, SharedString, Window};
use std::path::PathBuf;
use std::time::Duration;
use terminal_core::ShellCommand;
use workspace_service::PanelSnapshot;

const CENTER_WIDTH_PX: f32 = 460.0;
const STATUS_DOT_PX: f32 = 8.0;
const TICK: Duration = Duration::from_secs(1);
const INTERRUPT: &[u8] = b"\x03";
const CARET: &str = "\u{258f}";

pub(super) struct AgentCenter {
    provider: String,
    cwd: SharedString,
    error: Option<String>,
}

impl Shell {
    pub(super) fn open_agent_center(&mut self, cx: &mut Context<Self>) {
        let provider = self.agent_registry.ids().into_iter().next().unwrap_or_default();
        let cwd = self.model.active_root().display().to_string().into();
        self.is_workspace_menu_open = false;
        self.agent_center = Some(AgentCenter { provider, cwd, error: None });
        self._agent_tick = Some(cx.spawn(async move |shell, cx| loop {
            cx.background_executor().timer(TICK).await;
            let is_open = shell.update(cx, |shell, cx| {
                cx.notify();
                shell.agent_center.is_some()
            });
            if !matches!(is_open, Ok(true)) {
                break;
            }
        }));
        cx.notify();
    }

    fn close_agent_center(&mut self, cx: &mut Context<Self>) {
        self.agent_center = None;
        self._agent_tick = None;
        cx.notify();
    }

    pub(super) fn handle_agent_center_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.close_agent_center(cx),
            ENTER_KEY => self.launch_agent(cx),
            _ => {
                if let Some(center) = self.agent_center.as_mut() {
                    edit_field(&mut center.cwd, keystroke);
                    center.error = None;
                }
            }
        }
        cx.notify();
    }

    fn choose_provider(&mut self, provider: String, cx: &mut Context<Self>) {
        if let Some(center) = self.agent_center.as_mut() {
            center.provider = provider;
            center.error = None;
        }
        cx.notify();
    }

    fn launch_agent(&mut self, cx: &mut Context<Self>) {
        let Some(center) = self.agent_center.as_ref() else {
            return;
        };
        let provider = center.provider.clone();
        let cwd = PathBuf::from(center.cwd.trim());
        let search_path = std::env::var_os("PATH").unwrap_or_default();
        let request = LaunchRequest { provider: &provider, cwd: &cwd, search_path: &search_path };
        let outcome = prepare_launch(&self.agent_registry, request)
            .map_err(|err| err.to_string())
            .and_then(|spec| self.spawn_agent(&provider, spec, cx).ok_or_else(|| spawn_failure(&provider)));
        match outcome {
            Ok(()) => self.close_agent_center(cx),
            Err(message) => {
                if let Some(center) = self.agent_center.as_mut() {
                    center.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    fn spawn_agent(&mut self, provider: &str, spec: CommandSpec, cx: &mut Context<Self>) -> Option<()> {
        let title = self.agent_registry.get(provider).map(|a| a.display_name().to_string()).unwrap_or_default();
        let panel = PanelId::new();
        let command = ShellCommand { program: spec.executable, args: spec.args, cwd: spec.cwd.clone() };
        let session = self.spawn_session(panel, command, title.clone(), cx)?;
        let created = Agent::new(AgentId::new(), provider.to_string(), self.model.active_id(), session, spec.cwd.clone());
        let agent = created.transition(AgentState::Running).ok()?;
        self.model.add_panel(PanelSnapshot { panel: Panel::new_agent(panel, title, agent.id), cwd: spec.cwd });
        self.model.agents_mut().push(agent);
        self.model.tabs_mut().open(panel);
        self.persist();
        tracing::info!(%provider, "agent launched");
        Some(())
    }

    pub(super) fn record_agent_exit(&mut self, agent: AgentId, code: i32) {
        let Some(record) = self.model.agent_mut(agent) else {
            return;
        };
        let classified = self
            .agent_registry
            .get(&record.provider)
            .map(|adapter| adapter.classify_exit(known_exit_code(code)))
            .unwrap_or(AgentState::Failed);
        self.tracking.record_exit(record, code, classified);
        self.persist();
    }

    pub(super) fn mark_agent_terminated(&mut self, panel: PanelId) {
        let Some(agent) = self.model.agent_of(panel).and_then(|id| self.model.agent_mut(id)) else {
            return;
        };
        mark_terminated(agent);
    }

    pub(super) fn is_agent_alive(&self, panel: PanelId) -> bool {
        self.model
            .agent_of(panel)
            .and_then(|id| self.model.agents().iter().find(|agent| agent.id == id))
            .is_some_and(|agent| agent.state.is_alive())
    }

    pub(super) fn agent_provider(&self, id: AgentId) -> &str {
        self.model.agents().iter().find(|agent| agent.id == id).map(|agent| agent.provider.as_str()).unwrap_or_default()
    }

    fn request_stop_agent(&mut self, id: AgentId, cx: &mut Context<Self>) {
        self.pending = Some(Confirmation::StopAgent(id));
        self.close_agent_center(cx);
    }

    pub(super) fn stop_agent(&mut self, id: AgentId) {
        let Some(pane) = self.model.panel_for_agent(id).and_then(|panel| self.panes.get(&panel)) else {
            return;
        };
        if let Err(err) = pane.terminal.handle.write(INTERRUPT) {
            tracing::warn!(?err, "agent interrupt failed");
        }
        if let Some(agent) = self.model.agent_mut(id) {
            self.tracking.record_stop_request(agent, Utc::now());
        }
        self.persist();
    }

    fn force_kill_agent(&mut self, id: AgentId, cx: &mut Context<Self>) {
        let Some(pane) = self.model.panel_for_agent(id).and_then(|panel| self.panes.get(&panel)) else {
            return;
        };
        let handle = pane.terminal.handle.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = handle.kill() {
                    tracing::warn!(?err, "agent force kill failed");
                }
            })
            .detach();
    }

    fn focus_agent(&mut self, id: AgentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.model.panel_for_agent(id) else {
            return;
        };
        let Some((workspace, index)) = self.model.locate(panel) else {
            return;
        };
        if workspace != self.model.active_id() {
            self.model.switch_to(workspace);
            self.show_active_workspace(cx);
        }
        self.model.tabs_mut().activate(index);
        self.focus_pane(panel);
        window.focus(&self.focus_handle);
        self.close_agent_center(cx);
    }

    pub(super) fn tab_agent_state(&self, tab: &Tab) -> Option<AgentState> {
        tab.layout().leaves().into_iter().find_map(|panel| {
            let id = self.model.agent_of(panel)?;
            self.model.agents().iter().find(|agent| agent.id == id).map(|agent| agent.state)
        })
    }

    fn agent_rows(&self) -> Vec<AgentRow> {
        let now = Utc::now();
        let workspace = self.model.active_id();
        self.model
            .agents()
            .iter()
            .filter(|agent| agent.workspace == workspace)
            .map(|agent| self.tracking.row(agent, &self.home, now))
            .collect()
    }

    pub(super) fn render_agents_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let hover_bg = theme.border.muted;
        let running = self.agent_rows().iter().filter(|row| row.state.is_alive()).count();
        let label = match running {
            0 => "Agents".to_string(),
            n => format!("Agents \u{00b7} {n}"),
        };
        div()
            .id("agents-button")
            .h_full()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .border_l_1()
            .border_color(theme.border_default())
            .text_color(theme.text.muted)
            .hover(move |style| style.bg(hover_bg))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.toggle_agent_center(cx)))
            .when(running > 0, |el| el.child(status_dot(status_color(theme, AgentState::Running))))
            .child(label)
    }

    fn toggle_agent_center(&mut self, cx: &mut Context<Self>) {
        if self.agent_center.is_some() {
            self.close_agent_center(cx);
            return;
        }
        self.open_agent_center(cx);
    }

    pub(super) fn render_agent_center(&self, center: &AgentCenter, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let rows = self.agent_rows();
        let workspace = self.model.workspace_name(self.model.active_id()).unwrap_or_default().to_string();
        let list = if rows.is_empty() {
            vec![div().text_color(theme.text.muted).child("No agents in this workspace").into_any_element()]
        } else {
            rows.into_iter().map(|row| self.render_agent_row(row, cx).into_any_element()).collect()
        };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.close_agent_center(cx)))
            .child(
                div()
                    .absolute()
                    .top(px(theme.layout.tab_strip_height_px))
                    .right(px(theme.layout.gap_px))
                    .w(px(CENTER_WIDTH_PX))
                    .occlude()
                    .p_3()
                    .bg(theme.surface.raised)
                    .border_1()
                    .border_color(theme.border_default())
                    .rounded_md()
                    .shadow_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_color(theme.text.muted).child("Launch agent"))
                    .child(self.render_providers(&center.provider, cx))
                    .child(self.render_cwd_field(&center.cwd))
                    .child(self.render_launch_button(cx))
                    .children(center.error.clone().map(|error| div().text_color(theme.status.failed).child(error)))
                    .child(div().my_1().h(px(1.0)).bg(theme.border_default()))
                    .child(div().text_color(theme.text.muted).child(format!("Agents in {workspace}")))
                    .children(list),
            )
    }

    fn render_providers(&self, selected: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let pills = self.agent_registry.list().into_iter().map(|(id, name)| {
            let is_selected = id == selected;
            let provider = id.to_string();
            let (bg, fg) = if is_selected {
                (theme.accent.primary, theme.text.inverse)
            } else {
                (theme.surface.subtle, theme.text_default())
            };
            div()
                .id(SharedString::from(format!("provider-{id}")))
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .bg(bg)
                .text_color(fg)
                .border_1()
                .border_color(theme.border_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |shell, _event, _window, cx| shell.choose_provider(provider.clone(), cx)),
                )
                .child(name.to_string())
        });
        div().flex().flex_row().gap_2().children(pills)
    }

    fn render_cwd_field(&self, cwd: &SharedString) -> impl IntoElement {
        let theme = &self.state.theme;
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border_focus())
            .bg(theme.canvas())
            .flex()
            .flex_row()
            .child(cwd.clone())
            .child(div().text_color(theme.accent.primary).child(CARET))
    }

    fn render_launch_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        div().flex().flex_row().justify_end().child(
            self.prompt_button("agent-launch", "Launch", (theme.accent.primary, theme.text.inverse))
                .on_mouse_down(MouseButton::Left, cx.listener(|shell, _event, _window, cx| shell.launch_agent(cx))),
        )
    }

    fn render_agent_row(&self, row: AgentRow, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.theme;
        let hover_bg = theme.surface.subtle;
        let id = row.id;
        let danger = theme.status.failed;
        let exit = row.exit_code.map(|code| format!("exit {code}")).unwrap_or_default();
        div()
            .id(SharedString::from(format!("agent-{id}")))
            .px_2()
            .py_1()
            .rounded_md()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, window, cx| shell.focus_agent(id, window, cx)))
            .child(status_dot(status_color(theme, row.state)))
            .child(div().child(row.provider))
            .child(div().flex_1().text_color(theme.text.muted).child(row.cwd_label))
            .child(div().text_color(theme.text.muted).child(state_label(row.state)))
            .child(div().text_color(theme.text.muted).child(row.elapsed))
            .child(div().text_color(theme.text.muted).child(exit))
            .when(row.can_stop, |el| {
                el.child(row_action(("stop", id), "Stop", cx, move |shell, cx| shell.request_stop_agent(id, cx)).text_color(danger))
            })
            .when(row.can_force, |el| {
                el.child(row_action(("force", id), "Force kill", cx, move |shell, cx| shell.force_kill_agent(id, cx)).text_color(danger))
            })
    }
}

fn row_action(
    (kind, id): (&'static str, AgentId),
    label: &'static str,
    cx: &mut Context<Shell>,
    action: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(format!("{kind}-{id}")))
        .px_1()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, _window, cx| {
                cx.stop_propagation();
                action(shell, cx);
            }),
        )
        .child(label)
}

pub(super) fn status_dot(color: gpui::Hsla) -> impl IntoElement {
    div().size(px(STATUS_DOT_PX)).flex_none().rounded_full().bg(color)
}

fn spawn_failure(provider: &str) -> String {
    format!("Failed to start {provider}; see the log for details")
}
