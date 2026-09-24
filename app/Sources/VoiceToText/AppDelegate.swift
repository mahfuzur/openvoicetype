import AppKit
import AVFoundation
import ObjCSupport
import ServiceManagement

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var statusItem: NSStatusItem!
    private var dictation: Dictation!
    private var hotKey: HotKey?
    private var escapeKey: HotKey?
    private var lastResult: String?
    private let overlay = OverlayController()
    private var connectingWork: DispatchWorkItem?

    private let defaults = UserDefaults.standard
    private let configURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/voice-to-text/config.sh")
    private let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/voice-to-text/dictate.log")
    private let dictionaryURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/voice-to-text/dictionary.txt")

    private var hotKeyIndex: Int {
        get { min(defaults.integer(forKey: "hotKeyIndex"), HotKey.presets.count - 1) }
        set { defaults.set(newValue, forKey: "hotKeyIndex") }
    }
    private var refine: Bool {
        get { defaults.object(forKey: "refine") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "refine"); dictation.refine = newValue }
    }
    private var autoPaste: Bool {
        get { defaults.object(forKey: "autoPaste") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "autoPaste") }
    }
    private var claudeModel: String {
        get { defaults.string(forKey: "claudeModel") ?? "haiku" }
        set { defaults.set(newValue, forKey: "claudeModel"); dictation.claudeModel = newValue }
    }
    /// "claude" (default) or "s1" (S1-mini, fully offline).
    private var cleanupEngine: String {
        get { defaults.string(forKey: "cleanupEngine") ?? "claude" }
        set { defaults.set(newValue, forKey: "cleanupEngine"); dictation.cleanupEngine = newValue }
    }
    private var s1Fallback: Bool {
        get { defaults.object(forKey: "s1Fallback") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "s1Fallback"); dictation.s1Fallback = newValue }
    }
    /// From `dictate.sh s1-server status`; nil until the first check finishes.
    private var s1Installed: Bool?
    private var usesS1: Bool { refine && cleanupEngine == "s1" }
    private var showOverlay: Bool {
        get { defaults.object(forKey: "showOverlay") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "showOverlay"); overlay.isEnabled = newValue }
    }
    private var overlayPosition: OverlayController.Position {
        get { OverlayController.Position(rawValue: defaults.string(forKey: "overlayPosition") ?? "") ?? .bottom }
        set { defaults.set(newValue.rawValue, forKey: "overlayPosition"); overlay.position = newValue }
    }
    private var playSounds: Bool {
        get { defaults.object(forKey: "playSounds") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "playSounds") }
    }
    /// nil = Auto (choose from the frontmost app).
    private var modeOverride: DictationMode? {
        get { defaults.string(forKey: "modeOverride").flatMap(DictationMode.init(rawValue:)) }
        set { defaults.set(newValue?.rawValue, forKey: "modeOverride") }
    }
    private var richPaste: Bool {
        get { defaults.object(forKey: "richPaste") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "richPaste") }
    }
    /// nil = system default input.
    private var inputDeviceUID: String? {
        get { defaults.string(forKey: "inputDeviceUID") }
        set { defaults.set(newValue, forKey: "inputDeviceUID"); dictation.inputDeviceUID = newValue }
    }
    private var hotKeyLabel: String { HotKey.presets[hotKeyIndex].label }
    /// Hold to talk: hold the hotkey while speaking and release it to finish. Off = press to start and to stop.
    private var holdToTalk: Bool {
        get { defaults.bool(forKey: "holdToTalk") }
        set { defaults.set(newValue, forKey: "holdToTalk") }
    }
    /// When the hotkey went down in hold-to-talk mode.
    private var holdStartedAt: Date?
    /// Set when a hold-to-talk press was too short, so the overlay explains instead of saying "Cancelled".
    private var showHoldHint = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard let script = Bundle.main.url(forResource: "dictate", withExtension: "sh") else {
            fatalAlert("dictate.sh is missing from the app bundle. Rebuild with scripts/build-app.sh.")
            return
        }
        dictation = Dictation(scriptURL: script)
        dictation.refine = refine
        dictation.claudeModel = claudeModel
        dictation.cleanupEngine = cleanupEngine
        dictation.s1Fallback = s1Fallback
        dictation.inputDeviceUID = inputDeviceUID
        dictation.onStateChange = { [weak self] state in self?.stateChanged(state) }
        dictation.onLevel = { [weak self] level in self?.overlay.push(level: level) }
        dictation.onFinish = { [weak self] outcome in self?.finished(outcome) }
        dictation.contextProvider = { [weak self] in DictationContext.current(override: self?.modeOverride) }
        overlay.isEnabled = showOverlay
        overlay.position = overlayPosition

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        let menu = NSMenu()
        menu.delegate = self
        statusItem.menu = menu
        updateIcon(.idle)

        if let index = CommandLine.arguments.firstIndex(of: "--overlay-snapshots"),
           index + 1 < CommandLine.arguments.count {
            let directory = URL(fileURLWithPath: CommandLine.arguments[index + 1])
            Task { @MainActor in
                OverlaySnapshots.render(to: directory)
                NSApp.terminate(nil)
            }
            return
        }
        if let index = CommandLine.arguments.firstIndex(of: "--recorder-selftest"),
           index + 1 < CommandLine.arguments.count {
            let pinDefault = CommandLine.arguments.contains("--pin-default")
            runRecorderSelfTest(report: URL(fileURLWithPath: CommandLine.arguments[index + 1]),
                                deviceUID: pinDefault ? AudioDevices.defaultInput()?.uid : inputDeviceUID)
            return
        }
        if CommandLine.arguments.contains("--overlay-demo") {
            runOverlayDemo()
            return
        }
        registerHotKey()
        requestMicrophoneIfNeeded()
        if !AXIsProcessTrusted() { promptAccessibility() }
        refreshS1Status()
        updateS1Server()
    }

    func applicationWillTerminate(_ notification: Notification) {
        dictation?.shutDown()
    }

    // MARK: - S1-mini server

    private func refreshS1Status() {
        dictation.server("s1-server", ["status"]) { [weak self] _, output in
            self?.s1Installed = output.trimmingCharacters(in: .whitespacesAndNewlines) != "missing"
        }
    }

    /// Keeps S1-mini loaded while it's the selected cleanup; otherwise lets it stop after idling
    /// (a Claude fallback starts it on demand).
    private func updateS1Server() {
        dictation.server("s1-server", usesS1 ? ["start", "--keep"] : ["release"])
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
        var tick = 0.0
        let levels = Timer.scheduledTimer(withTimeInterval: 1.0 / 45, repeats: true) { [weak self] _ in
            tick += 1
            let speech = abs(sin(tick / 9)) * (0.55 + 0.45 * abs(sin(tick / 2.3)))
            self?.overlay.push(level: Float(speech))
        }
        let steps: [(TimeInterval, () -> Void)] = [
            (4.0, { levels.invalidate(); self.overlay.show(.transcribing) }),
            (6.0, { self.overlay.show(.polishing(offline: false)) }),
            (8.5, { self.overlay.finish(.success("Pasted"), after: 1.5) }),
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
            overlay.show(.polishing(offline: usesS1))
        case .idle, .testingMic:
            break
        }
    }

    private func finished(_ outcome: Dictation.Outcome) {
        switch outcome {
        case .text(let text, let cleanupFailed, let offlineFallback, let context, let timing):
            lastResult = text
            // Rich text only helps where lists render; terminals and editors get plain text.
            let html = richPaste && context.mode != .code && RichText.containsList(text)
                ? RichText.html(from: text) : nil
            let pasted = Paster.paste(text, html: html, autoPaste: autoPaste)
            let modeSuffix = context.mode == .default ? "" : " · \(context.mode.title)"
            let label: String
            if !autoPaste {
                label = "Copied" + modeSuffix
            } else if !pasted {
                label = "Copied. Press ⌘V (allow Accessibility to auto-paste)"
            } else {
                label = (cleanupFailed ? "Pasted without cleanup" : offlineFallback ? "Pasted · cleaned offline" : "Pasted")
                    + modeSuffix
            }
            AppLog.write("RESULT \(pasted ? "pasted" : "copied") mode=\(context.mode.rawValue) app=\"\(context.appName)\" "
                + "chars=\(text.count) rich=\(html != nil) cleanupFailed=\(cleanupFailed) offlineFallback=\(offlineFallback)")
            let total = Int(Date().timeIntervalSince(timing.stoppedAt) * 1000)
            AppLog.write("TIMING stop→transcript=\(timing.transcribeMs)ms transcript→cleaned=\(timing.cleanupMs)ms "
                + "cleaned→pasted=\(total - timing.transcribeMs - timing.cleanupMs)ms total=\(total)ms "
                + "prestarted=\(timing.prestarted) mode=\(context.mode.rawValue)")
            overlay.finish(.success(label), after: pasted ? 0.9 : 2.5)
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
        case .failed(let message):
            AppLog.write("RESULT failed: \(message)")
            play("Basso")
            overlay.finish(.message(message, isError: true), after: 2.5)
        }
    }

    private func play(_ sound: String) {
        guard playSounds else { return }
        NSSound(named: NSSound.Name(sound))?.play()
    }

    private func updateIcon(_ state: Dictation.State) {
        guard let button = statusItem.button else { return }
        let (symbol, tint): (String, NSColor?) = switch state {
        case .idle: ("mic", nil)
        case .starting, .recording: ("mic.fill", .systemRed)
        case .transcribing, .polishing: ("waveform", .systemOrange)
        case .testingMic: ("mic.fill", .systemBlue)
        }
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "Voice to Text")
        image?.isTemplate = true
        button.image = image
        button.contentTintColor = tint
    }

    // MARK: - Hotkey

    private func registerHotKey() {
        hotKey = nil
        hotKey = HotKey(HotKey.presets[hotKeyIndex], onRelease: { [weak self] in self?.hotKeyReleased() }) { [weak self] in
            self?.hotKeyPressed()
        }
        if hotKey == nil {
            notify("\(hotKeyLabel) is used by another app. Pick a different hotkey from the menu.")
        }
    }

    private func hotKeyPressed() {
        guard holdToTalk else { return dictation.toggle() }
        guard dictation.state == .idle else { return }
        holdStartedAt = Date()
        dictation.start()
    }

    /// Hold to talk: releasing the hotkey stops and pastes. A press under 0.3 s is treated as an accidental tap.
    private func hotKeyReleased() {
        guard holdToTalk, let started = holdStartedAt else { return }
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

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()

        let status: String = switch dictation.state {
        case .idle: "Ready. \(holdToTalk ? "Hold" : "Press") \(hotKeyLabel) to dictate"
        case .starting: "Starting \(dictation.deviceName)…"
        case .recording: "Recording… \(holdToTalk ? "Release" : "Press") \(hotKeyLabel) to finish, Esc to cancel"
        case .testingMic: "Testing \(dictation.deviceName)…"
        case .transcribing: "Transcribing…"
        case .polishing: usesS1 ? "Polishing with S1-mini…" : "Polishing with Claude…"
        }
        menu.addItem(disabled(status))
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
            let preview = lastResult.count > 50 ? String(lastResult.prefix(50)) + "…" : lastResult
            menu.addItem(item("Copy Last: \(preview)", #selector(copyLast)))
        }

        menu.addItem(.separator())
        menu.addItem(item("Clean Up Text", #selector(toggleRefine), checked: refine))
        menu.addItem(cleanupModelMenuItem())
        menu.addItem(item("Paste Automatically", #selector(toggleAutoPaste), checked: autoPaste))
        menu.addItem(item("Paste Lists as Rich Text", #selector(toggleRichPaste), checked: richPaste))
        menu.addItem(modeMenuItem())

        let hotKeyMenu = NSMenu()
        for (index, combo) in HotKey.presets.enumerated() {
            let entry = item(combo.label, #selector(selectHotKey(_:)), checked: index == hotKeyIndex)
            entry.tag = index
            hotKeyMenu.addItem(entry)
        }
        hotKeyMenu.addItem(.separator())
        hotKeyMenu.addItem(item("Press to Start and Stop", #selector(selectToggleMode), checked: !holdToTalk))
        hotKeyMenu.addItem(item("Hold to Talk", #selector(selectHoldToTalk), checked: holdToTalk))
        menu.addItem(submenu("Hotkey: \(hotKeyLabel)\(holdToTalk ? " (hold)" : "")", hotKeyMenu))
        menu.addItem(microphoneMenuItem())

        menu.addItem(.separator())
        menu.addItem(item("Show Overlay", #selector(toggleOverlay), checked: showOverlay))
        let positionMenu = NSMenu()
        for (title, position) in [("Bottom", OverlayController.Position.bottom), ("Top", .top)] {
            let entry = item(title, #selector(selectOverlayPosition(_:)), checked: overlayPosition == position)
            entry.representedObject = position.rawValue
            positionMenu.addItem(entry)
        }
        let positionItem = submenu("Overlay Position", positionMenu)
        menu.addItem(positionItem)
        menu.addItem(item("Play Sounds", #selector(toggleSounds), checked: playSounds))

        menu.addItem(.separator())
        let micOK = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        menu.addItem(item(micOK ? "Microphone: Allowed ✓" : "Microphone: Not Allowed. Click to Fix",
                          #selector(fixMicrophone)))
        let axOK = AXIsProcessTrusted()
        menu.addItem(item(axOK ? "Accessibility: Allowed ✓" : "Accessibility: Not Allowed. Click to Fix",
                          #selector(fixAccessibility)))

        menu.addItem(.separator())
        menu.addItem(item("Launch at Login", #selector(toggleLaunchAtLogin),
                          checked: SMAppService.mainApp.status == .enabled))
        menu.addItem(item("Edit Dictionary…", #selector(openDictionary)))
        menu.addItem(item("Edit Config…", #selector(openConfig)))
        menu.addItem(item("Open Log", #selector(openLog)))
        menu.addItem(.separator())
        menu.addItem(item("Quit Voice to Text", #selector(quit), key: "q"))
    }

    private func cleanupModelMenuItem() -> NSMenuItem {
        refreshS1Status() // for the next time the menu opens
        let installed = s1Installed ?? true
        let menu = NSMenu()
        let claudeModels = [("Claude Haiku (fastest)", "haiku"), ("Claude Sonnet (smarter)", "sonnet")]
        for (title, model) in claudeModels {
            let entry = item(title, #selector(selectModel(_:)), checked: cleanupEngine == "claude" && claudeModel == model)
            entry.representedObject = model
            menu.addItem(entry)
        }
        menu.addItem(.separator())
        let s1Item = item(installed ? "S1-mini (offline)" : "S1-mini (not installed)", #selector(selectS1),
                          checked: cleanupEngine == "s1")
        s1Item.isEnabled = installed
        menu.addItem(s1Item)
        let fallbackItem = item("Use S1-mini When Claude Is Unavailable", #selector(toggleS1Fallback),
                                checked: s1Fallback && installed)
        fallbackItem.isEnabled = installed && cleanupEngine == "claude"
        menu.addItem(fallbackItem)
        menu.addItem(.separator())
        if installed {
            menu.addItem(disabled("S1-mini by Superwhisper runs on this Mac:"))
            menu.addItem(disabled("no internet needed, English only."))
            menu.addItem(disabled("It skips Code mode (editors, terminals): raw text."))
        } else {
            menu.addItem(disabled("Run scripts/install.sh to install S1-mini"))
            menu.addItem(disabled("(offline cleanup, about 500 MB)."))
        }
        menu.autoenablesItems = false

        let current = cleanupEngine == "s1" ? "S1-mini"
            : claudeModels.first { $0.1 == claudeModel }.map { $0.0.components(separatedBy: " (")[0] } ?? "Claude"
        return submenu("Cleanup Model: \(current)", menu)
    }

    private func modeMenuItem() -> NSMenuItem {
        let menu = NSMenu()
        let autoTitle = "Auto (by app)"
        menu.addItem(item(autoTitle, #selector(selectMode(_:)), checked: modeOverride == nil))
        menu.addItem(.separator())
        for mode in DictationMode.allCases {
            let entry = item(mode.title, #selector(selectMode(_:)), checked: modeOverride == mode)
            entry.representedObject = mode.rawValue
            menu.addItem(entry)
        }
        menu.addItem(.separator())
        menu.addItem(disabled("Auto picks Chat for Slack/Teams, Email for Mail,"))
        menu.addItem(disabled("Code for editors and terminals, Notes for Notes/Notion."))
        return submenu("Mode: \(modeOverride?.title ?? "Auto")", menu)
    }

    private func microphoneMenuItem() -> NSMenuItem {
        let devices = AudioDevices.inputDevices()
        let systemDefault = AudioDevices.defaultInput()
        let selected = inputDeviceUID.flatMap { uid in devices.first { $0.uid == uid } }
        let menu = NSMenu()

        let defaultTitle = "System Default" + (systemDefault.map { " (\($0.name))" } ?? "")
        let defaultItem = item(defaultTitle, #selector(selectMicrophone(_:)), checked: inputDeviceUID == nil)
        menu.addItem(defaultItem)
        menu.addItem(.separator())
        for device in devices {
            let title = device.name + (device.isBluetooth ? " (Bluetooth)" : "")
            let entry = item(title, #selector(selectMicrophone(_:)), checked: device.uid == inputDeviceUID)
            entry.representedObject = device.uid
            menu.addItem(entry)
        }
        if let uid = inputDeviceUID, selected == nil {
            // The chosen mic isn't connected; recording falls back to the system default.
            let missing = disabled("\(defaults.string(forKey: "inputDeviceName") ?? uid) (not connected)")
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

    @objc private func toggleDictation() { dictation.toggle() }
    @objc private func cancelDictation() { dictation.cancel() }
    @objc private func toggleRefine() {
        refine.toggle()
        updateS1Server()
    }

    @objc private func selectS1() {
        cleanupEngine = "s1"
        updateS1Server()
    }

    @objc private func toggleS1Fallback() { s1Fallback.toggle() }
    @objc private func toggleAutoPaste() { autoPaste.toggle() }
    @objc private func toggleOverlay() { showOverlay.toggle() }
    @objc private func toggleRichPaste() { richPaste.toggle() }

    @objc private func selectMode(_ sender: NSMenuItem) {
        modeOverride = (sender.representedObject as? String).flatMap(DictationMode.init(rawValue:))
    }

    @objc private func openDictionary() {
        if !FileManager.default.fileExists(atPath: dictionaryURL.path) {
            try? FileManager.default.createDirectory(at: dictionaryURL.deletingLastPathComponent(),
                                                     withIntermediateDirectories: true)
            let template = """
                # Voice to Text dictionary
                #
                # One name or term per line: Whisper and Claude will spell it exactly like this.
                #   Claude Code
                #   PostgreSQL
                #
                # Replacements, applied after cleanup: heard => wanted
                #   cloud code => Claude Code
                #   super whisper => Superwhisper

                """
            try? template.write(to: dictionaryURL, atomically: true, encoding: .utf8)
        }
        NSWorkspace.shared.open([dictionaryURL], withApplicationAt: URL(fileURLWithPath: "/System/Applications/TextEdit.app"),
                                configuration: NSWorkspace.OpenConfiguration())
    }

    @objc private func selectMicrophone(_ sender: NSMenuItem) {
        let uid = sender.representedObject as? String
        inputDeviceUID = uid
        let name = uid.flatMap(AudioDevices.device(uid:))?.name
        defaults.set(name, forKey: "inputDeviceName")
    }

    @objc private func testMicrophone() {
        guard dictation.state == .idle else { return }
        overlay.isEnabled = true // always show the test, even if the overlay is turned off
        overlay.show(.connecting(AudioDevices.device(uid: inputDeviceUID ?? "")?.name
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
            self.overlay.isEnabled = self.showOverlay
        })
    }
    @objc private func toggleSounds() { playSounds.toggle() }

    @objc private func selectOverlayPosition(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let position = OverlayController.Position(rawValue: raw) {
            overlayPosition = position
            // Preview where it will appear.
            overlay.finish(.message("Overlay position", isError: false), after: 1.2)
        }
    }

    @objc private func copyLast() {
        guard let lastResult else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(lastResult, forType: .string)
    }

    @objc private func selectModel(_ sender: NSMenuItem) {
        guard let model = sender.representedObject as? String else { return }
        claudeModel = model
        cleanupEngine = "claude"
        updateS1Server()
    }

    @objc private func selectToggleMode() { holdToTalk = false }
    @objc private func selectHoldToTalk() { holdToTalk = true }

    @objc private func selectHotKey(_ sender: NSMenuItem) {
        hotKeyIndex = sender.tag
        registerHotKey()
    }

    @objc private func fixMicrophone() {
        if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
            requestMicrophoneIfNeeded()
        } else {
            openSettings("Privacy_Microphone")
        }
    }

    @objc private func fixAccessibility() {
        promptAccessibility()
        openSettings("Privacy_Accessibility")
    }

    @objc private func toggleLaunchAtLogin() {
        do {
            if SMAppService.mainApp.status == .enabled {
                try SMAppService.mainApp.unregister()
            } else {
                try SMAppService.mainApp.register()
            }
        } catch {
            notify("Could not change Launch at Login: \(error.localizedDescription)")
        }
    }

    @objc private func openConfig() {
        if !FileManager.default.fileExists(atPath: configURL.path) {
            try? FileManager.default.createDirectory(at: configURL.deletingLastPathComponent(),
                                                     withIntermediateDirectories: true)
            try? "# voice-to-text config, see scripts/config.example.sh\n"
                .write(to: configURL, atomically: true, encoding: .utf8)
        }
        NSWorkspace.shared.open([configURL], withApplicationAt: URL(fileURLWithPath: "/System/Applications/TextEdit.app"),
                                configuration: NSWorkspace.OpenConfiguration())
    }

    @objc private func openLog() {
        if FileManager.default.fileExists(atPath: logURL.path) {
            NSWorkspace.shared.open(logURL)
        } else {
            notify("No log yet. Dictate something first.")
        }
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

    private func promptAccessibility() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        _ = AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    private func openSettings(_ pane: String) {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
            NSWorkspace.shared.open(url)
        }
    }

    private func notify(_ message: String) {
        let escaped = message.replacingOccurrences(of: "\"", with: "\\\"")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", "display notification \"\(escaped)\" with title \"Voice to Text\""]
        try? process.run()
    }

    private func fatalAlert(_ message: String) {
        let alert = NSAlert()
        alert.messageText = "Voice to Text"
        alert.informativeText = message
        alert.runModal()
        NSApp.terminate(nil)
    }
}
