#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandRegistry;
use crate::keymap::Keymap;
use crate::theme::Theme;
use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteChoice {
    pub key: String,
    pub title: String,
}

#[derive(Debug)]
pub struct AppState {
    pub theme: Theme,
    pub settings: Settings,
    pub settings_visible: bool,
    pub settings_font_field: Option<String>,
    pub settings_error: Option<String>,
    pub registry: CommandRegistry,
    pub keymap: Keymap,
    pub palette_visible: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    pub palette_choices: Option<Vec<PaletteChoice>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::with_settings(Settings::default())
    }

    pub fn with_settings(settings: Settings) -> Self {
        Self {
            theme: settings.theme_tokens(),
            settings,
            settings_visible: false,
            settings_font_field: None,
            settings_error: None,
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
