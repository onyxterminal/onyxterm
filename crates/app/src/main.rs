#![deny(warnings)]
#![forbid(unsafe_code)]

use platform::{FileLock, SingleInstanceLock};
use std::path::PathBuf;
use std::sync::Arc;
use terminal_core::{InMemoryTerminalService, NativePty};

const EXIT_ALREADY_RUNNING: i32 = 2;
const EXIT_UI_FAILED: i32 = 1;

fn main() {
    observability::init_tracing();
    let lock_path = std::env::var("ONYXTERM_LOCK_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| platform::default_lock_path("default"));
    let lock = FileLock::new(lock_path);
    let _guard = match lock.acquire() {
        Ok(guard) => guard,
        Err(err) => {
            tracing::error!(?err, "another onyxterm instance is already running");
            std::process::exit(EXIT_ALREADY_RUNNING);
        }
    };
    tracing::info!(version = ui::version(), "starting onyxterm");
    let terminals = Arc::new(InMemoryTerminalService::new(Box::new(NativePty::new())));
    if let Err(err) = ui::run_app(terminals) {
        tracing::error!(?err, "onyxterm ui failed");
        std::process::exit(EXIT_UI_FAILED);
    }
}
