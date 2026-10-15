#![allow(clippy::disallowed_methods)]

use crate::app::AppState;
use crate::command::{
    open_palette, palette_backspace, palette_move, palette_selected, palette_type, toggle_palette, CommandId,
    CommandRegistry,
};
use crate::input::{page_scroll_lines, wheel_bytes};
use crate::keymap::{Combo, Keymap};
use crate::theme::{hex_to_hsla, Appearance, Theme, ThemeKind};
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
    assert_eq!(theme.name, "apple-light");
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
    let cmd_shift_p = Combo::new(Modifiers::command_shift(), "p");
    assert_eq!(map.resolve(&cmd_shift_p), Some(CommandId::OpenPalette));
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
fn palette_search_hides_indexed_tab_and_workspace_commands() {
    let registry = CommandRegistry::with_defaults();
    let is_indexed = |id: CommandId| matches!(id, CommandId::SelectTab(_) | CommandId::SelectWorkspace(_));
    for query in ["", "select", "select tab 1", "workspace 2"] {
        assert!(registry.search(query).iter().all(|command| !is_indexed(command.id)), "{query:?}");
    }
    assert_eq!(registry.list().iter().filter(|command| is_indexed(command.id)).count(), 18);
    assert!(registry.search("tab").iter().any(|command| command.id == CommandId::NewTab));
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
fn input_shift_space_sends_newline_to_the_pty() {
    assert_eq!(encode(Modifiers::shift(), "space", Some(" ")), Some(b"\n".to_vec()));
    assert_eq!(encode(Modifiers::none(), "space", Some(" ")), Some(b" ".to_vec()));
    assert_eq!(encode(Modifiers::control(), "space", Some(" ")), Some(vec![0]));
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
fn tabs_reorder_swaps_within_range_and_clamps_out_of_range() {
    use crate::tabs::Tab;
    use domain::TabId;
    let mut tabs = Tabs::default();
    let ids = [PanelId::new(), PanelId::new(), PanelId::new(), PanelId::new()];
    for id in ids {
        tabs.open(id);
    }
    tabs.activate(0);
    let original: Vec<TabId> = tabs.iter().map(Tab::id).collect();

    tabs.reorder(0, 2);
    let mut expected = original.clone();
    expected.remove(0);
    expected.insert(2, original[0]);
    assert_eq!(tabs.iter().map(Tab::id).collect::<Vec<_>>(), expected);
    assert_eq!(tabs.active_index(), 2);

    tabs.activate(0);
    tabs.reorder(0, 0);
    assert_eq!(tabs.iter().map(Tab::id).collect::<Vec<_>>(), expected);
    assert_eq!(tabs.active_index(), 0);

    tabs.reorder(2, 99);
    assert_eq!(tabs.iter().last().map(Tab::id), Some(expected[2]));

    tabs.activate(1);
    tabs.reorder(3, 1);
    let pos_of_active = tabs.iter().position(|tab| tab.id() == expected[1]).unwrap();
    assert_eq!(tabs.active_index(), pos_of_active);
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
fn keymap_binds_tabs_and_workspaces_by_index_and_order() {
    let map = Keymap::default();
    let cmd = Modifiers::command();
    let cmd_alt = cmd | Modifiers::alt();
    for slot in 1..=9 {
        let key = slot.to_string();
        assert_eq!(map.resolve(&Combo::new(cmd, key.as_str())), Some(CommandId::SelectTab(slot - 1)));
        assert_eq!(map.resolve(&Combo::new(cmd_alt, key.as_str())), Some(CommandId::SelectWorkspace(slot - 1)));
    }
    assert_eq!(map.resolve(&Combo::new(Modifiers::control(), "tab")), Some(CommandId::NextDocument));
    assert_eq!(map.resolve(&Combo::new(Modifiers::control() | Modifiers::shift(), "tab")), Some(CommandId::PrevDocument));
    assert_eq!(map.resolve(&Combo::new(cmd_alt, "]")), Some(CommandId::NextWorkspace));
    assert_eq!(map.resolve(&Combo::new(cmd_alt, "[")), Some(CommandId::PrevWorkspace));
    assert_eq!(map.resolve(&Combo::new(cmd, "o")), Some(CommandId::OpenWorkspaceMenu));
    assert_eq!(map.combo_for(CommandId::NextTab), Some(&Combo::new(cmd, "}")));
}

#[test]
fn default_keymap_has_no_conflicts() {
    assert!(Keymap::default().conflicts().is_empty());
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
    assert_eq!(state.palette_query.text(), "spli");
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
        "Next Document",
        "Previous Document",
        "Close Document",
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
    let available = ["Menlo", "Courier"];
    assert_eq!(theme.font.resolve_family(|name| available.contains(&name)), "Menlo");
    let with_sf = ["SF Mono", "Menlo"];
    assert_eq!(theme.font.resolve_family(|name| with_sf.contains(&name)), "SF Mono");
    assert_eq!(theme.font.resolve_family(|_| false), "Menlo");
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
    let copy = delete_workspace_copy("Backend", 3, 0);
    assert!(copy.title.contains("Backend"));
    assert!(copy.body.ends_with("3 sessions will end."));
    assert!(copy.is_destructive);
    assert_eq!(sessions_ending(1), "1 session will end.");
}

#[test]
fn quit_prompt_counts_sessions_and_cmd_q_routes_to_quit() {
    use crate::confirm::quit_copy;
    let copy = quit_copy(4, 0);
    assert_eq!(copy.title, "Quit Onyx?");
    assert_eq!(copy.body, "4 sessions will end.");
    assert_eq!(copy.action, "Quit");
    assert!(!copy.is_destructive);
    assert!(crate::confirm::close_pane_copy().is_destructive);
    assert!(crate::confirm::close_tab_copy().is_destructive);
    let map = Keymap::default();
    assert_eq!(map.resolve(&Combo::new(Modifiers::command(), "q")), Some(CommandId::Quit));
}

#[test]
fn discard_pane_copy_names_the_file_and_is_destructive() {
    use crate::confirm::discard_pane_copy;
    let copy = discard_pane_copy("main.rs");
    assert!(copy.body.contains("main.rs"));
    assert!(copy.is_destructive);
    assert_eq!(copy.action, "Discard");
}

#[test]
fn discard_tab_copy_counts_unsaved_files() {
    use crate::confirm::discard_tab_copy;
    let copy = discard_tab_copy(2, false);
    assert!(copy.body.contains("2 unsaved files"));
    assert!(copy.is_destructive);
    assert_eq!(copy.action, "Close tab");
}

#[test]
fn discard_tab_copy_warns_about_running_processes_only_when_present() {
    use crate::confirm::discard_tab_copy;
    assert_eq!(discard_tab_copy(1, false).body, "This tab has 1 unsaved file.");
    assert_eq!(
        discard_tab_copy(1, true).body,
        "This tab has 1 unsaved file. Running processes in this tab will be terminated."
    );
}

#[test]
fn quit_copy_full_bodies_are_complete_sentences() {
    use crate::confirm::quit_copy;
    assert_eq!(quit_copy(0, 2).body, "2 unsaved files will be lost.");
    assert_eq!(quit_copy(0, 1).body, "1 unsaved file will be lost.");
    assert_eq!(quit_copy(1, 1).body, "1 session will end and 1 unsaved file will be lost.");
    assert_eq!(quit_copy(2, 0).body, "2 sessions will end.");
    assert!(quit_copy(0, 1).is_destructive);
    assert!(!quit_copy(1, 0).is_destructive);
}

#[test]
fn delete_workspace_copy_full_bodies_are_complete_sentences() {
    use crate::confirm::delete_workspace_copy;
    assert_eq!(
        delete_workspace_copy("Code", 0, 3).body,
        "Its tabs and layout are removed. 3 unsaved files will be lost."
    );
    assert_eq!(
        delete_workspace_copy("Code", 2, 1).body,
        "Its tabs and layout are removed. 2 sessions will end and 1 unsaved file will be lost."
    );
}

#[test]
fn quit_copy_mentions_unsaved_files() {
    use crate::confirm::quit_copy;
    assert!(quit_copy(0, 2).body.contains("2 unsaved files"));
    assert!(quit_copy(1, 1).body.contains("1 unsaved file"));
    assert!(!quit_copy(1, 0).body.contains("unsaved"));
}

#[test]
fn delete_workspace_copy_mentions_unsaved_files() {
    use crate::confirm::delete_workspace_copy;
    assert!(delete_workspace_copy("Code", 0, 3).body.contains("3 unsaved files"));
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
fn island_insets_place_content_under_header_and_grid_uses_inner_size() {
    use crate::terminal_view::{content_bounds, island_grid, ContentInsets, TerminalStyle};
    let theme = Theme::load_default().unwrap();
    let insets = ContentInsets::from_theme(&theme);
    assert_eq!(theme.layout.pane_header_height_px, 26.0);
    assert_eq!(theme.island.pane_padding_bottom_px, 12.0);
    assert_eq!(theme.layout.island_radius_px, 10.0);
    assert_eq!(theme.layout.island_gap_px, 6.0);
    assert_eq!(theme.layout.window_margin_px, 6.0);
    assert_eq!(insets, ContentInsets { x: px(14.0), top: px(32.0), bottom: px(12.0) });
    let island = gpui::Bounds::new(gpui::point(px(100.0), px(50.0)), gpui::size(px(816.0), px(513.0)));
    let inner = content_bounds(island, insets);
    assert_eq!(inner.origin, gpui::point(px(114.0), px(82.0)));
    assert_eq!(inner.size, gpui::size(px(788.0), px(469.0)));
    let cell = gpui::size(px(8.0), px(18.0));
    let style = TerminalStyle::from_theme(&theme, "ui-monospace".into());
    assert_eq!(island_grid(island.size, &style, cell), GridSize::new(98, 26));
    assert_eq!(grid_size_for(island.size, cell), GridSize::new(102, 28));
    let small = gpui::Bounds::new(gpui::point(px(0.0), px(0.0)), gpui::size(px(10.0), px(10.0)));
    assert_eq!(content_bounds(small, insets).size, gpui::size(px(0.0), px(0.0)));
}

#[test]
fn short_cwd_uses_home_mark_and_last_two_segments() {
    use crate::pane_title::short_cwd;
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
    let mut overrides = crate::keymap::ShortcutOverrides::default();
    overrides.set(CommandId::Save, Combo::parse("alt-cmd-s"));
    overrides.set(CommandId::Quit, None);
    let effective = Keymap::with_overrides(&overrides);
    let shortcuts = crate::menus::menu_shortcuts(&effective);
    assert_eq!(shortcuts.len(), effective.len());
    let save = crate::menus::RunCommand(CommandId::Save);
    let save_keys: Vec<String> = shortcuts.iter().filter(|binding| binding.action().partial_eq(&save))
        .flat_map(|binding| binding.keystrokes().iter().map(|keystroke| keystroke.inner().unparse())).collect();
    assert_eq!(save_keys, ["alt-cmd-s"]);
    assert!(!shortcuts.iter().any(|binding| binding.action().partial_eq(&quit)));
}

#[test]
fn menu_bar_lists_native_sections_and_window_shortcuts() {
    let names: Vec<String> = crate::menus::app_menus().iter().map(|menu| menu.name.to_string()).collect();
    assert_eq!(names, ["Onyx", "File", "Edit", "View", "Window"]);
    let map = Keymap::default();
    let cmd = Modifiers::command();
    assert_eq!(map.resolve(&Combo::new(cmd, "m")), Some(CommandId::Minimize));
    assert_eq!(map.combo_for(CommandId::Hide), None);
    assert_eq!(map.combo_for(CommandId::HideOthers), None);
    assert_eq!(map.resolve(&Combo::new(cmd | Modifiers::control(), "f")), Some(CommandId::ToggleFullScreen));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "w")), Some(CommandId::CloseTab));
}

#[test]
fn shortcuts_match_the_physical_key_under_an_arabic_layout() {
    let map = Keymap::default();
    let cmd = Modifiers::command();
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{062d}", None)), Some(CommandId::OpenPalette));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0635}", None)), Some(CommandId::CloseActive));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0641}", None)), Some(CommandId::NewTab));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{064a}", None)), Some(CommandId::SplitDown));
    assert_eq!(map.resolve_keystroke(&keystroke(cmd, "\u{0663}", None)), Some(CommandId::SelectTab(2)));
    let cmd_alt = cmd | Modifiers::alt();
    assert_eq!(map.resolve_keystroke(&keystroke(cmd_alt, "\u{0629}", None)), Some(CommandId::NextWorkspace));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{0629}", None)), Some(CommandId::NextTab));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "\u{062c}", None)), Some(CommandId::PrevTab));
    assert_eq!(map.resolve_keystroke(&keystroke(Modifiers::command_shift(), "p", None)), Some(CommandId::OpenPalette));
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
fn insert_beside_focused_on_the_left_puts_the_new_leaf_first() {
    use crate::tabs::Tab;
    use domain::DropZone;
    let (existing, inserted) = (PanelId::new(), PanelId::new());
    let mut tab = Tab::new(existing);
    tab.insert_beside_focused(inserted, DropZone::Left).unwrap();
    assert_eq!(tab.layout().leaves(), vec![inserted, existing]);
    assert_eq!(tab.focused(), inserted);
}

