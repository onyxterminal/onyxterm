use super::{Language, LineState, Span};
use crate::rope_compare::common_prefix_bytes;
use ropey::{Rope, RopeSlice};
use std::borrow::Cow;

const MAX_LEXED_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
pub struct HighlightSource<'a> {
    pub rope: &'a Rope,
    pub version: u64,
    pub language: Option<Language>,
}

#[derive(Debug, Default)]
pub struct LineStates {
    language: Option<Language>,
    version: u64,
    rope: Rope,
    starts: Vec<LineState>,
}

impl LineStates {
    pub fn update(&mut self, source: HighlightSource<'_>, through_line: usize) {
        self.invalidate_changed_lines(source);
        self.extend_through(through_line);
    }

    pub fn highlight(&self, line: usize) -> Vec<Span> {
        let (Some(language), Some(state)) = (self.language, self.starts.get(line)) else {
            return Vec::new();
        };
        match lexable_text(self.rope.line(line)) {
            Some(text) => language.highlight_line(&text, *state),
            None => Vec::new(),
        }
    }

    fn invalidate_changed_lines(&mut self, source: HighlightSource<'_>) {
        if self.language != source.language || self.starts.is_empty() {
            self.starts = vec![LineState::Normal];
        } else if self.version != source.version {
            let first_changed = source.rope.byte_to_line(common_prefix_bytes(&self.rope, source.rope));
            self.starts.truncate(first_changed + 1);
        }
        self.language = source.language;
        self.version = source.version;
        self.rope = source.rope.clone();
    }

    fn extend_through(&mut self, through_line: usize) {
        let Some(language) = self.language else {
            return;
        };
        let last_line = through_line.min(self.rope.len_lines().saturating_sub(1));
        let first_unlexed = self.starts.len() - 1;
        let unlexed = (last_line + 1).saturating_sub(self.starts.len());
        let mut state = self.starts[first_unlexed];
        for slice in self.rope.lines_at(first_unlexed).take(unlexed) {
            if let Some(text) = lexable_text(slice) {
                state = language.end_state(&text, state);
            }
            self.starts.push(state);
        }
    }
}

fn lexable_text(slice: RopeSlice<'_>) -> Option<Cow<'_, str>> {
    if slice.len_bytes() > MAX_LEXED_LINE_BYTES {
        return None;
    }
    let text = match slice.as_str() {
        Some(text) => Cow::Borrowed(text.strip_suffix('\n').unwrap_or(text)),
        None => Cow::Owned(slice.to_string().trim_end_matches('\n').to_string()),
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::TokenKind;

    fn source(rope: &Rope, version: u64) -> HighlightSource<'_> {
        HighlightSource { rope, version, language: Some(Language::Rust) }
    }

    #[test]
    fn states_follow_multi_line_comments_to_visible_lines() {
        let rope = Rope::from_str("let a = 1;\n/* open\nstill\n*/ fn after() {}\n");
        let mut states = LineStates::default();
        states.update(source(&rope, 0), 3);
        assert_eq!(states.highlight(2), vec![Span { range: 0..5, kind: TokenKind::Comment }]);
        assert!(states.highlight(3).iter().any(|span| span.kind == TokenKind::Function));
        assert_eq!(states.starts.len(), 4);
    }

    #[test]
    fn states_are_computed_only_through_the_requested_line() {
        let rope = Rope::from_str(&"let x = 1;\n".repeat(1000));
        let mut states = LineStates::default();
        states.update(source(&rope, 0), 40);
        assert_eq!(states.starts.len(), 41);
    }

    #[test]
    fn an_edit_keeps_states_before_the_changed_line() {
        let mut rope = Rope::from_str(&"let x = 1;\n".repeat(100));
        let mut states = LineStates::default();
        states.update(source(&rope, 0), 99);
        rope.insert(rope.line_to_char(60), "/*");
        states.invalidate_changed_lines(source(&rope, 1));
        assert_eq!(states.starts.len(), 61);
        states.extend_through(99);
        assert_eq!(states.starts[61], LineState::BlockComment(1));
        assert_eq!(states.highlight(80), vec![Span { range: 0..10, kind: TokenKind::Comment }]);
    }

    #[test]
    fn an_unchanged_version_reuses_states() {
        let rope = Rope::from_str("/* a\nb\n");
        let mut states = LineStates::default();
        states.update(source(&rope, 7), 1);
        let edited = Rope::from_str("plain\nb\n");
        states.update(source(&edited, 7), 1);
        assert_eq!(states.starts[1], LineState::BlockComment(1));
    }

    #[test]
    fn a_language_change_recomputes_from_the_start() {
        let rope = Rope::from_str("/* a\n# b\n");
        let mut states = LineStates::default();
        states.update(source(&rope, 0), 1);
        states.update(HighlightSource { rope: &rope, version: 0, language: Some(Language::Markdown) }, 1);
        assert_eq!(states.highlight(1), vec![Span { range: 0..3, kind: TokenKind::Heading }]);
        states.update(HighlightSource { rope: &rope, version: 0, language: None }, 1);
        assert!(states.highlight(0).is_empty());
    }

    #[test]
    fn overlong_lines_are_left_plain_and_keep_the_incoming_state() {
        let long = format!("/* {}\n", "x".repeat(MAX_LEXED_LINE_BYTES));
        let rope = Rope::from_str(&format!("{long}fn next() {{}}\n"));
        let mut states = LineStates::default();
        states.update(source(&rope, 0), 1);
        assert!(states.highlight(0).is_empty());
        assert_eq!(states.starts[1], LineState::Normal);
    }
}
