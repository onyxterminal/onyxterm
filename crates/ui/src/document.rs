use crate::editor_view::EditorView;
use crate::file_tree::FileTree;
use crate::theme::Theme;
use domain::{DropZone, PanelKind};
use editor_core::io::LoadError;
use fs_service::Entry;
use gpui::{
    div, px, AnyElement, IntoElement, ParentElement, ScrollStrategy, SharedString, Styled, UniformListScrollHandle,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use workspace_service::PanelSnapshot;

const NOT_FOUND: &str = "not found";
const LABEL_GAP_PX: f32 = 6.0;

pub enum DocumentPane {
    Editor(EditorPane),
    FileTree(FileTreePane),
}

pub struct EditorPane {
    pub path: PathBuf,
    pub state: EditorState,
    pub view: Rc<RefCell<EditorView>>,
}

pub enum EditorState {
    Loading,
    Ready(editor_core::Editor),
    Failed(String),
}

pub struct FileTreePane {
    pub root: PathBuf,
    pub exists: bool,
    pub tree: FileTree,
    pub scroll: UniformListScrollHandle,
}

impl DocumentPane {
    pub fn from_snapshot(record: &PanelSnapshot) -> Option<Self> {
        let path = record.path.clone()?;
        match record.panel.kind {
            PanelKind::Editor => Some(Self::Editor(EditorPane::loading(path))),
            PanelKind::FileTree => Some(Self::FileTree(FileTreePane::new(path))),
            PanelKind::Terminal | PanelKind::Agent => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Editor(pane) => &pane.path,
            Self::FileTree(pane) => &pane.root,
        }
    }

    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Editor(pane) if pane.ready().is_some_and(|editor| editor.buffer().is_dirty()))
    }
}

impl EditorPane {
    pub fn loading(path: PathBuf) -> Self {
        Self { path, state: EditorState::Loading, view: Rc::new(RefCell::new(EditorView::default())) }
    }

    pub fn ready(&self) -> Option<&editor_core::Editor> {
        match &self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut editor_core::Editor> {
        match &mut self.state {
            EditorState::Ready(editor) => Some(editor),
            EditorState::Loading | EditorState::Failed(_) => None,
        }
    }
}

impl FileTreePane {
    fn new(root: PathBuf) -> Self {
        Self { tree: FileTree::new(root.clone()), root, exists: true, scroll: UniformListScrollHandle::new() }
    }

    pub fn reveal_selection(&self, strategy: ScrollStrategy) {
        self.scroll.scroll_to_item(self.tree.selected_index(), strategy);
    }

    pub fn apply_listing(&mut self, dir: &Path, listed: std::io::Result<Vec<Entry>>) {
        match listed {
            Ok(entries) => self.apply_entries(dir, entries),
            Err(err) if dir == self.root => {
                tracing::warn!(%err, root = %dir.display(), "file tree root listing failed");
                self.exists = false;
            }
            Err(err) => tracing::warn!(%err, dir = %dir.display(), "file tree listing failed"),
        }
    }

