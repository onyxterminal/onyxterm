#![deny(warnings)]
#![forbid(unsafe_code)]

mod buffer;
mod edit;

pub use buffer::{line_len, Buffer, LineEnding};
pub use edit::{Bias, Change, Edit};
pub use ropey::Rope;
