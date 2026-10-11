use super::Shell;
use crate::document::explicit_title;
use crate::pane_title::{display_title, title_tooltip, TitleSource};
use domain::PanelId;
use gpui::{Context, Task};
use std::path::PathBuf;
use std::time::Duration;
use terminal_core::{ForegroundJob, TerminalHandle};

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
        display_title(&self.title_source(panel))
    }

    pub(super) fn is_pane_busy(&self, panel: PanelId) -> bool {
        self.panes.get(&panel).is_some_and(|pane| pane.status.job.is_some())
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
