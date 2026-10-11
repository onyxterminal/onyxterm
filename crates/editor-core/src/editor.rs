use crate::buffer::{Buffer, CRLF, LF};
use crate::edit::{Bias, Change, Edit};
use crate::history::{EditKind, History, Record};
use crate::motion::{line_end, line_start, move_selection, next_grapheme, prev_grapheme, word_left, word_right, Motion};
use crate::selection::{Selection, Selections};
use ropey::Rope;
use std::time::Instant;

const NEWLINE: &str = LF;
const INDENT: &str = "    ";
const INDENT_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Move { motion: Motion, extend: bool },
    InsertText(String),
    Newline,
    DeleteBackward(Granularity),
    DeleteForward(Granularity),
    Indent,
    Outdent,
    SelectAll,
    Undo,
    Redo,
    Cut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    AfterInsert,
    Mapped,
}

pub struct Editor {
    buffer: Buffer,
    selections: Selections,
    history: History,
}

impl Editor {
    pub fn new(buffer: Buffer) -> Self {
        Self { buffer, selections: Selections::single(Selection::cursor(0)), history: History::default() }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    pub fn set_selections(&mut self, selections: Selections) {
        let len = self.buffer.len_chars();
        self.selections = selections.map(|selection| Selection {
            anchor: selection.anchor.min(len),
            head: selection.head.min(len),
            goal_column: selection.goal_column,
        });
    }

    pub fn mark_saved(&mut self) {
        self.buffer.mark_saved();
    }

    pub fn apply(&mut self, action: EditorAction, now: Instant) -> Option<String> {
        match action {
            EditorAction::Move { motion, extend } => {
                let rope = self.buffer.rope();
                self.selections = self.selections.map(|selection| move_selection(rope, selection, motion, extend));
            }
            EditorAction::InsertText(text) => self.replace_selections(&text.replace(CRLF, LF), EditKind::Typing, now),
            EditorAction::Newline => self.insert_newline(now),
            EditorAction::DeleteBackward(granularity) => self.delete(granularity, true, now),
            EditorAction::DeleteForward(granularity) => self.delete(granularity, false, now),
            EditorAction::Indent => self.indent(now),
            EditorAction::Outdent => self.outdent(now),
            EditorAction::SelectAll => {
                self.selections = Selections::single(Selection::new(0, self.buffer.len_chars()));
            }
            EditorAction::Undo => {
                if let Some(selections) = self.history.undo(&mut self.buffer) {
                    self.selections = selections;
                }
            }
            EditorAction::Redo => {
                if let Some(selections) = self.history.redo(&mut self.buffer) {
                    self.selections = selections;
                }
            }
            EditorAction::Cut => return self.cut(now),
        }
        None
    }

    fn replace_selections(&mut self, text: &str, kind: EditKind, now: Instant) {
        let per_selection: Vec<Option<Change>> = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| Some(Change { range: selection.range(), text: text.to_string() }))
            .collect();
        self.commit(per_selection, kind, now, Placement::AfterInsert);
    }

