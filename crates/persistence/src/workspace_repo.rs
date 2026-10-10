use crate::codec::{
    decode_json, encode_tab_order, parse_uuid_id, path_to_string_lossy,
};
use crate::error::PersistenceError;
use crate::sqlite::SqlitePool;
use domain::{Workspace, WorkspaceId};

pub trait WorkspaceRepo {
    fn list(&self) -> Result<Vec<Workspace>, PersistenceError>;
    fn get(&self, id: WorkspaceId) -> Result<Option<Workspace>, PersistenceError>;
    fn insert(&self, workspace: Workspace) -> Result<(), PersistenceError>;
    fn update(&self, workspace: Workspace) -> Result<(), PersistenceError>;
    fn delete(&self, id: WorkspaceId) -> Result<(), PersistenceError>;
}

pub struct SqliteWorkspaceRepo<'a> {
    pub(crate) pool: &'a SqlitePool,
}

impl<'a> WorkspaceRepo for SqliteWorkspaceRepo<'a> {
    fn list(&self) -> Result<Vec<Workspace>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, root, tab_order, active_tab, archived \
                 FROM workspaces",
            )?;
            let rows = stmt.query_map([], row_to_workspace)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    fn get(&self, id: WorkspaceId) -> Result<Option<Workspace>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, root, tab_order, active_tab, archived \
                 FROM workspaces WHERE id = ?1",
            )?;
            let mut rows = stmt.query([id.to_string()])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row_to_workspace(row)?))
            } else {
                Ok(None)
            }
        })
    }

    fn insert(&self, workspace: Workspace) -> Result<(), PersistenceError> {
        self.pool.transaction(|tx| {
            write_workspace_row(tx, &workspace)?;
            Ok(())
        })
    }

    fn update(&self, workspace: Workspace) -> Result<(), PersistenceError> {
        self.pool.transaction(|tx| {
            tx.execute(
                "UPDATE workspaces \
                 SET name = ?2, root = ?3, tab_order = ?4, active_tab = ?5, archived = ?6 \
                 WHERE id = ?1",
                rusqlite::params![
                    workspace.id.to_string(),
                    workspace.name,
                    workspace.root.as_deref().map(path_to_string_lossy),
                    encode_tab_order(&workspace.tab_order)?,
                    workspace.active_tab.map(|t| t.to_string()),
                    workspace.archived as i64,
                ],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: WorkspaceId) -> Result<(), PersistenceError> {
        self.pool.with_conn(|conn| {
            conn.execute("DELETE FROM workspaces WHERE id = ?1", [id.to_string()])?;
            Ok(())
        })
    }
}

pub(crate) fn write_workspace_row(
    tx: &rusqlite::Transaction<'_>,
    workspace: &Workspace,
) -> Result<(), PersistenceError> {
    tx.execute(
        "INSERT INTO workspaces (id, name, root, tab_order, active_tab, archived) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(id) DO UPDATE SET \
           name = excluded.name, \
           root = excluded.root, \
           tab_order = excluded.tab_order, \
           active_tab = excluded.active_tab, \
           archived = excluded.archived",
        rusqlite::params![
            workspace.id.to_string(),
            workspace.name,
            workspace.root.as_deref().map(path_to_string_lossy),
            encode_tab_order(&workspace.tab_order)?,
            workspace.active_tab.map(|t| t.to_string()),
            workspace.archived as i64,
        ],
    )?;
    Ok(())
}

fn row_to_workspace(row: &rusqlite::Row<'_>) -> Result<Workspace, rusqlite::Error> {
    let id_raw: String = row.get(0)?;
    let name: String = row.get(1)?;
    let root_raw: Option<String> = row.get(2)?;
    let tab_order_raw: String = row.get(3)?;
    let active_tab_raw: Option<String> = row.get(4)?;
    let archived: i64 = row.get(5)?;
    let id: WorkspaceId = parse_uuid_id(&id_raw).map_err(rusqlite::Error::from)?;
    let tab_order = decode_json(&tab_order_raw).map_err(rusqlite::Error::from)?;
    let active_tab = match active_tab_raw {
        Some(raw) => Some(parse_uuid_id::<domain::TabId>(&raw).map_err(rusqlite::Error::from)?),
        None => None,
    };
    let root = root_raw.map(std::path::PathBuf::from);
    Ok(Workspace {
        id,
        name,
        root,
        tab_order,
        active_tab,
        archived: archived != 0,
    })
}