#[test]
fn cmd_shift_b_toggles_file_tree() {
    let keymap = Keymap::default();
    let combo = Combo::new(Modifiers::command_shift(), "b");
    assert_eq!(keymap.resolve(&combo), Some(CommandId::ToggleFileTree));
}

#[test]
fn drop_zone_uses_edge_bands_with_nearest_edge_winning() {
    use crate::pane_drop::drop_zone_at;
    use domain::DropZone;
    assert_eq!(drop_zone_at(0.5, 0.5), DropZone::Center);
    assert_eq!(drop_zone_at(0.31, 0.69), DropZone::Center);
    assert_eq!(drop_zone_at(0.29, 0.5), DropZone::Left);
    assert_eq!(drop_zone_at(0.75, 0.5), DropZone::Right);
    assert_eq!(drop_zone_at(0.5, 0.1), DropZone::Top);
    assert_eq!(drop_zone_at(0.5, 0.9), DropZone::Bottom);
    assert_eq!(drop_zone_at(0.05, 0.2), DropZone::Left);
    assert_eq!(drop_zone_at(0.2, 0.05), DropZone::Top);
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> gpui::Bounds<gpui::Pixels> {
    gpui::Bounds::new(gpui::point(px(x), px(y)), gpui::size(px(w), px(h)))
}

#[test]
fn pane_hit_testing_only_targets_the_pane_under_the_cursor() {
    use crate::pane_drop::pane_drop_target;
    use domain::DropZone;
    let (source, target, tiny) = (PanelId::new(), PanelId::new(), PanelId::new());
    let panes = [
        (source, rect(0.0, 0.0, 400.0, 600.0)),
        (target, rect(406.0, 0.0, 400.0, 600.0)),
        (tiny, rect(812.0, 0.0, 20.0, 20.0)),
    ];
    let at = |x: f32, y: f32| pane_drop_target(gpui::point(px(x), px(y)), &panes, source);
    assert_eq!(at(403.0, 300.0), None, "gap between islands");
    assert_eq!(at(2000.0, 300.0), None, "outside the window");
    assert_eq!(at(-5.0, -5.0), None, "outside the window");
    assert_eq!(at(200.0, 300.0), None, "own pane");
    assert_eq!(at(410.0, 300.0), Some((target, DropZone::Left)));
    assert_eq!(at(800.0, 300.0), Some((target, DropZone::Right)));
    assert_eq!(at(606.0, 5.0), Some((target, DropZone::Top)));
    assert_eq!(at(606.0, 595.0), Some((target, DropZone::Bottom)));
    assert_eq!(at(606.0, 300.0), Some((target, DropZone::Center)));
    assert_eq!(at(822.0, 10.0), Some((tiny, DropZone::Center)));
    assert_eq!(at(813.0, 10.0), Some((tiny, DropZone::Left)));
}

#[test]
fn tab_hit_testing_skips_the_source_tab_and_drags_need_a_dead_zone() {
    use crate::pane_drop::{has_left_dead_zone, tab_drop_target};
    let tabs = [(0, rect(90.0, 4.0, 80.0, 28.0)), (1, rect(176.0, 4.0, 80.0, 28.0))];
    assert_eq!(tab_drop_target(gpui::point(px(100.0), px(10.0)), &tabs, 0), None);
    assert_eq!(tab_drop_target(gpui::point(px(200.0), px(10.0)), &tabs, 0), Some(1));
    assert_eq!(tab_drop_target(gpui::point(px(200.0), px(60.0)), &tabs, 0), None);
    let press = gpui::point(px(10.0), px(10.0));
    assert!(!has_left_dead_zone(press, gpui::point(px(13.0), px(13.0))));
    assert!(has_left_dead_zone(press, gpui::point(px(16.0), px(10.0))));
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
        ("Split Right", Some("\u{2318}D")),
        ("Split Down", Some("\u{21e7}\u{2318}D")),
        ("Maximize Pane", Some("\u{2318}\u{21a9}")),
        ("-", None),
        ("Rename Pane\u{2026}", None),
        ("Move to New Tab", None),
        ("-", None),
        ("Close Pane", None),
    ];
    let expected: Vec<(String, Option<String>)> =
        expected.iter().map(|(title, shortcut)| (title.to_string(), shortcut.map(str::to_string))).collect();
    assert_eq!(rows, expected);
    let restore = pane_menu_entries(true, &map);
    assert!(matches!(&restore[2], PaneMenuEntry::Item(item) if item.title == "Restore Pane"));
    let commands = selectable_commands(&entries);
    assert_eq!(commands.len(), 6);
    assert_eq!(commands.last(), Some(&CommandId::ClosePane));
    let destructive: Vec<&str> = entries
        .iter()
        .filter_map(|entry| match entry {
            PaneMenuEntry::Item(item) if item.is_destructive() => Some(item.title),
            _ => None,
        })
        .collect();
    assert_eq!(destructive, ["Close Pane"]);
    assert_eq!(
        map.combo_for(CommandId::NewWorkspace).map(|combo| combo.glyphs()).as_deref(),
        Some("\u{21e7}\u{2318}N")
    );
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

#[test]
fn releasing_over_nothing_leaves_the_layout_and_a_pane_drop_moves_it() {
    use crate::pane_drop::{pane_drop_target, DropTarget};
    use domain::DropZone;
    let mut tabs = Tabs::default();
    let (a, b) = (PanelId::new(), PanelId::new());
    tabs.open(a);
    tabs.active_mut().unwrap().split_focused(Axis::Horizontal, b).unwrap();
    let before = tabs.active().unwrap().layout().clone();
    let panes = [(a, rect(0.0, 0.0, 400.0, 600.0)), (b, rect(406.0, 0.0, 400.0, 600.0))];
    let in_gap = pane_drop_target(gpui::point(px(403.0), px(300.0)), &panes, b);
    assert_eq!(in_gap, None);
    if let Some((panel, zone)) = in_gap {
        tabs.drop_panel(b, DropTarget::Pane(panel, zone)).unwrap();
    }
    assert_eq!(tabs.active().unwrap().layout(), &before);
    let (panel, zone) = pane_drop_target(gpui::point(px(200.0), px(590.0)), &panes, b).unwrap();
    assert_eq!((panel, zone), (a, DropZone::Bottom));
    tabs.drop_panel(b, DropTarget::Pane(panel, zone)).unwrap();
    let expected = LayoutNode::Split {
        axis: Axis::Vertical,
        ratio: 0.5,
        first: Box::new(LayoutNode::leaf(a)),
        second: Box::new(LayoutNode::leaf(b)),
    };
    assert_eq!(tabs.active().unwrap().layout(), &expected);
    assert_eq!(tabs.active().unwrap().focused(), b);
}

mod pane_titles {
    use crate::pane_title::{display_title, short_cwd, title_tooltip, OscTitle, TitleSource, MAX_TITLE_CHARS};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use terminal_core::{ForegroundJob, GridSize, InMemoryTerminalService, NativePty, ShellCommand, TerminalHandle};

    const HOME: &str = "/Users/me";
    const JOB_GROUP: i32 = 4242;
    const SHELL_PROMPT_TITLE: &str = "me@host:~";

    fn job(argv: &[&str]) -> ForegroundJob {
        ForegroundJob { process_group: JOB_GROUP, argv: argv.iter().map(|arg| arg.to_string()).collect() }
    }

    fn source<'a>(job: Option<&'a ForegroundJob>, osc: Option<&'a OscTitle>, cwd: &'a str) -> TitleSource<'a> {
        TitleSource { name: None, job, osc, cwd: Path::new(cwd), home: Path::new(HOME) }
    }

    #[test]
    fn rename_wins_over_job_and_cwd() {
        let running = job(&["vim", "notes.md"]);
        let named = TitleSource { name: Some("build"), ..source(Some(&running), None, HOME) };
        assert_eq!(display_title(&named), "build");
    }

    #[test]
    fn running_job_prefers_its_own_osc_title() {
        let running = job(&["/usr/bin/vim", "notes.md"]);
        let osc = OscTitle { text: "notes.md - VIM".to_string(), owner: Some(JOB_GROUP) };
        assert_eq!(display_title(&source(Some(&running), Some(&osc), HOME)), "notes.md - VIM");
    }

    #[test]
    fn running_job_without_osc_title_shows_shortened_argv() {
        let running = job(&["/opt/homebrew/bin/htop", "-d", "10"]);
        let shell_title = OscTitle { text: SHELL_PROMPT_TITLE.to_string(), owner: None };
        assert_eq!(display_title(&source(Some(&running), Some(&shell_title), HOME)), "htop -d 10");
    }

    #[test]
    fn idle_shell_shows_short_cwd_ignoring_shell_title() {
        let shell_title = OscTitle { text: SHELL_PROMPT_TITLE.to_string(), owner: None };
        let idle = source(None, Some(&shell_title), "/Users/me/Development/onyxterm");
        assert_eq!(display_title(&idle), "~/Development/onyxterm");
    }

    #[test]
    fn idle_home_shows_tilde() {
        assert_eq!(display_title(&source(None, None, HOME)), "~");
    }

    #[test]
    fn idle_long_path_collapses_to_last_two_segments() {
        let deep = source(None, None, "/Users/me/Development/Personal/onyxterm/crates");
        assert_eq!(display_title(&deep), "~/\u{2026}/onyxterm/crates");
    }

    #[test]
    fn long_command_truncates_with_ellipsis() {
        let running = job(&["cargo", "test", "--workspace", "--all-targets", "--", "--nocapture", "--test-threads=1"]);
        let title = display_title(&source(Some(&running), None, HOME));
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert!(title.starts_with("cargo test --workspace"), "got {title:?}");
        assert!(title.ends_with('\u{2026}'), "got {title:?}");
    }

    #[test]
    fn tooltip_carries_full_title_and_full_cwd() {
        let running = job(&["sleep", "5"]);
        let busy = source(Some(&running), None, "/Users/me/Development/Personal/onyxterm");
        assert_eq!(title_tooltip(&busy), "sleep 5\n/Users/me/Development/Personal/onyxterm");
        assert_eq!(title_tooltip(&source(None, None, HOME)), HOME);
    }

    fn live_title(handle: &TerminalHandle, home: &Path) -> String {
        let job = handle.foreground_job();
        let cwd = handle.current_dir().unwrap_or_default();
        display_title(&TitleSource { name: None, job: job.as_ref(), osc: None, cwd: &cwd, home })
    }

    fn wait_for_title(handle: &TerminalHandle, home: &Path, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut title = live_title(handle, home);
        while Instant::now() < deadline && title != expected {
            std::thread::sleep(Duration::from_millis(50));
            title = live_title(handle, home);
        }
        title
    }

    #[test]
    fn live_title_follows_foreground_job_in_real_pty() {
        let cwd = std::fs::canonicalize(std::env::temp_dir()).expect("temp dir");
        let home = PathBuf::from(HOME);
        let service = InMemoryTerminalService::new(Box::new(NativePty::new()));
        let command = ShellCommand { program: PathBuf::from("/bin/sh"), args: vec!["-i".to_string()], cwd: cwd.clone() };
        let id = service.spawn_command(command, GridSize::new(40, 5)).expect("spawn");
        let handle = service.handle_for(id).expect("handle");
        let idle = short_cwd(&cwd, &home);
        assert_eq!(wait_for_title(&handle, &home, &idle), idle);
        handle.write(b"sleep 5\n").expect("write");
        assert_eq!(wait_for_title(&handle, &home, "sleep 5"), "sleep 5");
        handle.write(b"\x03").expect("interrupt");
        assert_eq!(wait_for_title(&handle, &home, &idle), idle);
        service.terminate_session(id).expect("terminate");
    }
}

