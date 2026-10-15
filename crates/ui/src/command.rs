#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::keymap::Combo;
use domain::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    NewTab,
    CloseActive,
    ClosePane,
    OpenPalette,
    OpenSettings,
    ClosePalette,
    NextTab,
    PrevTab,
    SplitRight,
    SplitDown,
    FocusAdjacent,
    MaximizePane,
    NewWorkspace,
    RenameWorkspace,
    MoveTabLeft,
    MoveTabRight,
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
    About,
    CloseTab,
    SelectAll,
    RenamePane,
    MoveToNewTab,
    FocusPane(Direction),
    SwapPane(Direction),
    ResizePane(Direction),
    GridLayout,
    EqualizePanes,
    SelectTab(usize),
    SelectWorkspace(usize),
    NextWorkspace,
    PrevWorkspace,
    OpenWorkspaceMenu,
    ToggleCopyMode,
    OpenFile,
    OpenFolder,
    OpenEditorHere,
    ToggleFileTree,
    ToggleEditorBundle,
    Save,
    NextDocument,
    PrevDocument,
    CloseDocument,
    ClearTerminal,
    InterruptProcess,
}

const COMMAND_KEYS: [(CommandId, &str); 73] = [
    (CommandId::NewTab, "new_tab"), (CommandId::CloseActive, "close"), (CommandId::MoveTabLeft, "move_tab_left"),
    (CommandId::MoveTabRight, "move_tab_right"), (CommandId::ClosePane, "close_pane"),
    (CommandId::OpenPalette, "open_palette"), (CommandId::OpenSettings, "open_settings"),
    (CommandId::ClosePalette, "close_palette"), (CommandId::NextTab, "next_tab"), (CommandId::RenameTab, "rename_tab"),
    (CommandId::PrevTab, "previous_tab"), (CommandId::SplitRight, "split_right"), (CommandId::SplitDown, "split_down"),
    (CommandId::FocusAdjacent, "focus_next_pane"), (CommandId::FocusPane(Direction::Left), "focus_pane_left"),
    (CommandId::FocusPane(Direction::Right), "focus_pane_right"),
    (CommandId::FocusPane(Direction::Up), "focus_pane_up"), (CommandId::FocusPane(Direction::Down), "focus_pane_down"),
    (CommandId::SwapPane(Direction::Left), "swap_pane_left"),
    (CommandId::SwapPane(Direction::Right), "swap_pane_right"), (CommandId::SwapPane(Direction::Up), "swap_pane_up"),
    (CommandId::SwapPane(Direction::Down), "swap_pane_down"), (CommandId::MaximizePane, "maximize_pane"),
    (CommandId::GridLayout, "grid_layout"), (CommandId::EqualizePanes, "equalize_panes"),
    (CommandId::RenamePane, "rename_pane"), (CommandId::MoveToNewTab, "move_pane_to_new_tab"),
    (CommandId::OpenFile, "open_file"), (CommandId::OpenFolder, "open_folder"),
    (CommandId::OpenEditorHere, "open_editor_here"), (CommandId::ToggleFileTree, "toggle_file_tree"),
    (CommandId::ToggleEditorBundle, "toggle_editor_bundle"), (CommandId::Save, "save"),
    (CommandId::NextDocument, "next_document"), (CommandId::PrevDocument, "previous_document"),
    (CommandId::CloseDocument, "close_document"),
    (CommandId::NewWorkspace, "new_workspace"), (CommandId::RenameWorkspace, "rename_workspace"),
    (CommandId::NextWorkspace, "next_workspace"), (CommandId::PrevWorkspace, "previous_workspace"),
    (CommandId::CloseTab, "close_tab"), (CommandId::Copy, "copy"), (CommandId::SelectAll, "select_all"),
    (CommandId::ToggleCopyMode, "toggle_copy_mode"), (CommandId::Paste, "paste"),
    (CommandId::ToggleFullScreen, "toggle_full_screen"), (CommandId::Quit, "quit"),
    (CommandId::Minimize, "minimize"), (CommandId::OpenWorkspaceMenu, "open_workspace_menu"),
    (CommandId::ResizePane(Direction::Left), "resize_pane_left"),
    (CommandId::ResizePane(Direction::Right), "resize_pane_right"),
    (CommandId::ResizePane(Direction::Up), "resize_pane_up"),
    (CommandId::ResizePane(Direction::Down), "resize_pane_down"),
    (CommandId::SelectTab(0), "select_tab_1"), (CommandId::SelectTab(1), "select_tab_2"),
    (CommandId::SelectTab(2), "select_tab_3"), (CommandId::SelectTab(3), "select_tab_4"),
    (CommandId::SelectTab(4), "select_tab_5"), (CommandId::SelectTab(5), "select_tab_6"),
    (CommandId::SelectTab(6), "select_tab_7"), (CommandId::SelectTab(7), "select_tab_8"),
    (CommandId::SelectTab(8), "select_tab_9"),
    (CommandId::SelectWorkspace(0), "select_workspace_1"), (CommandId::SelectWorkspace(1), "select_workspace_2"),
    (CommandId::SelectWorkspace(2), "select_workspace_3"), (CommandId::SelectWorkspace(3), "select_workspace_4"),
    (CommandId::SelectWorkspace(4), "select_workspace_5"), (CommandId::SelectWorkspace(5), "select_workspace_6"),
    (CommandId::SelectWorkspace(6), "select_workspace_7"), (CommandId::SelectWorkspace(7), "select_workspace_8"),
    (CommandId::SelectWorkspace(8), "select_workspace_9"),
    (CommandId::ClearTerminal, "clear_terminal"), (CommandId::InterruptProcess, "interrupt_process"),
];

