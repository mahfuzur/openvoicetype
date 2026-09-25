import Foundation

/// The dictation pipeline: records in-process (for live levels), then runs the bundled
/// dictate.sh in two stages, `transcribe <wav>` and `refine` (stdin), so the UI can show each stage.
///
/// Speed (M3): when recording starts, `refine` is already launched (it starts Claude, then waits for the transcript)
/// and `whisper-server` is loaded, so neither startup cost is paid after you stop speaking.
///
/// Where the text goes is decided when recording starts (v0.3.0): the frontmost app and window (`PasteTarget`), and the
/// mode for that app. The app delegate checks the target again before pasting.
final class Dictation {
    enum State { case idle, starting, recording, transcribing, polishing, testingMic }

    /// Stage timings for one dictation, for the `APP TIMING` log line.
    struct Timing {
        let stoppedAt: Date
        var transcribeMs = 0
        var cleanupMs = 0
        /// The transcript went to a `refine` started when recording began.
        var prestarted = false
    }

    /// What `refine` reported (its VTT_RESULT_FILE): the engine that cleaned up, why the online engine failed, and the
    /// meaning guard's reason with the cleanup it turned down.
    struct CleanupDetails: Decodable {
        var status = ""
        /// claude, openai, s1 or none.
        var engine = ""
        /// limit, auth, auth-mismatch, offline, timeout, config or error; empty when nothing failed.
        var error = ""
        var resets = ""
        var guardReason = ""
        var rejected = ""
        /// Whisper's text after the dictionary and output filter: what "swap to Whisper's text" pastes.
        var raw = ""

        enum CodingKeys: String, CodingKey { case status, engine, error, resets, guardReason = "guard", rejected, raw }

