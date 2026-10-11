#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::keymap::physical_combo;
use gpui::{Bounds, Keystroke, Pixels, UTF16Selection};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

pub type CharFilter = fn(char) -> bool;
pub type CaretBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

pub trait Clipboard {
    fn read_text(&mut self) -> Option<String>;
    fn write_text(&mut self, text: String);
}

#[derive(Debug, Clone, Default)]
pub struct TextField {
    text: String,
    caret: usize,
    anchor: usize,
    marked: Option<Range<usize>>,
    filter: Option<CharFilter>,
    max_chars: Option<usize>,
    caret_bounds: CaretBounds,
}

impl PartialEq for TextField {
    fn eq(&self, other: &Self) -> bool {
        let same_text = self.text == other.text && self.marked == other.marked;
        same_text && self.caret == other.caret && self.anchor == other.anchor
    }
}

impl Eq for TextField {}

impl TextField {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self { text, caret: end, anchor: end, ..Self::default() }
    }

    pub fn with_filter(mut self, filter: CharFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = Some(max_chars);
        self
    }

    pub fn with_all_selected(mut self) -> Self {
        self.select_all();
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn caret_bounds(&self) -> &CaretBounds {
        &self.caret_bounds
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.anchor = 0;
        self.marked = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    pub fn insert(&mut self, typed: &str) {
        let range = self.marked.take().unwrap_or_else(|| self.selection());
        self.splice(range, typed);
    }

    pub fn delete_backward(&mut self) {
        self.delete_toward(previous_char(&self.text, self.caret));
    }

    pub fn copy(&self, clipboard: &mut dyn Clipboard) {
        let selection = self.selection();
        if !selection.is_empty() {
            clipboard.write_text(self.text[selection].to_string());
        }
    }

    pub fn cut(&mut self, clipboard: &mut dyn Clipboard) {
        self.copy(clipboard);
        let selection = self.selection();
        self.splice(selection, "");
    }

    pub fn paste(&mut self, clipboard: &mut dyn Clipboard) {
        if let Some(pasted) = clipboard.read_text() {
            self.insert(&pasted);
        }
    }

    pub fn handle_key(&mut self, keystroke: &Keystroke, clipboard: &mut dyn Clipboard) -> bool {
        let modifiers = keystroke.modifiers;
        if modifiers.control || (modifiers.platform && modifiers.alt) {
            return false;
        }
        let key = physical_combo(keystroke).map_or_else(|| keystroke.key.clone(), |combo| combo.key);
        match (key.as_str(), modifiers.platform) {
            ("left", _) => self.move_caret(self.left_target(keystroke), modifiers.shift),
            ("right", _) => self.move_caret(self.right_target(keystroke), modifiers.shift),
            ("home", _) => self.move_caret(0, modifiers.shift),
            ("end", _) => self.move_caret(self.text.len(), modifiers.shift),
            ("backspace", _) => self.delete_toward(self.backward_target(keystroke)),
            ("delete", _) => self.delete_toward(self.forward_target(keystroke)),
            ("space", false) => self.insert(" "),
            ("a", true) => self.select_all(),
            ("c", true) => self.copy(clipboard),
            ("x", true) => self.cut(clipboard),
            ("v", true) => self.paste(clipboard),
            _ => return false,
        }
        true
    }

    pub fn replace_utf16(&mut self, range: Option<Range<usize>>, typed: &str) {
        let range = self.resolve_utf16(range);
        self.marked = None;
        self.splice(range, typed);
    }

    pub fn mark_utf16(&mut self, range: Option<Range<usize>>, typed: &str, selected: Option<Range<usize>>) {
        let range = self.resolve_utf16(range);
        let inserted = self.splice(range, typed);
        self.marked = Some(inserted.clone()).filter(|marked| !marked.is_empty());
        let Some(selected) = selected else {
            return;
        };
        let inserted_text = &self.text[inserted.clone()];
        self.anchor = inserted.start + offset_from_utf16(inserted_text, selected.start);
        self.caret = inserted.start + offset_from_utf16(inserted_text, selected.end);
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

    pub fn text_for_utf16(&self, range: Range<usize>) -> (String, Range<usize>) {
        let range = self.offset_range(range);
        let text = self.text[range.clone()].to_string();
        (text, self.utf16_range(range))
    }

    pub fn selected_utf16(&self) -> UTF16Selection {
        UTF16Selection { range: self.utf16_range(self.selection()), reversed: self.caret < self.anchor }
    }

    pub fn marked_utf16(&self) -> Option<Range<usize>> {
        self.marked.clone().map(|marked| self.utf16_range(marked))
    }

    fn splice(&mut self, range: Range<usize>, typed: &str) -> Range<usize> {
        let accepted = self.accepted(typed, &range);
        self.text.replace_range(range.clone(), &accepted);
        let end = range.start + accepted.len();
        self.caret = end;
        self.anchor = end;
        range.start..end
    }

    fn accepted(&self, typed: &str, replaced: &Range<usize>) -> String {
        let kept = self.text.chars().count() - self.text[replaced.clone()].chars().count();
        let room = self.max_chars.map_or(usize::MAX, |max| max.saturating_sub(kept));
        typed
            .chars()
            .filter(|typed| !typed.is_control())
            .filter(|typed| self.filter.is_none_or(|filter| filter(*typed)))
            .take(room)
            .collect()
    }

    fn move_caret(&mut self, target: usize, is_extending: bool) {
        self.marked = None;
        self.caret = target;
        if !is_extending {
            self.anchor = target;
        }
    }

    fn delete_toward(&mut self, target: usize) {
        let selection = self.selection();
        if !selection.is_empty() {
            self.splice(selection, "");
            return;
        }
        let range = self.caret.min(target)..self.caret.max(target);
        self.splice(range, "");
    }

    fn left_target(&self, keystroke: &Keystroke) -> usize {
        let modifiers = keystroke.modifiers;
        let selection = self.selection();
        if modifiers.platform {
            return 0;
        }
        if modifiers.alt {
            return previous_word_start(&self.text, self.caret);
        }
        if !modifiers.shift && !selection.is_empty() {
            return selection.start;
        }
        previous_char(&self.text, self.caret)
    }

    fn right_target(&self, keystroke: &Keystroke) -> usize {
        let modifiers = keystroke.modifiers;
        let selection = self.selection();
        if modifiers.platform {
            return self.text.len();
        }
        if modifiers.alt {
            return next_word_end(&self.text, self.caret);
        }
        if !modifiers.shift && !selection.is_empty() {
            return selection.end;
        }
        next_char(&self.text, self.caret)
    }

    fn backward_target(&self, keystroke: &Keystroke) -> usize {
        let modifiers = keystroke.modifiers;
        if modifiers.platform {
            return 0;
        }
        if modifiers.alt {
            return previous_word_start(&self.text, self.caret);
        }
        previous_char(&self.text, self.caret)
    }

    fn forward_target(&self, keystroke: &Keystroke) -> usize {
        let modifiers = keystroke.modifiers;
        if modifiers.platform {
            return self.text.len();
        }
        if modifiers.alt {
            return next_word_end(&self.text, self.caret);
        }
        next_char(&self.text, self.caret)
    }

    fn resolve_utf16(&self, range: Option<Range<usize>>) -> Range<usize> {
        range
            .map(|range| self.offset_range(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection())
    }

    fn offset_range(&self, range: Range<usize>) -> Range<usize> {
        let start = offset_from_utf16(&self.text, range.start);
        start..offset_from_utf16(&self.text, range.end).max(start)
    }

    fn utf16_range(&self, range: Range<usize>) -> Range<usize> {
        utf16_len(&self.text[..range.start])..utf16_len(&self.text[..range.end])
    }
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn offset_from_utf16(text: &str, target: usize) -> usize {
    let mut units = 0;
    for (offset, typed) in text.char_indices() {
        if units >= target {
            return offset;
        }
        units += typed.len_utf16();
    }
    text.len()
}

fn previous_char(text: &str, offset: usize) -> usize {
    text[..offset].char_indices().next_back().map_or(0, |(index, _)| index)
}

fn next_char(text: &str, offset: usize) -> usize {
    text[offset..].chars().next().map_or(text.len(), |typed| offset + typed.len_utf8())
}

fn is_word_char(typed: char) -> bool {
    typed.is_alphanumeric() || typed == '_'
}

fn previous_word_start(text: &str, offset: usize) -> usize {
    let before = &text[..offset];
    let trimmed = before.trim_end_matches(|typed: char| !is_word_char(typed));
    trimmed.trim_end_matches(is_word_char).len()
}

fn next_word_end(text: &str, offset: usize) -> usize {
    let after = &text[offset..];
    let trimmed = after.trim_start_matches(|typed: char| !is_word_char(typed));
    let word = trimmed.len() - trimmed.trim_start_matches(is_word_char).len();
    text.len() - trimmed.len() + word
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::custom::{hex_color_field, normalized_hex};

    #[derive(Default)]
    struct FakeClipboard(Option<String>);

    impl Clipboard for FakeClipboard {
        fn read_text(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn write_text(&mut self, text: String) {
            self.0 = Some(text);
        }
    }

    fn press(field: &mut TextField, keys: &str) -> bool {
        let mut clipboard = FakeClipboard::default();
        press_with(field, keys, &mut clipboard)
    }

    fn press_with(field: &mut TextField, keys: &str, clipboard: &mut FakeClipboard) -> bool {
        let keystroke = Keystroke::parse(keys).expect("keystroke");
        field.handle_key(&keystroke, clipboard)
    }

    #[test]
    fn caret_moves_over_multibyte_characters() {
        let mut field = TextField::new("aé😀b");
        press(&mut field, "left");
        assert_eq!(field.caret(), "aé😀".len());
        press(&mut field, "left");
        assert_eq!(field.caret(), "aé".len());
        press(&mut field, "left");
        assert_eq!(field.caret(), 1);
        press(&mut field, "backspace");
        assert_eq!(field.text(), "é😀b");
        press(&mut field, "delete");
        assert_eq!(field.text(), "😀b");
        press(&mut field, "end");
        field.insert("ü");
        assert_eq!(field.text(), "😀bü");
    }

    #[test]
    fn typing_replaces_the_selection() {
        let mut field = TextField::new("hello world");
        press(&mut field, "shift-left");
        press(&mut field, "shift-left");
        assert_eq!(field.selection(), 9..11);
        field.insert("X");
        assert_eq!(field.text(), "hello worX");
        press(&mut field, "cmd-a");
        field.insert("new");
        assert_eq!(field.text(), "new");
        assert_eq!(field.selection(), 3..3);
    }

    #[test]
    fn plain_arrow_collapses_selection_to_its_edge() {
        let mut field = TextField::new("abcdef").with_all_selected();
        press(&mut field, "left");
        assert_eq!(field.selection(), 0..0);
        press(&mut field, "shift-cmd-right");
        assert_eq!(field.selection(), 0..6);
        press(&mut field, "right");
        assert_eq!(field.selection(), 6..6);
    }

    #[test]
    fn word_and_line_operations() {
        let mut field = TextField::new("open the  pod_bay doors");
        press(&mut field, "alt-left");
        assert_eq!(field.caret(), "open the  pod_bay ".len());
        press(&mut field, "alt-left");
        assert_eq!(field.caret(), "open the  ".len());
        press(&mut field, "alt-right");
        assert_eq!(field.caret(), "open the  pod_bay".len());
        press(&mut field, "alt-backspace");
        assert_eq!(field.text(), "open the   doors");
        press(&mut field, "cmd-backspace");
        assert_eq!(field.text(), " doors");
        press(&mut field, "alt-delete");
        assert_eq!(field.text(), "");
        let mut field = TextField::new("abc");
        press(&mut field, "home");
        assert_eq!(field.caret(), 0);
        press(&mut field, "cmd-right");
        assert_eq!(field.caret(), 3);
        press(&mut field, "cmd-left");
        assert_eq!(field.caret(), 0);
    }

    #[test]
    fn clipboard_copy_cut_and_filtered_paste() {
        let mut clipboard = FakeClipboard::default();
        let mut field = TextField::new("alpha beta");
        press_with(&mut field, "alt-shift-left", &mut clipboard);
        press_with(&mut field, "cmd-c", &mut clipboard);
        assert_eq!(clipboard.0.as_deref(), Some("beta"));
        press_with(&mut field, "cmd-x", &mut clipboard);
        assert_eq!(field.text(), "alpha ");
        clipboard.0 = Some("one\ntwo\tthree\u{7}".into());
        press_with(&mut field, "cmd-v", &mut clipboard);
        assert_eq!(field.text(), "alpha onetwothree");
    }

    #[test]
    fn control_and_unknown_keys_are_not_text() {
        let mut field = TextField::new("abc");
        assert!(!press(&mut field, "tab"));
        assert!(!press(&mut field, "enter"));
        assert!(!press(&mut field, "escape"));
        assert!(!press(&mut field, "ctrl-a"));
        assert!(press(&mut field, "space"));
        field.insert("\t\n\r");
        assert_eq!(field.text(), "abc ");
    }

    #[test]
    fn filter_and_max_length_limit_insertions() {
        let mut field = TextField::default().with_filter(|typed| typed.is_ascii_digit()).with_max_chars(4);
        field.insert("1a2b3");
        assert_eq!(field.text(), "123");
        field.insert("456");
        assert_eq!(field.text(), "1234");
        press(&mut field, "shift-left");
        field.insert("98");
        assert_eq!(field.text(), "1239");
    }

    #[test]
    fn hex_field_starts_selected_and_normalizes_the_prefix() {
        let mut field = hex_color_field("#1A2B3C");
        assert_eq!(field.selection(), 0..7);
        field.insert("ff00zz00");
        assert_eq!(field.text(), "ff0000");
        assert_eq!(normalized_hex(field.text()), "#ff0000");
        press(&mut field, "cmd-a");
        field.insert("#abcdef12");
        assert_eq!(field.text(), "#abcdef");
        assert_eq!(normalized_hex(field.text()), "#abcdef");
    }

    #[test]
    fn ime_marks_and_commits_in_utf16_units() {
        let mut field = TextField::new("😀a");
        assert_eq!(field.selected_utf16().range, 3..3);
        field.mark_utf16(None, "´", Some(1..1));
        assert_eq!(field.text(), "😀a´");
        assert_eq!(field.marked_utf16(), Some(3..4));
        field.replace_utf16(None, "é");
        assert_eq!(field.text(), "😀aé");
        assert_eq!(field.marked_range(), None);
        assert_eq!(field.text_for_utf16(0..2), ("😀".to_string(), 0..2));
        field.replace_utf16(Some(2..3), "b");
        assert_eq!(field.text(), "😀bé");
    }
}