const SELECT_TAB_TITLES: [&str; 9] = [
    "Select Tab 1", "Select Tab 2", "Select Tab 3", "Select Tab 4", "Select Tab 5", "Select Tab 6", "Select Tab 7",
    "Select Tab 8", "Select Tab 9",
];

const SELECT_WORKSPACE_TITLES: [&str; 9] = [
    "Select Workspace 1", "Select Workspace 2", "Select Workspace 3", "Select Workspace 4", "Select Workspace 5",
    "Select Workspace 6", "Select Workspace 7", "Select Workspace 8", "Select Workspace 9",
];

impl CommandId {
    pub fn key(self) -> Option<&'static str> {
        COMMAND_KEYS.iter().find(|(id, _)| *id == self).map(|(_, key)| *key)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        COMMAND_KEYS.iter().find(|(_, candidate)| *candidate == key).map(|(id, _)| *id)
    }

    pub fn is_rebindable(self) -> bool {
        !matches!(self, CommandId::ClosePalette)
    }

    pub fn is_listed_in_palette(self) -> bool {
        !matches!(self, CommandId::SelectTab(_) | CommandId::SelectWorkspace(_))
    }

    pub fn is_structural(self) -> bool {
        use CommandId::*;
        matches!(
            self,
            NewTab | NextTab | PrevTab | SelectTab(_) | SplitRight | SplitDown | FocusAdjacent | FocusPane(_)
                | SwapPane(_) | ResizePane(_) | GridLayout | EqualizePanes | CloseActive | ClosePane | CloseTab | MoveToNewTab
                | NextDocument | PrevDocument | CloseDocument
        )
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
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        self.title.to_lowercase().contains(&needle) || self.category.to_lowercase().contains(&needle)
    }

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
}

fn palette_len(state: &AppState) -> usize {
    state.filtered_commands().len()
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
    state.palette_query.insert(text);
    state.palette_selection = 0;
}

pub fn palette_backspace(state: &mut AppState) {
    state.palette_query.delete_backward();
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
    state.filtered_commands().get(state.palette_selection).copied()
}

