#![deny(warnings)]
#![forbid(unsafe_code)]

mod buffer;
mod edit;
mod editor;
mod history;
mod motion;
mod selection;

pub use buffer::{line_len, Buffer, LineEnding};
pub use edit::{Bias, Change, Edit};
pub use editor::{Editor, EditorAction, Granularity};
pub use motion::{char_column, display_column, move_selection, Motion, TAB_WIDTH};
pub use selection::{Selection, Selections};
pub use ropey::Rope;
