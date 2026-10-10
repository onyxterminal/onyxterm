#![deny(warnings)]
#![forbid(unsafe_code)]

use platform::{FileLock, SingleInstanceLock};
use std::path::PathBuf;

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
            std::process::exit(2);
        }
    };
    println!("Onyxterm {}", ui::version());
}