#[test]
fn unnamed_tabs_are_numbered_by_position() {
    let mut tabs = Tabs::default();
    let (a, b, c) = (PanelId::new(), PanelId::new(), PanelId::new());
    tabs.open(a);
    tabs.open(b);
    tabs.open(c);
    assert_eq!(tabs.labels(), ["#1", "#2", "#3"]);
    tabs.close_panel(b).unwrap();
    assert_eq!(tabs.labels(), ["#1", "#2"]);
}

#[test]
fn named_tab_shows_its_name_until_cleared() {
    let mut tabs = Tabs::default();
    tabs.open(PanelId::new());
    tabs.open(PanelId::new());
    tabs.get_mut(1).unwrap().rename(Some("logs".to_string()));
    assert_eq!(tabs.labels(), ["#1", "logs"]);
    tabs.get_mut(1).unwrap().rename(Some("   ".to_string()));
    assert_eq!(tabs.labels(), ["#1", "#2"]);
}

fn assert_color(actual: gpui::Hsla, hex: &str) {
    let expected = hex_to_hsla(hex, "test.expected").unwrap();
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(
        close(actual.h, expected.h) && close(actual.s, expected.s) && close(actual.l, expected.l) && close(actual.a, expected.a),
        "{actual:?} != {hex}"
    );
}

