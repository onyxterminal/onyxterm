#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::file_tree::Row;
use crate::theme::Theme;
use gpui::{div, prelude::*, px, uniform_list, App, Div, Hsla, SharedString, UniformList, UniformListScrollHandle, Window};
use std::ops::Range;

pub const COLLAPSED_CHEVRON: &str = "\u{25B8}";
pub const EXPANDED_CHEVRON: &str = "\u{25BE}";
const INDENT_PX: f32 = 14.0;
const CHEVRON_SLOT_PX: f32 = 14.0;
const ROW_PADDING_X_PX: f32 = 8.0;

pub fn row_style(row: &Row, is_selected: bool, is_focused: bool, theme: &Theme) -> (SharedString, Hsla, Option<Hsla>) {
    let label = SharedString::from(row.name.clone());
    let resting_color = if row.is_ignored { theme.text.muted } else { theme.text.default };
    match (is_selected, is_focused) {
        (true, true) => (label, theme.text.inverse, Some(theme.accent.primary)),
        (true, false) => (label, resting_color, Some(theme.chrome.control_fill)),
        (false, _) => (label, resting_color, None),
    }
}

pub fn chevron_glyph(row: &Row) -> Option<&'static str> {
    if !row.is_dir {
        return None;
    }
    Some(if row.is_expanded { EXPANDED_CHEVRON } else { COLLAPSED_CHEVRON })
}

pub fn tree_list(
    id: SharedString,
    row_count: usize,
    scroll: &UniformListScrollHandle,
    render_rows: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<Div> + 'static,
) -> UniformList {
    uniform_list(id, row_count, render_rows).size_full().track_scroll(scroll.clone())
}

pub fn tree_row(row: &Row, is_selected: bool, is_focused: bool, theme: &Theme) -> Div {
    let (label, text_color, fill) = row_style(row, is_selected, is_focused, theme);
    let chevron = div().w(px(CHEVRON_SLOT_PX)).flex_none().children(chevron_glyph(row));
    div()
        .h(px(theme.menu.row_height_px))
        .w_full()
        .pl(px(ROW_PADDING_X_PX + row.depth as f32 * INDENT_PX))
        .pr(px(ROW_PADDING_X_PX))
        .flex()
        .flex_row()
        .items_center()
        .rounded(px(theme.menu.row_radius_px))
        .bg(fill.unwrap_or_else(gpui::transparent_black))
        .text_size(px(theme.ui_font.menu_px))
        .text_color(text_color)
        .child(chevron)
        .child(div().flex_none().whitespace_nowrap().child(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn row(is_dir: bool, is_expanded: bool, is_ignored: bool) -> Row {
        Row { path: PathBuf::from("/r/src"), name: "src".to_string(), depth: 0, is_dir, is_expanded, is_ignored }
    }

    fn theme() -> Theme {
        Theme::load_default().unwrap()
    }

    #[test]
    fn plain_row_uses_default_text_and_no_fill() {
        let theme = theme();
        let (label, color, fill) = row_style(&row(false, false, false), false, true, &theme);
        assert_eq!(label, SharedString::from("src"));
        assert_eq!(color, theme.text.default);
        assert_eq!(fill, None);
    }

    #[test]
    fn ignored_row_is_muted() {
        let theme = theme();
        let (_, color, _) = row_style(&row(false, false, true), false, true, &theme);
        assert_eq!(color, theme.text.muted);
    }

    #[test]
    fn selected_row_in_focused_pane_uses_the_palette_selection() {
        let theme = theme();
        let (_, color, fill) = row_style(&row(false, false, false), true, true, &theme);
        assert_eq!(fill, Some(theme.accent.primary));
        assert_eq!(color, theme.text.inverse);
    }

    #[test]
    fn selected_row_in_unfocused_pane_uses_a_muted_fill() {
        let theme = theme();
        let (_, color, fill) = row_style(&row(false, false, true), true, false, &theme);
        assert_eq!(fill, Some(theme.chrome.control_fill));
        assert_eq!(color, theme.text.muted);
    }

    #[test]
    fn only_directories_get_a_chevron_that_tracks_expansion() {
        assert_eq!(chevron_glyph(&row(false, false, false)), None);
        assert_eq!(chevron_glyph(&row(true, false, false)), Some(COLLAPSED_CHEVRON));
        assert_eq!(chevron_glyph(&row(true, true, false)), Some(EXPANDED_CHEVRON));
    }
}
