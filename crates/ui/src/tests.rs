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
    let translucent = hex_to_hsla("#D1D9E0B0", "test.translucent").unwrap();
    let opaque = hex_to_hsla("#D1D9E0", "test.opaque").unwrap();
    assert!((translucent.a - 0xB0 as f32 / 255.0).abs() < 1e-3);
    assert!((translucent.h - opaque.h).abs() < 1e-3 && (translucent.l - opaque.l).abs() < 1e-3);
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

#[test]
fn workspace_picker_fuzzy_filters_choices_and_returns_key() {
    use crate::app::PaletteChoice;
    use crate::command::{filtered_choices, open_choice_palette, palette_selected_choice};
    let mut state = AppState::new();
    let choice = |key: &str, title: &str| PaletteChoice { key: key.into(), title: title.into() };
    open_choice_palette(
        &mut state,
        vec![choice("a", "Personal"), choice("b", "Infrastructure"), choice("c", "Backend API")],
    );
    assert_eq!(filtered_choices(&state).len(), 3);
    assert_eq!(palette_selected(&state), None);
    palette_type(&mut state, "bapi");
    assert_eq!(palette_selected_choice(&state), Some("c".to_string()));
    palette_backspace(&mut state);
    palette_backspace(&mut state);
    palette_backspace(&mut state);
    palette_move(&mut state, 1);
    assert_eq!(filtered_choices(&state).len(), 1);
    open_palette(&mut state);
    assert!(state.palette_choices.is_none());
    assert!(palette_selected(&state).is_some());
}

#[test]
fn palette_offers_workspace_commands() {
    let registry = CommandRegistry::with_defaults();
    let titles: Vec<&str> = registry.list().iter().map(|c| c.title).collect();
    assert!(titles.contains(&"Switch Workspace"));
    assert!(titles.contains(&"New Workspace"));
}

#[test]
fn delete_workspace_copy_names_workspace_and_session_count() {
    use crate::confirm::{delete_workspace_copy, sessions_ending};
    let copy = delete_workspace_copy("Backend", 3);
    assert!(copy.title.contains("Backend"));
    assert!(copy.body.ends_with("3 sessions will end."));
    assert_eq!(sessions_ending(1), "1 session will end.");
}

#[test]
fn quit_prompt_counts_sessions_and_cmd_q_routes_to_quit() {
    use crate::confirm::quit_copy;
    let copy = quit_copy(4);
    assert_eq!(copy.title, "Quit Onyxterm?");
    assert_eq!(copy.body, "4 sessions will end.");
    assert_eq!(copy.action, "Quit");
    let map = Keymap::default();
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "q")), Some(CommandId::Quit));
}

#[test]
fn maximize_toggles_view_without_touching_layout() {
    let mut tabs = Tabs::default();
    let (first, second) = (PanelId::new(), PanelId::new());
    tabs.open(first);
    let tab = tabs.active_mut().unwrap();
    tab.toggle_maximized();
    assert!(!tab.is_maximized());
    tab.toggle_maximized();
    tab.split_focused(Axis::Vertical, second).unwrap();
    assert!(!tab.is_maximized());
    let layout = tab.layout().clone();
    tab.toggle_maximized();
    assert!(tab.is_maximized());
    assert_eq!(tab.layout(), &layout);
    tab.focus_next();
    assert!(tab.is_maximized());
    tab.toggle_maximized();
    assert!(!tab.is_maximized());
    assert_eq!(tab.layout(), &layout);
    tab.toggle_maximized();
    tabs.close_panel(second).unwrap();
    assert!(!tabs.active().unwrap().is_maximized());
}

