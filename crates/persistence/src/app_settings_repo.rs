use crate::error::PersistenceError;
use crate::sqlite::SqlitePool;

pub trait AppSettingsRepo {
    fn get(&self, key: &str) -> Result<Option<String>, PersistenceError>;
    fn set(&self, key: &str, value: &str) -> Result<(), PersistenceError>;
    fn all(&self) -> Result<Vec<(String, String)>, PersistenceError>;
}

pub struct SqliteAppSettingsRepo<'a> {
    pub(crate) pool: &'a SqlitePool,
}

impl<'a> AppSettingsRepo for SqliteAppSettingsRepo<'a> {
    fn get(&self, key: &str) -> Result<Option<String>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
            let mut rows = stmt.query([key])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
    }

    fn set(&self, key: &str, value: &str) -> Result<(), PersistenceError> {
        self.pool.transaction(|tx| {
            tx.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )?;
            Ok(())
        })
    }

    fn all(&self) -> Result<Vec<(String, String)>, PersistenceError> {
        self.pool.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }
}
