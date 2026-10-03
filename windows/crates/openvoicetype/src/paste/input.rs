//! Synthetic key presses (follows `Paster.whenModifiersReleased`, `Paster.sendShortcut` and `KeyboardLayout`): wait
//! until the hotkey's modifiers are up, then Ctrl+V with the key that types "v" in the target's keyboard layout, or
//! Shift+Insert in a terminal.

use crate::hotkey::INJECTED_MARK;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, SendInput, VkKeyScanExW, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// A held Ctrl+Alt would turn our Ctrl+V into Ctrl+Alt+V. Waits up to `timeout`; true if they're all up.
pub fn wait_for_modifiers_released(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        // SAFETY: reads the key state.
        let held = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } as u16 & 0x8000 != 0);
        if !held {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Sends the paste chord to whatever has focus; false if Windows dropped it (UIPI: an elevated window).
pub fn send_paste(target: HWND, terminal: bool) -> bool {
    if terminal {
        chord(VK_SHIFT, VK_INSERT, KEYEVENTF_EXTENDEDKEY)
    } else {
        chord(VK_CONTROL, layout_key('v', target), KEYBD_EVENT_FLAGS(0))
    }
}

/// The virtual key that types `ch` in the layout of the window's thread (Dvorak, AZERTY…); the US key otherwise.
fn layout_key(ch: char, window: HWND) -> VIRTUAL_KEY {
    // SAFETY: plain Win32 calls; a stale window gives thread 0, the current layout.
    let scan = unsafe {
        let thread = GetWindowThreadProcessId(window, None);
        VkKeyScanExW(ch as u16, GetKeyboardLayout(thread))
    };
    if scan == -1 || (scan >> 8) & 0x06 != 0 {
        // Not on this layout, or only with Ctrl/Alt (AltGr) held.
        VIRTUAL_KEY(ch.to_ascii_uppercase() as u16)
    } else {
        VIRTUAL_KEY((scan & 0xFF) as u16)
    }
}

fn key(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: INJECTED_MARK },
        },
    }
}

fn chord(modifier: VIRTUAL_KEY, main: VIRTUAL_KEY, main_flags: KEYBD_EVENT_FLAGS) -> bool {
    let inputs = [
        key(modifier, KEYBD_EVENT_FLAGS(0)),
        key(main, main_flags),
        key(main, main_flags | KEYEVENTF_KEYUP),
        key(modifier, KEYEVENTF_KEYUP),
    ];
    // SAFETY: a valid array of keyboard inputs.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent == inputs.len() as u32
}