#[test]
fn island_padding_and_header_inset_content_and_grid_uses_inner_size() {
    use crate::terminal_view::content_bounds;
    let theme = Theme::load_default().unwrap();
    let padding = px(theme.layout.island_padding_px);
    let header = px(theme.layout.pane_header_height_px);
    assert_eq!(theme.layout.pane_header_height_px, 28.0);
    assert_eq!(theme.layout.island_padding_px, 8.0);
    assert_eq!(theme.layout.island_radius_px, 10.0);
    assert_eq!(theme.layout.island_gap_px, 6.0);
    assert_eq!(theme.layout.window_margin_px, 8.0);
    let island = gpui::Bounds::new(gpui::point(px(100.0), px(50.0)), gpui::size(px(816.0), px(513.0)));
    let inner = content_bounds(island, padding, header);
    assert_eq!(inner.origin, gpui::point(px(108.0), px(86.0)));
    assert_eq!(inner.size, gpui::size(px(800.0), px(469.0)));
    let cell = gpui::size(px(8.0), px(18.0));
    assert_eq!(grid_size_for(inner.size, cell), GridSize::new(100, 26));
    assert_eq!(grid_size_for(island.size, cell), GridSize::new(102, 28));
    let small = gpui::Bounds::new(gpui::point(px(0.0), px(0.0)), gpui::size(px(10.0), px(10.0)));
    let tiny = content_bounds(small, padding, header);
    assert_eq!(tiny.size, gpui::size(px(0.0), px(0.0)));
}

#[test]
fn short_cwd_uses_home_mark_and_last_two_segments() {
    use crate::shell::short_cwd;
    use std::path::Path;
    let home = Path::new("/Users/me");
    assert_eq!(short_cwd(Path::new("/Users/me"), home), "~");
    assert_eq!(short_cwd(Path::new("/Users/me/Development/onyxterm"), home), "~/Development/onyxterm");
    assert_eq!(short_cwd(Path::new("/Users/me/Development/Personal/onyxterm"), home), "~/\u{2026}/Personal/onyxterm");
    assert_eq!(short_cwd(Path::new("/usr"), home), "/usr");
    assert_eq!(short_cwd(Path::new("/"), home), "/");
    assert_eq!(short_cwd(Path::new("/opt/homebrew/bin"), home), "/\u{2026}/homebrew/bin");
}

#[test]
fn tab_rename_trims_and_blank_reverts_to_automatic_title() {
    use crate::command::CommandRegistry;
    let mut tabs = Tabs::default();
    tabs.open(PanelId::new());
    let tab = tabs.get_mut(0).unwrap();
    assert_eq!(tab.name(), None);
    tab.rename(Some("  build  ".to_string()));
    assert_eq!(tab.name(), Some("build"));
    tab.rename(Some("   ".to_string()));
    assert_eq!(tab.name(), None);
    tab.rename(None);
    assert_eq!(tab.name(), None);
    assert!(tabs.get_mut(1).is_none());
    let titles: Vec<&str> = CommandRegistry::with_defaults().list().iter().map(|c| c.title).collect();
    assert!(titles.contains(&"Rename Tab"));
}

#[test]
fn every_keymap_binding_round_trips_into_a_gpui_menu_shortcut() {
    let map = Keymap::default();
    assert_eq!(Combo::new(Modifiers::command() | Modifiers::alt(), "right").binding_text(), "alt-cmd-right");
    assert_eq!(Combo::new(Modifiers::command() | Modifiers::control(), "f").binding_text(), "ctrl-cmd-f");
    for (combo, _) in map.iter() {
        let parsed = gpui::Keystroke::parse(&combo.binding_text()).expect("binding text parses");
        assert_eq!((parsed.modifiers, parsed.key.as_str()), (combo.modifiers, combo.key.as_str()));
    }
    let shortcuts = crate::menus::menu_shortcuts(&map);
    assert_eq!(shortcuts.len(), map.len());
    let quit = crate::menus::RunCommand(CommandId::Quit);
    assert!(shortcuts.iter().any(|binding| binding.action().partial_eq(&quit)));
}