#[test]
fn rgba_colors_parse_channels_and_alpha_and_reject_garbage() {
    use crate::theme::parse_color;
    let separator = parse_color("rgba(0,0,0,0.08)", "test.separator").unwrap();
    assert!(separator.l.abs() < 1e-3);
    assert!((separator.a - 0.08).abs() < 1e-3);
    let menu = parse_color("rgba(255, 255, 255, 0.94)", "test.menu").unwrap();
    assert!((menu.l - 1.0).abs() < 1e-3);
    assert!((menu.a - 0.94).abs() < 1e-3);
    assert_color(parse_color("#007AFF", "test.accent").unwrap(), "#007AFF");
    assert!(parse_color("rgba(0,0,0)", "test.short").is_err());
    assert!(parse_color("rgba(0,0,0,x)", "test.bad").is_err());
    assert!(parse_color("rgba(0,0,0,1", "test.open").is_err());
}

#[test]
fn every_builtin_theme_has_editor_tokens() {
    for kind in ThemeKind::ALL {
        let theme = Theme::load(kind).expect("theme loads");
        assert!(theme.editor.cursor_width_px > 0.0);
        assert!(theme.editor.gutter_padding_px > 0.0);
        assert!(theme.editor.selection.a > 0.0);
    }
}

fn relative_luminance(color: gpui::Hsla) -> f32 {
    let rgba: gpui::Rgba = color.into();
    let linear = |channel: f32| match channel <= 0.03928 {
        true => channel / 12.92,
        false => ((channel + 0.055) / 1.055).powf(2.4),
    };
    0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b)
}

