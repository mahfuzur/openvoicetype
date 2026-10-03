#!/usr/bin/python3
"""OpenVoiceType P1 spike 1, on a real GNOME desktop: does the extension do what the Linux app needs?

Run it in a terminal after installing the extension (see install.sh) and logging out and in once:

    /usr/bin/python3 linux/spikes/gnome-shell/spike.py

It asks you to do one thing at a time (click a window, hold a key...), checks what the extension reports, and writes
~/openvoicetype-spike1.txt. Send that file back. Nothing is recorded or sent anywhere.
"""

import os
import platform
import subprocess
import sys
import time

try:
    from gi.repository import Gio, GLib
except ImportError:
    sys.exit("This python3 has no GNOME bindings (gi). Run it with Ubuntu's own Python:\n"
             "    /usr/bin/python3 linux/spikes/gnome-shell/spike.py\n"
             "(if that fails too: sudo apt install python3-gi)")

BUS_NAME = "io.github.mahfuzur.OpenVoiceType.Shell"
PATH = "/io/github/mahfuzur/OpenVoiceType/Shell"
REPORT = os.path.expanduser("~/openvoicetype-spike1.txt")
PURPOSES = {0: "normal", 1: "alpha", 2: "digits", 3: "number", 4: "phone", 5: "url", 6: "email", 7: "name",
            8: "PASSWORD", 9: "PIN", 10: "terminal"}
CONTROL, ALT = 1 << 2, 1 << 3

lines = []
signals = []


def log(text=""):
    print(text)
    lines.append(text)


def result(name, ok, detail=""):
    log(f"{'ok  ' if ok else 'FAIL'} {name}{': ' + str(detail) if detail != '' else ''}")


def ask(question):
    answer = input(f"\n>>> {question} [y/n] ").strip().lower()
    return answer.startswith("y")


def countdown(message, seconds):
    print(f"\n>>> {message}")
    for left in range(seconds, 0, -1):
        print(f"    {left}...", flush=True)
        pump(1)


try:
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    proxy = Gio.DBusProxy.new_sync(bus, 0, None, BUS_NAME, PATH, BUS_NAME)
    proxy.call_sync("GetVersion", None, 0, 2000, None)
except GLib.Error as error:
    print("The extension isn't running. Run install.sh, log out and back in, then enable it:")
    print(f"    gnome-extensions enable openvoicetype-spike@mahfuzur.github.io\n({error.message})")
    sys.exit(1)

proxy.connect("g-signal", lambda p, sender, name, params: signals.append((time.monotonic(), name, params.unpack())))
context = GLib.MainContext.default()


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        context.iteration(False)
        time.sleep(0.01)


def call(method, signature=None, *args):
    params = GLib.Variant(signature, args) if signature else None
    return proxy.call_sync(method, params, 0, 5000, None).unpack()


def set_clipboard(text):
    data = text.encode()
    call("SetClipboard", "(a{say})", {"text/plain;charset=utf-8": data, "text/plain": data})


def get_clipboard():
    return bytes(call("GetClipboard", "(s)", "text/plain;charset=utf-8")[0]).decode(errors="replace")


def wait_for_modifiers_released(limit=2.0):
    end = time.monotonic() + limit
    while call("GetModifiers")[0] and time.monotonic() < end:
        pump(0.02)


def shell(command):
    try:
        return subprocess.run(command, shell=True, capture_output=True, text=True, timeout=5).stdout.strip()
    except Exception as error:  # noqa: BLE001 - a report line, whatever went wrong
        return f"({error})"


log(f"OpenVoiceType spike 1 report, {time.strftime('%Y-%m-%d %H:%M')}")
log(f"system: {shell('lsb_release -ds')} / {shell('gnome-shell --version')} / {os.environ.get('XDG_SESSION_TYPE')}"
    f" / {platform.machine()}")
log(f"keyboard layout: {shell('gsettings get org.gnome.desktop.input-sources sources')}")
result("extension answers", call("GetVersion") == (0,), call("GetVersion")[0])
print("\nThis takes about 5 minutes. Keep this terminal visible; each step says what to do.")

# 1. The focused window.
input("\n>>> Step 1: open the Text Editor app (press Super, type 'Text Editor'). Then come back and press Enter.")
countdown("Click inside the Text Editor window now. Reading which window has focus in", 5)
focus = call("GetFocus")[0]
result("focused window", bool(focus.get("window")) and "texteditor" in focus.get("app_id", "").lower(), focus)

# 2. Modifier state.
countdown("Step 2: press and HOLD Ctrl+Alt (keep holding until the countdown ends)", 3)
held = call("GetModifiers")[0]
result("modifiers while held", held & CONTROL and held & ALT, hex(held))
print("    (you can let go now)")
pump(1.0)
released = call("GetModifiers")[0]
result("modifiers after release", released == 0, hex(released))

# 3. Hold to talk. (The extension also releases a grab if this script stops early, e.g. with Ctrl+C.)
action = call("GrabAccelerator", "(s)", "<Control><Alt>space")[0]
result("hotkey grabbed", action > 0, action)
signals.clear()
try:
    countdown("Step 3: press Ctrl+Alt+Space and HOLD it for about 2 seconds, then let go. Then tap it once quickly."
              " You have", 8)
