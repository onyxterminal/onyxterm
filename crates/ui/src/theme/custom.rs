use super::hsv::Rgb;
use super::{hex_to_hsla, Theme, ThemeKind};
use crate::text_field::TextField;
use gpui::{Hsla, Rgba};
use serde::{Deserialize, Serialize};

const HEX_PREFIX: char = '#';
const HEX_COLOR_CHARS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorField {
    Background, Chrome, Surface, Text, MutedText, Accent,
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
}

impl ColorField {
    pub const APPEARANCE: [Self; 6] = [
        Self::Background, Self::Chrome, Self::Surface, Self::Text, Self::MutedText, Self::Accent,
    ];
    pub const TERMINAL: [Self; 8] = [
        Self::Black, Self::Red, Self::Green, Self::Yellow,
        Self::Blue, Self::Magenta, Self::Cyan, Self::White,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Background => "Pane background", Self::Chrome => "Window background",
            Self::Surface => "Menus and dialogs", Self::Text => "Text",
            Self::MutedText => "Muted text", Self::Accent => "Accent",
            Self::Black => "ANSI black", Self::Red => "ANSI red",
            Self::Green => "ANSI green", Self::Yellow => "ANSI yellow",
            Self::Blue => "ANSI blue", Self::Magenta => "ANSI magenta",
            Self::Cyan => "ANSI cyan", Self::White => "ANSI white",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomColors {
    pub background: String,
    pub chrome: String,
    pub surface: String,
    pub text: String,
    pub muted_text: String,
    pub accent: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
}

impl CustomColors {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            background: hex(theme.surface.canvas), chrome: hex(theme.surface.window),
            surface: hex(theme.surface.raised), text: hex(theme.text.default),
            muted_text: hex(theme.text.muted), accent: hex(theme.accent.primary),
            black: hex(theme.ansi.black), red: hex(theme.ansi.red),
            green: hex(theme.ansi.green), yellow: hex(theme.ansi.yellow),
            blue: hex(theme.ansi.blue), magenta: hex(theme.ansi.magenta),
            cyan: hex(theme.ansi.cyan), white: hex(theme.ansi.white),
        }
    }

    pub fn get(&self, field: ColorField) -> &str {
        match field {
            ColorField::Background => &self.background, ColorField::Chrome => &self.chrome,
            ColorField::Surface => &self.surface, ColorField::Text => &self.text,
            ColorField::MutedText => &self.muted_text, ColorField::Accent => &self.accent,
            ColorField::Black => &self.black, ColorField::Red => &self.red,
            ColorField::Green => &self.green, ColorField::Yellow => &self.yellow,
            ColorField::Blue => &self.blue, ColorField::Magenta => &self.magenta,
            ColorField::Cyan => &self.cyan, ColorField::White => &self.white,
        }
    }

    pub fn set(&mut self, field: ColorField, value: &str) -> Result<(), &'static str> {
        let value = value.trim();
        if !valid_hex(value) { return Err("Use a color like #RRGGBB"); }
        let target = match field {
            ColorField::Background => &mut self.background, ColorField::Chrome => &mut self.chrome,
            ColorField::Surface => &mut self.surface, ColorField::Text => &mut self.text,
            ColorField::MutedText => &mut self.muted_text, ColorField::Accent => &mut self.accent,
            ColorField::Black => &mut self.black, ColorField::Red => &mut self.red,
            ColorField::Green => &mut self.green, ColorField::Yellow => &mut self.yellow,
            ColorField::Blue => &mut self.blue, ColorField::Magenta => &mut self.magenta,
            ColorField::Cyan => &mut self.cyan, ColorField::White => &mut self.white,
        };
        *target = value.to_ascii_uppercase();
        Ok(())
    }

    pub fn is_valid(&self) -> bool {
        ColorField::APPEARANCE.into_iter().chain(ColorField::TERMINAL)
            .all(|field| valid_hex(self.get(field)))
    }

    pub fn apply(&self, theme: &mut Theme) {
        let color = |field| hex_to_hsla(self.get(field), "custom color").expect("validated custom color");
        theme.surface.canvas = color(ColorField::Background);
        theme.surface.sunken = theme.surface.canvas;
        theme.surface.settings_window = theme.surface.canvas;
        theme.surface.window = color(ColorField::Chrome);
        theme.surface.raised = color(ColorField::Surface);
        theme.chrome.menu_fill = theme.surface.raised;
        theme.text.default = color(ColorField::Text);
        theme.text.muted = color(ColorField::MutedText);
        theme.text.subtle = theme.text.muted;
        theme.accent.primary = color(ColorField::Accent);
        theme.accent.primary_hover = shifted_lightness(theme.accent.primary, 0.08);
        theme.accent.primary_active = shifted_lightness(theme.accent.primary, -0.08);
        theme.border.focus = theme.accent.primary.opacity(theme.border.focus.a);
        theme.accent.selection = theme.accent.primary.opacity(theme.accent.selection.a);
        theme.status.starting = theme.accent.primary;
        let accent_rgb: Rgba = theme.accent.primary.into();
        let brightness = 0.2126 * accent_rgb.r + 0.7152 * accent_rgb.g + 0.0722 * accent_rgb.b;
        theme.text.on_accent = if brightness > 0.57 { gpui::rgb(0x17191D).into() }
            else { gpui::rgb(0xFFFFFF).into() };
        theme.ansi.black = color(ColorField::Black);
        theme.ansi.red = color(ColorField::Red);
        theme.ansi.green = color(ColorField::Green);
        theme.ansi.yellow = color(ColorField::Yellow);
        theme.ansi.blue = color(ColorField::Blue);
        theme.ansi.magenta = color(ColorField::Magenta);
        theme.ansi.cyan = color(ColorField::Cyan);
        theme.ansi.white = color(ColorField::White);
        theme.name = "custom".into();
    }
}

