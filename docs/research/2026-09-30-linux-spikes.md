# Linux P1 spikes (M7)

What each P1 spike in [plans/M7-linux.md](../plans/M7-linux.md) found. Results from the maintainer's PC (Ubuntu 26.04.1,
GNOME Shell 50.1, Wayland, Intel UHD 770) are added as they come in.

## Spike 1: the GNOME Shell extension

Code: `linux/spikes/gnome-shell/`. The extension offers the app a D-Bus service (`io.github.mahfuzur.OpenVoiceType.Shell`).
`headless/run.sh` runs it inside a headless GNOME Shell 50 in Docker (Ubuntu 26.04, a system bus and python-dbusmock's
logind) and probes it, so changes can be checked without logging out of a desktop. `spike.py` is the guided test on a
real desktop.

**In the headless GNOME Shell 50.1 (2026-09-30):**

| Need | Result |
|---|---|
| Hotkey press **and release** (hold to talk) | ✅ with `Meta.KeyBindingFlags.TRIGGER_RELEASE` (128): `accelerator-activated` on press, `accelerator-deactivated` on release (0.41 s for a 0.4 s hold). Without that flag, or with `IGNORE_AUTOREPEAT`, there is no release signal. |
| Esc only while recording | ✅ grab and ungrab work |
| Focused window | ✅ pid, app id (`org.gnome.TextEditor.desktop`), a stable window id (`Meta.Window.get_id()`), title, WM class |
| Clipboard write and read | ✅ through `St.Clipboard` (`text/plain;charset=utf-8`), Unicode kept |
| Clipboard with several types (text/html for rich paste) and a "was read" signal | ❌ a `Meta.SelectionSource` subclass can't be written in GJS: "VFunc read_async accepts another callback as a parameter. This is not supported". So on GNOME: plain text only, and the clipboard is restored after a fixed delay. Rich paste to try another way (an X11 client owning CLIPBOARD through Xwayland, which mutter bridges). |
| Typing and pasting through a virtual keyboard | ✅ `Clutter` virtual keyboard with key symbols: typed text and Ctrl+V reach GNOME Text Editor. Ctrl+A then Ctrl+C sent back to back lost the Ctrl+C; a short pause fixed it. |
| Modifier state | ✅ `global.get_pointer()` |
| Password field | ✅ readable: GNOME Shell keeps the focused field's purpose in `Main.inputMethod._purpose` (IBus: 8 = password, 10 = terminal); a private field, so it needs checking on every GNOME release |
| Overlay | ✅ shows through `Main.layoutManager.addTopChrome`. GNOME 50 removed the `affectsInputRegion` option; a non-reactive actor lets clicks through. |
| Panel icon | ✅ `PanelMenu.Button` |

Also found: a window opened without a user action doesn't get focus in GNOME 50 (focus-stealing prevention), which only
matters for tests.

**On the real PC:** to do (`spike.py`: a real key hold, Ctrl+Shift+V in Ptyxis, Firefox's password field, PRIMARY, whether
the overlay is visible and keeps focus, the panel icon, and behaviour after logging out and in).
