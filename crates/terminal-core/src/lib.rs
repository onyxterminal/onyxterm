#![allow(unsafe_code)]
#![allow(clippy::all)]

pub mod event;
pub mod pty;
pub mod service;
pub mod vt;

pub use event::{
    Cell, CellColor, CursorPosition, GridSize, GridSnapshot, InputModes, TerminalError, TerminalEvent,
    TerminalOutput,
};
pub use pty::{NativePty, Pty, PtyHandle};
pub use service::{InMemoryTerminalService, ShellCommand, TerminalService};
pub use vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
