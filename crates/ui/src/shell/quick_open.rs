use super::{FieldTarget, Shell, DOWN_KEY, ENTER_KEY, ESCAPE_KEY, UP_KEY};
use crate::command::close_palette;
use crate::git_status::{read_git_status, GitStatus};
use crate::palette::{build_overlay, file_content, PaletteView};
use crate::quick_open::{rank_paths, QuickOpen, LISTED_FILE_LIMIT, RESULT_LIMIT};
use gpui::{prelude::*, Context, Div, Keystroke, MouseButton, ScrollStrategy, Task, Window};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct QuickOpenJobs {
    listing: Option<Task<()>>,
    ranking: Option<Task<()>>,
}

impl Shell {
    pub(super) fn toggle_quick_open(&mut self, cx: &mut Context<Self>) {
        if self.quick_open.is_some() {
            self.close_quick_open(cx);
            return;
        }
        close_palette(&mut self.state);
        let root = self.quick_open_root();
        self.quick_open = Some(QuickOpen::new(root.clone()));
        self.scroll_palette_to_top();
        let listing = cx.background_executor().spawn(async move {
            let files = Arc::new(fs_service::list_files(&root, LISTED_FILE_LIMIT));
            (files, read_git_status(&root).map(Arc::new))
        });
        self.quick_open_jobs.listing = Some(cx.spawn(async move |shell, cx| {
            let (files, git) = listing.await;
            let updated = shell.update(cx, |shell, cx| shell.finish_quick_open_listing(files, git, cx));
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before quick open listing finished");
            }
        }));
    }

    pub(super) fn close_quick_open(&mut self, cx: &mut Context<Self>) {
        self.quick_open = None;
        self.quick_open_jobs = QuickOpenJobs::default();
        cx.notify();
    }

    pub(super) fn handle_quick_open_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        match keystroke.key.as_str() {
            ESCAPE_KEY => self.close_quick_open(cx),
            ENTER_KEY => self.open_quick_open_selection(cx),
            UP_KEY => self.move_quick_open_selection(-1, ScrollStrategy::Top),
            DOWN_KEY => self.move_quick_open_selection(1, ScrollStrategy::Bottom),
            _ => return self.edit_field(FieldTarget::QuickOpenQuery, keystroke, cx),
        }
        cx.notify();
        true
    }

    pub(super) fn rank_quick_open(&mut self, cx: &mut Context<Self>) {
        let Some(quick_open) = self.quick_open.as_ref() else {
            return;
        };
        let files = quick_open.files.clone();
        let query = quick_open.query.text().to_string();
        let ranking = cx.background_executor().spawn(async move { Arc::new(rank_paths(&files, &query, RESULT_LIMIT)) });
        self.quick_open_jobs.ranking = Some(cx.spawn(async move |shell, cx| {
            let results = ranking.await;
            let updated = shell.update(cx, |shell, cx| {
                if let Some(quick_open) = shell.quick_open.as_mut() {
                    quick_open.accept_results(results);
                }
                shell.scroll_palette_to_top();
                cx.notify();
            });
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before quick open ranking finished");
            }
        }));
    }

    pub(super) fn render_quick_open(&self, cx: &mut Context<Self>) -> Option<Div> {
        let quick_open = self.quick_open.as_ref()?;
        let content = file_content(&quick_open.query, quick_open.rows(), quick_open.selection);
        let overlay = build_overlay(content, &self.state.theme, self.quick_open_view(cx));
        let dismiss = cx.listener(|shell, _event, _window, cx| shell.close_quick_open(cx));
        Some(overlay.on_mouse_down(MouseButton::Left, dismiss))
    }

    fn finish_quick_open_listing(
        &mut self,
        files: Arc<Vec<String>>,
        git: Option<Arc<GitStatus>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(quick_open) = self.quick_open.as_mut() {
            quick_open.files = files;
            quick_open.git = git;
        }
        self.rank_quick_open(cx);
    }

    fn quick_open_root(&self) -> PathBuf {
        let bundle_root = self.find_bundle().and_then(|(tree, _)| self.tree_pane(tree)).map(|tree| tree.root.clone());
        let focused = self.model.tabs().active().map(|tab| tab.focused());
        bundle_root
            .or_else(|| self.model.workspace_root())
            .unwrap_or_else(|| focused.map_or_else(|| self.model.active_root(), |panel| self.panel_cwd(panel)))
    }

    fn move_quick_open_selection(&mut self, step: isize, strategy: ScrollStrategy) {
        if let Some(quick_open) = self.quick_open.as_mut() {
            quick_open.move_selection(step);
            self.palette_scroll.scroll_to_item(quick_open.selection, strategy);
        }
    }

    fn open_quick_open_selection(&mut self, cx: &mut Context<Self>) {
        let Some(quick_open) = self.quick_open.take() else {
            return;
        };
        self.quick_open_jobs = QuickOpenJobs::default();
        let Some(path) = quick_open.selected_path() else {
            return;
        };
        self.show_in_bundle(quick_open.root, path, cx);
        self.remember_focused_editor();
        self.persist();
    }

    fn quick_open_view(&self, cx: &mut Context<Self>) -> PaletteView {
        let select_target = cx.entity().downgrade();
        let hover_target = cx.entity().downgrade();
        PaletteView {
            scroll: self.palette_scroll.clone(),
            on_select: Rc::new(move |index, window: &mut Window, cx: &mut gpui::App| {
                let Some(shell) = select_target.upgrade() else {
                    return;
                };
                shell.update(cx, |shell, cx| {
                    if let Some(quick_open) = shell.quick_open.as_mut() {
                        quick_open.selection = index;
                    }
                    shell.open_quick_open_selection(cx);
                    window.focus(&shell.focus_handle);
                    cx.notify();
                });
            }),
            on_hover: Rc::new(move |index, _window: &mut Window, cx: &mut gpui::App| {
                let Some(shell) = hover_target.upgrade() else {
                    return;
                };
                shell.update(cx, |shell, cx| {
                    let Some(quick_open) = shell.quick_open.as_mut().filter(|open| open.selection != index) else {
                        return;
                    };
                    quick_open.selection = index;
                    cx.notify();
                });
            }),
        }
    }
}
