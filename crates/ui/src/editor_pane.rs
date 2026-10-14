#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::editor_view::EditorView;
use domain::DocumentId;
use editor_core::{io::LoadError, Buffer, Editor};
use gpui::ScrollHandle;
use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

pub enum EditorState {
    Loading,
    Ready(Editor),
    Failed(String),
}

pub struct OpenDocument {
    pub id: DocumentId,
    pub path: PathBuf,
    pub state: EditorState,
    pub view: Rc<RefCell<EditorView>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Document(usize),
    Pane,
}

pub struct EditorPane {
    documents: Vec<OpenDocument>,
    active: usize,
    strip_scroll: ScrollHandle,
}

impl OpenDocument {
    pub fn loading(path: PathBuf) -> Self {
        Self {
            id: DocumentId::new(),
            path,
            state: EditorState::Loading,
            view: Rc::new(RefCell::new(EditorView::default())),
        }
    }

    pub fn ready(&self) -> Option<&Editor> {
        match &self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.ready().is_some_and(|editor| editor.buffer().is_dirty())
    }
}

impl EditorPane {
    pub fn single(path: PathBuf) -> Self {
        Self::with_documents(vec![OpenDocument::loading(path)], 0)
    }

    pub fn restored(paths: Vec<PathBuf>, active_path: Option<&Path>) -> Option<Self> {
        if paths.is_empty() {
            return None;
        }
        let active = active_path.and_then(|path| paths.iter().position(|open| open == path)).unwrap_or(0);
        let documents = paths.into_iter().map(OpenDocument::loading).collect();
        Some(Self::with_documents(documents, active))
    }

    fn with_documents(documents: Vec<OpenDocument>, active: usize) -> Self {
        let pane = Self { documents, active, strip_scroll: ScrollHandle::new() };
        pane.reveal_active();
        pane
    }

    pub fn strip_scroll(&self) -> &ScrollHandle {
        &self.strip_scroll
    }

    fn reveal_active(&self) {
        self.strip_scroll.scroll_to_item(self.active);
    }

