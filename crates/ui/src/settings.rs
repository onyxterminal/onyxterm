#![deny(warnings)]

use crate::theme::{Theme, ThemeKind};

pub const THEME_KEY: &str = "appearance.theme";
pub const FONT_KEY: &str = "terminal.font_family";
pub const FONT_SIZE_KEY: &str = "terminal.font_size";
pub const UI_SCALE_KEY: &str = "appearance.ui_scale";
pub const DIM_UNFOCUSED_KEY: &str = "appearance.dim_unfocused_panes";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeKind,
    pub font_family: String,
    pub font_size: f32,
    pub ui_scale: f32,
    pub dim_unfocused_panes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: ThemeKind::Light, font_family: "ui-monospace".into(), font_size: 13.0,
            ui_scale: 1.0, dim_unfocused_panes: true }
    }
}

impl Settings {
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut settings = Self::default();
        for (key, value) in pairs {
            match key.as_str() {
                THEME_KEY => settings.theme = ThemeKind::from_key(value),
                FONT_KEY if !value.trim().is_empty() && value.len() <= 128 => settings.font_family = value.clone(),
                FONT_SIZE_KEY => settings.font_size = bounded(value, 10.0, 28.0, settings.font_size),
                UI_SCALE_KEY => settings.ui_scale = bounded(value, 0.75, 1.5, settings.ui_scale),
                DIM_UNFOCUSED_KEY => settings.dim_unfocused_panes = value != "false",
                _ => {}
            }
        }
        settings
    }

    pub fn theme_tokens(&self) -> Theme {
        let mut theme = Theme::load(self.theme).expect("built-in theme must load");
        theme.font.family = self.font_family.clone();
        theme.font.size_px = self.font_size;
        theme.scale_ui(self.ui_scale);
        theme.island.dim_unfocused = self.dim_unfocused_panes;
        theme
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
    fn invalid_persisted_sizes_use_safe_defaults() {
        let settings = Settings::from_pairs(&[
            (FONT_SIZE_KEY.into(), "NaN".into()),
            (UI_SCALE_KEY.into(), "25".into()),
        ]);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn font_picker_filters_and_sorts_installed_families() {
        let installed = vec!["Menlo".into(), "Arial".into(), "SF Mono".into(), "menlo".into()];
        assert_eq!(filtered_fonts(&installed, "mono"), vec!["SF Mono"]);
        assert_eq!(filtered_fonts(&installed, ""), vec!["Arial", "Menlo", "SF Mono"]);
    }
}
