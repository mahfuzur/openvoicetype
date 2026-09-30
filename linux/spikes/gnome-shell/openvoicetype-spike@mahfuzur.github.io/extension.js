// OpenVoiceType P1 spike (docs/plans/M7-linux.md): can a GNOME Shell extension give the Linux app what macOS gives the
// Mac app? It offers, over D-Bus on the session bus:
//   - global hotkeys with press AND release (hold to talk), and Esc only while recording;
//   - the focused window (pid, app id, a stable window id, title) for the paste-target check;
//   - the clipboard (plain text; read and written);
//   - key chords through a virtual keyboard (Ctrl+V, Ctrl+Shift+V), and the live modifier state;
//   - the input purpose (a password field) and the PRIMARY selection;
//   - an overlay pill and a panel icon.
// It's an experiment: it doesn't check who calls it. Remove it after the test (spike.py tells you how).

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.mahfuzur.OpenVoiceType.Shell';
const OBJECT_PATH = '/io/github/mahfuzur/OpenVoiceType/Shell';
const API_VERSION = 0; // 0 = the spike

const INTERFACE = `
<node>
  <interface name="io.github.mahfuzur.OpenVoiceType.Shell">
    <method name="GetVersion"><arg type="u" direction="out"/></method>
    <method name="GetFocus"><arg type="a{sv}" direction="out"/></method>
    <method name="GrabAccelerator"><arg type="s" direction="in"/><arg type="u" direction="out"/></method>
    <method name="GrabAcceleratorWithFlags"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="u" direction="out"/></method>
    <method name="UngrabAccelerator"><arg type="u" direction="in"/><arg type="b" direction="out"/></method>
    <method name="GetModifiers"><arg type="u" direction="out"/></method>
    <method name="SendKeys"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
    <method name="SetClipboard"><arg type="a{say}" direction="in"/><arg type="s" direction="out"/></method>
    <method name="GetClipboardMimetypes"><arg type="as" direction="out"/></method>
    <method name="GetClipboard"><arg type="s" direction="in"/><arg type="ay" direction="out"/></method>
    <method name="GetPrimaryText"><arg type="s" direction="out"/></method>
    <method name="GetInputPurpose"><arg type="i" direction="out"/></method>
    <method name="ShowOverlay"><arg type="s" direction="in"/></method>
    <method name="HideOverlay"/>
    <method name="SetBusy"><arg type="b" direction="in"/></method>
    <method name="ListWindows"><arg type="as" direction="out"/></method>
    <method name="ActivateNewestWindow"/>
    <signal name="Activated"><arg type="u"/></signal>
    <signal name="Deactivated"><arg type="u"/></signal>
  </interface>
</node>`;

// The clipboard is St.Clipboard (text/plain;charset=utf-8). A multi-MIME owner (text/html for rich paste) that reports
// reads would need a Meta.SelectionSource subclass, but GJS can't implement its read_async vfunc ("accepts another
// callback as a parameter. This is not supported", GNOME 50): tried in the container on 2026-09-30.

const MODIFIER_MASK = Clutter.ModifierType.SHIFT_MASK | Clutter.ModifierType.CONTROL_MASK |
    Clutter.ModifierType.MOD1_MASK | Clutter.ModifierType.SUPER_MASK | Clutter.ModifierType.META_MASK |
    Clutter.ModifierType.HYPER_MASK;

const KEY_NAMES = {
    ctrl: Clutter.KEY_Control_L, control: Clutter.KEY_Control_L, shift: Clutter.KEY_Shift_L,
    alt: Clutter.KEY_Alt_L, super: Clutter.KEY_Super_L, esc: Clutter.KEY_Escape,
};

