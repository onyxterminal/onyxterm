use std::ops::Range;

const SEPARATOR: char = ':';
const EXTENSION_DOT: char = '.';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLink {
    pub columns: Range<usize>,
    pub path: String,
    pub line: u32,
    pub column: Option<u32>,
}

pub fn may_contain_link(chars: impl Iterator<Item = char>) -> bool {
    let mut previous = ' ';
    for ch in chars {
        if previous == SEPARATOR && ch.is_ascii_digit() {
            return true;
        }
        previous = ch;
    }
    false
}

pub fn file_links(chars: &[char]) -> Vec<FileLink> {
    let mut links = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match link_at_separator(chars, index) {
            Some(link) => {
                index = link.columns.end;
                links.push(link);
            }
            None => index += 1,
        }
    }
    links
}

fn link_at_separator(chars: &[char], separator: usize) -> Option<FileLink> {
    if chars[separator] != SEPARATOR {
        return None;
    }
    let start = path_start(chars, separator);
    let path = &chars[start..separator];
    if !has_extension(path) || (start > 0 && chars[start - 1] == SEPARATOR) {
        return None;
    }
    let (line, after_line) = number_at(chars, separator + 1)?;
    let column = chars.get(after_line).filter(|ch| **ch == SEPARATOR).and_then(|_| number_at(chars, after_line + 1));
    let end = column.map_or(after_line, |(_, after_column)| after_column);
    Some(FileLink {
        columns: start..end,
        path: path.iter().collect(),
        line,
        column: column.map(|(value, _)| value),
    })
}

fn is_path_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '/' | '.' | '-')
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn path_start(chars: &[char], end: usize) -> usize {
    let mut start = end;
    while start > 0 && is_path_char(chars[start - 1]) {
        start -= 1;
    }
    start
}

fn has_extension(path: &[char]) -> bool {
    let Some(dot) = path.iter().rposition(|ch| *ch == EXTENSION_DOT) else {
        return false;
    };
    let extension = &path[dot + 1..];
    dot > 0 && !extension.is_empty() && extension.iter().all(|ch| is_word_char(*ch))
}

fn number_at(chars: &[char], start: usize) -> Option<(u32, usize)> {
    let digits = chars[start.min(chars.len())..].iter().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let text: String = chars[start..start + digits].iter().collect();
    Some((text.parse().ok()?, start + digits))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<FileLink> {
        file_links(&text.chars().collect::<Vec<_>>())
    }

    fn link(columns: Range<usize>, path: &str, line: u32, column: Option<u32>) -> FileLink {
        FileLink { columns, path: path.into(), line, column }
    }

    #[test]
    fn finds_rust_panic_locations_with_line_and_column() {
        let expected = vec![link(5..35, "crates/ui/src/settings.rs", 15, Some(9))];
        assert_eq!(links("  at crates/ui/src/settings.rs:15:9"), expected);
    }

    #[test]
    fn finds_path_and_line_without_column() {
        assert_eq!(links("error in src/main.rs:42, see"), vec![link(9..23, "src/main.rs", 42, None)]);
        assert_eq!(links("./lib.ts:7:"), vec![link(0..10, "./lib.ts", 7, None)]);
    }

    #[test]
    fn finds_several_links_on_one_row() {
        let found = links("a.rs:1 b/c.py:22:3");
        assert_eq!(found, vec![link(0..6, "a.rs", 1, None), link(7..18, "b/c.py", 22, Some(3))]);
    }

    #[test]
    fn ignores_text_that_is_not_a_file_location() {
        assert!(links("12:04:11 pty spawned").is_empty());
        assert!(links("http://example.com:8080/path").is_empty());
        assert!(links("Makefile:12").is_empty());
        assert!(links("version.:3").is_empty());
        assert!(links("src/main.rs: no line").is_empty());
        assert!(links("").is_empty());
    }

    #[test]
    fn quick_check_spots_rows_worth_scanning() {
        assert!(may_contain_link("x src/a.rs:3".chars()));
        assert!(!may_contain_link("no locations here: none".chars()));
    }
}
