#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::filtered_choices;
use crate::theme::icons::Icon;
use crate::theme::{box_shadows, Theme};
use crate::workspace_menu::CARET;
use gpui::{div, prelude::*, px, Div, Hsla, SharedString};
use std::ops::Range;

pub const CHOICE_CATEGORY: &str = "Workspaces";
const QUERY_PLACEHOLDER: &str = "Search commands";
const NO_RESULTS: &str = "No Results";
const FOOTER_HINTS: [(&str, &str); 3] = [("\u{2191}\u{2193}", "Navigate"), ("\u{21b5}", "Run"), ("esc", "Close")];
const SEARCH_PADDING_X_PX: f32 = 16.0;
const SEARCH_GAP_PX: f32 = 10.0;
const RESULTS_PADDING_PX: f32 = 6.0;
const ROW_PADDING_X_PX: f32 = 10.0;
const CATEGORY_GAP_PX: f32 = 8.0;
const FOOTER_PADDING_X_PX: f32 = 16.0;
const FOOTER_HINT_GAP_PX: f32 = 14.0;
const FOOTER_KEY_GAP_PX: f32 = 4.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteRow {
    pub title: String,
    pub category: &'static str,
    pub shortcut: Option<String>,
}

pub fn palette_entries(state: &AppState) -> Vec<PaletteRow> {
    if state.palette_choices.is_some() {
        return filtered_choices(state)
            .into_iter()
            .map(|choice| PaletteRow { title: choice.title.clone(), category: CHOICE_CATEGORY, shortcut: None })
            .collect();
    }
    state
        .registry
        .search(&state.palette_query)
        .into_iter()
        .map(|cmd| PaletteRow {
            title: cmd.title.to_string(),
            category: cmd.category,
            shortcut: state.keymap.combo_for(cmd.id).map(|combo| combo.glyphs()),
        })
        .collect()
}

pub fn visible_rows(len: usize, selected: usize, max_rows: usize) -> Range<usize> {
    if len <= max_rows {
        return 0..len;
    }
    let start = (selected + 1).saturating_sub(max_rows).min(len - max_rows);
    start..start + max_rows
}

pub fn build_overlay(state: &AppState, theme: &Theme) -> Div {
    let palette = theme.palette;
    div().absolute().inset_0().flex().flex_col().items_center().pt(px(palette.top_px)).child(
        div()
            .w(px(palette.width_px))
            .occlude()
            .font_family(SharedString::from(theme.ui_font.family.clone()))
            .bg(theme.chrome.palette_fill)
            .border(px(palette.border_px))
            .border_color(theme.chrome.menu_border)
            .rounded(px(palette.radius_px))
            .shadow(box_shadows(&theme.shadow.palette))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(search_row(&state.palette_query, theme))
            .child(results(&palette_entries(state), state.palette_selection, theme))
            .child(footer(theme)),
    )
}

fn search_row(query: &str, theme: &Theme) -> Div {
    let palette = theme.palette;
    let typed = match query.is_empty() {
        true => div().text_color(theme.text.subtle).child(QUERY_PLACEHOLDER),
        false => div().text_color(theme.text.default).child(query.to_string()),
    };
    div()
        .h(px(palette.search_height_px))
        .flex_none()
        .px(px(SEARCH_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SEARCH_GAP_PX))
        .border_b_1()
        .border_color(theme.border.muted)
        .text_size(px(theme.ui_font.palette_input_px))
        .child(Icon::Magnifier.sized(palette.search_icon_px))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .when(query.is_empty(), |field| field.child(caret(theme)))
                .child(typed)
                .when(!query.is_empty(), |field| field.child(caret(theme))),
        )
}

fn caret(theme: &Theme) -> Div {
    div().text_color(theme.accent.primary).child(CARET)
}

fn results(rows: &[PaletteRow], selected: usize, theme: &Theme) -> Div {
    let list = div().p(px(RESULTS_PADDING_PX)).flex().flex_col();
    if rows.is_empty() {
        return list.child(
            div()
                .h(px(theme.palette.row_height_px))
                .px(px(ROW_PADDING_X_PX))
                .flex()
                .items_center()
                .text_size(px(theme.ui_font.menu_px))
                .text_color(theme.text.subtle)
                .child(NO_RESULTS),
        );
    }
    let window = visible_rows(rows.len(), selected, theme.palette.max_rows);
    list.children(rows[window.clone()].iter().zip(window).map(|(row, index)| result_row(row, index == selected, theme)))
}

fn result_row(row: &PaletteRow, is_selected: bool, theme: &Theme) -> Div {
    let (fill, title_color, label_color) = match is_selected {
        true => (theme.accent.primary, theme.text.inverse, theme.chrome.selected_shortcut),
        false => (gpui::transparent_black(), theme.text.default, theme.text.subtle),
    };
    div()
        .h(px(theme.palette.row_height_px))
        .px(px(ROW_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .rounded(px(theme.palette.row_radius_px))
        .bg(fill)
        .text_size(px(theme.ui_font.menu_px))
        .child(div().flex_none().text_color(title_color).child(row.title.clone()))
        .child(label(row.category, label_color, theme).ml(px(CATEGORY_GAP_PX)))
        .child(div().flex_1())
        .children(row.shortcut.clone().map(|shortcut| label(shortcut, label_color, theme)))
}

fn label(text: impl Into<SharedString>, color: Hsla, theme: &Theme) -> Div {
    div().flex_none().text_size(px(theme.ui_font.shortcut_px)).text_color(color).child(text.into())
}

fn footer(theme: &Theme) -> Div {
    div()
        .h(px(theme.palette.footer_height_px))
        .flex_none()
        .px(px(FOOTER_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(FOOTER_HINT_GAP_PX))
        .border_t_1()
        .border_color(theme.border.muted)
        .text_size(px(theme.ui_font.hint_px))
        .text_color(theme.text.subtle)
        .children(FOOTER_HINTS.iter().map(|(keys, action)| {
            div().flex().flex_row().gap(px(FOOTER_KEY_GAP_PX)).child(*keys).child(*action)
        }))
}
