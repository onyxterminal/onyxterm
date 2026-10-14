#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandId;
use crate::keymap::Keymap;
use gpui::{App, KeyBinding, Menu, MenuItem};

const MENU_ONLY_CONTEXT: &str = "OnyxtermMenuShortcut";

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = onyxterm, no_json)]
pub struct RunCommand(pub CommandId);

pub fn install_menus(keymap: &Keymap, cx: &mut App) {
    cx.clear_key_bindings();
    cx.bind_keys(menu_shortcuts(keymap));
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
        menu("Onyx", &[Some((About, "About Onyx")), Some((OpenSettings, "Settings\u{2026}")), None, Some((Hide, "Hide Onyx")),
            Some((HideOthers, "Hide Others")), Some((ShowAll, "Show All")), None, Some((Quit, "Quit Onyx"))]),
        menu("File", &[Some((NewTab, "New Tab")), Some((NewWorkspace, "New Workspace\u{2026}")), None,
            Some((OpenFile, "Open File\u{2026}")), Some((OpenFolder, "Open Folder\u{2026}")),
            Some((OpenEditorHere, "Open Editor Here")), Some((ToggleFileTree, "Toggle File Tree")),
            Some((Save, "Save")), None,
            Some((NextDocument, "Next Document")), Some((PrevDocument, "Previous Document")),
            Some((CloseDocument, "Close Document")), None,
            Some((CloseActive, "Close")), Some((ClosePane, "Close Pane")), Some((CloseTab, "Close Tab"))]),
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
