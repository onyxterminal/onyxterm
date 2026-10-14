#![deny(warnings)]

use crate::keymap::ShortcutOverrides;
use crate::slider::SliderRange;
use crate::theme::custom::{ColorField, CustomColors};
use crate::theme::{Theme, ThemeKind};
use serde::{Deserialize, Serialize};

pub const THEME_KEY: &str = "appearance.theme";
pub const FONT_KEY: &str = "terminal.font_family";
pub const FONT_SIZE_KEY: &str = "terminal.font_size";
pub const UI_SCALE_KEY: &str = "appearance.ui_scale";
pub const DIM_UNFOCUSED_KEY: &str = "appearance.dim_unfocused_panes";
pub const CUSTOM_BASE_KEY: &str = "theme.custom.base";
pub const CUSTOM_COLORS_KEY: &str = "theme.custom.colors";
pub const EDITOR_BUNDLE_SIDE_KEY: &str = "editor.bundle_side";
pub const EDITOR_BUNDLE_SIZE_KEY: &str = "editor.bundle_size";
pub const SHORTCUTS_KEY: &str = "keyboard.bindings";
pub const BUNDLE_SIZE_MIN: f32 = 0.1;
pub const BUNDLE_SIZE_MAX: f32 = 0.9;
pub const UI_SCALE_PERCENT: SliderRange = SliderRange { min: 75.0, max: 150.0, step: 5.0 };
pub const BUNDLE_SIZE_PERCENT: SliderRange = SliderRange { min: 10.0, max: 90.0, step: 5.0 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl BundleSide {
    pub fn from_key(key: &str) -> Self {
        match key {
            "left" => Self::Left,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            _ => Self::Right,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsPane {
    #[default]
    Appearance,
    Terminal,
    Editor,
    Keyboard,
}

impl SettingsPane {
    pub const ALL: [Self; 4] = [Self::Appearance, Self::Terminal, Self::Editor, Self::Keyboard];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Keyboard => "Keyboard",
        }
    }

    pub fn setting_keys(self) -> &'static [&'static str] {
        match self {
            Self::Appearance => &[THEME_KEY, UI_SCALE_KEY, DIM_UNFOCUSED_KEY],
            Self::Terminal => &[FONT_KEY, FONT_SIZE_KEY],
            Self::Editor => &[EDITOR_BUNDLE_SIDE_KEY, EDITOR_BUNDLE_SIZE_KEY],
            Self::Keyboard => &[SHORTCUTS_KEY],
        }
    }

    pub fn default_pairs(self) -> Vec<(&'static str, String)> {
        let defaults = Settings::default();
        self.setting_keys().iter()
            .filter_map(|key| defaults.persisted_value(key).map(|value| (*key, value)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeKind,
    pub custom_base: ThemeKind,
    pub custom_colors: CustomColors,
    pub font_family: String,
    pub font_size: f32,
    pub ui_scale: f32,
    pub dim_unfocused_panes: bool,
    pub editor_bundle_side: BundleSide,
    pub editor_bundle_size: f32,
    pub shortcut_overrides: ShortcutOverrides,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: ThemeKind::Light, custom_base: ThemeKind::Dark,
            custom_colors: CustomColors::from_theme(&Theme::load(ThemeKind::Dark).expect("built-in dark theme")),
            font_family: "JetBrains Mono".into(), font_size: 13.0,
            ui_scale: 1.0, dim_unfocused_panes: true,
            editor_bundle_side: BundleSide::Right, editor_bundle_size: 0.4,
            shortcut_overrides: ShortcutOverrides::default() }
    }
}

impl Settings {
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut settings = Self::default();
        for (key, value) in pairs {
            settings.apply_pair(key, value);
        }
        settings
    }

    pub fn apply_pair(&mut self, key: &str, value: &str) {
        match key {
            THEME_KEY => self.theme = ThemeKind::from_key(value),
            CUSTOM_BASE_KEY => {
                let base = ThemeKind::from_key(value);
                self.custom_base = if base == ThemeKind::Custom { ThemeKind::Dark } else { base };
            }
            CUSTOM_COLORS_KEY => {
                if let Ok(colors) = serde_json::from_str::<CustomColors>(value) {
                    if colors.is_valid() { self.custom_colors = colors; }
                }
            }
            FONT_KEY if !value.trim().is_empty() && value.len() <= 128 => self.font_family = value.to_string(),
            FONT_SIZE_KEY => self.font_size = bounded(value, 10.0, 28.0, self.font_size),
            UI_SCALE_KEY => self.ui_scale = bounded(value, 0.75, 1.5, self.ui_scale),
            DIM_UNFOCUSED_KEY => self.dim_unfocused_panes = value != "false",
            EDITOR_BUNDLE_SIDE_KEY => self.editor_bundle_side = BundleSide::from_key(value),
            EDITOR_BUNDLE_SIZE_KEY => {
                self.editor_bundle_size = bounded(value, BUNDLE_SIZE_MIN, BUNDLE_SIZE_MAX, self.editor_bundle_size);
            }
            SHORTCUTS_KEY => self.shortcut_overrides = ShortcutOverrides::parse(value),
            _ => {}
        }
    }

    pub fn persisted_value(&self, key: &str) -> Option<String> {
        match key {
            THEME_KEY => Some(self.theme.key().to_string()),
            FONT_KEY => Some(self.font_family.clone()),
            FONT_SIZE_KEY => Some(self.font_size.to_string()),
            UI_SCALE_KEY => Some(self.ui_scale.to_string()),
            DIM_UNFOCUSED_KEY => Some(self.dim_unfocused_panes.to_string()),
            EDITOR_BUNDLE_SIDE_KEY => Some(self.editor_bundle_side.key().to_string()),
            EDITOR_BUNDLE_SIZE_KEY => Some(self.editor_bundle_size.to_string()),
            SHORTCUTS_KEY => Some(self.shortcut_overrides.to_json()),
            _ => None,
        }
    }

    pub fn theme_tokens(&self) -> Theme {
        let base = if self.theme == ThemeKind::Custom { self.custom_base } else { self.theme };
        let mut theme = Theme::load(base).expect("built-in theme must load");
        if self.theme == ThemeKind::Custom { self.custom_colors.apply(&mut theme); }
        theme.font.family = self.font_family.clone();
        theme.font.size_px = self.font_size;
        theme.scale_ui(self.ui_scale);
        theme.island.dim_unfocused = self.dim_unfocused_panes;
        theme
    }

    pub fn customize_current_theme(&mut self) {
        if self.theme != ThemeKind::Custom {
            self.custom_base = self.theme;
            let base = Theme::load(self.theme).expect("built-in theme must load");
            self.custom_colors = CustomColors::from_theme(&base);
            self.theme = ThemeKind::Custom;
        }
    }

    pub fn reset_custom_colors(&mut self) {
        let base = Theme::load(self.custom_base).expect("built-in theme must load");
        self.custom_colors = CustomColors::from_theme(&base);
    }

    pub fn set_custom_color(&mut self, field: ColorField, value: &str) -> Result<(), &'static str> {
        self.custom_colors.set(field, value)
    }
}

