#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandId;
use domain::Direction;
use gpui::{Keystroke, Modifiers};
use std::collections::HashMap;

pub type KeyCode = String;

const US_KEY_FOR_LAYOUT_CHAR: [(char, &str); 43] = [
    ('\u{0634}', "a"), ('\u{0632}', "b"), ('\u{0630}', "c"), ('\u{064a}', "d"), ('\u{062b}', "e"), ('\u{0628}', "f"),
    ('\u{0644}', "g"), ('\u{0627}', "h"), ('\u{0647}', "i"), ('\u{062a}', "j"), ('\u{0646}', "k"), ('\u{0645}', "l"),
    ('\u{0648}', "m"), ('\u{0631}', "n"), ('\u{062e}', "o"), ('\u{062d}', "p"), ('\u{0636}', "q"), ('\u{0642}', "r"),
    ('\u{0633}', "s"), ('\u{0641}', "t"), ('\u{0639}', "u"), ('\u{062f}', "v"), ('\u{0635}', "w"), ('\u{0637}', "x"),
    ('\u{063a}', "y"), ('\u{0638}', "z"), ('\u{0660}', "0"), ('\u{0661}', "1"), ('\u{0662}', "2"), ('\u{0663}', "3"),
    ('\u{0664}', "4"), ('\u{0665}', "5"), ('\u{0666}', "6"), ('\u{0667}', "7"), ('\u{0668}', "8"), ('\u{0669}', "9"),
    ('\u{0640}', "-"), ('\u{060c}', ","), ('\u{0643}', ";"), ('\u{061b}', "'"), ('\u{062c}', "["), ('\u{0629}', "]"),
    ('\u{061f}', "/"),
];

const US_SHIFTED: [(&str, &str); 17] = [
    ("1", "!"), ("2", "@"), ("3", "#"), ("4", "$"), ("5", "%"), ("6", "^"), ("7", "&"), ("8", "*"), ("9", "("),
    ("0", ")"), ("-", "_"), (",", "<"), (";", ":"), ("'", "\""), ("[", "{"), ("]", "}"), ("/", "?"),
];

