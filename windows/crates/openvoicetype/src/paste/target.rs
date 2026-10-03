//! The paste target (follows `PasteTarget.swift`): the foreground window, its process and title, whether a password
//! box has focus (UI Automation's `IsPassword`), and whether the process runs elevated, which UIPI keeps our keys
//! from reaching. Captured when recording starts and again just before pasting; the rules are `ovt_core::target`'s.

use super::apps;
use ovt_core::target::{PasteTarget, TargetCheck};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

#[derive(Clone, Debug)]
pub struct Target {
    /// What `ovt_core::target` compares.
    pub target: PasteTarget,
    /// The foreground window (as a number, so the target can cross threads).
    pub hwnd: isize,
    /// `code`, `windowsterminal`… (`apps::app_id`).
    pub app_id: String,
    pub window_class: String,
    /// The process runs as administrator (or we can't tell, which means it's protected): our keys can't reach it.
    pub elevated: bool,
}

/// What to do with the text, decided just before pasting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    Same,
    /// Focus moved: copy, and say where to.
    Changed(String),
    /// A password box has focus: neither paste nor copy.
    Secure,
    /// Same place, but it runs as administrator: copy.
    Elevated,
}

impl Target {
    pub fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    pub fn is_terminal(&self) -> bool {
        apps::is_terminal(&self.app_id, &self.window_class)
    }

    pub fn is_code(&self) -> bool {
        apps::is_code(&self.app_id) || self.is_terminal()
    }

    /// Compares with what has focus now. Editors and terminals retitle themselves while they work, so there the same
    /// window is enough (`ovt_core`'s tables don't know Windows exe names yet).
    pub fn check(&self) -> Check {
        let now = capture();
        let verdict = if now.target.is_secure {
            TargetCheck::Secure
        } else if self.is_code() && now.hwnd == self.hwnd && now.target.pid == self.target.pid {
            TargetCheck::Same
        } else {
            self.target.compare(&now.target)
        };
        match verdict {
            TargetCheck::Secure => Check::Secure,
            TargetCheck::Changed(place) => Check::Changed(place),
            TargetCheck::Same if now.elevated && !self_elevated() => Check::Elevated,
            TargetCheck::Same => Check::Same,
        }
    }
}

pub fn capture() -> Target {
    // SAFETY: plain Win32 queries; every one copes with a window that just closed.
    let hwnd = unsafe { GetForegroundWindow() };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let exe = process_path(pid).unwrap_or_default();
    let app_id = apps::app_id(&exe);
    let name = exe.rsplit('\\').next().unwrap_or_default();
    let app_name = name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name).to_string();
    let target = PasteTarget {
        pid,
        app_id: Some(app_id.clone()),
        app_name,
        window: (!hwnd.is_invalid()).then(|| format!("{:x}", hwnd.0 as usize)),
        window_title: Some(window_text(hwnd)),
        is_secure: focus_is_password(),
    };
    Target { target, hwnd: hwnd.0 as isize, app_id, window_class: class_name(hwnd), elevated: is_elevated(pid) }
}

fn window_text(hwnd: HWND) -> String {
    // SAFETY: the buffer is sized from the length query (plus the NUL).
    unsafe {
        let length = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        let copied = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: a fixed buffer; the call returns how much it filled.
    let copied = unsafe { GetClassNameW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}

/// A process handle closed when dropped.
struct Process(HANDLE);

impl Process {
    fn open(pid: u32) -> Option<Process> {
        // SAFETY: plain Win32 call; limited information is granted for most processes, elevated ones included.
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok().map(Process)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: we opened it.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// `C:\…\Code.exe`.
pub fn process_path(pid: u32) -> Option<String> {
    let process = Process::open(pid)?;
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    // SAFETY: `size` is the buffer's length and comes back as the characters written.
    unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size) }.ok()?;
    Some(String::from_utf16_lossy(&buffer[..size as usize]))
}

/// True when the process's token is elevated, or when we may not even read it (access denied: an elevated or
/// protected process). The desktop's own pid 0 isn't.
pub fn is_elevated(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    match Process::open(pid) {
        Some(process) => token_elevated(process.0).unwrap_or(true),
        None => true,
    }
}

/// We run elevated ourselves: then UIPI lets our keys through.
pub fn self_elevated() -> bool {
    // SAFETY: the pseudo-handle of our own process needs no closing.
    token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    // SAFETY: the token handle is closed below; the output buffer is a TOKEN_ELEVATION.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        result.ok().map(|_| elevation.TokenIsElevated != 0)
    }
}

thread_local! {
    /// One UI Automation client per thread (the thread must have called CoInitializeEx).
    static AUTOMATION: Option<IUIAutomation> =
        // SAFETY: plain COM activation.
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok();
}

/// The focused element is a password box. False when unknown (no UI Automation, an app that exposes none): like the
/// Mac without Accessibility, dictation is refused only when it's known.
pub fn focus_is_password() -> bool {
    AUTOMATION.with(|automation| {
        let Some(automation) = automation else { return false };
        // SAFETY: COM calls on a live client; failures read as "not a password box".
        unsafe { automation.GetFocusedElement().and_then(|element| element.CurrentIsPassword()) }
            .is_ok_and(|is| is.as_bool())
    })
}
