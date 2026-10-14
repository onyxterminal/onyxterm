use super::Shell;
use crate::document::{document_title, DocumentPane, FileTreePane};
use crate::file_tree::TreeAction;
use crate::file_tree_view::{tree_list, tree_row};
use domain::{Panel, PanelId};
use gpui::{
    AnyElement, ClipboardItem, Context, Div, InteractiveElement, IntoElement, KeyDownEvent, Keystroke, MouseButton,
    MouseDownEvent, MouseUpEvent, ScrollStrategy, SharedString,
};
use std::ops::Range;
use std::path::PathBuf;

const DOUBLE_CLICK: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TreeKey {
    Move(isize),
    Toggle,
    Expand,
    CollapseOrParent,
    CopyPath,
}

pub(super) fn tree_key(keystroke: &Keystroke) -> Option<TreeKey> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
        return None;
    }
    match keystroke.key.as_str() {
        "j" | "down" => Some(TreeKey::Move(1)),
        "k" | "up" => Some(TreeKey::Move(-1)),
        "enter" | "l" => Some(TreeKey::Toggle),
        "right" => Some(TreeKey::Expand),
        "h" | "left" => Some(TreeKey::CollapseOrParent),
        "y" => Some(TreeKey::CopyPath),
        _ => None,
    }
}

fn reveal_strategy(delta: isize) -> ScrollStrategy {
    if delta < 0 {
        return ScrollStrategy::Top;
    }
    ScrollStrategy::Bottom
}

