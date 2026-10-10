#![allow(clippy::disallowed_methods)]

use crate::app::AppState;
use crate::command::{toggle_palette, CommandId, CommandRegistry};
use crate::keymap::{Combo, Keymap};
use crate::theme::{hex_to_hsla, Theme};
use crate::tabs::Tabs;
use crate::terminal_view::{grid_size_for, xterm_indexed_rgb, TerminalColors};
use domain::{Axis, LayoutNode, PanelId};
use gpui::{px, Modifiers};
use terminal_core::{Cell, CellColor, GridSize, InputModes};

#[test]
fn theme_loads_from_assets() {
    let theme = Theme::load_default().expect("theme parses");
    assert_eq!(theme.name, "github-light");
    assert!((theme.surface.canvas.l - 1.0).abs() < 1e-3);
    assert!(theme.accent.primary.s > 0.5);
    assert!(theme.layout.tab_strip_height_px > 0.0);
    assert!(theme.layout.workspace_rail_width_px > 0.0);
    assert!(theme.layout.border_radius_px > 0.0);
    assert!(theme.font.size_px > 0.0);
    assert!(theme.scrollback.default_lines >= 1000);
    assert!(theme.ansi.black.l < 0.2);
}

#[test]
fn theme_hex_to_hsla_conversion() {
    let black = hex_to_hsla("#000000", "test.black").unwrap();
    assert!(black.l.abs() < 1e-3);
    let white = hex_to_hsla("#FFFFFF", "test.white").unwrap();
    assert!((white.l - 1.0).abs() < 1e-3);
    let accent = hex_to_hsla("#0969DA", "test.accent").unwrap();
    assert!(accent.s > 0.5);
    assert!(accent.l > 0.3 && accent.l < 0.6);
}

#[test]
fn keymap_resolves_default_bindings() {
    let map = Keymap::default();
    let cmd_k = Combo::new(Modifiers::command(), "k");
    assert_eq!(map.resolve(&cmd_k), Some(CommandId::OpenPalette));
    let cmd_w = Combo::new(Modifiers::command(), "w");
    assert_eq!(map.resolve(&cmd_w), Some(CommandId::CloseActive));
    let cmd_t = Combo::new(Modifiers::command(), "t");
    assert_eq!(map.resolve(&cmd_t), Some(CommandId::NewTab));
}

#[test]
fn keymap_detects_conflicts() {
    let mut map = Keymap::new();
    let combo = Combo::new(Modifiers::command(), "p");
    map.bind(combo.clone(), CommandId::OpenPalette);
    map.bind(combo.clone(), CommandId::CloseActive);
    let conflicts = map.conflicts();
    assert!(!conflicts.is_empty(), "second bind should record conflict");
    assert_eq!(conflicts[0].0, combo);
    assert_eq!(conflicts[0].1, CommandId::OpenPalette);
    assert_eq!(conflicts[0].2, CommandId::CloseActive);
}

#[test]
fn command_registry_fuzzy_search() {
    let registry = CommandRegistry::with_defaults();
    let matches = registry.search("pal");
    let titles: Vec<&str> = matches.iter().map(|c| c.title).collect();
    assert!(
        titles.iter().any(|t| t.contains("Palette")),
        "search should return palette commands, got {:?}",
        titles
    );

    let matches = registry.search("tab");
    let titles: Vec<&str> = matches.iter().map(|c| c.title).collect();
    assert!(
        titles.iter().any(|t| t.contains("Tab")),
        "search should return tab commands"
    );
}

#[test]
fn command_registry_execute_calls_handler() {
    let _registry = CommandRegistry::with_defaults();
    let mut state = AppState::new();
    state.palette_visible = false;
    CommandRegistry::dispatch(&mut state, CommandId::OpenPalette);
    assert!(state.palette_visible);
    let _ = toggle_palette;
}

#[test]
fn app_state_starts_with_three_workspaces() {
    let state = AppState::new();
    assert_eq!(state.workspaces.len(), 3);
    assert_eq!(state.workspaces[0].name, "Personal");
    assert_eq!(state.workspaces[1].name, "Infrastructure");
    assert_eq!(state.workspaces[2].name, "Project");
    assert!(!state.palette_visible);
}

fn keystroke(modifiers: Modifiers, key: &str, key_char: Option<&str>) -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

fn encode(modifiers: Modifiers, key: &str, key_char: Option<&str>) -> Option<Vec<u8>> {
    crate::input::encode_keystroke(&keystroke(modifiers, key, key_char), InputModes::default())
}

#[test]
fn input_encodes_printable_and_named_keys() {
    let none = Modifiers::none();
    assert_eq!(encode(none, "a", Some("a")), Some(b"a".to_vec()));
    assert_eq!(encode(Modifiers::shift(), "a", Some("A")), Some(b"A".to_vec()));
    assert_eq!(encode(none, "enter", Some("\n")), Some(b"\r".to_vec()));
    assert_eq!(encode(none, "backspace", None), Some(vec![0x7f]));
    assert_eq!(encode(none, "tab", Some("\t")), Some(b"\t".to_vec()));
    assert_eq!(encode(none, "escape", None), Some(vec![0x1b]));
    assert_eq!(encode(none, "up", None), Some(b"\x1b[A".to_vec()));
    assert_eq!(encode(none, "left", None), Some(b"\x1b[D".to_vec()));
}

