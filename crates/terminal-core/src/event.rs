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

    pub fn is_italic(&self) -> bool {
        self.has_flag(Flags::ITALIC)
    }

    pub fn is_dim(&self) -> bool {
        self.has_flag(Flags::DIM)
    }

    pub fn is_strikeout(&self) -> bool {
        self.has_flag(Flags::STRIKEOUT)
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
    pub alt_screen: bool,
    pub mouse_reporting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPoint {
    pub row: u16,
    pub col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionSpan {
    pub start: GridPoint,
    pub end: GridPoint,
}

impl SelectionSpan {
    pub fn columns_on(&self, row: u16, cols: u16) -> Option<std::ops::Range<u16>> {
        if row < self.start.row || row > self.end.row {
            return None;
        }
        let first = if row == self.start.row { self.start.col } else { 0 };
        let last = if row == self.end.row { self.end.col.saturating_add(1).min(cols) } else { cols };
        (first < last).then_some(first..last)
    }
}

#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub cells: Vec<Cell>,
    pub cursor: Option<CursorPosition>,
    pub selection: Option<SelectionSpan>,
}

impl GridSnapshot {
    pub fn empty(size: GridSize) -> Self {
        let count = (size.rows as usize) * (size.cols as usize);
        Self {
            rows: size.rows,
            cols: size.cols,
            cells: vec![Cell::empty(); count],
            cursor: None,
            selection: None,
        }
    }
}
