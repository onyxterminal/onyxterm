const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const OSC_INTRODUCER: u8 = b']';
const STRING_TERMINATOR: u8 = b'\\';
const CANCEL: u8 = 0x18;
const SUBSTITUTE: u8 = 0x1a;
const MAX_PAYLOAD: usize = 4096;
const MARK_PREFIX: &[u8] = b"133;";
const COMMAND_LINE_KEY: &str = "cmdline=";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellMark {
    PromptStart,
    CommandStart,
    CommandExecuted { command: Option<String> },
    CommandFinished { exit_code: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedMark {
    pub end: usize,
    pub mark: ShellMark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ScanState {
    #[default]
    Ground,
    Escape,
    OscBody,
    OscEscape,
}

#[derive(Debug, Default)]
pub struct MarkScanner {
    state: ScanState,
    payload: Vec<u8>,
    is_overflowing: bool,
}

impl MarkScanner {
    pub fn scan(&mut self, bytes: &[u8]) -> Vec<ScannedMark> {
        let mut marks = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if self.state == ScanState::Ground {
                match bytes[index..].iter().position(|byte| *byte == ESC) {
                    Some(offset) => index += offset,
                    None => break,
                }
            }
            if let Some(mark) = self.advance(bytes[index]) {
                marks.push(ScannedMark { end: index + 1, mark });
            }
            index += 1;
        }
        marks
    }

    fn advance(&mut self, byte: u8) -> Option<ShellMark> {
        match (self.state, byte) {
            (ScanState::Ground, ESC) => self.state = ScanState::Escape,
            (ScanState::Ground, _) => {}
            (ScanState::Escape, OSC_INTRODUCER) => self.begin_payload(),
            (ScanState::Escape, ESC) => {}
            (ScanState::Escape, _) => self.state = ScanState::Ground,
            (ScanState::OscBody, BEL) => return self.finish_payload(),
            (ScanState::OscBody, ESC) => self.state = ScanState::OscEscape,
            (ScanState::OscBody, CANCEL | SUBSTITUTE) => self.state = ScanState::Ground,
            (ScanState::OscBody, _) => self.push_payload(byte),
            (ScanState::OscEscape, STRING_TERMINATOR) => return self.finish_payload(),
            (ScanState::OscEscape, OSC_INTRODUCER) => self.begin_payload(),
            (ScanState::OscEscape, ESC) => self.state = ScanState::Escape,
            (ScanState::OscEscape, _) => self.state = ScanState::Ground,
        }
        None
    }

    fn begin_payload(&mut self) {
        self.state = ScanState::OscBody;
        self.payload.clear();
        self.is_overflowing = false;
    }

    fn push_payload(&mut self, byte: u8) {
        if self.payload.len() >= MAX_PAYLOAD {
            self.is_overflowing = true;
            return;
        }
        self.payload.push(byte);
    }

    fn finish_payload(&mut self) -> Option<ShellMark> {
        self.state = ScanState::Ground;
        if self.is_overflowing {
            return None;
        }
        parse_mark(&self.payload)
    }
}

pub fn parse_mark(payload: &[u8]) -> Option<ShellMark> {
    let body = payload.strip_prefix(MARK_PREFIX)?;
    let text = String::from_utf8_lossy(body);
    let (kind, params) = text.split_once(';').unwrap_or((&text, ""));
    match kind {
        "A" => Some(ShellMark::PromptStart),
        "B" => Some(ShellMark::CommandStart),
        "C" => Some(ShellMark::CommandExecuted { command: command_param(params) }),
        "D" => Some(ShellMark::CommandFinished { exit_code: exit_code_param(params) }),
        _ => None,
    }
}

fn command_param(params: &str) -> Option<String> {
    let command = params.strip_prefix(COMMAND_LINE_KEY)?.trim();
    (!command.is_empty()).then(|| command.to_string())
}

fn exit_code_param(params: &str) -> Option<i32> {
    params.split(';').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all(chunks: &[&[u8]]) -> Vec<ShellMark> {
        let mut scanner = MarkScanner::default();
        chunks.iter().flat_map(|chunk| scanner.scan(chunk)).map(|scanned| scanned.mark).collect()
    }

    #[test]
    fn parses_each_final_term_mark() {
        let marks = scan_all(&[b"\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C;cmdline=ls -la\x07out\x1b]133;D;2\x07"]);
        assert_eq!(marks, vec![
            ShellMark::PromptStart,
            ShellMark::CommandStart,
            ShellMark::CommandExecuted { command: Some("ls -la".into()) },
            ShellMark::CommandFinished { exit_code: Some(2) },
        ]);
    }

    #[test]
    fn accepts_string_terminator_and_missing_params() {
        let marks = scan_all(&[b"\x1b]133;C\x1b\\\x1b]133;D\x1b\\"]);
        assert_eq!(marks, vec![
            ShellMark::CommandExecuted { command: None },
            ShellMark::CommandFinished { exit_code: None },
        ]);
    }

    #[test]
    fn marks_split_across_reads_are_found_once() {
        let marks = scan_all(&[b"text \x1b]13", b"3;D;1", b"30\x07 more"]);
        assert_eq!(marks, vec![ShellMark::CommandFinished { exit_code: Some(130) }]);
    }

    #[test]
    fn reports_the_byte_offset_just_past_the_terminator() {
        let mut scanner = MarkScanner::default();
        let bytes = b"ab\x1b]133;B\x07cd";
        let scanned = scanner.scan(bytes);
        assert_eq!(scanned.len(), 1);
        assert_eq!(&bytes[scanned[0].end..], b"cd");
    }

    #[test]
    fn ignores_other_sequences_and_unknown_marks() {
        let marks = scan_all(&[b"\x1b]0;title\x07\x1b[31mred\x1b]8;;http://x\x1b\\\x1b]133;Z\x07\x1b]1337;B\x07"]);
        assert!(marks.is_empty());
    }

    #[test]
    fn keeps_semicolons_inside_the_command_line() {
        let marks = scan_all(&[b"\x1b]133;C;cmdline=cd src; cargo test\x07"]);
        assert_eq!(marks, vec![ShellMark::CommandExecuted { command: Some("cd src; cargo test".into()) }]);
    }

    #[test]
    fn drops_oversized_payloads() {
        let mut bytes = b"\x1b]133;C;cmdline=".to_vec();
        bytes.extend(std::iter::repeat_n(b'x', MAX_PAYLOAD + 10));
        bytes.extend_from_slice(b"\x07\x1b]133;D;0\x07");
        let marks = scan_all(&[&bytes]);
        assert_eq!(marks, vec![ShellMark::CommandFinished { exit_code: Some(0) }]);
    }

    #[test]
    fn cancel_aborts_an_open_sequence() {
        let marks = scan_all(&[b"\x1b]133;A\x18\x07\x1b]133;B\x07"]);
        assert_eq!(marks, vec![ShellMark::CommandStart]);
    }
}
