use super::Shell;
use crate::document::explicit_title;
use crate::pane_title::{display_title, short_cwd, terminal_title, title_tooltip, TitleSource};
use crate::theme::Theme;
use domain::PanelId;
use gpui::{Context, Hsla, Task};
use std::path::PathBuf;
use std::time::Duration;
use super::toast::ToastTone;
use terminal_core::{BlockStatus, CommandBlock, ForegroundJob, ShellPhase, TerminalHandle};

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(500);
const FAILED_VERB: &str = "failed";
const FINISHED_VERB: &str = "finished";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RunState {
    Idle,
    Running,
    Failed,
}

impl RunState {
    pub(super) fn color(self, theme: &Theme) -> Hsla {
        match self {
            RunState::Idle => theme.status.idle,
            RunState::Running => theme.status.running,
            RunState::Failed => theme.status.failed,
        }
    }
}

#[derive(Default)]
pub(super) struct PaneStatus {
    job: Option<ForegroundJob>,
    cwd: Option<PathBuf>,
}

impl PaneStatus {
    fn read(handle: &TerminalHandle) -> Self {
        Self { job: handle.foreground_job(), cwd: handle.current_dir() }
    }
}

pub(super) fn poll_pane_statuses(cx: &mut Context<Shell>) -> Task<()> {
    cx.spawn(async move |shell, cx| loop {
        cx.background_executor().timer(STATUS_POLL_INTERVAL).await;
        if shell.update(cx, |shell, cx| shell.refresh_all_statuses(cx)).is_err() {
            break;
        }
    })
}

impl Shell {
    pub(super) fn pane_display_title(&self, panel: PanelId) -> String {
        let source = self.title_source(panel);
        match self.panes.get(&panel) {
            Some(pane) => terminal_title(&source, &pane.shell_name),
            None => display_title(&source),
        }
    }

    pub(super) fn pane_cwd_label(&self, panel: PanelId) -> Option<String> {
        self.panes.get(&panel)?;
        let source = self.title_source(panel);
        Some(short_cwd(source.cwd, source.home))
    }

    pub(super) fn tab_title(&self, tab: &crate::tabs::Tab) -> String {
        tab.name().map_or_else(|| self.pane_display_title(tab.focused()), str::to_string)
    }

    pub(super) fn tab_run_state(&self, tab: &crate::tabs::Tab) -> RunState {
        let states = tab.layout().leaves().into_iter().map(|panel| self.pane_run_state(panel));
        states.max().unwrap_or(RunState::Idle)
    }

    pub(super) fn pane_run_state(&self, panel: PanelId) -> RunState {
        let Some(pane) = self.panes.get(&panel) else {
            return RunState::Idle;
        };
        let terminal = pane.terminal.handle.terminal();
        if pane.status.job.is_some() || matches!(terminal.shell_phase(), ShellPhase::Running(_)) {
            return RunState::Running;
        }
        let last_outcome = terminal.with_command_blocks(|blocks| blocks.last_outcome());
        if last_outcome.is_some_and(|outcome| outcome.is_failure()) {
            return RunState::Failed;
        }
        RunState::Idle
    }

    pub(super) fn announce_finished_commands(
        &mut self,
        panel: PanelId,
        finished: Vec<CommandBlock>,
        cx: &mut Context<Self>,
    ) {
        let is_focused = self.model.tabs().active().is_some_and(|tab| tab.focused() == panel);
        let Some(block) = finished.into_iter().rev().find(|block| !block.command.is_empty()) else {
            return;
        };
        let BlockStatus::Finished(outcome) = block.status else {
            return;
        };
        if is_focused || outcome.is_interrupted() {
            return;
        }
        let (verb, tone) = match outcome.is_failure() {
            true => (FAILED_VERB, ToastTone::Failure),
            false => (FINISHED_VERB, ToastTone::Success),
        };
        let pane_name = self.pane_name(panel);
        self.show_toast(format!("{} {verb} in {pane_name}", block.command), tone, cx);
    }

    fn pane_name(&self, panel: PanelId) -> String {
        let explicit = self.model.panel(panel).and_then(explicit_title);
        let shell = self.panes.get(&panel).map(|pane| pane.shell_name.clone());
        explicit.map(str::to_string).or(shell).unwrap_or_default()
    }

    pub(super) fn pane_title_tooltip(&self, panel: PanelId) -> String {
        title_tooltip(&self.title_source(panel))
    }

    fn title_source(&self, panel: PanelId) -> TitleSource<'_> {
        let record = self.model.panel(panel);
        let pane = self.panes.get(&panel);
        let home = self.model.home();
        let cwd = pane
            .and_then(|pane| pane.status.cwd.as_deref())
            .or_else(|| record.map(|record| record.cwd.as_path()))
            .unwrap_or(home);
        TitleSource {
            name: record.and_then(explicit_title),
            job: pane.and_then(|pane| pane.status.job.as_ref()),
            osc: pane.and_then(|pane| pane.osc.as_ref()),
            cwd,
            home,
        }
    }

    fn refresh_all_statuses(&mut self, cx: &mut Context<Self>) {
        let panels: Vec<PanelId> = self.panes.keys().copied().collect();
        for panel in panels {
            self.refresh_status(panel, cx);
        }
    }

    pub(super) fn refresh_status(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get_mut(&panel).filter(|pane| !pane.is_refreshing) else {
            return;
        };
        pane.is_refreshing = true;
        let handle = pane.terminal.handle.clone();
        cx.spawn(async move |shell, cx| {
            let status = cx.background_executor().spawn(async move { PaneStatus::read(&handle) }).await;
            let _ = shell.update(cx, |shell, cx| shell.apply_status(panel, status, cx));
        })
        .detach();
    }

    fn apply_status(&mut self, panel: PanelId, status: PaneStatus, cx: &mut Context<Self>) {
        let shown_before = (self.pane_display_title(panel), self.pane_title_tooltip(panel));
        let Some(pane) = self.panes.get_mut(&panel) else {
            return;
        };
        pane.is_refreshing = false;
        pane.status = status;
        if (self.pane_display_title(panel), self.pane_title_tooltip(panel)) != shown_before {
            cx.notify();
        }
    }
}