pub fn default_commands() -> Vec<Command> {
    use CommandId::*;
    let mut commands = vec![
        Command::builder(NewTab, "New Tab", "Tabs", no_op).build(),
        Command::builder(SplitRight, "Split Right", "Panes", no_op).build(),
        Command::builder(SplitDown, "Split Down", "Panes", no_op).build(),
        Command::builder(OpenEditorHere, "Open Editor Here", "Files", no_op).build(),
        Command::builder(ToggleFileTree, "Toggle File Tree", "Files", no_op).build(),
        Command::builder(ToggleEditorBundle, "Toggle Editor Bundle", "Files", no_op).build(),
        Command::builder(Save, "Save", "Files", no_op).build(),
        Command::builder(NextDocument, "Next Document", "Files", no_op).build(),
        Command::builder(ClearTerminal, "Clear Terminal", "Terminal", no_op).build(),
        Command::builder(InterruptProcess, "Interrupt Process", "Terminal", no_op).build(),
        Command::builder(MaximizePane, "Maximize Pane", "Panes", no_op).build(),
        Command::builder(GridLayout, "Arrange Panes in Grid", "Panes", no_op).build(),
        Command::builder(EqualizePanes, "Equalize Panes", "Panes", no_op).build(),
        Command::builder(RenamePane, "Rename Pane\u{2026}", "Panes", no_op).build(),
        Command::builder(MoveToNewTab, "Move Pane to New Tab", "Panes", no_op).build(),
        Command::builder(ClosePane, "Close Pane", "Panes", no_op).build(),
        Command::builder(NextTab, "Next Tab", "Tabs", no_op).build(),
        Command::builder(PrevTab, "Previous Tab", "Tabs", no_op).build(),
        Command::builder(OpenWorkspaceMenu, "Switch Workspace", "Workspaces", no_op).build(),
        Command::builder(NewWorkspace, "New Workspace", "Workspaces", no_op).build(),
        Command::builder(ToggleCopyMode, "Toggle Copy Mode", "Edit", no_op).build(),
        Command::builder(OpenSettings, "Settings", "Application", no_op).build(),
        Command::builder(CloseActive, "Close", "Panes", no_op).build(),
        Command::builder(MoveTabLeft, "Move Tab Left", "Tabs", no_op).build(),
        Command::builder(MoveTabRight, "Move Tab Right", "Tabs", no_op).build(),
        Command::builder(OpenPalette, "Open Palette", "Palette", toggle_palette).build(),
        Command::builder(ClosePalette, "Close Palette", "Palette", close_palette)
            .enabled_when(palette_open)
            .build(),
        Command::builder(RenameTab, "Rename Tab", "Tabs", no_op).build(),
        Command::builder(FocusAdjacent, "Focus Next Pane", "Panes", no_op).build(),
        Command::builder(FocusPane(Direction::Left), "Focus Pane Left", "Panes", no_op).build(),
        Command::builder(FocusPane(Direction::Right), "Focus Pane Right", "Panes", no_op).build(),
        Command::builder(FocusPane(Direction::Up), "Focus Pane Up", "Panes", no_op).build(),
        Command::builder(FocusPane(Direction::Down), "Focus Pane Down", "Panes", no_op).build(),
        Command::builder(SwapPane(Direction::Left), "Swap Pane Left", "Panes", no_op).build(),
        Command::builder(SwapPane(Direction::Right), "Swap Pane Right", "Panes", no_op).build(),
        Command::builder(SwapPane(Direction::Up), "Swap Pane Up", "Panes", no_op).build(),
        Command::builder(SwapPane(Direction::Down), "Swap Pane Down", "Panes", no_op).build(),
        Command::builder(ResizePane(Direction::Left), "Resize Pane Left", "Panes", no_op).build(),
        Command::builder(ResizePane(Direction::Right), "Resize Pane Right", "Panes", no_op).build(),
        Command::builder(ResizePane(Direction::Up), "Resize Pane Up", "Panes", no_op).build(),
        Command::builder(ResizePane(Direction::Down), "Resize Pane Down", "Panes", no_op).build(),
        Command::builder(OpenFile, "Open File…", "Files", no_op).build(),
        Command::builder(OpenFolder, "Open Folder…", "Files", no_op).build(),
        Command::builder(PrevDocument, "Previous Document", "Files", no_op).build(),
        Command::builder(CloseDocument, "Close Document", "Files", no_op).build(),
        Command::builder(RenameWorkspace, "Rename Workspace\u{2026}", "Workspaces", no_op).build(),
        Command::builder(NextWorkspace, "Next Workspace", "Workspaces", no_op).build(),
        Command::builder(PrevWorkspace, "Previous Workspace", "Workspaces", no_op).build(),
        Command::builder(CloseTab, "Close Tab", "Tabs", no_op).build(),
        Command::builder(Copy, "Copy", "Edit", no_op).build(),
        Command::builder(SelectAll, "Select All", "Edit", no_op).build(),
        Command::builder(Paste, "Paste", "Edit", no_op).build(),
        Command::builder(ToggleFullScreen, "Toggle Full Screen", "Window", no_op).build(),
        Command::builder(Minimize, "Minimize", "Window", no_op).build(),
        Command::builder(Quit, "Quit Onyx", "Application", no_op)
            .enabled_when(always_enabled)
            .build(),
    ];
    commands.extend(indexed_commands(SelectTab, &SELECT_TAB_TITLES, "Tabs"));
    commands.extend(indexed_commands(SelectWorkspace, &SELECT_WORKSPACE_TITLES, "Workspaces"));
    commands
}

fn indexed_commands(
    id_at: fn(usize) -> CommandId,
    titles: &'static [&'static str],
    category: &'static str,
) -> impl Iterator<Item = Command> {
    titles.iter().enumerate().map(move |(index, title)| Command::builder(id_at(index), title, category, no_op).build())
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
        let listed = self.commands.iter().filter(|cmd| cmd.id.is_listed_in_palette());
        if needle.is_empty() {
            return listed.collect();
        }
        let mut scored: Vec<(&Command, usize)> = listed
            .filter_map(|cmd| score_match(cmd, &needle).map(|s| (cmd, s)))
            .collect();
        scored.sort_by_key(|(_, score)| *score);
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
