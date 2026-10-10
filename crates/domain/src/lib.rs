#![deny(warnings)]
#![forbid(unsafe_code)]

mod agent;
mod error;
mod id;
mod layout;
mod panel;
mod reducer;
mod terminal;
mod workspace;

pub use agent::{Agent, AgentState};
pub use error::DomainError;
pub use id::{AgentId, PanelId, TabId, TerminalSessionId, WorkspaceId};
pub use layout::{axis_helpers, Axis, LayoutNode, SplitRatio};
pub use panel::{Panel, PanelKind, ResourceRef};
pub use reducer::{WorkspaceReducer, WorkspaceReducerEvent};
pub use terminal::{ProcessStatus, TerminalSession};
pub use workspace::{Tab, Workspace};
