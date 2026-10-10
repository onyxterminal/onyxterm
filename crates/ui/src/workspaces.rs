#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::tabs::{Tab, Tabs};
use domain::{Agent, DomainError, Panel, PanelId, PanelKind, TabId, TerminalSessionId, Workspace, WorkspaceId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use workspace_service::{PanelSnapshot, StoreWarning, TabSnapshot, WorkspaceSnapshot, DEFAULT_PANEL_TITLE};

const FALLBACK_WORKSPACE_NAME: &str = "Personal";

#[derive(Debug)]
pub struct WorkspaceEntry {
    pub workspace: Workspace,
    pub tabs: Tabs,
}

#[derive(Debug)]
pub struct WorkspaceModel {
    entries: Vec<WorkspaceEntry>,
    panels: HashMap<PanelId, PanelSnapshot>,
    agents: Vec<Agent>,
    active: usize,
    home: PathBuf,
}

impl WorkspaceModel {
    pub fn from_snapshot(snapshot: WorkspaceSnapshot, home: &Path) -> Self {
        let mut panels = HashMap::new();
        let mut tabs_by_id: HashMap<TabId, Tab> = HashMap::new();
        for tab in snapshot.tabs {
            for panel in tab.panels {
                panels.insert(panel.panel.id, as_new_terminal(panel));
            }
            tabs_by_id.insert(tab.tab.id, Tab::restore(tab.tab.id, tab.tab.layout, tab.tab.focused_panel));
        }
        let mut entries: Vec<WorkspaceEntry> = snapshot
            .workspaces
            .into_iter()
            .map(|workspace| restore_entry(workspace, &mut tabs_by_id))
            .collect();
        if entries.is_empty() {
            let workspace = Workspace::new(WorkspaceId::new(), FALLBACK_WORKSPACE_NAME.to_string());
            entries.push(WorkspaceEntry { workspace, tabs: Tabs::default() });
        }
        let active = snapshot
            .active_workspace
            .and_then(|id| entries.iter().position(|entry| entry.workspace.id == id))
            .unwrap_or(0);
        let mut model = Self { entries, panels, agents: snapshot.agents, active, home: home.to_path_buf() };
        model.fill_missing_panels();
        model
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspaces: self.entries.iter().map(snapshot_workspace).collect(),
            tabs: self.entries.iter().flat_map(|entry| self.snapshot_tabs(entry)).collect(),
            agents: self.agents.clone(),
            active_workspace: Some(self.active_id()),
        }
    }

    pub fn entries(&self) -> &[WorkspaceEntry] {
        &self.entries
    }

    pub fn active_id(&self) -> WorkspaceId {
        self.entries[self.active].workspace.id
    }

    pub fn tabs(&self) -> &Tabs {
        &self.entries[self.active].tabs
    }

    pub fn tabs_mut(&mut self) -> &mut Tabs {
        &mut self.entries[self.active].tabs
    }

    pub fn active_root(&self) -> PathBuf {
        self.entries[self.active].workspace.root.clone().unwrap_or_else(|| self.home.clone())
    }

    pub fn switch_to(&mut self, id: WorkspaceId) {
        if let Some(index) = self.position(id) {
            self.active = index;
        }
    }

    pub fn panels_to_spawn(&self, is_live: impl Fn(PanelId) -> bool) -> Vec<PanelId> {
        self.tabs().iter().flat_map(|tab| tab.layout().leaves()).filter(|panel| !is_live(*panel)).collect()
    }

    pub fn panel(&self, id: PanelId) -> Option<&PanelSnapshot> {
        self.panels.get(&id)
    }

    pub fn panel_mut(&mut self, id: PanelId) -> Option<&mut PanelSnapshot> {
        self.panels.get_mut(&id)
    }

    pub fn add_panel(&mut self, panel: PanelSnapshot) {
        self.panels.insert(panel.panel.id, panel);
    }

    pub fn remove_panel(&mut self, panel: PanelId) -> Result<(), DomainError> {
        self.panels.remove(&panel);
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.tabs.position_of(panel).is_some())
            .ok_or_else(|| DomainError::PanelNotFound(panel.to_string()))?;
        entry.tabs.close_panel(panel)
    }

    pub fn locate(&self, panel: PanelId) -> Option<(WorkspaceId, usize)> {
        self.entries
            .iter()
            .find_map(|entry| entry.tabs.position_of(panel).map(|index| (entry.workspace.id, index)))
    }

    pub fn create_workspace(&mut self, name: String) -> WorkspaceId {
        let mut workspace = Workspace::new(WorkspaceId::new(), name);
        workspace.root = Some(self.home.clone());
        let id = workspace.id;
        self.entries.push(WorkspaceEntry { workspace, tabs: Tabs::default() });
        id
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, name: String) {
        if let Some(index) = self.position(id) {
            self.entries[index].workspace.name = name;
        }
    }

    pub fn can_delete_workspace(&self) -> bool {
        self.entries.len() > 1
    }

    pub fn workspace_panels(&self, id: WorkspaceId) -> Vec<PanelId> {
        self.position(id)
            .map(|index| self.entries[index].tabs.iter().flat_map(|tab| tab.layout().leaves()).collect())
            .unwrap_or_default()
    }

    pub fn delete_workspace(&mut self, id: WorkspaceId) {
        if !self.can_delete_workspace() {
            return;
        }
        let Some(index) = self.position(id) else {
            return;
        };
        for panel in self.workspace_panels(id) {
            self.panels.remove(&panel);
        }
        self.agents.retain(|agent| agent.workspace != id);
        let active_id = self.active_id();
        self.entries.remove(index);
        self.active = self.position(active_id).unwrap_or(0);
    }

    pub fn workspace_name(&self, id: WorkspaceId) -> Option<&str> {
        self.position(id).map(|index| self.entries[index].workspace.name.as_str())
    }

    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    pub fn agents_mut(&mut self) -> &mut Vec<Agent> {
        &mut self.agents
    }

    fn position(&self, id: WorkspaceId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.workspace.id == id)
    }

    fn fill_missing_panels(&mut self) {
        for entry in &self.entries {
            let cwd = entry.workspace.root.clone().unwrap_or_else(|| self.home.clone());
            for panel in entry.tabs.iter().flat_map(|tab| tab.layout().leaves()) {
                self.panels.entry(panel).or_insert_with(|| fresh_terminal(panel, cwd.clone()));
            }
        }
    }

    fn snapshot_tabs<'a>(&'a self, entry: &'a WorkspaceEntry) -> impl Iterator<Item = TabSnapshot> + 'a {
        entry.tabs.iter().map(move |tab| {
            let panels: Vec<PanelSnapshot> =
                tab.layout().leaves().iter().filter_map(|panel| self.panels.get(panel)).cloned().collect();
            let title = self.panels.get(&tab.focused()).map(|panel| panel.panel.title.clone());
            let mut snapshot = domain::Tab::new(tab.id(), entry.workspace.id, tab.layout().clone());
            snapshot.title = title;
            snapshot.focused_panel = Some(tab.focused());
            TabSnapshot { tab: snapshot, panels }
        })
    }
}

