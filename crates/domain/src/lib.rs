#![deny(warnings)]
#![forbid(unsafe_code)]

mod agent;
mod error;
mod id;
mod layout;
mod panel;
mod reducer;
mod ssh;
mod terminal;
mod workspace;

pub use agent::{Agent, AgentState};
pub use error::DomainError;
pub use id::{AgentId, DocumentId, PanelId, TabId, TerminalSessionId, WorkspaceId};
pub use layout::{axis_helpers, Axis, Direction, DropZone, LayoutNode, SplitRatio, MAX_SPLIT_RATIO, MIN_SPLIT_RATIO};
pub use panel::{Panel, PanelKind, ResourceRef};
pub use reducer::{WorkspaceReducer, WorkspaceReducerEvent};
pub use ssh::{parse_ssh_config, read_ssh_hosts, SshHost};
pub use terminal::{ProcessStatus, TerminalSession};
pub use workspace::{Tab, Workspace};
