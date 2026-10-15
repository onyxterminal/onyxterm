use crate::edit::Change;
use ropey::Rope;

const COMMENT_GAP: char = ' ';

struct CommentLine {
    indent_end: usize,
    marker_len: Option<usize>,
    is_blank: bool,
}

pub(crate) fn toggle_comment_changes(rope: &Rope, lines: &[usize], token: &str) -> Vec<Option<Change>> {
    let parsed: Vec<CommentLine> = lines.iter().map(|line| parse_line(rope, *line, token)).collect();
    let skips_blank = lines.len() > 1;
    let considered = || parsed.iter().filter(move |line| !(skips_blank && line.is_blank));
    let is_commented = considered().next().is_some() && considered().all(|line| line.marker_len.is_some());
    parsed
        .iter()
        .map(|line| {
            if skips_blank && line.is_blank {
                return None;
            }
            match (is_commented, line.marker_len) {
                (true, Some(len)) => Some(Change { range: line.indent_end..line.indent_end + len, text: String::new() }),
                _ => Some(Change {
                    range: line.indent_end..line.indent_end,
                    text: format!("{token}{COMMENT_GAP}"),
                }),
            }
        })
        .collect()
}

fn parse_line(rope: &Rope, line: usize, token: &str) -> CommentLine {
    let start = rope.line_to_char(line);
    let text: String = rope.line(line).chars().take_while(|ch| *ch != '\n').collect();
    let indent = text.chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
    let rest: String = text.chars().skip(indent).collect();
    let marker_len = rest.strip_prefix(token).map(|after| {
        let gap = usize::from(after.starts_with(COMMENT_GAP));
        token.chars().count() + gap
    });
    CommentLine { indent_end: start + indent, marker_len, is_blank: rest.is_empty() }
}
