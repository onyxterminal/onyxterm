#![deny(warnings)]

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
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: ThemeKind::Light, custom_base: ThemeKind::Dark,
            custom_colors: CustomColors::from_theme(&Theme::load(ThemeKind::Dark).expect("built-in dark theme")),
            font_family: "JetBrains Mono".into(), font_size: 13.0,
            ui_scale: 1.0, dim_unfocused_panes: true,
            editor_bundle_side: BundleSide::Right, editor_bundle_size: 0.4 }
    }
}

impl Settings {
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut settings = Self::default();
        for (key, value) in pairs {
            match key.as_str() {
                THEME_KEY => settings.theme = ThemeKind::from_key(value),
                CUSTOM_BASE_KEY => {
                    let base = ThemeKind::from_key(value);
                    settings.custom_base = if base == ThemeKind::Custom { ThemeKind::Dark } else { base };
                }
                CUSTOM_COLORS_KEY => {
                    if let Ok(colors) = serde_json::from_str::<CustomColors>(value) {
                        if colors.is_valid() { settings.custom_colors = colors; }
                    }
                }
                FONT_KEY if !value.trim().is_empty() && value.len() <= 128 => settings.font_family = value.clone(),
                FONT_SIZE_KEY => settings.font_size = bounded(value, 10.0, 28.0, settings.font_size),
                UI_SCALE_KEY => settings.ui_scale = bounded(value, 0.75, 1.5, settings.ui_scale),
                DIM_UNFOCUSED_KEY => settings.dim_unfocused_panes = value != "false",
                EDITOR_BUNDLE_SIDE_KEY => settings.editor_bundle_side = BundleSide::from_key(value),
                EDITOR_BUNDLE_SIZE_KEY => settings.editor_bundle_size = bounded(value, 0.1, 0.9, settings.editor_bundle_size),
                _ => {}
            }
        }
        settings
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
