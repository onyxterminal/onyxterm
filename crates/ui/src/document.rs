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
