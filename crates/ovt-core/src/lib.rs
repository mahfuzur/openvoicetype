//! OpenVoiceType's logic that doesn't touch the desktop, ported from the macOS app (`app/Sources/VoiceToText/`).
//!
//! The Swift files are the specification: each module names the file it follows, and the tests carry the same cases as
//! `LogicSelfTest.swift`. Behaviour should stay the same on both platforms; where Linux differs (app ids, folders), the
//! module says so.

pub mod command;
pub mod dictionary;
pub mod history;
pub mod hotkey;
pub mod mode;
pub mod models;
pub mod paths;
pub mod richtext;
pub mod script;
pub mod settings;
pub mod target;
pub mod updater;
