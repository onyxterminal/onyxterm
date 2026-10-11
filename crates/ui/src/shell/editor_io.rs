use super::Shell;
use crate::document::{load_error_text, DocumentPane, EditorState};
use domain::PanelId;
use editor_core::{io, Buffer, Editor};
use gpui::Context;
use std::path::PathBuf;

impl Shell {
    pub(super) fn load_editor(&mut self, panel: PanelId, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async move { io::load(&path) });
        cx.spawn(async move |shell, cx| {
            let loaded = task.await;
            let updated = shell.update(cx, |shell, cx| {
                shell.finish_load(panel, loaded);
                cx.notify();
            });
            if let Err(err) = updated {
                tracing::debug!(?err, "shell dropped before load finished");
            }
        })
        .detach();
    }

    fn finish_load(&mut self, panel: PanelId, loaded: Result<Buffer, io::LoadError>) {
        let Some(DocumentPane::Editor(pane)) = self.documents.get_mut(&panel) else {
            return;
        };
        pane.state = match loaded {
            Ok(buffer) => EditorState::Ready(Editor::new(buffer)),
            Err(err) => EditorState::Failed(load_error_text(&err)),
        };
    }
}
