import AppKit
import ApplicationServices
import Carbon

/// The selected text in the frontmost app, for Command Mode.
///
/// Accessibility first (the focused element's selected text, then WebKit and Chromium text markers). If it gives nothing,
/// the app's own Edit ▸ Copy is pressed through Accessibility: no key event, so no beep and no held modifiers, and a
/// disabled Copy item means nothing is selected. Apps without that menu item get a ⌘C. Either way the clipboard is saved
/// first and put back after, a marker catches stale contents, and code editors that copy the whole line when nothing is
/// selected (VS Code, JetBrains…) don't count as a selection.
struct Selection {
    /// ax: the selected-text attribute; axMarkers: WebKit/Chromium text markers; menu: Edit ▸ Copy; copy: ⌘C.
    enum Source: String { case ax, axMarkers, menu, copy, none }

    /// The selected text, or nil when nothing is selected (or it couldn't be read).
    let text: String?
    let source: Source
    /// The focused element takes text (a paste would replace the selection or type at the cursor). True when unknown.
    let editable: Bool
    /// Accessibility could say whether a text field has focus (false for most Electron and web views).
    let knowsFocus: Bool
    /// The selection couldn't be read (no Accessibility answer, no Copy menu item, and the hotkey's keys were still held
    /// for ⌘C): there may be a selection, so the answer must not be pasted over it.
    var inconclusive = false

    static let none = Selection(text: nil, source: .none, editable: true, knowsFocus: false)
}

enum SelectionReader {
    /// Reads the selection off the main thread and calls back on main.
    static func read(target: PasteTarget, completion: @escaping (Selection) -> Void) {
        // On main: a dictation's clipboard restore that's still pending runs now (else its snapshot, the user's clipboard,
        // would be lost under our marker), and the ⌘C key code (the keyboard-layout APIs are main-thread only).
        Paster.settlePending()
        let copyKey = KeyboardLayout.keyCode(for: "c") ?? CGKeyCode(kVK_ANSI_C)
        queue.async {
            let selection = readNow(target: target, copyKey: copyKey)
            DispatchQueue.main.async { completion(selection) }
        }
    }

    /// Apps that always get the answer on the clipboard: pasting runs commands instead of replacing text.
    static let terminals: Set<String> = [
        "com.apple.terminal", "com.googlecode.iterm2", "dev.warp.warp-stable", "com.mitchellh.ghostty",
        "net.kovidgoyal.kitty", "org.alacritty", "com.github.wez.wezterm", "co.zeit.hyper",
    ]

    /// Editors whose ⌘C copies the current line when nothing is selected.
    private static let lineCopyEditors: Set<String> = [
        "com.microsoft.vscode", "com.microsoft.vscodeinsiders", "com.todesktop.230313mzl4w4u92", "com.vscodium",
        "com.sublimetext.4", "com.apple.dt.xcode", "com.exafunction.windsurf", "dev.zed.zed",
    ]

    /// True if a copied text is what a line-copying editor puts on the clipboard with nothing selected.
    static func looksLikeLineCopy(_ text: String, bundleID: String?) -> Bool {
        let id = bundleID?.lowercased() ?? ""
        guard lineCopyEditors.contains(id) || id.hasPrefix("com.jetbrains.") else { return false }
        let body = text.hasSuffix("\n") ? String(text.dropLast()) : text
        return text.hasSuffix("\n") && !body.contains("\n")
    }

    private static let queue = DispatchQueue(label: "VoiceToText.SelectionReader")

    private static func readNow(target: PasteTarget, copyKey: CGKeyCode) -> Selection {
        guard AXIsProcessTrusted(), target.pid != 0 else { return .none }
        let app = AXUIElementCreateApplication(target.pid)
        AXUIElementSetMessagingTimeout(app, 0.3)
        var focused: AXUIElement? = attribute(app, kAXFocusedUIElementAttribute)
        // Chromium turns its accessibility tree on when first asked, so an empty first answer is worth a second try.
        if focused == nil {
            usleep(150_000)
            focused = attribute(app, kAXFocusedUIElementAttribute)
        }
        let editable = focused.map(isEditable) ?? true
        if let focused {
            if let text: String = attribute(focused, kAXSelectedTextAttribute), !text.isEmpty {
                return Selection(text: text, source: .ax, editable: editable, knowsFocus: true)
            }
            if let text = markerText(focused), !text.isEmpty {
                return Selection(text: text, source: .axMarkers, editable: editable, knowsFocus: true)
            }
            // A real, empty selection in a text field: nothing selected, no need to copy.
            if let range = selectedRange(focused), range.length == 0, isTextRole(focused) {
                return Selection(text: nil, source: .none, editable: editable, knowsFocus: true)
            }
        }
        let copied = copyThroughApp(app, target: target, copyKey: copyKey)
        var selection = Selection(text: copied.text, source: copied.source, editable: editable, knowsFocus: focused != nil)
        selection.inconclusive = copied.inconclusive
        return selection
    }

