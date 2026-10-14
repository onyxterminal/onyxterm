#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::PanelId;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    Activate { panel: PanelId, index: usize },
    AddTo(PanelId),
    NewPane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenEditor {
    pub panel: PanelId,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabEditors {
    pub focused: Option<PanelId>,
    pub editors: Vec<OpenEditor>,
    pub recent: Vec<PanelId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentEditors {
    panels: Vec<PanelId>,
}

impl RecentEditors {
    pub fn touch(&mut self, panel: PanelId) {
        self.forget(panel);
        self.panels.insert(0, panel);
    }

    pub fn forget(&mut self, panel: PanelId) {
        self.panels.retain(|recent| *recent != panel);
    }

    pub fn order(&self) -> Vec<PanelId> {
        self.panels.clone()
    }
}

impl TabEditors {
    fn showing(&self, path: &Path) -> Option<OpenTarget> {
        self.editors.iter().find_map(|editor| {
            let index = editor.paths.iter().position(|open| open == path)?;
            Some(OpenTarget::Activate { panel: editor.panel, index })
        })
    }

    fn host(&self) -> Option<PanelId> {
        let is_editor = |panel: &PanelId| self.editors.iter().any(|editor| editor.panel == *panel);
        self.focused
            .filter(is_editor)
            .or_else(|| self.recent.iter().copied().find(is_editor))
            .or_else(|| self.editors.first().map(|editor| editor.panel))
    }
}

pub fn choose_editor_target(tab: &TabEditors, path: &Path) -> OpenTarget {
    if let Some(showing) = tab.showing(path) {
        return showing;
    }
    tab.host().map_or(OpenTarget::NewPane, OpenTarget::AddTo)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(paths: &[&str]) -> OpenEditor {
        OpenEditor { panel: PanelId::new(), paths: paths.iter().map(PathBuf::from).collect() }
    }

    fn tab(focused: Option<PanelId>, editors: Vec<OpenEditor>, recent: Vec<PanelId>) -> TabEditors {
        TabEditors { focused, editors, recent }
    }

    #[test]
    fn a_document_already_open_in_the_tab_is_activated() {
        let other = editor(&["/r/b.rs"]);
        let showing = editor(&["/r/c.rs", "/r/a.rs"]);
        let tab = tab(Some(other.panel), vec![other.clone(), showing.clone()], vec![other.panel]);

        let target = choose_editor_target(&tab, Path::new("/r/a.rs"));
        assert_eq!(target, OpenTarget::Activate { panel: showing.panel, index: 1 });
    }

    #[test]
    fn a_new_file_joins_the_focused_editor_pane() {
        let first = editor(&["/r/b.rs"]);
        let focused = editor(&["/r/c.rs"]);
        let tab = tab(Some(focused.panel), vec![first.clone(), focused.clone()], vec![first.panel]);

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::AddTo(focused.panel));
    }

    #[test]
    fn without_a_focused_editor_the_most_recent_editor_in_the_tab_hosts_the_file() {
        let first = editor(&["/r/b.rs"]);
        let recent = editor(&["/r/c.rs"]);
        let tab = tab(Some(PanelId::new()), vec![first.clone(), recent.clone()], vec![recent.panel, first.panel]);

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::AddTo(recent.panel));
    }

    #[test]
    fn recent_editors_from_other_tabs_are_skipped_for_the_first_in_layout_order() {
        let first = editor(&["/r/b.rs"]);
        let second = editor(&["/r/c.rs"]);
        let elsewhere = PanelId::new();
        let tab = tab(None, vec![first.clone(), second], vec![elsewhere]);

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::AddTo(first.panel));
    }

    #[test]
    fn a_new_pane_is_created_only_when_the_tab_has_no_editor() {
        let tab = tab(None, Vec::new(), vec![PanelId::new()]);

        assert_eq!(choose_editor_target(&tab, Path::new("/r/a.rs")), OpenTarget::NewPane);
    }

    #[test]
    fn a_closed_editor_is_forgotten_by_the_recent_list() {
        let mut recent = RecentEditors::default();
        let closed = PanelId::new();
        let kept = PanelId::new();
        recent.touch(kept);
        recent.touch(closed);
        recent.forget(closed);

        assert_eq!(recent.order(), vec![kept]);
    }

    #[test]
    fn touching_moves_an_editor_to_the_front() {
        let mut recent = RecentEditors::default();
        let (first, second) = (PanelId::new(), PanelId::new());
        recent.touch(first);
        recent.touch(second);
        recent.touch(first);

        assert_eq!(recent.order(), vec![first, second]);
    }
}
