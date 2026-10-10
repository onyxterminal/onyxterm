#![deny(warnings)]
#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("adapter not found: {0}")]
    AdapterNotFound(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("invalid profile: {0}")]
    InvalidProfile(String),
    #[error("cwd {0} is not under any allowed directory")]
    CwdNotAllowed(String),
    #[error("cwd contains parent traversal")]
    CwdTraversal,
    #[error("env key {0} not in whitelist")]
    EnvKeyNotAllowed(String),
    #[error("template placeholder {{{0}}} unresolved")]
    TemplatePlaceholder(String),
    #[error("no executable resolved for adapter {0}")]
    NoExecutable(String),
    #[error("terminal error: {0}")]
    Terminal(String),
    #[error("stop timed out after grace period")]
    StopTimeout,
    #[error("agent state transition refused: {0}")]
    InvalidTransition(String),
}