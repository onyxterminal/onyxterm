#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::{AppState, PaletteChoice};
use crate::keymap::Combo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    NewTab,
    CloseActive,
    OpenPalette,
    ClosePalette,
    NextTab,
    PrevTab,
    SplitRight,
    SplitDown,
    FocusAdjacent,
    MaximizePane,
    SwitchWorkspace,
    NewWorkspace,
    RenameTab,
    Copy,
    Paste,
    Quit,
    ToggleFullScreen,
    Minimize,
    Zoom,
    Hide,
    HideOthers,
    ShowAll,
}

impl CommandId {
    pub fn is_structural(self) -> bool {
        use CommandId::*;
        matches!(self, NewTab | NextTab | PrevTab | SplitRight | SplitDown | FocusAdjacent | CloseActive)
    }
}

pub type EnabledFn = fn(&AppState) -> bool;
pub type HandlerFn = fn(&mut AppState);

#[derive(Debug, Clone)]
pub struct Command {
    pub id: CommandId,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<Combo>,
    pub enabled: EnabledFn,
    pub handler: HandlerFn,
}

impl Command {
    pub fn builder(
        id: CommandId,
        title: &'static str,
        category: &'static str,
        handler: HandlerFn,
    ) -> CommandBuilder {
        CommandBuilder {
            id,
            title,
            category,
            shortcut: None,
            enabled: always_enabled,
            handler,
        }
    }
}

pub struct CommandBuilder {
    id: CommandId,
    title: &'static str,
    category: &'static str,
    shortcut: Option<Combo>,
    enabled: EnabledFn,
    handler: HandlerFn,
}

impl CommandBuilder {
    pub fn shortcut(mut self, combo: Combo) -> Self {
        self.shortcut = Some(combo);
        self
    }

    pub fn enabled_when(mut self, enabled: EnabledFn) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn build(self) -> Command {
        Command {
            id: self.id,
            title: self.title,
            category: self.category,
            shortcut: self.shortcut,
            enabled: self.enabled,
            handler: self.handler,
        }
    }
}

fn always_enabled(_state: &AppState) -> bool {
    true
}

fn palette_open(state: &AppState) -> bool {
    state.palette_visible
}

pub fn open_palette(state: &mut AppState) {
    state.palette_visible = true;
    state.palette_query.clear();
    state.palette_selection = 0;
    state.palette_choices = None;
}

pub fn open_choice_palette(state: &mut AppState, choices: Vec<PaletteChoice>) {
    open_palette(state);
    state.palette_choices = Some(choices);
}

pub fn filtered_choices(state: &AppState) -> Vec<&PaletteChoice> {
    let Some(choices) = state.palette_choices.as_ref() else {
        return Vec::new();
    };
    let needle = state.palette_query.trim().to_ascii_lowercase();
    let mut scored: Vec<(&PaletteChoice, usize)> = choices
        .iter()
        .filter_map(|choice| fuzzy_score(&choice.title.to_ascii_lowercase(), &needle).map(|score| (choice, score)))
        .collect();
    scored.sort_by_key(|(_, score)| *score);
    scored.into_iter().map(|(choice, _)| choice).collect()
}

pub fn palette_selected_choice(state: &AppState) -> Option<String> {
    filtered_choices(state).get(state.palette_selection).map(|choice| choice.key.clone())
}

fn palette_len(state: &AppState) -> usize {
    match state.palette_choices {
        Some(_) => filtered_choices(state).len(),
        None => state.filtered_commands().len(),
    }
}

pub fn close_palette(state: &mut AppState) {
    state.palette_visible = false;
}

pub fn toggle_palette(state: &mut AppState) {
    if state.palette_visible {
        close_palette(state);
        return;
    }
    open_palette(state);
}

pub fn palette_type(state: &mut AppState, text: &str) {
    state.palette_query.push_str(text);
    state.palette_selection = 0;
}

pub fn palette_backspace(state: &mut AppState) {
    state.palette_query.pop();
    state.palette_selection = 0;
}

pub fn palette_move(state: &mut AppState, step: isize) {
    let count = palette_len(state);
    if count == 0 {
        state.palette_selection = 0;
        return;
    }
    let current = state.palette_selection.min(count - 1) as isize;
    state.palette_selection = (current + step).rem_euclid(count as isize) as usize;
}

