use ropey::Rope;
use std::ops::Range;

pub(crate) const CRLF: &str = "\r\n";
pub(crate) const LF: &str = "\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

#[derive(Debug, Clone)]
pub struct Buffer {
    rope: Rope,
    saved_rope: Rope,
    version: u64,
    saved_version: u64,
    line_ending: LineEnding,
}

impl Buffer {
    pub fn from_text(text: &str) -> Self {
        let line_ending = if text.contains(CRLF) { LineEnding::CrLf } else { LineEnding::Lf };
        let rope = Rope::from_str(&text.replace(CRLF, LF));
        Self { saved_rope: rope.clone(), rope, version: 0, saved_version: 0, line_ending }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn saved_rope(&self) -> &Rope {
        &self.saved_rope
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn saved_version(&self) -> u64 {
        self.saved_version
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
        self.saved_rope = self.rope.clone();
    }

    pub(crate) fn mark_saved_at(&mut self, version: u64, saved_rope: Rope) {
        self.saved_version = version;
        self.saved_rope = saved_rope;
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn to_saved_text(&self) -> String {
        let text = self.rope.to_string();
        match self.line_ending {
            LineEnding::Lf => text,
            LineEnding::CrLf => text.replace(LF, CRLF),
        }
    }

    pub(crate) fn replace(&mut self, range: Range<usize>, text: &str) {
        self.rope.remove(range.clone());
        self.rope.insert(range.start, text);
        self.version += 1;
    }
}

pub fn line_len(rope: &Rope, line: usize) -> usize {
    let slice = rope.line(line);
    let len = slice.len_chars();
    if len > 0 && slice.char(len - 1) == '\n' {
        len - 1
    } else {
        len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_text_is_normalized_and_restored_on_save() {
        let buffer = Buffer::from_text("a\r\nb\r\n");
        assert_eq!(buffer.line_ending(), LineEnding::CrLf);
        assert_eq!(buffer.rope().to_string(), "a\nb\n");
        assert_eq!(buffer.to_saved_text(), "a\r\nb\r\n");
    }

    #[test]
    fn lf_text_stays_lf() {
        let buffer = Buffer::from_text("a\nb");
        assert_eq!(buffer.line_ending(), LineEnding::Lf);
        assert_eq!(buffer.to_saved_text(), "a\nb");
    }

    #[test]
    fn replace_marks_dirty_until_saved() {
        let mut buffer = Buffer::from_text("abc");
        assert!(!buffer.is_dirty());
        buffer.replace(1..2, "XY");
        assert_eq!(buffer.rope().to_string(), "aXYc");
        assert!(buffer.is_dirty());
        buffer.mark_saved();
        assert!(!buffer.is_dirty());
        assert_eq!(buffer.saved_rope().to_string(), "aXYc");
    }

    #[test]
    fn saved_text_starts_as_the_loaded_text() {
        let mut buffer = Buffer::from_text("a\r\nb");
        buffer.replace(0..1, "z");
        assert_eq!(buffer.saved_rope().to_string(), "a\nb");
    }

    #[test]
    fn line_len_excludes_the_newline() {
        let rope = Rope::from_str("ab\ncde");
        assert_eq!(line_len(&rope, 0), 2);
        assert_eq!(line_len(&rope, 1), 3);
    }

    #[test]
    fn only_lf_breaks_lines() {
        let buffer = Buffer::from_text("a\rb\u{2028}c\nd");
        assert_eq!(buffer.line_count(), 2);
        assert_eq!(line_len(buffer.rope(), 0), 5);
    }
}