fn contrast_ratio(first: gpui::Hsla, second: gpui::Hsla) -> f32 {
    let (first, second) = (relative_luminance(first), relative_luminance(second));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

#[test]
fn every_builtin_theme_has_readable_syntax_colors() {
    for kind in ThemeKind::ALL {
        let theme = Theme::load(kind).expect("theme loads");
        let syntax = theme.syntax;
        let colors = [syntax.keyword, syntax.string, syntax.number, syntax.type_name, syntax.function,
            syntax.macro_name, syntax.comment, syntax.punctuation, syntax.heading];
        for color in colors {
            let ratio = contrast_ratio(color, theme.surface.canvas);
            assert!(ratio >= 4.5, "{kind:?} syntax color {color:?} has contrast {ratio}");
        }
    }
}

#[test]
fn every_builtin_theme_has_visible_change_marks() {
    for kind in ThemeKind::ALL {
        let theme = Theme::load(kind).expect("theme loads");
        let editor = theme.editor;
        for color in [editor.diff_added, editor.diff_modified] {
            let ratio = contrast_ratio(color, theme.surface.canvas);
            assert!(ratio >= 3.0, "{kind:?} change mark {color:?} has contrast {ratio}");
        }
    }
}

#[test]
fn custom_theme_keeps_its_base_syntax_colors() {
    let mut settings = crate::settings::Settings { theme: ThemeKind::Nord, ..Default::default() };
    settings.customize_current_theme();
    assert_eq!(settings.theme_tokens().syntax.keyword, Theme::load(ThemeKind::Nord).unwrap().syntax.keyword);
}

#[test]
fn every_builtin_theme_reports_its_appearance() {
    let expected = [
        (ThemeKind::Light, Appearance::Light), (ThemeKind::Dark, Appearance::Dark),
        (ThemeKind::Nord, Appearance::Dark), (ThemeKind::Dracula, Appearance::Dark),
        (ThemeKind::SolarizedDark, Appearance::Dark), (ThemeKind::SolarizedLight, Appearance::Light),
    ];
    for (kind, appearance) in expected {
        assert_eq!(Theme::load(kind).unwrap().appearance, appearance, "{kind:?}");
    }
}

#[test]
fn custom_theme_inherits_its_base_appearance() {
    let mut settings = crate::settings::Settings { theme: ThemeKind::SolarizedLight, ..Default::default() };
    settings.customize_current_theme();
    assert_eq!(settings.theme, ThemeKind::Custom);
    assert_eq!(settings.theme_tokens().appearance, Appearance::Light);
}

#[test]
fn apple_light_theme_loads_every_redesign_token() {
    let theme = Theme::load_default().unwrap();
    assert_color(theme.surface.window, "#ECECEF");
    assert_color(theme.surface.settings_window, "#F2F2F4");
    assert_color(theme.surface.canvas, "#FFFFFF");
    assert_color(theme.text.default, "#1D1D1F");
    assert_color(theme.text.muted, "#6E6E73");
    assert_color(theme.text.subtle, "#8A8A90");
    assert_color(theme.accent.primary, "#007AFF");
    assert_color(theme.chrome.destructive, "#D70015");
    assert_color(theme.status.running, "#248A3D");
    assert_color(theme.status.idle, "#B4B4B9");
    assert!((theme.border.muted.a - 0.07).abs() < 1e-3);
    assert!((theme.border.default.a - 0.11).abs() < 1e-3);
    assert!((theme.border.focus.a - 0.5).abs() < 1e-3);
    assert!((theme.accent.selection.a - 0.12).abs() < 1e-3);
    assert!((theme.chrome.menu_fill.a - 0.985).abs() < 1e-3);
    assert_eq!(theme.ui_font.family, ".SystemUIFont");
    assert_eq!(
        [theme.ui_font.tab_px, theme.ui_font.pane_title_px, theme.ui_font.menu_px, theme.ui_font.palette_input_px],
        [12.0, 11.5, 13.0, 16.0]
    );
    assert_eq!(theme.ui_font.hint_px, 11.0);
    assert_eq!(theme.layout.tab_strip_height_px, 38.0);
    assert_eq!(theme.top_bar.control_height_px, 26.0);
    assert_eq!(theme.top_bar.tab_gap_px, 2.0);
    assert_eq!(theme.menu.row_height_px, 26.0);
    assert_eq!(theme.palette.width_px, 600.0);
    assert_eq!(theme.palette.row_height_px, 32.0);
    assert_eq!(theme.dialog.radius_px, 12.0);
    assert!(theme.island.dim_unfocused);
    assert_eq!(theme.island.unfocused_opacity, 0.8);
    assert_eq!((theme.island.pane_padding_x_px, theme.island.pane_padding_top_px), (14.0, 6.0));
    assert_eq!(theme.island.focus_ring_px, 1.0);
}

#[test]
fn shadow_tokens_become_layered_box_shadows() {
    use crate::theme::box_shadows;
    let theme = Theme::load_default().unwrap();
    let tab = box_shadows(&theme.shadow.tab);
    assert_eq!(tab.len(), 2);
    assert_eq!(tab[0].spread_radius, px(0.5));
    assert_eq!(tab[0].blur_radius, px(0.0));
    assert!((tab[0].color.a - 0.11).abs() < 1e-3);
    assert_eq!(tab[1].offset, gpui::point(px(0.0), px(1.0)));
    assert_eq!(tab[1].blur_radius, px(2.0));
    let menu = box_shadows(&theme.shadow.menu);
    assert_eq!((menu[0].offset.y, menu[0].blur_radius), (px(12.0), px(32.0)));
    let palette = box_shadows(&theme.shadow.palette);
    assert_eq!((palette[0].offset.y, palette[0].blur_radius), (px(30.0), px(80.0)));
}

#[test]
fn onyx_dark_theme_matches_the_design_board() {
    let theme = Theme::load(ThemeKind::Dark).unwrap();
    assert_color(theme.surface.window, "#151517");
    assert_color(theme.surface.canvas, "#1E1E20");
    assert_color(theme.surface.raised, "#2C2C2E");
    assert_color(theme.surface.settings_window, "#232325");
    assert_color(theme.text.default, "#E8EAEE");
    assert_color(theme.text.subtle, "#7D838F");
    assert_color(theme.chrome.destructive, "#FF6961");
    assert_color(theme.status.failed, "#FF453A");
    assert!((theme.accent.selection.a - 0.18).abs() < 1e-3);
    assert!((theme.border.default.a - 0.12).abs() < 1e-3);
    assert!((theme.border.muted.a - 0.07).abs() < 1e-3);
}

#[test]
fn palette_rows_carry_category_and_bound_shortcut() {
    use crate::app::PaletteChoice;
    use crate::command::open_choice_palette;
    use crate::palette::{palette_entries, PaletteRow, CHOICE_CATEGORY};
    let mut state = AppState::new();
    open_palette(&mut state);
    let rows = palette_entries(&state);
    let row = |title: &str| rows.iter().find(|row| row.title == title).cloned().unwrap();
    assert_eq!(
        row("Split Right"),
        PaletteRow { title: "Split Right".into(), category: "Panes", shortcut: Some("\u{2318}D".into()) }
    );
    assert_eq!(row("New Tab").category, "Tabs");
    assert_eq!(row("New Tab").shortcut.as_deref(), Some("\u{2318}T"));
    assert_eq!(row("New Workspace").category, "Workspaces");
    assert_eq!(row("New Workspace").shortcut.as_deref(), Some("\u{21e7}\u{2318}N"));
    assert_eq!(row("Rename Tab").shortcut, None);
    let choice = PaletteChoice { key: "k".into(), title: "Backend".into() };
    open_choice_palette(&mut state, vec![choice]);
    let choices = palette_entries(&state);
    assert_eq!(choices, [PaletteRow { title: "Backend".into(), category: CHOICE_CATEGORY, shortcut: None }]);
    assert_eq!(CHOICE_CATEGORY, "Workspaces");
}

#[test]
fn workspace_button_falls_back_to_scratch_and_rows_count_tabs() {
    use crate::workspace_menu::{tab_count_label, workspace_button_label};
    assert_eq!(workspace_button_label(Some("Backend")), "Backend");
    assert_eq!(workspace_button_label(Some("  ")), "Scratch");
    assert_eq!(workspace_button_label(None), "Scratch");
    assert_eq!(tab_count_label(0), "0 tabs");
    assert_eq!(tab_count_label(1), "1 tab");
    assert_eq!(tab_count_label(3), "3 tabs");
}

#[test]
fn top_bar_leading_inset_drops_traffic_lights_in_fullscreen() {
    let theme = Theme::load_default().unwrap();
    let windowed = theme.layout.traffic_lights_width_px + theme.top_bar.leading_gap_px;
    assert_eq!(theme.top_bar_leading_inset_px(false), windowed);
    assert_eq!(theme.top_bar_leading_inset_px(true), theme.layout.window_margin_px);
}

fn look_for(is_focused: bool, is_split: bool) -> crate::island_look::IslandLook {
    use crate::island_look::{IslandLook, IslandRole};
    IslandLook::for_role(&Theme::load_default().unwrap(), IslandRole { is_focused, is_split })
}

#[test]
fn focus_ring_shows_only_on_the_focused_island_of_a_split() {
    let theme = Theme::load_default().unwrap();
    let ring = look_for(true, true);
    assert_eq!((ring.edge_px, ring.edge_color), (1.0, theme.border_focus()));
    for (is_focused, is_split) in [(true, false), (false, true), (false, false)] {
        let hairline = look_for(is_focused, is_split);
        assert_eq!((hairline.edge_px, hairline.edge_color), (0.5, theme.border_default()));
    }
}

#[test]
fn unfocused_content_dims_only_in_a_split_with_dimming_enabled() {
    let dimmed = look_for(false, true);
    assert!(dimmed.is_dimmed());
    assert_eq!(dimmed.content_opacity, 0.8);
    assert!(!look_for(true, true).is_dimmed());
    assert!(!look_for(false, false).is_dimmed());
    assert!(!look_for(true, false).is_dimmed());

    let mut theme = Theme::load_default().unwrap();
    theme.island.dim_unfocused = false;
    let role = crate::island_look::IslandRole { is_focused: false, is_split: true };
    assert!(!crate::island_look::IslandLook::for_role(&theme, role).is_dimmed());
}

#[test]
fn ghost_islands_keep_the_look_they_had_when_they_left_the_layout() {
    use crate::island_look::IslandLooks;
    use std::collections::HashMap;
    let leaving = PanelId::new();
    let staying = PanelId::new();
    let mut looks = IslandLooks::default();
    looks.remember(HashMap::from([(leaving, look_for(false, true)), (staying, look_for(true, true))]), &[]);
    looks.remember(HashMap::from([(staying, look_for(true, false))]), &[leaving]);
    assert_eq!(looks.get(leaving), Some(look_for(false, true)));
    assert_eq!(looks.get(staying), Some(look_for(true, false)));
    looks.remember(HashMap::from([(staying, look_for(true, false))]), &[]);
    assert_eq!(looks.get(leaving), None);
}

fn split_node(axis: Axis, first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split { axis, ratio: 0.5, first: Box::new(first), second: Box::new(second) }
}

#[test]
fn directional_neighbor_follows_pane_geometry() {
    use crate::pane_layout::neighbor_toward;
    use domain::Direction;
    let (a, b, c, d) = (PanelId::new(), PanelId::new(), PanelId::new(), PanelId::new());
    let right_column = split_node(Axis::Vertical, LayoutNode::leaf(b), LayoutNode::leaf(c));
    let top = split_node(Axis::Horizontal, LayoutNode::leaf(a), right_column);
    let layout = split_node(Axis::Vertical, top, LayoutNode::leaf(d));
    assert_eq!(neighbor_toward(&layout, a, Direction::Right), Some(b));
    assert_eq!(neighbor_toward(&layout, a, Direction::Left), None);
    assert_eq!(neighbor_toward(&layout, a, Direction::Up), None);
    assert_eq!(neighbor_toward(&layout, c, Direction::Left), Some(a));
    assert_eq!(neighbor_toward(&layout, b, Direction::Down), Some(c));
    assert_eq!(neighbor_toward(&layout, c, Direction::Down), Some(d));
    assert_eq!(neighbor_toward(&layout, d, Direction::Up), Some(a));
    assert_eq!(neighbor_toward(&layout, PanelId::new(), Direction::Up), None);
}

#[test]
fn tab_focuses_swaps_and_resizes_toward_a_direction() {
    use domain::Direction;
    let mut tabs = Tabs::default();
    let (a, b) = (PanelId::new(), PanelId::new());
    tabs.open(a);
    let tab = tabs.active_mut().unwrap();
    tab.split_focused(Axis::Horizontal, b).unwrap();
    tab.focus_toward(Direction::Left);
    assert_eq!(tab.focused(), a);
    tab.focus_toward(Direction::Left);
    assert_eq!(tab.focused(), a);
    tab.focus_toward(Direction::Right);
    assert_eq!(tab.focused(), b);
    tab.swap_toward(Direction::Left).unwrap();
    assert_eq!(tab.layout().leaves(), vec![b, a]);
    assert_eq!(tab.focused(), b);
    tab.swap_toward(Direction::Up).unwrap();
    assert_eq!(tab.layout().leaves(), vec![b, a]);
    tab.resize_toward(Direction::Right);
    let LayoutNode::Split { ratio, .. } = tab.layout() else { panic!("expected split") };
    assert!((ratio - (0.5 + crate::tabs::RESIZE_STEP)).abs() < 1e-6);
}

#[test]
fn keymap_binds_vim_movement_with_command_held() {
    use domain::Direction;
    let map = Keymap::default();
    let cmd = Modifiers::command();
    assert_eq!(map.resolve(&Combo::new(cmd, "h")), Some(CommandId::FocusPane(Direction::Left)));
    assert_eq!(map.resolve(&Combo::new(cmd, "j")), Some(CommandId::FocusPane(Direction::Down)));
    assert_eq!(map.resolve(&Combo::new(cmd, "k")), Some(CommandId::FocusPane(Direction::Up)));
    assert_eq!(map.resolve(&Combo::new(cmd, "l")), Some(CommandId::FocusPane(Direction::Right)));
    assert_eq!(map.resolve(&Combo::new(cmd | Modifiers::alt(), "up")), Some(CommandId::FocusPane(Direction::Up)));
    assert_eq!(map.combo_for(CommandId::FocusPane(Direction::Left)), Some(&Combo::new(cmd, "h")));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "l")), Some(CommandId::SwapPane(Direction::Right)));
    assert_eq!(map.resolve(&Combo::new(cmd | Modifiers::control(), "j")), Some(CommandId::ResizePane(Direction::Down)));
    assert_eq!(map.resolve(&Combo::new(cmd, "d")), Some(CommandId::SplitRight));
    assert_eq!(map.resolve(&Combo::new(cmd, "enter")), Some(CommandId::MaximizePane));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "g")), Some(CommandId::GridLayout));
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "e")), Some(CommandId::EqualizePanes));
    assert_eq!(map.combo_for(CommandId::FocusAdjacent), None);
}

