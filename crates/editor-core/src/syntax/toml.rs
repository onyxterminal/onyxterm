use super::scanner::{Scanner, SpanSink};
use super::{LineState, TokenKind};

const PUNCTUATION: &[u8] = b"=[]{},.";
const TRIPLE_DOUBLE: &[u8] = b"\"\"\"";
const TRIPLE_SINGLE: &[u8] = b"'''";
const CONSTANTS: [&[u8]; 2] = [b"true", b"false"];
const SPECIAL_FLOATS: [&[u8]; 2] = [b"inf", b"nan"];

pub(super) fn lex<S: SpanSink>(scanner: &mut Scanner<S>, state: LineState) -> LineState {
    let mut state = match state {
        LineState::TripleQuoted(quote) => triple_quoted(scanner, quote),
        _ => line_start(scanner),
    };
    while state == LineState::Normal && !scanner.is_done() {
        state = value(scanner);
        if let LineState::TripleQuoted(quote) = state {
            state = triple_quoted(scanner, quote);
        }
    }
    state
}

fn line_start<S: SpanSink>(scanner: &mut Scanner<S>) -> LineState {
    scanner.skip_whitespace();
    match scanner.peek() {
        Some(b'[') => table_header(scanner),
        Some(_) => key(scanner),
        None => {}
    }
    LineState::Normal
}

fn table_header<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    let Some(length) = header_length(scanner.rest()) else {
        return;
    };
    scanner.advance(length);
    scanner.emit_from(start, TokenKind::Type);
}

fn header_length(rest: &[u8]) -> Option<usize> {
    let brackets = if rest.starts_with(b"[[") { 2 } else { 1 };
    let name_length = rest[brackets..].iter().position(|byte| *byte == b']')?;
    let name = &rest[brackets..brackets + name_length];
    if name.is_empty() || !name.iter().all(|byte| is_key_byte(*byte)) {
        return None;
    }
    let closing = &rest[brackets + name_length..];
    closing.starts_with(&b"]]"[..brackets]).then_some(brackets * 2 + name_length)
}

fn key<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    let Some(length) = key_length(scanner.rest()) else {
        return;
    };
    scanner.advance(length);
    scanner.emit_from(start, TokenKind::Function);
}

fn key_length(rest: &[u8]) -> Option<usize> {
    let equals = rest.iter().position(|byte| *byte == b'=')?;
    let key = &rest[..equals];
    let trimmed = key.iter().rposition(|byte| !byte.is_ascii_whitespace())? + 1;
    key.iter().all(|byte| is_key_byte(*byte)).then_some(trimmed)
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b' ' | b'\t' | b'"' | b'\'') || !byte.is_ascii()
}

fn value<S: SpanSink>(scanner: &mut Scanner<S>) -> LineState {
    let Some(byte) = scanner.peek() else {
        return LineState::Normal;
    };
    let start = scanner.pos();
    match byte {
        b'#' => {
            scanner.skip_to_end();
            scanner.emit_from(start, TokenKind::Comment);
        }
        b'"' | b'\'' if scanner.starts_with(&[byte; 3]) => {
            scanner.advance(3);
            scanner.emit_from(start, TokenKind::String);
            return LineState::TripleQuoted(byte);
        }
        b'"' => basic_string(scanner),
        b'\'' => literal_string(scanner),
        b'+' | b'-' if scanner.peek_at(1).is_some_and(|next| next.is_ascii_digit()) => number(scanner),
        b'0'..=b'9' => number(scanner),
        byte if byte.is_ascii_alphabetic() => bare_word(scanner),
        byte if PUNCTUATION.contains(&byte) => scanner.emit_one(TokenKind::Punctuation),
        _ => scanner.advance(1),
    }
    LineState::Normal
}

fn basic_string<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    scanner.advance(1);
    while let Some(byte) = scanner.peek() {
        match byte {
            b'\\' => scanner.advance(2),
            b'"' => {
                scanner.advance(1);
                break;
            }
            _ => scanner.advance(1),
        }
    }
    scanner.emit_from(start, TokenKind::String);
}

fn literal_string<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    scanner.advance(1);
    scanner.advance_while(|byte| byte != b'\'');
    scanner.advance(1);
    scanner.emit_from(start, TokenKind::String);
}

fn number<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    scanner.advance(1);
    scanner.advance_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-' | b'+'));
    scanner.emit_from(start, TokenKind::Number);
}

fn bare_word<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    scanner.advance_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    let word = scanner.text_from(start);
    if CONSTANTS.contains(&word) {
        scanner.emit_from(start, TokenKind::Constant);
    } else if SPECIAL_FLOATS.contains(&word) {
        scanner.emit_from(start, TokenKind::Number);
    }
}

fn triple_quoted<S: SpanSink>(scanner: &mut Scanner<S>, quote: u8) -> LineState {
    let start = scanner.pos();
    let closing = if quote == b'"' { TRIPLE_DOUBLE } else { TRIPLE_SINGLE };
    while !scanner.is_done() {
        if quote == b'"' && scanner.peek() == Some(b'\\') {
            scanner.advance(2);
        } else if scanner.starts_with(closing) {
            scanner.advance(closing.len());
            scanner.advance_while(|byte| byte == quote);
            scanner.emit_from(start, TokenKind::String);
            return LineState::Normal;
        } else {
            scanner.advance(1);
        }
    }
    scanner.emit_from(start, TokenKind::String);
    LineState::TripleQuoted(quote)
}
