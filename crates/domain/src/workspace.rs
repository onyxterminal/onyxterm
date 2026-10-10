use crate::id::{PanelId, TabId, WorkspaceId};
use crate::layout::LayoutNode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub workspace_id: WorkspaceId,
    pub title: Option<String>,
    pub layout: LayoutNode,
    pub focused_panel: Option<PanelId>,
}

impl Tab {
    pub fn new(id: TabId, workspace_id: WorkspaceId, layout: LayoutNode) -> Self {
        let focused_panel = layout.leaves().first().copied();
        Self {
            id,
            workspace_id,
            title: None,
            layout,
            focused_panel,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root: Option<PathBuf>,
    pub tab_order: Vec<TabId>,
    pub active_tab: Option<TabId>,
    pub archived: bool,
}

impl Workspace {
    pub fn new(id: WorkspaceId, name: String) -> Self {
        Self {
            id,
            name,
            root: None,
            tab_order: Vec::new(),
            active_tab: None,
            archived: false,
        }
    }

    pub fn validate(&self) -> Result<(), crate::error::DomainError> {
        use crate::error::DomainError;
        for tab_id in &self.tab_order {
            if !self.tab_order.iter().any(|other| other == tab_id) {
                return Err(DomainError::TabNotFound(tab_id.to_string()));
            }
        }
        if let Some(active) = self.active_tab {
            if !self.tab_order.contains(&active) {
                return Err(DomainError::InconsistentActiveTab);
            }
        }
        Ok(())
    }
}
