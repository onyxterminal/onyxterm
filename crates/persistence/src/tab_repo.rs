use crate::codec::{
    decode_layout, encode_layout, panel_kind_from_str, panel_kind_to_str, parse_uuid_id,
    path_to_string_lossy, resource_ref_columns, resource_ref_from,
};
use crate::error::PersistenceError;
use crate::sqlite::SqlitePool;
use domain::{Panel, PanelId, Tab, TabId, WorkspaceId};
use std::path::Path;

pub trait TabRepo {
    fn list_for(&self, workspace: WorkspaceId) -> Result<Vec<Tab>, PersistenceError>;
    fn get(&self, workspace: WorkspaceId, tab: TabId) -> Result<Option<Tab>, PersistenceError>;
    fn list_panels(&self, workspace: WorkspaceId, tab: TabId) -> Result<Vec<Panel>, PersistenceError>;
    fn upsert(
        &self,
        workspace: WorkspaceId,
        tab: Tab,
        panels: &[Panel],
    ) -> Result<(), PersistenceError>;
    fn delete(&self, workspace: WorkspaceId, tab: TabId) -> Result<(), PersistenceError>;
}

pub struct SqliteTabRepo<'a> {
    pub(crate) pool: &'a SqlitePool,
}

impl<'a> TabRepo for SqliteTabRepo<'a> {
    fn list_for(&self, workspace: WorkspaceId) -> Result<Vec<Tab>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workspace_id, title, focused_panel \
                 FROM tabs WHERE workspace_id = ?1",
            )?;
            let rows = stmt.query_map([workspace.to_string()], row_to_tab_partial)?;
            let mut out = Vec::new();
            for row in rows {
                let tab_partial = row?;
                let layout = read_layout(conn, tab_partial.id)?;
                out.push(Tab {
                    id: tab_partial.id,
                    workspace_id: tab_partial.workspace_id,
                    title: tab_partial.title,
                    focused_panel: tab_partial.focused_panel,
                    layout,
                });
            }
            Ok(out)
        })
    }

    fn get(
        &self,
        workspace: WorkspaceId,
        tab: TabId,
    ) -> Result<Option<Tab>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workspace_id, title, focused_panel \
                 FROM tabs WHERE id = ?1 AND workspace_id = ?2",
            )?;
            let mut rows = stmt.query(rusqlite::params![tab.to_string(), workspace.to_string()])?;
            if let Some(row) = rows.next()? {
                let tab_partial = row_to_tab_partial(row)?;
                let layout = read_layout(conn, tab_partial.id)?;
                Ok(Some(Tab {
                    id: tab_partial.id,
                    workspace_id: tab_partial.workspace_id,
                    title: tab_partial.title,
                    focused_panel: tab_partial.focused_panel,
                    layout,
                }))
            } else {
                Ok(None)
            }
        })
    }

    fn list_panels(
        &self,
        _workspace: WorkspaceId,
        tab: TabId,
    ) -> Result<Vec<Panel>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, title, resource_kind, resource_id \
                 FROM panels WHERE tab_id = ?1",
            )?;
            let rows = stmt.query_map([tab.to_string()], row_to_panel)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    fn upsert(
        &self,
        workspace: WorkspaceId,
        tab: Tab,
        panels: &[Panel],
    ) -> Result<(), PersistenceError> {
        if tab.workspace_id != workspace {
            return Err(PersistenceError::MigrationFailed(format!(
                "tab workspace mismatch: tab has {} expected {}",
                tab.workspace_id, workspace
            )));
        }
        self.pool.transaction(|tx| {
            write_tab_row(tx, &tab)?;
            for panel in panels {
                write_panel_row(tx, tab.id, panel, None)?;
            }
            Ok(())
        })
    }

    fn delete(&self, _workspace: WorkspaceId, tab: TabId) -> Result<(), PersistenceError> {
        self.pool.with_conn(|conn| {
            conn.execute("DELETE FROM tabs WHERE id = ?1", [tab.to_string()])?;
            Ok(())
        })
    }
}

