#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::app::AppState;
use crate::command::CommandRegistry;
use crate::theme::Theme;
use gpui::{div, Div, Hsla, ParentElement, Styled};

pub fn build_overlay(state: &AppState, theme: &Theme) -> Div {
    let backdrop_bg = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.35,
    };
    let panel_bg = theme.surface.raised;
    let query = state.palette_query.clone();
    let query_bg = theme.surface.subtle;
    let query_text = theme.text.default;
    let selection = state.palette_selection;
    let matches: Vec<(String, String, bool)> = state
        .registry
        .search(&state.palette_query)
        .into_iter()
        .enumerate()
        .map(|(idx, cmd)| {
            (
                cmd.title.to_string(),
                cmd.category.to_string(),
                idx == selection,
            )
        })
        .collect();

    div()
        .absolute()
        .inset_0()
        .bg(backdrop_bg)
        .flex()
        .items_start()
        .justify_center()
        .pt_8()
        .child(
            div()
                .w(gpui::px(560.0))
                .bg(panel_bg)
                .border_1()
                .border_color(theme.border_default())
                .rounded_md()
                .flex_col()
                .child(palette_query_panel(&query, query_bg, query_text))
                .child(palette_results_list(&matches, theme)),
        )
}

fn palette_query_panel(query: &str, bg: Hsla, fg: Hsla) -> Div {
    div()
        .px_3()
        .py_2()
        .bg(bg)
        .rounded_t_md()
        .child(query_element(query, fg))
}

fn query_element(query: &str, fg: Hsla) -> Div {
    let display = if query.is_empty() {
        "Type to search\u{2026}".to_string()
    } else {
        query.to_string()
    };
    div().text_color(fg).child(display)
}

fn palette_results_list(items: &[(String, String, bool)], theme: &Theme) -> Div {
    let mut list = div().flex_col().py_1();
    if items.is_empty() {
        list = list.child(
            div()
                .px_3()
                .py_2()
                .text_color(theme.text.muted)
                .child("No matches"),
        );
    } else {
        for (title, category, selected) in items {
            list = list.child(render_palette_row(title, category, *selected, theme));
        }
    }
    list
}

fn render_palette_row(title: &str, category: &str, selected: bool, theme: &Theme) -> Div {
    let bg = if selected {
        theme.accent.primary
    } else {
        theme.surface.raised
    };
    let fg = if selected {
        theme.text.inverse
    } else {
        theme.text.default
    };
    div()
        .px_3()
        .py_1()
        .bg(bg)
        .text_color(fg)
        .flex()
        .items_center()
        .justify_between()
        .child(title.to_string())
        .child(category.to_string())
}

pub fn command_count(registry: &CommandRegistry) -> usize {
    registry.list().len()
}

pub fn palette_placeholder_visible(state: &AppState) -> bool {
    state.palette_visible
}

pub fn palette_empty_query_visible(state: &AppState) -> bool {
    state.palette_visible && state.palette_query.is_empty()
}
