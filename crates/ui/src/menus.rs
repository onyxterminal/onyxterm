#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandId;
use crate::keymap::Keymap;
use gpui::{App, KeyBinding, Menu, MenuItem};

const MENU_ONLY_CONTEXT: &str = "OnyxtermMenuShortcut";

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = onyxterm, no_json)]
pub struct RunCommand(pub CommandId);

pub fn install_menus(cx: &mut App) {
    cx.bind_keys(menu_shortcuts(&Keymap::default()));
    cx.set_menus(app_menus());
}

pub fn menu_shortcuts(keymap: &Keymap) -> Vec<KeyBinding> {
    keymap
        .iter()
        .map(|(combo, id)| KeyBinding::new(&combo.binding_text(), RunCommand(*id), Some(MENU_ONLY_CONTEXT)))
        .collect()
}

pub fn app_menus() -> Vec<Menu> {
    use CommandId::*;
    vec![
        menu("Onyxterm", &[Some((About, "About Onyxterm")), Some((OpenSettings, "Settings\u{2026}")), None, Some((Hide, "Hide Onyxterm")),
            Some((HideOthers, "Hide Others")), Some((ShowAll, "Show All")), None, Some((Quit, "Quit Onyxterm"))]),
        menu("File", &[Some((NewTab, "New Tab")), Some((NewWorkspace, "New Workspace\u{2026}")), None,
            Some((CloseActive, "Close Pane")), Some((CloseTab, "Close Tab"))]),
        menu("Edit", &[Some((Copy, "Copy")), Some((Paste, "Paste")), Some((SelectAll, "Select All"))]),
        menu("View", &[Some((SplitRight, "Split Right")), Some((SplitDown, "Split Down")),
            Some((MaximizePane, "Maximize Pane")), None, Some((ToggleFullScreen, "Enter Full Screen")), None,
            Some((OpenPalette, "Command Palette"))]),
        menu("Window", &[Some((Minimize, "Minimize")), Some((Zoom, "Zoom")), None, Some((NextTab, "Next Tab")),
            Some((PrevTab, "Previous Tab")), None, Some((SwitchWorkspace, "Switch Workspace\u{2026}"))]),
    ]
}

fn menu(name: &'static str, entries: &[Option<(CommandId, &'static str)>]) -> Menu {
    let items = entries
        .iter()
        .map(|entry| match entry {
            Some((id, title)) => MenuItem::action(*title, RunCommand(*id)),
            None => MenuItem::separator(),
        })
        .collect();
    Menu { name: name.into(), items }
}
