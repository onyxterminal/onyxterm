use super::editor_io::DocumentRef;
use super::Shell;
use crate::command::CommandId;
use crate::editor_chrome::{breadcrumb_text, caret_status_text, CaretStatus};
use crate::editor_pane::{document_labels, DocumentLabel, EditorPane, OpenDocument};
use crate::theme::icons::Icon;
use crate::theme::{tabular_ui_font, Theme};
use domain::PanelId;
use editor_core::detect_indentation;
use editor_core::syntax::Language;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, FontWeight, MouseButton, MouseDownEvent, SharedString, Stateful,
};
use std::path::Path;

const STRIP_HEIGHT_PX: f32 = 34.0;
const STRIP_PADDING_X_PX: f32 = 6.0;
const STRIP_GAP_PX: f32 = 8.0;
const TABS_GAP_PX: f32 = 2.0;
const TAB_HEIGHT_PX: f32 = 24.0;
const TAB_MIN_WIDTH_PX: f32 = 60.0;
const TAB_PADDING_LEFT_PX: f32 = 10.0;
const TAB_PADDING_RIGHT_PX: f32 = 26.0;
const TAB_RADIUS_PX: f32 = 6.0;
const TAB_TEXT_PX: f32 = 12.0;
const TAB_BUTTON_PX: f32 = 18.0;
const TAB_BUTTON_RIGHT_PX: f32 = 4.0;
const TAB_BUTTON_RADIUS_PX: f32 = 4.0;
const TAB_CLOSE_ICON_PX: f32 = 8.0;
const DETAIL_GAP_PX: f32 = 5.0;
const DIRTY_DOT_PX: f32 = 7.0;
const DIRTY_DOT_OPACITY: f32 = 0.7;
const CRUMBS_HEIGHT_PX: f32 = 22.0;
const CRUMBS_PADDING_X_PX: f32 = 16.0;
const CRUMBS_BORDER_PX: f32 = 1.0;

pub(super) fn editor_chrome_height(scale: f32) -> f32 {
    (STRIP_HEIGHT_PX + CRUMBS_HEIGHT_PX + CRUMBS_BORDER_PX) * scale
}

struct DocumentTab {
    document: DocumentRef,
    index: usize,
    label: DocumentLabel,
    path: SharedString,
    is_active: bool,
    is_dirty: bool,
}

