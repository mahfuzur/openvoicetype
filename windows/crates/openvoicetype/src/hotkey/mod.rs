//! Global hotkeys (follows `HotKey.swift`): `RegisterHotKey` for the press, and for Hold to Talk a low-level keyboard
//! hook that reports the release. Esc is registered only while recording, to cancel.

pub mod keys;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;