#[test]
fn tab_grid_keeps_panes_and_focus_and_clears_maximize() {
    let mut tabs = Tabs::default();
    let (a, b, c) = (PanelId::new(), PanelId::new(), PanelId::new());
    tabs.open(a);
    let tab = tabs.active_mut().unwrap();
    tab.split_focused(Axis::Horizontal, b).unwrap();
    tab.split_focused(Axis::Horizontal, c).unwrap();
    tab.toggle_maximized();
    tab.arrange_grid();
    assert_eq!(tab.layout().leaves(), vec![a, b, c]);
    assert_eq!(tab.focused(), c);
    assert!(!tab.is_maximized());
    assert!(matches!(tab.layout(), LayoutNode::Split { axis: Axis::Vertical, .. }));
    tab.equalize();
    tab.layout().validate().unwrap();
}

#[test]
fn copy_mode_maps_vim_keys_to_actions() {
    use crate::copy_mode::{copy_mode_action, CopyModeAction};
    use terminal_core::ViMotion;
    let none = Modifiers::none();
    let typed = |key: &str, text: &str| keystroke(none, key, Some(text));
    assert_eq!(copy_mode_action(&typed("h", "h"), false), CopyModeAction::Motion(ViMotion::Left));
    assert_eq!(copy_mode_action(&typed("j", "j"), false), CopyModeAction::Motion(ViMotion::Down));
    assert_eq!(copy_mode_action(&typed("w", "w"), false), CopyModeAction::Motion(ViMotion::SemanticRight));
    assert_eq!(copy_mode_action(&typed("b", "b"), false), CopyModeAction::Motion(ViMotion::SemanticLeft));
    assert_eq!(copy_mode_action(&typed("e", "e"), false), CopyModeAction::Motion(ViMotion::SemanticRightEnd));
    assert_eq!(copy_mode_action(&typed("0", "0"), false), CopyModeAction::Motion(ViMotion::First));
    assert_eq!(copy_mode_action(&typed("4", "$"), false), CopyModeAction::Motion(ViMotion::Last));
    assert_eq!(copy_mode_action(&typed("g", "g"), false), CopyModeAction::AwaitTop);
    assert_eq!(copy_mode_action(&typed("g", "g"), true), CopyModeAction::Top);
    assert_eq!(copy_mode_action(&keystroke(Modifiers::shift(), "g", Some("G")), false), CopyModeAction::Bottom);
    assert_eq!(copy_mode_action(&keystroke(Modifiers::shift(), "g", None), false), CopyModeAction::Bottom);
    assert_eq!(copy_mode_action(&typed("v", "v"), false), CopyModeAction::ToggleSelection(SelectionKind::Simple));
    assert_eq!(
        copy_mode_action(&keystroke(Modifiers::shift(), "v", Some("V")), false),
        CopyModeAction::ToggleSelection(SelectionKind::Line)
    );
    assert_eq!(copy_mode_action(&typed("y", "y"), false), CopyModeAction::Yank);
    assert_eq!(copy_mode_action(&typed("q", "q"), false), CopyModeAction::Exit);
    assert_eq!(copy_mode_action(&keystroke(none, "escape", None), false), CopyModeAction::Exit);
    assert_eq!(copy_mode_action(&keystroke(Modifiers::control(), "u", None), false), CopyModeAction::HalfPageUp);
    assert_eq!(copy_mode_action(&keystroke(Modifiers::control(), "d", None), false), CopyModeAction::HalfPageDown);
    assert_eq!(copy_mode_action(&typed("x", "x"), false), CopyModeAction::Ignore);
    let map = Keymap::default();
    assert_eq!(map.resolve(&Combo::new(Modifiers::command_shift(), "c")), Some(CommandId::ToggleCopyMode));
}