    fn insert_newline(&mut self, now: Instant) {
        let rope = self.buffer.rope();
        let per_selection: Vec<Option<Change>> = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| {
                let start = line_start(rope, selection.range().start);
                let indent: String = rope
                    .slice(start..selection.range().start)
                    .chars()
                    .take_while(|ch| *ch == ' ' || *ch == '\t')
                    .collect();
                Some(Change { range: selection.range(), text: format!("{NEWLINE}{indent}") })
            })
            .collect();
        self.commit(per_selection, EditKind::Other, now, Placement::AfterInsert);
    }

    fn delete(&mut self, granularity: Granularity, is_backward: bool, now: Instant) {
        let rope = self.buffer.rope();
        let per_selection: Vec<Option<Change>> = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| {
                if !selection.is_empty() {
                    return Some(Change { range: selection.range(), text: String::new() });
                }
                let target = deletion_target(rope, selection.head, granularity, is_backward);
                let range = target.min(selection.head)..target.max(selection.head);
                (!range.is_empty()).then(|| Change { range, text: String::new() })
            })
            .collect();
        self.commit(per_selection, EditKind::Typing, now, Placement::AfterInsert);
    }

    pub fn copy_text(&self) -> Option<String> {
        let rope = self.buffer.rope();
        let parts: Vec<String> = self
            .selections
            .as_slice()
            .iter()
            .filter(|selection| !selection.is_empty())
            .map(|selection| rope.slice(selection.range()).to_string())
            .collect();
        (!parts.is_empty()).then(|| parts.join(NEWLINE))
    }

    pub fn paste(&mut self, text: &str, now: Instant) {
        let text = text.replace(CRLF, LF);
        let lines: Vec<&str> = text.split(NEWLINE).collect();
        if self.selections.len() > 1 && lines.len() == self.selections.len() {
            let per_selection: Vec<Option<Change>> = self
                .selections
                .as_slice()
                .iter()
                .zip(lines)
                .map(|(selection, line)| Some(Change { range: selection.range(), text: line.to_string() }))
                .collect();
            self.commit(per_selection, EditKind::Other, now, Placement::AfterInsert);
            return;
        }
        self.replace_selections(&text, EditKind::Other, now);
    }

    fn cut(&mut self, now: Instant) -> Option<String> {
        let text = self.copy_text()?;
        let per_selection: Vec<Option<Change>> = self
            .selections
            .as_slice()
            .iter()
            .map(|selection| {
                if selection.is_empty() {
                    None
                } else {
                    Some(Change { range: selection.range(), text: String::new() })
                }
            })
            .collect();
        self.commit(per_selection, EditKind::Other, now, Placement::AfterInsert);
        Some(text)
    }

    fn indent(&mut self, now: Instant) {
        if self.selections.as_slice().iter().all(Selection::is_empty) {
            self.replace_selections(INDENT, EditKind::Typing, now);
            return;
        }
        let rope = self.buffer.rope();
        let per_selection: Vec<Option<Change>> = touched_lines(rope, &self.selections)
            .into_iter()
            .map(|line| {
                let start = rope.line_to_char(line);
                Some(Change { range: start..start, text: INDENT.to_string() })
            })
            .collect();
        self.commit(per_selection, EditKind::Other, now, Placement::Mapped);
    }

    fn outdent(&mut self, now: Instant) {
        let rope = self.buffer.rope();
        let per_selection: Vec<Option<Change>> = touched_lines(rope, &self.selections)
            .into_iter()
            .map(|line| {
                let start = rope.line_to_char(line);
                let spaces = rope.line(line).chars().take(INDENT_WIDTH).take_while(|ch| *ch == ' ').count();
                let removable = if spaces == 0 && rope.line(line).chars().next() == Some('\t') { 1 } else { spaces };
                (removable > 0).then(|| Change { range: start..start + removable, text: String::new() })
            })
            .collect();
        self.commit(per_selection, EditKind::Other, now, Placement::Mapped);
    }

    fn commit(
        &mut self,
        per_selection: Vec<Option<Change>>,
        kind: EditKind,
        now: Instant,
        placement: Placement,
    ) {
        let per_selection: Vec<Option<Change>> = per_selection
            .into_iter()
            .map(|c| c.filter(|c| !(c.range.is_empty() && c.text.is_empty())))
            .collect();
        let changes: Vec<Change> = per_selection.iter().filter_map(|c| c.clone()).collect();
        let edit = Edit::new(changes);
        if edit.is_empty() {
            return;
        }
        let before = self.selections.clone();
        let inverse = edit.apply(&mut self.buffer);
        let after = match placement {
            Placement::AfterInsert => {
                let newest_before = before.newest_index();
                let cursors: Vec<Selection> = before
                    .as_slice()
                    .iter()
                    .zip(per_selection.iter())
                    .map(|(selection, change)| match change {
                        Some(c) if !c.range.is_empty() => Selection::cursor(edit.map(c.range.end, Bias::Left)),
                        Some(c) => Selection::cursor(edit.map(c.range.start, Bias::Right)),
                        None => Selection::cursor(edit.map(selection.head, Bias::Right)),
                    })
                    .collect();
                Selections::new(cursors, newest_before)
            }
            Placement::Mapped => before.map(|selection| {
                Selection::new(edit.map(selection.anchor, Bias::Right), edit.map(selection.head, Bias::Right))
            }),
        };
        self.history.record(Record { forward: edit, inverse, before, after: after.clone(), kind, at: now });
        self.selections = after;
    }
}

fn deletion_target(rope: &Rope, head: usize, granularity: Granularity, is_backward: bool) -> usize {
    match (granularity, is_backward) {
        (Granularity::Char, true) => prev_grapheme(rope, head),
        (Granularity::Char, false) => next_grapheme(rope, head),
        (Granularity::Word, true) => word_left(rope, head),
        (Granularity::Word, false) => word_right(rope, head),
        (Granularity::Line, true) => {
            let start = line_start(rope, head);
            if start == head {
                head.saturating_sub(1)
            } else {
                start
            }
        }
        (Granularity::Line, false) => {
            let end = line_end(rope, head);
            if end == head {
                next_grapheme(rope, head)
            } else {
                end
            }
        }
    }
}

