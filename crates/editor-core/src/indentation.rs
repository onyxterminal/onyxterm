use crate::motion::TAB_WIDTH;
use ropey::Rope;

const SAMPLED_LINES: usize = 400;
const MIN_SPACE_INDENT: usize = 2;
const DEFAULT_SPACE_INDENT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indentation {
    Spaces(usize),
    Tabs(usize),
}

pub fn detect_indentation(rope: &Rope) -> Indentation {
    let mut tab_lines = 0;
    let mut space_lines = 0;
    let mut narrowest_spaces: Option<usize> = None;
    for line in rope.lines().take(SAMPLED_LINES) {
        let mut chars = line.chars();
        match chars.next() {
            Some('\t') => tab_lines += 1,
            Some(' ') => {
                let width = 1 + chars.take_while(|ch| *ch == ' ').count();
                space_lines += 1;
                if width >= MIN_SPACE_INDENT {
                    narrowest_spaces = Some(narrowest_spaces.map_or(width, |narrowest| narrowest.min(width)));
                }
            }
            _ => {}
        }
    }
    if tab_lines > space_lines {
        return Indentation::Tabs(TAB_WIDTH);
    }
    Indentation::Spaces(narrowest_spaces.unwrap_or(DEFAULT_SPACE_INDENT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_indented_text_reports_its_narrowest_step() {
        let rope = Rope::from_str("fn main() {\n  if x {\n    y();\n  }\n}\n");
        assert_eq!(detect_indentation(&rope), Indentation::Spaces(2));
    }

    #[test]
    fn tab_indented_text_reports_tabs() {
        let rope = Rope::from_str("fn main() {\n\tif x {\n\t\ty();\n\t}\n}\n");
        assert_eq!(detect_indentation(&rope), Indentation::Tabs(TAB_WIDTH));
    }

    #[test]
    fn unindented_text_defaults_to_four_spaces() {
        assert_eq!(detect_indentation(&Rope::from_str("a\nb\n")), Indentation::Spaces(4));
        assert_eq!(detect_indentation(&Rope::from_str(" * single space\n")), Indentation::Spaces(4));
    }
}
