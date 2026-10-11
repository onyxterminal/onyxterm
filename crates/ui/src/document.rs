use crate::editor_view::EditorView;
use crate::theme::Theme;
use domain::PanelKind;
use editor_core::io::LoadError;
use gpui::{div, px, AnyElement, IntoElement, ParentElement, SharedString, Styled};
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
}

impl DocumentPane {
    pub fn from_snapshot(record: &PanelSnapshot) -> Option<Self> {
        let path = record.path.clone()?;
        match record.panel.kind {
            PanelKind::Editor => Some(Self::Editor(EditorPane {
                path,
                state: EditorState::Loading,
                view: Rc::new(RefCell::new(EditorView::default())),
            })),
            PanelKind::FileTree => Some(Self::FileTree(FileTreePane { exists: path.is_dir(), root: path })),
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
    match &pane.state {
        EditorState::Loading => div().size_full().into_any_element(),
        EditorState::Failed(message) => centered_label(format!("{name} — {message}"), theme),
        EditorState::Ready(_) => div().size_full().into_any_element(),
    }
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

    #[test]
    fn load_errors_read_as_plain_words() {
        let missing = LoadError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(load_error_text(&missing), "not found");
        assert_eq!(load_error_text(&LoadError::Binary), "binary file");
        assert_eq!(load_error_text(&LoadError::NotUtf8), "not UTF-8 text");
    }
}
