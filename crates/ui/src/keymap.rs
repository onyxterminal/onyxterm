#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandId;
use gpui::Modifiers;
use std::collections::HashMap;

pub type KeyCode = String;

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

    pub fn conflicts(&self) -> Vec<(Combo, CommandId, CommandId)> {
        self.conflicts.clone()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Combo, &CommandId)> {
        self.bindings.iter()
    }
}

impl Default for Keymap {
    fn default() -> Self {
        use CommandId::*;
        let cmd_alt = Modifiers::command() | Modifiers::alt();
        let mut map = Self::new();
        map.bind(Combo::new(Modifiers::command(), "t"), NewTab);
        map.bind(Combo::new(Modifiers::command(), "w"), CloseActive);
        map.bind(Combo::new(Modifiers::command(), "k"), OpenPalette);
        map.bind(
            Combo::new(Modifiers::command_shift(), "]"),
            NextTab,
        );
        map.bind(
            Combo::new(Modifiers::command_shift(), "["),
            PrevTab,
        );
        map.bind(Combo::new(cmd_alt, "right"), SplitRight);
        map.bind(Combo::new(cmd_alt, "down"), SplitDown);
        map.bind(Combo::new(cmd_alt, "up"), FocusAdjacent);
        map.bind(Combo::new(cmd_alt, "left"), FocusAdjacent);
        map.bind(
            Combo::new(Modifiers::command_shift(), "m"),
            MaximizePane,
        );
        map
    }
}
