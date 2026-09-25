import Combine
import Foundation

/// Every user setting, stored in UserDefaults under the keys the menu has always used. The menu, the Settings window,
/// first-run setup and `Dictation` all read this one object, so a change anywhere shows everywhere.
final class AppSettings: ObservableObject {
    static let shared = AppSettings()

    private let defaults = UserDefaults.standard

    @Published var hotKey: HotKey.Combo { didSet { hotKey.save(to: defaults) } }
    /// Hold to talk: hold the hotkey while speaking and release it to finish. Off = press to start and to stop.
    @Published var holdToTalk: Bool { didSet { defaults.set(holdToTalk, forKey: "holdToTalk") } }
    /// True while the Settings window records a new hotkey, so the current one is unregistered meanwhile.
    @Published var isRecordingHotKey = false
    /// Set when the chosen hotkey couldn't be registered (another app owns it).
    @Published var hotKeyError: String?
    /// Swaps the last paste between the cleaned text and Whisper's text (⌃⌥Z by default).
    @Published var swapHotKey: HotKey.Combo { didSet { swapHotKey.save(to: defaults, prefix: "swapHotKey") } }
    @Published var swapHotKeyError: String?
    /// Command Mode: select text, press it, and say how to change it (⌃⌥⇧Space by default).
    @Published var commandHotKey: HotKey.Combo { didSet { commandHotKey.save(to: defaults, prefix: "commandHotKey") } }
    @Published var commandHotKeyError: String?
    /// The engine Command Mode uses: "claude" (default) or "openai" (the API endpoint). Never S1-mini.
    @Published var commandEngine: String { didSet { defaults.set(commandEngine, forKey: "commandEngine") } }

    @Published var refine: Bool { didSet { defaults.set(refine, forKey: "refine") } }
    @Published var claudeModel: String { didSet { defaults.set(claudeModel, forKey: "claudeModel") } }
    /// "claude" (default), "s1" (S1-mini, fully offline) or "openai" (an OpenAI-compatible endpoint).
    @Published var cleanupEngine: String { didSet { defaults.set(cleanupEngine, forKey: "cleanupEngine") } }
    @Published var s1Fallback: Bool { didSet { defaults.set(s1Fallback, forKey: "s1Fallback") } }
    /// The OpenAI-compatible endpoint (Ollama, LM Studio, OpenAI, Groq, OpenRouter…); its key is in `APIKeychain`.
    @Published var openaiBaseURL: String { didSet { defaults.set(openaiBaseURL, forKey: "openaiBaseURL") } }
    @Published var openaiModel: String { didSet { defaults.set(openaiModel, forKey: "openaiModel") } }

    @Published var autoPaste: Bool { didSet { defaults.set(autoPaste, forKey: "autoPaste") } }
    @Published var richPaste: Bool { didSet { defaults.set(richPaste, forKey: "richPaste") } }
    @Published var showOverlay: Bool { didSet { defaults.set(showOverlay, forKey: "showOverlay") } }
    @Published var overlayPosition: OverlayController.Position {
        didSet { defaults.set(overlayPosition.rawValue, forKey: "overlayPosition") }
    }
    @Published var playSounds: Bool { didSet { defaults.set(playSounds, forKey: "playSounds") } }

    /// nil = Auto (choose from the frontmost app).
    @Published var modeOverride: DictationMode? { didSet { defaults.set(modeOverride?.rawValue, forKey: "modeOverride") } }
    /// The user's own app → mode choices (bundle ID → mode), checked before the built-in list.
    @Published var appModes: [String: DictationMode] {
        didSet { defaults.set(appModes.mapValues(\.rawValue), forKey: "appModes") }
    }
    /// Display names for `appModes`, by bundle ID.
    @Published var appNames: [String: String] { didSet { defaults.set(appNames, forKey: "appNames") } }

    /// nil = system default input.
    @Published var inputDeviceUID: String? { didSet { defaults.set(inputDeviceUID, forKey: "inputDeviceUID") } }
    @Published var inputDeviceName: String? { didSet { defaults.set(inputDeviceName, forKey: "inputDeviceName") } }

    /// File name of the Whisper model in ~/.local/share/whisper (see `ModelCatalog`).
    @Published var whisperModel: String { didSet { defaults.set(whisperModel, forKey: "whisperModel") } }

    @Published var setupCompleted: Bool { didSet { defaults.set(setupCompleted, forKey: "setupCompleted") } }
    @Published var checkForUpdates: Bool { didSet { defaults.set(checkForUpdates, forKey: "checkForUpdates") } }
    /// Dictated text in the log, for debugging. Off by default: the log keeps timings and outcomes only.
    @Published var logText: Bool { didSet { defaults.set(logText, forKey: "logText") } }

    var usesS1: Bool { refine && cleanupEngine == "s1" }
    var usesAPI: Bool { refine && cleanupEngine == "openai" }

    private init() {
        let defaults = UserDefaults.standard // a local, so the helper below doesn't capture self before init ends
        func bool(_ key: String, _ fallback: Bool) -> Bool { defaults.object(forKey: key) as? Bool ?? fallback }
        hotKey = HotKey.Combo.load(from: defaults)
        swapHotKey = HotKey.Combo.load(from: defaults, prefix: "swapHotKey", fallback: HotKey.Combo.defaultSwapCombo)
        commandHotKey = HotKey.Combo.load(from: defaults, prefix: "commandHotKey", fallback: HotKey.Combo.defaultCommandCombo)
        commandEngine = defaults.string(forKey: "commandEngine") ?? "claude"
        holdToTalk = defaults.bool(forKey: "holdToTalk")
        refine = bool("refine", true)
        claudeModel = defaults.string(forKey: "claudeModel") ?? "haiku"
        cleanupEngine = defaults.string(forKey: "cleanupEngine") ?? "claude"
        s1Fallback = bool("s1Fallback", true)
        openaiBaseURL = defaults.string(forKey: "openaiBaseURL") ?? "http://localhost:11434/v1"
        openaiModel = defaults.string(forKey: "openaiModel") ?? ""
        autoPaste = bool("autoPaste", true)
        richPaste = bool("richPaste", true)
        showOverlay = bool("showOverlay", true)
        overlayPosition = OverlayController.Position(rawValue: defaults.string(forKey: "overlayPosition") ?? "") ?? .bottom
        playSounds = bool("playSounds", true)
        modeOverride = defaults.string(forKey: "modeOverride").flatMap(DictationMode.init(rawValue:))
        appModes = (defaults.dictionary(forKey: "appModes") as? [String: String] ?? [:])
            .compactMapValues(DictationMode.init(rawValue:))
        appNames = defaults.dictionary(forKey: "appNames") as? [String: String] ?? [:]
        inputDeviceUID = defaults.string(forKey: "inputDeviceUID")
        inputDeviceName = defaults.string(forKey: "inputDeviceName")
        // Existing installs keep the full model they already have; new ones default to the compressed one.
        whisperModel = defaults.string(forKey: "whisperModel")
            ?? (ModelCatalog.fullWhisper.isInstalled ? ModelCatalog.fullWhisper.fileName : ModelCatalog.defaultWhisper.fileName)
        // Anyone with a Whisper model already set things up with install.sh; a new Mac starts with first-run setup.
        setupCompleted = defaults.object(forKey: "setupCompleted") as? Bool ?? ModelCatalog.whisper.contains { $0.isInstalled }
        checkForUpdates = bool("checkForUpdates", true)
        logText = bool("logText", false)
    }
}