fn bounded(value: &str, min: f32, max: f32, fallback: f32) -> f32 {
    value.parse::<f32>().ok().filter(|number| number.is_finite() && (min..=max).contains(number)).unwrap_or(fallback)
}

pub fn filtered_fonts(installed: &[String], query: &str) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    let mut fonts: Vec<String> = installed.iter()
        .filter(|name| name.to_lowercase().contains(&needle))
        .cloned().collect();
    fonts.sort_by_key(|name| name.to_lowercase());
    fonts.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_settings_apply_theme_font_and_scale() {
        let settings = Settings::from_pairs(&[
            (THEME_KEY.into(), "dark".into()),
            (FONT_KEY.into(), "Menlo".into()),
            (FONT_SIZE_KEY.into(), "16".into()),
            (UI_SCALE_KEY.into(), "1.2".into()),
            (DIM_UNFOCUSED_KEY.into(), "false".into()),
        ]);
        let theme = settings.theme_tokens();
        assert_eq!(theme.name, "onyx-dark");
        assert_eq!(theme.font.family, "Menlo");
        assert_eq!(theme.font.size_px, 16.0);
        assert_eq!(theme.layout.tab_strip_height_px, 38.0 * 1.2);
        assert!(!theme.island.dim_unfocused);
    }

    #[test]
    fn editor_bundle_settings_roundtrip() {
        for side in [BundleSide::Left, BundleSide::Right, BundleSide::Top, BundleSide::Bottom] {
            let encoded = serde_json::to_string(&side).unwrap();
            let restored: BundleSide = serde_json::from_str(&encoded).unwrap();
            assert_eq!(restored, side);
        }
        let settings = Settings::from_pairs(&[
            (EDITOR_BUNDLE_SIDE_KEY.into(), "left".into()),
            (EDITOR_BUNDLE_SIZE_KEY.into(), "0.6".into()),
        ]);
        assert_eq!(settings.editor_bundle_side, BundleSide::Left);
        assert_eq!(settings.editor_bundle_size, 0.6);
        let settings = Settings::from_pairs(&[
            (EDITOR_BUNDLE_SIDE_KEY.into(), "bottom".into()),
            (EDITOR_BUNDLE_SIZE_KEY.into(), "0.9".into()),
        ]);
        assert_eq!(settings.editor_bundle_side, BundleSide::Bottom);
        assert_eq!(settings.editor_bundle_size, 0.9);
    }

    #[test]
    fn settings_open_on_the_appearance_pane_by_default() {
        assert_eq!(SettingsPane::default(), SettingsPane::Appearance);
        assert_eq!(crate::app::AppState::new().settings_pane, SettingsPane::Appearance);
        let labels: Vec<&str> = SettingsPane::ALL.iter().map(|pane| pane.label()).collect();
        assert_eq!(labels, ["Appearance", "Terminal", "Editor", "Keyboard"]);
    }

    #[test]
    fn every_slider_position_persists_within_bounds() {
        for position in 0..=100 {
            let fraction = position as f32 / 100.0;
            let ui_scale = UI_SCALE_PERCENT.value_at(fraction) / 100.0;
            let bundle_size = BUNDLE_SIZE_PERCENT.value_at(fraction) / 100.0;
            let restored = Settings::from_pairs(&[
                (UI_SCALE_KEY.into(), ui_scale.to_string()),
                (EDITOR_BUNDLE_SIZE_KEY.into(), bundle_size.to_string()),
            ]);
            assert_eq!((restored.ui_scale, restored.editor_bundle_size), (ui_scale, bundle_size));
        }
        assert_eq!(BUNDLE_SIZE_PERCENT.value_at(BUNDLE_SIZE_PERCENT.fraction_of(62.0)), 60.0);
        assert_eq!(UI_SCALE_PERCENT.fraction_of(Settings::default().ui_scale * 100.0), 1.0 / 3.0);
    }

    #[test]
    fn pane_default_pairs_cover_only_that_panes_settings() {
        let pairs = |pane: SettingsPane| -> Vec<(&str, String)> { pane.default_pairs() };
        assert_eq!(pairs(SettingsPane::Appearance), [
            (THEME_KEY, "light".to_string()), (UI_SCALE_KEY, "1".to_string()), (DIM_UNFOCUSED_KEY, "true".to_string()),
        ]);
        assert_eq!(pairs(SettingsPane::Terminal), [
            (FONT_KEY, "JetBrains Mono".to_string()), (FONT_SIZE_KEY, "13".to_string()),
        ]);
        assert_eq!(pairs(SettingsPane::Editor), [
            (EDITOR_BUNDLE_SIDE_KEY, "right".to_string()), (EDITOR_BUNDLE_SIZE_KEY, "0.4".to_string()),
        ]);
        assert_eq!(pairs(SettingsPane::Keyboard), [(SHORTCUTS_KEY, "{}".to_string())]);
    }

    #[test]
    fn restoring_a_pane_resets_its_settings_and_keeps_the_rest() {
        let mut settings = Settings {
            theme: ThemeKind::Nord, ui_scale: 1.3, dim_unfocused_panes: false, font_family: "Menlo".into(),
            font_size: 18.0, editor_bundle_side: BundleSide::Left, editor_bundle_size: 0.7, ..Default::default()
        };
        for (key, value) in SettingsPane::Editor.default_pairs() {
            settings.apply_pair(key, &value);
        }
        assert_eq!(settings.editor_bundle_side, BundleSide::Right);
        assert_eq!(settings.editor_bundle_size, 0.4);
        assert_eq!(settings.font_family, "Menlo");
        assert_eq!(settings.theme, ThemeKind::Nord);
        for pane in [SettingsPane::Appearance, SettingsPane::Terminal] {
            for (key, value) in pane.default_pairs() {
                settings.apply_pair(key, &value);
            }
        }
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn restored_shortcut_overrides_drive_the_startup_keymap() {
        use crate::command::CommandId;
        use crate::keymap::Combo;
        use gpui::Modifiers;
        let settings = Settings::from_pairs(&[(SHORTCUTS_KEY.into(), r#"{"save":"alt-cmd-s","copy":null}"#.into())]);
        let state = crate::app::AppState::with_settings(settings);
        let save = Combo::new(Modifiers::command() | Modifiers::alt(), "s");
        assert_eq!(state.keymap.resolve(&save), Some(CommandId::Save));
        assert_eq!(state.keymap.resolve(&Combo::new(Modifiers::command(), "s")), None);
        assert_eq!(state.keymap.combo_for(CommandId::Copy), None);
        assert_eq!(state.settings.persisted_value(SHORTCUTS_KEY).unwrap(), r#"{"copy":null,"save":"alt-cmd-s"}"#);
    }

    #[test]
    fn invalid_persisted_sizes_use_safe_defaults() {
        let settings = Settings::from_pairs(&[
            (FONT_SIZE_KEY.into(), "NaN".into()),
            (UI_SCALE_KEY.into(), "25".into()),
            (EDITOR_BUNDLE_SIZE_KEY.into(), "1.5".into()),
        ]);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn font_picker_filters_and_sorts_installed_families() {
        let installed = vec!["Menlo".into(), "Arial".into(), "SF Mono".into(), "menlo".into()];
        assert_eq!(filtered_fonts(&installed, "mono"), vec!["SF Mono"]);
        assert_eq!(filtered_fonts(&installed, ""), vec!["Arial", "Menlo", "SF Mono"]);
    }

    #[test]
    fn every_builtin_theme_loads() {
        for kind in ThemeKind::ALL {
            assert_eq!(ThemeKind::from_key(kind.key()), kind);
            let theme = Theme::load(kind).unwrap();
            assert!(theme.surface.canvas != theme.text.default, "{:?} lacks contrast", kind);
        }
    }

    #[test]
    fn custom_theme_survives_settings_roundtrip() {
        let mut original = Settings { theme: ThemeKind::Nord, ..Default::default() };
        original.customize_current_theme();
        original.set_custom_color(ColorField::Background, "#123456").unwrap();
        original.set_custom_color(ColorField::Red, "#EF1234").unwrap();
        let stored = vec![
            (THEME_KEY.into(), original.theme.key().into()),
            (CUSTOM_BASE_KEY.into(), original.custom_base.key().into()),
            (CUSTOM_COLORS_KEY.into(), serde_json::to_string(&original.custom_colors).unwrap()),
        ];
        let restored = Settings::from_pairs(&stored);
        assert_eq!(restored.theme, ThemeKind::Custom);
        assert_eq!(restored.custom_base, ThemeKind::Nord);
        assert_eq!(restored.custom_colors, original.custom_colors);
        assert_eq!(restored.theme_tokens().name, "custom");
    }

    #[test]
    fn reset_restores_every_custom_color_from_the_base_theme() {
        let mut settings = Settings { theme: ThemeKind::Dracula, ..Default::default() };
        settings.customize_current_theme();
        let pristine = settings.custom_colors.clone();
        settings.set_custom_color(ColorField::Accent, "#00FF00").unwrap();
        settings.set_custom_color(ColorField::Blue, "#123456").unwrap();
        settings.reset_custom_colors();
        assert_eq!(settings.custom_colors, pristine);
        assert_eq!(settings.custom_base, ThemeKind::Dracula);
        assert_eq!(settings.theme, ThemeKind::Custom);
    }
}
