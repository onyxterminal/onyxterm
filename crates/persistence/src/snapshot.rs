use crate::codec::{agent_state_to_str, encode_json, encode_layout, panel_kind_to_str, parse_uuid_id, path_to_string_lossy, resource_ref_columns};
use crate::error::PersistenceError;
use crate::repository::Repository;
use crate::restore::RestoreWarning;
use crate::workspace_repo::write_workspace_row;
use domain::{Agent, Panel, PanelId, Tab, Workspace, WorkspaceId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const ACTIVE_WORKSPACE_KEY: &str = "workspace.active";

#[derive(Debug, Clone, PartialEq)]
pub struct PanelSnapshot {
    pub panel: Panel,
    pub cwd: PathBuf,
    pub name: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabSnapshot {
    pub tab: Tab,
    pub panels: Vec<PanelSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorkspaceSnapshot {
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<TabSnapshot>,
    pub agents: Vec<Agent>,
    pub active_workspace: Option<WorkspaceId>,
}

#[derive(Debug, Clone)]
pub struct LoadedSnapshot {
    pub snapshot: WorkspaceSnapshot,
    pub warnings: Vec<RestoreWarning>,
}

impl Repository {
    pub fn write_snapshot(&self, snapshot: &WorkspaceSnapshot) -> Result<(), PersistenceError> {
        self.pool.transaction(|tx| {
            for workspace in &snapshot.workspaces {
                write_workspace_row(tx, workspace)?;
            }
            for tab in &snapshot.tabs {
                tx.execute(
                    "INSERT INTO tabs (id, workspace_id, title, focused_panel) VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id, title=excluded.title, focused_panel=excluded.focused_panel",
                    rusqlite::params![tab.tab.id.to_string(), tab.tab.workspace_id.to_string(), tab.tab.title, tab.tab.focused_panel.map(|p| p.to_string())],
                )?;
                tx.execute(
                    "INSERT INTO layouts (tab_id, tree_json) VALUES (?1, ?2) \
                     ON CONFLICT(tab_id) DO UPDATE SET tree_json=excluded.tree_json",
                    rusqlite::params![tab.tab.id.to_string(), encode_layout(&tab.tab.layout)?],
                )?;
                for panel in &tab.panels {
                    let (resource_kind, resource_id) = resource_ref_columns(panel.panel.resource);
                    tx.execute(
                        "INSERT INTO panels (id, tab_id, kind, title, resource_kind, resource_id, cwd, name, path) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                         ON CONFLICT(id) DO UPDATE SET tab_id=excluded.tab_id, kind=excluded.kind, title=excluded.title, \
                         resource_kind=excluded.resource_kind, resource_id=excluded.resource_id, cwd=excluded.cwd, name=excluded.name, \
                         path=excluded.path",
                        rusqlite::params![panel.panel.id.to_string(), tab.tab.id.to_string(), panel_kind_to_str(panel.panel.kind),
                            panel.panel.title, resource_kind, resource_id, path_to_string_lossy(&panel.cwd), panel.name,
                            panel.path.as_deref().map(path_to_string_lossy)],
                    )?;
                }
            }
            for agent in &snapshot.agents {
                tx.execute(
                    "INSERT INTO agents (id, provider, workspace_id, terminal_session_id, cwd, state, started_at, ended_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                     ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, workspace_id=excluded.workspace_id, \
                     terminal_session_id=excluded.terminal_session_id, cwd=excluded.cwd, state=excluded.state, \
                     started_at=excluded.started_at, ended_at=excluded.ended_at",
                    rusqlite::params![agent.id.to_string(), agent.provider, agent.workspace.to_string(), agent.terminal_session.to_string(),
                        path_to_string_lossy(&agent.cwd), agent_state_to_str(agent.state),
                        agent.started_at.map(|d| d.to_rfc3339()), agent.ended_at.map(|d| d.to_rfc3339())],
                )?;
            }
            delete_absent(tx, "panels", snapshot.tabs.iter().flat_map(|t| t.panels.iter()).map(|p| p.panel.id.to_string()))?;
            delete_absent(tx, "tabs", snapshot.tabs.iter().map(|t| t.tab.id.to_string()))?;
            delete_absent(tx, "agents", snapshot.agents.iter().map(|a| a.id.to_string()))?;
            delete_absent(tx, "workspaces", snapshot.workspaces.iter().map(|w| w.id.to_string()))?;
            write_active_workspace(tx, snapshot.active_workspace)
        })
    }

    pub fn load_snapshot(&self, default_cwd: &Path) -> Result<LoadedSnapshot, PersistenceError> {
        let restored = self.restore()?;
        let cwds = self.panel_cwds()?;
        let mut names = self.panel_names()?;
        let mut paths = self.panel_paths()?;
        let panels_by_id: HashMap<PanelId, Panel> =
            restored.panels.into_iter().map(|p| (p.id, p)).collect();
        let tabs = restored
            .tabs
            .into_iter()
            .map(|tab| {
                let panels = tab
                    .layout
                    .leaves()
                    .into_iter()
                    .filter_map(|id| panels_by_id.get(&id).cloned())
                    .map(|panel| PanelSnapshot {
                        cwd: cwds.get(&panel.id).cloned().unwrap_or_else(|| default_cwd.to_path_buf()),
                        name: names.remove(&panel.id),
                        path: paths.remove(&panel.id),
                        panel,
                    })
                    .collect();
                TabSnapshot { tab, panels }
            })
            .collect();
        let active_workspace = restored
            .app_settings
            .iter()
            .find(|(key, _)| key == ACTIVE_WORKSPACE_KEY)
            .and_then(|(_, value)| parse_uuid_id(value).ok());
        Ok(LoadedSnapshot {
            snapshot: WorkspaceSnapshot {
                workspaces: restored.workspaces,
                tabs,
                agents: restored.agents,
                active_workspace,
            },
            warnings: restored.warnings,
        })
    }

    fn panel_names(&self) -> Result<HashMap<PanelId, String>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, name FROM panels WHERE name IS NOT NULL")?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
            let mut out = HashMap::new();
            for row in rows {
                let (id, name) = row?;
                out.insert(parse_uuid_id(&id)?, name);
            }
            Ok(out)
        })
    }

    fn panel_paths(&self) -> Result<HashMap<PanelId, PathBuf>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, path FROM panels WHERE path IS NOT NULL")?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
            let mut out = HashMap::new();
            for row in rows {
                let (id, path) = row?;
                out.insert(parse_uuid_id(&id)?, PathBuf::from(path));
            }
            Ok(out)
        })
    }

    fn panel_cwds(&self) -> Result<HashMap<PanelId, PathBuf>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, cwd FROM panels WHERE cwd IS NOT NULL")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = HashMap::new();
            for row in rows {
                let (id, cwd) = row?;
                out.insert(parse_uuid_id(&id)?, PathBuf::from(cwd));
            }
            Ok(out)
        })
    }
}

fn delete_absent(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    kept_ids: impl Iterator<Item = String>,
) -> Result<(), PersistenceError> {
    let kept: Vec<String> = kept_ids.collect();
    tx.execute(
        &format!("DELETE FROM {table} WHERE id NOT IN (SELECT value FROM json_each(?1))"),
        [encode_json(&kept)?],
    )?;
    Ok(())
}

fn write_active_workspace(
    tx: &rusqlite::Transaction<'_>,
    active: Option<WorkspaceId>,
) -> Result<(), PersistenceError> {
    match active {
        Some(id) => tx.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![ACTIVE_WORKSPACE_KEY, id.to_string()],
        )?,
        None => tx.execute("DELETE FROM app_settings WHERE key = ?1", [ACTIVE_WORKSPACE_KEY])?,
    };
    Ok(())
}