#[test]
fn input_uses_ss3_arrows_in_app_cursor_mode() {
    let modes = InputModes { app_cursor: true, ..InputModes::default() };
    let bytes = crate::input::encode_keystroke(&keystroke(Modifiers::none(), "down", None), modes);
    assert_eq!(bytes, Some(b"\x1bOB".to_vec()));
}

#[test]
fn input_encodes_control_letters_and_option_as_meta() {
    assert_eq!(encode(Modifiers::control(), "c", None), Some(vec![0x03]));
    assert_eq!(encode(Modifiers::control(), "[", None), Some(vec![0x1b]));
    assert_eq!(encode(Modifiers::alt(), "b", Some("\u{222b}")), Some(b"\x1bb".to_vec()));
    assert_eq!(encode(Modifiers::alt(), "left", None), Some(b"\x1b\x1b[D".to_vec()));
}

#[test]
fn input_ignores_command_shortcuts() {
    assert_eq!(encode(Modifiers::command(), "c", None), None);
}

#[test]
fn paste_wraps_only_when_bracketed_mode_enabled() {
    let plain = crate::input::paste_bytes("a\nb", InputModes::default());
    assert_eq!(plain, b"a\rb".to_vec());
    let modes = InputModes { bracketed_paste: true, ..InputModes::default() };
    let bracketed = crate::input::paste_bytes("x\x1by", modes);
    assert_eq!(bracketed, b"\x1b[200~xy\x1b[201~".to_vec());
}

#[test]
fn colors_map_ansi_from_theme_and_extended_palette() {
    let theme = Theme::load_default().unwrap();
    let colors = TerminalColors::from_theme(&theme);
    let mut cell = Cell::empty();
    assert_eq!(colors.foreground_of(&cell), theme.text.default);
    assert_eq!(colors.background_of(&cell), None);
    cell.fg = CellColor::Indexed(1);
    assert_eq!(colors.foreground_of(&cell), theme.ansi.red);
    cell.fg = CellColor::Indexed(9);
    assert_eq!(colors.foreground_of(&cell), theme.ansi.bright_red);
    cell.bg = CellColor::Rgb(0x12, 0x34, 0x56);
    let expected: gpui::Hsla = gpui::rgb(0x123456).into();
    assert_eq!(colors.background_of(&cell), Some(expected));
}

#[test]
fn colors_swap_on_inverse() {
    let theme = Theme::load_default().unwrap();
    let colors = TerminalColors::from_theme(&theme);
    let inverse_flag = 0b1;
    let cell = Cell { flags: inverse_flag, ..Cell::empty() };
    assert_eq!(colors.foreground_of(&cell), theme.surface.canvas);
    assert_eq!(colors.background_of(&cell), Some(theme.text.default));
}

#[test]
fn xterm_256_palette_matches_reference_values() {
    assert_eq!(xterm_indexed_rgb(16), 0x000000);
    assert_eq!(xterm_indexed_rgb(21), 0x0000ff);
    assert_eq!(xterm_indexed_rgb(196), 0xff0000);
    assert_eq!(xterm_indexed_rgb(231), 0xffffff);
    assert_eq!(xterm_indexed_rgb(232), 0x080808);
    assert_eq!(xterm_indexed_rgb(255), 0xeeeeee);
}

#[test]
fn grid_size_floors_area_by_cell_metrics() {
    let cell = gpui::size(px(8.0), px(18.0));
    assert_eq!(grid_size_for(gpui::size(px(800.0), px(450.0)), cell), GridSize::new(100, 25));
    assert_eq!(grid_size_for(gpui::size(px(803.9), px(469.0)), cell), GridSize::new(100, 26));
    assert_eq!(grid_size_for(gpui::size(px(0.0), px(0.0)), cell), GridSize::new(2, 1));
}

#[test]
fn tabs_split_focus_and_close_collapse_layout() {
    let mut tabs = Tabs::default();
    let first = PanelId::new();
    tabs.open(first);
    let second = PanelId::new();
    tabs.active_mut().unwrap().split_focused(Axis::Horizontal, second).unwrap();
    assert_eq!(tabs.active().unwrap().focused(), second);
    tabs.active_mut().unwrap().focus_next();
    assert_eq!(tabs.active().unwrap().focused(), first);
    tabs.close_panel(first).unwrap();
    let tab = tabs.active().unwrap();
    assert_eq!(tab.layout(), &LayoutNode::leaf(second));
    assert_eq!(tab.focused(), second);
}

#[test]
fn tabs_cycle_and_remove_tab_when_last_panel_closes() {
    let mut tabs = Tabs::default();
    let (a, b, c) = (PanelId::new(), PanelId::new(), PanelId::new());
    tabs.open(a);
    tabs.open(b);
    tabs.open(c);
    assert_eq!(tabs.active_index(), 2);
    tabs.select_next();
    assert_eq!(tabs.active_index(), 0);
    tabs.select_previous();
    assert_eq!(tabs.active_index(), 2);
    tabs.close_panel(c).unwrap();
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 1);
    tabs.close_panel(a).unwrap();
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(tabs.active().unwrap().focused(), b);
}

#[test]
fn keymap_tab_switch_matches_gpui_shifted_bracket_keys() {
    let map = Keymap::default();
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "}")), Some(CommandId::NextTab));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "{")), Some(CommandId::PrevTab));
}
