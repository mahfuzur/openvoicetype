import AppKit
import ApplicationServices

enum Paster {
    /// Puts text (and optional HTML) on the clipboard and, if allowed, sends Cmd+V to the focused app,
    /// then restores the previous clipboard (all types, including images). Returns false if it could not paste.
    @discardableResult
    static func paste(_ text: String, html: String? = nil, autoPaste: Bool) -> Bool {
        let pasteboard = NSPasteboard.general
        let saved = snapshot(pasteboard)

        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        if let html { pasteboard.setString(html, forType: .html) }

        guard autoPaste, AXIsProcessTrusted() else { return false }
        sendCommandV()

        let ourChange = pasteboard.changeCount
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            // Leave the clipboard alone if something else was copied in the meantime.
            guard pasteboard.changeCount == ourChange, !saved.isEmpty else { return }
            restore(saved, to: pasteboard)
        }
        return true
    }

    private static func sendCommandV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let vKey: CGKeyCode = 9
        let down = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
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
            return item
        }
        pasteboard.clearContents()
        pasteboard.writeObjects(items)
    }
}
