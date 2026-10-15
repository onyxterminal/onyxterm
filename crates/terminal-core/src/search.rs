use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Point};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::Term;
use std::ops::Range;

const CASE_INSENSITIVE_FLAG: &str = "(?i)";
const REGEX_META_CHARACTERS: &str = "\\.+*?()|[]{}^$#&-~";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MatchPoint {
    pub line: i32,
    pub col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMatch {
    pub start: MatchPoint,
    pub end: MatchPoint,
}

impl TextMatch {
    pub fn columns_on(&self, line: i32, cols: u16) -> Option<Range<u16>> {
        if line < self.start.line || line > self.end.line {
            return None;
        }
        let first = if line == self.start.line { self.start.col } else { 0 };
        let last = if line == self.end.line { self.end.col.saturating_add(1).min(cols) } else { cols };
        (first < last).then_some(first..last)
    }

    fn from_match(found: Match) -> Self {
        let point = |point: &Point| MatchPoint { line: point.line.0, col: point.column.0 as u16 };
        Self { start: point(found.start()), end: point(found.end()) }
    }
}

pub fn literal_pattern(query: &str) -> String {
    let mut pattern = String::from(CASE_INSENSITIVE_FLAG);
    for ch in query.chars() {
        if REGEX_META_CHARACTERS.contains(ch) {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern
}

pub(crate) fn find_in<T>(term: &Term<T>, query: &str, limit: usize) -> Vec<TextMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let Ok(mut regex) = RegexSearch::new(&literal_pattern(query)) else {
        return Vec::new();
    };
    let grid = term.grid();
    let start = Point::new(grid.topmost_line(), Column(0));
    let end = Point::new(grid.bottommost_line(), grid.last_column());
    RegexIter::new(start, end, Direction::Right, term, &mut regex).take(limit).map(TextMatch::from_match).collect()
}

pub(crate) fn offset_revealing(line: i32, offset: usize, screen_lines: usize, history: usize) -> usize {
    let row = line + offset as i32;
    if row >= 0 && row < screen_lines as i32 {
        return offset;
    }
    let centered = screen_lines as i32 / 2 - line;
    centered.clamp(0, history as i32) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_patterns_escape_regex_syntax_and_ignore_case() {
        assert_eq!(literal_pattern("a.b"), "(?i)a\\.b");
        assert_eq!(literal_pattern("fn(x) [1]"), "(?i)fn\\(x\\) \\[1\\]");
    }

    #[test]
    fn match_columns_cover_wrapped_rows() {
        let found = TextMatch { start: MatchPoint { line: -1, col: 7 }, end: MatchPoint { line: 0, col: 2 } };
        assert_eq!(found.columns_on(-1, 10), Some(7..10));
        assert_eq!(found.columns_on(0, 10), Some(0..3));
        assert_eq!(found.columns_on(1, 10), None);
    }

    #[test]
    fn revealing_keeps_visible_lines_and_centers_history_lines() {
        assert_eq!(offset_revealing(3, 0, 24, 100), 0);
        assert_eq!(offset_revealing(-40, 0, 24, 100), 52);
        assert_eq!(offset_revealing(-200, 0, 24, 100), 100);
        assert_eq!(offset_revealing(10, 30, 24, 100), 2);
    }
}
