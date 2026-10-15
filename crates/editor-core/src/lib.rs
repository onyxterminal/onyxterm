#![deny(warnings)]
#![forbid(unsafe_code)]

mod buffer;
mod diff;
mod edit;
mod editor;
mod history;
mod indentation;
mod keys;
mod motion;
mod rope_compare;
mod selection;
pub mod io;
pub mod syntax;

pub use buffer::{line_len, Buffer, LineEnding};
pub use diff::{diff_lines, line_marks, DiffSource, Hunk, HunkKind, LineDiff, LineMark};
pub use edit::{Bias, Change, Edit};
pub use editor::{ClickMode, Editor, EditorAction, Granularity};
pub use indentation::{detect_indentation, Indentation};
pub use keys::{standard_action, KeyInput};
pub use motion::{char_column, display_column, move_selection, Motion, TAB_WIDTH};
pub use selection::{Selection, Selections};
pub use ropey::Rope;
