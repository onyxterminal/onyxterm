#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::Keystroke;
use terminal_core::{SelectionKind, ViMotion};

const ESCAPE_KEY: &str = "escape";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyModeAction {
    Motion(ViMotion),
    HalfPageUp,
    HalfPageDown,
    AwaitTop,
    Top,
    Bottom,
    ToggleSelection(SelectionKind),
    Yank,
    Exit,
    Ignore,
}

pub fn copy_mode_action(keystroke: &Keystroke, is_awaiting_top: bool) -> CopyModeAction {
    if keystroke.modifiers.control {
        return control_action(&keystroke.key);
    }
    if keystroke.key == ESCAPE_KEY {
        return CopyModeAction::Exit;
    }
    let typed = typed_text(keystroke);
    if typed == "g" {
        return if is_awaiting_top { CopyModeAction::Top } else { CopyModeAction::AwaitTop };
    }
    motion_for(&typed).map_or_else(|| command_for(&typed), CopyModeAction::Motion)
}

fn control_action(key: &str) -> CopyModeAction {
    match key {
        "u" => CopyModeAction::HalfPageUp,
        "d" => CopyModeAction::HalfPageDown,
        _ => CopyModeAction::Ignore,
    }
}

fn typed_text(keystroke: &Keystroke) -> String {
    if let Some(text) = keystroke.key_char.as_deref().filter(|text| !text.is_empty()) {
        return text.to_string();
    }
    if keystroke.modifiers.shift {
        return keystroke.key.to_uppercase();
    }
    keystroke.key.clone()
}

fn motion_for(typed: &str) -> Option<ViMotion> {
    let motion = match typed {
        "h" | "left" => ViMotion::Left,
        "j" | "down" => ViMotion::Down,
        "k" | "up" => ViMotion::Up,
        "l" | "right" => ViMotion::Right,
        "w" => ViMotion::SemanticRight,
        "b" => ViMotion::SemanticLeft,
        "e" => ViMotion::SemanticRightEnd,
        "0" => ViMotion::First,
        "$" => ViMotion::Last,
        _ => return None,
    };
    Some(motion)
}

fn command_for(typed: &str) -> CopyModeAction {
    match typed {
        "G" => CopyModeAction::Bottom,
        "v" => CopyModeAction::ToggleSelection(SelectionKind::Simple),
        "V" => CopyModeAction::ToggleSelection(SelectionKind::Line),
        "y" => CopyModeAction::Yank,
        "q" => CopyModeAction::Exit,
        _ => CopyModeAction::Ignore,
    }
}
