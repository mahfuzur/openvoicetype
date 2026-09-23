import AppKit

/// Writing style for the cleanup step; each maps to prompts/modes/<rawValue>.md.
enum DictationMode: String, CaseIterable {
    case `default`, chat, email, code, notes, raw

    var title: String {
        switch self {
        case .default: "Default"
        case .chat: "Chat"
        case .email: "Email"
        case .code: "Code"
        case .notes: "Notes"
        case .raw: "Raw (no cleanup)"
        }
    }

    /// Picks a mode from the app that will receive the text. Browsers stay `default`: we can't
    /// tell which web app is open.
    static func forApp(bundleID: String?) -> DictationMode {
        guard let id = bundleID?.lowercased() else { return .default }
        if chatApps.contains(id) { return .chat }
        if emailApps.contains(id) { return .email }
        if codeApps.contains(id) || id.hasPrefix("com.jetbrains.") { return .code }
        if notesApps.contains(id) { return .notes }
        return .default
    }

    private static let chatApps: Set<String> = [
        "com.tinyspeck.slackmacgap", "com.microsoft.teams", "com.microsoft.teams2", "com.hnc.discord",
        "net.whatsapp.whatsapp", "desktop.whatsapp", "com.apple.mobilesms", "ru.keepcoder.telegram",
        "org.telegram.desktop", "com.facebook.archon", "com.skype.skype",
    ]
    private static let emailApps: Set<String> = [
        "com.apple.mail", "com.microsoft.outlook", "com.readdle.smartemail-mac", "com.readdle.sparkdesktop",
        "com.superhuman.electron",
    ]
    private static let codeApps: Set<String> = [
        "com.microsoft.vscode", "com.microsoft.vscodeinsiders", "com.todesktop.230313mzl4w4u92", "com.apple.dt.xcode",
        "com.apple.terminal", "com.googlecode.iterm2", "dev.warp.warp-stable", "com.mitchellh.ghostty", "dev.zed.zed",
        "com.sublimetext.4", "com.vscodium",
    ]
    private static let notesApps: Set<String> = [
        "com.apple.notes", "notion.id", "md.obsidian", "net.shinyfrog.bear", "com.apple.iwork.pages",
        "com.microsoft.word", "com.apple.textedit",
    ]
}

/// The app that has focus (and will receive the paste), and the mode to use for it.
struct DictationContext {
    let mode: DictationMode
    let appName: String
    let bundleID: String?

    /// `override` nil means "Auto": choose from the frontmost app.
    static func current(override: DictationMode?) -> DictationContext {
        let app = NSWorkspace.shared.frontmostApplication
        let bundleID = app?.bundleIdentifier
        let mode = override ?? DictationMode.forApp(bundleID: bundleID)
        return DictationContext(mode: mode, appName: app?.localizedName ?? "", bundleID: bundleID)
    }
}
