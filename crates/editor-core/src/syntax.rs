mod line_states;
mod markdown;
mod rust;
mod scanner;
mod toml;

use scanner::{DiscardSpans, Scanner, SpanSink};
use std::ops::Range;
use std::path::Path;

pub use line_states::{HighlightSource, LineStates};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Number,
    Constant,
    Type,
    Function,
    Macro,
    Comment,
    Punctuation,
    Heading,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    #[default]
    Normal,
    BlockComment(u32),
    String,
    RawString(u8),
    TripleQuoted(u8),
}

impl Language {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "md" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Toml => "TOML",
            Self::Markdown => "Markdown",
        }
    }

    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("//"),
            Self::Toml => Some("#"),
            Self::Markdown => None,
        }
    }

    pub fn highlight_line(self, line: &str, state: LineState) -> Vec<Span> {
        let mut scanner = Scanner::new(line, Vec::new());
        self.lex(&mut scanner, state);
        let byte_spans = scanner.into_sink();
        match line.is_ascii() {
            true => byte_spans,
            false => char_spans(line, byte_spans),
        }
    }

    pub fn end_state(self, line: &str, state: LineState) -> LineState {
        self.lex(&mut Scanner::new(line, DiscardSpans), state)
    }

    fn lex<S: SpanSink>(self, scanner: &mut Scanner<S>, state: LineState) -> LineState {
        match self {
            Self::Rust => rust::lex(scanner, state),
            Self::Toml => toml::lex(scanner, state),
            Self::Markdown => {
                markdown::lex(scanner);
                LineState::Normal
            }
        }
    }
}