    fn apply_entries(&mut self, dir: &Path, entries: Vec<Entry>) {
        if dir == self.root {
            self.exists = true;
        }
        self.tree.set_children(dir, entries);
    }
}

pub fn load_error_text(error: &LoadError) -> String {
    match error {
        LoadError::Io(err) if err.kind() == std::io::ErrorKind::NotFound => NOT_FOUND.to_string(),
        other => other.to_string(),
    }
}

pub fn document_cwd(kind: PanelKind, path: &Path, fallback: PathBuf) -> PathBuf {
    match kind {
        PanelKind::FileTree => path.to_path_buf(),
        _ => path.parent().map(Path::to_path_buf).unwrap_or(fallback),
    }
}

pub fn document_zone(kind: PanelKind) -> DropZone {
    match kind {
        PanelKind::FileTree => DropZone::Left,
        _ => DropZone::Right,
    }
}

pub fn explicit_title(record: &PanelSnapshot) -> Option<&str> {
    let document_name = record.panel.is_document().then_some(record.panel.title.as_str());
    record.name.as_deref().or(document_name)
}

pub fn document_title(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

pub fn render_document(pane: &DocumentPane, theme: &Theme) -> AnyElement {
    match pane {
        DocumentPane::Editor(editor_pane) => render_editor_pane(editor_pane, theme),
        DocumentPane::FileTree(file_tree_pane) => render_file_tree(file_tree_pane, theme),
    }
}

fn render_editor_pane(pane: &EditorPane, theme: &Theme) -> AnyElement {
    let name = document_title(&pane.path);
    if let EditorState::Failed(message) = &pane.state {
        return centered_label(format!("{name} — {message}"), theme);
    }
    div().size_full().into_any_element()
}

fn render_file_tree(pane: &FileTreePane, theme: &Theme) -> AnyElement {
    let name = document_title(&pane.root);
    let label: SharedString = if pane.exists { name.into() } else { format!("{name} {NOT_FOUND}").into() };
    centered_label(label, theme)
}

fn centered_label(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    let label = label.into();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(LABEL_GAP_PX))
        .text_color(theme.text.muted)
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane_title::{display_title, TitleSource};
    use domain::{Panel, PanelId};
    use editor_core::{io::LoadError, Buffer, Editor};

    fn editor_record() -> PanelSnapshot {
        editor_record_named(None)
    }

    fn editor_record_named(name: Option<&str>) -> PanelSnapshot {
        PanelSnapshot {
            panel: Panel::new_editor(PanelId::new(), "main.rs".to_string()),
            cwd: PathBuf::from("/home/me/proj"),
            name: name.map(str::to_string),
            path: Some(PathBuf::from("/home/me/proj/main.rs")),
        }
    }

    fn shown(record: &PanelSnapshot) -> String {
        let home = Path::new("/home/me");
        display_title(&TitleSource { name: explicit_title(record), job: None, osc: None, cwd: &record.cwd, home })
    }

    #[test]
    fn document_title_is_the_panel_title_not_the_cwd() {
        assert_eq!(shown(&editor_record_named(None)), "main.rs");
    }

    #[test]
    fn renamed_document_shows_its_name() {
        assert_eq!(shown(&editor_record_named(Some("scratch"))), "scratch");
    }

    #[test]
    fn file_tree_cwd_is_the_folder_itself() {
        let cwd = document_cwd(PanelKind::FileTree, Path::new("/home/me/proj"), PathBuf::from("/fallback"));
        assert_eq!(cwd, PathBuf::from("/home/me/proj"));
    }

    #[test]
    fn file_trees_open_on_the_left_and_editors_on_the_right() {
        assert_eq!(document_zone(PanelKind::FileTree), DropZone::Left);
        assert_eq!(document_zone(PanelKind::Editor), DropZone::Right);
    }

    #[test]
    fn editor_cwd_is_the_parent_folder() {
        let cwd = document_cwd(PanelKind::Editor, Path::new("/home/me/proj/main.rs"), PathBuf::from("/fallback"));
        assert_eq!(cwd, PathBuf::from("/home/me/proj"));
    }

    #[test]
    fn editor_pane_starts_loading_and_finishes_ready() {
        let record = editor_record();
        let Some(DocumentPane::Editor(mut pane)) = DocumentPane::from_snapshot(&record) else {
            panic!("expected an editor pane");
        };
        assert!(matches!(pane.state, EditorState::Loading));
        pane.state = EditorState::Ready(Editor::new(Buffer::from_text("x")));
        assert!(pane.ready().is_some());
        assert!(!DocumentPane::Editor(pane).is_dirty());
    }

    fn tree_pane() -> FileTreePane {
        let record = PanelSnapshot {
            panel: Panel::new_file_tree(PanelId::new(), "proj".to_string()),
            cwd: PathBuf::from("/home/me/proj"),
            name: None,
            path: Some(PathBuf::from("/home/me/proj")),
        };
        let Some(DocumentPane::FileTree(pane)) = DocumentPane::from_snapshot(&record) else {
            panic!("expected a file tree pane");
        };
        pane
    }

    fn listing_error() -> std::io::Result<Vec<fs_service::Entry>> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    #[test]
    fn root_listing_fills_the_tree() {
        let mut pane = tree_pane();
        let entry = fs_service::Entry {
            path: PathBuf::from("/home/me/proj/a.txt"),
            name: "a.txt".to_string(),
            is_dir: false,
            is_ignored: false,
        };
        pane.apply_listing(&PathBuf::from("/home/me/proj"), Ok(vec![entry]));
        assert!(pane.exists);
        assert_eq!(pane.tree.rows().len(), 1);
    }

    #[test]
    fn failed_root_listing_marks_the_tree_missing() {
        let mut pane = tree_pane();
        pane.apply_listing(&PathBuf::from("/home/me/proj"), listing_error());
        assert!(!pane.exists);
    }

    #[test]
    fn successful_root_listing_recovers_a_missing_tree() {
        let mut pane = tree_pane();
        pane.apply_listing(&PathBuf::from("/home/me/proj"), listing_error());
        pane.apply_listing(&PathBuf::from("/home/me/proj"), Ok(Vec::new()));
        assert!(pane.exists);
    }

    #[test]
    fn failed_subdirectory_listing_leaves_the_tree_present_and_empty() {
        let mut pane = tree_pane();
        pane.apply_listing(&PathBuf::from("/home/me/proj/src"), listing_error());
        assert!(pane.exists);
        assert!(pane.tree.rows().is_empty());
    }

    #[test]
    fn load_errors_read_as_plain_words() {
        let missing = LoadError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(load_error_text(&missing), "not found");
        assert_eq!(load_error_text(&LoadError::Binary), "binary file");
        assert_eq!(load_error_text(&LoadError::NotUtf8), "not UTF-8 text");
    }
}
