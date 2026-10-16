use crate::command_blocks::{BlockId, CommandBlock};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{NamedColor, Rgb};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const ANSI_COLOR_COUNT: usize = 16;
const LAST_INDEXED_COLOR: usize = 255;
const FOREGROUND_INDEX: usize = NamedColor::Foreground as usize;
const BACKGROUND_INDEX: usize = NamedColor::Background as usize;
const CURSOR_INDEX: usize = NamedColor::Cursor as usize;
const CUBE_START: u8 = 16;
const GRAYSCALE_START: u8 = 232;
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

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
    CommandStarted(BlockId),
    CommandFinished(CommandBlock),
    ClipboardStore(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPalette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub ansi: [Rgb; 16],
}

impl ColorPalette {
    pub fn color_at(&self, index: usize) -> Option<Rgb> {
        match index {
            0..ANSI_COLOR_COUNT => Some(self.ansi[index]),
            ANSI_COLOR_COUNT..=LAST_INDEXED_COLOR => Some(xterm_indexed_rgb(index as u8)),
            FOREGROUND_INDEX => Some(self.foreground),
            BACKGROUND_INDEX => Some(self.background),
            CURSOR_INDEX => Some(self.cursor),
            _ => None,
        }
    }
}

pub fn xterm_indexed_rgb(index: u8) -> Rgb {
    if index >= GRAYSCALE_START {
        let level = 8 + (index - GRAYSCALE_START) * 10;
        return Rgb { r: level, g: level, b: level };
    }
    let cube = index.saturating_sub(CUBE_START);
    let level = |n: u8| CUBE_LEVELS[(n % 6) as usize];
    Rgb { r: level(cube / 36), g: level(cube / 6), b: level(cube) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellPixels {
    pub width: u16,
    pub height: u16,
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
    pub block: Option<BlockId>,
}

impl Cell {
    pub fn empty() -> Self {
        Self { ch: ' ', fg: CellColor::Default, bg: CellColor::Default, flags: 0, block: None }
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
    pub alternate_scroll: bool,
    pub focus_reporting: bool,
    pub mouse: MouseModes,
    pub kitty: KittyKeyboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseModes {
    pub report_click: bool,
    pub report_drag: bool,
    pub report_motion: bool,
    pub sgr: bool,
    pub utf8: bool,
}

impl MouseModes {
    pub fn is_reporting(&self) -> bool {
        self.report_click || self.report_drag || self.report_motion
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyKeyboard {
    pub disambiguate: bool,
    pub report_event_types: bool,
    pub report_alternate_keys: bool,
    pub report_all_keys: bool,
    pub report_associated_text: bool,
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
    pub is_vi_mode: bool,
    pub partial_row: Option<PartialRow>,
}

#[derive(Debug, Clone)]
pub struct PartialRow {
    pub cells: Vec<Cell>,
    pub visible_fraction: f32,
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
            is_vi_mode: false,
            partial_row: None,
        }
    }
}
