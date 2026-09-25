import AppKit
import AVFoundation
import Carbon
import Combine
import ObjCSupport

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var statusItem: NSStatusItem!
    private var menuBarIcon: MenuBarIcon!
    private var dictation: Dictation!
    private var hotKey: HotKey?
    private var swapKey: HotKey?
    private var escapeKey: HotKey?
    private var lastResult: String?
    private let history = DictationHistory()
    private let overlay = OverlayController()
    private var connectingWork: DispatchWorkItem?

    private let settings = AppSettings.shared
    private let claude = ClaudeCLI.shared
    private let models = ModelManager.shared
    private let updater = Updater.shared
    private var subscriptions: Set<AnyCancellable> = []
    private lazy var settingsWindow = SettingsWindowController(actions: actions)
    private lazy var setupWindow = SetupWindowController(actions: actions, lastResult: lastResultModel)
    private let lastResultModel = LastResult()
    private var updateTimer: Timer?

    /// From `dictate.sh s1-server status`; nil until the first check finishes.
    private var s1Installed: Bool?
    private var hotKeyLabel: String { settings.hotKey.label }
    /// When the hotkey went down in hold-to-talk mode.
    private var holdStartedAt: Date?
    /// Set when a hold-to-talk press was too short, so the overlay explains instead of saying "Cancelled".
    private var showHoldHint = false

    private var actions: AppActions {
        AppActions(
            testMicrophone: { [weak self] in self?.testMicrophone() },
            testCleanup: { [weak self] sample, completion in self?.dictation.testCleanup(sample, completion: completion) },
            openSetup: { [weak self] in self?.setupWindow.show() },
            openSettings: { [weak self] in self?.settingsWindow.show() })
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard let script = Bundle.main.url(forResource: "dictate", withExtension: "sh") else {
            fatalAlert("dictate.sh is missing from the app bundle. Rebuild with scripts/build-app.sh.")
            return
        }
        dictation = Dictation(scriptURL: script)
        dictation.onStateChange = { [weak self] state in self?.stateChanged(state) }
        dictation.onLevel = { [weak self] level in self?.overlay.push(level: level) }
        dictation.onFinish = { [weak self] outcome in self?.finished(outcome) }
        dictation.contextProvider = { [weak self] in
            DictationContext.current(override: self?.settings.modeOverride, custom: self?.settings.appModes ?? [:])
        }
        overlay.isEnabled = settings.showOverlay
        overlay.position = settings.overlayPosition

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        let menu = NSMenu()
        menu.delegate = self
        statusItem.menu = menu
        menuBarIcon = MenuBarIcon(button: statusItem.button!)

        if let index = CommandLine.arguments.firstIndex(of: "--overlay-snapshots"),
           index + 1 < CommandLine.arguments.count {
            let directory = URL(fileURLWithPath: CommandLine.arguments[index + 1])
            Task { @MainActor in
                OverlaySnapshots.render(to: directory)
                NSApp.terminate(nil)
            }
            return
        }
        if let index = CommandLine.arguments.firstIndex(of: "--settings-snapshots"),
           index + 1 < CommandLine.arguments.count {
            claude.refresh()
            SettingsSnapshots.render(to: URL(fileURLWithPath: CommandLine.arguments[index + 1]), actions: actions,
                                     lastResult: lastResultModel) { NSApp.terminate(nil) }
            return
        }
        if let index = CommandLine.arguments.firstIndex(of: "--recorder-selftest"),
           index + 1 < CommandLine.arguments.count {
            let pinDefault = CommandLine.arguments.contains("--pin-default")
            runRecorderSelfTest(report: URL(fileURLWithPath: CommandLine.arguments[index + 1]),
                                deviceUID: pinDefault ? AudioDevices.defaultInput()?.uid : settings.inputDeviceUID)
            return
        }
        if let index = CommandLine.arguments.firstIndex(of: "--logic-selftest"), index + 1 < CommandLine.arguments.count {
            // For development: checks the parts of pasting that need no keystrokes, and writes one line per check.
            let report = URL(fileURLWithPath: CommandLine.arguments[index + 1])
            try? LogicSelfTest.run().joined(separator: "\n").appending("\n").write(to: report, atomically: true, encoding: .utf8)
            NSApp.terminate(nil)
            return
        }
        if CommandLine.arguments.contains("--settings-window-test") {
            // For development: opens Settings, prints the window's content size once it has settled, and quits.
            settingsWindow.show()
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                let size = self.settingsWindow.window?.contentLayoutRect.size ?? .zero
                print("settings window \(Int(size.width))x\(Int(size.height))")
                NSApp.terminate(nil)
            }
            return
        }
        if CommandLine.arguments.contains("--overlay-demo") {
            runOverlayDemo()
            return
        }
        // Before the hotkey: a running old "Voice to Text" would own it until it's quit.
        Migration.offerToRemoveOldApp()
        registerHotKey()
        observeSettings()
        claude.refresh()
        models.onInstalled = { [weak self] model in self?.modelInstalled(model) }
        refreshS1Status()
        updateS1Server()
        warmUpNewHelpers()
        updater.checkIfDue()
        updateTimer = Timer.scheduledTimer(withTimeInterval: 3600, repeats: true) { [weak self] _ in
            self?.updater.checkIfDue()
        }
        if settings.setupCompleted && !AppLocation.needsMove {
            requestMicrophoneIfNeeded()
            if !AXIsProcessTrusted() { Permissions.promptAccessibility() }
        } else {
            setupWindow.show()
        }
    }

    /// Applies setting changes made anywhere (menu, Settings, setup).
    private func observeSettings() {
        settings.$hotKey.dropFirst().removeDuplicates().sink { [weak self] _ in
            DispatchQueue.main.async { self?.registerHotKey() }
        }.store(in: &subscriptions)
        settings.$swapHotKey.dropFirst().removeDuplicates().sink { [weak self] _ in
            DispatchQueue.main.async { self?.registerHotKey() }
        }.store(in: &subscriptions)
        settings.$isRecordingHotKey.dropFirst().removeDuplicates().sink { [weak self] recording in
            // The recorder needs the key events: a registered hotkey would swallow its own combo.
            if recording {
                self?.hotKey = nil
                self?.swapKey = nil
            } else {
                DispatchQueue.main.async { self?.registerHotKey() }
            }
        }.store(in: &subscriptions)
        settings.$showOverlay.dropFirst().sink { [weak self] in self?.overlay.isEnabled = $0 }.store(in: &subscriptions)
        settings.$overlayPosition.dropFirst().removeDuplicates().sink { [weak self] position in
            self?.overlay.position = position
            self?.overlay.finish(.message("Overlay position", isError: false), after: 1.2) // preview where it appears
        }.store(in: &subscriptions)
        Publishers.CombineLatest(settings.$refine, settings.$cleanupEngine).dropFirst().sink { [weak self] _ in
            DispatchQueue.main.async { self?.updateS1Server() }
        }.store(in: &subscriptions)
        settings.$whisperModel.dropFirst().removeDuplicates().sink { [weak self] _ in
            self?.dictation.reloadWhisperModel()
        }.store(in: &subscriptions)
    }

    /// After an install or an update, the helpers compile their Metal shaders on first launch (10–20 s; macOS caches the
    /// result per binary and location). Do it now, in the background, instead of during the first dictation.
    private func warmUpNewHelpers() {
        guard let helper = BundledHelpers.directory?.appendingPathComponent("whisper-server"),
              let attributes = try? FileManager.default.attributesOfItem(atPath: helper.path),
              let modified = attributes[.modificationDate] as? Date else { return }
        let stamp = "\(Updater.currentVersion)-\(Int(modified.timeIntervalSince1970))-\(helper.path)"
        guard UserDefaults.standard.string(forKey: "warmedHelpers") != stamp else { return }
        UserDefaults.standard.set(stamp, forKey: "warmedHelpers")
        if ModelCatalog.whisperModel(named: settings.whisperModel)?.isInstalled == true {
            dictation.warmUp("whisper-server")
        }
        if ModelCatalog.s1Mini.isInstalled && !settings.usesS1 { dictation.warmUp("s1-server") }
    }

    /// A download finished: load it once now, so the bundled build's first-launch shader compile doesn't slow
    /// down the first dictation.
    private func modelInstalled(_ model: ModelFile) {
        // A new user who downloaded a different model than the selected one (from its own row) should get it.
        if model.directory == ModelCatalog.whisperDirectory,
           ModelCatalog.whisperModel(named: settings.whisperModel)?.isInstalled != true {
            settings.whisperModel = model.fileName
        }
        if model == ModelCatalog.s1Mini {
            refreshS1Status()
            if settings.usesS1 { updateS1Server() } else { dictation.warmUp("s1-server") }
        } else if model.fileName == settings.whisperModel {
            dictation.warmUp("whisper-server")
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        dictation?.shutDown()
    }

    // MARK: - S1-mini server

    private func refreshS1Status() {
        dictation.server("s1-server", ["status"]) { [weak self] _, output in
            self?.s1Installed = output.trimmingCharacters(in: .whitespacesAndNewlines) != "missing"
                && ModelCatalog.s1Mini.isInstalled
        }
    }

    /// Keeps S1-mini loaded while it's the selected cleanup; otherwise lets it stop after idling
    /// (a Claude fallback starts it on demand).
    private func updateS1Server() {
        dictation.server("s1-server", settings.usesS1 ? ["start", "--keep"] : ["release"])
    }

    /// Records 2 s from the selected mic and writes a one-line report (for development: checks that
    /// the recorder delivers audio and writes a valid WAV). Launch with `open` so the app's own
    /// microphone permission applies.
    private func runRecorderSelfTest(report: URL, deviceUID: String?) {
        let recorder = Recorder()
        let wav = FileManager.default.temporaryDirectory.appendingPathComponent("voice-to-text/selftest-rec.wav")
        let started = Date()
        var peak: Float = 0
        var readyAfter: TimeInterval = -1
        recorder.onLevel = { peak = max(peak, $0) }
        func write(_ line: String) {
            try? (line + "\n").write(to: report, atomically: true, encoding: .utf8)
            NSApp.terminate(nil)
        }
        // Confirms Objective-C exceptions are converted into Swift errors instead of crashing.
        var catcher = "not-caught"
        do {
            try VTTObjC.catchException { _ = NSArray().object(at: 1) }
        } catch {
            catcher = "ok"
        }
        recorder.start(to: wav, deviceUID: deviceUID, onReady: {
            readyAfter = Date().timeIntervalSince(started)
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                guard let url = recorder.stop() else { return write("FAIL stop returned nil") }
                let bytes = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int) ?? 0
                write(String(format: "OK device=%@ ready=%.2fs peak=%.2f wavBytes=%d exceptionCatcher=%@",
                             recorder.deviceName, readyAfter, peak, bytes, catcher))
            }
        }, onFailure: { error in
            write("FAIL \(error.localizedDescription)")
        })
    }

    /// Cycles the overlay through every state with synthetic levels (for development and screenshots).
    private func runOverlayDemo() {
        overlay.isEnabled = true
        overlay.showRecording()
        menuBarIcon.setActive(true)
        var tick = 0.0
        let levels = Timer.scheduledTimer(withTimeInterval: 1.0 / 45, repeats: true) { [weak self] _ in
            tick += 1
            let speech = abs(sin(tick / 9)) * (0.55 + 0.45 * abs(sin(tick / 2.3)))
            self?.overlay.push(level: Float(speech))
        }
        let steps: [(TimeInterval, () -> Void)] = [
            (4.0, { levels.invalidate(); self.overlay.show(.transcribing) }),
            (6.0, { self.overlay.show(.polishing(offline: false)) }),
            (8.5, { self.overlay.finish(.success("Pasted"), after: 1.5); self.menuBarIcon.setActive(false) }),
            (10.5, { self.overlay.finish(.message("No speech detected", isError: false), after: 1.5) }),
            (12.5, { self.overlay.finish(.message("Transcription failed", isError: true), after: 1.5) }),
            (14.5, { NSApp.terminate(nil) }),
        ]
        for (delay, action) in steps {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: action)
        }
    }

    // MARK: - Dictation flow

    private func stateChanged(_ state: Dictation.State) {
        updateIcon(state)
        let wantsEscape = state == .starting || state == .recording
        if wantsEscape != (escapeKey != nil) {
            escapeKey = nil // unregister before registering again: the same combo can't be held twice
            if wantsEscape { escapeKey = HotKey(HotKey.escape) { [weak self] in self?.dictation.cancel() } }
        }

        connectingWork?.cancel()
        switch state {
        case .starting:
            // Built-in mics start in ~50 ms; only show "Connecting" for slow (Bluetooth) ones.
            let work = DispatchWorkItem { [weak self] in
                guard let self, self.dictation.state == .starting else { return }
                self.overlay.show(.connecting(self.dictation.deviceName))
            }
            connectingWork = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: work)
        case .recording:
            play("Tink")
            overlay.showRecording()
        case .transcribing:
            play("Pop")
            overlay.show(.transcribing)
        case .polishing:
            overlay.show(.polishing(offline: settings.usesS1))
        case .idle, .testingMic:
            break
        }
    }

    private func finished(_ outcome: Dictation.Outcome) {
        switch outcome {
        case .text(let result):
            deliver(result)
        case .noSpeech:
            AppLog.write("RESULT no-speech")
            play("Funk")
            overlay.finish(.message("No speech detected", isError: false), after: 1.6)
        case .cancelled where showHoldHint:
            showHoldHint = false
            AppLog.write("RESULT cancelled (hold-to-talk tap)")
            overlay.finish(.message("Hold \(hotKeyLabel) while you speak", isError: false), after: 1.4)
        case .cancelled:
            AppLog.write("RESULT cancelled")
            play("Funk")
            overlay.finish(.message("Cancelled", isError: false), after: 0.8)
        case .refused(let message):
            AppLog.write("RESULT refused: \(message)")
            play("Funk")
            overlay.finish(.message(message, isError: false), after: 1.8)
        case .failed(let message):
            AppLog.write("RESULT failed: \(message)")
            play("Basso")
            overlay.finish(.message(message, isError: true), after: 2.5)
        }
    }

    /// Pastes a finished dictation, unless focus moved to another app or window (then it's copied) or a password field
    /// has focus (then it's only kept for Copy Last). Says what happened, including why cleanup didn't run.
    private func deliver(_ result: Dictation.Result) {
        let text = result.text
        let details = result.details
        lastResult = text
        let html = richHTML(for: text, mode: result.context.mode)
        let check: PasteTarget.Check = settings.autoPaste ? result.target.check() : .same
        let problem = Dictation.problemDescription(details)
        let guarded = details.status == "guard-raw"
        var pasted = false
        let label: String
        switch check {
        case .secure:
            label = "Not pasted: a password field has focus (see Copy Last)"
        case .changed(let whereTo):
            Paster.copy(text, html: html)
            label = "Copied: you switched to \(whereTo). Press ⌘V"
        case .same:
            pasted = Paster.paste(text, html: html, autoPaste: settings.autoPaste)
            let modeSuffix = result.context.mode == .default ? "" : " · \(result.context.mode.title)"
            if !settings.autoPaste {
                label = "Copied" + modeSuffix
            } else if !pasted {
                label = "Copied. Press ⌘V (allow Accessibility to auto-paste)"
            } else if guarded {
                label = "Pasted Whisper's text: the cleanup dropped \(details.guardReason)"
            } else if result.offlineFallback {
                label = "Pasted · cleaned offline" + (problem.map { " · \($0)" } ?? "")
            } else if result.cleanupFailed {
                label = "Pasted without cleanup" + (problem.map { " · \($0)" } ?? "")
            } else {
                label = "Pasted" + modeSuffix
            }
        }
        let cleaned: String? = guarded ? (details.rejected.isEmpty ? nil : details.rejected)
            : result.cleanupFailed || details.engine == "none" || details.engine.isEmpty ? nil : text
        history.add(.init(date: Date(), appName: result.context.appName, mode: result.context.mode,
                          raw: details.raw.isEmpty ? result.raw : details.raw, cleaned: cleaned,
                          guardReason: guarded ? details.guardReason : nil, target: result.target,
                          wasPasted: pasted, showingRaw: guarded || cleaned == nil))

        let outcome = pasted ? "pasted" : check == .secure ? "withheld-secure" : "copied"
        AppLog.write("RESULT \(outcome) mode=\(result.context.mode.rawValue) app=\"\(result.context.appName)\" "
            + "chars=\(text.count) rich=\(html != nil) status=\(details.status.isEmpty ? "-" : details.status) "
            + "engine=\(details.engine.isEmpty ? "-" : details.engine)"
            + (details.error.isEmpty ? "" : " error=\(details.error)")
            + (guarded ? " guard=\"\(settings.logText ? details.guardReason : details.guardReason.components(separatedBy: " (")[0])\"" : "")
            + " target=\(checkLabel(check)) cleanupFailed=\(result.cleanupFailed) offlineFallback=\(result.offlineFallback)")
        let timing = result.timing
        let total = Int(Date().timeIntervalSince(timing.stoppedAt) * 1000)
        lastResultModel.summary = "It worked: \(String(format: "%.1f", Double(total) / 1000)) s after you stopped, "
            + "cleaned up with \(Dictation.engineName(details.engine, settings: settings))."
        AppLog.write("TIMING stop→transcript=\(timing.transcribeMs)ms transcript→cleaned=\(timing.cleanupMs)ms "
            + "cleaned→pasted=\(total - timing.transcribeMs - timing.cleanupMs)ms total=\(total)ms "
            + "prestarted=\(timing.prestarted) mode=\(result.context.mode.rawValue)")
        if check == .secure { play("Funk") }
        let notice = check != .same || guarded || problem != nil
        overlay.finish(check == .same && pasted && !notice ? .success(label) : .message(label, isError: false),
                       after: pasted && !notice ? 0.9 : 3.0)
    }

    private func checkLabel(_ check: PasteTarget.Check) -> String {
        switch check {
        case .same: "same"
        case .changed: "changed"
        case .secure: "secure"
        }
    }

    /// Rich text only helps where lists render; terminals and editors get plain text.
    private func richHTML(for text: String, mode: DictationMode) -> String? {
        settings.richPaste && mode != .code && RichText.containsList(text) ? RichText.html(from: text) : nil
    }

    /// ⌃⌥Z: puts the other version of the last dictation in its place, Whisper's text for the cleanup or back. Undoes the
    /// paste (⌘Z) and pastes the other version, if the same app and window are still in front, the paste is still right
    /// before the cursor (checked through Accessibility; where the app doesn't say, only within 30 s), and it's under 2
    /// minutes old. Terminals and editors, where ⌘Z doesn't take a paste back, and every other case get the other version
    /// on the clipboard instead.
    @objc private func swapLastPaste() {
        guard dictation.state == .idle else { return }
        guard let last = history.last, let alternative = last.alternative else {
            overlay.finish(.message(history.last == nil ? "No dictation to swap yet"
                : "Nothing to swap: there's no other version", isError: false), after: 1.8)
            return
        }
        let name = last.showingRaw ? "the cleaned text" : "Whisper's text"
        let age = Date().timeIntervalSince(last.date)
        func squeezed(_ text: String) -> String {
            text.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.joined(separator: " ")
        }
        let stillThere: Bool = {
            let shown = squeezed(last.shown)
            guard let before = PasteTarget.textBeforeCursor(count: last.shown.count + 40) else { return age < 30 }
            return squeezed(before).hasSuffix(String(shown.suffix(200)))
        }()
        guard last.wasPasted, age < 120, settings.autoPaste, AXIsProcessTrusted(), last.mode != .code,
              last.target.check() == .same, stillThere else {
            Paster.copy(alternative)
            history.swappedLast(pasted: false)
            overlay.finish(.message("Copied \(name). Press ⌘V", isError: false), after: 2.5)
            return
        }
        let html = richHTML(for: alternative, mode: last.mode)
        Paster.whenModifiersReleased {
            Paster.sendShortcut("z", fallback: CGKeyCode(kVK_ANSI_Z))
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                Paster.paste(alternative, html: html, autoPaste: true)
            }
        }
        history.swappedLast(pasted: true)
        lastResult = alternative
        AppLog.write("SWAP to \(last.showingRaw ? "cleaned" : "raw") app=\"\(last.appName)\"")
        overlay.finish(.success("Replaced with \(name) · \(settings.swapHotKey.label) swaps back"), after: 1.8)
    }

    private func play(_ sound: String) {
        guard settings.playSounds else { return }
        NSSound(named: NSSound.Name(sound))?.play()
    }

    private func updateIcon(_ state: Dictation.State) {
        menuBarIcon.setActive(state != .idle)
    }

    // MARK: - Hotkey

    private func registerHotKey() {
        hotKey = nil
        swapKey = nil
        guard !settings.isRecordingHotKey else { return }
        hotKey = HotKey(settings.hotKey, onRelease: { [weak self] in self?.hotKeyReleased() }) { [weak self] in
            self?.hotKeyPressed()
        }
        settings.hotKeyError = hotKey == nil ? "\(hotKeyLabel) is used by another app. Pick another." : nil
        if hotKey == nil {
            notify("\(hotKeyLabel) is used by another app. Pick a different hotkey in Settings.")
        }
        if settings.swapHotKey != settings.hotKey {
            swapKey = HotKey(settings.swapHotKey) { [weak self] in self?.swapLastPaste() }
        }
        settings.swapHotKeyError = swapKey == nil ? "\(settings.swapHotKey.label) is taken. Pick another." : nil
    }

    /// No speech model yet (a new Mac before setup finished): open setup instead of failing to transcribe.
    private func speechModelMissing() -> Bool {
        guard dictation.state == .idle,
              !(ModelCatalog.whisperModel(named: settings.whisperModel)?.isInstalled ?? false) else { return false }
        overlay.finish(.message("Download a speech model first", isError: false), after: 2.0)
        setupWindow.show()
        return true
    }

    private func hotKeyPressed() {
        guard !speechModelMissing() else { return }
        guard settings.holdToTalk else { return dictation.toggle() }
        guard dictation.state == .idle else { return }
        holdStartedAt = Date()
        dictation.start()
    }

    /// Hold to talk: releasing the hotkey stops and pastes. A press under 0.3 s is treated as an accidental tap.
    private func hotKeyReleased() {
        guard settings.holdToTalk, let started = holdStartedAt else { return }
        holdStartedAt = nil
        if Date().timeIntervalSince(started) < 0.3 {
            showHoldHint = true
            dictation.cancel()
        } else if dictation.state == .recording {
            dictation.stop()
        } else {
            dictation.cancel() // released before the mic was ready: nothing useful was recorded
        }
    }

    // MARK: - Menu

    /// A short menu for quick switches; everything else is in the Settings window.
    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let hold = settings.holdToTalk

        let status: String = switch dictation.state {
        case .idle: "Ready. \(hold ? "Hold" : "Press") \(hotKeyLabel) to dictate"
        case .starting: "Starting \(dictation.deviceName)…"
        case .recording: "Recording… \(hold ? "Release" : "Press") \(hotKeyLabel) to finish, Esc to cancel"
        case .testingMic: "Testing \(dictation.deviceName)…"
        case .transcribing: "Transcribing…"
        case .polishing: settings.usesS1 ? "Polishing with S1-mini…" : settings.usesAPI ? "Polishing with the API…"
            : "Polishing with Claude…"
        }
        menu.addItem(disabled(status))
        if let release = updater.available {
            menu.addItem(item("Update Available (\(release.version))…", #selector(openUpdate)))
        }
        if !settings.setupCompleted || AppLocation.needsMove {
            menu.addItem(item("Finish Setting Up…", #selector(openSetup)))
        }
        menu.addItem(.separator())

        switch dictation.state {
        case .idle: menu.addItem(item("Start Dictation", #selector(toggleDictation)))
        case .starting: menu.addItem(item("Cancel", #selector(cancelDictation)))
        case .testingMic: menu.addItem(disabled("Testing microphone…"))
        case .recording:
            menu.addItem(item("Stop and Paste", #selector(toggleDictation)))
            menu.addItem(item("Cancel Recording", #selector(cancelDictation)))
        case .transcribing, .polishing: menu.addItem(disabled("Working…"))
        }
        if let lastResult {
            menu.addItem(item("Copy Last: \(Self.preview(lastResult, 50))", #selector(copyLast)))
        }
        if let last = history.last, last.alternative != nil {
            let swap = item(last.showingRaw ? "Swap Last Paste to the Cleaned Text" : "Swap Last Paste to Whisper's Text",
                            #selector(swapLastPaste))
            swap.toolTip = "Undoes the last paste and pastes the other version (\(settings.swapHotKey.label))"
            menu.addItem(swap)
        }
        if !history.entries.isEmpty { menu.addItem(historyMenuItem()) }

        menu.addItem(.separator())
        menu.addItem(modeMenuItem())
        menu.addItem(cleanupMenuItem())
        menu.addItem(microphoneMenuItem())

        menu.addItem(.separator())
        menu.addItem(item("Settings…", #selector(openSettingsWindow), key: ","))
        menu.addItem(item("Set Up…", #selector(openSetup)))
        menu.addItem(.separator())
        menu.addItem(item("Quit OpenVoiceType", #selector(quit), key: "q"))
    }

    private func cleanupMenuItem() -> NSMenuItem {
        refreshS1Status() // for the next time the menu opens
        let installed = s1Installed ?? ModelCatalog.s1Mini.isInstalled
        let menu = NSMenu()
        menu.addItem(item("Off (paste Whisper's text)", #selector(selectCleanupOff), checked: !settings.refine))
        menu.addItem(.separator())
        for (title, model) in [("Claude Haiku (fastest)", "haiku"), ("Claude Sonnet (smarter)", "sonnet")] {
            let entry = item(title, #selector(selectModel(_:)),
                             checked: settings.refine && settings.cleanupEngine == "claude" && settings.claudeModel == model)
            entry.representedObject = model
            menu.addItem(entry)
        }
        let s1Item = item(installed ? "S1-mini (offline)" : "S1-mini (download in Settings)", #selector(selectS1),
                          checked: settings.usesS1)
        s1Item.isEnabled = installed
        menu.addItem(s1Item)
        let apiModel = settings.openaiModel.trimmingCharacters(in: .whitespaces)
        let apiItem = item(apiModel.isEmpty ? "API (set up in Settings)" : "API: \(apiModel)", #selector(selectAPI),
                           checked: settings.usesAPI)
        apiItem.isEnabled = !apiModel.isEmpty
        menu.addItem(apiItem)
        menu.addItem(.separator())
        menu.addItem(item("Cleanup Settings…", #selector(openCleanupSettings)))
        menu.autoenablesItems = false

        let current = !settings.refine ? "Off" : settings.cleanupEngine == "s1" ? "S1-mini"
            : settings.cleanupEngine == "openai" ? "API" : "Claude \(settings.claudeModel.capitalized)"
        return submenu("Clean Up With: \(current)", menu)
    }

    /// Recent Dictations: the last 10 (in memory only), each with Copy Cleaned Text and Copy Whisper's Text.
    private func historyMenuItem() -> NSMenuItem {
        let menu = NSMenu()
        let time = DateFormatter()
        time.timeStyle = .short
        for entry in history.entries {
            let sub = NSMenu()
            if let cleaned = entry.cleaned {
                let copy = item(entry.guardReason == nil ? "Copy Cleaned Text" : "Copy Cleaned Text (turned down: it dropped \(entry.guardReason!))",
                                #selector(copyText(_:)))
                copy.representedObject = cleaned
                sub.addItem(copy)
            }
            let raw = item("Copy Whisper's Text", #selector(copyText(_:)))
            raw.representedObject = entry.raw
            sub.addItem(raw)
            let shown = entry.showingRaw ? entry.raw : entry.cleaned ?? entry.raw
            let app = entry.appName.isEmpty ? "" : " · \(entry.appName)"
            menu.addItem(submenu("\(time.string(from: entry.date))\(app): \(Self.preview(shown, 40))", sub))
        }
        menu.addItem(.separator())
        menu.addItem(disabled("Kept in memory only, never saved to disk"))
        return submenu("Recent Dictations", menu)
    }

    private static func preview(_ text: String, _ length: Int) -> String {
        let line = text.replacingOccurrences(of: "\n", with: " ")
        return line.count > length ? String(line.prefix(length)) + "…" : line
    }

    private func modeMenuItem() -> NSMenuItem {
        let menu = NSMenu()
        menu.addItem(item("Auto (by app)", #selector(selectMode(_:)), checked: settings.modeOverride == nil))
        menu.addItem(.separator())
        for mode in DictationMode.allCases {
            let entry = item(mode.title, #selector(selectMode(_:)), checked: settings.modeOverride == mode)
            entry.representedObject = mode.rawValue
            menu.addItem(entry)
        }
        return submenu("Mode: \(settings.modeOverride?.title ?? "Auto")", menu)
    }

    private func microphoneMenuItem() -> NSMenuItem {
        let devices = AudioDevices.inputDevices()
        let systemDefault = AudioDevices.defaultInput()
        let selected = settings.inputDeviceUID.flatMap { uid in devices.first { $0.uid == uid } }
        let menu = NSMenu()

        let defaultTitle = "System Default" + (systemDefault.map { " (\($0.name))" } ?? "")
        menu.addItem(item(defaultTitle, #selector(selectMicrophone(_:)), checked: settings.inputDeviceUID == nil))
        menu.addItem(.separator())
        for device in devices {
            let title = device.name + (device.isBluetooth ? " (Bluetooth)" : "")
            let entry = item(title, #selector(selectMicrophone(_:)), checked: device.uid == settings.inputDeviceUID)
            entry.representedObject = device.uid
            menu.addItem(entry)
        }
        if let uid = settings.inputDeviceUID, selected == nil {
            // The chosen mic isn't connected; recording falls back to the system default.
            let missing = disabled("\(settings.inputDeviceName ?? uid) (not connected)")
            missing.state = .on
            menu.addItem(missing)
        }

        menu.addItem(.separator())
        let testItem = item("Test Microphone…", #selector(testMicrophone))
        testItem.isEnabled = dictation.state == .idle
        menu.addItem(testItem)

        let active = selected ?? systemDefault
        if active?.isBluetooth == true {
            menu.addItem(.separator())
            menu.addItem(disabled("Tip: Bluetooth mics switch earbuds to low-quality"))
            menu.addItem(disabled("call mode. The built-in mic often transcribes better."))
        }

        menu.autoenablesItems = false
        return submenu("Microphone: \(active?.name ?? "None")", menu)
    }

    private func item(_ title: String, _ action: Selector, checked: Bool = false, key: String = "") -> NSMenuItem {
        let entry = NSMenuItem(title: title, action: action, keyEquivalent: key)
        entry.target = self
        entry.state = checked ? .on : .off
        return entry
    }

    private func disabled(_ title: String) -> NSMenuItem {
        let entry = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        entry.isEnabled = false
        return entry
    }

    private func submenu(_ title: String, _ menu: NSMenu) -> NSMenuItem {
        let entry = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        entry.submenu = menu
        return entry
    }

    // MARK: - Actions

    @objc private func toggleDictation() {
        guard !speechModelMissing() else { return }
        dictation.toggle()
    }
    @objc private func cancelDictation() { dictation.cancel() }
    @objc private func openSettingsWindow() { settingsWindow.show() }
    @objc private func openCleanupSettings() { settingsWindow.show(.cleanup) }
    @objc private func openSetup() { setupWindow.show() }
    @objc private func openUpdate() { updater.openReleasePage() }

    @objc private func selectCleanupOff() { settings.refine = false }

    @objc private func selectModel(_ sender: NSMenuItem) {
        guard let model = sender.representedObject as? String else { return }
        settings.claudeModel = model
        settings.cleanupEngine = "claude"
        settings.refine = true
    }

    @objc private func selectS1() {
        settings.cleanupEngine = "s1"
        settings.refine = true
    }

    @objc private func selectAPI() {
        settings.cleanupEngine = "openai"
        settings.refine = true
    }

    @objc private func selectMode(_ sender: NSMenuItem) {
        settings.modeOverride = (sender.representedObject as? String).flatMap(DictationMode.init(rawValue:))
    }

    @objc private func selectMicrophone(_ sender: NSMenuItem) {
        let uid = sender.representedObject as? String
        settings.inputDeviceUID = uid
        settings.inputDeviceName = uid.flatMap(AudioDevices.device(uid:))?.name
    }

    @objc private func testMicrophone() {
        guard dictation.state == .idle else { return }
        overlay.isEnabled = true // always show the test, even if the overlay is turned off
        overlay.show(.connecting(AudioDevices.device(uid: settings.inputDeviceUID ?? "")?.name
            ?? AudioDevices.defaultInput()?.name ?? "microphone"))
        dictation.testMicrophone(duration: 4, onReady: { [weak self] in
            guard let self else { return }
            self.overlay.showMicTest(deviceName: self.dictation.deviceName)
        }, completion: { [weak self] result in
            guard let self else { return }
            let name = self.dictation.deviceName
            switch result {
            case .success(let peak) where peak >= 0.45:
                self.overlay.finish(.success("\(name) works"), after: 2.0)
            case .success(let peak) where peak >= 0.15:
                self.overlay.finish(.message("\(name) is very quiet. Speak up or move closer", isError: false), after: 3.0)
            case .success:
                self.play("Basso")
                self.overlay.finish(.message("No sound from \(name). Check mic or pick another", isError: true), after: 3.5)
            case .failure(let error):
                self.play("Basso")
                self.overlay.finish(.message(error.localizedDescription, isError: true), after: 3.5)
            }
            self.overlay.isEnabled = self.settings.showOverlay
        })
    }

    @objc private func copyLast() {
        guard let lastResult else { return }
        Paster.copy(lastResult)
    }

    @objc private func copyText(_ sender: NSMenuItem) {
        guard let text = sender.representedObject as? String else { return }
        Paster.copy(text)
    }

    @objc private func quit() {
        dictation.cancel()
        NSApp.terminate(nil)
    }

    // MARK: - Permissions & helpers

    private func requestMicrophoneIfNeeded() {
        guard AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined else { return }
        AVCaptureDevice.requestAccess(for: .audio) { _ in }
    }

    private func notify(_ message: String) {
        let escaped = message.replacingOccurrences(of: "\"", with: "\\\"")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", "display notification \"\(escaped)\" with title \"OpenVoiceType\""]
        try? process.run()
    }

    private func fatalAlert(_ message: String) {
        let alert = NSAlert()
        alert.messageText = "OpenVoiceType"
        alert.informativeText = message
        alert.runModal()
        NSApp.terminate(nil)
    }
}
