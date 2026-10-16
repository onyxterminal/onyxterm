#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::Modifiers;
use terminal_core::{GridPoint, InputModes, MouseModes};

const SHIFT_FLAG: u8 = 4;
const ALT_FLAG: u8 = 8;
const CONTROL_FLAG: u8 = 16;
const MOTION_FLAG: u8 = 32;
const NO_BUTTON_CODE: u8 = 3;
const WHEEL_UP_CODE: u8 = 64;
const WHEEL_DOWN_CODE: u8 = 65;
const LEGACY_OFFSET: u32 = 32;
const X10_MAX_COORDINATE: u32 = 223;
const UTF8_MAX_COORDINATE: u32 = 2015;
const LEGACY_PREFIX: &[u8] = b"\x1b[M";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportedButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    None,
}

impl ReportedButton {
    fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::None => NO_BUTTON_CODE,
            Self::WheelUp => WHEEL_UP_CODE,
            Self::WheelDown => WHEEL_DOWN_CODE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseReport {
    pub button: ReportedButton,
    pub action: MouseAction,
    pub point: GridPoint,
    pub modifiers: Modifiers,
}

pub fn mouse_report_bytes(report: MouseReport, modes: InputModes) -> Option<Vec<u8>> {
    if !is_reported(report, modes.mouse) {
        return None;
    }
    let col = u32::from(report.point.col) + 1;
    let row = u32::from(report.point.row) + 1;
    if modes.mouse.sgr {
        return Some(sgr_bytes(report, col, row));
    }
    let code = legacy_code(report);
    if modes.mouse.utf8 {
        return Some(utf8_bytes(code, col, row));
    }
    Some(x10_bytes(code, col, row))
}

pub fn wheel_report_bytes(lines: i32, point: GridPoint, modifiers: Modifiers, modes: InputModes) -> Vec<u8> {
    let button = if lines > 0 { ReportedButton::WheelUp } else { ReportedButton::WheelDown };
    let report = MouseReport { button, action: MouseAction::Press, point, modifiers };
    mouse_report_bytes(report, modes).unwrap_or_default().repeat(lines.unsigned_abs() as usize)
}

fn is_reported(report: MouseReport, mouse: MouseModes) -> bool {
    match (report.action, report.button) {
        (MouseAction::Motion, ReportedButton::None) => mouse.report_motion,
        (MouseAction::Motion, _) => mouse.report_drag || mouse.report_motion,
        _ => mouse.is_reporting(),
    }
}

fn button_code(report: MouseReport) -> u8 {
    let motion = if report.action == MouseAction::Motion { MOTION_FLAG } else { 0 };
    report.button.code() + motion + modifier_flags(report.modifiers)
}

fn legacy_code(report: MouseReport) -> u8 {
    if report.action == MouseAction::Release {
        return NO_BUTTON_CODE + modifier_flags(report.modifiers);
    }
    button_code(report)
}

fn modifier_flags(modifiers: Modifiers) -> u8 {
    let flag = |is_held: bool, value: u8| if is_held { value } else { 0 };
    flag(modifiers.shift, SHIFT_FLAG) + flag(modifiers.alt, ALT_FLAG) + flag(modifiers.control, CONTROL_FLAG)
}

fn sgr_bytes(report: MouseReport, col: u32, row: u32) -> Vec<u8> {
    let final_char = if report.action == MouseAction::Release { 'm' } else { 'M' };
    format!("\x1b[<{};{col};{row}{final_char}", button_code(report)).into_bytes()
}

fn x10_bytes(code: u8, col: u32, row: u32) -> Vec<u8> {
    let encode = |value: u32| (value.min(X10_MAX_COORDINATE) + LEGACY_OFFSET) as u8;
    [LEGACY_PREFIX, &[encode(u32::from(code)), encode(col), encode(row)]].concat()
}

fn utf8_bytes(code: u8, col: u32, row: u32) -> Vec<u8> {
    let mut encoded = String::new();
    for value in [u32::from(code), col.min(UTF8_MAX_COORDINATE), row.min(UTF8_MAX_COORDINATE)] {
        encoded.extend(char::from_u32(value + LEGACY_OFFSET));
    }
    [LEGACY_PREFIX, encoded.as_bytes()].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes_with(mouse: MouseModes) -> InputModes {
        InputModes { mouse, ..InputModes::default() }
    }

    fn sgr_modes() -> InputModes {
        modes_with(MouseModes { report_click: true, sgr: true, ..MouseModes::default() })
    }

    fn click_modes() -> InputModes {
        modes_with(MouseModes { report_click: true, ..MouseModes::default() })
    }

    fn report(button: ReportedButton, action: MouseAction, col: u16, row: u16) -> MouseReport {
        MouseReport { button, action, point: GridPoint { row, col }, modifiers: Modifiers::default() }
    }

    #[test]
    fn sgr_reports_press_and_release_with_one_based_cells() {
        let press = report(ReportedButton::Left, MouseAction::Press, 4, 9);
        let release = report(ReportedButton::Right, MouseAction::Release, 0, 0);
        assert_eq!(mouse_report_bytes(press, sgr_modes()), Some(b"\x1b[<0;5;10M".to_vec()));
        assert_eq!(mouse_report_bytes(release, sgr_modes()), Some(b"\x1b[<2;1;1m".to_vec()));
    }

    #[test]
    fn modifiers_add_their_flags_to_the_button_code() {
        let modifiers = Modifiers { control: true, alt: true, ..Modifiers::default() };
        let press = MouseReport { modifiers, ..report(ReportedButton::Middle, MouseAction::Press, 0, 0) };
        assert_eq!(mouse_report_bytes(press, sgr_modes()), Some(b"\x1b[<25;1;1M".to_vec()));
    }

    #[test]
    fn x10_offsets_by_32_and_releases_as_button_three() {
        let press = report(ReportedButton::Left, MouseAction::Press, 0, 1);
        let release = report(ReportedButton::Left, MouseAction::Release, 0, 1);
        assert_eq!(mouse_report_bytes(press, click_modes()), Some(vec![0x1b, b'[', b'M', 32, 33, 34]));
        assert_eq!(mouse_report_bytes(release, click_modes()), Some(vec![0x1b, b'[', b'M', 35, 33, 34]));
    }

    #[test]
    fn x10_clamps_coordinates_past_223() {
        let press = report(ReportedButton::Left, MouseAction::Press, 400, 300);
        assert_eq!(mouse_report_bytes(press, click_modes()), Some(vec![0x1b, b'[', b'M', 32, 255, 255]));
    }

    #[test]
    fn utf8_mode_encodes_large_coordinates_as_characters() {
        let modes = modes_with(MouseModes { report_click: true, utf8: true, ..MouseModes::default() });
        let press = report(ReportedButton::Left, MouseAction::Press, 299, 0);
        let expected = [b"\x1b[M ".as_slice(), "\u{14c}".as_bytes(), b"!"].concat();
        assert_eq!(mouse_report_bytes(press, modes), Some(expected));
    }

    #[test]
    fn drag_needs_button_event_tracking_and_motion_needs_any_event_tracking() {
        let drag = report(ReportedButton::Left, MouseAction::Motion, 1, 1);
        let hover = report(ReportedButton::None, MouseAction::Motion, 1, 1);
        assert_eq!(mouse_report_bytes(drag, sgr_modes()), None);
        let drag_modes = modes_with(MouseModes { report_drag: true, sgr: true, ..MouseModes::default() });
        assert_eq!(mouse_report_bytes(drag, drag_modes), Some(b"\x1b[<32;2;2M".to_vec()));
        assert_eq!(mouse_report_bytes(hover, drag_modes), None);
        let any_modes = modes_with(MouseModes { report_motion: true, sgr: true, ..MouseModes::default() });
        assert_eq!(mouse_report_bytes(hover, any_modes), Some(b"\x1b[<35;2;2M".to_vec()));
    }

    #[test]
    fn nothing_is_reported_without_a_mouse_mode() {
        let press = report(ReportedButton::Left, MouseAction::Press, 0, 0);
        assert_eq!(mouse_report_bytes(press, InputModes::default()), None);
    }

    #[test]
    fn wheel_reports_one_press_per_line() {
        let point = GridPoint { row: 0, col: 0 };
        let up = wheel_report_bytes(2, point, Modifiers::default(), sgr_modes());
        assert_eq!(up, b"\x1b[<64;1;1M\x1b[<64;1;1M".to_vec());
        let down = wheel_report_bytes(-1, point, Modifiers::default(), sgr_modes());
        assert_eq!(down, b"\x1b[<65;1;1M".to_vec());
    }
}