#[test]
fn cmd_shift_o_opens_folder() {
    let keymap = Keymap::default();
    let combo = Combo::new(Modifiers::command_shift(), "o");
    assert_eq!(keymap.resolve(&combo), Some(CommandId::OpenFolder));
}

#[test]
fn cmd_e_opens_editor_here() {
    let keymap = Keymap::default();
    let combo = Combo::new(Modifiers::command(), "e");
    assert_eq!(keymap.resolve(&combo), Some(CommandId::OpenEditorHere));
}

#[test]
fn cmd_shift_y_toggles_editor_bundle() {
    let keymap = Keymap::default();
    let combo = Combo::new(Modifiers::command_shift(), "y");
    assert_eq!(keymap.resolve(&combo), Some(CommandId::ToggleEditorBundle));
}

#[test]
fn cmd_s_saves() {
    let keymap = Keymap::default();
    assert_eq!(keymap.resolve(&Combo::new(Modifiers::command(), "s")), Some(CommandId::Save));
}

#[test]
fn wrap_focused_with_pair_places_pair_to_the_right() {
    use crate::tabs::Tab;
    let (focused, file_tree, editor) = (PanelId::new(), PanelId::new(), PanelId::new());
    let mut tab = Tab::new(focused);
    tab.wrap_focused_with_pair(Axis::Horizontal, 0.4, 0.5, file_tree, editor, false).unwrap();

    let leaves = tab.layout().leaves();
    assert_eq!(leaves, vec![focused, file_tree, editor]);
    assert_eq!(tab.focused(), editor);
}

