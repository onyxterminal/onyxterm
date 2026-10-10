use crate::error::DomainError;
use crate::id::{AgentId, TerminalSessionId, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    Created,
    Starting,
    Running,
    Exited,
    Failed,
    Stopping,
    Unknown,
}

impl AgentState {
    pub fn is_alive(self) -> bool {
        matches!(
            self,
            AgentState::Created | AgentState::Starting | AgentState::Running | AgentState::Stopping
        )
    }

    pub fn can_transition_to(self, to: AgentState) -> bool {
        use AgentState::*;
        if self == to {
            return true;
        }
        matches!(
            (self, to),
            (Created, Starting)
                | (Created, Running)
                | (Starting, Running)
                | (Starting, Failed)
                | (Running, Stopping)
                | (Running, Exited)
                | (Running, Failed)
                | (Stopping, Exited)
                | (Stopping, Failed)
                | (_, Unknown)
                | (Unknown, Starting)
                | (Unknown, Running)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub provider: String,
    pub workspace: WorkspaceId,
    pub terminal_session: TerminalSessionId,
    pub cwd: PathBuf,
    pub state: AgentState,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Agent {
    pub fn new(
        id: AgentId,
        provider: String,
        workspace: WorkspaceId,
        terminal_session: TerminalSessionId,
        cwd: PathBuf,
    ) -> Self {
        Self {
            id,
            provider,
            workspace,
            terminal_session,
            cwd,
            state: AgentState::Created,
            started_at: None,
            ended_at: None,
        }
    }

    pub fn transition(self, next: AgentState) -> Result<Self, DomainError> {
        if !self.state.can_transition_to(next) {
            return Err(DomainError::InvalidAgentTransition {
                from: format!("{:?}", self.state),
                to: format!("{:?}", next),
            });
        }
        let mut updated = self;
        updated.state = next;
        match next {
            AgentState::Running | AgentState::Starting
                if updated.started_at.is_none() =>
            {
                updated.started_at = Some(Utc::now());
            }
            AgentState::Exited | AgentState::Failed
                if updated.ended_at.is_none() =>
            {
                updated.ended_at = Some(Utc::now());
            }
            _ => {}
        }
        Ok(updated)
    }
}