        init() {}
        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            func value(_ key: CodingKeys) -> String { (try? container.decode(String.self, forKey: key)) ?? "" }
            status = value(.status)
            engine = value(.engine)
            error = value(.error)
            resets = value(.resets)
            guardReason = value(.guardReason)
            rejected = value(.rejected)
            raw = value(.raw)
        }
    }

    struct Result {
        let text: String
        /// Whisper's transcript, before any cleanup.
        let raw: String
        /// Cleanup was expected but didn't happen: the text is Whisper's.
        let cleanupFailed: Bool
        /// The online engine was unavailable and S1-mini cleaned the text up instead.
        let offlineFallback: Bool
        let details: CleanupDetails
        let context: DictationContext
        let target: PasteTarget
        let timing: Timing
    }

    enum Outcome {
        case text(Result)
        case noSpeech
        case cancelled
        case failed(String)
        /// Not an error: dictation isn't allowed here (a password field).
        case refused(String)
    }

    private(set) var state: State = .idle {
        didSet { onStateChange?(state) }
    }

    var onStateChange: ((State) -> Void)?
    var onLevel: ((Float) -> Void)?
    var onFinish: ((Outcome) -> Void)?
    /// The app and mode that will receive the text, asked when recording starts.
    var contextProvider: (() -> DictationContext)?

    /// The mic used by the current or most recent recording.
    var deviceName: String { recorder.deviceName }

    private let scriptURL: URL
    private let settings = AppSettings.shared
    private let recorder = Recorder()
    private var maxDurationTimer: Timer?
    private let maxDuration: TimeInterval = 300
    private static let workDirectory = FileManager.default.temporaryDirectory.appendingPathComponent("voice-to-text")
    private let recordingURL = Dictation.workDirectory.appendingPathComponent("app-recording.wav")
    private let micTestURL = Dictation.workDirectory.appendingPathComponent("mic-test.wav")
    private var micTestPeak: Float = 0
    /// `refine` started when recording started, waiting for the transcript on stdin.
    private var prestartedRefine: ScriptRun?
    /// Where the text goes, fixed when recording starts.
    private var startContext: DictationContext?
    private var startTarget: PasteTarget?

    init(scriptURL: URL) {
        self.scriptURL = scriptURL
        // Key and result files a crash left behind (each normally lives for one run).
        let leftovers = (try? FileManager.default.contentsOfDirectory(atPath: Self.workDirectory.path)) ?? []
        for name in leftovers where name.hasPrefix("key-") || name.hasPrefix("result-") {
            try? FileManager.default.removeItem(at: Self.workDirectory.appendingPathComponent(name))
        }
        recorder.onLevel = { [weak self] level in
            guard let self else { return }
            if self.state == .testingMic { self.micTestPeak = max(self.micTestPeak, level) }
            self.onLevel?(level)
        }
    }

    func toggle() {
        switch state {
        case .idle: start()
        case .starting: cancel()
        case .recording: stop()
        case .transcribing, .polishing, .testingMic: break // busy; the overlay already shows progress
        }
    }

    func start() {
        guard state == .idle else { return }
        let target = PasteTarget.capture()
        guard !target.isSecure else {
            return finish(.refused("Not in password fields"))
        }
        startTarget = target
        startContext = contextProvider?() ?? DictationContext.current(override: nil)
        state = .starting
        prestart()
        recorder.start(to: recordingURL, deviceUID: settings.inputDeviceUID, onReady: { [weak self] in
            guard let self, self.state == .starting else { return }
            self.state = .recording
            self.maxDurationTimer = Timer.scheduledTimer(withTimeInterval: self.maxDuration, repeats: false) { [weak self] _ in
                self?.stop()
            }
        }, onFailure: { [weak self] error in
            guard let self else { return }
            switch self.state {
            case .recording:
                // The mic dropped out mid-dictation: transcribe what was captured so far.
                self.stop()
            case .starting:
                self.maxDurationTimer?.invalidate()
                self.dropPrestart()
                self.finish(.failed(error.localizedDescription))
            default:
                break
            }
        })
    }

    /// Records for `duration` seconds without transcribing and reports the peak input level (0...1).
    func testMicrophone(duration: TimeInterval, onReady: @escaping () -> Void,
                        completion: @escaping (Swift.Result<Float, Error>) -> Void) {
        guard state == .idle else { return }
        micTestPeak = 0
        state = .testingMic
        recorder.start(to: micTestURL, deviceUID: settings.inputDeviceUID, onReady: { [weak self] in
            onReady()
            DispatchQueue.main.asyncAfter(deadline: .now() + duration) {
                guard let self, self.state == .testingMic else { return }
                self.recorder.cancel()
                self.state = .idle
                completion(.success(self.micTestPeak))
            }
        }, onFailure: { [weak self] error in
            self?.recorder.cancel()
            self?.state = .idle
            completion(.failure(error))
        })
    }

    func stop() {
        guard state == .recording else { return }
        maxDurationTimer?.invalidate()
        guard let wav = recorder.stop() else {
            dropPrestart()
            finish(.failed("Recording failed"))
            return
        }

        let context = startContext ?? DictationContext.current(override: nil)
        let target = startTarget ?? PasteTarget.capture()
        var timing = Timing(stoppedAt: Date())
        state = .transcribing
        run(["transcribe", wav.path], timeout: 120) { [weak self] status, output, _ in
            guard let self else { return }
            try? FileManager.default.removeItem(at: wav)
            timing.transcribeMs = Self.milliseconds(since: timing.stoppedAt)
            let raw = output.trimmingCharacters(in: .whitespacesAndNewlines)
            guard status == 0 else { self.dropPrestart(); return self.finish(.failed("Transcription failed")) }
            guard !raw.isEmpty else { self.dropPrestart(); return self.finish(.noSpeech) }

            // Always run `refine`: with cleanup off or in Raw mode it skips the model but still applies
            // the dictionary replacements and output filter. S1-mini has no code style, so it skips code mode.
            let usesCleanup = self.settings.refine && context.mode != .raw
                && !(self.settings.cleanupEngine == "s1" && context.mode == .code)
            if usesCleanup { self.state = .polishing }

            // The `refine` started with recording was started for this context.
            let refineRun: ScriptRun?
            if let prestarted = self.prestartedRefine {
                self.prestartedRefine = nil
                refineRun = prestarted
                timing.prestarted = true
            } else {
                refineRun = self.launch(["refine"], extraEnv: Self.contextEnv(context))
            }
            let cleanupStarted = Date()
            guard let refineRun else {
                return self.finish(.text(Result(text: raw, raw: raw, cleanupFailed: usesCleanup, offlineFallback: false,
                                                details: CleanupDetails(), context: context, target: target,
                                                timing: timing)))
            }
            // A stuck CLI must not hold the dictation: after the script's own limits (Claude 15 s, S1-mini 10 s), stop it
            // and paste Whisper's text.
            refineRun.finish(input: raw, timeout: 45) { status, output, details in
                timing.cleanupMs = Self.milliseconds(since: cleanupStarted)
                let cleaned = output.trimmingCharacters(in: .whitespacesAndNewlines)
                // Exit 4: the online engine was unavailable and S1-mini cleaned it up. Exit 3: raw text.
                // Exit 5: the meaning guard pasted Whisper's text (the reason is in `details`).
                let offlineFallback = usesCleanup && status == 4 && !cleaned.isEmpty
                let failed = usesCleanup && !offlineFallback && status != 5 && (status != 0 || cleaned.isEmpty)
                self.finish(.text(Result(text: cleaned.isEmpty ? raw : cleaned, raw: raw, cleanupFailed: failed,
                                         offlineFallback: offlineFallback, details: details ?? CleanupDetails(),
                                         context: context, target: target, timing: timing)))
            }
        }
    }

    func cancel() {
        guard state == .starting || state == .recording else { return }
        maxDurationTimer?.invalidate()
        recorder.cancel()
        dropPrestart()
        finish(.cancelled)
    }

    /// Runs `dictate.sh <name> <args>` for a local model server (s1-server, whisper-server): `start --keep` while the
    /// app wants it loaded, `release` to let it stop after idling, `status` (prints running, stopped or missing).
    func server(_ name: String, _ args: [String], completion: ((Int32, String) -> Void)? = nil) {
        run([name] + args) { status, output, _ in completion?(status, output) }
    }

    /// Runs a sample transcript through `refine` like a dictation (Settings → Cleanup → Test). Calls back on main with
    /// the text, which engine produced it and the time taken.
    func testCleanup(_ sample: String, completion: @escaping (_ text: String, _ engine: String, _ seconds: Double) -> Void) {
        let started = Date()
        run(["refine"], input: sample, extraEnv: ["VTT_MODE": "default", "VTT_APP": "OpenVoiceType"],
            timeout: 60) { [weak self] status, output, details in
            guard let self else { return }
            let text = output.trimmingCharacters(in: .whitespacesAndNewlines)
            let problem = details.map(Self.problemDescription) ?? nil
            let engine: String = switch status {
            case 0 where !self.settings.refine: "No cleanup (turned off)"
            case 0: Self.engineName(details?.engine ?? "", settings: self.settings)
            case 4: "S1-mini" + (problem.map { " (\($0))" } ?? " (the online engine was unavailable)")
            case 5: "Whisper's text: the cleanup dropped \(details?.guardReason ?? "something")"
            default: "Raw text" + (problem.map { " (\($0))" } ?? " (cleanup failed; see the log)")
            }
            completion(text.isEmpty ? sample : text, engine, Date().timeIntervalSince(started))
        }
    }

    /// A short name for the engine `refine` reported.
    static func engineName(_ engine: String, settings: AppSettings) -> String {
        switch engine {
        case "claude": "Claude \(settings.claudeModel.capitalized)"
        case "openai": settings.openaiModel.isEmpty ? "the API" : settings.openaiModel
        case "s1": "S1-mini"
        default: "no cleanup"
        }
    }

    /// Why the online engine failed, for the overlay and the test result; nil when nothing failed or it was offline.
    static func problemDescription(_ details: CleanupDetails) -> String? {
        let claude = AppSettings.shared.cleanupEngine != "openai" // the online engine that failed
        switch details.error {
        case "limit":
            let resets = details.resets.isEmpty ? "" : ", resets \(details.resets)"
            return (claude ? "Claude limit reached" : "API rate limit") + resets
        case "auth": return claude ? "Claude isn't signed in" : "API key rejected"
        case "auth-mismatch": return "Claude Code changed how scripts sign in; see the log"
        case "config": return "set up the API in Settings"
        case "timeout": return claude ? "Claude timed out" : "the API timed out"
        default: return nil
        }
    }

    /// Unloads whisper-server, so the next dictation loads the newly chosen model.
    func reloadWhisperModel() {
        server("whisper-server", ["stop"])
    }

    /// Loads a server once so the bundled build compiles its Metal shaders now (10–20 s the very first time, then
    /// cached by macOS) instead of during the first dictation. It then stops after idling as usual.
    func warmUp(_ name: String) {
        server(name, ["start"]) { [weak self] _, _ in self?.server(name, ["release"]) }
    }

    /// Before the app quits: ends a waiting `refine` and stops both servers (blocks briefly).
    func shutDown() {
        prestartedRefine?.terminate()
        prestartedRefine = nil
        for name in ["s1-server", "whisper-server"] {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/bin/bash")
            process.arguments = [scriptURL.path, name, "stop"]
            process.environment = environment()
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try? process.run()
            process.waitUntilExit()
        }
    }

    /// Starts the slow parts while you speak: loads whisper-server, and launches `refine`, which starts Claude and
    /// then waits for the transcript. It uses the context fixed when recording started.
    private func prestart() {
        server("whisper-server", ["start"])
        dropPrestart()
        guard let context = startContext, settings.refine, context.mode != .raw,
              let run = launch(["refine"], extraEnv: Self.contextEnv(context)) else { return }
        prestartedRefine = run
    }

    /// Ends a waiting `refine` without a transcript: the script sees empty input and exits, and so does its Claude.
    private func dropPrestart() {
        prestartedRefine?.finish(input: nil)
        prestartedRefine = nil
    }

    private func finish(_ outcome: Outcome) {
        state = .idle
        onFinish?(outcome)
    }

    private static func contextEnv(_ context: DictationContext) -> [String: String] {
        ["VTT_MODE": context.mode.rawValue, "VTT_APP": context.appName]
    }

    private static func milliseconds(since date: Date) -> Int { Int(Date().timeIntervalSince(date) * 1000) }

    private func environment(_ extra: [String: String] = [:]) -> [String: String] {
        var env = ProcessInfo.processInfo.environment.merging(extra) { _, new in new }
        env["VTT_QUIET"] = "on"
        env["VTT_REFINE"] = settings.refine ? "on" : "off"
        env["VTT_CLAUDE_MODEL"] = settings.claudeModel
        env["VTT_CLEANUP"] = settings.cleanupEngine
        env["VTT_S1_FALLBACK"] = settings.s1Fallback ? "on" : "off"
        env["VTT_LOG_TEXT"] = settings.logText ? "on" : "off"
        env["VTT_OPENAI_BASE_URL"] = settings.openaiBaseURL.trimmingCharacters(in: .whitespaces)
        env["VTT_OPENAI_MODEL"] = settings.openaiModel.trimmingCharacters(in: .whitespaces)
        if let helpers = BundledHelpers.directory { env["VTT_BIN_DIR"] = helpers.path }
        if let model = ModelCatalog.whisperModel(named: settings.whisperModel), model.isInstalled {
            env["VTT_WHISPER_MODEL"] = model.path.path
        }
        // The Claude CLI found through the login shell (npm and nvm installs aren't on the app's PATH); an npm
        // install is a node script that needs its `node` from the same directory.
        if let claude = ClaudeCLI.shared.path {
            env["VTT_CLAUDE_BIN"] = claude
            env["PATH"] = (claude as NSString).deletingLastPathComponent + ":" + (env["PATH"] ?? "/usr/bin:/bin")
        }
        return env
    }

    /// Launches the script now; its stdin stays open until `finish(input:)`. A `refine` gets a private result file, and,
    /// with the API engine, a private file holding the key for this one run; both are deleted when it exits.
    private func launch(_ args: [String], extraEnv: [String: String] = [:]) -> ScriptRun? {
        var env = extraEnv
        var files: [URL] = []
        var resultURL: URL?
        if args.first == "refine" {
            try? FileManager.default.createDirectory(at: Self.workDirectory, withIntermediateDirectories: true)
            let result = Self.workDirectory.appendingPathComponent("result-\(UUID().uuidString).json")
            env["VTT_RESULT_FILE"] = result.path
            files.append(result)
            resultURL = result
            if settings.cleanupEngine == "openai", let key = APIKeychain.key(for: settings.openaiBaseURL),
               let keyFile = Self.privateFile(named: "key-\(UUID().uuidString)", contents: key) {
                env["VTT_OPENAI_KEY_FILE"] = keyFile.path
                files.append(keyFile)
            }
        }
        let run = ScriptRun(scriptPath: scriptURL.path, args: args, environment: environment(env), files: files,
                            resultURL: resultURL)
        if run == nil { files.forEach { try? FileManager.default.removeItem(at: $0) } }
        return run
    }

    /// A file only this user can read (0600), in the app's temporary directory.
    private static func privateFile(named name: String, contents: String) -> URL? {
        let url = workDirectory.appendingPathComponent(name)
        guard FileManager.default.createFile(atPath: url.path, contents: Data(contents.utf8),
                                             attributes: [.posixPermissions: 0o600]) else { return nil }
        return url
    }

    /// Runs the script with `input` on stdin; calls back on main with exit status, stdout and (for `refine`) its details.
    private func run(_ args: [String], input: String? = nil, extraEnv: [String: String] = [:], timeout: TimeInterval? = nil,
                     completion: @escaping (Int32, String, CleanupDetails?) -> Void) {
        guard let run = launch(args, extraEnv: extraEnv) else { return completion(-1, "", nil) }
        run.finish(input: input, timeout: timeout, completion: completion)
    }
}

