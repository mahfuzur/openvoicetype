//! `OpenVoiceType.exe --logic-selftest [report.txt]` (follows `LogicSelfTest.swift`): checks that need no keystrokes,
//! microphone or window, one `OK name` / `FAIL name: why` line each, plus `INFO` lines about what's installed. The
//! exit code is 1 if anything failed.

use crate::hotkey::keys;
use crate::overlay::canvas::{Canvas, Mask};
use crate::overlay::scene::{Phase, Scene, TextRenderer, HEIGHT, WIDTH};
use crate::paste::{apps, formats};
use crate::recorder::wav;
use crate::{pipeline, tray};
use ovt_core::hotkey::Combo;
use ovt_core::mode::DictationMode;
use std::collections::HashMap;
use std::path::Path;

type Check = Result<(), String>;
type Named = (&'static str, fn() -> Check);

fn ensure(ok: bool, what: impl Into<String>) -> Check {
    if ok {
        Ok(())
    } else {
        Err(what.into())
    }
}

pub fn run(report: Option<&Path>) -> i32 {
    let checks: [Named; 8] = [
        ("hotkey-combos", combos),
        ("terminals", terminals),
        ("modes", modes),
        ("clipboard-formats", clipboard_formats),
        ("wav-round-trip", wav_round_trip),
        ("overlay-frame", overlay_frame),
        ("tray-icon", tray_icon),
        ("target-capture", target_capture),
    ];
    let mut lines: Vec<String> = checks
        .iter()
        .map(|(name, check)| match check() {
            Ok(()) => format!("OK {name}"),
            Err(why) => format!("FAIL {name}: {why}"),
        })
        .collect();
    lines.extend(installed());
    let text = lines.join("\n") + "\n";
    print!("{text}");
    if let Some(path) = report {
        if let Err(error) = std::fs::write(path, &text) {
            eprintln!("can't write {}: {error}", path.display());
        }
    }
    i32::from(lines.iter().any(|line| line.starts_with("FAIL")))
}

fn combos() -> Check {
    let dictation = keys::win_combo(&Combo::new(ovt_core::hotkey::DEFAULT_DICTATION));
    ensure(
        dictation == Some(keys::WinCombo { modifiers: keys::MOD_CONTROL | keys::MOD_ALT, vk: 0x20 }),
        "Ctrl+Alt+Space",
    )?;
    ensure(keys::win_combo(&Combo::new("<Super>F9")).is_some_and(|c| c.vk == 0x78), "Win+F9")?;
    ensure(keys::win_combo(&Combo::new("<Hyper>x")).is_none(), "an unknown modifier is refused")?;
    let combo = dictation.unwrap_or(keys::WinCombo { modifiers: 0, vk: 0 });
    ensure(keys::releases(combo, 0xA3) && !keys::releases(combo, 0xA0), "release of right Ctrl, not Shift")
}

fn terminals() -> Check {
    ensure(apps::is_terminal("windowsterminal", ""), "Windows Terminal")?;
    ensure(apps::is_terminal("x", "ConsoleWindowClass"), "a console window")?;
    ensure(apps::is_terminal("mintty", "") && apps::is_terminal("wezterm-gui", ""), "mintty, WezTerm")?;
    ensure(!apps::is_terminal("code", "Chrome_WidgetWin_1"), "VS Code isn't one")
}

fn modes() -> Check {
    let none = HashMap::new();
    let cases = [
        ("Slack.exe", DictationMode::Chat),
        ("OUTLOOK.EXE", DictationMode::Email),
        ("Code.exe", DictationMode::Code),
        ("WINWORD.EXE", DictationMode::Notes),
        ("msedge.exe", DictationMode::Default),
    ];
    for (exe, mode) in cases {
        ensure(apps::mode_for(exe, &none) == mode, format!("{exe} → {}", mode.as_str()))?;
    }
    Ok(())
}

fn clipboard_formats() -> Check {
    ensure(formats::unicode_text("a\n").len() == 8, "CF_UNICODETEXT with CRLF and NUL")?;
    ensure(String::from_utf8_lossy(&formats::cf_html("<b>x</b>")).contains("StartFragment:"), "CF_HTML header")?;
    #[cfg(windows)]
    for name in [formats::HTML, formats::EXCLUDE_FROM_MONITORS, formats::IN_HISTORY, formats::UPLOAD_TO_CLOUD] {
        ensure(crate::paste::clipboard::format_id(name) >= 0xC000, format!("RegisterClipboardFormat(\"{name}\")"))?;
    }
    Ok(())
}

fn wav_round_trip() -> Check {
    let path = std::env::temp_dir().join(format!("ovt-selftest-{}.wav", std::process::id()));
    let tone: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.06).sin() * 0.4).collect();
    wav::write_wav(&path, &tone, 44_100).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    ensure(wav::read_header(&bytes) == Some((16_000, 1, 16, 32_000)), format!("header {:?}", wav::read_header(&bytes)))
}