finally:
    call("UngrabAccelerator", "(u)", action)
events = [(name, round(t - signals[0][0], 2)) for t, name, _ in signals] if signals else []
names = [name for name, _ in events]
# Press-release pairs: a held key must give one Activated, not one per key repeat.
pairs = [(names[i], names[i + 1]) for i in range(0, len(names) - 1, 2)]
result("hotkey press and release (hold to talk)", names[:2] == ["Activated", "Deactivated"], events)
result("no repeats while held", all(pair == ("Activated", "Deactivated") for pair in pairs) and len(names) % 2 == 0,
       names)
first_release = next((t for name, t in events if name == "Deactivated"), 0)
result("hold time measured", first_release > 1.0, f"{first_release} s")
result("quick tap also reported", len(pairs) >= 2, names)

# 4. Esc only while recording.
action = call("GrabAccelerator", "(s)", "Escape")[0]
signals.clear()
try:
    countdown("Step 4: press Esc once", 4)
finally:
    call("UngrabAccelerator", "(u)", action)
result("Esc grabbed", "Activated" in [s[1] for s in signals], [s[1] for s in signals])
result("Esc released back to apps", ask("Press Esc in the Text Editor search box (Ctrl+F, then Esc). Does Esc close it"
                                        " normally again?"))

# 5. Paste into a GTK app, restoring the clipboard after.
set_clipboard("your own clipboard, restored")
pump(0.3)
countdown("Step 5: click at the end of the text in the Text Editor. Pasting in", 5)
before = get_clipboard()
set_clipboard("Hello from OpenVoiceType 👋 (pasted by the spike)")
pump(0.1)
wait_for_modifiers_released()
call("SendKeys", "(s)", "ctrl+v")
pump(0.5)
set_clipboard(before)  # only a fixed delay on GNOME (no "was read" signal): step 5's question checks the paste got in
result("paste in Text Editor", ask("Did 'Hello from OpenVoiceType 👋 (pasted by the spike)' appear in the Text Editor?"))

# 6. Paste into a terminal with Ctrl+Shift+V.
input("\n>>> Step 6: open a second terminal window (Ctrl+Alt+T). Come back here and press Enter.")
countdown("Click in the NEW terminal window now. Pasting 'echo openvoicetype-terminal-test' in", 5)
focus = call("GetFocus")[0]
log(f"     terminal window: app_id={focus.get('app_id')} wm_class={focus.get('wm_class')}")
set_clipboard("echo openvoicetype-terminal-test")
pump(0.1)
call("SendKeys", "(s)", "ctrl+shift+v")
pump(0.5)
purpose = call("GetInputPurpose")[0]
result("input purpose in a terminal", purpose == 10, f"{purpose} ({PURPOSES.get(purpose, '?')})")
result("paste in terminal (Ctrl+Shift+V)", ask("Did 'echo openvoicetype-terminal-test' appear at the prompt of the new"
                                               " terminal (not run)? You can close that terminal now."))

# 7. Password fields.
input("\n>>> Step 7: open Firefox and go to https://github.com/login (don't sign in). Come back and press Enter.")
countdown("Click in the PASSWORD box on that page now. Reading in", 6)
purpose = call("GetInputPurpose")[0]
result("password field detected (Firefox)", purpose == 8, f"{purpose} ({PURPOSES.get(purpose, '?')})")
countdown("Now click in the USERNAME box on that page. Reading in", 5)
purpose = call("GetInputPurpose")[0]
result("normal field not a password (Firefox)", purpose not in (8, 9), f"{purpose} ({PURPOSES.get(purpose, '?')})")
focus = call("GetFocus")[0]
log(f"     browser window: app_id={focus.get('app_id')} wm_class={focus.get('wm_class')} "
    f"sandboxed_app_id={focus.get('sandboxed_app_id')}")

# 8. PRIMARY selection (select without copying).
countdown("Step 8: in the Text Editor, select a word with the mouse (don't copy it). Reading in", 7)
primary = call("GetPrimaryText")[0]
result("PRIMARY selection read", bool(primary), repr(primary[:60]))

# 9. Overlay and panel icon.
countdown("Step 9: click in the Text Editor and keep typing while the overlay shows. It appears in", 3)
for second in range(1, 5):
    call("ShowOverlay", "(s)", f"●  Recording  0:0{second}   (OpenVoiceType spike)")
    pump(1)
call("HideOverlay")
result("overlay visible at the bottom", ask("Did a dark pill with 'Recording' appear at the bottom centre?"))
result("overlay didn't take focus", ask("Could you keep typing in the Text Editor while it showed?"))
call("SetBusy", "(b)", True)
result("panel icon", ask("Is there a microphone icon in the top bar (near the clock/system icons), now red?"))
call("SetBusy", "(b)", False)

log()
log(f"Done. Report: {REPORT}")
with open(REPORT, "w") as report:
    report.write("\n".join(lines) + "\n")
print("\nSend the report (paste it into the chat):\n    cat ~/openvoicetype-spike1.txt")
print("To remove the extension afterwards: bash linux/spikes/gnome-shell/install.sh --remove")
