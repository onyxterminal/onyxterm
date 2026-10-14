use super::editor_io::DocumentRef;
use super::Shell;
use crate::confirm::Confirmation;
use crate::document::{document_cwd, document_title, DocumentPane};
use crate::editor_pane::Closing;
use crate::open_target::{choose_editor_target, OpenEditor, OpenTarget, TabEditors};
use domain::{DocumentId, Panel, PanelId, PanelKind};
use gpui::Context;
use std::path::PathBuf;

impl Shell {
    pub(super) fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match choose_editor_target(&self.tab_editors(), &path) {
            OpenTarget::Activate { panel, index } => self.activate_document(panel, index),
            OpenTarget::AddTo(panel) => self.add_document(panel, path, cx),
            OpenTarget::NewPane => {
                let panel = Panel::new_editor(PanelId::new(), document_title(&path));
                self.open_document(panel, path, cx);
            }
        }
    }

    fn tab_editors(&self) -> TabEditors {
        let tab = self.model.tabs().active();
        let leaves = tab.map(|tab| tab.layout().leaves()).unwrap_or_default();
        let editors = leaves
            .into_iter()
            .filter_map(|panel| self.editor_pane(panel).map(|pane| OpenEditor { panel, paths: pane.paths() }))
            .collect();
        TabEditors { focused: tab.map(|tab| tab.focused()), editors, recent: self.recent_editors.order() }
    }

    pub(super) fn activate_document(&mut self, panel: PanelId, index: usize) {
        if let Some(pane) = self.editor_pane_mut(panel) {
            pane.activate(index);
        }
        self.sync_editor_record(panel);
        self.focus_pane(panel);
    }

    fn add_document(&mut self, panel: PanelId, path: PathBuf, cx: &mut Context<Self>) {
        let Some(pane) = self.editor_pane_mut(panel) else {
            return;
        };
        pane.open(path.clone());
        let id = pane.active().id;
        self.load_document(DocumentRef { panel, id }, path, cx);
        self.sync_editor_record(panel);
        self.focus_pane(panel);
    }

    pub(super) fn load_editor_documents(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        let Some(pane) = self.editor_pane(panel) else {
            return;
        };
        let pending: Vec<(DocumentId, PathBuf)> =
            pane.documents().iter().map(|document| (document.id, document.path.clone())).collect();
        for (id, path) in pending {
            self.load_document(DocumentRef { panel, id }, path, cx);
        }
    }

    pub(super) fn sync_editor_record(&mut self, panel: PanelId) {
        let Some(pane) = self.editor_pane(panel) else {
            return;
        };
        let path = pane.active().path.clone();
        let documents = pane.paths();
        let fallback = self.model.active_root();
        let Some(record) = self.model.panel_mut(panel) else {
            return;
        };
        record.cwd = document_cwd(PanelKind::Editor, &path, fallback);
        record.panel.title = document_title(&path);
        record.path = Some(path);
        record.documents = documents;
    }

    pub(super) fn cycle_documents(&mut self, step: isize) -> bool {
        let Some(panel) = self.focused_editor_pane().filter(|(_, pane)| pane.documents().len() > 1).map(|(panel, _)| panel)
        else {
            return false;
        };
        if let Some(pane) = self.editor_pane_mut(panel) {
            pane.cycle(step);
        }
        self.sync_editor_record(panel);
        true
    }

    pub(super) fn request_close_focused_document(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((panel, pane)) = self.focused_editor_pane() else {
            return false;
        };
        let id = pane.active().id;
        self.request_close_document(DocumentRef { panel, id }, cx);
        true
    }

    pub(super) fn request_close_document(&mut self, document: DocumentRef, cx: &mut Context<Self>) {
        let is_dirty = self
            .editor_pane(document.panel)
            .and_then(|pane| pane.find(document.id))
            .is_some_and(|open| open.is_dirty());
        if is_dirty {
            self.pending = Some(Confirmation::DiscardDocument(document.panel, document.id));
            return;
        }
        self.close_document(document, cx);
    }

    pub(super) fn close_document(&mut self, document: DocumentRef, cx: &mut Context<Self>) {
        let closing = self.editor_pane(document.panel).and_then(|pane| pane.closing(document.id));
        match closing {
            Some(Closing::Pane) => self.close_pane(document.panel, cx),
            Some(Closing::Document(index)) => {
                if let Some(pane) = self.editor_pane_mut(document.panel) {
                    pane.remove(index);
                }
                self.sync_editor_record(document.panel);
                self.persist();
            }
            None => {}
        }
    }

    pub(super) fn request_close_pane(&mut self, panel: PanelId, cx: &mut Context<Self>) {
        if self.documents.get(&panel).is_some_and(DocumentPane::is_dirty) {
            self.pending = Some(Confirmation::DiscardPane(panel));
            return;
        }
        let is_busy = self.panes.get(&panel).is_some_and(|pane| pane.terminal.handle.has_foreground_job());
        if is_busy {
            self.pending = Some(Confirmation::ClosePane(panel));
            return;
        }
        self.close_pane(panel, cx);
    }

    pub(super) fn document_name(&self, document: DocumentRef) -> Option<String> {
        let open = self.editor_pane(document.panel)?.find(document.id)?;
        Some(document_title(&open.path))
    }

    pub(super) fn dirty_document_name(&self, panel: PanelId) -> Option<String> {
        let pane = self.editor_pane(panel)?;
        let dirty = pane.documents().iter().find(|document| document.is_dirty())?;
        Some(document_title(&dirty.path))
    }
}
