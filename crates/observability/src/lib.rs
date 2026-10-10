#![deny(warnings)]
#![forbid(unsafe_code)]

use std::sync::Once;
use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

pub fn init_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,domain=debug,workspace_service=debug"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init();
    });
}

pub fn redact(s: &str) -> String {
    s.to_string()
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