pub fn physical_combo(keystroke: &Keystroke) -> Option<Combo> {
    let modifiers = keystroke.modifiers;
    if !(modifiers.platform || modifiers.control) {
        return None;
    }
    let mut chars = keystroke.key.chars();
    let (Some(typed), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let us_key = US_KEY_FOR_LAYOUT_CHAR.iter().find(|(layout_char, _)| *layout_char == typed)?.1;
    if !modifiers.shift || us_key.chars().all(|c| c.is_ascii_lowercase()) {
        return Some(Combo::new(modifiers, us_key));
    }
    let shifted = US_SHIFTED.iter().find(|(base, _)| *base == us_key)?.1;
    Some(Combo::new(Modifiers { shift: false, ..modifiers }, shifted))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combo {
    pub modifiers: Modifiers,
    pub key: KeyCode,
}

impl Combo {
    pub fn new(modifiers: Modifiers, key: impl Into<KeyCode>) -> Self {
        Self {
            modifiers,
            key: key.into(),
        }
    }

    pub fn glyphs(&self) -> String {
        let flags = [
            (self.modifiers.control, "\u{2303}"),
            (self.modifiers.alt, "\u{2325}"),
            (self.modifiers.shift, "\u{21e7}"),
            (self.modifiers.platform, "\u{2318}"),
        ];
        let prefix: String = flags.iter().filter(|(is_held, _)| *is_held).map(|(_, glyph)| *glyph).collect();
        let key = match self.key.as_str() {
            "right" => "\u{2192}".to_string(),
            "left" => "\u{2190}".to_string(),
            "up" => "\u{2191}".to_string(),
            "down" => "\u{2193}".to_string(),
            "enter" => "\u{21a9}".to_string(),
            "tab" => "\u{21e5}".to_string(),
            other => other.to_uppercase(),
        };
        format!("{prefix}{key}")
    }

    pub fn binding_text(&self) -> String {
        let flags = [
            (self.modifiers.control, "ctrl-"),
            (self.modifiers.alt, "alt-"),
            (self.modifiers.shift, "shift-"),
            (self.modifiers.platform, "cmd-"),
            (self.modifiers.function, "fn-"),
        ];
        let prefix: String = flags.iter().filter(|(is_held, _)| *is_held).map(|(_, text)| *text).collect();
        format!("{prefix}{}", self.key)
    }
}

#[derive(Debug)]
pub struct Keymap {
    bindings: HashMap<Combo, CommandId>,
    order: Vec<Combo>,
    conflicts: Vec<(Combo, CommandId, CommandId)>,
}

impl Keymap {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            order: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn bind(&mut self, combo: Combo, cmd: CommandId) {
        if let Some(existing) = self.bindings.get(&combo) {
            if *existing != cmd {
                self.conflicts.push((combo.clone(), *existing, cmd));
            }
        } else {
            self.order.push(combo.clone());
        }
        self.bindings.insert(combo, cmd);
    }

    pub fn unbind(&mut self, combo: &Combo) -> Option<CommandId> {
        let prev = self.bindings.remove(combo);
        if prev.is_some() {
            self.order.retain(|c| c != combo);
        }
        prev
    }

    pub fn resolve(&self, combo: &Combo) -> Option<CommandId> {
        self.bindings.get(combo).copied()
    }

    pub fn combo_for(&self, command: CommandId) -> Option<&Combo> {
        self.order.iter().find(|combo| self.bindings.get(*combo) == Some(&command))
    }

    pub fn resolve_keystroke(&self, keystroke: &Keystroke) -> Option<CommandId> {
        let typed = Combo::new(keystroke.modifiers, keystroke.key.as_str());
        self.resolve(&typed).or_else(|| self.resolve(&physical_combo(keystroke)?))
    }

    pub fn conflicts(&self) -> Vec<(Combo, CommandId, CommandId)> {
        self.conflicts.clone()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Combo, &CommandId)> {
        self.bindings.iter()
    }
}

const INDEXED_SLOTS: usize = 9;

const DIRECTION_KEYS: [(Direction, &str, &str); 4] = [
    (Direction::Left, "h", "left"),
    (Direction::Down, "j", "down"),
    (Direction::Up, "k", "up"),
    (Direction::Right, "l", "right"),
];

impl Default for Keymap {
    fn default() -> Self {
        let mut map = Self::new();
        bind_application(&mut map);
        bind_panes(&mut map);
        bind_pane_movement(&mut map);
        bind_tabs(&mut map);
        bind_workspaces(&mut map);
        map
    }
}

fn bind_application(map: &mut Keymap) {
    use CommandId::*;
    let cmd = Modifiers::command();
    map.bind(Combo::new(cmd, "a"), SelectAll);
    map.bind(Combo::new(cmd, "q"), Quit);
    map.bind(Combo::new(Modifiers::command_shift(), "p"), OpenPalette);
    map.bind(Combo::new(Modifiers::command_shift(), "o"), OpenFolder);
    map.bind(Combo::new(cmd, ","), OpenSettings);
    map.bind(Combo::new(cmd, "c"), Copy);
    map.bind(Combo::new(cmd, "v"), Paste);
    map.bind(Combo::new(Modifiers::command_shift(), "c"), ToggleCopyMode);
    map.bind(Combo::new(cmd | Modifiers::control(), "f"), ToggleFullScreen);
    map.bind(Combo::new(cmd, "m"), Minimize);
}

fn bind_panes(map: &mut Keymap) {
    use CommandId::*;
    let cmd = Modifiers::command();
    map.bind(Combo::new(cmd, "w"), CloseActive);
    map.bind(Combo::new(cmd, "d"), SplitRight);
    map.bind(Combo::new(Modifiers::command_shift(), "d"), SplitDown);
    map.bind(Combo::new(cmd, "enter"), MaximizePane);
    map.bind(Combo::new(Modifiers::command_shift(), "g"), GridLayout);
    map.bind(Combo::new(Modifiers::command_shift(), "e"), EqualizePanes);
}

fn bind_pane_movement(map: &mut Keymap) {
    use CommandId::*;
    let cmd = Modifiers::command();
    for (direction, vim_key, arrow_key) in DIRECTION_KEYS {
        map.bind(Combo::new(cmd, vim_key), FocusPane(direction));
        map.bind(Combo::new(cmd | Modifiers::alt(), arrow_key), FocusPane(direction));
        map.bind(Combo::new(Modifiers::command_shift(), vim_key), SwapPane(direction));
        map.bind(Combo::new(cmd | Modifiers::control(), vim_key), ResizePane(direction));
    }
}

fn bind_tabs(map: &mut Keymap) {
    use CommandId::*;
    let cmd = Modifiers::command();
    map.bind(Combo::new(cmd, "t"), NewTab);
    map.bind(Combo::new(Modifiers::command_shift(), "w"), CloseTab);
    map.bind(Combo::new(cmd, "}"), NextTab);
    map.bind(Combo::new(cmd, "{"), PrevTab);
    map.bind(Combo::new(Modifiers::control(), "tab"), NextTab);
    map.bind(Combo::new(Modifiers::control() | Modifiers::shift(), "tab"), PrevTab);
    for index in 0..INDEXED_SLOTS {
        map.bind(Combo::new(cmd, slot_key(index)), SelectTab(index));
    }
}

fn bind_workspaces(map: &mut Keymap) {
    use CommandId::*;
    let cmd_alt = Modifiers::command() | Modifiers::alt();
    map.bind(Combo::new(Modifiers::command_shift(), "n"), NewWorkspace);
    map.bind(Combo::new(Modifiers::command(), "o"), OpenWorkspaceMenu);
    map.bind(Combo::new(cmd_alt, "]"), NextWorkspace);
    map.bind(Combo::new(cmd_alt, "["), PrevWorkspace);
    for index in 0..INDEXED_SLOTS {
        map.bind(Combo::new(cmd_alt, slot_key(index)), SelectWorkspace(index));
    }
}

fn slot_key(index: usize) -> String {
    (index + 1).to_string()
}
