use crate::id::TerminalSessionId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Created,
    Running,
    Exited(i32),
    Failed(String),
    Stopping,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSession {
    pub id: TerminalSessionId,
    pub cwd: PathBuf,
    pub shell: PathBuf,
    pub process_status: ProcessStatus,
}

impl TerminalSession {
    pub fn new(id: TerminalSessionId, cwd: PathBuf, shell: PathBuf) -> Self {
        Self {
            id,
            cwd,
            shell,
            process_status: ProcessStatus::Created,
        }
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.process_status, ProcessStatus::Running | ProcessStatus::Created)
    }
}
