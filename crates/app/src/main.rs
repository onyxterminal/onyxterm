#![deny(warnings)]
#![forbid(unsafe_code)]

use platform::{FileLock, SingleInstanceLock};
use std::path::PathBuf;
use std::sync::Arc;
use terminal_core::{InMemoryTerminalService, NativePty, ShellIntegration};
use workspace_service::{StoreError, WorkspaceStore};

const EXIT_ALREADY_RUNNING: i32 = 2;
const EXIT_UI_FAILED: i32 = 1;
const EXIT_STORE_FAILED: i32 = 3;
const PROFILE: &str = "default";
const SHELL_INTEGRATION_DIR: &str = "shell-integration";

fn main() {
    observability::start_clock();
    observability::init_tracing();
    let lock_path = std::env::var("ONYXTERM_LOCK_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| platform::default_lock_path(PROFILE));
    let lock = FileLock::new(lock_path);
    let _guard = match lock.acquire() {
        Ok(guard) => guard,
        Err(err) => {
            tracing::error!(?err, "another onyxterm instance is already running");
            std::process::exit(EXIT_ALREADY_RUNNING);
        }
    };
    tracing::info!(version = ui::version(), "starting onyxterm");
    let startup = match observability::timed("store open", open_startup) {
        Ok(startup) => startup,
        Err(err) => {
            tracing::error!(%err, "failed to open workspace store");
            std::process::exit(EXIT_STORE_FAILED);
        }
    };
    if let Err(err) = ui::run_app(startup) {
        tracing::error!(?err, "onyxterm ui failed");
        std::process::exit(EXIT_UI_FAILED);
    }
}

fn open_startup() -> Result<ui::Startup, StoreError> {
    let database = platform::database_path(PROFILE)?;
    tracing::info!(path = %database.display(), "opening workspace store");
    let (store, restored) = WorkspaceStore::open(&database)?;
    for warning in &restored.warnings {
        tracing::warn!(?warning, "restore warning");
    }
    Ok(ui::Startup {
        terminals: Arc::new(terminal_service()),
        store,
        restored,
        home: platform::home_dir()?,
    })
}

fn terminal_service() -> InMemoryTerminalService {
    let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
    let installed = platform::profile_dir(PROFILE)
        .map_err(|err| err.to_string())
        .and_then(|dir| ShellIntegration::install(&dir.join(SHELL_INTEGRATION_DIR)).map_err(|err| err.to_string()));
    match installed {
        Ok(integration) => service.with_shell_integration(integration),
        Err(err) => {
            tracing::warn!(%err, "shell integration unavailable");
            service
        }
    }
}
