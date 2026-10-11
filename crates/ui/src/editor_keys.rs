#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::input::is_composed_text;
use editor_core::KeyInput;
use gpui::Keystroke;

pub fn key_input(keystroke: &Keystroke) -> KeyInput<'_> {
    KeyInput {
        key: &keystroke.key,
        text: keystroke.key_char.as_deref(),
        cmd: keystroke.modifiers.platform,
        alt: keystroke.modifiers.alt,
        shift: keystroke.modifiers.shift,
        ctrl: keystroke.modifiers.control,
    }
}

pub fn should_defer_to_input_handler(keystroke: &Keystroke, is_held: bool) -> bool {
    is_composed_text(keystroke, false) && !is_held
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn stroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: key_char.map(str::to_string),
        }
    }

    #[test]
    fn composed_text_is_left_to_the_input_handler() {
        assert!(should_defer_to_input_handler(&stroke("a", Some("a"), Modifiers::default()), false));
        assert!(!should_defer_to_input_handler(&stroke("a", Some("a"), Modifiers::default()), true));
        assert!(!should_defer_to_input_handler(&stroke("left", None, Modifiers::default()), false));
        assert!(!should_defer_to_input_handler(&stroke("z", Some("z"), Modifiers::command()), false));
    }

    #[test]
    fn key_input_carries_modifiers_and_text() {
        let keystroke = stroke(
            "left",
            None,
            Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::default()
            },
        );
        let input = key_input(&keystroke);
        assert_eq!(input.key, "left");
        assert!(input.alt && input.shift && !input.cmd && !input.ctrl);
    }
}
