use crate::buffer::line_len;
use crate::selection::Selection;
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub const TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    PageUp(usize),
    PageDown(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

pub fn move_selection(rope: &Rope, selection: Selection, motion: Motion, extend: bool) -> Selection {
    if !extend && !selection.is_empty() {
        match motion {
            Motion::Left => return Selection::cursor(selection.range().start),
            Motion::Right => return Selection::cursor(selection.range().end),
            _ => {}
        }
    }
    let (head, goal_column) = match motion {
        Motion::Up => vertical(rope, selection, -1),
        Motion::Down => vertical(rope, selection, 1),
        Motion::PageUp(lines) => vertical(rope, selection, -(lines as isize)),
        Motion::PageDown(lines) => vertical(rope, selection, lines as isize),
        other => (horizontal(rope, selection.head, other), None),
    };
    let anchor = if extend { selection.anchor } else { head };
    Selection { anchor, head, goal_column }
}

pub fn display_column(line: &str, char_column: usize) -> usize {
    line.chars().take(char_column).fold(0, advance)
}

pub fn char_column(line: &str, display_column: usize) -> usize {
    let mut column = 0;
    for (index, ch) in line.chars().enumerate() {
        let next = advance(column, ch);
        if next > display_column {
            return index;
        }
        column = next;
    }
    line.chars().count()
}

pub(crate) fn prev_grapheme(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let line = rope.char_to_line(offset);
    let start = rope.line_to_char(line);
    if offset == start {
        return offset - 1;
    }
    let column = offset - start;
    grapheme_boundaries(&line_text(rope, line))
        .into_iter()
        .take_while(|boundary| *boundary < column)
        .last()
        .map_or(offset - 1, |boundary| start + boundary)
}

pub(crate) fn next_grapheme(rope: &Rope, offset: usize) -> usize {
    if offset >= rope.len_chars() {
        return rope.len_chars();
    }
    let line = rope.char_to_line(offset);
    let start = rope.line_to_char(line);
    let column = offset - start;
    if column >= line_len(rope, line) {
        return offset + 1;
    }
    grapheme_boundaries(&line_text(rope, line))
        .into_iter()
        .find(|boundary| *boundary > column)
        .map_or(offset + 1, |boundary| start + boundary)
}

pub(crate) fn word_right(rope: &Rope, offset: usize) -> usize {
    let mut chars = rope.chars_at(offset).peekable();
    let mut position = offset;
    while chars.peek().is_some_and(|ch| class(*ch) == CharClass::Whitespace) {
        chars.next();
        position += 1;
    }
    let Some(first) = chars.peek().map(|ch| class(*ch)) else {
        return position;
    };
    while chars.peek().is_some_and(|ch| class(*ch) == first) {
        chars.next();
        position += 1;
    }
    position
}

pub(crate) fn word_left(rope: &Rope, offset: usize) -> usize {
    let mut chars = rope.chars_at(offset);
    let mut position = offset;
    let mut previous = chars.prev();
    while previous.is_some_and(|ch| class(ch) == CharClass::Whitespace) {
        position -= 1;
        previous = chars.prev();
    }
    let Some(first) = previous.map(class) else {
        return position;
    };
    while previous.is_some_and(|ch| class(ch) == first) {
        position -= 1;
        previous = chars.prev();
    }
    position
}

pub(crate) fn line_start(rope: &Rope, offset: usize) -> usize {
    rope.line_to_char(rope.char_to_line(offset))
}

pub(crate) fn line_end(rope: &Rope, offset: usize) -> usize {
    let line = rope.char_to_line(offset);
    rope.line_to_char(line) + line_len(rope, line)
}

fn horizontal(rope: &Rope, head: usize, motion: Motion) -> usize {
    match motion {
        Motion::Left => prev_grapheme(rope, head),
        Motion::Right => next_grapheme(rope, head),
        Motion::WordLeft => word_left(rope, head),
        Motion::WordRight => word_right(rope, head),
        Motion::LineStart => smart_line_start(rope, head),
        Motion::LineEnd => line_end(rope, head),
        Motion::DocumentStart => 0,
        Motion::DocumentEnd => rope.len_chars(),
        Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_) => head,
    }
}

