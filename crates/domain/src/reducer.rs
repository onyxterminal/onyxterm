use crate::error::DomainError;
use crate::id::TabId;
use crate::workspace::{Tab, Workspace};

#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceReducerEvent {
    TabAdded {
        tab: TabId,
        index: usize,
    },
    TabRemoved {
        tab: TabId,
    },
    TabReordered {
        tab: TabId,
        from: usize,
        to: usize,
    },
    ActiveTabChanged {
        tab: Option<TabId>,
    },
    TabCreated(Tab),
    WorkspaceRenamed(String),
}

pub struct WorkspaceReducer {
    workspace: Workspace,
}

impl WorkspaceReducer {
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn into_workspace(self) -> Workspace {
        self.workspace
    }

    pub fn add_tab(&mut self, tab: Tab, index: Option<usize>) -> Result<WorkspaceReducerEvent, DomainError> {
        if tab.workspace_id != self.workspace.id {
            return Err(DomainError::TabNotFound(tab.id.to_string()));
        }
        if self.workspace.tab_order.contains(&tab.id) {
            return Err(DomainError::DuplicateTab);
        }
        let idx = index.unwrap_or(self.workspace.tab_order.len());
        let clamped = idx.min(self.workspace.tab_order.len());
        self.workspace.tab_order.insert(clamped, tab.id);
        if self.workspace.active_tab.is_none() {
            self.workspace.active_tab = Some(tab.id);
        }
        Ok(WorkspaceReducerEvent::TabCreated(tab))
    }

    pub fn remove_tab(&mut self, tab: TabId) -> Result<WorkspaceReducerEvent, DomainError> {
        let idx = self
            .workspace
            .tab_order
            .iter()
            .position(|t| *t == tab)
            .ok_or_else(|| DomainError::TabNotFound(tab.to_string()))?;
        self.workspace.tab_order.remove(idx);
        if self.workspace.active_tab == Some(tab) {
            self.workspace.active_tab = self.workspace.tab_order.get(idx).copied();
        }
        Ok(WorkspaceReducerEvent::TabRemoved { tab })
    }

    pub fn reorder_tab(&mut self, from: usize, to: usize) -> Result<WorkspaceReducerEvent, DomainError> {
        if from >= self.workspace.tab_order.len() || to >= self.workspace.tab_order.len() {
            return Err(DomainError::TabNotFound(format!(
                "from={} to={} len={}",
                from,
                to,
                self.workspace.tab_order.len()
            )));
        }
        let tab = self.workspace.tab_order.remove(from);
        self.workspace.tab_order.insert(to, tab);
        Ok(WorkspaceReducerEvent::TabReordered { tab, from, to })
    }

    pub fn set_active_tab(&mut self, tab: Option<TabId>) -> Result<WorkspaceReducerEvent, DomainError> {
        if let Some(active) = tab {
            if !self.workspace.tab_order.contains(&active) {
                return Err(DomainError::InconsistentActiveTab);
            }
        }
        self.workspace.active_tab = tab;
        Ok(WorkspaceReducerEvent::ActiveTabChanged { tab })
    }

    pub fn rename(&mut self, name: String) -> WorkspaceReducerEvent {
        self.workspace.name = name.clone();
        WorkspaceReducerEvent::WorkspaceRenamed(name)
    }
}
