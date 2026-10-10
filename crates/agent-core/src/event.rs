#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{AgentId, AgentState, WorkspaceId};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Launched {
        id: AgentId,
        workspace: WorkspaceId,
        provider: String,
        state: AgentState,
    },
    StateChanged {
        id: AgentId,
        workspace: WorkspaceId,
        state: AgentState,
    },
    Exited {
        id: AgentId,
        workspace: WorkspaceId,
        code: Option<i32>,
    },
}

impl AgentEvent {
    pub fn id(&self) -> AgentId {
        match self {
            AgentEvent::Launched { id, .. }
            | AgentEvent::StateChanged { id, .. }
            | AgentEvent::Exited { id, .. } => *id,
        }
    }

    pub fn workspace(&self) -> WorkspaceId {
        match self {
            AgentEvent::Launched { workspace, .. }
            | AgentEvent::StateChanged { workspace, .. }
            | AgentEvent::Exited { workspace, .. } => *workspace,
        }
    }
}