fn touched_lines(rope: &Rope, selections: &Selections) -> Vec<usize> {
    let mut lines: Vec<usize> = selections
        .as_slice()
        .iter()
        .flat_map(|selection| {
            let range = selection.range();
            let first = rope.char_to_line(range.start);
            let mut last = rope.char_to_line(range.end);
            if last > first && range.end == rope.line_to_char(last) {
                last -= 1;
            }
            first..=last
        })
        .collect();
    lines.dedup();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn editor_with(text: &str, cursors: &[usize]) -> Editor {
        let mut editor = Editor::new(Buffer::from_text(text));
        let selections = cursors.iter().map(|offset| Selection::cursor(*offset)).collect();
        editor.set_selections(Selections::new(selections, 0));
        editor
    }

    fn text(editor: &Editor) -> String {
        editor.buffer().rope().to_string()
    }

    fn heads(editor: &Editor) -> Vec<usize> {
        editor.selections().as_slice().iter().map(|selection| selection.head).collect()
    }

    #[test]
    fn typing_at_three_cursors_inserts_at_each() {
        let mut editor = editor_with("a\nb\nc", &[1, 3, 5]);
        editor.apply(EditorAction::InsertText("!".into()), Instant::now());
        assert_eq!(text(&editor), "a!\nb!\nc!");
        assert_eq!(heads(&editor), vec![2, 5, 8]);
    }

    #[test]
    fn typing_replaces_a_selection() {
        let mut editor = Editor::new(Buffer::from_text("hello world"));
        editor.set_selections(Selections::single(Selection::new(0, 5)));
        editor.apply(EditorAction::InsertText("bye".into()), Instant::now());
        assert_eq!(text(&editor), "bye world");
        assert_eq!(heads(&editor), vec![3]);
    }

    #[test]
    fn quick_typing_is_one_undo_step_and_a_pause_starts_another() {
        let mut editor = editor_with("", &[0]);
        let start = Instant::now();
        editor.apply(EditorAction::InsertText("a".into()), start);
        editor.apply(EditorAction::InsertText("b".into()), start + Duration::from_millis(100));
        editor.apply(EditorAction::InsertText("c".into()), start + Duration::from_millis(1000));
        editor.apply(EditorAction::Undo, start + Duration::from_millis(1100));
        assert_eq!(text(&editor), "ab");
        editor.apply(EditorAction::Undo, start + Duration::from_millis(1200));
        assert_eq!(text(&editor), "");
        editor.apply(EditorAction::Redo, start + Duration::from_millis(1300));
        assert_eq!(text(&editor), "ab");
        assert_eq!(heads(&editor), vec![2]);
    }

    #[test]
    fn a_cursor_jump_starts_a_new_undo_step() {
        let mut editor = editor_with("xy", &[0]);
        let now = Instant::now();
        editor.apply(EditorAction::InsertText("a".into()), now);
        editor.apply(EditorAction::Move { motion: Motion::DocumentEnd, extend: false }, now);
        editor.apply(EditorAction::InsertText("b".into()), now);
        editor.apply(EditorAction::Undo, now);
        assert_eq!(text(&editor), "axy");
    }

    #[test]
    fn newline_keeps_the_current_indentation() {
        let mut editor = editor_with("    foo", &[7]);
        editor.apply(EditorAction::Newline, Instant::now());
        assert_eq!(text(&editor), "    foo\n    ");
        assert_eq!(heads(&editor), vec![12]);
    }

    #[test]
    fn backspace_with_adjacent_cursors_merges_changes() {
        let mut editor = editor_with("abc", &[1, 2]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Char), Instant::now());
        assert_eq!(text(&editor), "c");
        assert_eq!(heads(&editor), vec![0]);
    }

    #[test]
    fn backspace_deletes_a_whole_grapheme_cluster() {
        let mut editor = editor_with("e\u{301}x", &[2]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Char), Instant::now());
        assert_eq!(text(&editor), "x");
    }

    #[test]
    fn word_and_line_deletion() {
        let mut editor = editor_with("let value = 1", &[9]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Word), Instant::now());
        assert_eq!(text(&editor), "let  = 1");
        editor.apply(EditorAction::DeleteBackward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), " = 1");
        editor.apply(EditorAction::DeleteForward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), "");
    }

    #[test]
    fn line_backspace_at_column_zero_joins_lines() {
        let mut editor = editor_with("ab\ncd", &[3]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Line), Instant::now());
        assert_eq!(text(&editor), "abcd");
    }

    #[test]
    fn select_all_then_type_replaces_everything() {
        let mut editor = editor_with("abc\ndef", &[0]);
        editor.apply(EditorAction::SelectAll, Instant::now());
        editor.apply(EditorAction::InsertText("z".into()), Instant::now());
        assert_eq!(text(&editor), "z");
    }

    #[test]
    fn set_selections_clamps_to_the_buffer() {
        let editor = editor_with("ab", &[99]);
        assert_eq!(heads(&editor), vec![2]);
    }

    #[test]
    fn tab_inserts_indent_at_a_cursor() {
        let mut editor = editor_with("ab", &[1]);
        editor.apply(EditorAction::Indent, Instant::now());
        assert_eq!(text(&editor), "a    b");
    }

    #[test]
    fn tab_with_a_multiline_selection_indents_each_line_and_shift_tab_reverts() {
        let mut editor = Editor::new(Buffer::from_text("a\nb\nc"));
        editor.set_selections(Selections::single(Selection::new(0, 3)));
        editor.apply(EditorAction::Indent, Instant::now());
        assert_eq!(text(&editor), "    a\n    b\nc");
        editor.apply(EditorAction::Outdent, Instant::now());
        assert_eq!(text(&editor), "a\nb\nc");
    }

    #[test]
    fn outdent_removes_a_leading_tab() {
        let mut editor = editor_with("\tx", &[2]);
        editor.apply(EditorAction::Outdent, Instant::now());
        assert_eq!(text(&editor), "x");
    }

    #[test]
    fn copy_joins_selections_and_ignores_empty_ones() {
        let mut editor = Editor::new(Buffer::from_text("one two three"));
        editor.set_selections(Selections::new(
            vec![Selection::new(0, 3), Selection::cursor(5), Selection::new(8, 13)],
            0,
        ));
        assert_eq!(editor.copy_text().as_deref(), Some("one\nthree"));
        editor.set_selections(Selections::single(Selection::cursor(1)));
        assert_eq!(editor.copy_text(), None);
    }

    #[test]
    fn cut_returns_the_text_and_removes_it() {
        let mut editor = Editor::new(Buffer::from_text("hello world"));
        editor.set_selections(Selections::single(Selection::new(5, 11)));
        let cut = editor.apply(EditorAction::Cut, Instant::now());
        assert_eq!(cut.as_deref(), Some(" world"));
        assert_eq!(text(&editor), "hello");
    }

    #[test]
    fn paste_distributes_lines_across_matching_cursors() {
        let mut editor = editor_with("a\nb", &[1, 3]);
        editor.paste("1\n2", Instant::now());
        assert_eq!(text(&editor), "a1\nb2");
    }

    #[test]
    fn paste_inserts_whole_text_when_counts_differ() {
        let mut editor = editor_with("ab", &[1]);
        editor.paste("x\ny", Instant::now());
        assert_eq!(text(&editor), "ax\nyb");
        assert_eq!(heads(&editor), vec![4]);
    }

    #[test]
    fn backspace_at_offset_zero_keeps_every_cursor() {
        let mut editor = editor_with("ab\ncd", &[0, 4]);
        editor.apply(EditorAction::DeleteBackward(Granularity::Char), Instant::now());
        assert_eq!(text(&editor), "ab\nd");
        assert_eq!(heads(&editor), vec![0, 3]);
    }

    #[test]
    fn cut_keeps_empty_cursors_in_place() {
        let mut editor = Editor::new(Buffer::from_text("one two"));
        editor.set_selections(Selections::new(vec![Selection::new(0, 3), Selection::cursor(6)], 0));
        let cut = editor.apply(EditorAction::Cut, Instant::now());
        assert_eq!(cut.as_deref(), Some("one"));
        assert_eq!(text(&editor), " two");
        assert_eq!(heads(&editor), vec![0, 3]);
    }

    #[test]
    fn typing_with_a_selection_then_a_cursor_keeps_newest_by_origin() {
        let mut editor = Editor::new(Buffer::from_text("abcdef"));
        editor.set_selections(Selections::new(vec![Selection::new(0, 3), Selection::cursor(3)], 0));
        editor.apply(EditorAction::InsertText("x".into()), Instant::now());
        assert_eq!(text(&editor), "xxdef");
        assert_eq!(heads(&editor), vec![1, 2]);
        assert_eq!(editor.selections().newest().head, 1);
    }

    #[test]
    fn empty_insert_does_not_mark_dirty_or_clear_redo() {
        let mut editor = editor_with("abc", &[1]);
        editor.apply(EditorAction::InsertText("x".into()), Instant::now());
        editor.apply(EditorAction::Undo, Instant::now());
        editor.mark_saved();
        editor.apply(EditorAction::InsertText("".into()), Instant::now());
        assert!(!editor.buffer().is_dirty());
        editor.apply(EditorAction::Redo, Instant::now());
        assert_eq!(text(&editor), "axbc");
        assert_eq!(heads(&editor), vec![2]);
    }

    #[test]
    fn paste_normalises_crlf_in_a_crlf_buffer() {
        let mut editor = Editor::new(Buffer::from_text("a\r\nb"));
        editor.set_selections(Selections::single(Selection::cursor(0)));
        editor.paste("x\r\ny", Instant::now());
        assert_eq!(editor.buffer().to_saved_text(), "x\r\nya\r\nb");
    }
}
