#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::error::AgentError;
use crate::profile::AdapterProfile;
use domain::AgentState;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

impl CommandSpec {
    pub fn new(
        executable: PathBuf,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        cwd: PathBuf,
    ) -> Self {
        Self {
            executable,
            args,
            env,
            cwd,
        }
    }
}

pub trait Adapter: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn resolve_executable(&self) -> Option<PathBuf>;
    fn build_command(
        &self,
        profile: &AdapterProfile,
        cwd: PathBuf,
    ) -> Result<CommandSpec, AgentError>;
    fn classify_exit(&self, code: Option<i32>) -> AgentState {
        match code {
            Some(0) => AgentState::Exited,
            Some(_) => AgentState::Failed,
            None => AgentState::Failed,
        }
    }
}