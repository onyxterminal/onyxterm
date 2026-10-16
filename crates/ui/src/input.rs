#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::Keystroke;
use std::path::PathBuf;
use terminal_core::InputModes;

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const CONTROL_MASK: u8 = 0x1f;
const LINE_START: u8 = 0x01;
const LINE_END: u8 = 0x05;
const KILL_LINE: u8 = 0x15;
const UNMODIFIED: u8 = 1;
const SHIFT_BIT: u8 = 1;
const ALT_BIT: u8 = 2;
const CONTROL_BIT: u8 = 4;
const ENTER_KEY: &str = "enter";
const KITTY_ESCAPE: u32 = 27;
const KITTY_ENTER: u32 = 13;
const KITTY_TAB: u32 = 9;
const KITTY_BACKSPACE: u32 = 127;
const KITTY_SPACE: u32 = 32;
const KITTY_F3: u8 = 13;
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const SHELL_SAFE_PUNCTUATION: &str = "/._-+,:@%=";
pub const FOCUS_IN_REPORT: &[u8] = b"\x1b[I";
pub const FOCUS_OUT_REPORT: &[u8] = b"\x1b[O";

#[derive(Debug, Clone, Copy)]
enum FunctionalKey {
    Cursor(u8),
    Function(u8),
    Tilde(u8),
}

pub fn encode_keystroke(keystroke: &Keystroke, modes: InputModes) -> Option<Vec<u8>> {
    if keystroke.modifiers.platform {
        return line_editing_bytes(keystroke);
    }
    let mode_specific = if modes.kitty.disambiguate { kitty_key_bytes(keystroke) } else { word_jump_bytes(keystroke) };
    mode_specific
        .or_else(|| functional_key_bytes(keystroke, modes))
        .or_else(|| legacy_key_bytes(keystroke))
}

pub fn is_composed_text(keystroke: &Keystroke, option_as_meta: bool) -> bool {
    let modifiers = keystroke.modifiers;
    let is_meta = modifiers.alt && option_as_meta;
    !is_named_key(&keystroke.key) && keystroke.key_char.is_some() && !modifiers.control && !modifiers.platform && !is_meta
}

pub fn dropped_paths_text(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| shell_escape(&path.to_string_lossy())).collect::<Vec<_>>().join(" ")
}

pub fn shell_escape(text: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || SHELL_SAFE_PUNCTUATION.contains(c);
    if !text.is_empty() && text.chars().all(is_safe) {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

pub fn paste_bytes(text: &str, modes: InputModes) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if !modes.bracketed_paste {
        return normalized.into_bytes();
    }
    let sanitized = normalized.replace('\u{1b}', "");
    [PASTE_START, sanitized.as_bytes(), PASTE_END].concat()
}

pub fn page_scroll_lines(keystroke: &Keystroke, rows: u16) -> Option<i32> {
    let modifiers = keystroke.modifiers;
    if !modifiers.shift || modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    let page = i32::from(rows.max(1));
    match keystroke.key.as_str() {
        "pageup" => Some(page),
        "pagedown" => Some(-page),
        _ => None,
    }
}

pub fn wheel_bytes(lines: i32, modes: InputModes) -> Vec<u8> {
    let final_byte = if lines > 0 { b'A' } else { b'B' };
    cursor_bytes(final_byte, modes).repeat(lines.unsigned_abs() as usize)
}

fn is_named_key(key: &str) -> bool {
    key.chars().count() > 1
}

fn line_editing_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let modifiers = keystroke.modifiers;
    if modifiers.shift || modifiers.alt || modifiers.control {
        return None;
    }
    let byte = match keystroke.key.as_str() {
        "left" => LINE_START,
        "right" => LINE_END,
        "backspace" => KILL_LINE,
        _ => return None,
    };
    Some(vec![byte])
}

fn word_jump_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let modifiers = keystroke.modifiers;
    if !modifiers.alt || modifiers.shift || modifiers.control {
        return None;
    }
    let bytes: &[u8] = match keystroke.key.as_str() {
        "left" => b"\x1bb",
        "right" => b"\x1bf",
        _ => return None,
    };
    Some(bytes.to_vec())
}

fn kitty_key_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let code = kitty_key_code(keystroke)?;
    let modifier = modifier_param(keystroke);
    if modifier == UNMODIFIED {
        return Some(format!("\x1b[{code}u").into_bytes());
    }
    Some(format!("\x1b[{code};{modifier}u").into_bytes())
}

fn kitty_key_code(keystroke: &Keystroke) -> Option<u32> {
    let modifiers = keystroke.modifiers;
    let is_modified = modifiers.shift || modifiers.alt || modifiers.control;
    let has_command_modifier = modifiers.alt || modifiers.control;
    match keystroke.key.as_str() {
        "escape" => Some(KITTY_ESCAPE),
        "enter" if is_modified => Some(KITTY_ENTER),
        "tab" if is_modified => Some(KITTY_TAB),
        "backspace" if is_modified => Some(KITTY_BACKSPACE),
        "space" if has_command_modifier => Some(KITTY_SPACE),
        key if has_command_modifier => single_char(key).map(u32::from),
        _ => None,
    }
}