#[test]
fn wrap_focused_with_pair_places_pair_above() {
    use crate::tabs::Tab;
    let (focused, file_tree, editor) = (PanelId::new(), PanelId::new(), PanelId::new());
    let mut tab = Tab::new(focused);
    tab.wrap_focused_with_pair(Axis::Vertical, 0.4, 0.5, file_tree, editor, true).unwrap();

    let leaves = tab.layout().leaves();
    assert_eq!(leaves, vec![file_tree, editor, focused]);
    assert_eq!(tab.focused(), editor);
}

#[test]
fn closing_both_pair_panels_collapses_back_to_focused() {
    use crate::tabs::Tabs;
    let mut tabs = Tabs::default();
    let (focused, file_tree, editor) = (PanelId::new(), PanelId::new(), PanelId::new());
    tabs.open(focused);
    tabs.active_mut().unwrap().wrap_focused_with_pair(Axis::Horizontal, 0.4, 0.5, file_tree, editor, false).unwrap();

    tabs.close_panel(file_tree).unwrap();
    let tab = tabs.active().unwrap();
    let LayoutNode::Split { first, second, .. } = tab.layout() else { panic!("expected outer split") };
    assert!(matches!(first.as_ref(), LayoutNode::Leaf { .. }));
    assert!(matches!(second.as_ref(), LayoutNode::Leaf { panel_id } if *panel_id == editor));

    tabs.close_panel(editor).unwrap();
    let tab = tabs.active().unwrap();
    assert_eq!(tab.layout(), &LayoutNode::leaf(focused));
    assert_eq!(tab.focused(), focused);
}

#[test]
fn keycaps_split_modifier_glyphs_from_the_key() {
    let map = Keymap::default();
    let next_document = map.combo_for(CommandId::NextDocument).expect("next document is bound");
    assert_eq!(next_document.keycaps(), ["\u{2303}", "\u{21e5}"]);
    let new_workspace = map.combo_for(CommandId::NewWorkspace).expect("new workspace is bound");
    assert_eq!(new_workspace.keycaps(), ["\u{21e7}", "\u{2318}", "N"]);
    assert_eq!(new_workspace.glyphs(), new_workspace.keycaps().concat());
}

#[test]
fn shortcut_groups_follow_registry_category_order_and_list_unbound_commands() {
    use crate::keymap::shortcut_groups;
    let registry = CommandRegistry::with_defaults();
    let mut map = Keymap::new();
    map.bind(Combo::new(Modifiers::command(), "s"), CommandId::Save);
    map.bind(Combo::new(Modifiers::command_shift(), "left"), CommandId::MoveTabLeft);
    let groups = shortcut_groups(&registry, &map, "");
    let categories: Vec<&str> = groups.iter().map(|group| group.category).collect();
    assert_eq!(categories, ["Tabs", "Panes", "Palette", "Application", "Files", "Workspaces", "Edit", "Window"]);
    let listed: usize = groups.iter().map(|group| group.shortcuts.len()).sum();
    assert_eq!(listed, registry.list().len() - 1);
    assert!(groups.iter().flat_map(|group| &group.shortcuts).all(|shortcut| shortcut.command != CommandId::ClosePalette));
    let tab_titles: Vec<&str> = groups[0].shortcuts.iter().map(|shortcut| shortcut.title).collect();
    assert_eq!(tab_titles[..7], ["New Tab", "Move Tab Left", "Move Tab Right", "Next Tab", "Rename Tab",
        "Previous Tab", "Close Tab"]);
    assert_eq!(tab_titles[7..], ["Select Tab 1", "Select Tab 2", "Select Tab 3", "Select Tab 4", "Select Tab 5",
        "Select Tab 6", "Select Tab 7", "Select Tab 8", "Select Tab 9"]);
    assert_eq!(groups[0].shortcuts[0].combo, None);
    assert_eq!(groups[0].shortcuts[1].combo, Some(Combo::new(Modifiers::command_shift(), "left")));
    let save = groups[4].shortcuts.iter().find(|shortcut| shortcut.command == CommandId::Save).unwrap();
    assert_eq!(save.combo, Some(Combo::new(Modifiers::command(), "s")));
}

#[test]
fn shortcut_filter_matches_title_or_category_ignoring_case() {
    use crate::keymap::shortcut_groups;
    let registry = CommandRegistry::with_defaults();
    let map = Keymap::default();
    let titles = |query: &str| -> Vec<&str> {
        shortcut_groups(&registry, &map, query).iter()
            .flat_map(|group| group.shortcuts.iter().map(|shortcut| shortcut.title)).collect()
    };
    assert_eq!(titles("SPLIT"), ["Split Right", "Split Down"]);
    assert_eq!(titles("  workspaces ")[..6], ["Switch Workspace", "New Workspace", "Rename Workspace\u{2026}",
        "Next Workspace", "Previous Workspace", "Open Workspace Menu"]);
    assert_eq!(titles("select workspace 9"), ["Select Workspace 9"]);
    assert!(titles("no such command").is_empty());
    let categories: Vec<&str> = shortcut_groups(&registry, &map, "copy").iter().map(|group| group.category).collect();
    assert_eq!(categories, ["Edit"]);
}