    pub fn documents(&self) -> &[OpenDocument] {
        &self.documents
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &OpenDocument {
        &self.documents[self.active]
    }

    pub fn active_mut(&mut self) -> &mut OpenDocument {
        &mut self.documents[self.active]
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.documents.iter().map(|document| document.path.clone()).collect()
    }

    pub fn position_of(&self, path: &Path) -> Option<usize> {
        self.documents.iter().position(|document| document.path == path)
    }

    pub fn position_of_id(&self, id: DocumentId) -> Option<usize> {
        self.documents.iter().position(|document| document.id == id)
    }

    pub fn unsaved_count(&self) -> usize {
        self.documents.iter().filter(|document| document.is_dirty()).count()
    }

    pub fn is_dirty(&self) -> bool {
        self.unsaved_count() > 0
    }

    pub fn open(&mut self, path: PathBuf) {
        let index = self.active + 1;
        self.documents.insert(index, OpenDocument::loading(path));
        self.active = index;
        self.reveal_active();
    }

    pub fn activate(&mut self, index: usize) {
        if index < self.documents.len() {
            self.active = index;
            self.reveal_active();
        }
    }

    pub fn cycle(&mut self, step: isize) {
        let count = self.documents.len() as isize;
        self.active = (self.active as isize + step).rem_euclid(count) as usize;
        self.reveal_active();
    }

    pub fn closing(&self, id: DocumentId) -> Option<Closing> {
        let index = self.position_of_id(id)?;
        if self.documents.len() == 1 {
            return Some(Closing::Pane);
        }
        Some(Closing::Document(index))
    }

    pub fn remove(&mut self, index: usize) {
        if self.documents.len() <= 1 || index >= self.documents.len() {
            return;
        }
        self.documents.remove(index);
        if index < self.active || self.active == self.documents.len() {
            self.active -= 1;
        }
        self.reveal_active();
    }

    pub fn apply_load(&mut self, id: DocumentId, path: &Path, loaded: Result<Buffer, LoadError>) {
        let Some(document) = self.find_mut(id).filter(|document| document.path == path) else {
            return;
        };
        document.state = match loaded {
            Ok(buffer) => EditorState::Ready(Editor::new(buffer)),
            Err(err) => EditorState::Failed(crate::document::load_error_text(&err)),
        };
    }

    pub fn mark_saved(&mut self, id: DocumentId, version: u64) {
        if let Some(editor) = self.find_mut(id).and_then(OpenDocument::ready_mut) {
            editor.mark_saved_at(version);
        }
    }

    pub fn find(&self, id: DocumentId) -> Option<&OpenDocument> {
        self.documents.iter().find(|document| document.id == id)
    }

    fn find_mut(&mut self, id: DocumentId) -> Option<&mut OpenDocument> {
        self.documents.iter_mut().find(|document| document.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLabel {
    pub name: String,
    pub detail: Option<String>,
}

pub fn document_labels(paths: &[&Path]) -> Vec<DocumentLabel> {
    paths
        .iter()
        .map(|path| {
            let name = crate::document::document_title(path);
            let twins: Vec<&Path> =
                paths.iter().copied().filter(|other| *other != *path && file_name(other) == file_name(path)).collect();
            let detail = (!twins.is_empty()).then(|| distinguishing_parent(path, &twins));
            DocumentLabel { name, detail }
        })
        .collect()
}

fn file_name(path: &Path) -> Option<&std::ffi::OsStr> {
    path.file_name()
}

fn distinguishing_parent(path: &Path, twins: &[&Path]) -> String {
    let parents = parent_names(path);
    let mut depth = 1;
    while depth < parents.len() && twins.iter().any(|twin| parent_suffix(&parent_names(twin), depth) == parent_suffix(&parents, depth)) {
        depth += 1;
    }
    parent_suffix(&parents, depth).join("/")
}

fn parent_names(path: &Path) -> Vec<String> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    parent
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn parent_suffix(parents: &[String], depth: usize) -> Vec<String> {
    let start = parents.len().saturating_sub(depth);
    parents[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn pane(paths: &[&str]) -> EditorPane {
        EditorPane::restored(paths.iter().map(PathBuf::from).collect(), None).expect("documents")
    }

    fn active_path(pane: &EditorPane) -> &Path {
        &pane.active().path
    }

    fn make_ready(pane: &mut EditorPane, index: usize) -> DocumentId {
        let document = &pane.documents()[index];
        let (id, path) = (document.id, document.path.clone());
        pane.apply_load(id, &path, Ok(Buffer::from_text("x")));
        id
    }

    fn make_dirty(pane: &mut EditorPane, index: usize) {
        make_ready(pane, index);
        let editor = pane.documents[index].ready_mut().expect("ready");
        editor.paste("edit", Instant::now());
    }

    #[test]
    fn opening_adds_a_tab_after_the_active_one_and_activates_it() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs"]);
        pane.open(PathBuf::from("/r/c.rs"));

        assert_eq!(pane.paths(), vec![PathBuf::from("/r/a.rs"), PathBuf::from("/r/c.rs"), PathBuf::from("/r/b.rs")]);
        assert_eq!(active_path(&pane), Path::new("/r/c.rs"));
    }

    #[test]
    fn opening_beside_a_dirty_document_keeps_its_edits() {
        let mut pane = pane(&["/r/a.rs"]);
        make_dirty(&mut pane, 0);
        pane.open(PathBuf::from("/r/b.rs"));

        assert_eq!(pane.documents().len(), 2);
        assert!(pane.documents()[0].is_dirty());
        assert_eq!(pane.unsaved_count(), 1);
    }

    #[test]
    fn closing_the_active_document_activates_its_right_neighbor() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs", "/r/c.rs"]);
        pane.activate(1);
        let id = pane.active().id;
        let Some(Closing::Document(index)) = pane.closing(id) else {
            panic!("expected a document close");
        };
        pane.remove(index);

        assert_eq!(active_path(&pane), Path::new("/r/c.rs"));
    }

    #[test]
    fn closing_the_last_tab_in_the_row_activates_its_left_neighbor() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs"]);
        pane.activate(1);
        pane.remove(1);

        assert_eq!(active_path(&pane), Path::new("/r/a.rs"));
    }

    #[test]
    fn closing_a_document_before_the_active_one_keeps_the_active_document() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs", "/r/c.rs"]);
        pane.activate(2);
        pane.remove(0);

        assert_eq!(active_path(&pane), Path::new("/r/c.rs"));
    }

    #[test]
    fn closing_the_only_document_closes_the_pane() {
        let pane = pane(&["/r/a.rs"]);

        assert_eq!(pane.closing(pane.active().id), Some(Closing::Pane));
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs", "/r/c.rs"]);
        pane.cycle(-1);
        assert_eq!(active_path(&pane), Path::new("/r/c.rs"));
        pane.cycle(1);
        assert_eq!(active_path(&pane), Path::new("/r/a.rs"));
    }

    #[test]
    fn restoring_activates_the_saved_active_path() {
        let paths = vec![PathBuf::from("/r/a.rs"), PathBuf::from("/r/b.rs")];
        let pane = EditorPane::restored(paths, Some(Path::new("/r/b.rs"))).expect("documents");

        assert_eq!(pane.active_index(), 1);
    }

    #[test]
    fn a_load_for_a_closed_document_is_ignored() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs"]);
        let closed = pane.documents()[1].id;
        pane.remove(1);
        pane.open(PathBuf::from("/r/b.rs"));
        pane.apply_load(closed, Path::new("/r/b.rs"), Ok(Buffer::from_text("stale")));

        assert!(matches!(pane.active().state, EditorState::Loading));
    }

    #[test]
    fn a_load_for_a_different_path_is_ignored() {
        let mut pane = pane(&["/r/a.rs"]);
        let id = pane.active().id;
        pane.apply_load(id, Path::new("/r/other.rs"), Ok(Buffer::from_text("stale")));

        assert!(matches!(pane.active().state, EditorState::Loading));
    }

    #[test]
    fn a_save_marks_the_saved_document_even_after_switching_tabs() {
        let mut pane = pane(&["/r/a.rs", "/r/b.rs"]);
        make_dirty(&mut pane, 0);
        let id = pane.documents()[0].id;
        let version = pane.documents()[0].ready().expect("ready").buffer().version();
        pane.activate(1);
        pane.mark_saved(id, version);

        assert!(!pane.documents()[0].is_dirty());
    }

    #[test]
    fn unique_names_have_no_detail() {
        let labels = document_labels(&[Path::new("/r/a.rs"), Path::new("/r/b.rs")]);

        assert!(labels.iter().all(|label| label.detail.is_none()));
    }

    #[test]
    fn duplicate_names_show_their_parent_folder() {
        let labels = document_labels(&[Path::new("/r/core/mod.rs"), Path::new("/r/ui/mod.rs"), Path::new("/r/a.rs")]);

        assert_eq!(labels[0].detail.as_deref(), Some("core"));
        assert_eq!(labels[1].detail.as_deref(), Some("ui"));
        assert_eq!(labels[2].detail, None);
    }

    #[test]
    fn duplicate_parents_widen_until_they_differ() {
        let labels = document_labels(&[Path::new("/a/src/lib.rs"), Path::new("/b/src/lib.rs")]);

        assert_eq!(labels[0].detail.as_deref(), Some("a/src"));
        assert_eq!(labels[1].detail.as_deref(), Some("b/src"));
    }
}
