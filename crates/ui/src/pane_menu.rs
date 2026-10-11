#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandId;
use crate::keymap::Keymap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneMenuItem {
    pub command: CommandId,
    pub title: &'static str,
    pub shortcut: Option<String>,
}

impl PaneMenuItem {
    pub fn is_destructive(&self) -> bool {
        self.command == CommandId::CloseActive
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneMenuEntry {
    Item(PaneMenuItem),
    Divider,
}

pub fn pane_menu_entries(is_maximized: bool, keymap: &Keymap) -> Vec<PaneMenuEntry> {
    let maximize_title = if is_maximized { "Restore Pane" } else { "Maximize Pane" };
    let item = |command: CommandId, title: &'static str| {
        let shortcut = keymap.combo_for(command).map(|combo| combo.glyphs());
        PaneMenuEntry::Item(PaneMenuItem { command, title, shortcut })
    };
    vec![
        item(CommandId::SplitRight, "Split Right"),
        item(CommandId::SplitDown, "Split Down"),
        item(CommandId::MaximizePane, maximize_title),
        PaneMenuEntry::Divider,
        item(CommandId::RenamePane, "Rename Pane\u{2026}"),
        item(CommandId::MoveToNewTab, "Move to New Tab"),
        PaneMenuEntry::Divider,
        item(CommandId::CloseActive, "Close Pane"),
    ]
}

pub fn selectable_commands(entries: &[PaneMenuEntry]) -> Vec<CommandId> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            PaneMenuEntry::Item(item) => Some(item.command),
            PaneMenuEntry::Divider => None,
        })
        .collect()
}

pub fn step_selection(current: usize, step: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (current.min(count - 1) as isize + step).rem_euclid(count as isize) as usize
}
