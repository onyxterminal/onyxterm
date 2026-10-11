use crate::error::PersistenceError;
use crate::schema::SCHEMA_SQL;
use rusqlite::Connection;

pub const CURRENT_VERSION: i64 = 2;

pub(crate) fn apply_migrations(conn: &mut Connection, target: i64) -> Result<(), PersistenceError> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;
    let existing: Option<i64> = {
        let mut stmt = tx.prepare("SELECT MAX(version) FROM schema_migrations")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            row.get::<_, Option<i64>>(0)?
        } else {
            None
        }
    };
    if let Some(current) = existing {
        if current > target {
            return Err(PersistenceError::DowngradeForbidden {
                from: current,
                to: target,
            });
        }
    }
    let needs_v1 = match existing {
        Some(v) => v < 1,
        None => true,
    };
    if needs_v1 {
        tx.execute_batch(SCHEMA_SQL)
            .map_err(|e| PersistenceError::MigrationFailed(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![1_i64, &now],
        )?;
    }
    if existing.unwrap_or(0) < 2 {
        tx.execute_batch(
            "ALTER TABLE panels ADD COLUMN cwd TEXT; \
             ALTER TABLE panels ADD COLUMN name TEXT;",
        )
        .map_err(|e| PersistenceError::MigrationFailed(e.to_string()))?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![2_i64, chrono::Utc::now().to_rfc3339()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