#[test]
fn menu_bar_lists_native_sections_and_window_shortcuts() {
    let names: Vec<String> = crate::menus::app_menus().iter().map(|menu| menu.name.to_string()).collect();
    assert_eq!(names, ["Onyxterm", "File", "Edit", "View", "Window"]);
    let map = Keymap::default();
    let cmd = Modifiers::command();
    assert_eq!(map.resolve(&Combo::new(cmd, "m")), Some(CommandId::Minimize));
    assert_eq!(map.resolve(&Combo::new(cmd, "h")), Some(CommandId::Hide));
    assert_eq!(map.resolve(&Combo::new(cmd | Modifiers::alt(), "h")), Some(CommandId::HideOthers));
    assert_eq!(map.resolve(&Combo::new(cmd | Modifiers::control(), "f")), Some(CommandId::ToggleFullScreen));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "w")), Some(CommandId::CloseTab));
}

#[test]
fn shortcuts_match_the_physical_key_under_an_arabic_layout() {
    let map = Keymap::default();
    let cmd = Modifiers::command();
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0646}", None)), Some(CommandId::OpenPalette));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0635}", None)), Some(CommandId::CloseActive));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0641}", None)), Some(CommandId::NewTab));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{0648}", None)), Some(CommandId::MaximizePane));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{0629}", None)), Some(CommandId::NextTab));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{062c}", None)), Some(CommandId::PrevTab));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "k", None)), Some(CommandId::OpenPalette));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::none(), "\u{0646}", Some("\u{0646}"))), None);
    let control_c = crate::keymap::physical_combo(&keystroke(Modifiers::control(), "\u{0630}", None));
    assert_eq!(control_c, Some(Combo::new(Modifiers::control(), "c")));
}

#[test]
fn arabic_text_reaches_the_shell_as_arabic() {
    let typed = "\u{0646}";
    assert_eq!(encode(Modifiers::none(), typed, Some(typed)), Some(typed.as_bytes().to_vec()));
}

#[test]
fn printable_keys_go_through_the_input_method_unless_option_is_meta() {
    use crate::input::is_composed_text;
    let none = Modifiers::none();
    assert!(is_composed_text(&keystroke(none, "a", Some("a")), true));
    assert!(is_composed_text(&keystroke(none, "\u{0646}", Some("\u{0646}")), true));
    assert!(is_composed_text(&keystroke(Modifiers::alt(), "e", Some("")), false));
    assert!(!is_composed_text(&keystroke(Modifiers::alt(), "e", Some("")), true));
    assert!(!is_composed_text(&keystroke(none, "enter", Some("\n")), true));
    assert!(!is_composed_text(&keystroke(Modifiers::control(), "c", None), true));
    assert!(!is_composed_text(&keystroke(Modifiers::command(), "k", None), false));
}

#[test]
fn dropped_paths_are_shell_escaped_and_space_separated() {
    use crate::input::{dropped_paths_text, shell_escape};
    use std::path::PathBuf;
    assert_eq!(shell_escape("/usr/local/bin"), "/usr/local/bin");
    assert_eq!(shell_escape("/tmp/My Files"), "'/tmp/My Files'");
    assert_eq!(shell_escape("/tmp/it's"), r"'/tmp/it'\''s'");
    let paths = [PathBuf::from("/a/b.txt"), PathBuf::from("/c/d e")];
    assert_eq!(dropped_paths_text(&paths), "/a/b.txt '/c/d e'");
}

#[test]
fn theme_defaults_option_to_meta() {
    assert!(Theme::load_default().unwrap().input.option_as_meta);
}

#[test]
fn split_ratio_follows_the_pointer_and_clamps_to_minimum_pane_size() {
    use crate::tabs::clamp_split_ratio;
    assert!((clamp_split_ratio(300.0, 1000.0, 120.0) - 0.3).abs() < 1e-6);
    assert!((clamp_split_ratio(10.0, 1000.0, 120.0) - 0.12).abs() < 1e-6);
    assert!((clamp_split_ratio(990.0, 1000.0, 120.0) - 0.88).abs() < 1e-6);
    assert_eq!(clamp_split_ratio(50.0, 200.0, 120.0), 0.5);
}