pub fn palette_selected(state: &AppState) -> Option<CommandId> {
    if state.palette_choices.is_some() {
        return None;
    }
    state.filtered_commands().get(state.palette_selection).copied()
}

pub fn default_commands() -> Vec<Command> {
    use CommandId::*;
    vec![
        Command::builder(NewTab, "New Tab", "Tabs", no_op).build(),
        Command::builder(CloseActive, "Close Pane", "Panes", no_op).build(),
        Command::builder(OpenPalette, "Open Palette", "Palette", toggle_palette).build(),
        Command::builder(ClosePalette, "Close Palette", "Palette", close_palette)
            .enabled_when(palette_open)
            .build(),
        Command::builder(NextTab, "Next Tab", "Tabs", no_op).build(),
        Command::builder(RenameTab, "Rename Tab", "Tabs", no_op).build(),
        Command::builder(PrevTab, "Previous Tab", "Tabs", no_op).build(),
        Command::builder(SplitRight, "Split Right", "Panes", no_op).build(),
        Command::builder(SplitDown, "Split Down", "Panes", no_op).build(),
        Command::builder(FocusAdjacent, "Focus Next Pane", "Panes", no_op).build(),
        Command::builder(MaximizePane, "Maximize Pane", "Panes", no_op).build(),
        Command::builder(SwitchWorkspace, "Switch Workspace", "Workspaces", no_op).build(),
        Command::builder(NewWorkspace, "New Workspace", "Workspaces", no_op).build(),
        Command::builder(Copy, "Copy", "Edit", no_op).build(),
        Command::builder(Paste, "Paste", "Edit", no_op).build(),
        Command::builder(ToggleFullScreen, "Toggle Full Screen", "Window", no_op).build(),
        Command::builder(Quit, "Quit Onyxterm", "Application", no_op)
            .enabled_when(always_enabled)
            .build(),
    ]
}

pub fn no_op(_state: &mut AppState) {}

#[derive(Debug, Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        Self {
            commands: default_commands(),
        }
    }

    pub fn register(&mut self, command: Command) {
        if let Some(slot) = self.commands.iter_mut().find(|c| c.id == command.id) {
            *slot = command;
        } else {
            self.commands.push(command);
        }
    }

    pub fn list(&self) -> Vec<&Command> {
        self.commands.iter().collect()
    }

    pub fn search(&self, query: &str) -> Vec<&Command> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return self.list();
        }
        let mut scored: Vec<(&Command, usize)> = self
            .commands
            .iter()
            .filter_map(|cmd| score_match(cmd, &needle).map(|s| (cmd, s)))
            .collect();
        scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.title.cmp(b.0.title)));
        scored.into_iter().map(|(cmd, _)| cmd).collect()
    }

    pub fn dispatch(state: &mut AppState, id: CommandId) {
        let (enabled, handler) = match state.registry.commands.iter().find(|c| c.id == id) {
            Some(cmd) => (cmd.enabled, cmd.handler),
            None => return,
        };
        if !(enabled)(state) {
            return;
        }
        (handler)(state);
    }

    pub fn execute(&self, id: CommandId, state: &mut AppState) {
        Self::dispatch(state, id);
    }

    pub fn get(&self, id: CommandId) -> Option<&Command> {
        self.commands.iter().find(|c| c.id == id)
    }
}

fn score_match(cmd: &Command, needle: &str) -> Option<usize> {
    if !cmd.category.is_empty() && needle.contains(cmd.category.to_ascii_lowercase().as_str()) {
        return Some(0);
    }
    fuzzy_score(&cmd.title.to_ascii_lowercase(), needle)
}

fn fuzzy_score(title: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut best: Option<usize> = None;
    let mut cursor = 0usize;
    for ch in needle.chars() {
        let pos = title[cursor..].find(ch)?;
        cursor += pos + ch.len_utf8();
        let score = best.unwrap_or(0) + pos;
        best = Some(score);
    }
    if title == needle {
        return Some(best.unwrap_or(0).saturating_sub(10));
    }
    if title.starts_with(needle) {
        return Some(best.unwrap_or(0).saturating_sub(5));
    }
    if title.contains(needle) {
        return Some(best.unwrap_or(0));
    }
    best
}
