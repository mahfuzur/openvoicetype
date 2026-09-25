import AppKit
import Carbon

/// A system-wide hotkey via Carbon's RegisterEventHotKey (needs no Accessibility permission).
/// Reports key presses and, for hold-to-talk, key releases.
final class HotKey {
    struct Combo: Equatable {
        let keyCode: UInt32
        /// Carbon modifier flags (cmdKey, optionKey, controlKey, shiftKey).
        let modifiers: UInt32
        let label: String

        static let defaultCombo = Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(controlKey | optionKey),
                                        label: "⌃⌥Space")
        /// Swaps the last paste between the cleaned text and Whisper's text.
        static let defaultSwapCombo = Combo(keyCode: UInt32(kVK_ANSI_Z), modifiers: UInt32(controlKey | optionKey),
                                            label: "⌃⌥Z")

        /// A combo saved under `prefix` (e.g. "swapHotKey"), or `fallback`.
        static func load(from defaults: UserDefaults, prefix: String, fallback: Combo) -> Combo {
            guard let label = defaults.string(forKey: prefix + "Label"), defaults.object(forKey: prefix + "Code") != nil else {
                return fallback
            }
            return Combo(keyCode: UInt32(defaults.integer(forKey: prefix + "Code")),
                         modifiers: UInt32(defaults.integer(forKey: prefix + "Modifiers")), label: label)
        }

        /// The dictation combo the Settings window recorded; migrates the older preset menu (`hotKeyIndex`).
        static func load(from defaults: UserDefaults) -> Combo {
            if defaults.string(forKey: "hotKeyLabel") != nil, defaults.object(forKey: "hotKeyCode") != nil {
                return load(from: defaults, prefix: "hotKey", fallback: defaultCombo)
            }
            let legacy = [
                defaultCombo,
                Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(optionKey), label: "⌥Space"),
                Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(cmdKey | shiftKey), label: "⇧⌘Space"),
                Combo(keyCode: UInt32(kVK_ANSI_D), modifiers: UInt32(controlKey | optionKey), label: "⌃⌥D"),
            ]
            return legacy[min(max(defaults.integer(forKey: "hotKeyIndex"), 0), legacy.count - 1)]
        }

        func save(to defaults: UserDefaults, prefix: String = "hotKey") {
            defaults.set(Int(keyCode), forKey: prefix + "Code")
            defaults.set(Int(modifiers), forKey: prefix + "Modifiers")
            defaults.set(label, forKey: prefix + "Label")
        }

        /// A combo from a key press in the hotkey recorder, or nil if it can't be a global hotkey. It needs ⌃ or ⌥
        /// (a plain letter would stop you typing it, and ⌘ shortcuts like ⌘V or ⌘Q belong to every app: a global ⌘V
        /// would even catch the app's own paste). Function keys are fine on their own.
        init?(event: NSEvent) {
            let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
            let code = Int(event.keyCode)
            let isFunctionKey = Self.functionKeys[code] != nil
            guard isFunctionKey || flags.contains(.option) || flags.contains(.control) else { return nil }
            var carbon = 0
            var symbols = ""
            if flags.contains(.control) { carbon |= controlKey; symbols += "⌃" }
            if flags.contains(.option) { carbon |= optionKey; symbols += "⌥" }
            if flags.contains(.shift) { carbon |= shiftKey; symbols += "⇧" }
            if flags.contains(.command) { carbon |= cmdKey; symbols += "⌘" }
            let key = Self.functionKeys[code] ?? Self.namedKeys[code]
                ?? (event.charactersIgnoringModifiers ?? "").uppercased()
            guard !key.isEmpty else { return nil }
            self.init(keyCode: UInt32(code), modifiers: UInt32(carbon), label: symbols + key)
        }

        init(keyCode: UInt32, modifiers: UInt32, label: String) {
            self.keyCode = keyCode
            self.modifiers = modifiers
            self.label = label
        }

        private static let functionKeys: [Int: String] = [
            kVK_F1: "F1", kVK_F2: "F2", kVK_F3: "F3", kVK_F4: "F4", kVK_F5: "F5", kVK_F6: "F6", kVK_F7: "F7",
            kVK_F8: "F8", kVK_F9: "F9", kVK_F10: "F10", kVK_F11: "F11", kVK_F12: "F12", kVK_F13: "F13",
            kVK_F14: "F14", kVK_F15: "F15", kVK_F16: "F16", kVK_F17: "F17", kVK_F18: "F18", kVK_F19: "F19",
        ]
        private static let namedKeys: [Int: String] = [
            kVK_Space: "Space", kVK_Return: "↩", kVK_Tab: "⇥", kVK_Delete: "⌫", kVK_ForwardDelete: "⌦",
            kVK_LeftArrow: "←", kVK_RightArrow: "→", kVK_UpArrow: "↑", kVK_DownArrow: "↓",
            kVK_Home: "↖", kVK_End: "↘", kVK_PageUp: "⇞", kVK_PageDown: "⇟",
        ]
    }

    static let escape = Combo(keyCode: UInt32(kVK_Escape), modifiers: 0, label: "Esc")

    private static var handlers: [UInt32: (press: () -> Void, release: (() -> Void)?)] = [:]
    private static var nextID: UInt32 = 1
    private static var handlerInstalled = false

    private var ref: EventHotKeyRef?
    private let id: UInt32

    /// Returns nil if the combo is already taken by another app.
    init?(_ combo: Combo, onRelease: (() -> Void)? = nil, handler: @escaping () -> Void) {
        HotKey.installHandlerIfNeeded()
        id = HotKey.nextID
        HotKey.nextID += 1
        let hotKeyID = EventHotKeyID(signature: OSType(0x5654_5458), id: id) // 'VTTX'
        let status = RegisterEventHotKey(combo.keyCode, combo.modifiers, hotKeyID,
                                         GetApplicationEventTarget(), 0, &ref)
        guard status == noErr, ref != nil else { return nil }
        HotKey.handlers[id] = (handler, onRelease)
    }

    deinit { unregister() }

    func unregister() {
        if let ref { UnregisterEventHotKey(ref) }
        ref = nil
        HotKey.handlers[id] = nil
    }

    private static func installHandlerIfNeeded() {
        guard !handlerInstalled else { return }
        handlerInstalled = true
        var specs = [
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed)),
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyReleased)),
        ]
        InstallEventHandler(GetApplicationEventTarget(), { _, event, _ in
            var hotKeyID = EventHotKeyID()
            GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                              nil, MemoryLayout<EventHotKeyID>.size, nil, &hotKeyID)
            let released = GetEventKind(event) == UInt32(kEventHotKeyReleased)
            DispatchQueue.main.async {
                guard let handler = HotKey.handlers[hotKeyID.id] else { return }
                if released { handler.release?() } else { handler.press() }
            }
            return noErr
        }, specs.count, &specs, nil, nil)
    }
}
