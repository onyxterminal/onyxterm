use super::scanner::{Scanner, SpanSink};
use super::TokenKind;

const MAX_HEADING_INDENT: usize = 3;
const MAX_HEADING_LEVEL: usize = 6;

pub(super) fn lex<S: SpanSink>(scanner: &mut Scanner<S>) {
    if is_heading(scanner.rest()) {
        scanner.skip_to_end();
        scanner.emit_from(0, TokenKind::Heading);
        return;
    }
    while !scanner.is_done() {
        scanner.advance_while(|byte| byte != b'`');
        code_span(scanner);
    }
}

fn is_heading(line: &[u8]) -> bool {
    let indent = line.iter().take_while(|byte| **byte == b' ').count();
    let level = line[indent..].iter().take_while(|byte| **byte == b'#').count();
    let after = line.get(indent + level);
    indent <= MAX_HEADING_INDENT
        && (1..=MAX_HEADING_LEVEL).contains(&level)
        && after.is_none_or(|byte| *byte == b' ' || *byte == b'\t')
}

fn code_span<S: SpanSink>(scanner: &mut Scanner<S>) {
    let start = scanner.pos();
    let fence = scanner.rest().iter().take_while(|byte| **byte == b'`').count();
    scanner.advance(fence);
    let Some(length) = closing_fence_end(scanner.rest(), fence) else {
        return;
    };
    scanner.advance(length);
    scanner.emit_from(start, TokenKind::String);
}

fn closing_fence_end(rest: &[u8], fence: usize) -> Option<usize> {
    let mut offset = 0;
    while offset < rest.len() {
        let run = rest[offset..].iter().take_while(|byte| **byte == b'`').count();
        if run == fence {
            return Some(offset + run);
        }
        offset += run.max(1);
    }
    None
}
