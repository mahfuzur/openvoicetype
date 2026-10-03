//! The Win32 side of the hotkeys (follows `HotKey.swift`'s Carbon registration and its release events).
//!
//! The press comes from `RegisterHotKey` (MOD_NOREPEAT: holding doesn't repeat it) as WM_HOTKEY on the app's window.
//! Windows has no release event for a hotkey, so Hold to Talk arms a `WH_KEYBOARD_LL` hook on its own thread: the
//! first key-up of the hotkey's key or one of its modifiers posts `WM_HOTKEY_RELEASED`. Windows drops a hook that
//! takes more than about a second, so the callback only reads atomics and posts a message.

use super::keys::{self, WinCombo};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Once;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostMessageW, SetWindowsHookExW, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WM_APP, WM_KEYUP, WM_SYSKEYUP,
};

/// WM_HOTKEY ids on the app's window.
pub const ID_DICTATION: i32 = 1;
pub const ID_ESCAPE: i32 = 2;

/// Posted to the app's window when a watched hotkey is released.
pub const WM_HOTKEY_RELEASED: u32 = WM_APP + 1;

/// Marks the keys we send ourselves (`paste::input`), so the hook never mistakes them for the user's.
pub const INJECTED_MARK: usize = 0x4F56_5400;

/// Registers a combo; false if another app holds it.
pub fn register(hwnd: HWND, id: i32, combo: WinCombo) -> bool {
    let modifiers = HOT_KEY_MODIFIERS(combo.modifiers | keys::MOD_NOREPEAT);
    // SAFETY: plain Win32 call on our own window.
    unsafe { RegisterHotKey(Some(hwnd), id, modifiers, combo.vk).is_ok() }
}

pub fn unregister(hwnd: HWND, id: i32) {
    // SAFETY: as above; unregistering an id that isn't registered just fails.
    let _ = unsafe { UnregisterHotKey(Some(hwnd), id) };
}

static WATCH_VK: AtomicU32 = AtomicU32::new(0);
static WATCH_MODIFIERS: AtomicU32 = AtomicU32::new(0);
static NOTIFY: AtomicIsize = AtomicIsize::new(0);
static HOOK: Once = Once::new();

/// Hold to Talk: report the next release of `combo` to `hwnd`. Installs the hook the first time.
pub fn watch_release(hwnd: HWND, combo: WinCombo) {
    NOTIFY.store(hwnd.0 as isize, Ordering::SeqCst);
    WATCH_MODIFIERS.store(combo.modifiers, Ordering::SeqCst);
    WATCH_VK.store(combo.vk, Ordering::SeqCst);
    HOOK.call_once(|| {
        std::thread::Builder::new().name("keyboard-hook".into()).spawn(hook_thread).ok();
    });
    // A quick tap can be over before WM_HOTKEY reached us: then the release already happened.
    // SAFETY: reads the key state.
    if unsafe { GetAsyncKeyState(combo.vk as i32) } as u16 & 0x8000 == 0 {
        release();
    }
}

pub fn stop_watching() {
    WATCH_VK.store(0, Ordering::SeqCst);
}

fn release() {
    if WATCH_VK.swap(0, Ordering::SeqCst) != 0 {
        let hwnd = HWND(NOTIFY.load(Ordering::SeqCst) as *mut _);
        // SAFETY: posting to our own window; it may be gone at exit, which only fails the call.
        let _ = unsafe { PostMessageW(Some(hwnd), WM_HOTKEY_RELEASED, WPARAM(0), LPARAM(0)) };
    }
}

fn hook_thread() {
    // SAFETY: the hook procedure is a plain function; the hook lives as long as this thread's message loop.
    unsafe {
        let module = GetModuleHandleW(None).ok().map(|m| m.into());
        if SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0).is_err() {
            crate::applog::write("HOTKEY keyboard hook failed: Hold to Talk can't see the release");
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let key_up = wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize;
    if code == HC_ACTION as i32 && key_up {
        let vk = WATCH_VK.load(Ordering::Relaxed);
        if vk != 0 {
            // SAFETY: for WH_KEYBOARD_LL, lParam points to a KBDLLHOOKSTRUCT.
            let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let combo = WinCombo { modifiers: WATCH_MODIFIERS.load(Ordering::Relaxed), vk };
            if info.dwExtraInfo != INJECTED_MARK && keys::releases(combo, info.vkCode) {
                release();
            }
        }
    }
    // SAFETY: passes the event on, as every hook must.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
