use crate::theme::Theme;
use domain::PanelKind;
use gpui::{div, px, AnyElement, IntoElement, ParentElement, SharedString, Styled};
use std::path::{Path, PathBuf};
use workspace_service::PanelSnapshot;

const NOT_FOUND_SUFFIX: &str = " not found";
const LABEL_GAP_PX: f32 = 6.0;

pub struct DocumentPane {
    pub kind: PanelKind,
    pub path: PathBuf,
}

impl DocumentPane {
    pub fn from_snapshot(record: &PanelSnapshot) -> Option<Self> {
        let path = record.path.clone()?;
        record.panel.is_document().then_some(Self { kind: record.panel.kind, path })
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
    let name = document_title(&pane.path);
    let label: SharedString =
        if pane.path.exists() { name.into() } else { format!("{name}{NOT_FOUND_SUFFIX}").into() };
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

    fn editor_record(name: Option<&str>) -> PanelSnapshot {
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
        assert_eq!(shown(&editor_record(None)), "main.rs");
    }

    #[test]
    fn renamed_document_shows_its_name() {
        assert_eq!(shown(&editor_record(Some("scratch"))), "scratch");
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
}
