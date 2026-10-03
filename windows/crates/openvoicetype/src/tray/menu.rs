//! The tray icon and menu through `tray-icon` + `muda` (follows `MenuBarIcon.swift` and the Mac app's short menu).
//! Menu clicks arrive through `muda::MenuEvent`'s handler, which the app forwards to its window.

use super::{icon, Action, ENGINES};
use ovt_core::mode::DictationMode;
use ovt_core::settings::Settings;
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    modes: Vec<(Option<DictationMode>, CheckMenuItem)>,
    engines: Vec<(&'static str, CheckMenuItem)>,
    busy: bool,
}

impl Tray {
    pub fn new(settings: &Settings, status: &str) -> Result<Tray, String> {
        let status = MenuItem::with_id("status", status, false, None);
        let modes: Vec<_> = std::iter::once(None)
            .chain(DictationMode::ALL.map(Some))
            .map(|mode| {
                let title = mode.map_or("Auto (from the app in front)", |m| m.title());
                (mode, CheckMenuItem::with_id(Action::Mode(mode).id(), title, true, false, None))
            })
            .collect();
        let engines: Vec<_> = ENGINES
            .iter()
            .map(|(engine, title)| {
                (*engine, CheckMenuItem::with_id(Action::Engine(engine.to_string()).id(), title, true, false, None))
            })
            .collect();
        let mode_menu = Submenu::new("Mode", true);
        for (_, item) in &modes {
            mode_menu.append(item).map_err(|e| e.to_string())?;
        }
        let engine_menu = Submenu::new("Cleanup", true);
        for (_, item) in &engines {
            engine_menu.append(item).map_err(|e| e.to_string())?;
        }
        let item = |action: Action, title: &str| MenuItem::with_id(action.id(), title, true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &status,
            &PredefinedMenuItem::separator(),
            &mode_menu,
            &engine_menu,
            &PredefinedMenuItem::separator(),
            &item(Action::CopyLast, "Copy Last Dictation"),
            &item(Action::Settings, "Settings…"),
            &item(Action::OpenLog, "Open Log"),
            &PredefinedMenuItem::separator(),
            &item(Action::Quit, "Quit OpenVoiceType"),
        ])
        .map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("OpenVoiceType")
            .with_icon(make_icon(false)?)
            .build()
            .map_err(|e| e.to_string())?;
        let tray = Tray { icon, status, modes, engines, busy: false };
        tray.update(settings);
        Ok(tray)
    }

    /// Ticks the current mode and engine (muda toggles a clicked item itself, so all are set every time).
    pub fn update(&self, settings: &Settings) {
        for (mode, item) in &self.modes {
            item.set_checked(*mode == settings.mode_override);
        }
        for (engine, item) in &self.engines {
            item.set_checked(*engine == settings.cleanup_engine);
        }
    }

    pub fn set_status(&self, text: &str) {
        self.status.set_text(text);
    }

    /// The red dot while recording, transcribing or polishing.
    pub fn set_busy(&mut self, busy: bool) {
        if busy == self.busy {
            return;
        }
        self.busy = busy;
        if let Ok(icon) = make_icon(busy) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let _ = self.icon.set_tooltip(Some(if busy { "OpenVoiceType: working" } else { "OpenVoiceType" }));
    }
}

fn make_icon(busy: bool) -> Result<Icon, String> {
    Icon::from_rgba(icon::rgba(busy, light_taskbar()), icon::SIZE as u32, icon::SIZE as u32).map_err(|e| e.to_string())
}

/// The taskbar follows "Choose your default Windows mode" (light or dark).
fn light_taskbar() -> bool {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: a DWORD read into a DWORD-sized buffer.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 1
}