/// One `dictate.sh` process whose stdin is written later, so it can start work (Claude) before its input exists.
final class ScriptRun {
    private let process = Process()
    private let stdin = Pipe()
    private let stdout = Pipe()
    private var finished = false
    /// Deleted when the process exits (the result file, a key file).
    private let files: [URL]
    private let resultURL: URL?

    init?(scriptPath: String, args: [String], environment: [String: String], files: [URL] = [], resultURL: URL? = nil) {
        self.files = files
        self.resultURL = resultURL
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [scriptPath] + args
        process.environment = environment
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return nil }
    }

    /// Writes `input` (nil writes nothing), closes stdin, and calls back on main with the exit status, stdout and the
    /// details `refine` wrote. With `timeout`, a run still going after that many seconds is terminated (its output is
    /// then empty, and the caller falls back).
    func finish(input: String?, timeout: TimeInterval? = nil,
                completion: ((Int32, String, Dictation.CleanupDetails?) -> Void)? = nil) {
        guard !finished else { return }
        finished = true
        let (process, stdin, stdout, files, resultURL) = (self.process, self.stdin, self.stdout, self.files, self.resultURL)
        if let timeout {
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout) {
                guard process.isRunning else { return }
                AppLog.write("SCRIPT \(process.arguments?.dropFirst().first ?? "?") still running after \(Int(timeout)) s, stopped")
                process.terminate()
            }
        }
        DispatchQueue.global(qos: .userInitiated).async {
            // `write(contentsOf:)` throws instead of raising an uncatchable exception if the script already exited.
            if let input { try? stdin.fileHandleForWriting.write(contentsOf: Data(input.utf8)) }
            try? stdin.fileHandleForWriting.close()
            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            let output = String(decoding: data, as: UTF8.self)
            let details = resultURL.flatMap { try? Data(contentsOf: $0) }
                .flatMap { try? JSONDecoder().decode(Dictation.CleanupDetails.self, from: $0) }
            files.forEach { try? FileManager.default.removeItem(at: $0) }
            DispatchQueue.main.async { completion?(process.terminationStatus, output, details) }
        }
    }

    func terminate() {
        finished = true
        if process.isRunning { process.terminate() }
        files.forEach { try? FileManager.default.removeItem(at: $0) }
    }
}
