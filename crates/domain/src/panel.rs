use crate::id::{AgentId, DocumentId, PanelId, TerminalSessionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    Terminal,
    Agent,
    Editor,
    FileTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceRef {
    Terminal(TerminalSessionId),
    Agent(AgentId),
    Document(DocumentId),
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

    pub fn new_editor(id: PanelId, title: String) -> Self {
        Self::new_document(id, PanelKind::Editor, title)
    }

    pub fn new_file_tree(id: PanelId, title: String) -> Self {
        Self::new_document(id, PanelKind::FileTree, title)
    }

    pub fn is_document(&self) -> bool {
        matches!(self.kind, PanelKind::Editor | PanelKind::FileTree)
    }

    fn new_document(id: PanelId, kind: PanelKind, title: String) -> Self {
        Self {
            id,
            kind,
            title,
            resource: ResourceRef::Document(DocumentId::new()),
        }
    }
}
