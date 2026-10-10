use crate::codec::{agent_state_from_str, agent_state_to_str, parse_uuid_id, path_to_string_lossy};
use crate::error::PersistenceError;
use crate::sqlite::SqlitePool;
use chrono::{DateTime, Utc};
use domain::{Agent, AgentId, TerminalSessionId, WorkspaceId};
use std::path::PathBuf;
use std::str::FromStr;

pub trait AgentRepo {
    fn list(&self) -> Result<Vec<Agent>, PersistenceError>;
    fn get(&self, id: AgentId) -> Result<Option<Agent>, PersistenceError>;
    fn upsert(&self, agent: Agent) -> Result<(), PersistenceError>;
    fn delete(&self, id: AgentId) -> Result<(), PersistenceError>;
}

pub struct SqliteAgentRepo<'a> {
    pub(crate) pool: &'a SqlitePool,
}

impl<'a> AgentRepo for SqliteAgentRepo<'a> {
    fn list(&self) -> Result<Vec<Agent>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, provider, workspace_id, terminal_session_id, cwd, state, started_at, ended_at \
                 FROM agents",
            )?;
            let rows = stmt.query_map([], row_to_agent)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    fn get(&self, id: AgentId) -> Result<Option<Agent>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, provider, workspace_id, terminal_session_id, cwd, state, started_at, ended_at \
                 FROM agents WHERE id = ?1",
            )?;
            let mut rows = stmt.query([id.to_string()])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row_to_agent(row)?))
            } else {
                Ok(None)
            }
        })
    }

    fn upsert(&self, agent: Agent) -> Result<(), PersistenceError> {
        self.pool.transaction(|tx| write_agent_row(tx, &agent))
    }

    fn delete(&self, id: AgentId) -> Result<(), PersistenceError> {
        self.pool.with_conn(|conn| {
            conn.execute("DELETE FROM agents WHERE id = ?1", [id.to_string()])?;
            Ok(())
        })
    }
}

pub(crate) fn write_agent_row(
    tx: &rusqlite::Transaction<'_>,
    agent: &Agent,
) -> Result<(), PersistenceError> {
    tx.execute(
        "INSERT INTO agents \
          (id, provider, workspace_id, terminal_session_id, cwd, state, started_at, ended_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET \
           provider = excluded.provider, \
           workspace_id = excluded.workspace_id, \
           terminal_session_id = excluded.terminal_session_id, \
           cwd = excluded.cwd, \
           state = excluded.state, \
           started_at = excluded.started_at, \
           ended_at = excluded.ended_at",
        rusqlite::params![
            agent.id.to_string(),
            agent.provider,
            agent.workspace.to_string(),
            agent.terminal_session.to_string(),
            path_to_string_lossy(agent.cwd.as_path()),
            agent_state_to_str(agent.state),
            agent.started_at.map(|d| d.to_rfc3339()),
            agent.ended_at.map(|d| d.to_rfc3339()),
        ],
    )?;
    Ok(())
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> Result<Agent, rusqlite::Error> {
    let id_raw: String = row.get(0)?;
    let provider: String = row.get(1)?;
    let workspace_raw: String = row.get(2)?;
    let session_raw: String = row.get(3)?;
    let cwd_raw: String = row.get(4)?;
    let state_raw: String = row.get(5)?;
    let started_raw: Option<String> = row.get(6)?;
    let ended_raw: Option<String> = row.get(7)?;
    let id: AgentId = parse_uuid_id(&id_raw).map_err(rusqlite::Error::from)?;
    let workspace: WorkspaceId = parse_uuid_id(&workspace_raw).map_err(rusqlite::Error::from)?;
    let terminal_session: TerminalSessionId = parse_uuid_id(&session_raw).map_err(rusqlite::Error::from)?;
    let cwd = PathBuf::from(cwd_raw);
    let state = agent_state_from_str(&state_raw).map_err(rusqlite::Error::from)?;
    let started_at = match started_raw {
        Some(s) => Some(parse_datetime(&s).map_err(rusqlite::Error::from)?),
        None => None,
    };
    let ended_at = match ended_raw {
        Some(s) => Some(parse_datetime(&s).map_err(rusqlite::Error::from)?),
        None => None,
    };
    Ok(Agent {
        id,
        provider,
        workspace,
        terminal_session,
        cwd,
        state,
        started_at,
        ended_at,
    })
}

fn parse_datetime(raw: &str) -> Result<DateTime<Utc>, PersistenceError> {
    DateTime::<Utc>::from_str(raw)
        .map_err(|e| PersistenceError::MigrationFailed(format!("bad datetime {raw}: {e}")))
}
