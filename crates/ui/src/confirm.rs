#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{PanelId, TabId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ClosePane(PanelId),
    CloseTab(TabId),
    DeleteWorkspace(WorkspaceId),
    DiscardPane(PanelId),
    DiscardTab(TabId),
    Quit { sessions: usize, unsaved: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmCopy {
    pub title: String,
    pub body: String,
    pub action: &'static str,
    pub is_destructive: bool,
}

pub fn close_pane_copy() -> ConfirmCopy {
    ConfirmCopy {
        title: "Terminate running process?".into(),
        body: "A command is still running in this pane. Closing it will end the process.".into(),
        action: "Close and terminate",
        is_destructive: true,
    }
}

pub fn close_tab_copy() -> ConfirmCopy {
    ConfirmCopy {
        title: "Terminate running processes?".into(),
        body: "A command is still running in this tab. Closing it will end every process in it.".into(),
        action: "Close tab",
        is_destructive: true,
    }
}

pub fn discard_pane_copy(name: &str) -> ConfirmCopy {
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body: format!("{name} has changes that haven't been saved."),
        action: "Discard",
        is_destructive: true,
    }
}

pub fn discard_tab_copy(unsaved: usize) -> ConfirmCopy {
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body: format!("This tab has {}.", unsaved_files(unsaved)),
        action: "Close tab",
        is_destructive: true,
    }
}

pub fn delete_workspace_copy(name: &str, sessions: usize, unsaved: usize) -> ConfirmCopy {
    let sessions_text = if sessions > 0 { Some(sessions_ending(sessions)) } else { None };
    let unsaved_text = if unsaved > 0 { Some(unsaved_files(unsaved)) } else { None };
    let body = match (sessions_text, unsaved_text) {
        (None, None) => "Its tabs and layout are removed.".to_string(),
        (Some(s), None) => format!("Its tabs and layout are removed. {s}"),
        (None, Some(u)) => format!("Its tabs and layout are removed. {u}."),
        (Some(s), Some(u)) => {
            let prefix = s.trim_end_matches('.');
            format!("Its tabs and layout are removed. {prefix} and {u}.")
        }
    };
    ConfirmCopy {
        title: format!("Delete workspace \u{201c}{name}\u{201d}?"),
        body,
        action: "Delete",
        is_destructive: true,
    }
}

pub fn quit_copy(sessions: usize, unsaved: usize) -> ConfirmCopy {
    let body = match (sessions, unsaved) {
        (0, 0) => String::new(),
        (_, 0) => sessions_ending(sessions),
        (0, _) => format!("{}.", unsaved_files(unsaved)),
        (_, _) => {
            let prefix = sessions_ending(sessions).trim_end_matches('.').to_string();
            format!("{prefix} and {}.", unsaved_files(unsaved))
        }
    };
    ConfirmCopy {
        title: "Quit Onyxterm?".into(),
        body,
        action: "Quit",
        is_destructive: false,
    }
}

pub fn sessions_ending(count: usize) -> String {
    match count {
        1 => "1 session will end.".to_string(),
        n => format!("{n} sessions will end."),
    }
}

fn unsaved_files(count: usize) -> String {
    match count {
        1 => "1 unsaved file".to_string(),
        n => format!("{n} unsaved files"),
    }
}