    /// Presses Edit ▸ Copy (or sends ⌘C) and reads what it put on the clipboard, then restores the clipboard.
    private static func copyThroughApp(_ app: AXUIElement, target: PasteTarget, copyKey: CGKeyCode)
        -> (text: String?, source: Selection.Source, inconclusive: Bool) {
        let menuItem = copyMenuItem(app)
        if let menuItem, attribute(menuItem, kAXEnabledAttribute) as Bool? == false {
            return (nil, .none, false) // Copy is greyed out: nothing is selected
        }
        // ⌘C with the hotkey's ⌃⌥⇧ still down would arrive as ⌃⌥⇧⌘C: wait for them (up to 0.8 s), else give up.
        if menuItem == nil && !modifiersReleased(within: 0.8) {
            return (nil, .copy, true)
        }
        let pasteboard = NSPasteboard.general
        let saved = Paster.snapshot(pasteboard)
        // A marker, so text that was already on the clipboard can't pass for the selection.
        let marker = "openvoicetype-selection-\(UUID().uuidString)"
        let item = NSPasteboardItem()
        item.setString(marker, forType: .string)
        Paster.markTransient(item)
        pasteboard.clearContents()
        pasteboard.writeObjects([item])
        let before = pasteboard.changeCount
        let source: Selection.Source
        if let menuItem {
            AXUIElementPerformAction(menuItem, kAXPressAction as CFString)
            source = .menu
        } else {
            let events = CGEventSource(stateID: .privateState)
            for down in [true, false] {
                let event = CGEvent(keyboardEventSource: events, virtualKey: copyKey, keyDown: down)
                event?.flags = .maskCommand
                event?.post(tap: .cghidEventTap)
            }
            source = .copy
        }
        // Safari and Word write the clipboard slowly.
        let id = target.bundleID?.lowercased() ?? ""
        let timeout: TimeInterval = id == "com.apple.safari" || id == "com.microsoft.word" ? 0.5 : 0.3
        let deadline = Date().addingTimeInterval(timeout)
        var copied: String?
        while Date() < deadline {
            if pasteboard.changeCount != before, let text = pasteboard.string(forType: .string), text != marker {
                copied = text
                break
            }
            usleep(5_000)
        }
        // Put the user's clipboard back. Always: within these few tenths of a second the change was ours (a copy that
        // wrote no text, an image, still counts).
        func putBack() { if saved.isEmpty { pasteboard.clearContents() } else { Paster.restore(saved, to: pasteboard) } }
        putBack()
        if copied == nil {
            // A slow app may still copy after the wait: catch that and put the clipboard back again.
            let restored = pasteboard.changeCount
            usleep(400_000)
            if pasteboard.changeCount != restored { putBack() }
        }
        guard let text = copied, !text.isEmpty, !looksLikeLineCopy(text, bundleID: target.bundleID) else {
            return (nil, source, false)
        }
        return (text, source, false)
    }

    private static func modifiersReleased(within seconds: TimeInterval) -> Bool {
        let held: CGEventFlags = [.maskControl, .maskAlternate, .maskShift, .maskCommand]
        let deadline = Date().addingTimeInterval(seconds)
        repeat {
            if CGEventSource.flagsState(.combinedSessionState).intersection(held).isEmpty { return true }
            usleep(20_000)
        } while Date() < deadline
        return false
    }

    /// The app's Copy menu item: the one whose action is `copy:`, else ⌘C in the first menus (usually Edit).
    private static func copyMenuItem(_ app: AXUIElement) -> AXUIElement? {
        guard let bar: AXUIElement = attribute(app, kAXMenuBarAttribute),
              let menus: [AXUIElement] = attribute(bar, kAXChildrenAttribute) else { return nil }
        for barItem in menus.prefix(6) {
            guard let menu = (attribute(barItem, kAXChildrenAttribute) as [AXUIElement]?)?.first,
                  let items: [AXUIElement] = attribute(menu, kAXChildrenAttribute) else { continue }
            for item in items {
                if attribute(item, "AXIdentifier") as String? == "copy:" { return item }
                if attribute(item, kAXMenuItemCmdCharAttribute) as String? == "C",
                   attribute(item, kAXMenuItemCmdModifiersAttribute) as Int? == 0 { return item }
            }
        }
        return nil
    }

    private static func isEditable(_ element: AXUIElement) -> Bool {
        var settable = DarwinBoolean(false)
        if AXUIElementIsAttributeSettable(element, kAXValueAttribute as CFString, &settable) == .success, settable.boolValue {
            return true
        }
        if AXUIElementIsAttributeSettable(element, kAXSelectedTextAttribute as CFString, &settable) == .success,
           settable.boolValue {
            return true
        }
        // Web content: a focused text area or contenteditable reports these roles even when the value isn't settable.
        if isTextRole(element) { return true }
        // Nothing says it takes text; with no role at all we don't know, so allow it.
        return (attribute(element, kAXRoleAttribute) as String?) == nil
    }

    private static func isTextRole(_ element: AXUIElement) -> Bool {
        let role: String? = attribute(element, kAXRoleAttribute)
        return [kAXTextFieldRole, kAXTextAreaRole, kAXComboBoxRole, "AXSearchField"].contains { $0 as String == role }
    }

    private static func selectedRange(_ element: AXUIElement) -> CFRange? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, kAXSelectedTextRangeAttribute as CFString, &value) == .success,
              let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
        var range = CFRange()
        return AXValueGetValue(value as! AXValue, .cfRange, &range) ? range : nil
    }

    /// WebKit and Chromium expose the selection of web content as a text-marker range.
    private static func markerText(_ element: AXUIElement) -> String? {
        var markers: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, "AXSelectedTextMarkerRange" as CFString, &markers) == .success,
              let markers else { return nil }
        var text: CFTypeRef?
        guard AXUIElementCopyParameterizedAttributeValue(element, "AXStringForTextMarkerRange" as CFString, markers,
                                                         &text) == .success else { return nil }
        return text as? String
    }

    private static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success, let value else { return nil }
        if T.self == AXUIElement.self {
            guard CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
            return (value as! T)
        }
        if T.self == [AXUIElement].self { return value as? [AXUIElement] as? T }
        if T.self == Bool.self { return (value as? NSNumber)?.boolValue as? T }
        if T.self == Int.self { return (value as? NSNumber)?.intValue as? T }
        return value as? T
    }
}
