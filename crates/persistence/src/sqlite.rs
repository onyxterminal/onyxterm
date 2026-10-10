use crate::error::PersistenceError;
use crate::migrations::{apply_migrations, CURRENT_VERSION};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

const PRAGMAS: &str = "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;";

pub(crate) struct Inner {
    pub conn: Mutex<Connection>,
}

#[derive(Clone)]
pub struct SqlitePool {
    inner: Arc<Inner>,
}

impl SqlitePool {
    pub fn open(path: &Path) -> Result<Self, PersistenceError> {
        let mut conn = Connection::open(path)
            .map_err(|e| PersistenceError::OpenFailed(e.to_string()))?;
        conn.execute_batch(PRAGMAS)
            .map_err(|e| PersistenceError::OpenFailed(e.to_string()))?;
        apply_migrations(&mut conn, CURRENT_VERSION)?;
        Ok(Self {
            inner: Arc::new(Inner {
                conn: Mutex::new(conn),
            }),
        })
    }

    pub fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> R) -> R {
        let mut guard = self.inner.conn.lock();
        f(&mut guard)
    }

    pub fn transaction<R>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<R, PersistenceError>,
    ) -> Result<R, PersistenceError> {
        let mut guard = self.inner.conn.lock();
        let tx = guard.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }
}
