"""Drives the spike extension inside a headless GNOME Shell (run.sh): clipboard, focus, hotkeys with release, Esc,
modifiers, overlay, input purpose, typing and pasting into GNOME Text Editor. One ok/FAIL line per check."""

import os
import subprocess
import time

from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION)
proxy = Gio.DBusProxy.new_sync(bus, 0, None, "io.github.mahfuzur.OpenVoiceType.Shell",
                               "/io/github/mahfuzur/OpenVoiceType/Shell", "io.github.mahfuzur.OpenVoiceType.Shell")
signals = []
proxy.connect("g-signal", lambda p, sender, name, params: signals.append((time.monotonic(), name, params.unpack())))
ctx = GLib.MainContext.default()

def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        ctx.iteration(False)
        time.sleep(0.01)

def call(method, sig=None, *args):
    params = GLib.Variant(sig, args) if sig else None
    return proxy.call_sync(method, params, 0, 5000, None).unpack()

failures = 0


def report(name, ok, detail=""):
    global failures
    failures += not ok
    print(f"{'ok  ' if ok else 'FAIL'} {name}{': ' + str(detail) if detail != '' else ''}")

report("version", call("GetVersion") == (0,), call("GetVersion"))
how = call("SetClipboard", "(a{say})", {"text/plain;charset=utf-8": "héllo 👋".encode(), "text/plain": "héllo 👋".encode(),
                                          "text/html": b"<ul><li>one</li></ul>"})
report("set clipboard", how == ("text/plain;charset=utf-8",), how)
pump(0.3)
mimes = call("GetClipboardMimetypes")[0]
report("mimetypes", "text/plain;charset=utf-8" in mimes, mimes)
signals.clear()
data = bytes(call("GetClipboard", "(s)", "text/plain;charset=utf-8")[0]).decode()
pump(0.3)
report("read back", data == "héllo 👋", data)

env = dict(os.environ, WAYLAND_DISPLAY="wayland-0", GDK_BACKEND="wayland", GSK_RENDERER="cairo")
editor = subprocess.Popen(["gnome-text-editor", "--standalone", "--new-window"], env=env,
                          stdout=subprocess.DEVNULL, stderr=open("/tmp/editor.log", "w"))
focus = {}
for _ in range(40):
    pump(0.25)
    if call("ListWindows")[0]:
        call("ActivateNewestWindow")
    focus = call("GetFocus")[0]
    if focus.get("window"):
        break
report("focus", bool(focus.get("pid")) and bool(focus.get("window")), focus or str(call("ListWindows")) + open("/tmp/editor.log").read()[-600:] + " sockets=" + str(os.listdir(os.environ["XDG_RUNTIME_DIR"])))

for label, flags in [("none", 0), ("trigger-release", 128), ("ignore-autorepeat", 16)]:
    action = call("GrabAcceleratorWithFlags", "(su)", "<Control><Alt>space", flags)[0]
    signals.clear()
    call("SendKeys", "(s)", "ctrl+alt+space@400")
    pump(1.0)
    names = [(s[1], round(s[0] - signals[0][0], 2)) for s in signals] if signals else []
    released = [n for n, _ in names][:2] == ["Activated", "Deactivated"]
    if label == "trigger-release":
        report("hotkey press and release (TRIGGER_RELEASE)", released, names)
    else:
        print(f"info grab flags={label}: {names} (no release expected)")
    call("UngrabAccelerator", "(u)", action)
action = call("GrabAccelerator", "(s)", "Escape")[0]
signals.clear(); call("SendKeys", "(s)", "esc"); pump(0.5)
report("escape grab", [s[1] for s in signals][:1] == ["Activated"], [s[1] for s in signals])
report("ungrab", call("UngrabAccelerator", "(u)", action) == (True,))
report("modifiers (none held)", call("GetModifiers") == (0,), call("GetModifiers"))
call("ShowOverlay", "(s)", "● Recording 0:03")
call("HideOverlay")
report("overlay", True)
report("input purpose", call("GetInputPurpose")[0] >= 0, call("GetInputPurpose"))
call("SetBusy", "(b)", True)
report("busy icon", True)

print("info focus before typing:", call("GetFocus")[0].get("title"))
for key in "hi":
    call("SendKeys", "(s)", key)
    pump(0.1)
call("SetClipboard", "(a{say})", {"text/plain;charset=utf-8": b"x"})
pump(0.2)
call("SendKeys", "(s)", "ctrl+a"); pump(0.3)
call("SendKeys", "(s)", "ctrl+c"); pump(1.0)
print("info typed text copied back:", repr(bytes(call("GetClipboard", "(s)", "text/plain;charset=utf-8")[0]).decode()))
print("info focus after:", call("GetFocus")[0].get("title"))
# Paste into the editor with the virtual keyboard, then read the editor's text back through the clipboard (Ctrl+A, Ctrl+C).
call("SetClipboard", "(a{say})", {"text/plain;charset=utf-8": b"pasted by the spike", "text/plain": b"pasted by the spike"})
signals.clear()
pump(0.3)
call("SendKeys", "(s)", "ctrl+v")
pump(1.0)
call("SetClipboard", "(a{say})", {"text/plain;charset=utf-8": b"something else"})
pump(0.3)
call("SendKeys", "(s)", "ctrl+a"); pump(0.3)
call("SendKeys", "(s)", "ctrl+c"); pump(1.0)
text = bytes(call("GetClipboard", "(s)", "text/plain;charset=utf-8")[0]).decode()
report("text arrived in the editor", "pasted by the spike" in text, repr(text))
editor.terminate()
raise SystemExit(1 if failures else 0)
