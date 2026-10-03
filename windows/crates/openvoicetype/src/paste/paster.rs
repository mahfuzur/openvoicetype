//! Paste and copy (follows `Paster.paste` and `Paster.copy`): our text goes on the clipboard, Ctrl+V (Shift+Insert in
//! terminals) goes to the target, and the user's clipboard comes back afterwards.
//!
//! Only the latest paste restores, and only if nobody wrote to the clipboard meanwhile. A paste whose restore hasn't
//! run yet (two quick dictations) reuses its snapshot, which is the user's clipboard; ours isn't.

use super::clipboard::{self, Snapshot};
use super::input;
use super::target::Target;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use windows::Win32::Foundation::HWND;

struct Pending {
    saved: Snapshot,
    /// The clipboard's sequence number right after our write.
    sequence: u32,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// TODO(W3): delayed rendering (`SetClipboardData(format, NULL)` + WM_RENDERFORMAT on the owner window) tells us when
/// the target read the text, so the restore can run right after, as on the Mac. Until then: a fixed wait.
const RESTORE_AFTER: Duration = Duration::from_millis(500);

/// Pastes into `target` (already checked by the caller). False if the text couldn't even reach the clipboard.
pub fn paste(owner: HWND, target: &Target, text: &str, html: Option<&str>) -> bool {
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
    let saved = pending.map(|p| p.saved).unwrap_or_else(|| clipboard::snapshot(owner));
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(sequence) = clipboard::set_text(owner, text, html, true) else {
        clipboard::restore(owner, &saved);
        return false;
    };
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending { saved, sequence });

    input::wait_for_modifiers_released(Duration::from_millis(600));
    let sent = input::send_paste(target.hwnd(), target.is_terminal());

    let owner = owner.0 as isize;
    std::thread::spawn(move || {
        std::thread::sleep(RESTORE_AFTER);
        restore_if_latest(HWND(owner as *mut _), generation);
    });
    sent
}

fn restore_if_latest(owner: HWND, generation: u64) {
    if GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    let Some(pending) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    // Something else was copied meanwhile, or there's nothing to put back: leave the clipboard alone.
    if clipboard::sequence() == pending.sequence && !pending.saved.is_empty() {
        clipboard::restore(owner, &pending.saved);
    }
}

/// Puts text on the clipboard for the user to paste (focus moved, an elevated window, auto-paste off). It's the
/// user's text now: it stays, and a pending restore is dropped.
pub fn copy(owner: HWND, text: &str, html: Option<&str>) -> bool {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
    clipboard::set_text(owner, text, html, false).is_some()
}
