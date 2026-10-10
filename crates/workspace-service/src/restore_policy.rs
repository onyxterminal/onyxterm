use crate::store::StoreWarning;
use domain::{
    AgentState, LayoutNode, Panel, PanelId, Tab, TabId, TerminalSessionId, Workspace, WorkspaceId,
};
use persistence::{PanelSnapshot, TabSnapshot, WorkspaceSnapshot};
use std::path::{Path, PathBuf};

pub const DEFAULT_PANEL_TITLE: &str = "Terminal";

pub fn new_terminal_tab(workspace: WorkspaceId, cwd: PathBuf) -> TabSnapshot {
    let panel_id = PanelId::new();
    let tab = Tab::new(TabId::new(), workspace, LayoutNode::leaf(panel_id));
    let panel = Panel::new_terminal(panel_id, DEFAULT_PANEL_TITLE.to_string(), TerminalSessionId::new());
    TabSnapshot {
        tab,
        panels: vec![PanelSnapshot { panel, cwd }],
    }
}

pub(crate) struct RestorePolicy<'a> {
    pub home: &'a Path,
    pub warnings: &'a mut Vec<StoreWarning>,
}

impl RestorePolicy<'_> {
    pub fn apply(&mut self, mut snapshot: WorkspaceSnapshot) -> WorkspaceSnapshot {
        self.fall_back_missing_roots(&mut snapshot.workspaces);
        self.fall_back_missing_panel_cwds(&mut snapshot);
        mark_agents_disconnected(&mut snapshot);
        ensure_every_workspace_has_a_tab(&mut snapshot, self.home);
        snapshot.active_workspace = resolve_active_workspace(&snapshot);
        snapshot
    }

    fn fall_back_missing_roots(&mut self, workspaces: &mut [Workspace]) {
        for workspace in workspaces {
            let Some(root) = workspace.root.as_ref().filter(|root| !root.is_dir()) else {
                continue;
            };
            self.warn_missing(workspace.id, root.clone(), self.home.to_path_buf());
            workspace.root = Some(self.home.to_path_buf());
        }
    }

    fn fall_back_missing_panel_cwds(&mut self, snapshot: &mut WorkspaceSnapshot) {
        for tab in &mut snapshot.tabs {
            let workspace_id = tab.tab.workspace_id;
            let fallback = snapshot
                .workspaces
                .iter()
                .find(|w| w.id == workspace_id)
                .and_then(|w| w.root.clone())
                .unwrap_or_else(|| self.home.to_path_buf());
            for panel in tab.panels.iter_mut().filter(|p| !p.cwd.is_dir()) {
                let missing = std::mem::replace(&mut panel.cwd, fallback.clone());
                self.warn_missing(workspace_id, missing, fallback.clone());
            }
        }
    }

    fn warn_missing(&mut self, workspace: WorkspaceId, missing: PathBuf, fallback: PathBuf) {
        let warning = StoreWarning::MissingDirectory {
            workspace,
            missing,
            fallback,
        };
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

fn mark_agents_disconnected(snapshot: &mut WorkspaceSnapshot) {
    for agent in snapshot.agents.iter_mut().filter(|a| a.state.is_alive()) {
        agent.state = AgentState::Exited;
    }
}

fn ensure_every_workspace_has_a_tab(snapshot: &mut WorkspaceSnapshot, home: &Path) {
    for workspace in &mut snapshot.workspaces {
        let existing: Vec<TabId> = snapshot
            .tabs
            .iter()
            .filter(|t| t.tab.workspace_id == workspace.id)
            .map(|t| t.tab.id)
            .collect();
        workspace.tab_order.retain(|id| existing.contains(id));
        if workspace.tab_order.is_empty() {
            let cwd = workspace.root.clone().unwrap_or_else(|| home.to_path_buf());
            let tab = new_terminal_tab(workspace.id, cwd);
            workspace.tab_order.push(tab.tab.id);
            snapshot.tabs.push(tab);
        }
        let active_is_valid = workspace
            .active_tab
            .is_some_and(|id| workspace.tab_order.contains(&id));
        if !active_is_valid {
            workspace.active_tab = workspace.tab_order.first().copied();
        }
    }
}

fn resolve_active_workspace(snapshot: &WorkspaceSnapshot) -> Option<WorkspaceId> {
    let is_open = |id: WorkspaceId| snapshot.workspaces.iter().any(|w| w.id == id && !w.archived);
    snapshot
        .active_workspace
        .filter(|id| is_open(*id))
        .or_else(|| snapshot.workspaces.iter().find(|w| !w.archived).map(|w| w.id))
        .or_else(|| snapshot.workspaces.first().map(|w| w.id))
}
