#![deny(warnings)]
#![forbid(unsafe_code)]

mod files;
mod listing;
mod ops;
mod watch;

pub use files::list_files;
pub use listing::{list_dir, natural_cmp, Entry};
pub use ops::{create_dir, create_file, rename, trash, validate_name, OpError};
pub use watch::{Watcher, DEFAULT_DEBOUNCE};
