#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::PanelId;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    Focus(PanelId),
    Replace(PanelId),
    SplitRightOf(PanelId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenEditor {
    pub panel: PanelId,
    pub path: PathBuf,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabEditors {
    pub tree: PanelId,
    pub editors: Vec<OpenEditor>,
    pub last_editor: Option<PanelId>,
}

pub fn choose_editor_target(tab: &TabEditors, path: &Path) -> OpenTarget {
    if let Some(showing) = tab.editors.iter().find(|editor| editor.path == path) {
        return OpenTarget::Focus(showing.panel);
    }
    let reusable = tab
        .last_editor
        .and_then(|last| tab.editors.iter().find(|editor| editor.panel == last))
        .filter(|editor| !editor.is_dirty);
    match reusable {
        Some(editor) => OpenTarget::Replace(editor.panel),
        None => OpenTarget::SplitRightOf(tab.tree),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(path: &str, is_dirty: bool) -> OpenEditor {
        OpenEditor { panel: PanelId::new(), path: PathBuf::from(path), is_dirty }
    }

    fn tab(editors: Vec<OpenEditor>, last_editor: Option<PanelId>) -> TabEditors {
        TabEditors { tree: PanelId::new(), editors, last_editor }
    }

    #[test]
    fn an_editor_already_showing_the_path_is_focused() {
        let showing = editor("/r/a.rs", true);
        let other = editor("/r/b.rs", false);
        let tab = tab(vec![other.clone(), showing.clone()], Some(other.panel));

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::Focus(showing.panel));
    }

    #[test]
    fn a_clean_last_focused_editor_is_reused() {
        let last = editor("/r/b.rs", false);
        let tab = tab(vec![editor("/r/c.rs", false), last.clone()], Some(last.panel));

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::Replace(last.panel));
    }

    #[test]
    fn a_dirty_last_focused_editor_forces_a_split() {
        let last = editor("/r/b.rs", true);
        let tab = tab(vec![editor("/r/c.rs", false), last.clone()], Some(last.panel));

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::SplitRightOf(tab.tree));
    }

    #[test]
    fn no_editor_in_the_tab_forces_a_split() {
        let tab = tab(Vec::new(), Some(PanelId::new()));

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::SplitRightOf(tab.tree));
    }
}
