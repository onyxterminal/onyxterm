#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::Keystroke;
use std::path::PathBuf;
use terminal_core::InputModes;

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;
const CONTROL_MASK: u8 = 0x1f;
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const SHELL_SAFE_PUNCTUATION: &str = "/._-+,:@%=";

pub fn encode_keystroke(keystroke: &Keystroke, modes: InputModes) -> Option<Vec<u8>> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform {
        return None;
    }
    let base = named_key_bytes(keystroke, modes)
        .or_else(|| control_key_bytes(keystroke))
        .or_else(|| meta_key_bytes(keystroke))
        .or_else(|| keystroke.key_char.as_ref().map(|text| text.as_bytes().to_vec()))?;
    if modifiers.alt && is_named_key(&keystroke.key) {
        return Some(prefixed_with_escape(base));
    }
    Some(base)
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
    let key = if lines > 0 { "up" } else { "down" };
    arrow_bytes(key, modes).repeat(lines.unsigned_abs() as usize)
}

fn is_named_key(key: &str) -> bool {
    key.chars().count() > 1
}

fn named_key_bytes(keystroke: &Keystroke, modes: InputModes) -> Option<Vec<u8>> {
    let bytes: &[u8] = match keystroke.key.as_str() {
        "enter" => b"\r",
        "backspace" => &[DEL],
        "tab" if keystroke.modifiers.shift => b"\x1b[Z",
        "tab" => b"\t",
        "escape" => &[ESC],
        "space" if keystroke.modifiers.control => &[0],
        "space" => b" ",
        "up" | "down" | "right" | "left" => return Some(arrow_bytes(&keystroke.key, modes)),
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "delete" => b"\x1b[3~",
        "insert" => b"\x1b[2~",
        _ => return None,
    };
    Some(bytes.to_vec())
}

fn arrow_bytes(key: &str, modes: InputModes) -> Vec<u8> {
    let final_byte = match key {
        "up" => b'A',
        "down" => b'B',
        "right" => b'C',
        _ => b'D',
    };
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

fn single_ascii_byte(key: &str) -> Option<u8> {
    match key.as_bytes() {
        [byte] => Some(*byte),
        _ => None,
    }
}

fn prefixed_with_escape(bytes: Vec<u8>) -> Vec<u8> {
    [&[ESC][..], &bytes].concat()
}
