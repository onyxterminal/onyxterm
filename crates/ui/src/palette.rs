#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::filtered_choices;
use crate::theme::icons::Icon;
use crate::text_field::TextField;
use crate::theme::menu_chrome::field_text;
use crate::theme::{box_shadows, Theme};
use gpui::{div, prelude::*, px, uniform_list, App, Div, Hsla, MouseButton, SharedString, UniformListScrollHandle, Window};
use std::rc::Rc;

pub type PaletteSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub const CHOICE_CATEGORY: &str = "Workspaces";
const QUERY_PLACEHOLDER: &str = "Search commands";
const NO_RESULTS: &str = "No Results";
const RESULTS_ID: &str = "palette-results";
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
        .search(state.palette_query.text())
        .into_iter()
        .map(|cmd| PaletteRow {
            title: cmd.title.to_string(),
            category: cmd.category,
            shortcut: state.keymap.combo_for(cmd.id).map(|combo| combo.glyphs()),
        })
        .collect()
}

pub fn build_overlay(state: &AppState, theme: &Theme, scroll: &UniformListScrollHandle, on_select: PaletteSelect) -> Div {
    let palette = theme.palette;
    div().absolute().inset_0().flex().flex_col().items_center().px(px(16.0)).pt(px(palette.top_px)).child(
        div()
            .w(px(palette.width_px)).max_w_full()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| cx.stop_propagation())
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
            .child(results(palette_entries(state), state.palette_selection, theme, scroll, on_select))
            .child(footer(theme)),
    )
}

fn search_row(query: &TextField, theme: &Theme) -> Div {
    let palette = theme.palette;
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
        .child(field_text(query, QUERY_PLACEHOLDER, theme))
}

fn results(rows: Vec<PaletteRow>, selected: usize, theme: &Theme, scroll: &UniformListScrollHandle, on_select: PaletteSelect) -> Div {
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
    let height = theme.palette.row_height_px * rows.len().min(theme.palette.max_rows) as f32;
    let row_theme = theme.clone();
    let count = rows.len();
    let scrolling_rows = uniform_list(RESULTS_ID, count, move |range, _window, _cx| {
        range.map(|index| {
            let on_select = on_select.clone();
            result_row(&rows[index], index == selected, &row_theme)
                .id(("palette-command", index)).cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    cx.stop_propagation(); on_select(index, window, cx);
                })
        }).collect::<Vec<_>>()
    });
    list.child(scrolling_rows.w_full().h(px(height)).track_scroll(scroll.clone()))
}

fn result_row(row: &PaletteRow, is_selected: bool, theme: &Theme) -> Div {
    let (fill, title_color, label_color) = match is_selected {
        true => (theme.accent.primary, theme.text.inverse, theme.text.inverse),
        false => (gpui::transparent_black(), theme.text.default, theme.text.subtle),
    };
    div()
        .w_full()
        .h(px(theme.palette.row_height_px))
        .px(px(ROW_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .rounded(px(theme.palette.row_radius_px))
        .bg(fill)
        .text_size(px(theme.ui_font.menu_px))
        .child(div().flex_1().min_w(px(0.0)).truncate().text_color(title_color).child(row.title.clone()))
        .child(label(row.category, label_color, theme).ml(px(CATEGORY_GAP_PX)))
        .children(row.shortcut.clone().map(|shortcut| label(shortcut, label_color, theme).ml(px(CATEGORY_GAP_PX))))
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
