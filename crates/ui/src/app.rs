#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandRegistry;
use crate::keymap::Keymap;
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteChoice {
    pub key: String,
    pub title: String,
}

#[derive(Debug)]
pub struct AppState {
    pub theme: Theme,
    pub registry: CommandRegistry,
    pub keymap: Keymap,
    pub palette_visible: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    pub palette_choices: Option<Vec<PaletteChoice>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            theme: Theme::load_default().expect("default theme must load"),
            registry: CommandRegistry::with_defaults(),
            keymap: Keymap::default(),
            palette_visible: false,
            palette_query: String::new(),
            palette_selection: 0,
            palette_choices: None,
        }
    }

    pub fn filtered_commands(&self) -> Vec<crate::command::CommandId> {
        self.registry
            .search(&self.palette_query)
            .into_iter()
            .map(|c| c.id)
            .collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