fn modifier_param(keystroke: &Keystroke) -> u8 {
    let modifiers = keystroke.modifiers;
    let bit = |is_held: bool, value: u8| if is_held { value } else { 0 };
    UNMODIFIED + bit(modifiers.shift, SHIFT_BIT) + bit(modifiers.alt, ALT_BIT) + bit(modifiers.control, CONTROL_BIT)
}

fn functional_key(key: &str, modes: InputModes) -> Option<FunctionalKey> {
    let functional = match key {
        "up" => FunctionalKey::Cursor(b'A'),
        "down" => FunctionalKey::Cursor(b'B'),
        "right" => FunctionalKey::Cursor(b'C'),
        "left" => FunctionalKey::Cursor(b'D'),
        "home" => FunctionalKey::Cursor(b'H'),
        "end" => FunctionalKey::Cursor(b'F'),
        "f1" => FunctionalKey::Function(b'P'),
        "f2" => FunctionalKey::Function(b'Q'),
        "f3" if modes.kitty.disambiguate => FunctionalKey::Tilde(KITTY_F3),
        "f3" => FunctionalKey::Function(b'R'),
        "f4" => FunctionalKey::Function(b'S'),
        "insert" => FunctionalKey::Tilde(2),
        "delete" => FunctionalKey::Tilde(3),
        "pageup" => FunctionalKey::Tilde(5),
        "pagedown" => FunctionalKey::Tilde(6),
        "f5" => FunctionalKey::Tilde(15),
        "f6" => FunctionalKey::Tilde(17),
        "f7" => FunctionalKey::Tilde(18),
        "f8" => FunctionalKey::Tilde(19),
        "f9" => FunctionalKey::Tilde(20),
        "f10" => FunctionalKey::Tilde(21),
        "f11" => FunctionalKey::Tilde(23),
        "f12" => FunctionalKey::Tilde(24),
        _ => return None,
    };
    Some(functional)
}

fn functional_key_bytes(keystroke: &Keystroke, modes: InputModes) -> Option<Vec<u8>> {
    let key = functional_key(&keystroke.key, modes)?;
    let modifier = modifier_param(keystroke);
    let bytes = match (key, modifier) {
        (FunctionalKey::Cursor(final_byte), UNMODIFIED) => cursor_bytes(final_byte, modes),
        (FunctionalKey::Function(final_byte), UNMODIFIED) => vec![ESC, b'O', final_byte],
        (FunctionalKey::Cursor(final_byte) | FunctionalKey::Function(final_byte), _) => {
            format!("\x1b[1;{modifier}{}", char::from(final_byte)).into_bytes()
        }
        (FunctionalKey::Tilde(number), UNMODIFIED) => format!("\x1b[{number}~").into_bytes(),
        (FunctionalKey::Tilde(number), _) => format!("\x1b[{number};{modifier}~").into_bytes(),
    };
    Some(bytes)
}

fn legacy_key_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let base = named_key_bytes(keystroke)
        .or_else(|| control_key_bytes(keystroke))
        .or_else(|| meta_key_bytes(keystroke))
        .or_else(|| keystroke.key_char.as_ref().map(|text| text.as_bytes().to_vec()))?;
    if is_escape_prefixed(keystroke) {
        return Some(prefixed_with_escape(base));
    }
    Some(base)
}

fn is_escape_prefixed(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    let is_shift_enter = modifiers.shift && keystroke.key == ENTER_KEY;
    is_named_key(&keystroke.key) && (modifiers.alt || is_shift_enter)
}

fn named_key_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let bytes: &[u8] = match keystroke.key.as_str() {
        "enter" => b"\r",
        "backspace" => &[DEL],
        "tab" if keystroke.modifiers.shift => b"\x1b[Z",
        "tab" => b"\t",
        "escape" => &[ESC],
        "space" if keystroke.modifiers.shift => b"\n",
        "space" if keystroke.modifiers.control => &[0],
        "space" => b" ",
        _ => return None,
    };
    Some(bytes.to_vec())
}

fn cursor_bytes(final_byte: u8, modes: InputModes) -> Vec<u8> {
    let introducer = if modes.app_cursor { b'O' } else { b'[' };
    vec![ESC, introducer, final_byte]
}

fn control_key_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    if !keystroke.modifiers.control {
        return None;
    }
    let byte = single_ascii_byte(&keystroke.key)?;
    let control_byte = match byte {
        b'a'..=b'z' | b'[' | b'\\' | b']' | b'@' | b'^' | b'_' => byte & CONTROL_MASK,
        b'?' => DEL,
        _ => return None,
    };
    let encoded = vec![control_byte];
    if keystroke.modifiers.alt {
        return Some(prefixed_with_escape(encoded));
    }
    Some(encoded)
}

fn meta_key_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    if !keystroke.modifiers.alt || is_named_key(&keystroke.key) {
        return None;
    }
    let key = if keystroke.modifiers.shift {
        keystroke.key.to_uppercase()
    } else {
        keystroke.key.clone()
    };
    Some(prefixed_with_escape(key.into_bytes()))
}

fn single_char(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let (Some(only), None) = (chars.next(), chars.next()) else {
        return None;
    };
    Some(only)
}

fn single_ascii_byte(key: &str) -> Option<u8> {
    match key.as_bytes() {
        [byte] => Some(*byte),
        _ => None,
    }
}

fn prefixed_with_escape(bytes: Vec<u8>) -> Vec<u8> {
    [&[ESC][..], &bytes].concat()
}
