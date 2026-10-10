#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{PanelId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ClosePane(PanelId),
    DeleteWorkspace(WorkspaceId),
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

pub fn sessions_ending(count: usize) -> String {
    match count {
        1 => "1 session will end.".to_string(),
        n => format!("{n} sessions will end."),
    }
}
