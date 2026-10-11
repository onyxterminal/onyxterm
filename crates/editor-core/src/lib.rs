#![deny(warnings)]
#![forbid(unsafe_code)]

mod buffer;
mod edit;
mod selection;

pub use buffer::{line_len, Buffer, LineEnding};
pub use edit::{Bias, Change, Edit};
pub use selection::{Selection, Selections};
pub use ropey::Rope;
