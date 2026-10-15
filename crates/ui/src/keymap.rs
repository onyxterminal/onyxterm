#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::{CommandId, CommandRegistry};
use domain::Direction;
use gpui::{Keystroke, Modifiers};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

pub type KeyCode = String;

const MAX_OVERRIDES_BYTES: usize = 16 * 1024;
const MAX_FUNCTION_KEY: u8 = 24;
const CANCEL_KEY: &str = "escape";
const UNBIND_KEYS: [&str; 2] = ["backspace", "delete"];

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
        self.keycaps().concat()
    }

    pub fn keycaps(&self) -> Vec<String> {
        let flags = [
            (self.modifiers.control, "\u{2303}"),
            (self.modifiers.alt, "\u{2325}"),
            (self.modifiers.shift, "\u{21e7}"),
            (self.modifiers.platform, "\u{2318}"),
        ];
        let key = match self.key.as_str() {
            "right" => "\u{2192}".to_string(),
            "left" => "\u{2190}".to_string(),
            "up" => "\u{2191}".to_string(),
            "down" => "\u{2193}".to_string(),
            "enter" => "\u{21a9}".to_string(),
            "tab" => "\u{21e5}".to_string(),
            other => other.to_uppercase(),
        };
        flags.iter().filter(|(is_held, _)| *is_held).map(|(_, glyph)| glyph.to_string()).chain([key]).collect()
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

    pub fn parse(text: &str) -> Option<Self> {
        let keystroke = Keystroke::parse(text).ok()?;
        let combo = Self::new(keystroke.modifiers, keystroke.key);
        (combo.is_shortcut() && combo.binding_text() == text).then_some(combo)
    }

    pub fn is_shortcut(&self) -> bool {
        let modifiers = self.modifiers;
        let has_command_modifier = modifiers.platform || modifiers.control || modifiers.alt;
        !self.key.is_empty() && (has_command_modifier || is_function_key(&self.key))
    }
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=MAX_FUNCTION_KEY).contains(&number))
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

    pub fn display_combo(&self, command: CommandId) -> Option<&Combo> {
        self.combo_for(command).or_else(|| match command {
            CommandId::ClosePane => self.combo_for(CommandId::CloseActive),
            _ => None,
        })
    }

    pub fn combos_for(&self, command: CommandId) -> Vec<&Combo> {
        self.order.iter().filter(|combo| self.bindings.get(*combo) == Some(&command)).collect()
    }

    pub fn with_overrides(overrides: &ShortcutOverrides) -> Self {
        let mut map = Self::default();
        for (command, _) in overrides.iter() {
            let replaced: Vec<Combo> = map.combos_for(command).into_iter().cloned().collect();
            for combo in &replaced {
                map.unbind(combo);
            }
        }
        for (command, combo) in overrides.iter() {
            if let Some(combo) = combo {
                map.unbind(combo);
                map.bind(combo.clone(), command);
            }
        }
        map
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutOverrides {
    entries: BTreeMap<&'static str, Option<Combo>>,
}

impl ShortcutOverrides {
    pub fn parse(text: &str) -> Self {
        let mut overrides = Self::default();
        if text.len() > MAX_OVERRIDES_BYTES {
            return overrides;
        }
        let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(text) else {
            return overrides;
        };
        for (key, value) in entries {
            let Some(command) = CommandId::from_key(&key) else {
                continue;
            };
            match value {
                Value::Null => overrides.set(command, None),
                Value::String(text) => {
                    if let Some(combo) = Combo::parse(&text) {
                        overrides.set(command, Some(combo));
                    }
                }
                _ => {}
            }
        }
        overrides
    }

    pub fn to_json(&self) -> String {
        let entries: Map<String, Value> = self.entries.iter()
            .map(|(key, combo)| {
                let binding = combo.as_ref().map_or(Value::Null, |combo| Value::String(combo.binding_text()));
                (key.to_string(), binding)
            })
            .collect();
        Value::Object(entries).to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (CommandId, Option<&Combo>)> {
        self.entries.iter().filter_map(|(key, combo)| CommandId::from_key(key).map(|command| (command, combo.as_ref())))
    }

    pub fn set(&mut self, command: CommandId, combo: Option<Combo>) {
        if let Some(key) = command.key() {
            self.entries.insert(key, combo);
        }
    }

    pub fn assign(&mut self, command: CommandId, combo: Combo) {
        let effective = Keymap::with_overrides(self);
        let displaced = effective.resolve(&combo).filter(|other| *other != command);
        if let Some(other) = displaced {
            let remaining = effective.combos_for(other).into_iter().find(|kept| **kept != combo).cloned();
            self.set(other, remaining);
        }
        self.set(command, Some(combo));
    }

    pub fn reset(&mut self, command: CommandId) {
        let Some(key) = command.key() else {
            return;
        };
        self.entries.remove(key);
        let defaults = Keymap::default();
        let restored = defaults.combos_for(command);
        for combo in self.entries.values_mut() {
            if combo.as_ref().is_some_and(|taken| restored.contains(&taken)) {
                *combo = None;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    Cancel,
    Unbind,
    Assign(Combo),
}

pub fn capture_shortcut(keystroke: &Keystroke) -> Option<Capture> {
    if keystroke.key == CANCEL_KEY {
        return Some(Capture::Cancel);
    }
    if UNBIND_KEYS.contains(&keystroke.key.as_str()) && !keystroke.modifiers.modified() {
        return Some(Capture::Unbind);
    }
    let combo = physical_combo(keystroke).unwrap_or_else(|| Combo::new(keystroke.modifiers, keystroke.key.as_str()));
    combo.is_shortcut().then_some(Capture::Assign(combo))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub command: CommandId,
    pub title: &'static str,
    pub combo: Option<Combo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub category: &'static str,
    pub shortcuts: Vec<Shortcut>,
}

pub fn shortcut_groups(registry: &CommandRegistry, keymap: &Keymap, query: &str) -> Vec<ShortcutGroup> {
    let mut groups: Vec<ShortcutGroup> = Vec::new();
    let listed = registry.list().into_iter().filter(|command| command.id.is_rebindable() && command.matches(query));
    for command in listed {
        let combo = keymap.combo_for(command.id).cloned();
        let shortcut = Shortcut { command: command.id, title: command.title, combo };
        match groups.iter_mut().find(|group| group.category == command.category) {
            Some(group) => group.shortcuts.push(shortcut),
            None => groups.push(ShortcutGroup { category: command.category, shortcuts: vec![shortcut] }),
        }
    }
    groups
}

const INDEXED_SLOTS: usize = 9;
const CLEAR_TERMINAL_KEY: &str = "k";

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
        bind_documents(&mut map);
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
    map.bind(Combo::new(cmd, "e"), OpenEditorHere);
    map.bind(Combo::new(Modifiers::command_shift(), "b"), ToggleFileTree);
    map.bind(Combo::new(Modifiers::command_shift(), "y"), ToggleEditorBundle);
    map.bind(Combo::new(cmd, "s"), Save);
    map.bind(Combo::new(cmd, "f"), Find);
    map.bind(Combo::new(cmd, "p"), QuickOpen);
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
    map.bind(Combo::new(cmd, CLEAR_TERMINAL_KEY), ClearTerminal);
    map.bind(Combo::new(Modifiers::control(), "c"), InterruptProcess);
    map.bind(Combo::new(Modifiers::command_shift(), "e"), EqualizePanes);
}

fn bind_pane_movement(map: &mut Keymap) {
    use CommandId::*;
    let cmd = Modifiers::command();
    for (direction, vim_key, arrow_key) in DIRECTION_KEYS {
        if vim_key != CLEAR_TERMINAL_KEY {
            map.bind(Combo::new(cmd, vim_key), FocusPane(direction));
        }
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
    map.bind(Combo::new(Modifiers::command_shift(), "left"), MoveTabLeft);
    map.bind(Combo::new(Modifiers::command_shift(), "right"), MoveTabRight);
    for index in 0..INDEXED_SLOTS {
        map.bind(Combo::new(cmd, slot_key(index)), SelectTab(index));
    }
}

fn bind_documents(map: &mut Keymap) {
    use CommandId::*;
    map.bind(Combo::new(Modifiers::control(), "tab"), NextDocument);
    map.bind(Combo::new(Modifiers::control() | Modifiers::shift(), "tab"), PrevDocument);
}

fn bind_workspaces(map: &mut Keymap) {
    use CommandId::*;
    let cmd_alt = Modifiers::command() | Modifiers::alt();
    map.bind(Combo::new(Modifiers::command_shift(), "n"), NewWorkspace);
    map.bind(Combo::new(Modifiers::command_shift(), "r"), RenameWorkspace);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRegistry;

    fn cmd(key: &str) -> Combo {
        Combo::new(Modifiers::command(), key)
    }

    #[test]
    fn every_registry_command_has_a_unique_stable_key() {
        let registry = CommandRegistry::with_defaults();
        let mut keys: Vec<&str> = registry.list().iter().map(|command| command.id.key().expect("keyed")).collect();
        for command in registry.list() {
            assert_eq!(CommandId::from_key(command.id.key().unwrap()), Some(command.id));
        }
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), registry.list().len());
        assert_eq!(CommandId::SelectTab(0).key(), Some("select_tab_1"));
        assert_eq!(CommandId::SelectWorkspace(8).key(), Some("select_workspace_9"));
        assert_eq!(CommandId::SelectTab(9).key(), None);
    }

    #[test]
    fn every_default_binding_targets_a_registered_command() {
        let registry = CommandRegistry::with_defaults();
        for (combo, command) in Keymap::default().iter() {
            assert!(registry.get(*command).is_some(), "{} is bound to unregistered {command:?}", combo.binding_text());
        }
    }

    #[test]
    fn combo_text_parses_only_valid_shortcuts() {
        assert_eq!(Combo::parse("shift-cmd-k"), Some(Combo::new(Modifiers::command_shift(), "k")));
        assert_eq!(Combo::parse("ctrl-tab"), Some(Combo::new(Modifiers::control(), "tab")));
        assert_eq!(Combo::parse("f5"), Some(Combo::new(Modifiers::none(), "f5")));
        for invalid in ["", "k", "cmd-", "cmd-x-y", "shift-k", "cmd-K", "f99"] {
            assert_eq!(Combo::parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn overrides_parse_bindings_and_unbound_markers() {
        let overrides = ShortcutOverrides::parse(r#"{"split_right":"alt-cmd-d","copy":null}"#);
        let entries: Vec<(CommandId, Option<&Combo>)> = overrides.iter().collect();
        let split = Combo::new(Modifiers::command() | Modifiers::alt(), "d");
        assert_eq!(entries, [(CommandId::Copy, None), (CommandId::SplitRight, Some(&split))]);
        assert_eq!(ShortcutOverrides::parse(&overrides.to_json()), overrides);
    }

    #[test]
    fn overrides_ignore_untrusted_entries() {
        for payload in ["not json", "[1,2]", "\"cmd-d\"", r#"{"unknown_command":"cmd-d"}"#,
            r#"{"split_right":"cmd-"}"#, r#"{"split_right":"d"}"#, r#"{"split_right":42}"#] {
            assert!(ShortcutOverrides::parse(payload).is_empty(), "{payload}");
        }
        let oversized = format!(r#"{{"copy":null,"pad":"{}"}}"#, "x".repeat(MAX_OVERRIDES_BYTES));
        assert!(ShortcutOverrides::parse(&oversized).is_empty());
        let mixed = ShortcutOverrides::parse(r#"{"copy":"cmd-y","paste":"???","bogus":null}"#);
        assert_eq!(mixed.iter().collect::<Vec<_>>(), [(CommandId::Copy, Some(&cmd("y")))]);
    }

    #[test]
    fn override_replaces_every_default_combo_of_its_command() {
        let mut overrides = ShortcutOverrides::default();
        let new_combo = Combo::new(Modifiers::command() | Modifiers::alt(), "y");
        overrides.set(CommandId::FocusPane(Direction::Left), Some(new_combo.clone()));
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.combos_for(CommandId::FocusPane(Direction::Left)), [&new_combo]);
        assert_eq!(map.resolve(&cmd("h")), None);
        assert_eq!(map.resolve(&Combo::new(Modifiers::command() | Modifiers::alt(), "left")), None);
        assert!(map.conflicts().is_empty());
    }

    #[test]
    fn unbound_override_removes_the_command_and_keeps_the_rest() {
        let mut overrides = ShortcutOverrides::default();
        overrides.set(CommandId::Copy, None);
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.combo_for(CommandId::Copy), None);
        assert_eq!(map.resolve(&cmd("v")), Some(CommandId::Paste));
        assert_eq!(map.len(), Keymap::default().len() - 1);
    }

    #[test]
    fn override_takes_a_combo_from_a_command_without_overrides() {
        let mut overrides = ShortcutOverrides::default();
        overrides.set(CommandId::Save, Some(cmd("m")));
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.resolve(&cmd("m")), Some(CommandId::Save));
        assert_eq!(map.combo_for(CommandId::Minimize), None);
        assert!(map.conflicts().is_empty());
    }

    #[test]
    fn empty_overrides_match_the_default_keymap() {
        let map = Keymap::with_overrides(&ShortcutOverrides::default());
        let defaults = Keymap::default();
        assert_eq!(map.len(), defaults.len());
        assert!(defaults.iter().all(|(combo, id)| map.resolve(combo) == Some(*id)));
    }

    fn keystroke(modifiers: Modifiers, key: &str) -> Keystroke {
        Keystroke { modifiers, key: key.into(), key_char: None }
    }

    #[test]
    fn capture_requires_a_command_modifier_or_function_key() {
        let assign = |modifiers, key| Some(Capture::Assign(Combo::new(modifiers, key)));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::command(), "k")), assign(Modifiers::command(), "k"));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::alt(), "k")), assign(Modifiers::alt(), "k"));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::control(), "k")), assign(Modifiers::control(), "k"));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::none(), "f6")), assign(Modifiers::none(), "f6"));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::none(), "k")), None);
        assert_eq!(capture_shortcut(&keystroke(Modifiers::shift(), "k")), None);
        assert_eq!(capture_shortcut(&keystroke(Modifiers::command(), "")), None);
    }

    #[test]
    fn capture_cancels_on_escape_and_unbinds_on_plain_delete() {
        assert_eq!(capture_shortcut(&keystroke(Modifiers::none(), "escape")), Some(Capture::Cancel));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::command(), "escape")), Some(Capture::Cancel));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::none(), "backspace")), Some(Capture::Unbind));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::none(), "delete")), Some(Capture::Unbind));
        assert_eq!(capture_shortcut(&keystroke(Modifiers::command(), "backspace")),
            Some(Capture::Assign(cmd("backspace"))));
    }

    #[test]
    fn capture_records_the_physical_key_under_another_layout() {
        assert_eq!(capture_shortcut(&keystroke(Modifiers::command(), "\u{0635}")), Some(Capture::Assign(cmd("w"))));
    }

    #[test]
    fn assigning_a_taken_combo_moves_it_off_the_other_command() {
        let mut overrides = ShortcutOverrides::default();
        overrides.assign(CommandId::Save, cmd("t"));
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.resolve(&cmd("t")), Some(CommandId::Save));
        assert_eq!(map.combo_for(CommandId::NewTab), None);
        assert_eq!(map.resolve(&cmd("s")), None);
        assert!(map.conflicts().is_empty());
    }

    #[test]
    fn moving_one_of_several_combos_keeps_the_other() {
        let mut overrides = ShortcutOverrides::default();
        overrides.assign(CommandId::Save, cmd("h"));
        let map = Keymap::with_overrides(&overrides);
        let arrow = Combo::new(Modifiers::command() | Modifiers::alt(), "left");
        assert_eq!(map.combos_for(CommandId::FocusPane(Direction::Left)), [&arrow]);
        assert_eq!(map.resolve(&cmd("h")), Some(CommandId::Save));
    }

    #[test]
    fn reset_restores_defaults_and_wins_over_a_colliding_override() {
        let mut overrides = ShortcutOverrides::default();
        overrides.assign(CommandId::Save, cmd("t"));
        overrides.assign(CommandId::NewTab, cmd("y"));
        overrides.assign(CommandId::Copy, cmd("s"));
        overrides.reset(CommandId::NewTab);
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.resolve(&cmd("t")), Some(CommandId::NewTab));
        assert_eq!(map.combo_for(CommandId::Save), None);
        assert_eq!(map.resolve(&cmd("s")), Some(CommandId::Copy));
        overrides.reset(CommandId::Save);
        overrides.reset(CommandId::Copy);
        assert!(overrides.is_empty());
        let map = Keymap::with_overrides(&overrides);
        assert_eq!(map.resolve(&cmd("s")), Some(CommandId::Save));
        assert_eq!(map.resolve(&cmd("c")), Some(CommandId::Copy));
    }
}