fn vertical(rope: &Rope, selection: Selection, lines: isize) -> (usize, Option<usize>) {
    let line = rope.char_to_line(selection.head);
    let goal = selection
        .goal_column
        .unwrap_or_else(|| display_column(&line_text(rope, line), selection.head - rope.line_to_char(line)));
    let target = line as isize + lines;
    if target < 0 {
        return (0, Some(goal));
    }
    let target = target as usize;
    if target >= rope.len_lines() {
        return (rope.len_chars(), Some(goal));
    }
    let column = char_column(&line_text(rope, target), goal).min(line_len(rope, target));
    (rope.line_to_char(target) + column, Some(goal))
}

fn smart_line_start(rope: &Rope, head: usize) -> usize {
    let line = rope.char_to_line(head);
    let start = rope.line_to_char(line);
    let indent = rope.line(line).chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
    let first_text = start + indent;
    if head == first_text {
        start
    } else {
        first_text
    }
}

fn line_text(rope: &Rope, line: usize) -> String {
    rope.line(line).to_string()
}

fn grapheme_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut chars = 0;
    for grapheme in text.graphemes(true) {
        chars += grapheme.chars().count();
        boundaries.push(chars);
    }
    boundaries
}

fn class(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Whitespace
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn advance(column: usize, ch: char) -> usize {
    if ch == '\t' {
        column + TAB_WIDTH - column % TAB_WIDTH
    } else {
        column + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(text: &str, head: usize, motion: Motion) -> usize {
        move_selection(&Rope::from_str(text), Selection::cursor(head), motion, false).head
    }

    #[test]
    fn right_moves_over_a_whole_grapheme_cluster() {
        let family = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b";
        assert_eq!(moved(family, 1, Motion::Right), 6);
        assert_eq!(moved(family, 6, Motion::Left), 1);
        assert_eq!(moved("e\u{301}x", 0, Motion::Right), 2);
    }

    #[test]
    fn left_and_right_cross_line_breaks() {
        assert_eq!(moved("ab\ncd", 3, Motion::Left), 2);
        assert_eq!(moved("ab\ncd", 2, Motion::Right), 3);
        assert_eq!(moved("ab", 0, Motion::Left), 0);
        assert_eq!(moved("ab", 2, Motion::Right), 2);
    }

    #[test]
    fn word_motions_stop_at_class_boundaries() {
        let text = "foo.bar  baz";
        assert_eq!(moved(text, 0, Motion::WordRight), 3);
        assert_eq!(moved(text, 3, Motion::WordRight), 4);
        assert_eq!(moved(text, 4, Motion::WordRight), 7);
        assert_eq!(moved(text, 7, Motion::WordRight), 12);
        assert_eq!(moved(text, 12, Motion::WordLeft), 9);
        assert_eq!(moved(text, 9, Motion::WordLeft), 4);
    }

    #[test]
    fn line_start_toggles_between_indent_and_column_zero() {
        assert_eq!(moved("    xy", 6, Motion::LineStart), 4);
        assert_eq!(moved("    xy", 4, Motion::LineStart), 0);
        assert_eq!(moved("ab\ncd\n", 3, Motion::LineEnd), 5);
    }

    #[test]
    fn vertical_motion_keeps_display_goal_column_across_tabs() {
        let rope = Rope::from_str("\tab\nabcdefgh\nx");
        let start = Selection::cursor(3);
        let down = move_selection(&rope, start, Motion::Down, false);
        assert_eq!(down.head, 10);
        let down_again = move_selection(&rope, down, Motion::Down, false);
        assert_eq!(down_again.head, 14);
        let up = move_selection(&rope, down_again, Motion::Up, false);
        assert_eq!(up.head, 10);
    }

    #[test]
    fn up_on_first_line_goes_to_start_and_down_on_last_goes_to_end() {
        assert_eq!(moved("abc\ndef", 2, Motion::Up), 0);
        assert_eq!(moved("abc\ndef", 5, Motion::Down), 7);
    }

    #[test]
    fn plain_left_collapses_a_selection_to_its_start() {
        let rope = Rope::from_str("abcdef");
        let moved = move_selection(&rope, Selection::new(1, 4), Motion::Left, false);
        assert_eq!(moved, Selection::cursor(1));
    }

    #[test]
    fn extending_keeps_the_anchor() {
        let rope = Rope::from_str("abcdef");
        let moved = move_selection(&rope, Selection::cursor(2), Motion::WordRight, true);
        assert_eq!((moved.anchor, moved.head), (2, 6));
    }

    #[test]
    fn display_and_char_columns_round_trip_through_tabs() {
        assert_eq!(display_column("\tab", 2), 5);
        assert_eq!(char_column("\tab", 5), 2);
        assert_eq!(char_column("\tab", 2), 0);
    }
}
