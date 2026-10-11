#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{PanelId, TabId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ClosePane(PanelId),
    CloseTab(TabId),
    DeleteWorkspace(WorkspaceId),
    Quit { sessions: usize },
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

pub fn close_tab_copy() -> ConfirmCopy {
    ConfirmCopy {
        title: "Terminate running processes?".into(),
        body: "A command is still running in this tab. Closing it will end every process in it.".into(),
        action: "Close tab",
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

pub fn sessions_ending(count: usize) -> String {
    match count {
        1 => "1 session will end.".to_string(),
        n => format!("{n} sessions will end."),
    }
}