impl Shell {
    pub(super) fn render_document_strip(&self, panel: PanelId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pane = self.editor_pane(panel)?;
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let tabs = div()
            .id(SharedString::from(format!("document-tabs-{panel}")))
            .flex_shrink()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(TABS_GAP_PX * scale))
            .overflow_x_scroll()
            .track_scroll(pane.strip_scroll())
            .children(document_tabs(panel, pane).into_iter().map(|tab| self.render_document_tab(tab, cx)));
        let strip = div()
            .h(px(STRIP_HEIGHT_PX * scale))
            .flex_none()
            .px(px(STRIP_PADDING_X_PX * scale))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(STRIP_GAP_PX * scale))
            .child(tabs)
            .child(div().flex_1())
            .children(caret_status(pane.active()).map(|status| self.caret_status_label(status)))
            .child(self.close_editor_button(panel, cx));
        let chrome = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .child(strip)
            .child(self.breadcrumbs(pane.active(), theme));
        Some(chrome.into_any_element())
    }

    fn caret_status_label(&self, status: String) -> Div {
        let theme = &self.state.theme;
        div()
            .flex_none()
            .whitespace_nowrap()
            .font(tabular_ui_font(theme))
            .text_size(px(theme.ui_font.hint_px))
            .text_color(theme.text.subtle)
            .child(status)
    }

    fn breadcrumbs(&self, document: &OpenDocument, theme: &Theme) -> Div {
        let scale = self.state.settings.ui_scale;
        div()
            .h(px((CRUMBS_HEIGHT_PX + CRUMBS_BORDER_PX) * scale))
            .flex_none()
            .px(px(CRUMBS_PADDING_X_PX * scale))
            .flex()
            .items_center()
            .overflow_hidden()
            .border_b(px(CRUMBS_BORDER_PX))
            .border_color(theme.border.muted)
            .text_size(px(theme.ui_font.hint_px))
            .text_color(theme.text.subtle)
            .child(div().min_w(px(0.0)).truncate().child(breadcrumb_text(&document.path, &self.model.active_root())))
    }

    fn close_editor_button(&self, panel: PanelId, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let icon = Icon::Close.sized(theme.pane_header.close_icon_px, theme.text.subtle);
        let is_bundled = self.find_bundle().is_some_and(|(_, editor)| editor == panel);
        let command = if is_bundled { CommandId::ToggleEditorBundle } else { CommandId::ClosePane };
        self.header_button(format!("editor-close-{panel}"), icon).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, _event, window, cx| {
                cx.stop_propagation();
                shell.focus_pane(panel);
                window.focus(&shell.focus_handle);
                shell.run_command(command, window, cx);
            }),
        )
    }

    fn render_document_tab(&self, tab: DocumentTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let DocumentRef { panel, .. } = tab.document;
        let index = tab.index;
        let text_color = if tab.is_active { theme.text.default } else { theme.text.muted };
        let label = div()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .gap(px(DETAIL_GAP_PX * scale))
            .overflow_hidden()
            .child(div().min_w(px(0.0)).truncate().child(tab.label.name))
            .children(tab.label.detail.map(|detail| div().flex_none().text_color(theme.text.subtle).child(detail)));
        div()
            .id(SharedString::from(format!("document-tab-{}", tab.document.id.as_uuid())))
            .relative()
            .flex_shrink()
            .min_w(px(TAB_MIN_WIDTH_PX * scale))
            .h(px(TAB_HEIGHT_PX * scale))
            .pl(px(TAB_PADDING_LEFT_PX * scale))
            .pr(px(TAB_PADDING_RIGHT_PX * scale))
            .flex()
            .items_center()
            .rounded(px(TAB_RADIUS_PX * scale))
            .cursor_pointer()
            .text_size(px(TAB_TEXT_PX * scale))
            .text_color(text_color)
            .font_weight(if tab.is_active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
            .when(tab.is_active, |el| el.bg(theme.chrome.control_fill))
            .tooltip(self.tooltip_builder(tab.path.clone()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    shell.activate_document(panel, index);
                    window.focus(&shell.focus_handle);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |shell, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.request_close_document(tab.document, cx);
                    cx.notify();
                }),
            )
            .child(label)
            .child(self.document_tab_button(tab.document, tab.is_dirty, cx))
            .into_any_element()
    }

    fn document_tab_button(&self, document: DocumentRef, is_dirty: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let mark = match is_dirty {
            true => div()
                .size(px(DIRTY_DOT_PX * scale))
                .rounded_full()
                .bg(theme.text.default.opacity(DIRTY_DOT_OPACITY))
                .into_any_element(),
            false => Icon::CloseBold.sized(TAB_CLOSE_ICON_PX * scale, theme.text.subtle).into_any_element(),
        };
        div()
            .id(SharedString::from(format!("document-close-{}", document.id.as_uuid())))
            .absolute()
            .right(px(TAB_BUTTON_RIGHT_PX * scale))
            .size(px(TAB_BUTTON_PX * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(TAB_BUTTON_RADIUS_PX * scale))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.request_close_document(document, cx);
                    cx.notify();
                }),
            )
            .child(mark)
    }
}

fn caret_status(document: &OpenDocument) -> Option<String> {
    let editor = document.ready()?;
    let rope = editor.buffer().rope();
    Some(caret_status_text(CaretStatus {
        rope,
        head: editor.selections().newest().head,
        language: Language::from_path(&document.path),
        indentation: detect_indentation(rope),
    }))
}

fn document_tabs(panel: PanelId, pane: &EditorPane) -> Vec<DocumentTab> {
    let paths: Vec<&Path> = pane.documents().iter().map(|document| document.path.as_path()).collect();
    let labels = document_labels(&paths);
    pane.documents()
        .iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (document, label))| DocumentTab {
            document: DocumentRef { panel, id: document.id },
            index,
            label,
            path: SharedString::from(document.path.display().to_string()),
            is_active: index == pane.active_index(),
            is_dirty: document.is_dirty(),
        })
        .collect()
}