fn shifted_lightness(mut color: Hsla, shift: f32) -> Hsla {
    color.l = (color.l + shift).clamp(0.0, 1.0);
    color
}

pub fn preset_colors(field: ColorField) -> Vec<Rgb> {
    let mut colors = Vec::new();
    for kind in ThemeKind::ALL.into_iter().filter(|kind| *kind != ThemeKind::Custom) {
        let preset = CustomColors::from_theme(&Theme::load(kind).expect("built-in theme must load"));
        let color = Rgb::from_hex(preset.get(field)).expect("built-in colors are valid hex");
        if !colors.contains(&color) { colors.push(color); }
    }
    colors
}

pub fn hex_color_field(saved: &str) -> TextField {
    TextField::new(saved).with_filter(is_hex_color_char).with_max_chars(HEX_COLOR_CHARS).with_all_selected()
}

pub fn normalized_hex(typed: &str) -> String {
    format!("{HEX_PREFIX}{}", typed.trim().trim_start_matches(HEX_PREFIX))
}

fn is_hex_color_char(typed: char) -> bool {
    typed == HEX_PREFIX || typed.is_ascii_hexdigit()
}

fn valid_hex(value: &str) -> bool {
    Rgb::from_hex(value).is_some()
}

fn hex(value: Hsla) -> String {
    let rgba: Rgba = value.into();
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_colors_roundtrip_and_apply() {
        let mut custom = CustomColors::from_theme(&Theme::load(ThemeKind::Nord).unwrap());
        custom.set(ColorField::Accent, "#ff00aa").unwrap();
        assert!(custom.is_valid());
        let encoded = serde_json::to_string(&custom).unwrap();
        let decoded: CustomColors = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.get(ColorField::Accent), "#FF00AA");
        let mut theme = Theme::load(ThemeKind::Nord).unwrap();
        decoded.apply(&mut theme);
        assert_eq!(theme.name, "custom");
        assert_eq!(hex(theme.accent.primary), "#FF00AA");
        assert_eq!(theme.border.focus, theme.accent.primary.opacity(theme.border.focus.a));
        assert_eq!(theme.accent.selection.h, theme.accent.primary.h);
    }

    #[test]
    fn presets_list_each_builtin_value_once() {
        let presets = preset_colors(ColorField::Accent);
        let nord = Theme::load(ThemeKind::Nord).unwrap();
        assert!(presets.contains(&Rgb::from_hex(&hex(nord.accent.primary)).unwrap()));
        assert!(presets.len() < ThemeKind::ALL.len());
        assert!(presets.iter().enumerate().all(|(index, color)| !presets[..index].contains(color)));
    }

    #[test]
    fn rejects_invalid_colors() {
        let mut custom = CustomColors::from_theme(&Theme::load_default().unwrap());
        assert!(custom.set(ColorField::Text, "#12GG00").is_err());
        assert!(custom.set(ColorField::Text, "transparent").is_err());
    }
}