pub(crate) fn write_tab_row(
    tx: &rusqlite::Transaction<'_>,
    tab: &Tab,
) -> Result<(), PersistenceError> {
    tx.execute(
        "INSERT INTO tabs (id, workspace_id, title, focused_panel) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET \
           workspace_id = excluded.workspace_id, \
           title = excluded.title, \
           focused_panel = excluded.focused_panel",
        rusqlite::params![
            tab.id.to_string(),
            tab.workspace_id.to_string(),
            tab.title,
            tab.focused_panel.map(|p| p.to_string()),
        ],
    )?;
    tx.execute(
        "INSERT INTO layouts (tab_id, tree_json) VALUES (?1, ?2) \
         ON CONFLICT(tab_id) DO UPDATE SET tree_json = excluded.tree_json",
        rusqlite::params![tab.id.to_string(), encode_layout(&tab.layout)?],
    )?;
    tx.execute("DELETE FROM panels WHERE tab_id = ?1", [tab.id.to_string()])?;
    Ok(())
}

pub(crate) fn write_panel_row(
    tx: &rusqlite::Transaction<'_>,
    tab: TabId,
    panel: &Panel,
    cwd: Option<&Path>,
) -> Result<(), PersistenceError> {
    let (resource_kind, resource_id) = resource_ref_columns(panel.resource);
    tx.execute(
        "INSERT INTO panels (id, tab_id, kind, title, resource_kind, resource_id, cwd) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(id) DO UPDATE SET \
           tab_id = excluded.tab_id, \
           kind = excluded.kind, \
           title = excluded.title, \
           resource_kind = excluded.resource_kind, \
           resource_id = excluded.resource_id, \
           cwd = excluded.cwd",
        rusqlite::params![
            panel.id.to_string(),
            tab.to_string(),
            panel_kind_to_str(panel.kind),
            panel.title,
            resource_kind,
            resource_id,
            cwd.map(path_to_string_lossy),
        ],
    )?;
    Ok(())
}

struct TabPartial {
    id: TabId,
    workspace_id: WorkspaceId,
    title: Option<String>,
    focused_panel: Option<PanelId>,
}

fn row_to_tab_partial(row: &rusqlite::Row<'_>) -> Result<TabPartial, rusqlite::Error> {
    let id_raw: String = row.get(0)?;
    let ws_raw: String = row.get(1)?;
    let title: Option<String> = row.get(2)?;
    let focused_raw: Option<String> = row.get(3)?;
    let id: TabId = parse_uuid_id(&id_raw).map_err(rusqlite::Error::from)?;
    let workspace_id: WorkspaceId = parse_uuid_id(&ws_raw).map_err(rusqlite::Error::from)?;
    let focused_panel = match focused_raw {
        Some(raw) => Some(parse_uuid_id::<PanelId>(&raw).map_err(rusqlite::Error::from)?),
        None => None,
    };
    Ok(TabPartial {
        id,
        workspace_id,
        title,
        focused_panel,
    })
}

fn row_to_panel(row: &rusqlite::Row<'_>) -> Result<Panel, rusqlite::Error> {
    let id_raw: String = row.get(0)?;
    let kind_raw: String = row.get(1)?;
    let title: String = row.get(2)?;
    let resource_kind: String = row.get(3)?;
    let resource_id: String = row.get(4)?;
    let id: PanelId = parse_uuid_id(&id_raw).map_err(rusqlite::Error::from)?;
    let kind = panel_kind_from_str(&kind_raw).map_err(rusqlite::Error::from)?;
    let resource = resource_ref_from(&resource_kind, &resource_id).map_err(rusqlite::Error::from)?;
    Ok(Panel {
        id,
        kind,
        title,
        resource,
    })
}

fn read_layout(
    conn: &rusqlite::Connection,
    tab_id: TabId,
) -> Result<domain::LayoutNode, PersistenceError> {
    let raw: Option<String> = match conn.query_row(
        "SELECT tree_json FROM layouts WHERE tab_id = ?1",
        [tab_id.to_string()],
        |row| row.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.into()),
    };
    match raw {
        Some(s) => decode_layout(&s),
        None => Err(PersistenceError::NotFound {
            entity: "layout",
            id: tab_id.to_string(),
        }),
    }
}
