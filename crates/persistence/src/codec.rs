use crate::error::PersistenceError;
use domain::{AgentId, AgentState, LayoutNode, PanelKind, ResourceRef, TabId};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub fn encode_json<T: Serialize>(value: &T) -> Result<String, PersistenceError> {
    serde_json::to_string(value).map_err(PersistenceError::from)
}

pub fn decode_json<T: DeserializeOwned>(raw: &str) -> Result<T, PersistenceError> {
    serde_json::from_str(raw).map_err(PersistenceError::from)
}

pub fn agent_state_to_str(state: AgentState) -> &'static str {
    match state {
        AgentState::Created => "Created",
        AgentState::Starting => "Starting",
        AgentState::Running => "Running",
        AgentState::Exited => "Exited",
        AgentState::Failed => "Failed",
        AgentState::Stopping => "Stopping",
        AgentState::Unknown => "Unknown",
    }
}

pub fn agent_state_from_str(value: &str) -> Result<AgentState, PersistenceError> {
    match value {
        "Created" => Ok(AgentState::Created),
        "Starting" => Ok(AgentState::Starting),
        "Running" => Ok(AgentState::Running),
        "Exited" => Ok(AgentState::Exited),
        "Failed" => Ok(AgentState::Failed),
        "Stopping" => Ok(AgentState::Stopping),
        "Unknown" => Ok(AgentState::Unknown),
        other => Err(PersistenceError::MigrationFailed(format!(
            "unknown agent state {other}"
        ))),
    }
}

pub fn panel_kind_to_str(kind: PanelKind) -> &'static str {
    match kind {
        PanelKind::Terminal => "Terminal",
        PanelKind::Agent => "Agent",
    }
}

pub fn panel_kind_from_str(value: &str) -> Result<PanelKind, PersistenceError> {
    match value {
        "Terminal" => Ok(PanelKind::Terminal),
        "Agent" => Ok(PanelKind::Agent),
        other => Err(PersistenceError::MigrationFailed(format!(
            "unknown panel kind {other}"
        ))),
    }
}

pub fn resource_ref_columns(resource: ResourceRef) -> (&'static str, String) {
    match resource {
        ResourceRef::Terminal(session) => ("Terminal", session.to_string()),
        ResourceRef::Agent(agent) => ("Agent", agent.to_string()),
    }
}

pub fn resource_ref_from(kind: &str, id: &str) -> Result<ResourceRef, PersistenceError> {
    match kind {
        "Terminal" => {
            let session: domain::TerminalSessionId = parse_uuid_id(id)?;
            Ok(ResourceRef::Terminal(session))
        }
        "Agent" => {
            let agent: AgentId = parse_uuid_id(id)?;
            Ok(ResourceRef::Agent(agent))
        }
        other => Err(PersistenceError::MigrationFailed(format!(
            "unknown resource kind {other}"
        ))),
    }
}

pub fn encode_tab_order(tab_order: &[TabId]) -> Result<String, PersistenceError> {
    serde_json::to_string(tab_order).map_err(PersistenceError::from)
}

pub fn encode_layout(layout: &LayoutNode) -> Result<String, PersistenceError> {
    encode_json(layout)
}

pub fn decode_layout(raw: &str) -> Result<LayoutNode, PersistenceError> {
    decode_json(raw)
}

pub fn path_to_string_lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn parse_uuid_id<T: DeserializeOwned>(raw: &str) -> Result<T, PersistenceError> {
    let uuid_part = raw.rsplit_once('_').map(|(_, suffix)| suffix).unwrap_or(raw);
    let value = serde_json::Value::String(uuid_part.to_string());
    serde_json::from_value(value).map_err(PersistenceError::from)
}

pub trait OptionalResult<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalResult<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