/// Every character a 7 × 13 block: the layout without GDI.
struct Blocks;

impl TextRenderer for Blocks {
    fn measure(&mut self, text: &str) -> (f32, f32) {
        (text.chars().count() as f32 * 7.0, 13.0)
    }
    fn render(&mut self, text: &str) -> Mask {
        let width = text.chars().count() * 7;
        Mask { width, height: 13, coverage: vec![255; width * 13] }
    }
}

fn overlay_frame() -> Check {
    let mut scene = Scene::default();
    let mut canvas = Canvas::new((WIDTH * 1.5) as usize, (HEIGHT * 1.5) as usize);
    let now = std::time::Instant::now();
    for phase in [Phase::Recording, Phase::Transcribing, Phase::Success("Pasted".into())] {
        scene.set_phase(phase, now);
        scene.draw(&mut canvas, 1.5, now, &mut Blocks);
        ensure(canvas.pixels.iter().any(|p| p >> 24 > 200), "the pill is drawn")?;
    }
    Ok(())
}

fn tray_icon() -> Check {
    let (idle, busy) = (tray::icon::rgba(false, false), tray::icon::rgba(true, false));
    ensure(idle.len() == tray::icon::SIZE * tray::icon::SIZE * 4 && idle != busy, "idle and busy icons differ")
}

#[cfg(windows)]
fn target_capture() -> Check {
    use crate::paste::target;
    let me = std::process::id();
    let path = target::process_path(me).unwrap_or_default();
    ensure(path.to_lowercase().ends_with(".exe"), format!("own process path \"{path}\""))?;
    ensure(!target::is_elevated(0), "pid 0 isn't elevated")?;
    let _ = target::capture(); // the foreground window, whatever it is: must not fail
    Ok(())
}

#[cfg(not(windows))]
fn target_capture() -> Check {
    Ok(())
}

/// What the app will find: helpers, the model, Claude.
fn installed() -> Vec<String> {
    let settings = ovt_core::settings::Settings::load(&ovt_core::paths::settings_file());
    let helpers = pipeline::helpers_dir();
    let mut lines: Vec<String> = ["whisper-server.exe", "whisper-cli.exe", "llama-server.exe"]
        .iter()
        .map(|name| {
            let path = pipeline::helper(&helpers, name);
            format!("INFO {name} {}", if path.is_file() { "found" } else { "missing" })
        })
        .collect();
    let model = pipeline::whisper_model(&settings);
    lines.push(format!("INFO model {} {}", model.display(), if model.is_file() { "found" } else { "missing" }));
    let claude = pipeline::find_claude(|p| p.is_file(), |name| std::env::var_os(name));
    lines.push(format!("INFO claude {}", claude.map_or("not found".into(), |p| p.display().to_string())));
    lines.push(format!("INFO settings {}", ovt_core::paths::settings_file().display()));
    lines.push(format!("INFO log {}", ovt_core::paths::log_file().display()));
    lines
}

#[cfg(test)]
mod tests {
    #[test]
    fn passes() {
        let report = std::env::temp_dir().join(format!("ovt-selftest-{}.txt", std::process::id()));
        assert_eq!(super::run(Some(&report)), 0);
        let text = std::fs::read_to_string(&report).unwrap();
        assert!(text.lines().filter(|l| l.starts_with("OK ")).count() == 8, "{text}");
        std::fs::remove_file(report).unwrap();
    }
}
