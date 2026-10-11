use super::{box_shadows, Theme};
use crate::text_field::TextField;
use gpui::{canvas, div, prelude::*, px, AnyElement, Div, ElementId, FontWeight, Hsla, SharedString, Stateful};
use std::ops::Range;

const CARET_WIDTH_PX: f32 = 1.5;
const FIELD_SELECTION_ALPHA: f32 = 0.35;
const SEPARATOR_MARGIN_Y_PX: f32 = 5.0;
const SEPARATOR_THICKNESS_PX: f32 = 1.0;
const FOCUS_RING_ALPHA: f32 = 0.25;
const FOCUS_RING_SPREAD_PX: f32 = 3.0;

#[derive(Clone, Copy)]
struct SpanStyle {
    is_selected: bool,
    is_marked: bool,
}

pub struct MenuRowStyle {
    pub group: SharedString,
    pub is_highlighted: bool,
    pub is_destructive: bool,
}

pub fn menu_panel(theme: &Theme) -> Div {
    let menu = theme.menu;
    div()
        .w(px(menu.width_px))
        .p(px(menu.padding_px))
        .font_family(SharedString::from(theme.ui_font.family.clone()))
        .text_size(px(theme.ui_font.menu_px))
        .text_color(theme.text.default)
        .bg(theme.chrome.menu_fill)
        .border(px(menu.border_px))
        .border_color(theme.chrome.menu_border)
        .rounded(px(menu.radius_px))
        .shadow(box_shadows(&theme.shadow.menu))
        .flex()
        .flex_col()
}

pub fn menu_row(id: impl Into<ElementId>, style: MenuRowStyle, theme: &Theme) -> Stateful<Div> {
    let menu = theme.menu;
    let (highlight, inverse) = (theme.accent.primary, theme.text.inverse);
    let text_color = match style.is_destructive {
        true => theme.chrome.destructive,
        false => theme.text.default,
    };
    div()
        .id(id)
        .group(style.group)
        .h(px(menu.row_height_px))
        .flex_none()
        .px(px(menu.row_padding_x_px))
        .flex()
        .flex_row()
        .items_center()
        .rounded(px(menu.row_radius_px))
        .cursor_pointer()
        .text_color(text_color)
        .when(style.is_highlighted, |row| row.bg(highlight).text_color(inverse))
        .hover(move |style| style.bg(highlight).text_color(inverse))
}

pub fn menu_shortcut(text: impl Into<SharedString>, style: &MenuRowStyle, theme: &Theme) -> Div {
    let selected = theme.chrome.selected_shortcut;
    let color = match style.is_highlighted {
        true => selected,
        false => theme.text.subtle,
    };
    div()
        .flex_none()
        .text_size(px(theme.ui_font.shortcut_px))
        .text_color(color)
        .group_hover(style.group.clone(), move |style| style.text_color(selected))
        .child(text.into())
}

pub fn menu_separator(theme: &Theme) -> Div {
    let inset = px(theme.menu.row_padding_x_px);
    div().flex_none().h(px(SEPARATOR_THICKNESS_PX)).mx(inset).my(px(SEPARATOR_MARGIN_Y_PX)).bg(theme.border.muted)
}

pub fn menu_section_label(text: &'static str, theme: &Theme) -> Div {
    div()
        .h(px(theme.menu.section_label_height_px))
        .px(px(theme.menu.row_padding_x_px))
        .flex()
        .items_center()
        .text_size(px(theme.ui_font.section_label_px))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.subtle)
        .child(text)
}

pub fn focused_field(theme: &Theme) -> Div {
    div()
        .bg(theme.surface.canvas)
        .border_1()
        .border_color(theme.accent.primary)
        .shadow(vec![focus_ring(theme.accent.primary)])
}

fn focus_ring(accent: Hsla) -> gpui::BoxShadow {
    gpui::BoxShadow {
        color: accent.opacity(FOCUS_RING_ALPHA),
        offset: gpui::point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(FOCUS_RING_SPREAD_PX),
    }
}

pub fn field_text(field: &TextField, placeholder: &str, theme: &Theme) -> Div {
    let row = div().flex().flex_row().whitespace_nowrap();
    if field.is_empty() {
        let placeholder = div().text_color(theme.text.subtle).child(placeholder.to_string());
        return row.child(caret(field, theme)).child(placeholder);
    }
    row.children(field_spans(field, theme))
}

fn field_spans(field: &TextField, theme: &Theme) -> Vec<AnyElement> {
    let text = field.text();
    let selection = field.selection();
    let marked = field.marked_range().unwrap_or_default();
    let mut cuts = vec![0, text.len(), field.caret(), selection.start, selection.end, marked.start, marked.end];
    cuts.sort_unstable();
    cuts.dedup();
    let mut spans = Vec::new();
    for span in cuts.windows(2).map(|pair| pair[0]..pair[1]) {
        if span.start == field.caret() && selection.is_empty() {
            spans.push(caret(field, theme).into_any_element());
        }
        let style = SpanStyle { is_selected: covers(&selection, &span), is_marked: covers(&marked, &span) };
        spans.push(field_span(text[span].to_string(), style, theme).into_any_element());
    }
    if field.caret() == text.len() && selection.is_empty() {
        spans.push(caret(field, theme).into_any_element());
    }
    spans
}

fn covers(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    !outer.is_empty() && outer.start <= inner.start && inner.end <= outer.end
}

fn field_span(text: String, style: SpanStyle, theme: &Theme) -> Div {
    div()
        .text_color(theme.text.default)
        .when(style.is_selected, |span| span.bg(theme.accent.primary.opacity(FIELD_SELECTION_ALPHA)))
        .when(style.is_marked, |span| span.underline())
        .child(text)
}

fn caret(field: &TextField, theme: &Theme) -> Div {
    let caret_bounds = field.caret_bounds().clone();
    let probe = canvas(move |bounds, _window, _cx| caret_bounds.set(Some(bounds)), |_bounds, _state, _window, _cx| {});
    div()
        .flex_none()
        .w(px(CARET_WIDTH_PX))
        .mx(px(-CARET_WIDTH_PX / 2.0))
        .bg(theme.accent.primary)
        .child(probe.size_full())
}
