#![deny(warnings)]
#![forbid(unsafe_code)]

use editor_core::syntax::Language;
use editor_core::{Indentation, Rope};
use std::path::{Component, Path};

const PLAIN_TEXT: &str = "Plain Text";
const STATUS_SEPARATOR: &str = " \u{00B7} ";
const CRUMB_SEPARATOR: &str = " \u{203A} ";

pub struct CaretStatus<'a> {
    pub rope: &'a Rope,
    pub head: usize,
    pub language: Option<Language>,
    pub indentation: Indentation,
}

pub fn caret_status_text(status: CaretStatus) -> String {
    let head = status.head.min(status.rope.len_chars());
    let line = status.rope.char_to_line(head);
    let column = head - status.rope.line_to_char(line);
    let language = status.language.map_or(PLAIN_TEXT, Language::name);
    let indentation = indentation_label(status.indentation);
    [format!("Ln {}, Col {}", line + 1, column + 1), language.to_string(), indentation].join(STATUS_SEPARATOR)
}

fn indentation_label(indentation: Indentation) -> String {
    match indentation {
        Indentation::Spaces(width) => format!("Spaces: {width}"),
        Indentation::Tabs(width) => format!("Tab Size: {width}"),
    }
}

pub fn breadcrumb_text(path: &Path, root: &Path) -> String {
    let shown = path.strip_prefix(root).unwrap_or(path);
    let segments: Vec<String> = shown
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    segments.join(CRUMB_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(text: &str, head: usize, language: Option<Language>) -> String {
        let rope = Rope::from_str(text);
        caret_status_text(CaretStatus { rope: &rope, head, language, indentation: Indentation::Spaces(4) })
    }

    #[test]
    fn caret_status_reads_line_column_language_and_indentation() {
        let text = "fn main() {\n    let side = 1;\n}\n";
        assert_eq!(status(text, 20, Some(Language::Rust)), "Ln 2, Col 9 \u{00B7} Rust \u{00B7} Spaces: 4");
        assert_eq!(status(text, 0, Some(Language::Toml)), "Ln 1, Col 1 \u{00B7} TOML \u{00B7} Spaces: 4");
        assert_eq!(status("abc", 99, None), "Ln 1, Col 4 \u{00B7} Plain Text \u{00B7} Spaces: 4");
    }

    #[test]
    fn caret_status_names_tab_indentation() {
        let rope = Rope::from_str("x");
        let language = Some(Language::Markdown);
        let tabs = CaretStatus { rope: &rope, head: 0, language, indentation: Indentation::Tabs(4) };
        assert_eq!(caret_status_text(tabs), "Ln 1, Col 1 \u{00B7} Markdown \u{00B7} Tab Size: 4");
    }

    #[test]
    fn breadcrumbs_join_the_root_relative_path() {
        let crumbs = breadcrumb_text(Path::new("/w/onyx/crates/ui/src/shell.rs"), Path::new("/w/onyx"));
        assert_eq!(crumbs, "crates \u{203A} ui \u{203A} src \u{203A} shell.rs");
    }

    #[test]
    fn breadcrumbs_outside_the_root_show_the_whole_path() {
        assert_eq!(breadcrumb_text(Path::new("/tmp/a.rs"), Path::new("/w/onyx")), "tmp \u{203A} a.rs");
    }
}