fn char_spans(line: &str, byte_spans: Vec<Span>) -> Vec<Span> {
    let mut char_of_byte = vec![0; line.len() + 1];
    let mut chars = 0;
    for (byte, ch) in line.char_indices() {
        char_of_byte[byte..byte + ch.len_utf8()].fill(chars);
        chars += 1;
    }
    char_of_byte[line.len()] = chars;
    byte_spans
        .into_iter()
        .map(|span| Span { range: char_of_byte[span.range.start]..char_of_byte[span.range.end], kind: span.kind })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(language: Language, line: &str, state: LineState) -> Vec<(String, TokenKind)> {
        let chars: Vec<char> = line.chars().collect();
        language
            .highlight_line(line, state)
            .into_iter()
            .map(|span| (chars[span.range].iter().collect(), span.kind))
            .collect()
    }

    fn rust(line: &str) -> Vec<(String, TokenKind)> {
        tokens(Language::Rust, line, LineState::Normal)
    }

    fn kind_of(found: &[(String, TokenKind)], text: &str) -> Option<TokenKind> {
        found.iter().find(|(token, _)| token == text).map(|(_, kind)| *kind)
    }

    #[test]
    fn languages_come_from_file_extensions() {
        assert_eq!(Language::from_path(Path::new("/src/main.rs")), Some(Language::Rust));
        assert_eq!(Language::from_path(Path::new("Cargo.toml")), Some(Language::Toml));
        assert_eq!(Language::from_path(Path::new("docs/README.MD")), Some(Language::Markdown));
        assert_eq!(Language::from_path(Path::new("notes.txt")), None);
        assert_eq!(Language::from_path(Path::new("Makefile")), None);
    }

    #[test]
    fn languages_name_themselves_and_their_line_comment() {
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::Toml.name(), "TOML");
        assert_eq!(Language::Markdown.name(), "Markdown");
        assert_eq!(Language::Rust.line_comment(), Some("//"));
        assert_eq!(Language::Toml.line_comment(), Some("#"));
        assert_eq!(Language::Markdown.line_comment(), None);
    }

    #[test]
    fn rust_keywords_types_functions_and_macros() {
        let found = rust("pub fn main() { let value: Option<Vec<u8>> = compute(1); println!(\"x\"); }");
        assert_eq!(kind_of(&found, "pub"), Some(TokenKind::Keyword));
        assert_eq!(kind_of(&found, "fn"), Some(TokenKind::Keyword));
        assert_eq!(kind_of(&found, "let"), Some(TokenKind::Keyword));
        assert_eq!(kind_of(&found, "main"), Some(TokenKind::Function));
        assert_eq!(kind_of(&found, "compute"), Some(TokenKind::Function));
        assert_eq!(kind_of(&found, "Option"), Some(TokenKind::Type));
        assert_eq!(kind_of(&found, "Vec"), Some(TokenKind::Type));
        assert_eq!(kind_of(&found, "println!"), Some(TokenKind::Macro));
        assert_eq!(kind_of(&found, "1"), Some(TokenKind::Number));
        assert_eq!(kind_of(&found, "\"x\""), Some(TokenKind::String));
        assert_eq!(kind_of(&found, "value"), None);
        assert_eq!(kind_of(&found, "u8"), None);
    }

    #[test]
    fn rust_literal_constants_and_punctuation() {
        let found = rust("if done != true { return Some(Err(None)); }");
        assert_eq!(kind_of(&found, "true"), Some(TokenKind::Constant));
        assert_eq!(kind_of(&found, "Some"), Some(TokenKind::Constant));
        assert_eq!(kind_of(&found, "Err"), Some(TokenKind::Constant));
        assert_eq!(kind_of(&found, "None"), Some(TokenKind::Constant));
        assert_eq!(kind_of(&found, "done"), None);
        assert_eq!(kind_of(&found, "!="), Some(TokenKind::Punctuation));
        assert_eq!(kind_of(&found, "));"), Some(TokenKind::Punctuation));
    }

    #[test]
    fn rust_comments_strings_and_chars() {
        let found = rust("let c = '\\''; let d = 'x'; // done \"not a string\"");
        assert_eq!(kind_of(&found, "'\\''"), Some(TokenKind::String));
        assert_eq!(kind_of(&found, "'x'"), Some(TokenKind::String));
        assert_eq!(kind_of(&found, "// done \"not a string\""), Some(TokenKind::Comment));
        let escaped = rust("\"a \\\" b\" + x");
        assert_eq!(kind_of(&escaped, "\"a \\\" b\""), Some(TokenKind::String));
    }

    #[test]
    fn rust_lifetimes_are_not_char_literals() {
        let found = rust("fn get<'a>(value: &'a str) -> &'a str");
        assert!(found.iter().all(|(_, kind)| *kind != TokenKind::String), "{found:?}");
        assert_eq!(kind_of(&found, "get"), None);
    }

    #[test]
    fn rust_block_comments_span_lines_and_nest() {
        let open = Language::Rust.end_state("let x = 1; /* start /* inner */", LineState::Normal);
        assert_eq!(open, LineState::BlockComment(1));
        let middle = tokens(Language::Rust, "fn still_comment()", open);
        assert_eq!(middle, vec![("fn still_comment()".to_string(), TokenKind::Comment)]);
        let closing = tokens(Language::Rust, "end */ let y", open);
        assert_eq!(kind_of(&closing, "end */"), Some(TokenKind::Comment));
        assert_eq!(kind_of(&closing, "let"), Some(TokenKind::Keyword));
        assert_eq!(Language::Rust.end_state("end */ let y", open), LineState::Normal);
    }

    #[test]
    fn rust_strings_and_raw_strings_span_lines() {
        assert_eq!(Language::Rust.end_state("let s = \"first", LineState::Normal), LineState::String);
        let closing = tokens(Language::Rust, "second\"; fn", LineState::String);
        assert_eq!(kind_of(&closing, "second\""), Some(TokenKind::String));
        assert_eq!(kind_of(&closing, "fn"), Some(TokenKind::Keyword));
        let raw = Language::Rust.end_state("let r = r#\"has \" quote", LineState::Normal);
        assert_eq!(raw, LineState::RawString(1));
        assert_eq!(Language::Rust.end_state("still \" open", raw), raw);
        assert_eq!(Language::Rust.end_state("done\"# ;", raw), LineState::Normal);
        let same_line = rust("let r = r\"one\"; let b = b\"two\";");
        assert_eq!(kind_of(&same_line, "r\"one\""), Some(TokenKind::String));
        assert_eq!(kind_of(&same_line, "b\"two\""), Some(TokenKind::String));
    }

    #[test]
    fn rust_numbers_include_suffixes_and_floats_but_not_ranges() {
        let found = rust("let x = 1.5e3 + 0xFFu8; for i in 0..10 {}");
        assert_eq!(kind_of(&found, "1.5e3"), Some(TokenKind::Number));
        assert_eq!(kind_of(&found, "0xFFu8"), Some(TokenKind::Number));
        assert_eq!(kind_of(&found, "0"), Some(TokenKind::Number));
        assert_eq!(kind_of(&found, "10"), Some(TokenKind::Number));
    }

    #[test]
    fn spans_use_char_offsets_on_non_ascii_lines() {
        let found = rust("let é = \"ü\"; Ünïcode::new(ß)");
        assert_eq!(kind_of(&found, "\"ü\""), Some(TokenKind::String));
        assert_eq!(kind_of(&found, "Ünïcode"), Some(TokenKind::Type));
        assert_eq!(kind_of(&found, "new"), Some(TokenKind::Function));
    }

    #[test]
    fn toml_headers_keys_values_and_comments() {
        let toml = |line: &str| tokens(Language::Toml, line, LineState::Normal);
        assert_eq!(toml("[workspace.package]"), vec![("[workspace.package]".to_string(), TokenKind::Type)]);
        assert_eq!(toml("[[bin]]"), vec![("[[bin]]".to_string(), TokenKind::Type)]);
        let pair = toml("serde = { version = \"1.0\", default-features = false } # pinned");
        assert_eq!(kind_of(&pair, "serde"), Some(TokenKind::Function));
        assert_eq!(kind_of(&pair, "\"1.0\""), Some(TokenKind::String));
        assert_eq!(kind_of(&pair, "false"), Some(TokenKind::Constant));
        assert_eq!(kind_of(&pair, "# pinned"), Some(TokenKind::Comment));
        assert_eq!(kind_of(&pair, "version"), None);
        let number = toml("  size_px = -13.5");
        assert_eq!(kind_of(&number, "size_px"), Some(TokenKind::Function));
        assert_eq!(kind_of(&number, "-13.5"), Some(TokenKind::Number));
        assert_eq!(kind_of(&toml("\"quoted key\" = 'raw'"), "\"quoted key\""), Some(TokenKind::Function));
    }

    #[test]
    fn toml_multiline_strings_carry_state() {
        let open = Language::Toml.end_state("text = \"\"\"first", LineState::Normal);
        assert_eq!(open, LineState::TripleQuoted(b'"'));
        assert_eq!(tokens(Language::Toml, "key = 1", open), vec![("key = 1".to_string(), TokenKind::String)]);
        let closing = tokens(Language::Toml, "end\"\"\" # note", open);
        assert_eq!(kind_of(&closing, "end\"\"\""), Some(TokenKind::String));
        assert_eq!(kind_of(&closing, "# note"), Some(TokenKind::Comment));
        assert_eq!(Language::Toml.end_state("raw = '''x'''", LineState::Normal), LineState::Normal);
    }

    #[test]
    fn markdown_headings_and_inline_code() {
        let markdown = |line: &str| tokens(Language::Markdown, line, LineState::Normal);
        assert_eq!(markdown("## Install"), vec![("## Install".to_string(), TokenKind::Heading)]);
        assert_eq!(markdown("#hashtag"), vec![]);
        let code = markdown("Run `cargo test` or ``a ` b`` now");
        assert_eq!(code, vec![
            ("`cargo test`".to_string(), TokenKind::String),
            ("``a ` b``".to_string(), TokenKind::String),
        ]);
        assert_eq!(markdown("unclosed `tick"), vec![]);
    }
}
