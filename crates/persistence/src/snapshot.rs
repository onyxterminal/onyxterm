use crate::agent_repo::write_agent_row;
use crate::codec::{encode_json, parse_uuid_id};
use crate::error::PersistenceError;
use crate::repository::Repository;
use crate::restore::RestoreWarning;
use crate::tab_repo::{write_panel_row, write_tab_row};
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
            delete_absent(tx, "workspaces", snapshot.workspaces.iter().map(|w| w.id.to_string()))?;
            for tab in &snapshot.tabs {
                write_tab_row(tx, &tab.tab)?;
                for panel in &tab.panels {
                    write_panel_row(tx, tab.tab.id, &panel.panel, Some(&panel.cwd))?;
                    write_panel_name(tx, panel)?;
                }
            }
            delete_absent(tx, "tabs", snapshot.tabs.iter().map(|t| t.tab.id.to_string()))?;
            for agent in &snapshot.agents {
                write_agent_row(tx, agent)?;
            }
            delete_absent(tx, "agents", snapshot.agents.iter().map(|a| a.id.to_string()))?;
            write_active_workspace(tx, snapshot.active_workspace)
        })
    }

    pub fn load_snapshot(&self, default_cwd: &Path) -> Result<LoadedSnapshot, PersistenceError> {
        let restored = self.restore()?;
        let cwds = self.panel_cwds()?;
        let mut names = self.panel_names()?;
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

fn write_panel_name(tx: &rusqlite::Transaction<'_>, panel: &PanelSnapshot) -> Result<(), PersistenceError> {
    tx.execute(
        "UPDATE panels SET name = ?1 WHERE id = ?2",
        rusqlite::params![panel.name, panel.panel.id.to_string()],
    )?;
    Ok(())
}
