#![deny(warnings)]
#![forbid(unsafe_code)]

mod adapter;
mod error;
mod event;
mod profile;
mod redact;
mod registry;
mod service;

pub use adapter::{Adapter, CommandSpec};
pub use error::AgentError;
pub use event::AgentEvent;
pub use profile::AdapterProfile;
pub use redact::{is_sensitive, mask_value, redact_env, redacted};
pub use registry::{AdapterRegistry, GenericCliAdapter, PLACEHOLDER_CWD, PLACEHOLDER_PROMPT};
pub use service::{
    AgentService, InMemoryAgentService, DEFAULT_GRACE_PERIOD, DEFAULT_POLL_INTERVAL,
};
