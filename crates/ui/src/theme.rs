#![deny(warnings)]
#![forbid(unsafe_code)]

pub mod icons;
pub mod menu_chrome;
pub mod custom;
pub mod hsv;

use gpui::{point, px, BoxShadow, Hsla, Rgba, WindowBackgroundAppearance};
use serde::{Deserialize, Deserializer};

const LIGHT_THEME_TOML: &str = include_str!("../../../assets/themes/apple-light.toml");
const DARK_THEME_TOML: &str = include_str!("../../../assets/themes/onyx-dark.toml");
const NORD_THEME_TOML: &str = include_str!("../../../assets/themes/nord.toml");
const DRACULA_THEME_TOML: &str = include_str!("../../../assets/themes/dracula.toml");
const SOLARIZED_DARK_THEME_TOML: &str = include_str!("../../../assets/themes/solarized-dark.toml");
const SOLARIZED_LIGHT_THEME_TOML: &str = include_str!("../../../assets/themes/solarized-light.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind { Light, Dark, Nord, Dracula, SolarizedDark, SolarizedLight, Custom }

impl ThemeKind {
    pub const ALL: [Self; 7] = [Self::Light, Self::Dark, Self::Nord, Self::Dracula,
        Self::SolarizedDark, Self::SolarizedLight, Self::Custom];

    pub fn key(self) -> &'static str {
        match self {
            Self::Light => "light", Self::Dark => "dark", Self::Nord => "nord",
            Self::Dracula => "dracula", Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light", Self::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL.into_iter().find(|kind| kind.key() == key).unwrap_or(Self::Light)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Apple Light", Self::Dark => "Onyx Dark", Self::Nord => "Nord",
            Self::Dracula => "Dracula", Self::SolarizedDark => "Solarized Dark",
            Self::SolarizedLight => "Solarized Light", Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance { Light, Dark }

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ThemeError {
    #[error("missing key {0}")]
    MissingKey(&'static str),
    #[error("color parse failed for {key}: {value}")]
    ColorParse { key: &'static str, value: String },
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
    chrome: ChromeTokens,
    ui_font: UiFontTokens,
    top_bar: TopBarTokens,
    pane_header: PaneHeaderTokens,
    menu: MenuTokens,
    palette: PaletteTokens,
    dialog: DialogTokens,
    island: IslandTokens,
    editor: EditorTokens,
    syntax: SyntaxTokens,
    shadow: ShadowTokens,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMeta {
    name: String,
    kind: Appearance,
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
    idle: String,
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
    pub min_pane_px: f32,
    pub animation_ms: f32,
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
    pub idle: Hsla,
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
    pub appearance: Appearance,
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
    pub chrome: ChromeTokens,
    pub ui_font: UiFontTokens,
    pub top_bar: TopBarTokens,
    pub pane_header: PaneHeaderTokens,
    pub menu: MenuTokens,
    pub palette: PaletteTokens,
    pub dialog: DialogTokens,
    pub island: IslandTokens,
    pub editor: EditorTokens,
    pub syntax: SyntaxTokens,
    pub shadow: ShadowTokens,
}

impl Theme {
    pub fn load_default() -> Result<Self, ThemeError> {
        Self::load(ThemeKind::Light)
    }

    pub fn load(kind: ThemeKind) -> Result<Self, ThemeError> {
        let source = match kind {
            ThemeKind::Light => LIGHT_THEME_TOML,
            ThemeKind::Dark | ThemeKind::Custom => DARK_THEME_TOML,
            ThemeKind::Nord => NORD_THEME_TOML,
            ThemeKind::Dracula => DRACULA_THEME_TOML,
            ThemeKind::SolarizedDark => SOLARIZED_DARK_THEME_TOML,
            ThemeKind::SolarizedLight => SOLARIZED_LIGHT_THEME_TOML,
        };
        let raw: RawTheme = toml::from_str(source)?;
        Self::from_raw(raw)
    }

    pub fn scale_ui(&mut self, scale: f32) {
        macro_rules! scale { ($object:expr, $($field:ident),+ $(,)?) => { $( $object.$field *= scale; )+ }; }
        scale!(self.ui_font, tab_px, button_px, pane_title_px, menu_px, shortcut_px,
            palette_input_px, hint_px, section_label_px, dialog_title_px, dialog_body_px);
        scale!(self.layout, tab_strip_height_px, pane_header_height_px, border_radius_px,
            gap_px, island_radius_px, island_gap_px, window_margin_px, min_pane_px,
            traffic_light_left_px, traffic_light_top_px, traffic_lights_width_px);
        scale!(self.top_bar, leading_gap_px, control_height_px, control_radius_px, logo_px,
            separator_width_px, separator_height_px, tab_padding_x_px, tab_gap_px, item_gap_px);
        scale!(self.pane_header, padding_x_px, status_dot_px, icon_button_px, icon_button_radius_px, icon_px);
        scale!(self.menu, width_px, radius_px, padding_px, border_px, row_height_px, row_radius_px,
            row_padding_x_px, offset_px, section_label_height_px);
        scale!(self.palette, width_px, top_px, radius_px, border_px, search_height_px,
            search_icon_px, row_height_px, row_radius_px, footer_height_px);
        scale!(self.dialog, width_px, radius_px, button_height_px, button_radius_px);
        scale!(self.island, focus_ring_px, unfocused_border_px, pane_padding_x_px,
            pane_padding_top_px, pane_padding_bottom_px);
        scale!(self.editor, gutter_padding_px, cursor_width_px);
        for shadow in self.shadow.card.iter_mut().chain(&mut self.shadow.tab)
            .chain(&mut self.shadow.menu).chain(&mut self.shadow.palette) {
            scale!(shadow, x, y, blur, spread);
        }
    }

    fn from_raw(raw: RawTheme) -> Result<Self, ThemeError> {
        Ok(Self {
            name: raw.meta.name,
            appearance: raw.meta.kind,
            surface: SurfaceTokens {
                canvas: parse_color(&raw.surface.canvas, "surface.canvas")?,
                subtle: parse_color(&raw.surface.subtle, "surface.subtle")?,
                raised: parse_color(&raw.surface.raised, "surface.raised")?,
                sunken: parse_color(&raw.surface.sunken, "surface.sunken")?,
            },
            text: TextTokens {
                default: parse_color(&raw.text.default, "text.default")?,
                muted: parse_color(&raw.text.muted, "text.muted")?,
                subtle: parse_color(&raw.text.subtle, "text.subtle")?,
                inverse: parse_color(&raw.text.inverse, "text.inverse")?,
            },
            border: BorderTokens {
                default: parse_color(&raw.border.default, "border.default")?,
                muted: parse_color(&raw.border.muted, "border.muted")?,
                focus: parse_color(&raw.border.focus, "border.focus")?,
            },
            accent: AccentTokens {
                primary: parse_color(&raw.accent.primary, "accent.primary")?,
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
                running: parse_color(&raw.status.running, "status.running")?,
                exited: parse_color(&raw.status.exited, "status.exited")?,
                failed: parse_color(&raw.status.failed, "status.failed")?,
                stopping: parse_color(&raw.status.stopping, "status.stopping")?,
                starting: parse_color(&raw.status.starting, "status.starting")?,
                unknown: parse_color(&raw.status.unknown, "status.unknown")?,
                idle: parse_color(&raw.status.idle, "status.idle")?,
            },
            font: raw.font,
            scrollback: raw.scrollback,
            ansi: AnsiTokens {
                black: parse_color(&raw.ansi.black, "ansi.black")?,
                red: parse_color(&raw.ansi.red, "ansi.red")?,
                green: parse_color(&raw.ansi.green, "ansi.green")?,
                yellow: parse_color(&raw.ansi.yellow, "ansi.yellow")?,
                blue: parse_color(&raw.ansi.blue, "ansi.blue")?,
                magenta: parse_color(&raw.ansi.magenta, "ansi.magenta")?,
                cyan: parse_color(&raw.ansi.cyan, "ansi.cyan")?,
                white: parse_color(&raw.ansi.white, "ansi.white")?,
                bright_black: parse_color(&raw.ansi.bright_black, "ansi.bright_black")?,
                bright_red: parse_color(&raw.ansi.bright_red, "ansi.bright_red")?,
                bright_green: parse_color(&raw.ansi.bright_green, "ansi.bright_green")?,
                bright_yellow: parse_color(&raw.ansi.bright_yellow, "ansi.bright_yellow")?,
                bright_blue: parse_color(&raw.ansi.bright_blue, "ansi.bright_blue")?,
                bright_magenta: parse_color(&raw.ansi.bright_magenta, "ansi.bright_magenta")?,
                bright_cyan: parse_color(&raw.ansi.bright_cyan, "ansi.bright_cyan")?,
                bright_white: parse_color(&raw.ansi.bright_white, "ansi.bright_white")?,
            },
            layout: raw.layout,
            input: raw.input,
            chrome: raw.chrome,
            ui_font: raw.ui_font,
            top_bar: raw.top_bar,
            pane_header: raw.pane_header,
            menu: raw.menu,
            palette: raw.palette,
            dialog: raw.dialog,
            island: raw.island,
            editor: raw.editor,
            syntax: raw.syntax,
            shadow: raw.shadow,
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

    pub fn top_bar_leading_inset_px(&self, is_fullscreen: bool) -> f32 {
        match is_fullscreen {
            true => self.layout.window_margin_px,
            false => self.layout.traffic_lights_width_px + self.top_bar.leading_gap_px,
        }
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

    pub fn resolve_family(&self, is_installed: impl Fn(&str) -> bool) -> String {
        self.family_candidates()
            .into_iter()
            .find(|candidate| is_installed(candidate))
            .unwrap_or_else(|| LAST_RESORT_FONT.to_string())
    }
}

const HEX_WITH_ALPHA_LEN: usize = 8;
const RGBA_PREFIX: &str = "rgba(";
const RGBA_SUFFIX: &str = ")";
const RGBA_CHANNELS: usize = 4;
const CHANNEL_MAX: f32 = 255.0;

pub fn parse_color(value: &str, key: &'static str) -> Result<Hsla, ThemeError> {
    match value.trim().strip_prefix(RGBA_PREFIX) {
        Some(body) => rgba_to_hsla(body, value, key),
        None => hex_to_hsla(value, key),
    }
}

fn rgba_to_hsla(body: &str, value: &str, key: &'static str) -> Result<Hsla, ThemeError> {
    let parse_error = || ThemeError::ColorParse { key, value: value.to_string() };
    let channels: Vec<f32> = body
        .strip_suffix(RGBA_SUFFIX)
        .ok_or_else(parse_error)?
        .split(',')
        .map(|channel| channel.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| parse_error())?;
    let [red, green, blue, alpha]: [f32; RGBA_CHANNELS] = channels.try_into().map_err(|_| parse_error())?;
    let rgba = Rgba { r: red / CHANNEL_MAX, g: green / CHANNEL_MAX, b: blue / CHANNEL_MAX, a: alpha };
    Ok(rgba.into())
}

fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Hsla, D::Error> {
    let value = String::deserialize(deserializer)?;
    parse_color(&value, "color").map_err(serde::de::Error::custom)
}

pub fn hex_to_hsla(hex: &str, key: &'static str) -> Result<Hsla, ThemeError> {
    let stripped = hex.strip_prefix('#').unwrap_or(hex);
    let parsed = u32::from_str_radix(stripped, 16).map_err(|_| ThemeError::ColorParse {
        key,
        value: hex.to_string(),
    })?;
    let rgba: Rgba = match stripped.len() {
        HEX_WITH_ALPHA_LEN => gpui::rgba(parsed),
        _ => gpui::rgb(parsed),
    };
    Ok(rgba.into())
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ChromeTokens {
    #[serde(deserialize_with = "color")]
    pub control_fill: Hsla,
    #[serde(deserialize_with = "color")]
    pub icon_hover_fill: Hsla,
    #[serde(deserialize_with = "color")]
    pub menu_fill: Hsla,
    #[serde(deserialize_with = "color")]
    pub menu_border: Hsla,
    #[serde(deserialize_with = "color")]
    pub palette_fill: Hsla,
    #[serde(deserialize_with = "color")]
    pub selected_shortcut: Hsla,
    #[serde(deserialize_with = "color")]
    pub destructive: Hsla,
    #[serde(deserialize_with = "color")]
    pub backdrop: Hsla,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiFontTokens {
    pub family: String,
    pub tab_px: f32,
    pub button_px: f32,
    pub pane_title_px: f32,
    pub menu_px: f32,
    pub shortcut_px: f32,
    pub palette_input_px: f32,
    pub hint_px: f32,
    pub section_label_px: f32,
    pub dialog_title_px: f32,
    pub dialog_body_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TopBarTokens {
    pub leading_gap_px: f32,
    pub control_height_px: f32,
    pub control_radius_px: f32,
    pub logo_px: f32,
    pub separator_width_px: f32,
    pub separator_height_px: f32,
    pub tab_padding_x_px: f32,
    pub tab_gap_px: f32,
    pub item_gap_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PaneHeaderTokens {
    pub padding_x_px: f32,
    pub status_dot_px: f32,
    pub icon_button_px: f32,
    pub icon_button_radius_px: f32,
    pub icon_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MenuTokens {
    pub width_px: f32,
    pub radius_px: f32,
    pub padding_px: f32,
    pub border_px: f32,
    pub row_height_px: f32,
    pub row_radius_px: f32,
    pub row_padding_x_px: f32,
    pub offset_px: f32,
    pub section_label_height_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PaletteTokens {
    pub width_px: f32,
    pub top_px: f32,
    pub radius_px: f32,
    pub border_px: f32,
    pub search_height_px: f32,
    pub search_icon_px: f32,
    pub row_height_px: f32,
    pub row_radius_px: f32,
    pub footer_height_px: f32,
    pub max_rows: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DialogTokens {
    pub width_px: f32,
    pub radius_px: f32,
    pub button_height_px: f32,
    pub button_radius_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct IslandTokens {
    pub focus_ring_px: f32,
    pub unfocused_border_px: f32,
    pub dim_unfocused: bool,
    pub unfocused_opacity: f32,
    pub pane_padding_x_px: f32,
    pub pane_padding_top_px: f32,
    pub pane_padding_bottom_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EditorTokens {
    #[serde(deserialize_with = "color")]
    pub gutter_text: Hsla,
    #[serde(deserialize_with = "color")]
    pub gutter_text_active: Hsla,
    #[serde(deserialize_with = "color")]
    pub current_line: Hsla,
    #[serde(deserialize_with = "color")]
    pub selection: Hsla,
    #[serde(deserialize_with = "color")]
    pub cursor: Hsla,
    #[serde(deserialize_with = "color")]
    pub diff_added: Hsla,
    #[serde(deserialize_with = "color")]
    pub diff_modified: Hsla,
    pub gutter_padding_px: f32,
    pub cursor_width_px: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SyntaxTokens {
    #[serde(deserialize_with = "color")]
    pub keyword: Hsla,
    #[serde(deserialize_with = "color")]
    pub string: Hsla,
    #[serde(deserialize_with = "color")]
    pub number: Hsla,
    #[serde(rename = "type", deserialize_with = "color")]
    pub type_name: Hsla,
    #[serde(deserialize_with = "color")]
    pub function: Hsla,
    #[serde(rename = "macro", deserialize_with = "color")]
    pub macro_name: Hsla,
    #[serde(deserialize_with = "color")]
    pub comment: Hsla,
    #[serde(deserialize_with = "color")]
    pub punctuation: Hsla,
    #[serde(deserialize_with = "color")]
    pub heading: Hsla,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ShadowLayer {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub blur: f32,
    #[serde(default)]
    pub spread: f32,
    #[serde(deserialize_with = "color")]
    pub color: Hsla,
}

impl ShadowLayer {
    fn to_box_shadow(self) -> BoxShadow {
        BoxShadow {
            color: self.color,
            offset: point(px(self.x), px(self.y)),
            blur_radius: px(self.blur),
            spread_radius: px(self.spread),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShadowTokens {
    pub card: Vec<ShadowLayer>,
    pub tab: Vec<ShadowLayer>,
    pub menu: Vec<ShadowLayer>,
    pub palette: Vec<ShadowLayer>,
}

pub fn box_shadows(layers: &[ShadowLayer]) -> Vec<BoxShadow> {
    layers.iter().map(|layer| layer.to_box_shadow()).collect()
}
