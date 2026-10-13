use crate::{collect_workspace, handle_reducer_event, LayoutService, SshService, TabService, WorkspaceService};
use domain::{read_ssh_hosts, Axis, DomainError, LayoutNode, PanelId, SshHost, Tab, TabId, Workspace, WorkspaceId, WorkspaceReducer};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct InMemoryWorkspaceService {
    inner: Arc<RwLock<HashMap<WorkspaceId, Workspace>>>,
}

impl InMemoryWorkspaceService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, workspace: Workspace) {
        self.inner.write().insert(workspace.id, workspace);
    }
}

impl WorkspaceService for InMemoryWorkspaceService {
    fn create(&self, name: String, root: Option<PathBuf>) -> Result<Workspace, DomainError> {
        let id = WorkspaceId::new();
        let workspace = Workspace {
            id,
            name,
            root,
            tab_order: Vec::new(),
            active_tab: None,
            archived: false,
        };
        self.inner.write().insert(id, workspace.clone());
        Ok(workspace)
    }

    fn rename(&self, id: WorkspaceId, name: String) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&id)
            .ok_or_else(|| DomainError::WorkspaceNotFound(id.to_string()))?;
        let mut reducer = WorkspaceReducer::new(ws.clone());
        let event = reducer.rename(name);
        handle_reducer_event(event);
        *ws = reducer.into_workspace();
        Ok(())
    }

    fn archive(&self, id: WorkspaceId) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&id)
            .ok_or_else(|| DomainError::WorkspaceNotFound(id.to_string()))?;
        ws.archived = true;
        Ok(())
    }

    fn restore(&self, id: WorkspaceId) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&id)
            .ok_or_else(|| DomainError::WorkspaceNotFound(id.to_string()))?;
        ws.archived = false;
        Ok(())
    }

    fn set_root(&self, id: WorkspaceId, root: Option<PathBuf>) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&id)
            .ok_or_else(|| DomainError::WorkspaceNotFound(id.to_string()))?;
        ws.root = root;
        Ok(())
    }

    fn list_active(&self) -> Vec<Workspace> {
        self.inner
            .read()
            .values()
            .filter(|w| !w.archived)
            .cloned()
            .collect()
    }

    fn get(&self, id: WorkspaceId) -> Option<Workspace> {
        self.inner.read().get(&id).cloned()
    }
}

#[derive(Default)]
pub struct InMemoryTabService {
    inner: Arc<RwLock<HashMap<WorkspaceId, Workspace>>>,
}

impl InMemoryTabService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, workspace: Workspace) {
        self.inner.write().insert(workspace.id, workspace);
    }
}

impl TabService for InMemoryTabService {
    fn create(&self, workspace: WorkspaceId, title: Option<String>) -> Result<Tab, DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let reducer_ws = ws.clone();
        let mut reducer = WorkspaceReducer::new(reducer_ws);
        let tab_id = TabId::new();
        let panel_id = PanelId::new();
        let layout = LayoutNode::leaf(panel_id);
        let mut tab = Tab::new(tab_id, workspace, layout);
        tab.title = title;
        reducer.add_tab(tab.clone(), None)?;
        handle_reducer_event(domain::WorkspaceReducerEvent::TabCreated(tab.clone()));
        *ws = reducer.into_workspace();
        Ok(tab)
    }

    fn close(&self, workspace: WorkspaceId, tab: TabId) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let mut reducer = WorkspaceReducer::new(ws.clone());
        reducer.remove_tab(tab)?;
        *ws = reducer.into_workspace();
        Ok(())
    }

    fn reorder(&self, workspace: WorkspaceId, from: usize, to: usize) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let mut reducer = WorkspaceReducer::new(ws.clone());
        reducer.reorder_tab(from, to)?;
        *ws = reducer.into_workspace();
        Ok(())
    }

    fn focus(&self, workspace: WorkspaceId, tab: TabId) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let ws = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let mut reducer = WorkspaceReducer::new(ws.clone());
        reducer.set_active_tab(Some(tab))?;
        *ws = reducer.into_workspace();
        Ok(())
    }

    fn list(&self, workspace: WorkspaceId) -> Vec<Tab> {
        let _ = collect_workspace(&self.inner.read(), workspace);
        Vec::new()
    }

    fn get(&self, workspace: WorkspaceId, tab: TabId) -> Option<Tab> {
        let _ = (workspace, tab);
        None
    }
}

#[derive(Default)]
pub struct InMemoryLayoutService {
    inner: Arc<RwLock<HashMap<WorkspaceId, HashMap<TabId, LayoutNode>>>>,
    ratio: f32,
}

impl InMemoryLayoutService {
    pub fn new() -> Self {
        Self {
            ratio: crate::ServiceConfig::default_split_ratio(),
            ..Self::default()
        }
    }

    pub fn with_ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio;
        self
    }

    pub fn seed(&self, workspace: WorkspaceId, tab: TabId, layout: LayoutNode) {
        self.inner
            .write()
            .entry(workspace)
            .or_default()
            .insert(tab, layout);
    }
}

impl LayoutService for InMemoryLayoutService {
    fn split(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        target: PanelId,
        axis: Axis,
        new_panel: PanelId,
    ) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let tabs = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let node = tabs
            .get_mut(&tab)
            .ok_or_else(|| DomainError::TabNotFound(tab.to_string()))?;
        node.split_with(target, axis, self.ratio, new_panel)?;
        Ok(())
    }

    fn close_panel(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        panel: PanelId,
    ) -> Result<(), DomainError> {
        let mut map = self.inner.write();
        let tabs = map
            .get_mut(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let node = tabs
            .get_mut(&tab)
            .ok_or_else(|| DomainError::TabNotFound(tab.to_string()))?;
        node.close_leaf(&mut |id| id == panel)?;
        Ok(())
    }

    fn focus_panel(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
        panel: PanelId,
    ) -> Result<(), DomainError> {
        let map = self.inner.read();
        let tabs = map
            .get(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        let node = tabs
            .get(&tab)
            .ok_or_else(|| DomainError::TabNotFound(tab.to_string()))?;
        if !node.find_leaf(&mut |id| id == panel).is_some() {
            return Err(DomainError::PanelNotFound(panel.to_string()));
        }
        Ok(())
    }

    fn snapshot(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
    ) -> Result<LayoutNode, DomainError> {
        let map = self.inner.read();
        let tabs = map
            .get(&workspace)
            .ok_or_else(|| DomainError::WorkspaceNotFound(workspace.to_string()))?;
        tabs.get(&tab)
            .cloned()
            .ok_or_else(|| DomainError::TabNotFound(tab.to_string()))
    }
}

pub struct InMemorySshService {
    hosts: RwLock<Vec<SshHost>>,
    config_path: PathBuf,
}

impl InMemorySshService {
    pub fn new(home: &Path) -> Self {
        let config_path = ssh_config_path(home);
        let hosts = read_ssh_hosts(&config_path);
        Self { hosts: RwLock::new(hosts), config_path }
    }

    pub fn reload(&self) {
        let hosts = read_ssh_hosts(&self.config_path);
        *self.hosts.write() = hosts;
    }
}

impl SshService for InMemorySshService {
    fn list_hosts(&self) -> Vec<SshHost> {
        self.hosts.read().clone()
    }

    fn host_count(&self) -> usize {
        self.hosts.read().len()
    }
}

fn ssh_config_path(home: &Path) -> PathBuf {
    home.join(".ssh").join("config")
}
