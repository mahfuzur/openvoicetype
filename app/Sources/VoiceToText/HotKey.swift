import Carbon

/// A system-wide hotkey via Carbon's RegisterEventHotKey (needs no Accessibility permission).
/// Reports key presses and, for hold-to-talk, key releases.
final class HotKey {
    struct Combo: Equatable {
        let keyCode: UInt32
        let modifiers: UInt32
        let label: String
    }

    static let presets: [Combo] = [
        Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(controlKey | optionKey), label: "⌃⌥Space"),
        Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(optionKey), label: "⌥Space"),
        Combo(keyCode: UInt32(kVK_Space), modifiers: UInt32(cmdKey | shiftKey), label: "⇧⌘Space"),
        Combo(keyCode: UInt32(kVK_ANSI_D), modifiers: UInt32(controlKey | optionKey), label: "⌃⌥D"),
    ]

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