class Service {
    constructor(extension) {
        this._extension = extension;
        this._grabs = new Map(); // action -> binding name
        this._displaySignals = [];
        this._displaySignals.push(global.display.connect('accelerator-activated', (display, action) => {
            if (this._grabs.has(action))
                this._emit('Activated', action);
        }));
        try {
            this._displaySignals.push(global.display.connect('accelerator-deactivated', (display, action) => {
                if (this._grabs.has(action))
                    this._emit('Deactivated', action);
            }));
        } catch (e) {
            console.warn(`OpenVoiceType spike: no accelerator-deactivated signal: ${e}`);
        }
    }

    set dbus(object) {
        this._dbus = object;
    }

    _emit(signal, value) {
        const type = typeof value === 'string' ? '(s)' : '(u)';
        this._dbus?.emit_signal(signal, new GLib.Variant(type, [value]));
    }

    GetVersion() {
        return API_VERSION;
    }

    GetFocus() {
        const window = global.display.get_focus_window();
        if (!window)
            return {};
        const app = Shell.WindowTracker.get_default().get_window_app(window);
        return {
            pid: new GLib.Variant('u', Math.max(window.get_pid(), 0)),
            app_id: new GLib.Variant('s', app?.get_id() ?? ''),
            app_name: new GLib.Variant('s', app?.get_name() ?? ''),
            window: new GLib.Variant('s', String(window.get_id())),
            title: new GLib.Variant('s', window.get_title() ?? ''),
            wm_class: new GLib.Variant('s', window.get_wm_class() ?? ''),
            sandboxed_app_id: new GLib.Variant('s', window.get_sandboxed_app_id?.() ?? ''),
        };
    }

    // TRIGGER_RELEASE: GNOME 50 then signals Activated on press and Deactivated on release (hold to talk). Without it,
    // there's no Deactivated (tried in the container, 2026-09-30).
    GrabAccelerator(accelerator) {
        return this.GrabAcceleratorWithFlags(accelerator, Meta.KeyBindingFlags.TRIGGER_RELEASE);
    }

    // Meta.KeyBindingFlags, to find which grab reports key release.
    GrabAcceleratorWithFlags(accelerator, flags) {
        const action = global.display.grab_accelerator(accelerator, flags);
        if (action === Meta.KeyBindingAction.NONE)
            return 0;
        const name = Meta.external_binding_name_for_action(action);
        Main.wm.allowKeybinding(name, Shell.ActionMode.ALL);
        this._grabs.set(action, name);
        return action;
    }

    UngrabAccelerator(action) {
        const name = this._grabs.get(action);
        if (name === undefined)
            return false;
        this._grabs.delete(action);
        Main.wm.allowKeybinding(name, Shell.ActionMode.NONE);
        return global.display.ungrab_accelerator(action);
    }

    GetModifiers() {
        const [, , modifiers] = global.get_pointer();
        return modifiers & MODIFIER_MASK;
    }

