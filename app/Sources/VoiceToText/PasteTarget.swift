import AppKit
import ApplicationServices

/// Where a dictation's text should go, captured when recording starts: the app, its focused window and that window's
/// title, and whether a password field has focus. Checked again just before pasting, so text never lands in another app,
/// another window or another Slack channel (the window title changes) because focus moved while it was being prepared.
struct PasteTarget {
    let pid: pid_t
    let bundleID: String?
    let appName: String
    let window: AXUIElement?
    let windowTitle: String?
    let isSecure: Bool

    enum Check: Equatable {
        case same
        /// Focus moved; the associated value names where to, for the overlay.
        case changed(String)
        case secure
    }

    /// Reads the frontmost app through Accessibility (the app has it for pasting). Without it only the app is known.
    static func capture() -> PasteTarget {
        let app = NSWorkspace.shared.frontmostApplication
        let pid = app?.processIdentifier ?? 0
        let bundleID = app?.bundleIdentifier
        let appName = app?.localizedName ?? "another app"
        guard AXIsProcessTrusted(), pid != 0 else {
            return PasteTarget(pid: pid, bundleID: bundleID, appName: appName, window: nil, windowTitle: nil, isSecure: false)
        }
        let appElement = AXUIElementCreateApplication(pid)
        // A hung app must not hold up dictation (the default timeout is several seconds).
        AXUIElementSetMessagingTimeout(appElement, 0.3)
        let window: AXUIElement? = attribute(appElement, kAXFocusedWindowAttribute)
        let title: String? = window.flatMap { attribute($0, kAXTitleAttribute) }
        let focused: AXUIElement? = attribute(appElement, kAXFocusedUIElementAttribute)
        let subrole: String? = focused.flatMap { attribute($0, kAXSubroleAttribute) }
        return PasteTarget(pid: pid, bundleID: bundleID, appName: appName, window: window, windowTitle: title,
                           isSecure: subrole == (kAXSecureTextFieldSubrole as String))
    }

    /// Compares with what has focus now.
    func check() -> Check {
        let now = Self.capture()
        if now.isSecure { return .secure }
        guard now.pid == pid else { return .changed(now.appName) }
        if let window, let nowWindow = now.window, !CFEqual(window, nowWindow) {
            return .changed("another \(now.appName) window")
        }
        // Terminals and editors retitle themselves while they work (a job name, Claude Code's spinner, a file name), so
        // there the same window is enough.
        let sameWindow = window != nil && now.window != nil
        if sameWindow && isCodeApp { return .same }
        if let windowTitle, let nowTitle = now.windowTitle, Self.normalized(windowTitle) != Self.normalized(nowTitle) {
            return .changed(nowTitle.isEmpty ? "another \(now.appName) view" : "“\(Self.shortened(nowTitle))”")
        }
        return .same
    }

    /// Editors, terminals and IDEs, where the title isn't a reliable sign of where the text goes.
    var isCodeApp: Bool { DictationMode.forApp(bundleID: bundleID) == .code }

    /// Unread counts, activity spinners and "Edited" come and go in titles ("(3) Inbox", "• Slack", "⠋ claude",
    /// "Notes — Edited"): they don't mean the user moved.
    static func normalized(_ title: String) -> String {
        title.replacingOccurrences(of: #"\(\d+\)"#, with: "", options: .regularExpression)
            .replacingOccurrences(of: #"^[^\p{L}\p{N}#@(\[]+"#, with: "", options: .regularExpression)
            .replacingOccurrences(of: #"\s+[—–-]\s+Edited$"#, with: "", options: .regularExpression)
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
    }

    /// Up to `count` characters just before the cursor in the focused text field, or nil when the app doesn't say
    /// (many Electron and web apps). Used to check that the last paste is still what the user sees before swapping it.
    static func textBeforeCursor(count: Int) -> String? {
        guard AXIsProcessTrusted(), let app = NSWorkspace.shared.frontmostApplication else { return nil }
        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetMessagingTimeout(appElement, 0.3)
        guard let focused: AXUIElement = attribute(appElement, kAXFocusedUIElementAttribute) else { return nil }
        var rangeValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(focused, kAXSelectedTextRangeAttribute as CFString, &rangeValue) == .success,
              let rangeValue, CFGetTypeID(rangeValue) == AXValueGetTypeID() else { return nil }
        var selection = CFRange()
        guard AXValueGetValue(rangeValue as! AXValue, .cfRange, &selection) else { return nil }
        let start = max(0, selection.location - count)
        var wanted = CFRange(location: start, length: selection.location - start)
        guard wanted.length > 0, let range = AXValueCreate(.cfRange, &wanted) else { return nil }
        var text: CFTypeRef?
        guard AXUIElementCopyParameterizedAttributeValue(focused, kAXStringForRangeParameterizedAttribute as CFString,
                                                         range, &text) == .success else { return nil }
        return text as? String
    }

    private static func shortened(_ title: String) -> String {
        title.count > 40 ? String(title.prefix(40)) + "…" : title
    }

    private static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success, let value else { return nil }
        if T.self == AXUIElement.self {
            guard CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
            return (value as! T)
        }
        return value as? T
    }
}