fn restore_entry(workspace: Workspace, tabs_by_id: &mut HashMap<TabId, Tab>) -> WorkspaceEntry {
    let tabs: Vec<Tab> = workspace.tab_order.iter().filter_map(|id| tabs_by_id.remove(id)).collect();
    let active = workspace
        .active_tab
        .and_then(|active| tabs.iter().position(|tab| tab.id() == active))
        .unwrap_or(0);
    WorkspaceEntry { workspace, tabs: Tabs::restore(tabs, active) }
}

fn snapshot_workspace(entry: &WorkspaceEntry) -> Workspace {
    let mut workspace = entry.workspace.clone();
    workspace.tab_order = entry.tabs.iter().map(Tab::id).collect();
    workspace.active_tab = entry.tabs.active().map(Tab::id);
    workspace
}

fn as_new_terminal(snapshot: PanelSnapshot) -> PanelSnapshot {
    if snapshot.panel.kind == PanelKind::Terminal {
        return snapshot;
    }
    fresh_terminal(snapshot.panel.id, snapshot.cwd)
}

pub fn fresh_terminal(panel: PanelId, cwd: PathBuf) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_terminal(panel, DEFAULT_PANEL_TITLE.to_string(), TerminalSessionId::new()),
        cwd,
    }
}

pub fn restore_notice(warnings: &[StoreWarning]) -> Option<String> {
    let lines: Vec<String> = warnings.iter().map(warning_text).collect();
    (!lines.is_empty()).then(|| lines.join("  \u{2022}  "))
}

fn warning_text(warning: &StoreWarning) -> String {
    match warning {
        StoreWarning::DatabaseQuarantined { quarantined_to, .. } => {
            format!("Saved layout was unreadable and moved to {}", quarantined_to.display())
        }
        StoreWarning::MissingDirectory { missing, fallback, .. } => {
            format!("{} no longer exists; opened {} instead", missing.display(), fallback.display())
        }
        StoreWarning::LayoutDropped(_) => "An invalid saved layout was dropped".to_string(),
    }
}
