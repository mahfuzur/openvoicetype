import AppKit
import ApplicationServices
import Carbon

enum Paster {
    /// Puts text (and optional HTML) on the clipboard and, if allowed, sends ⌘V to the focused app, then restores the
    /// previous clipboard (all types, including images) once the app has read ours. Returns false if it could not paste.
    ///
    /// The ⌘V waits until the hotkey's modifiers are released (a held ⌃⌥ would turn it into ⌃⌥⌘V), comes from a private
    /// event source, and uses the key that types "v" in the current layout (Dvorak, AZERTY). Our clipboard item is
    /// marked transient, so clipboard managers skip it, and hands its data over through a provider: the restore runs just
    /// after the target reads it, not on a fixed timer that slow apps (Electron, Google Docs) could miss.
    @discardableResult
    static func paste(_ text: String, html: String? = nil, autoPaste: Bool) -> Bool {
        let pasteboard = NSPasteboard.general
        guard autoPaste, AXIsProcessTrusted() else {
            copy(text, html: html)
            return false
        }
        // A paste whose restore hasn't run yet (a quick swap): its snapshot is the user's clipboard; ours isn't.
        let saved = pending?.saved ?? snapshot(pasteboard)
        generation += 1
        let myGeneration = generation
        let provider = PasteProvider(text: text, html: html)
        let item = NSPasteboardItem()
        item.setDataProvider(provider, forTypes: html == nil ? [.string] : [.string, .html])
        markTransient(item)
        pasteboard.clearContents()
        pasteboard.writeObjects([item])
        let ourChange = pasteboard.changeCount
        pending = (saved, provider) // the pasteboard holds its provider weakly

        whenModifiersReleased {
            let sentAt = Date()
            sendShortcut("v", fallback: CGKeyCode(kVK_ANSI_V))
            let finish = {
                // Only the latest paste restores, and only once.
                guard generation == myGeneration, pending != nil else { return }
                pending = nil
                // Leave the clipboard alone if something else was copied in the meantime.
                guard pasteboard.changeCount == ourChange else { return }
                if saved.isEmpty {
                    // Nothing to put back: keep the text itself rather than a promise from a provider that's gone.
                    pasteboard.clearContents()
                    pasteboard.setString(text, forType: .string)
                    if let html { pasteboard.setString(html, forType: .html) }
                } else {
                    restore(saved, to: pasteboard)
                }
            }
            provider.onRead = {
                // The target read it: restore shortly after, but not before 0.3 s (another reader may have come first).
                let wait = max(0.15, 0.3 - Date().timeIntervalSince(sentAt))
                DispatchQueue.main.asyncAfter(deadline: .now() + wait, execute: finish)
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: finish)
        }
        return true
    }

    /// Puts text on the clipboard for the user to paste themselves (focus moved, or auto-paste is off). Not transient:
    /// it's the user's text now.
    static func copy(_ text: String, html: String? = nil) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        if let html { pasteboard.setString(html, forType: .html) }
    }

    /// ⌘ plus the key that types `character` in the current layout, once the modifiers are up (e.g. ⌘Z to undo).
    static func sendShortcut(_ character: String, fallback: CGKeyCode) {
        let source = CGEventSource(stateID: .privateState)
        let key = KeyboardLayout.keyCode(for: character) ?? fallback
        let down = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
    }

    /// Runs `action` once ⌃, ⌥, ⇧ and ⌘ are all released, or after 0.6 s (a key held on purpose shouldn't block pasting).
    static func whenModifiersReleased(_ action: @escaping () -> Void) {
        let held: CGEventFlags = [.maskControl, .maskAlternate, .maskShift, .maskCommand]
        let deadline = Date().addingTimeInterval(0.6)
        func poll() {
            if CGEventSource.flagsState(.combinedSessionState).intersection(held).isEmpty || Date() >= deadline {
                action()
            } else {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.02, execute: poll)
            }
        }
        poll()
    }

    /// The paste in progress: the user's clipboard to restore, and our provider (kept alive until the restore).
    private static var pending: (saved: Snapshot, provider: PasteProvider)?
    private static var generation = 0

    /// nspasteboard.org markers: clipboard managers (Maccy, Raycast, Paste, Alfred) skip these items.
    private static func markTransient(_ item: NSPasteboardItem) {
        item.setString("", forType: NSPasteboard.PasteboardType("org.nspasteboard.TransientType"))
        item.setString("", forType: NSPasteboard.PasteboardType("org.nspasteboard.AutoGeneratedType"))
        item.setString(Bundle.main.bundleIdentifier ?? "io.github.mahfuzur.openvoicetype",
                       forType: NSPasteboard.PasteboardType("org.nspasteboard.source"))
    }

    private typealias Snapshot = [[(NSPasteboard.PasteboardType, Data)]]

    private static func snapshot(_ pasteboard: NSPasteboard) -> Snapshot {
        (pasteboard.pasteboardItems ?? []).map { item in
            item.types.compactMap { type in item.data(forType: type).map { (type, $0) } }
        }
    }

    private static func restore(_ saved: Snapshot, to pasteboard: NSPasteboard) {
        let items = saved.map { entries -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in entries { item.setData(data, forType: type) }
            markTransient(item) // it's already in the user's clipboard history
            return item
        }
        pasteboard.clearContents()
        pasteboard.writeObjects(items)
    }
}

/// Hands the dictated text to whichever app asks for it, and tells `Paster` when that happened.
private final class PasteProvider: NSObject, NSPasteboardItemDataProvider {
    let text: String
    let html: String?
    var onRead: (() -> Void)?

    init(text: String, html: String?) {
        self.text = text
        self.html = html
    }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        if type == .string {
            item.setString(text, forType: .string)
        } else if type == .html, let html {
            item.setString(html, forType: .html)
        }
        let callback = onRead
        onRead = nil
        DispatchQueue.main.async { callback?() }
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {}
}

/// The key codes of the current keyboard layout, for shortcuts: in Dvorak, ⌘V is on another key than in QWERTY.
enum KeyboardLayout {
    static func keyCode(for character: String) -> CGKeyCode? {
        let sources = [TISCopyCurrentKeyboardLayoutInputSource(), TISCopyCurrentASCIICapableKeyboardLayoutInputSource()]
        for case let source? in sources.map({ $0?.takeRetainedValue() }) {
            guard let property = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) else { continue }
            let data = Unmanaged<CFData>.fromOpaque(property).takeUnretainedValue() as Data
            let found: CGKeyCode? = data.withUnsafeBytes { buffer in
                guard let layout = buffer.bindMemory(to: UCKeyboardLayout.self).baseAddress else { return nil }
                for code in 0..<128 {
                    var deadKeys: UInt32 = 0
                    var length = 0
                    var chars = [UniChar](repeating: 0, count: 4)
                    // The ⌘ modifier state: layouts like "Dvorak - QWERTY ⌘" switch to QWERTY while ⌘ is held.
                    let status = UCKeyTranslate(layout, UInt16(code), UInt16(kUCKeyActionDown), UInt32((cmdKey >> 8) & 0xFF),
                                                UInt32(LMGetKbdType()), OptionBits(kUCKeyTranslateNoDeadKeysBit),
                                                &deadKeys, chars.count, &length, &chars)
                    if status == noErr, length > 0, String(utf16CodeUnits: chars, count: length).lowercased() == character {
                        return CGKeyCode(code)
                    }
                }
                return nil
            }
            if let found { return found }
        }
        return nil
    }
}
