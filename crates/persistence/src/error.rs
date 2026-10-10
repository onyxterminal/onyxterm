use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("open failed: {0}")]
    OpenFailed(String),
    #[error("migration failed: {0}")]
    MigrationFailed(String),
    #[error("downgrade forbidden: stored version {from} > target {to}")]
    DowngradeForbidden { from: i64, to: i64 },
    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: String },
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl From<serde_json::Error> for PersistenceError {
    fn from(err: serde_json::Error) -> Self {
        PersistenceError::SerializationFailed(err.to_string())
    }
}

impl From<PersistenceError> for rusqlite::Error {
    fn from(value: PersistenceError) -> Self {
        rusqlite::Error::ToSqlConversionFailure(Box::new(value))
    }
}
