use super::Shell;
use crate::document::{DocumentPane, FileTreePane};
use crate::file_tree_view::{tree_list, tree_row};
use domain::PanelId;
use gpui::{AnyElement, Context, Div, IntoElement, SharedString};
use std::ops::Range;
use std::path::PathBuf;

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

    pub(super) fn render_file_tree(
        &self,
        panel: PanelId,
        pane: &FileTreePane,
        is_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(format!("file-tree-{panel}"));
        let render_rows = cx.processor(move |shell: &mut Shell, range: Range<usize>, _window, _cx| {
            shell.tree_rows(panel, range, is_focused)
        });
        tree_list(id, pane.tree.rows().len(), &pane.scroll, render_rows).into_any_element()
    }

    fn tree_rows(&self, panel: PanelId, range: Range<usize>, is_focused: bool) -> Vec<Div> {
        let Some(DocumentPane::FileTree(pane)) = self.documents.get(&panel) else {
            return Vec::new();
        };
        let selected = pane.tree.selected_index();
        let theme = &self.state.theme;
        let rows = pane.tree.rows();
        range
            .filter_map(|index| rows.get(index).map(|row| tree_row(row, index == selected, is_focused, theme)))
            .collect()
    }
}
