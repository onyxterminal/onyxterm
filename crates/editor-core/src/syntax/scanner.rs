use super::{Span, TokenKind};
use std::ops::Range;

pub(super) trait SpanSink {
    fn push(&mut self, range: Range<usize>, kind: TokenKind);
}

impl SpanSink for Vec<Span> {
    fn push(&mut self, range: Range<usize>, kind: TokenKind) {
        match self.last_mut() {
            Some(last) if last.kind == kind && last.range.end == range.start => last.range.end = range.end,
            _ => Vec::push(self, Span { range, kind }),
        }
    }
}

pub(super) struct DiscardSpans;

impl SpanSink for DiscardSpans {
    fn push(&mut self, _range: Range<usize>, _kind: TokenKind) {}
}

pub(super) struct Scanner<'a, S> {
    bytes: &'a [u8],
    pos: usize,
    sink: S,
}

impl<'a, S: SpanSink> Scanner<'a, S> {
    pub(super) fn new(line: &'a str, sink: S) -> Self {
        Self { bytes: line.as_bytes(), pos: 0, sink }
    }

    pub(super) fn into_sink(self) -> S {
        self.sink
    }

    pub(super) fn is_done(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub(super) fn pos(&self) -> usize {
        self.pos
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.peek_at(0)
    }

    pub(super) fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    pub(super) fn rest(&self) -> &'a [u8] {
        &self.bytes[self.pos.min(self.bytes.len())..]
    }

    pub(super) fn text_from(&self, start: usize) -> &'a [u8] {
        &self.bytes[start..self.pos]
    }

    pub(super) fn starts_with(&self, prefix: &[u8]) -> bool {
        self.rest().starts_with(prefix)
    }

    pub(super) fn advance(&mut self, count: usize) {
        self.pos = (self.pos + count).min(self.bytes.len());
    }

    pub(super) fn advance_while(&mut self, predicate: impl Fn(u8) -> bool) {
        let skipped = self.rest().iter().position(|byte| !predicate(*byte)).unwrap_or(self.rest().len());
        self.advance(skipped);
    }

    pub(super) fn skip_to_end(&mut self) {
        self.pos = self.bytes.len();
    }

    pub(super) fn skip_whitespace(&mut self) {
        self.advance_while(|byte| byte == b' ' || byte == b'\t');
    }

    pub(super) fn emit_from(&mut self, start: usize, kind: TokenKind) {
        if start < self.pos {
            self.sink.push(start..self.pos, kind);
        }
    }

    pub(super) fn emit_one(&mut self, kind: TokenKind) {
        let start = self.pos;
        self.advance(1);
        self.emit_from(start, kind);
    }
}

pub(super) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii()
}

pub(super) fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

pub(super) fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}
