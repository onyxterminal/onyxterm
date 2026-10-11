#![allow(unsafe_code)]
#![allow(clippy::all)]

pub mod event;
pub mod pty;
pub mod service;
pub mod vt;

pub use event::{
    Cell, CellColor, CursorPosition, GridPoint, GridSize, GridSnapshot, InputModes, SelectionKind, SelectionSpan,
    TerminalError, TerminalEvent, TerminalOutput,
};
pub use alacritty_terminal::index::Side;
pub use pty::{ForegroundJob, NativePty, Pty, PtyHandle};
pub use tokio::sync::broadcast;
pub use service::{InMemoryTerminalService, ShellCommand, TerminalService};
pub use vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
