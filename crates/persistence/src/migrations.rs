use crate::error::PersistenceError;
use crate::schema::SCHEMA_SQL;
use rusqlite::Connection;

pub const CURRENT_VERSION: i64 = 2;

const PANEL_CWD_SQL: &str = "ALTER TABLE panels ADD COLUMN cwd TEXT;";

const MIGRATIONS: &[(i64, &str)] = &[(1, SCHEMA_SQL), (2, PANEL_CWD_SQL)];

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
    let applied = existing.unwrap_or(0);
    for (version, sql) in MIGRATIONS {
        if *version <= applied || *version > target {
            continue;
        }
        tx.execute_batch(sql)
            .map_err(|e| PersistenceError::MigrationFailed(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, &now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

