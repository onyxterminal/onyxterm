#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{AgentId, PanelId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ClosePane(PanelId),
    DeleteWorkspace(WorkspaceId),
    Quit { sessions: usize },
    StopAgent(AgentId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmCopy {
    pub title: String,
    pub body: String,
    pub action: &'static str,
}

pub fn close_pane_copy() -> ConfirmCopy {
    ConfirmCopy {
        title: "Terminate running process?".into(),
        body: "A command is still running in this pane. Closing it will end the process.".into(),
        action: "Close and terminate",
    }
}

pub fn delete_workspace_copy(name: &str, sessions: usize) -> ConfirmCopy {
    ConfirmCopy {
        title: format!("Delete workspace \u{201c}{name}\u{201d}?"),
        body: format!("Its tabs and layout are removed. {}", sessions_ending(sessions)),
        action: "Delete",
    }
}

pub fn quit_copy(sessions: usize) -> ConfirmCopy {
    ConfirmCopy { title: "Quit Onyxterm?".into(), body: sessions_ending(sessions), action: "Quit" }
}

pub fn stop_agent_copy(provider: &str, grace_secs: i64) -> ConfirmCopy {
    ConfirmCopy {
        title: format!("Stop {provider} agent?"),
        body: format!(
            "Onyxterm sends Ctrl-C and waits for the process to exit. \
             If it is still running after {grace_secs}s you can force-kill it from Agents."
        ),
        action: "Stop",
    }
}

pub fn sessions_ending(count: usize) -> String {
    match count {
        1 => "1 session will end.".to_string(),
        n => format!("{n} sessions will end."),
    }
}