    // "ctrl+shift+v": press in order, release in reverse, through a virtual keyboard. Key symbols, not key codes, so the
    // keyboard layout doesn't matter (AZERTY, Dvorak). "ctrl+alt+space@400" holds the keys for 400 ms (for tests).
    SendKeys(chord) {
        const [keys, hold] = chord.split('@');
        const keyvals = keys.split('+').map(name => KEY_NAMES[name.toLowerCase()] ?? Clutter[`KEY_${name}`]);
        if (keyvals.some(k => k === undefined))
            return false;
        this._keyboard ??= Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        for (const keyval of keyvals)
            this._keyboard.notify_keyval(GLib.get_monotonic_time(), keyval, Clutter.KeyState.PRESSED);
        const release = () => {
            for (const keyval of [...keyvals].reverse())
                this._keyboard?.notify_keyval(GLib.get_monotonic_time(), keyval, Clutter.KeyState.RELEASED);
            return GLib.SOURCE_REMOVE;
        };
        if (hold)
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, Number(hold), release);
        else
            release();
        return true;
    }

    // Sets text/plain;charset=utf-8 (other types in `contents` are ignored: see the note above). Returns what was set.
    SetClipboard(contents) {
        const text = contents['text/plain;charset=utf-8'] ?? contents['text/plain'] ?? new Uint8Array();
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, new TextDecoder().decode(text));
        return 'text/plain;charset=utf-8';
    }

    GetClipboardMimetypes() {
        return St.Clipboard.get_default().get_mimetypes(St.ClipboardType.CLIPBOARD);
    }

    GetClipboardAsync([mimetype], invocation) {
        St.Clipboard.get_default().get_content(St.ClipboardType.CLIPBOARD, mimetype, (clipboard, bytes) => {
            const data = bytes?.get_data() ?? new Uint8Array();
            invocation.return_value(new GLib.Variant('(ay)', [data]));
        });
    }

    GetPrimaryTextAsync(params, invocation) {
        St.Clipboard.get_default().get_text(St.ClipboardType.PRIMARY, (clipboard, text) => {
            invocation.return_value(new GLib.Variant('(s)', [text ?? '']));
        });
    }

    // IBus.InputPurpose: 8 = PASSWORD, 9 = PIN, 10 = TERMINAL, 0 = free form (also when no text field has focus).
    GetInputPurpose() {
        return Main.inputMethod?._purpose ?? -1;
    }

    ShowOverlay(text) {
        if (!this._overlay) {
            this._overlay = new St.Label({
                style: 'background-color: rgba(0, 0, 0, 0.85); color: white; border-radius: 20px; ' +
                    'padding: 10px 22px; font-size: 13pt;',
                reactive: false,
            });
            // Not reactive, so clicks go through to the window below (GNOME 50 has no affectsInputRegion any more).
            Main.layoutManager.addTopChrome(this._overlay);
        }
        this._overlay.text = text;
        const monitor = Main.layoutManager.primaryMonitor;
        const [, width] = this._overlay.get_preferred_width(-1);
        const [, height] = this._overlay.get_preferred_height(width);
        this._overlay.set_position(
            Math.floor(monitor.x + (monitor.width - width) / 2),
            Math.floor(monitor.y + monitor.height - height - 28 * St.ThemeContext.get_for_stage(global.stage).scale_factor));
        this._overlay.show();
    }

    HideOverlay() {
        this._overlay?.hide();
    }

    // Test helpers (the headless test has no mouse to click a window with).
    ListWindows() {
        return global.display.list_all_windows().map(w => `${w.get_id()} ${w.get_wm_class()} ${w.get_title()}`);
    }

    ActivateNewestWindow() {
        global.display.list_all_windows().at(-1)?.activate(global.get_current_time());
    }

    SetBusy(busy) {
        this._extension.setBusy(busy);
    }

    destroy() {
        for (const action of [...this._grabs.keys()])
            this.UngrabAccelerator(action);
        this._displaySignals.forEach(id => global.display.disconnect(id));
        this._overlay?.destroy();
        this._overlay = null;
        this._keyboard = null;
    }
}

export default class OpenVoiceTypeSpike extends Extension {
    enable() {
        this._service = new Service(this);
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this._service);
        this._service.dbus = this._dbus;
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS_NAME, Gio.BusNameOwnerFlags.NONE, null, null);

        this._indicator = new PanelMenu.Button(0.0, 'OpenVoiceType', false);
        this._icon = new St.Icon({icon_name: 'audio-input-microphone-symbolic', style_class: 'system-status-icon'});
        this._indicator.add_child(this._icon);
        this._indicator.menu.addMenuItem(new PopupMenu.PopupMenuItem('OpenVoiceType spike is running', {reactive: false}));
        Main.panel.addToStatusArea(this.uuid, this._indicator);
    }

    setBusy(busy) {
        this._icon.style = busy ? 'color: #ff453a;' : null;
    }

    disable() {
        Gio.bus_unown_name(this._nameId);
        this._dbus.unexport();
        this._service.destroy();
        this._indicator.destroy();
        this._service = this._dbus = this._indicator = this._icon = null;
    }
}
