#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{DocumentId, PanelId, TabId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ClosePane(PanelId),
    CloseTab(TabId),
    DeleteWorkspace(WorkspaceId),
    DiscardPane(PanelId),
    DiscardDocument(PanelId, DocumentId),
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

pub fn discard_documents_copy(unsaved: usize) -> ConfirmCopy {
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body: format!("This pane has {}.", unsaved_files(unsaved)),
        action: "Close pane",
        is_destructive: true,
    }
}

pub fn discard_tab_copy(unsaved: usize, has_running_processes: bool) -> ConfirmCopy {
    let mut body = format!("This tab has {}.", unsaved_files(unsaved));
    if has_running_processes {
        body.push_str(" Running processes in this tab will be terminated.");
    }
    ConfirmCopy {
        title: "Discard unsaved changes?".to_string(),
        body,
        action: "Close tab",
        is_destructive: true,
    }
}

pub fn delete_workspace_copy(name: &str, sessions: usize, unsaved: usize) -> ConfirmCopy {
    let consequences = consequence_phrases(sessions, unsaved).join(" and ");
    let body = match consequences.is_empty() {
        true => "Its tabs and layout are removed.".to_string(),
        false => format!("Its tabs and layout are removed. {}.", consequences),
    };
    ConfirmCopy {
        title: format!("Delete workspace \u{201c}{name}\u{201d}?"),
        body,
        action: "Delete",
        is_destructive: true,
    }
}

pub fn quit_copy(sessions: usize, unsaved: usize) -> ConfirmCopy {
    let consequences = consequence_phrases(sessions, unsaved).join(" and ");
    let body = match consequences.is_empty() {
        true => String::new(),
        false => format!("{}.", consequences),
    };
    ConfirmCopy {
        title: "Quit Onyx?".into(),
        body,
        action: "Quit",
        is_destructive: unsaved > 0,
    }
}

pub fn sessions_ending(count: usize) -> String {
    format!("{}.", sessions_end_phrase(count))
}

fn consequence_phrases(sessions: usize, unsaved: usize) -> Vec<String> {
    let mut phrases = Vec::new();
    if sessions > 0 {
        phrases.push(sessions_end_phrase(sessions));
    }
    if unsaved > 0 {
        phrases.push(unsaved_lost_phrase(unsaved));
    }
    phrases
}

fn sessions_end_phrase(count: usize) -> String {
    match count {
        1 => "1 session will end".to_string(),
        n => format!("{n} sessions will end"),
    }
}

fn unsaved_lost_phrase(count: usize) -> String {
    match count {
        1 => "1 unsaved file will be lost".to_string(),
        n => format!("{n} unsaved files will be lost"),
    }
}

fn unsaved_files(count: usize) -> String {
    match count {
        1 => "1 unsaved file".to_string(),
        n => format!("{n} unsaved files"),
    }
}
