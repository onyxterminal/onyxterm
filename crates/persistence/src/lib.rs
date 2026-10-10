#![deny(warnings)]
#![forbid(unsafe_code)]

mod agent_repo;
mod app_settings_repo;
mod codec;
mod error;
mod migrations;
mod repository;
mod restore;
mod schema;
mod sqlite;
mod tab_repo;
mod workspace_repo;

pub use error::PersistenceError;
pub use repository::Repository;
pub use restore::{RestoreSnapshot, RestoreWarning};
pub use schema::SCHEMA_SQL;
pub use sqlite::SqlitePool;

pub use agent_repo::{AgentRepo, SqliteAgentRepo};
pub use app_settings_repo::{AppSettingsRepo, SqliteAppSettingsRepo};
pub use tab_repo::{SqliteTabRepo, TabRepo};
pub use workspace_repo::{SqliteWorkspaceRepo, WorkspaceRepo};

pub use migrations::CURRENT_VERSION;
