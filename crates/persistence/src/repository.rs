use crate::agent_repo::{AgentRepo, SqliteAgentRepo};
use crate::app_settings_repo::{AppSettingsRepo, SqliteAppSettingsRepo};
use crate::codec::{decode_layout, OptionalResult};
use crate::error::PersistenceError;
use crate::restore::{load_with_repair, RestoreSnapshot};
use crate::sqlite::SqlitePool;
use crate::tab_repo::{SqliteTabRepo, TabRepo};
use crate::workspace_repo::{SqliteWorkspaceRepo, WorkspaceRepo};
use domain::{LayoutNode, Panel, Tab, TabId, WorkspaceId};
use std::path::Path;

pub struct Repository {
    pub(crate) pool: SqlitePool,
}

impl Repository {
    pub fn open(path: &Path) -> Result<Self, PersistenceError> {
        Ok(Self {
            pool: SqlitePool::open(path)?,
        })
    }

    pub fn workspaces(&self) -> SqliteWorkspaceRepo<'_> {
        SqliteWorkspaceRepo { pool: &self.pool }
    }

    pub fn tabs(&self) -> SqliteTabRepo<'_> {
        SqliteTabRepo { pool: &self.pool }
    }

    pub fn agents(&self) -> SqliteAgentRepo<'_> {
        SqliteAgentRepo { pool: &self.pool }
    }

    pub fn settings(&self) -> SqliteAppSettingsRepo<'_> {
        SqliteAppSettingsRepo { pool: &self.pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn restore(&self) -> Result<RestoreSnapshot, PersistenceError> {
        let workspaces = self.workspaces().list()?;
        let mut raw_tabs: Vec<(WorkspaceId, Tab, Vec<Panel>)> = Vec::new();
        for ws in &workspaces {
            let tabs = self.tabs().list_for(ws.id)?;
            for tab in tabs {
                let panels = self.tabs().list_panels(ws.id, tab.id)?;
                raw_tabs.push((ws.id, tab, panels));
            }
        }
        let (tabs, panels, warnings) = load_with_repair(&self.pool, raw_tabs)?;
        let agents = self.agents().list()?;
        let settings_pairs = self.settings().all()?;
        Ok(RestoreSnapshot {
            workspaces,
            tabs,
            panels,
            agents,
            app_settings: settings_pairs,
            warnings,
        })
    }

    pub fn raw_layout(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
    ) -> Result<Option<LayoutNode>, PersistenceError> {
        let ws = match self.workspaces().get(workspace)? {
            Some(w) => w,
            None => return Ok(None),
        };
        if !ws.tab_order.contains(&tab) {
            return Ok(None);
        }
        self.pool.with_conn(|conn| {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT tree_json FROM layouts WHERE tab_id = ?1",
                    [tab.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            match raw {
                Some(s) => Ok(Some(decode_layout(&s)?)),
                None => Ok(None),
            }
        })
    }

    pub fn tabs_for_workspace(
        &self,
        workspace: WorkspaceId,
    ) -> Result<Vec<(Tab, Vec<Panel>)>, PersistenceError> {
        let tabs = self.tabs().list_for(workspace)?;
        let mut out = Vec::new();
        for tab in tabs {
            let panels = self.tabs().list_panels(workspace, tab.id)?;
            out.push((tab, panels));
        }
        Ok(out)
    }
}
