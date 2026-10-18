import Foundation

/// Apple's on-device engines (macOS 26+, M5.6): SpeechAnalyzer for speech and Apple's language model (Apple Intelligence)
/// for cleanup. The bundled `ovt-apple` helper runs them, through `dictate.sh apple …` and the pipeline commands; this
/// keeps what the menu, the Settings panes and `Dictation` need to know: what's available, and the speech model download.
final class AppleEngines: ObservableObject {
    static let shared = AppleEngines()

    /// `dictate.sh apple status`.
    struct Status: Decodable {
        struct Speech: Decodable {
            var available: Bool?
            var locale: String?
            /// installed, downloading, supported (can be downloaded) or unsupported.
            var assets: String?
        }
        struct Model: Decodable {
            var available: Bool?
            /// deviceNotEligible, appleIntelligenceNotEnabled or modelNotReady.
            var reason: String?
        }
        var speech: Speech?
        var llm: Model?
    }

    @Published private(set) var status: Status?
    /// 0...1 while the speech model downloads.
    @Published private(set) var downloadProgress: Double?
    @Published private(set) var downloadError: String?

    /// Runs `dictate.sh <args>`, calling `onLine` on main for each line it prints, then `completion` with the exit
    /// status. Set by the app delegate.
    var runner: ((_ args: [String], _ onLine: @escaping (String) -> Void,
                  _ completion: @escaping (Int32) -> Void) -> Void)?

    /// macOS 26 or later, with the helper in this build (it needs the macOS 26 SDK): the only case where the Apple
    /// options appear.
    static let isSupported = ProcessInfo.processInfo.isOperatingSystemAtLeast(
        OperatingSystemVersion(majorVersion: 26, minorVersion: 0, patchVersion: 0)) && BundledHelpers.has("ovt-apple")

    var speechAvailable: Bool { Self.isSupported && status?.speech?.available == true }
    /// Available, and the language's model is installed.
    var speechReady: Bool { speechAvailable && status?.speech?.assets == "installed" }
    var speechAssets: String { status?.speech?.assets ?? "" }
    var modelAvailable: Bool { Self.isSupported && status?.llm?.available == true }

    /// Why Apple's language model can't be used, for Settings; nil when it can.
    var modelProblem: String? {
        guard Self.isSupported else { return "Needs macOS 26 or later" }
        guard let model = status?.llm else { return "Checking…" }
        if model.available == true { return nil }
        switch model.reason ?? "" {
        case "appleIntelligenceNotEnabled": return "Turn on Apple Intelligence in System Settings"
        case "deviceNotEligible": return "This Mac can't run Apple Intelligence"
        case "modelNotReady": return "Apple Intelligence is still downloading its model"
        default: return "Apple Intelligence isn't available"
        }
    }

    /// Checks again (a second or so the first time; the script caches it for a minute).
    func refresh() {
        guard Self.isSupported, let runner else { return }
        var output = ""
        runner(["apple", "status"], { output += $0 }) { [weak self] _ in
            guard let decoded = try? JSONDecoder().decode(Status.self, from: Data(output.utf8)) else { return }
            self?.status = decoded
        }
    }

    /// Downloads the speech model for the dictation language. macOS stores it and shares it between apps.
    func downloadSpeechModel() {
        guard Self.isSupported, let runner, downloadProgress == nil else { return }
        downloadProgress = 0
        downloadError = nil
        runner(["apple", "speech-install"], { [weak self] line in
            if line.hasPrefix("progress "), let value = Double(line.dropFirst("progress ".count)) {
                self?.downloadProgress = value
            }
        }) { [weak self] status in
            self?.downloadProgress = nil
            if status != 0 { self?.downloadError = "The download failed. Check the connection and try again." }
            self?.refresh()
        }
    }
}
