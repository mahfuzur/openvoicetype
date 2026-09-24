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

    @Published var refine: Bool { didSet { defaults.set(refine, forKey: "refine") } }
    @Published var claudeModel: String { didSet { defaults.set(claudeModel, forKey: "claudeModel") } }
    /// "claude" (default) or "s1" (S1-mini, fully offline).
    @Published var cleanupEngine: String { didSet { defaults.set(cleanupEngine, forKey: "cleanupEngine") } }
    @Published var s1Fallback: Bool { didSet { defaults.set(s1Fallback, forKey: "s1Fallback") } }

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

    var usesS1: Bool { refine && cleanupEngine == "s1" }

    private init() {
        let defaults = UserDefaults.standard // a local, so the helper below doesn't capture self before init ends
        func bool(_ key: String, _ fallback: Bool) -> Bool { defaults.object(forKey: key) as? Bool ?? fallback }
        hotKey = HotKey.Combo.load(from: defaults)
        holdToTalk = defaults.bool(forKey: "holdToTalk")
        refine = bool("refine", true)
        claudeModel = defaults.string(forKey: "claudeModel") ?? "haiku"
        cleanupEngine = defaults.string(forKey: "cleanupEngine") ?? "claude"
        s1Fallback = bool("s1Fallback", true)
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
    }
}
