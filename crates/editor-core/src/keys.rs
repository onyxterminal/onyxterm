use crate::editor::{EditorAction, Granularity};
use crate::motion::Motion;

pub struct KeyInput<'a> {
    pub key: &'a str,
    pub text: Option<&'a str>,
    pub cmd: bool,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
}

pub fn standard_action(input: &KeyInput<'_>, page_lines: usize) -> Option<EditorAction> {
    let movement = |motion| Some(EditorAction::Move { motion, extend: input.shift });
    match input.key {
        "left" if input.cmd => movement(Motion::LineStart),
        "right" if input.cmd => movement(Motion::LineEnd),
        "up" if input.cmd => movement(Motion::DocumentStart),
        "down" if input.cmd => movement(Motion::DocumentEnd),
        "left" if input.alt => movement(Motion::WordLeft),
        "right" if input.alt => movement(Motion::WordRight),
        "left" => movement(Motion::Left),
        "right" => movement(Motion::Right),
        "up" => movement(Motion::Up),
        "down" => movement(Motion::Down),
        "home" => movement(Motion::LineStart),
        "end" => movement(Motion::LineEnd),
        "pageup" => movement(Motion::PageUp(page_lines)),
        "pagedown" => movement(Motion::PageDown(page_lines)),
        "backspace" => Some(EditorAction::DeleteBackward(granularity(input))),
        "delete" => Some(EditorAction::DeleteForward(granularity(input))),
        "enter" => Some(EditorAction::Newline),
        "tab" if input.shift => Some(EditorAction::Outdent),
        "tab" => Some(EditorAction::Indent),
        "z" if input.cmd && input.shift => Some(EditorAction::Redo),
        "z" if input.cmd => Some(EditorAction::Undo),
        "x" if input.cmd => Some(EditorAction::Cut),
        _ if input.cmd || input.ctrl => None,
        _ => input
            .text
            .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
            .map(|text| EditorAction::InsertText(text.to_string())),
    }
}

fn granularity(input: &KeyInput<'_>) -> Granularity {
    if input.cmd {
        Granularity::Line
    } else if input.alt {
        Granularity::Word
    } else {
        Granularity::Char
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> KeyInput<'_> {
        KeyInput { key, text: None, cmd: false, alt: false, shift: false, ctrl: false }
    }

    #[test]
    fn arrows_map_to_motions_with_modifiers() {
        let movement = |motion, extend| Some(EditorAction::Move { motion, extend });
        assert_eq!(standard_action(&key("left"), 30), movement(Motion::Left, false));
        assert_eq!(standard_action(&KeyInput { shift: true, ..key("right") }, 30), movement(Motion::Right, true));
        assert_eq!(standard_action(&KeyInput { alt: true, ..key("left") }, 30), movement(Motion::WordLeft, false));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("right") }, 30), movement(Motion::LineEnd, false));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("up") }, 30), movement(Motion::DocumentStart, false));
        assert_eq!(standard_action(&key("pagedown"), 30), movement(Motion::PageDown(30), false));
    }

    #[test]
    fn deletion_granularity_follows_modifiers() {
        assert_eq!(standard_action(&key("backspace"), 30), Some(EditorAction::DeleteBackward(Granularity::Char)));
        assert_eq!(
            standard_action(&KeyInput { alt: true, ..key("backspace") }, 30),
            Some(EditorAction::DeleteBackward(Granularity::Word))
        );
        assert_eq!(
            standard_action(&KeyInput { cmd: true, ..key("delete") }, 30),
            Some(EditorAction::DeleteForward(Granularity::Line))
        );
    }

    #[test]
    fn editing_commands() {
        assert_eq!(standard_action(&key("enter"), 30), Some(EditorAction::Newline));
        assert_eq!(standard_action(&key("tab"), 30), Some(EditorAction::Indent));
        assert_eq!(standard_action(&KeyInput { shift: true, ..key("tab") }, 30), Some(EditorAction::Outdent));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("z") }, 30), Some(EditorAction::Undo));
        assert_eq!(standard_action(&KeyInput { cmd: true, shift: true, ..key("z") }, 30), Some(EditorAction::Redo));
        assert_eq!(standard_action(&KeyInput { cmd: true, ..key("x") }, 30), Some(EditorAction::Cut));
    }

    #[test]
    fn printable_text_inserts_and_command_chords_do_not() {
        assert_eq!(
            standard_action(&KeyInput { text: Some("a"), ..key("a") }, 30),
            Some(EditorAction::InsertText("a".into()))
        );
        assert_eq!(
            standard_action(&KeyInput { text: Some("#"), alt: true, ..key("3") }, 30),
            Some(EditorAction::InsertText("#".into()))
        );
        assert_eq!(standard_action(&KeyInput { text: Some("q"), cmd: true, ..key("q") }, 30), None);
        assert_eq!(standard_action(&KeyInput { text: Some(""), ..key("escape") }, 30), None);
    }
}
