//! The Win32 clipboard (the pasteboard half of `Paster.swift`): a snapshot of every format that is plain memory, put
//! back after the paste, and our text with "HTML Format" and the formats that keep it out of clipboard history,
//! cloud sync and clipboard managers.

use super::formats::{self, CF_UNICODETEXT};
use std::time::Duration;
use windows::core::HSTRING;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

/// The clipboard's contents: (format, bytes) in the order the owner offered them.
#[derive(Default)]
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

impl Snapshot {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Holds the clipboard open; closes it when dropped.
struct Open;

impl Open {
    /// Another app can hold the clipboard for a moment: retry for up to ~0.25 s.
    fn new(owner: HWND) -> Option<Open> {
        for _ in 0..12 {
            // SAFETY: plain Win32 call; `owner` is our own window.
            if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
                return Some(Open);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: we opened it.
        let _ = unsafe { CloseClipboard() };
    }
}

/// The id of a registered format such as "HTML Format" (0 on failure).
pub fn format_id(name: &str) -> u32 {
    // SAFETY: plain Win32 call with a valid string.
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
}

/// Changes on every write to the clipboard, by anyone.
pub fn sequence() -> u32 {
    // SAFETY: plain Win32 call.
    unsafe { GetClipboardSequenceNumber() }
}

pub fn snapshot(owner: HWND) -> Snapshot {
    let Some(_open) = Open::new(owner) else { return Snapshot::default() };
    let mut saved = Vec::new();
    let mut format = 0;
    loop {
        // SAFETY: the clipboard is open; 0 ends the list.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if formats::is_memory_format(format) {
            if let Some(bytes) = read(format) {
                saved.push((format, bytes));
            }
        }
    }
    Snapshot(saved)
}

/// Copies one format's global memory.
fn read(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open; the handle is global memory for these formats, valid until it closes.
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let memory = HGLOBAL(handle.0);
        let size = GlobalSize(memory);
        let pointer = GlobalLock(memory) as *const u8;
        if pointer.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(pointer, size).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
    }
}

/// Hands `bytes` to the clipboard as `format` (the clipboard owns the memory once that succeeds).
fn write(format: u32, bytes: &[u8]) -> bool {
    // SAFETY: the clipboard is open and emptied by us; the memory is ours until SetClipboardData takes it.
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
        let pointer = GlobalLock(memory) as *mut u8;
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        let _ = GlobalUnlock(memory);
        if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        true
    }
}

/// Puts back what `snapshot` saved. The restored copy stays out of the history (it's already there).
pub fn restore(owner: HWND, snapshot: &Snapshot) {
    let Some(_open) = Open::new(owner) else { return };
    // SAFETY: the clipboard is open.
    if unsafe { EmptyClipboard() }.is_err() {
        return;
    }
    for (format, bytes) in &snapshot.0 {
        write(*format, bytes);
    }
    for (name, data) in &formats::privacy_formats()[1..] {
        let id = format_id(name);
        if id != 0 && !snapshot.0.iter().any(|(format, _)| *format == id) {
            write(id, data);
        }
    }
}

/// Replaces the clipboard with `text` (and HTML). `private`: our transient paste, kept out of the history, cloud sync
/// and clipboard managers; a copy for the user to paste is theirs and isn't. Returns the clipboard's sequence number
/// after the write, or None if the clipboard couldn't be opened.
pub fn set_text(owner: HWND, text: &str, html: Option<&str>, private: bool) -> Option<u32> {
    let open = Open::new(owner)?;
    // SAFETY: the clipboard is open.
    unsafe { EmptyClipboard() }.ok()?;
    let mut ok = write(CF_UNICODETEXT, &formats::unicode_text(text));
    if let Some(html) = html {
        let id = format_id(formats::HTML);
        ok &= id != 0 && write(id, &formats::cf_html(html));
    }
    if private {
        for (name, data) in formats::privacy_formats() {
            let id = format_id(name);
            if id != 0 {
                write(id, &data);
            }
        }
    }
    drop(open);
    ok.then(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    };

    fn current(owner: HWND, format: u32) -> Option<Vec<u8>> {
        let _open = Open::new(owner)?;
        read(format)
    }

    /// Uses the real clipboard, so it's run by hand on Windows: `cargo test -p openvoicetype-windows -- --ignored`.
    #[test]
    #[ignore]
    fn snapshot_and_restore() {
        // SAFETY: a message-only STATIC window as the clipboard's owner, destroyed at the end.
        let owner = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                None,
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .unwrap();
        assert!(set_text(owner, "the user's", None, false).is_some());
        let saved = snapshot(owner);
        assert!(!saved.is_empty());
        let sequence = set_text(owner, "ours\nline", Some("<ul><li>x</li></ul>"), true).unwrap();
        assert_eq!(sequence, super::sequence());
        assert_eq!(current(owner, CF_UNICODETEXT), Some(formats::unicode_text("ours\nline")));
        assert_eq!(current(owner, format_id(formats::IN_HISTORY)), Some(vec![0, 0, 0, 0]));
        assert!(current(owner, format_id(formats::HTML)).is_some_and(|html| html.starts_with(b"Version:0.9")));
        restore(owner, &saved);
        assert_eq!(current(owner, CF_UNICODETEXT), Some(formats::unicode_text("the user's")));
        assert!(current(owner, format_id(formats::EXCLUDE_FROM_MONITORS)).is_none());
        // SAFETY: ours.
        unsafe { DestroyWindow(owner) }.unwrap();
    }
}
