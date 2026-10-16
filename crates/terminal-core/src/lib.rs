#![allow(unsafe_code)]
#![allow(clippy::all)]

pub mod command_blocks;
pub mod event;
pub mod pty;
pub mod search;
pub mod service;
pub mod shell_integration;
pub mod shell_marks;
pub mod vt;

pub use event::{
    xterm_indexed_rgb, Cell, CellColor, CellPixels, ColorPalette, CursorPosition, GridPoint, GridSize, GridSnapshot,
    InputModes, KittyKeyboard, MouseModes, PartialRow, SelectionKind, SelectionSpan, TerminalError, TerminalEvent,
    TerminalOutput, DEFAULT_CURSOR_STYLE,
};
pub use command_blocks::{BlockId, BlockOutcome, BlockStatus, CommandBlock, CommandBlocks, ShellPhase};
pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::vi_mode::ViMotion;
pub use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Rgb};
pub use search::{MatchPoint, TextMatch};
pub use pty::{ForegroundJob, NativePty, Pty, PtyHandle};
pub use tokio::sync::broadcast;
pub use service::{InMemoryTerminalService, ShellCommand, TerminalService};
pub use shell_integration::ShellIntegration;
pub use vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
