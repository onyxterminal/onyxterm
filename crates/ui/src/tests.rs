#![allow(clippy::disallowed_methods)]

use crate::app::AppState;
use crate::command::{toggle_palette, CommandId, CommandRegistry};
use crate::keymap::{Combo, Keymap};
use crate::theme::{hex_to_hsla, Theme};
use gpui::Modifiers;

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