#[test]
fn set_split_ratio_targets_the_nested_split_by_path() {
    use crate::tabs::{SplitSide, Tab};
    let (a, b, c) = (PanelId::new(), PanelId::new(), PanelId::new());
    let mut tab = Tab::new(a);
    tab.split_focused(Axis::Horizontal, b).unwrap();
    tab.split_focused(Axis::Vertical, c).unwrap();
    tab.set_split_ratio(&[SplitSide::Second], 0.25);
    tab.set_split_ratio(&[], 0.7);
    tab.set_split_ratio(&[SplitSide::First], 0.1);
    let LayoutNode::Split { ratio, second, .. } = tab.layout() else { panic!("root split") };
    assert!((ratio - 0.7).abs() < 1e-6);
    let LayoutNode::Split { ratio: inner, .. } = second.as_ref() else { panic!("nested split") };
    assert!((inner - 0.25).abs() < 1e-6);
}

#[test]
fn drop_zone_picks_the_nearest_edge_or_the_center() {
    use crate::tabs::drop_zone_at;
    use domain::DropZone;
    assert_eq!(drop_zone_at(0.5, 0.5), DropZone::Center);
    assert_eq!(drop_zone_at(0.1, 0.5), DropZone::Left);
    assert_eq!(drop_zone_at(0.9, 0.4), DropZone::Right);
    assert_eq!(drop_zone_at(0.5, 0.05), DropZone::Top);
    assert_eq!(drop_zone_at(0.45, 0.95), DropZone::Bottom);
}

#[test]
fn pane_menu_lists_items_with_shortcuts_and_a_divider_before_close() {
    use crate::pane_menu::{pane_menu_entries, selectable_commands, step_selection, PaneMenuEntry};
    let map = Keymap::default();
    let entries = pane_menu_entries(false, &map);
    let rows: Vec<(String, Option<String>)> = entries
        .iter()
        .map(|entry| match entry {
            PaneMenuEntry::Item(item) => (item.title.to_string(), item.shortcut.clone()),
            PaneMenuEntry::Divider => ("-".to_string(), None),
        })
        .collect();
    let expected = [
        ("Split Right", Some("\u{2325}\u{2318}\u{2192}")),
        ("Split Down", Some("\u{2325}\u{2318}\u{2193}")),
        ("Maximize Pane", Some("\u{21e7}\u{2318}M")),
        ("Rename Pane", None),
        ("Move to New Tab", None),
        ("-", None),
        ("Close Pane", Some("\u{2318}W")),
    ];
    let expected: Vec<(String, Option<String>)> =
        expected.iter().map(|(title, shortcut)| (title.to_string(), shortcut.map(str::to_string))).collect();
    assert_eq!(rows, expected);
    let restore = pane_menu_entries(true, &map);
    assert!(matches!(&restore[2], PaneMenuEntry::Item(item) if item.title == "Restore Pane"));
    let commands = selectable_commands(&entries);
    assert_eq!(commands.len(), 6);
    assert_eq!(commands.last(), Some(&CommandId::CloseActive));
    assert_eq!(step_selection(0, -1, 6), 5);
    assert_eq!(step_selection(5, 1, 6), 0);
    assert_eq!(step_selection(2, 1, 6), 3);
}

#[test]
fn move_to_new_tab_detaches_a_split_pane_and_rejects_a_lone_pane() {
    let mut tabs = Tabs::default();
    let (a, b) = (PanelId::new(), PanelId::new());
    tabs.open(a);
    tabs.active_mut().unwrap().split_focused(Axis::Horizontal, b).unwrap();
    tabs.move_panel_to_new_tab(b).unwrap();
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active().unwrap().focused(), b);
    assert!(tabs.move_panel_to_new_tab(b).is_err());
    assert_eq!(tabs.len(), 2);
}
