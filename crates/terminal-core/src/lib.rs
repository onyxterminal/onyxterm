#![allow(unsafe_code)]
#![allow(clippy::all)]

pub mod event;
pub mod pty;
pub mod service;
pub mod vt;

pub use event::{Cell, GridSize, GridSnapshot, TerminalError, TerminalEvent, TerminalOutput};
pub use pty::{Pty, PtyHandle};
pub use service::{InMemoryTerminalService, TerminalService};
pub use vt::{Terminal, TerminalHandle, DEFAULT_SCROLLBACK};
