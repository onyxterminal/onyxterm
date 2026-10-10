#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{
    Axis, DomainError, LayoutNode, PanelId, Tab, TabId, Workspace, WorkspaceId,
    WorkspaceReducerEvent,
};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod in_memory;
mod restore_policy;
mod store;
mod writer;

pub use in_memory::{InMemoryLayoutService, InMemoryTabService, InMemoryWorkspaceService};
pub use persistence::{PanelSnapshot, RestoreWarning, TabSnapshot, WorkspaceSnapshot};
pub use restore_policy::{new_terminal_tab, DEFAULT_PANEL_TITLE};
pub use store::{RestoredState, StoreError, StoreWarning, WorkspaceStore};
pub use writer::SAVE_DEBOUNCE;

pub trait WorkspaceService: Send + Sync {
    fn create(&self, name: String, root: Option<PathBuf>) -> Result<Workspace, DomainError>;
    fn rename(&self, id: WorkspaceId, name: String) -> Result<(), DomainError>;
    fn archive(&self, id: WorkspaceId) -> Result<(), DomainError>;
    fn restore(&self, id: WorkspaceId) -> Result<(), DomainError>;
    fn set_root(&self, id: WorkspaceId, root: Option<PathBuf>) -> Result<(), DomainError>;
    fn list_active(&self) -> Vec<Workspace>;
    fn get(&self, id: WorkspaceId) -> Option<Workspace>;
}

pub trait TabService: Send + Sync {
    fn create(&self, workspace: WorkspaceId, title: Option<String>) -> Result<Tab, DomainError>;
    fn close(&self, workspace: WorkspaceId, tab: TabId) -> Result<(), DomainError>;
    fn reorder(&self, workspace: WorkspaceId, from: usize, to: usize) -> Result<(), DomainError>;
    fn focus(&self, workspace: WorkspaceId, tab: TabId) -> Result<(), DomainError>;
    fn list(&self, workspace: WorkspaceId) -> Vec<Tab>;
    fn get(&self, workspace: WorkspaceId, tab: TabId) -> Option<Tab>;
}

pub trait LayoutService: Send + Sync {
    fn split(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        target: PanelId,
        axis: Axis,
        new_panel: PanelId,
    ) -> Result<(), DomainError>;
    fn close_panel(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        panel: PanelId,
    ) -> Result<(), DomainError>;
    fn focus_panel(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        panel: PanelId,
    ) -> Result<(), DomainError>;
    fn snapshot(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
    ) -> Result<LayoutNode, DomainError>;
}

#[derive(Debug, Clone, Default)]
pub struct ServiceConfig;

impl ServiceConfig {
    pub fn default_split_ratio() -> f32 {
        0.5
    }
}

pub(crate) fn collect_workspace<T>(
    map: &HashMap<WorkspaceId, T>,
    id: WorkspaceId,
) -> Result<(), DomainError> {
    if map.contains_key(&id) {
        Ok(())
    } else {
        Err(DomainError::WorkspaceNotFound(id.to_string()))
    }
}

pub(crate) fn handle_reducer_event(event: WorkspaceReducerEvent) {
    tracing::debug!(?event, "workspace reducer event");
}
