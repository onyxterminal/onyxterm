use crate::id::{AgentId, PanelId, TerminalSessionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    Terminal,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceRef {
    Terminal(TerminalSessionId),
    Agent(AgentId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: PanelId,
    pub kind: PanelKind,
    pub title: String,
    pub resource: ResourceRef,
}

impl Panel {
    pub fn new_terminal(id: PanelId, title: String, session: TerminalSessionId) -> Self {
        Self {
            id,
            kind: PanelKind::Terminal,
            title,
            resource: ResourceRef::Terminal(session),
        }
    }

    pub fn new_agent(id: PanelId, title: String, agent: AgentId) -> Self {
        Self {
            id,
            kind: PanelKind::Agent,
            title,
            resource: ResourceRef::Agent(agent),
        }
    }
}
