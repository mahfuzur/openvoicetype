//! The floating pill (follows `Overlay.swift`): what it shows (`scene`), how it's painted (`canvas`), and the
//! layered, click-through, never-focused window it lives in.

pub mod canvas;
pub mod scene;

#[cfg(windows)]
mod text;
#[cfg(windows)]
mod window;
#[cfg(windows)]
pub use window::Overlay;
