use super::scanner::{is_ident_continue, is_ident_start, utf8_len, Scanner, SpanSink};
use super::{LineState, TokenKind};

const KEYWORDS: [&str; 36] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self",
    "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
];
const CONSTANTS: [&str; 6] = ["true", "false", "None", "Some", "Ok", "Err"];
const PUNCTUATION: &[u8] = b"{}[]()<>;:,.=&|?-";
const RAW_PREFIXES: [&str; 2] = ["r", "br"];
const BYTE_STRING_PREFIX: &str = "b";

pub(super) fn lex<S: SpanSink>(scanner: &mut Scanner<S>, state: LineState) -> LineState {
    let mut state = state;
    loop {
        state = match state {
            LineState::BlockComment(depth) => block_comment(scanner, depth),
            LineState::String => string(scanner),
            LineState::RawString(hashes) => raw_string(scanner, hashes),
            LineState::Normal | LineState::TripleQuoted(_) => normal(scanner),
        };
        if scanner.is_done() {
            return state;
        }
    }
}

fn normal<S: SpanSink>(scanner: &mut Scanner<S>) -> LineState {
    while let Some(byte) = scanner.peek() {
        let start = scanner.pos();
        match byte {
            b' ' | b'\t' => scanner.skip_whitespace(),
            b'/' if scanner.peek_at(1) == Some(b'/') => {
                scanner.skip_to_end();
                scanner.emit_from(start, TokenKind::Comment);
            }
            b'/' if scanner.peek_at(1) == Some(b'*') => {
                scanner.advance(2);
                scanner.emit_from(start, TokenKind::Comment);
                return LineState::BlockComment(1);
            }
            b'"' => {
                scanner.emit_one(TokenKind::String);
                return LineState::String;
            }
            b'\'' => char_or_lifetime(scanner),
            b'0'..=b'9' => number(scanner),
            byte if is_ident_start(byte) => {
                if let Some(next) = word(scanner) {
                    return next;
                }
            }
            byte if PUNCTUATION.contains(&byte) => scanner.emit_one(TokenKind::Punctuation),
            _ => scanner.advance(1),
        }
    }
    LineState::Normal
}

fn word<S: SpanSink>(scanner: &mut Scanner<S>) -> Option<LineState> {
    let start = scanner.pos();
    scanner.advance_while(is_ident_continue);
    let word = std::str::from_utf8(scanner.text_from(start)).unwrap_or_default();
    if let Some(hashes) = raw_string_opening(scanner, word) {
        scanner.advance(hashes as usize + 1);
        scanner.emit_from(start, TokenKind::String);
        return Some(LineState::RawString(hashes));
    }
    if word == BYTE_STRING_PREFIX && scanner.peek() == Some(b'"') {
        scanner.advance(1);
        scanner.emit_from(start, TokenKind::String);
        return Some(LineState::String);
    }
    if let Some(kind) = classify_word(scanner, word) {
        scanner.emit_from(start, kind);
    }
    None
}

fn raw_string_opening<S: SpanSink>(scanner: &Scanner<S>, word: &str) -> Option<u8> {
    if !RAW_PREFIXES.contains(&word) {
        return None;
    }
    let hashes = scanner.rest().iter().take_while(|byte| **byte == b'#').count();
    let opens = scanner.peek_at(hashes) == Some(b'"');
    opens.then(|| u8::try_from(hashes).unwrap_or(u8::MAX))
}

fn classify_word<S: SpanSink>(scanner: &mut Scanner<S>, word: &str) -> Option<TokenKind> {
    if scanner.peek() == Some(b'!') && scanner.peek_at(1) != Some(b'=') {
        scanner.advance(1);
        return Some(TokenKind::Macro);
    }
    if CONSTANTS.contains(&word) {
        return Some(TokenKind::Constant);
    }
    if KEYWORDS.contains(&word) {
        return Some(TokenKind::Keyword);
    }
    if scanner.peek() == Some(b'(') {
        return Some(TokenKind::Function);
    }
    word.starts_with(char::is_uppercase).then_some(TokenKind::Type)
}

fn char_or_lifetime<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    if scanner.peek_at(1) == Some(b'\\') {
        scanner.advance(3);
        scanner.advance_while(|byte| byte != b'\'');
        scanner.advance(1);
        scanner.emit_from(start, TokenKind::String);
        return;
    }
    let char_len = scanner.peek_at(1).map(utf8_len).unwrap_or(1);
    if scanner.peek_at(1 + char_len) == Some(b'\'') {
        scanner.advance(2 + char_len);
        scanner.emit_from(start, TokenKind::String);
        return;
    }
    scanner.advance(1);
    scanner.advance_while(is_ident_continue);
}

fn number<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    scanner.advance_while(is_ident_continue);
    if scanner.peek() == Some(b'.') && scanner.peek_at(1).is_some_and(|byte| byte.is_ascii_digit()) {
        scanner.advance(1);
        scanner.advance_while(is_ident_continue);
    }
    scanner.emit_from(start, TokenKind::Number);
}

fn string<S: SpanSink>(scanner: &mut Scanner<S>) -> LineState {
    let start = scanner.pos();
    loop {
        scanner.advance_while(|byte| byte != b'"' && byte != b'\\');
        match scanner.peek() {
            Some(b'\\') => scanner.advance(2),
            Some(_) => {
                scanner.advance(1);
                scanner.emit_from(start, TokenKind::String);
                return LineState::Normal;
            }
            None => {
                scanner.emit_from(start, TokenKind::String);
                return LineState::String;
            }
        }
    }
}

fn raw_string<S: SpanSink>(scanner: &mut Scanner<S>, hashes: u8) -> LineState {
    let start = scanner.pos();
    let closing_hashes = usize::from(hashes);
    while !scanner.is_done() {
        scanner.advance_while(|byte| byte != b'"');
        scanner.advance(1);
        let closes = scanner.rest().iter().take(closing_hashes).filter(|byte| **byte == b'#').count() == closing_hashes;
        if closes && scanner.text_from(start).ends_with(b"\"") {
            scanner.advance(closing_hashes);
            scanner.emit_from(start, TokenKind::String);
            return LineState::Normal;
        }
    }
    scanner.emit_from(start, TokenKind::String);
    LineState::RawString(hashes)
}

fn block_comment<S: SpanSink>(scanner: &mut Scanner<S>, depth: u32) -> LineState {
    let start = scanner.pos();
    let mut depth = depth;
    while !scanner.is_done() {
        scanner.advance_while(|byte| byte != b'*' && byte != b'/');
        if scanner.starts_with(b"*/") {
            scanner.advance(2);
            depth -= 1;
            if depth == 0 {
                scanner.emit_from(start, TokenKind::Comment);
                return LineState::Normal;
            }
        } else if scanner.starts_with(b"/*") {
            scanner.advance(2);
            depth += 1;
        } else {
            scanner.advance(1);
        }
    }
    scanner.emit_from(start, TokenKind::Comment);
    LineState::BlockComment(depth)
}
