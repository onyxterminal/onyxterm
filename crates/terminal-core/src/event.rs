use alacritty_terminal::term::cell::Flags;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("pty spawn failed: {0}")]
    PtySpawn(String),
    #[error("pty write failed: {0}")]
    PtyWrite(String),
    #[error("pty resize failed: {0}")]
    PtyResize(String),
    #[error("pty read failed: {0}")]
    PtyRead(String),
    #[error("terminal already terminated")]
    Terminated,
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl GridSize {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalOutput {
    pub bytes: Vec<u8>,
}

impl TerminalOutput {
    pub fn empty() -> Self {
        Self { bytes: Vec::new() }
    }
}

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(TerminalOutput),
    Resized { cols: u16, rows: u16 },
    Exited { code: i32 },
    TitleChanged(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: CellColor,
    pub bg: CellColor,
    pub flags: u16,
}

impl Cell {
    pub fn empty() -> Self {
        Self { ch: ' ', fg: CellColor::Default, bg: CellColor::Default, flags: 0 }
    }

    pub fn is_bold(&self) -> bool {
        self.has_flag(Flags::BOLD)
    }

    pub fn is_inverse(&self) -> bool {
        self.has_flag(Flags::INVERSE)
    }

    pub fn is_underline(&self) -> bool {
        self.has_flag(Flags::UNDERLINE)
    }

    fn has_flag(&self, flag: Flags) -> bool {
        Flags::from_bits_truncate(self.flags).contains(flag)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub row: u16,
    pub col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputModes {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
}

#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub cells: Vec<Cell>,
    pub cursor: Option<CursorPosition>,
}

impl GridSnapshot {
    pub fn empty(size: GridSize) -> Self {
        let count = (size.rows as usize) * (size.cols as usize);
        Self {
            rows: size.rows,
            cols: size.cols,
            cells: vec![Cell::empty(); count],
            cursor: None,
        }
    }
}
