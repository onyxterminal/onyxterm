#![allow(unsafe_code)]
#![allow(clippy::all)]

pub mod command_blocks;
pub mod event;
pub mod pty;
pub mod service;
pub mod shell_marks;
pub mod vt;

pub use event::{
    Cell, CellColor, CursorPosition, GridPoint, GridSize, GridSnapshot, InputModes, SelectionKind, SelectionSpan,
    TerminalError, TerminalEvent, TerminalOutput,
};
pub use command_blocks::{BlockId, BlockOutcome, BlockStatus, CommandBlock, CommandBlocks, ShellPhase};
pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::vi_mode::ViMotion;
pub use pty::{ForegroundJob, NativePty, Pty, PtyHandle};
pub use tokio::sync::broadcast;
pub use service::{InMemoryTerminalService, ShellCommand, TerminalService};
pub use vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
