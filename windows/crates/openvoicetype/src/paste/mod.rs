//! Where the text goes and how it gets there (follows `Paster.swift` and `PasteTarget.swift`): the target captured
//! when recording starts and checked before pasting, the clipboard snapshot and restore, and the Ctrl+V.

pub mod apps;
pub mod formats;

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod paster;
#[cfg(windows)]
pub mod target;

#[cfg(windows)]
pub use paster::{copy, paste};
