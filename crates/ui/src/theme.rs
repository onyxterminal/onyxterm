#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::{Hsla, Rgba, WindowBackgroundAppearance};
use serde::Deserialize;

const THEME_TOML: &str = include_str!("../../../assets/themes/github-light.toml");

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ThemeError {
    #[error("missing key {0}")]
    MissingKey(&'static str),
    #[error("hex parse failed for {key}: {value}")]
    HexParse { key: &'static str, value: String },
    #[error("toml parse failed: {0}")]
    Toml(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Deserialize)]
struct RawTheme {
    meta: RawMeta,
    surface: SurfaceTokensRaw,
    text: TextTokensRaw,
    border: BorderTokensRaw,
    accent: AccentTokensRaw,
    status: StatusTokensRaw,
    font: FontTokens,
    scrollback: ScrollbackTokens,
    ansi: AnsiTokensRaw,
    layout: LayoutTokens,
    input: InputTokens,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMeta {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SurfaceTokensRaw {
    canvas: String,
    subtle: String,
    raised: String,
    sunken: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TextTokensRaw {
    default: String,
    muted: String,
    subtle: String,
    inverse: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BorderTokensRaw {
    default: String,
    muted: String,
    focus: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AccentTokensRaw {
    primary: String,
    primary_hover: String,
    primary_active: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StatusTokensRaw {
    running: String,
    exited: String,
    failed: String,
    stopping: String,
    starting: String,
    unknown: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontTokens {
    pub family: String,
    pub fallback: String,
    pub size_px: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ScrollbackTokens {
    pub default_lines: u32,
    pub max_lines: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct AnsiTokensRaw {
    black: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    magenta: String,
    cyan: String,
    white: String,
    bright_black: String,
    bright_red: String,
    bright_green: String,
    bright_yellow: String,
    bright_blue: String,
    bright_magenta: String,
    bright_cyan: String,
    bright_white: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InputTokens {
    pub option_as_meta: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LayoutTokens {
    pub tab_strip_height_px: f32,
    pub pane_header_height_px: f32,
    pub border_radius_px: f32,
    pub gap_px: f32,
    pub island_radius_px: f32,
    pub island_gap_px: f32,
    pub window_margin_px: f32,
    pub island_padding_px: f32,
    pub traffic_light_left_px: f32,
    pub traffic_light_top_px: f32,
    pub traffic_lights_width_px: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceTokens {
    pub canvas: Hsla,
    pub subtle: Hsla,
    pub raised: Hsla,
    pub sunken: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct TextTokens {
    pub default: Hsla,
    pub muted: Hsla,
    pub subtle: Hsla,
    pub inverse: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct BorderTokens {
    pub default: Hsla,
    pub muted: Hsla,
    pub focus: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct AccentTokens {
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub primary_active: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusTokens {
    pub running: Hsla,
    pub exited: Hsla,
    pub failed: Hsla,
    pub stopping: Hsla,
    pub starting: Hsla,
    pub unknown: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub struct AnsiTokens {
    pub black: Hsla,
    pub red: Hsla,
    pub green: Hsla,
    pub yellow: Hsla,
    pub blue: Hsla,
    pub magenta: Hsla,
    pub cyan: Hsla,
    pub white: Hsla,
    pub bright_black: Hsla,
    pub bright_red: Hsla,
    pub bright_green: Hsla,
    pub bright_yellow: Hsla,
    pub bright_blue: Hsla,
    pub bright_magenta: Hsla,
    pub bright_cyan: Hsla,
    pub bright_white: Hsla,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub surface: SurfaceTokens,
    pub text: TextTokens,
    pub border: BorderTokens,
    pub accent: AccentTokens,
    pub status: StatusTokens,
    pub font: FontTokens,
    pub scrollback: ScrollbackTokens,
    pub ansi: AnsiTokens,
    pub layout: LayoutTokens,
    pub input: InputTokens,
}

impl Theme {
    pub fn load_default() -> Result<Self, ThemeError> {
        let raw: RawTheme = toml::from_str(THEME_TOML)?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawTheme) -> Result<Self, ThemeError> {
        Ok(Self {
            name: raw.meta.name,
            surface: SurfaceTokens {
                canvas: hex_to_hsla(&raw.surface.canvas, "surface.canvas")?,
                subtle: hex_to_hsla(&raw.surface.subtle, "surface.subtle")?,
                raised: hex_to_hsla(&raw.surface.raised, "surface.raised")?,
                sunken: hex_to_hsla(&raw.surface.sunken, "surface.sunken")?,
            },
            text: TextTokens {
                default: hex_to_hsla(&raw.text.default, "text.default")?,
                muted: hex_to_hsla(&raw.text.muted, "text.muted")?,
                subtle: hex_to_hsla(&raw.text.subtle, "text.subtle")?,
                inverse: hex_to_hsla(&raw.text.inverse, "text.inverse")?,
            },
            border: BorderTokens {
                default: hex_to_hsla(&raw.border.default, "border.default")?,
                muted: hex_to_hsla(&raw.border.muted, "border.muted")?,
                focus: hex_to_hsla(&raw.border.focus, "border.focus")?,
            },
            accent: AccentTokens {
                primary: hex_to_hsla(&raw.accent.primary, "accent.primary")?,
                primary_hover: hex_to_hsla(
                    &raw.accent.primary_hover,
                    "accent.primary_hover",
                )?,
                primary_active: hex_to_hsla(
                    &raw.accent.primary_active,
                    "accent.primary_active",
                )?,
            },
            status: StatusTokens {
                running: hex_to_hsla(&raw.status.running, "status.running")?,
                exited: hex_to_hsla(&raw.status.exited, "status.exited")?,
                failed: hex_to_hsla(&raw.status.failed, "status.failed")?,
                stopping: hex_to_hsla(&raw.status.stopping, "status.stopping")?,
                starting: hex_to_hsla(&raw.status.starting, "status.starting")?,
                unknown: hex_to_hsla(&raw.status.unknown, "status.unknown")?,
            },
            font: raw.font,
            scrollback: raw.scrollback,
            ansi: AnsiTokens {
                black: hex_to_hsla(&raw.ansi.black, "ansi.black")?,
                red: hex_to_hsla(&raw.ansi.red, "ansi.red")?,
                green: hex_to_hsla(&raw.ansi.green, "ansi.green")?,
                yellow: hex_to_hsla(&raw.ansi.yellow, "ansi.yellow")?,
                blue: hex_to_hsla(&raw.ansi.blue, "ansi.blue")?,
                magenta: hex_to_hsla(&raw.ansi.magenta, "ansi.magenta")?,
                cyan: hex_to_hsla(&raw.ansi.cyan, "ansi.cyan")?,
                white: hex_to_hsla(&raw.ansi.white, "ansi.white")?,
                bright_black: hex_to_hsla(&raw.ansi.bright_black, "ansi.bright_black")?,
                bright_red: hex_to_hsla(&raw.ansi.bright_red, "ansi.bright_red")?,
                bright_green: hex_to_hsla(&raw.ansi.bright_green, "ansi.bright_green")?,
                bright_yellow: hex_to_hsla(&raw.ansi.bright_yellow, "ansi.bright_yellow")?,
                bright_blue: hex_to_hsla(&raw.ansi.bright_blue, "ansi.bright_blue")?,
                bright_magenta: hex_to_hsla(&raw.ansi.bright_magenta, "ansi.bright_magenta")?,
                bright_cyan: hex_to_hsla(&raw.ansi.bright_cyan, "ansi.bright_cyan")?,
                bright_white: hex_to_hsla(&raw.ansi.bright_white, "ansi.bright_white")?,
            },
            layout: raw.layout,
            input: raw.input,
        })
    }

    pub fn canvas(&self) -> Hsla {
        self.surface.canvas
    }

    pub fn text_default(&self) -> Hsla {
        self.text.default
    }

    pub fn border_default(&self) -> Hsla {
        self.border.default
    }

    pub fn border_focus(&self) -> Hsla {
        self.border.focus
    }

    pub fn accent_color(&self) -> Hsla {
        self.accent.primary
    }

    pub fn window_background(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }
}

const SYSTEM_MONOSPACE: &str = "ui-monospace";
const GENERIC_MONOSPACE: &str = "monospace";
const SYSTEM_MONOSPACE_FAMILIES: [&str; 2] = ["SF Mono", "Menlo"];
pub const LAST_RESORT_FONT: &str = "Menlo";

impl FontTokens {
    pub fn family_candidates(&self) -> Vec<String> {
        std::iter::once(self.family.as_str())
            .chain(self.fallback.split(','))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .flat_map(|name| match name {
                SYSTEM_MONOSPACE => SYSTEM_MONOSPACE_FAMILIES.to_vec(),
                GENERIC_MONOSPACE => vec![LAST_RESORT_FONT],
                other => vec![other],
            })
            .map(str::to_string)
            .collect()
    }

    pub fn resolve_family(&self, available: &[String]) -> String {
        self.family_candidates()
            .into_iter()
            .find(|candidate| available.iter().any(|name| name == candidate))
            .unwrap_or_else(|| LAST_RESORT_FONT.to_string())
    }
}

const HEX_WITH_ALPHA_LEN: usize = 8;

pub fn hex_to_hsla(hex: &str, key: &'static str) -> Result<Hsla, ThemeError> {
    let stripped = hex.strip_prefix('#').unwrap_or(hex);
    let parsed = u32::from_str_radix(stripped, 16).map_err(|_| ThemeError::HexParse {
        key,
        value: hex.to_string(),
    })?;
    let rgba: Rgba = match stripped.len() {
        HEX_WITH_ALPHA_LEN => gpui::rgba(parsed),
        _ => gpui::rgb(parsed),
    };
    Ok(rgba.into())
}
