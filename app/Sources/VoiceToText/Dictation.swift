import Foundation

/// The dictation pipeline: records in-process (for live levels), then runs the bundled
/// dictate.sh in two stages, `transcribe <wav>` and `refine` (stdin), so the UI can show each stage.
///
/// Speed (M3): when recording starts, `refine` is already launched (it starts Claude, then waits for the transcript)
/// and `whisper-server` is loaded, so neither startup cost is paid after you stop speaking.
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

    enum Outcome {
        /// `offlineFallback`: Claude was unavailable and S1-mini cleaned the text up instead.
        case text(String, cleanupFailed: Bool, offlineFallback: Bool, context: DictationContext, timing: Timing)
        case noSpeech
        case cancelled
        case failed(String)
    }

    private(set) var state: State = .idle {
        didSet { onStateChange?(state) }
    }

    var onStateChange: ((State) -> Void)?
    var onLevel: ((Float) -> Void)?
    var onFinish: ((Outcome) -> Void)?
    /// Where the text will go, asked when recording stops (the paste target has focus then).
    var contextProvider: (() -> DictationContext)?

    /// The mic used by the current or most recent recording.
    var deviceName: String { recorder.deviceName }

    private let scriptURL: URL
    private let settings = AppSettings.shared
    /// The app's own whisper-server, whisper-cli and llama-server (M4), if this build bundles them.
    private let helpersURL = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers")
    private let recorder = Recorder()
    private var maxDurationTimer: Timer?
    private let maxDuration: TimeInterval = 300
    private let recordingURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("voice-to-text/app-recording.wav")
    private let micTestURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("voice-to-text/mic-test.wav")
    private var micTestPeak: Float = 0
    /// `refine` started when recording started, waiting for the transcript on stdin, and the mode it was started for.
    private var prestartedRefine: (run: ScriptRun, mode: DictationMode)?

    init(scriptURL: URL) {
        self.scriptURL = scriptURL
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
                        completion: @escaping (Result<Float, Error>) -> Void) {
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

        let context = contextProvider?() ?? DictationContext.current(override: nil)
        var timing = Timing(stoppedAt: Date())
        state = .transcribing
        run(["transcribe", wav.path]) { [weak self] status, output in
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

            // Use the `refine` started with recording if it was started for this mode (you may have switched apps).
            let refineRun: ScriptRun?
            if let prestarted = self.prestartedRefine, prestarted.mode == context.mode {
                self.prestartedRefine = nil
                refineRun = prestarted.run
                timing.prestarted = true
            } else {
                self.dropPrestart()
                refineRun = self.launch(["refine"], extraEnv: Self.contextEnv(context))
            }
            let cleanupStarted = Date()
            guard let refineRun else {
                return self.finish(.text(raw, cleanupFailed: usesCleanup, offlineFallback: false,
                                         context: context, timing: timing))
            }
            refineRun.finish(input: raw) { status, output in
                timing.cleanupMs = Self.milliseconds(since: cleanupStarted)
                let cleaned = output.trimmingCharacters(in: .whitespacesAndNewlines)
                // Exit 4: Claude was unavailable and S1-mini cleaned it up. Exit 3: raw text.
                let offlineFallback = usesCleanup && status == 4 && !cleaned.isEmpty
                let failed = usesCleanup && !offlineFallback && (status != 0 || cleaned.isEmpty)
                self.finish(.text(cleaned.isEmpty ? raw : cleaned, cleanupFailed: failed,
                                  offlineFallback: offlineFallback, context: context, timing: timing))
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
        run([name] + args) { status, output in completion?(status, output) }
    }

    /// Runs a sample transcript through `refine` like a dictation (Settings → Cleanup → Test). Calls back on main with
    /// the text, which engine produced it and the time taken.
    func testCleanup(_ sample: String, completion: @escaping (_ text: String, _ engine: String, _ seconds: Double) -> Void) {
        let started = Date()
        run(["refine"], input: sample, extraEnv: ["VTT_MODE": "default", "VTT_APP": "Voice to Text"]) { [weak self] status, output in
            guard let self else { return }
            let text = output.trimmingCharacters(in: .whitespacesAndNewlines)
            let engine: String = switch status {
            case 0 where !self.settings.refine: "No cleanup (turned off)"
            case 0: self.settings.cleanupEngine == "s1" ? "S1-mini" : "Claude \(self.settings.claudeModel.capitalized)"
            case 4: "S1-mini (Claude was unavailable)"
            default: "Raw text (cleanup failed; see the log)"
            }
            completion(text.isEmpty ? sample : text, engine, Date().timeIntervalSince(started))
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
        prestartedRefine?.run.terminate()
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
    /// then waits for the transcript. It uses the mode of the app in front now; stop() checks the mode again.
    private func prestart() {
        server("whisper-server", ["start"])
        dropPrestart()
        let context = contextProvider?() ?? DictationContext.current(override: nil)
        guard settings.refine, context.mode != .raw,
              let run = launch(["refine"], extraEnv: Self.contextEnv(context)) else { return }
        prestartedRefine = (run, context.mode)
    }

    /// Ends a waiting `refine` without a transcript: the script sees empty input and exits, and so does its Claude.
    private func dropPrestart() {
        prestartedRefine?.run.finish(input: nil)
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
        if FileManager.default.fileExists(atPath: helpersURL.path) { env["VTT_BIN_DIR"] = helpersURL.path }
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

    /// Launches the script now; its stdin stays open until `finish(input:)`.
    private func launch(_ args: [String], extraEnv: [String: String] = [:]) -> ScriptRun? {
        ScriptRun(scriptPath: scriptURL.path, args: args, environment: environment(extraEnv))
    }

    /// Runs the script with `input` on stdin; calls back on main with exit status and stdout.
    private func run(_ args: [String], input: String? = nil, extraEnv: [String: String] = [:],
                     completion: @escaping (Int32, String) -> Void) {
        guard let run = launch(args, extraEnv: extraEnv) else { return completion(-1, "") }
        run.finish(input: input, completion: completion)
    }
}

/// One `dictate.sh` process whose stdin is written later, so it can start work (Claude) before its input exists.
final class ScriptRun {
    private let process = Process()
    private let stdin = Pipe()
    private let stdout = Pipe()
    private var finished = false

    init?(scriptPath: String, args: [String], environment: [String: String]) {
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [scriptPath] + args
        process.environment = environment
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return nil }
    }

    /// Writes `input` (nil writes nothing), closes stdin, and calls back on main with the exit status and stdout.
    func finish(input: String?, completion: ((Int32, String) -> Void)? = nil) {
        guard !finished else { return }
        finished = true
        let (process, stdin, stdout) = (self.process, self.stdin, self.stdout)
        DispatchQueue.global(qos: .userInitiated).async {
            // `write(contentsOf:)` throws instead of raising an uncatchable exception if the script already exited.
            if let input { try? stdin.fileHandleForWriting.write(contentsOf: Data(input.utf8)) }
            try? stdin.fileHandleForWriting.close()
            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            let output = String(decoding: data, as: UTF8.self)
            DispatchQueue.main.async { completion?(process.terminationStatus, output) }
        }
    }

    func terminate() {
        finished = true
        if process.isRunning { process.terminate() }
    }
}
