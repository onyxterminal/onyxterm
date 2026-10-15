#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::file_tree::Row;
use crate::git_status::GitMark;
use crate::theme::Theme;
use gpui::{
    div, prelude::*, px, uniform_list, App, Div, FontWeight, Hsla, SharedString, UniformList, UniformListScrollHandle,
    Window,
};
use std::ops::Range;

pub const COLLAPSED_CHEVRON: &str = "\u{25B8}";
pub const EXPANDED_CHEVRON: &str = "\u{25BE}";
pub const TREE_WIDTH_PX: f32 = 236.0;
pub const TREE_BODY_PADDING_TOP_PX: f32 = 2.0;
pub const TREE_BODY_PADDING_X_PX: f32 = 6.0;
pub const TREE_BODY_PADDING_BOTTOM_PX: f32 = 10.0;
const ROW_HEIGHT_PX: f32 = 24.0;
const ROW_PADDING_X_PX: f32 = 8.0;
const ROW_GAP_PX: f32 = 6.0;
const ROW_RADIUS_PX: f32 = 5.0;
const ROW_TEXT_PX: f32 = 12.5;
const INDENT_PX: f32 = 14.0;
const CHEVRON_SLOT_PX: f32 = 10.0;
const CHEVRON_TEXT_PX: f32 = 9.0;
const MARK_TEXT_PX: f32 = 10.5;
const MARK_PADDING_LEFT_PX: f32 = 6.0;
const IGNORED_OPACITY: f32 = 0.55;

#[derive(Debug, Clone, Copy)]
pub struct RowPaint {
    pub is_selected: bool,
    pub mark: Option<GitMark>,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowColors {
    pub name: Hsla,
    pub mark: Hsla,
    pub fill: Option<Hsla>,
    pub opacity: f32,
}

pub fn mark_color(mark: GitMark, theme: &Theme) -> Hsla {
    if mark.is_addition() {
        return theme.editor.diff_added;
    }
    theme.ansi.yellow
}

pub fn row_colors(row: &Row, paint: RowPaint, theme: &Theme) -> RowColors {
    let mark = paint.mark.map_or(theme.text.default, |mark| mark_color(mark, theme));
    let name = match (row.is_ignored, paint.mark) {
        (true, _) => theme.text.subtle,
        (false, Some(_)) if !row.is_dir => mark,
        _ => theme.text.default,
    };
    RowColors {
        name,
        mark,
        fill: paint.is_selected.then_some(theme.accent.selection),
        opacity: if row.is_ignored { IGNORED_OPACITY } else { 1.0 },
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

pub fn tree_row(row: &Row, paint: RowPaint, theme: &Theme) -> Div {
    let colors = row_colors(row, paint, theme);
    let scale = paint.scale;
    let chevron = div()
        .w(px(CHEVRON_SLOT_PX * scale))
        .flex_none()
        .text_size(px(CHEVRON_TEXT_PX * scale))
        .text_color(theme.text.subtle)
        .children(chevron_glyph(row));
    let mark = paint.mark.map(|mark| {
        div()
            .flex_none()
            .pl(px(MARK_PADDING_LEFT_PX * scale))
            .text_size(px(MARK_TEXT_PX * scale))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.mark)
            .child(mark.letter())
    });
    div()
        .h(px(ROW_HEIGHT_PX * scale))
        .w_full()
        .pl(px((ROW_PADDING_X_PX + row.depth as f32 * INDENT_PX) * scale))
        .pr(px(ROW_PADDING_X_PX * scale))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP_PX * scale))
        .rounded(px(ROW_RADIUS_PX * scale))
        .bg(colors.fill.unwrap_or_else(gpui::transparent_black))
        .opacity(colors.opacity)
        .text_size(px(ROW_TEXT_PX * scale))
        .child(chevron)
        .child(div().min_w(px(0.0)).truncate().text_color(colors.name).child(SharedString::from(row.name.clone())))
        .child(div().flex_1())
        .children(mark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn row(is_dir: bool, is_ignored: bool) -> Row {
        Row { path: PathBuf::from("/r/src"), name: "src".to_string(), depth: 0, is_dir, is_expanded: false, is_ignored }
    }

    fn paint(is_selected: bool, mark: Option<GitMark>) -> RowPaint {
        RowPaint { is_selected, mark, scale: 1.0 }
    }

    fn theme() -> Theme {
        Theme::load(crate::theme::ThemeKind::Dark).unwrap()
    }

    #[test]
    fn plain_row_uses_default_text_and_no_fill() {
        let theme = theme();
        let colors = row_colors(&row(false, false), paint(false, None), &theme);
        assert_eq!(colors.name, theme.text.default);
        assert_eq!(colors.fill, None);
        assert_eq!(colors.opacity, 1.0);
    }

    #[test]
    fn ignored_row_is_subtle_and_faded() {
        let theme = theme();
        let colors = row_colors(&row(false, true), paint(false, None), &theme);
        assert_eq!(colors.name, theme.text.subtle);
        assert_eq!(colors.opacity, IGNORED_OPACITY);
    }

    #[test]
    fn selected_row_uses_the_pale_selection_fill_and_keeps_its_text() {
        let theme = theme();
        let colors = row_colors(&row(false, false), paint(true, None), &theme);
        assert_eq!(colors.fill, Some(theme.accent.selection));
        assert_eq!(colors.name, theme.text.default);
    }

    #[test]
    fn changed_files_tint_their_name_and_letter() {
        let theme = theme();
        let modified = row_colors(&row(false, false), paint(false, Some(GitMark::Modified)), &theme);
        assert_eq!((modified.name, modified.mark), (theme.ansi.yellow, theme.ansi.yellow));
        let untracked = row_colors(&row(false, false), paint(false, Some(GitMark::Untracked)), &theme);
        assert_eq!((untracked.name, untracked.mark), (theme.editor.diff_added, theme.editor.diff_added));
    }

    #[test]
    fn folders_with_changes_keep_their_name_and_show_a_yellow_dot() {
        let theme = theme();
        let colors = row_colors(&row(true, false), paint(false, Some(GitMark::ChangedInside)), &theme);
        assert_eq!(colors.name, theme.text.default);
        assert_eq!(colors.mark, theme.ansi.yellow);
    }

    #[test]
    fn only_directories_get_a_chevron_that_tracks_expansion() {
        let mut expanded = row(true, false);
        expanded.is_expanded = true;
        assert_eq!(chevron_glyph(&row(false, false)), None);
        assert_eq!(chevron_glyph(&row(true, false)), Some(COLLAPSED_CHEVRON));
        assert_eq!(chevron_glyph(&expanded), Some(EXPANDED_CHEVRON));
    }
}
