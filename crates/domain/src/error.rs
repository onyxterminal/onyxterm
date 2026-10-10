use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("workspace {0} not found")]
    WorkspaceNotFound(String),
    #[error("tab {0} not found")]
    TabNotFound(String),
    #[error("panel {0} not found")]
    PanelNotFound(String),
    #[error("agent {0} not found")]
    AgentNotFound(String),
    #[error("layout invalid: {0}")]
    InvalidLayout(String),
    #[error("ratio {0} outside (0.0, 1.0)")]
    InvalidRatio(f32),
    #[error("cycle detected in layout tree")]
    LayoutCycle,
    #[error("tab already present in workspace")]
    DuplicateTab,
    #[error("active tab not in workspace tab order")]
    InconsistentActiveTab,
    #[error("invalid agent state transition from {from:?} to {to:?}")]
    InvalidAgentTransition { from: String, to: String },
}
