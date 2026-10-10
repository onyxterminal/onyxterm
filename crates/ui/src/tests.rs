#![allow(clippy::disallowed_methods)]

use crate::app::AppState;
use crate::command::{
    open_palette, palette_backspace, palette_move, palette_selected, palette_type, toggle_palette, CommandId,
    CommandRegistry,
};
use crate::input::{page_scroll_lines, wheel_bytes};
use crate::keymap::{Combo, Keymap};
use crate::theme::{hex_to_hsla, Theme};
use crate::tabs::Tabs;
use crate::terminal_view::{
    grid_point_at, grid_size_for, selection_kind_for, split_scroll, xterm_indexed_rgb, TerminalColors,
};
use domain::{Axis, LayoutNode, PanelId};
use gpui::{px, Modifiers};
use terminal_core::{Cell, CellColor, GridPoint, GridSize, InputModes, SelectionKind, Side};

#[test]
fn theme_loads_from_assets() {
    let theme = Theme::load_default().expect("theme parses");
    assert_eq!(theme.name, "github-light");
    assert!((theme.surface.canvas.l - 1.0).abs() < 1e-3);
    assert!(theme.accent.primary.s > 0.5);
    assert!(theme.layout.tab_strip_height_px > 0.0);
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

#[test]
fn tabs_activate_by_index_ignores_out_of_range() {
    let mut tabs = Tabs::default();
    let (a, b, c) = (PanelId::new(), PanelId::new(), PanelId::new());
    tabs.open(a);
    tabs.open(b);
    tabs.open(c);
    tabs.activate(0);
    assert_eq!(tabs.active().unwrap().focused(), a);
    tabs.activate(1);
    assert_eq!(tabs.active_index(), 1);
    tabs.activate(7);
    assert_eq!(tabs.active_index(), 1);
}

#[test]
fn wheel_converts_to_repeated_arrows_respecting_cursor_mode() {
    assert_eq!(wheel_bytes(2, InputModes::default()), b"\x1b[A\x1b[A".to_vec());
    assert_eq!(wheel_bytes(-1, InputModes::default()), b"\x1b[B".to_vec());
    let app_cursor = InputModes { app_cursor: true, ..InputModes::default() };
    assert_eq!(wheel_bytes(-2, app_cursor), b"\x1bOB\x1bOB".to_vec());
}

#[test]
fn shift_page_keys_scroll_one_screen() {
    assert_eq!(page_scroll_lines(&keystroke(Modifiers::shift(), "pageup", None), 24), Some(24));
    assert_eq!(page_scroll_lines(&keystroke(Modifiers::shift(), "pagedown", None), 24), Some(-24));
    assert_eq!(page_scroll_lines(&keystroke(Modifiers::none(), "pageup", None), 24), None);
    assert_eq!(page_scroll_lines(&keystroke(Modifiers::shift(), "a", Some("A")), 24), None);
}

#[test]
fn scroll_accumulates_fractional_trackpad_deltas() {
    assert_eq!(split_scroll(10.0, 18.0), (0, 10.0));
    assert_eq!(split_scroll(40.0, 18.0), (2, 4.0));
    assert_eq!(split_scroll(-20.0, 18.0), (-1, -2.0));
}

#[test]
fn grid_point_clamps_and_picks_cell_side() {
    let cell = gpui::size(px(10.0), px(20.0));
    let origin = gpui::point(px(100.0), px(50.0));
    let grid = GridSize::new(80, 24);
    let (point, side) = grid_point_at(gpui::point(px(123.0), px(95.0)), origin, cell, grid);
    assert_eq!(point, GridPoint { row: 2, col: 2 });
    assert_eq!(side, Side::Left);
    let (_, side) = grid_point_at(gpui::point(px(128.0), px(95.0)), origin, cell, grid);
    assert_eq!(side, Side::Right);
    let (point, _) = grid_point_at(gpui::point(px(0.0), px(9_999.0)), origin, cell, grid);
    assert_eq!(point, GridPoint { row: 23, col: 0 });
    let (point, _) = grid_point_at(gpui::point(px(9_999.0), px(0.0)), origin, cell, grid);
    assert_eq!(point, GridPoint { row: 0, col: 79 });
}

#[test]
fn click_count_selects_simple_word_or_line() {
    assert_eq!(selection_kind_for(1), SelectionKind::Simple);
    assert_eq!(selection_kind_for(2), SelectionKind::Word);
    assert_eq!(selection_kind_for(3), SelectionKind::Line);
}

#[test]
fn palette_typing_filters_moves_and_selects() {
    let mut state = AppState::new();
    open_palette(&mut state);
    palette_type(&mut state, "split");
    let matches = state.filtered_commands();
    assert!(matches.len() >= 2);
    assert_eq!(palette_selected(&state), Some(matches[0]));
    palette_move(&mut state, 1);
    assert_eq!(palette_selected(&state), Some(matches[1]));
    palette_move(&mut state, -2);
    assert_eq!(palette_selected(&state), Some(matches[matches.len() - 1]));
    palette_backspace(&mut state);
    assert_eq!(state.palette_query, "spli");
    assert_eq!(state.palette_selection, 0);
    palette_type(&mut state, "zzzz");
    palette_move(&mut state, 1);
    assert_eq!(palette_selected(&state), None);
}

#[test]
fn palette_lists_required_commands() {
    let registry = CommandRegistry::with_defaults();
    let titles: Vec<&str> = registry.list().iter().map(|c| c.title).collect();
    for required in [
        "New Tab",
        "Close Pane",
        "Next Tab",
        "Previous Tab",
        "Split Right",
        "Split Down",
        "Focus Next Pane",
        "Copy",
        "Paste",
    ] {
        assert!(titles.contains(&required), "missing {required}");
    }
    let map = Keymap::default();
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "c")), Some(CommandId::Copy));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "v")), Some(CommandId::Paste));
}

#[test]
fn font_family_maps_system_monospace_and_falls_back() {
    let theme = Theme::load_default().unwrap();
    assert_eq!(theme.font.family_candidates(), vec!["SF Mono", "Menlo", "SF Mono", "Menlo", "Menlo"]);
    let available = vec!["Menlo".to_string(), "Courier".to_string()];
    assert_eq!(theme.font.resolve_family(&available), "Menlo");
    let with_sf = vec!["SF Mono".to_string(), "Menlo".to_string()];
    assert_eq!(theme.font.resolve_family(&with_sf), "SF Mono");
    assert_eq!(theme.font.resolve_family(&[]), "Menlo");
}