impl Shell {
    pub(super) fn load_tree_dir(&mut self, panel: PanelId, dir: PathBuf, cx: &mut Context<Self>) {
        let listed_dir = dir.clone();
        let task = cx.background_executor().spawn(async move { fs_service::list_dir(&listed_dir) });
        cx.spawn(async move |shell, cx| {
            let listed = task.await;
            let updated = shell.update(cx, |shell, cx| {
                if let Some(DocumentPane::FileTree(pane)) = shell.documents.get_mut(&panel) {
                    pane.apply_listing(&dir, listed);
                }
                cx.notify();
            });
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before listing finished");
            }
        })
        .detach();
    }

    pub(super) fn send_key_to_tree(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(panel) = self.focused_tree() else {
            return false;
        };
        let Some(key) = tree_key(&event.keystroke) else {
            return false;
        };
        match key {
            TreeKey::Move(delta) => self.move_tree_selection(panel, delta),
            TreeKey::Toggle => self.toggle_tree_row(panel, cx),
            TreeKey::Expand => self.expand_tree_row(panel, cx),
            TreeKey::CollapseOrParent => self.collapse_tree_row(panel),
            TreeKey::CopyPath => self.copy_tree_path(panel, cx),
        }
        cx.notify();
        true
    }

    pub(super) fn toggle_file_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(tree) = self.model.any_file_tree_in_active_tab() {
            self.close_pane(tree, cx);
            return;
        }
        let Some(focused) = self.model.tabs().active().map(|tab| tab.focused()) else {
            return;
        };
        let root = self.model.workspace_root().unwrap_or_else(|| self.panel_cwd(focused));
        self.open_file_tree(root, cx);
    }

    pub(super) fn open_file_tree(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        if let Some(existing) = self.model.file_tree_in_active_tab(&folder) {
            self.focus_pane(existing);
            return;
        }
        let panel = Panel::new_file_tree(PanelId::new(), document_title(&folder));
        self.open_document(panel, folder, cx);
    }

    fn focused_tree(&self) -> Option<PanelId> {
        let panel = self.model.focused_document()?;
        matches!(self.documents.get(&panel), Some(DocumentPane::FileTree(_))).then_some(panel)
    }

    fn tree_pane_mut(&mut self, panel: PanelId) -> Option<&mut FileTreePane> {
        match self.documents.get_mut(&panel) {
            Some(DocumentPane::FileTree(pane)) => Some(pane),
            _ => None,
        }
    }

    fn move_tree_selection(&mut self, panel: PanelId, delta: isize) {
        if let Some(pane) = self.tree_pane_mut(panel) {
            pane.tree.move_selection(delta);
            pane.reveal_selection(reveal_strategy(delta));
        }
    }

    fn collapse_tree_row(&mut self, panel: PanelId) {
        if let Some(pane) = self.tree_pane_mut(panel) {
            pane.tree.collapse_or_parent();
            pane.reveal_selection(ScrollStrategy::Top);
        }
    }

    fn toggle_tree_row(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(action) = self.tree_pane_mut(panel).map(|pane| pane.tree.toggle_selected()) else {
            return;
        };
        self.apply_tree_action(panel, action, cx);
    }

    fn expand_tree_row(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(action) = self.tree_pane_mut(panel).map(|pane| pane.tree.expand_selected()) else {
            return;
        };
        self.apply_tree_action(panel, action, cx);
    }

    fn apply_tree_action(&mut self, panel: PanelId, action: TreeAction, cx: &mut Context<Self>) {
        match action {
            TreeAction::None => {}
            TreeAction::Load(dir) => self.load_tree_dir(panel, dir, cx),
            TreeAction::Open(path) => self.open_from_tree(path, cx),
        }
    }

    fn copy_tree_path(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(path) = self.tree_pane_mut(panel).and_then(|pane| pane.tree.selected_row().map(|row| row.path.clone()))
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string_lossy().into_owned()));
    }

    fn open_from_tree(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_file(path, cx);
    }

    fn select_tree_row(&mut self, panel: PanelId, index: usize) {
        if let Some(pane) = self.tree_pane_mut(panel) {
            pane.tree.select_index(index);
        }
    }

    fn open_tree_row(&mut self, panel: PanelId, index: usize, cx: &mut Context<Self>) {
        self.select_tree_row(panel, index);
        self.toggle_tree_row(panel, cx);
        cx.notify();
    }

    pub(super) fn render_file_tree(
        &self,
        panel: PanelId,
        pane: &FileTreePane,
        is_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(format!("file-tree-{panel}"));
        let render_rows = cx.processor(move |shell: &mut Shell, range: Range<usize>, _window, cx| {
            shell.tree_rows(panel, range, is_focused, cx)
        });
        tree_list(id, pane.tree.rows().len(), &pane.scroll, render_rows).into_any_element()
    }

    fn tree_rows(&self, panel: PanelId, range: Range<usize>, is_focused: bool, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(DocumentPane::FileTree(pane)) = self.documents.get(&panel) else {
            return Vec::new();
        };
        let selected = pane.tree.selected_index();
        let theme = &self.state.theme;
        let rows = pane.tree.rows();
        range
            .filter_map(|index| {
                let row = rows.get(index)?;
                Some(self.clickable_row(tree_row(row, index == selected, is_focused, theme), panel, index, cx))
            })
            .collect()
    }

    fn clickable_row(&self, row: Div, panel: PanelId, index: usize, cx: &mut Context<Self>) -> Div {
        row.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event: &MouseDownEvent, _window, cx| {
                shell.select_tree_row(panel, index);
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |shell, event: &MouseUpEvent, _window, cx| {
                if event.click_count == DOUBLE_CLICK {
                    shell.open_tree_row(panel, index, cx);
                }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn plain(key: &str) -> Keystroke {
        Keystroke { modifiers: Modifiers::none(), key: key.to_string(), key_char: Some(key.to_string()) }
    }

    #[test]
    fn navigation_keys_map_to_tree_actions() {
        assert_eq!(tree_key(&plain("j")), Some(TreeKey::Move(1)));
        assert_eq!(tree_key(&plain("down")), Some(TreeKey::Move(1)));
        assert_eq!(tree_key(&plain("k")), Some(TreeKey::Move(-1)));
        assert_eq!(tree_key(&plain("up")), Some(TreeKey::Move(-1)));
        assert_eq!(tree_key(&plain("enter")), Some(TreeKey::Toggle));
        assert_eq!(tree_key(&plain("l")), Some(TreeKey::Toggle));
        assert_eq!(tree_key(&plain("right")), Some(TreeKey::Expand));
        assert_eq!(tree_key(&plain("h")), Some(TreeKey::CollapseOrParent));
        assert_eq!(tree_key(&plain("left")), Some(TreeKey::CollapseOrParent));
        assert_eq!(tree_key(&plain("y")), Some(TreeKey::CopyPath));
    }

    #[test]
    fn file_operation_and_modified_keys_are_left_unconsumed() {
        for key in ["a", "r", "d", "x"] {
            assert_eq!(tree_key(&plain(key)), None);
        }
        let shifted_a =
            Keystroke { modifiers: Modifiers::shift(), key: "a".to_string(), key_char: Some("A".to_string()) };
        assert_eq!(tree_key(&shifted_a), None);
        let cmd_j = Keystroke { modifiers: Modifiers::command(), key: "j".to_string(), key_char: None };
        assert_eq!(tree_key(&cmd_j), None);
    }
}
