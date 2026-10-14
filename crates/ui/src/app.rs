#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::color_picker::ColorPicker;
use crate::command::CommandRegistry;
use crate::keymap::Keymap;
use crate::theme::Theme;
use crate::settings::{Settings, SettingsPane};
use crate::text_field::TextField;

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
    pub settings_pane: SettingsPane,
    pub font_picker_visible: bool,
    pub font_picker_query: TextField,
    pub font_picker_selection: usize,
    pub theme_editor_visible: bool,
    pub theme_editor_terminal_colors: bool,
    pub color_picker: Option<ColorPicker>,
    pub theme_color_input: TextField,
    pub settings_error: Option<String>,
    pub registry: CommandRegistry,
    pub keymap: Keymap,
    pub palette_visible: bool,
    pub palette_query: TextField,
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
            keymap: Keymap::with_overrides(&settings.shortcut_overrides),
            settings,
            settings_visible: false,
            settings_pane: SettingsPane::default(),
            font_picker_visible: false,
            font_picker_query: TextField::default(),
            font_picker_selection: 0,
            theme_editor_visible: false,
            theme_editor_terminal_colors: false,
            color_picker: None,
            theme_color_input: TextField::default(),
            settings_error: None,
            registry: CommandRegistry::with_defaults(),
            palette_visible: false,
            palette_query: TextField::default(),
            palette_selection: 0,
            palette_choices: None,
        }
    }

    pub fn filtered_commands(&self) -> Vec<crate::command::CommandId> {
        self.registry
            .search(self.palette_query.text())
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
