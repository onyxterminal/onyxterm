#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::tabs::{Tab, Tabs};
use domain::{DomainError, Panel, PanelId, TabId, TerminalSessionId, Workspace, WorkspaceId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use workspace_service::{PanelSnapshot, StoreWarning, TabSnapshot, WorkspaceSnapshot, DEFAULT_PANEL_TITLE};

pub const SCRATCH_NAME: &str = "Scratch";

#[derive(Debug)]
pub struct WorkspaceEntry {
    pub workspace: Workspace,
    pub tabs: Tabs,
    pub is_scratch: bool,
}

impl WorkspaceEntry {
    fn scratch() -> Self {
        let workspace = Workspace::new(WorkspaceId::new(), SCRATCH_NAME.to_string());
        Self { workspace, tabs: Tabs::default(), is_scratch: true }
    }
}

#[derive(Debug)]
pub struct WorkspaceModel {
    entries: Vec<WorkspaceEntry>,
    panels: HashMap<PanelId, PanelSnapshot>,
    active: usize,
    home: PathBuf,
}

impl WorkspaceModel {
    pub fn from_snapshot(snapshot: WorkspaceSnapshot, home: &Path) -> Self {
        let mut panels = HashMap::new();
        let mut tabs_by_id: HashMap<TabId, Tab> = HashMap::new();
        for tab in snapshot.tabs {
            for panel in tab.panels {
                panels.insert(panel.panel.id, panel);
            }
            let mut restored = Tab::restore(tab.tab.id, tab.tab.layout, tab.tab.focused_panel);
            restored.rename(tab.tab.title);
            tabs_by_id.insert(tab.tab.id, restored);
        }
        let mut entries: Vec<WorkspaceEntry> = snapshot
            .workspaces
            .into_iter()
            .map(|workspace| restore_entry(workspace, &mut tabs_by_id))
            .collect();
        if entries.is_empty() {
            entries.push(WorkspaceEntry::scratch());
        }
        let active = snapshot
            .active_workspace
            .and_then(|id| entries.iter().position(|entry| entry.workspace.id == id))
            .unwrap_or(0);
        let mut model = Self { entries, panels, active, home: home.to_path_buf() };
        model.fill_missing_panels();
        model
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let persisted: Vec<&WorkspaceEntry> = self.entries.iter().filter(|entry| !entry.is_scratch).collect();
        let is_persisted = |id: WorkspaceId| persisted.iter().any(|entry| entry.workspace.id == id);
        WorkspaceSnapshot {
            workspaces: persisted.iter().map(|entry| snapshot_workspace(entry)).collect(),
            tabs: persisted.iter().flat_map(|entry| self.snapshot_tabs(entry)).collect(),
            agents: Vec::new(),
            active_workspace: Some(self.active_id()).filter(|id| is_persisted(*id)),
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

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn active_root(&self) -> PathBuf {
        self.entries[self.active].workspace.root.clone().unwrap_or_else(|| self.home.clone())
    }

    pub fn workspace_id_at(&self, index: usize) -> Option<WorkspaceId> {
        self.entries.get(index).map(|entry| entry.workspace.id)
    }

    pub fn neighbor_workspace_id(&self, step: isize) -> WorkspaceId {
        let index = (self.active as isize + step).rem_euclid(self.entries.len() as isize) as usize;
        self.entries[index].workspace.id
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
        entry.tabs.close_panel(panel)?;
        self.drop_empty_scratch();
        Ok(())
    }

    fn drop_empty_scratch(&mut self) {
        let has_workspaces = self.entries.iter().any(|entry| !entry.is_scratch);
        let Some(index) = self.entries.iter().position(|entry| entry.is_scratch && entry.tabs.is_empty()) else {
            return;
        };
        if !has_workspaces {
            return;
        }
        let active_id = self.active_id();
        self.entries.remove(index);
        self.active = self.position(active_id).unwrap_or(0);
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
        self.entries.push(WorkspaceEntry { workspace, tabs: Tabs::default(), is_scratch: false });
        id
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, name: String) {
        if let Some(index) = self.position(id) {
            self.entries[index].workspace.name = name;
        }
    }

    pub fn workspace_panels(&self, id: WorkspaceId) -> Vec<PanelId> {
        self.position(id)
            .map(|index| self.entries[index].tabs.iter().flat_map(|tab| tab.layout().leaves()).collect())
            .unwrap_or_default()
    }

    pub fn delete_workspace(&mut self, id: WorkspaceId) {
        let Some(index) = self.position(id).filter(|index| !self.entries[*index].is_scratch) else {
            return;
        };
        for panel in self.workspace_panels(id) {
            self.panels.remove(&panel);
        }
        let active_id = self.active_id();
        self.entries.remove(index);
        if self.entries.is_empty() {
            self.entries.push(WorkspaceEntry::scratch());
        }
        self.active = self
            .position(active_id)
            .or_else(|| self.entries.iter().position(|entry| !entry.is_scratch))
            .unwrap_or(0);
    }

    pub fn workspace_name(&self, id: WorkspaceId) -> Option<&str> {
        self.position(id).map(|index| self.entries[index].workspace.name.as_str())
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
            let mut snapshot = domain::Tab::new(tab.id(), entry.workspace.id, tab.layout().clone());
            snapshot.title = tab.name().map(str::to_string);
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
    WorkspaceEntry { workspace, tabs: Tabs::restore(tabs, active), is_scratch: false }
}

fn snapshot_workspace(entry: &WorkspaceEntry) -> Workspace {
    let mut workspace = entry.workspace.clone();
    workspace.tab_order = entry.tabs.iter().map(Tab::id).collect();
    workspace.active_tab = entry.tabs.active().map(Tab::id);
    workspace
}

pub fn fresh_terminal(panel: PanelId, cwd: PathBuf) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_terminal(panel, DEFAULT_PANEL_TITLE.to_string(), TerminalSessionId::new()),
        cwd,
        name: None,
        path: None,
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
