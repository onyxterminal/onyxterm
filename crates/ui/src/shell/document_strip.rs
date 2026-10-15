use super::editor_io::DocumentRef;
use super::Shell;
use crate::editor_pane::{document_labels, DocumentLabel, EditorPane};
use crate::theme::icons::Icon;
use domain::PanelId;
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, MouseButton, MouseDownEvent, SharedString, Stateful};
use std::path::Path;

const STRIP_HEIGHT_PX: f32 = 30.0;

pub(super) fn strip_height(scale: f32) -> f32 { STRIP_HEIGHT_PX * scale }
const PILL_HEIGHT_PX: f32 = 24.0;
const PILL_MAX_WIDTH_PX: f32 = 200.0;
const PILL_PADDING_LEFT_PX: f32 = 10.0;
const PILL_PADDING_RIGHT_PX: f32 = 4.0;
const PILL_GAP_PX: f32 = 5.0;
const PILL_RADIUS_PX: f32 = 6.0;
const STRIP_GAP_PX: f32 = 2.0;
const DIRTY_DOT_PX: f32 = 6.0;
const CLOSE_BUTTON_PX: f32 = 16.0;
const CLOSE_BUTTON_RADIUS_PX: f32 = 4.0;
const CLOSE_ICON_PX: f32 = 8.0;

struct DocumentPill {
    document: DocumentRef,
    index: usize,
    label: DocumentLabel,
    path: SharedString,
    is_active: bool,
    is_dirty: bool,
}

impl Shell {
    pub(super) fn render_document_strip(
        &self,
        panel: PanelId,
        is_focused: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = self.editor_pane(panel)?;
        let theme = &self.state.theme;
        let pills = document_pills(panel, pane);
        let scale = self.state.settings.ui_scale;
        let strip = div()
            .id(SharedString::from(format!("document-strip-{panel}")))
            .absolute()
            .top(px(theme.layout.pane_header_height_px))
            .left_0()
            .right_0()
            .h(px(strip_height(scale)))
            .px(px(theme.pane_header.padding_left_px - PILL_PADDING_RIGHT_PX * scale))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(STRIP_GAP_PX * scale))
            .overflow_x_scroll()
            .track_scroll(pane.strip_scroll())
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .text_size(px(theme.ui_font.pane_title_px))
            .bg(theme.surface.canvas)
            .border_b_1()
            .border_color(theme.border.muted)
            .children(pills.into_iter().map(|pill| self.render_document_pill(pill, is_focused, cx)));
        Some(strip.into_any_element())
    }

    fn render_document_pill(&self, pill: DocumentPill, is_focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.state.theme;
        let hover_fill = theme.chrome.control_fill;
        let scale = self.state.settings.ui_scale;
        let group = SharedString::from(format!("document-pill-{}", pill.document.id.as_uuid()));
        let DocumentRef { panel, .. } = pill.document;
        let index = pill.index;
        let text_color = match (pill.is_active, is_focused) {
            (true, true) => theme.text.default,
            (true, false) => theme.text.muted,
            (false, _) => theme.text.subtle,
        };
        div()
            .id(group.clone())
            .group(group.clone())
            .flex_none()
            .max_w(px(PILL_MAX_WIDTH_PX * scale))
            .h(px(PILL_HEIGHT_PX * scale))
            .pl(px(PILL_PADDING_LEFT_PX * scale))
            .pr(px(PILL_PADDING_RIGHT_PX * scale))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(PILL_GAP_PX * scale))
            .rounded(px(PILL_RADIUS_PX * scale))
            .cursor_pointer()
            .text_color(text_color)
            .when(pill.is_active, |el| {
                el.bg(theme.chrome.control_fill)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!pill.is_active, |el| el.hover(move |style| style.bg(hover_fill)))
            .tooltip(self.tooltip_builder(pill.path.clone()))
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
                    shell.request_close_document(pill.document, cx);
                    cx.notify();
                }),
            )
            .when(pill.is_dirty, |el| el.child(self.dirty_dot()))
            .child(div().min_w(px(0.0)).truncate().child(pill.label.name))
            .children(pill.label.detail.map(|detail| div().flex_none().text_color(theme.text.subtle).child(detail)))
            .child(self.document_close_button(pill.document, group, pill.is_active, cx))
            .into_any_element()
    }

    fn dirty_dot(&self) -> impl IntoElement {
        div().flex_none().size(px(DIRTY_DOT_PX * self.state.settings.ui_scale)).rounded_full().bg(self.state.theme.accent.primary)
    }

    fn document_close_button(
        &self,
        document: DocumentRef,
        group: SharedString,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let hover_fill = self.state.theme.chrome.icon_hover_fill;
        let scale = self.state.settings.ui_scale;
        div()
            .id(SharedString::from(format!("document-close-{}", document.id.as_uuid())))
            .flex_none()
            .size(px(CLOSE_BUTTON_PX * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(CLOSE_BUTTON_RADIUS_PX * scale))
            .hover(move |style| style.bg(hover_fill))
            .when(!is_active, |button| button.invisible().group_hover(group, |style| style.visible()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    shell.request_close_document(document, cx);
                    cx.notify();
                }),
            )
            .child(Icon::Close.sized(CLOSE_ICON_PX * scale, self.state.theme.text.subtle))
    }
}

fn document_pills(panel: PanelId, pane: &EditorPane) -> Vec<DocumentPill> {
    let paths: Vec<&Path> = pane.documents().iter().map(|document| document.path.as_path()).collect();
    let labels = document_labels(&paths);
    pane.documents()
        .iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (document, label))| DocumentPill {
            document: DocumentRef { panel, id: document.id },
            index,
            label,
            path: SharedString::from(document.path.display().to_string()),
            is_active: index == pane.active_index(),
            is_dirty: document.is_dirty(),
        })
        .collect()
}
