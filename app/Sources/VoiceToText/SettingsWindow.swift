import AppKit
import AVFoundation
import ServiceManagement
import SwiftUI

/// The Settings window: a toolbar with one SwiftUI pane per tab, in an `NSTabViewController`. (The SwiftUI `Settings`
/// scene needs the SwiftUI app lifecycle, which this AppKit menu-bar app doesn't use.)
final class SettingsWindowController: NSWindowController {
    enum Pane: Int { case general, speech, cleanup, dictionary, modes, about }

    private let tabs = SettingsTabViewController()

    init(actions: AppActions) {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 500),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        super.init(window: window)
        tabs.tabStyle = .toolbar
        let panes: [(String, String, AnyView, CGFloat)] = [
            ("General", "gearshape", AnyView(GeneralPane(actions: actions)), 720),
            ("Speech", "waveform", AnyView(SpeechPane()), 400),
            ("Cleanup", "sparkles", AnyView(CleanupPane(actions: actions)), 660),
            ("Dictionary", "character.book.closed", AnyView(DictionaryPane()), 540),
            ("Modes", "rectangle.3.group", AnyView(ModesPane()), 500),
            ("About", "info.circle", AnyView(AboutPane(actions: actions)), 600),
        ]
        for (title, symbol, view, height) in panes {
            let controller = NSHostingController(rootView: view.frame(width: 600, height: height))
            let item = NSTabViewItem(viewController: controller)
            item.label = title
            item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
            tabs.addTabViewItem(item)
        }
        window.contentViewController = tabs
        window.title = "OpenVoiceType Settings"
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show(_ pane: Pane? = nil) {
        if let pane { tabs.selectedTabViewItemIndex = pane.rawValue }
        if window?.isVisible == false {
            tabs.fitWindow(animate: false) // the first pane isn't "selected", so nothing else sizes the window for it
            window?.center()
        }
        NSApp.activate(ignoringOtherApps: true) // an accessory app's window doesn't come forward by itself
        window?.makeKeyAndOrderFront(nil)
    }
}

/// Resizes the window to the selected pane, keeping its top edge in place: on every tab change, and when the window
/// opens (the first pane never gets a "did select", so without that it opened at the placeholder size, clipped).
final class SettingsTabViewController: NSTabViewController {
    override func tabView(_ tabView: NSTabView, didSelect tabViewItem: NSTabViewItem?) {
        super.tabView(tabView, didSelect: tabViewItem)
        fitWindow(animate: view.window?.isVisible ?? false)
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        fitWindow(animate: false)
    }

    func fitWindow(animate: Bool) {
        guard let window = view.window, let pane = tabView.selectedTabViewItem?.viewController?.view else { return }
        let size = pane.fittingSize
        guard size.width > 0, size.height > 0 else { return }
        let content = window.frameRect(forContentRect: NSRect(origin: .zero, size: size))
        var frame = window.frame
        frame.origin.y += frame.height - content.height
        frame.size = content.size
        window.setFrame(frame, display: true, animate: animate)
    }
}

// MARK: - General

struct GeneralPane: View {
    let actions: AppActions
    @ObservedObject private var settings = AppSettings.shared
    @State private var devices = AudioDevices.inputDevices()
    @State private var micAllowed = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
    @State private var axAllowed = AXIsProcessTrusted()
    @State private var launchAtLogin = SMAppService.mainApp.status == .enabled
    @State private var loginError: String?
    private let poll = Timer.publish(every: 1.5, on: .main, in: .common).autoconnect()

    var body: some View {
        Form {
            Section("Hotkey") {
                LabeledContent("Shortcut") { HotKeyRecorder() }
                Picker("When you press it", selection: $settings.holdToTalk) {
                    Text("Press to start, press again to stop").tag(false)
                    Text("Hold while you speak (hold to talk)").tag(true)
                }
                LabeledContent("Command Mode") { HotKeyRecorder(slot: .command) }
                LabeledContent("Swap last paste") { HotKeyRecorder(slot: .swap) }
                Text("Esc cancels a recording. Command Mode: select text, press it and say how to change it "
                    + "(\u{201C}make this shorter\u{201D}); with nothing selected it writes at the cursor. "
                    + "Swap last paste replaces what was just pasted with Whisper's own text, or back.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Microphone") {
                Picker("Input", selection: $settings.inputDeviceUID) {
                    Text("System Default" + (AudioDevices.defaultInput().map { " (\($0.name))" } ?? "")).tag(String?.none)
                    ForEach(devices, id: \.uid) { device in
                        Text(device.name + (device.isBluetooth ? " (Bluetooth)" : "")).tag(String?.some(device.uid))
                    }
                }
                .onChange(of: settings.inputDeviceUID) { uid in
                    settings.inputDeviceName = uid.flatMap(AudioDevices.device(uid:))?.name
                }
                Button("Test Microphone") { actions.testMicrophone() }
            }
            Section("Pasting") {
                Toggle("Paste automatically", isOn: $settings.autoPaste)
                Toggle("Paste lists as rich text (bullets in Mail, Notes, Slack…)", isOn: $settings.richPaste)
            }
            Section("Feedback") {
                Toggle("Show the overlay while recording", isOn: $settings.showOverlay)
                Picker("Overlay position", selection: $settings.overlayPosition) {
                    Text("Bottom").tag(OverlayController.Position.bottom)
                    Text("Top").tag(OverlayController.Position.top)
                }
                .disabled(!settings.showOverlay)
                Toggle("Play sounds", isOn: $settings.playSounds)
            }
            Section("Startup") {
                Toggle("Open OpenVoiceType when you log in", isOn: Binding(get: { launchAtLogin }, set: setLaunchAtLogin))
                if let loginError { Text(loginError).font(.caption).foregroundColor(.red) }
            }
            Section("Permissions") {
                permissionRow("Microphone", allowed: micAllowed, why: "to hear you") {
                    if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
                        AVCaptureDevice.requestAccess(for: .audio) { _ in }
                    } else {
                        Permissions.openSettings("Privacy_Microphone")
                    }
                }
                permissionRow("Accessibility", allowed: axAllowed, why: "to paste into other apps") {
                    Permissions.promptAccessibility()
                    Permissions.openSettings("Privacy_Accessibility")
                }
                if !axAllowed {
                    HStack {
                        Text("OpenVoiceType is already switched on in the list, but this still says Needed?")
                            .font(.caption).foregroundColor(.secondary)
                        Spacer()
                        Button("Reset…") { Permissions.resetAccessibility() }
                            .help("Removes the old entry and asks again. Then switch OpenVoiceType on in the list.")
                    }
                }
            }
        }
        .formStyle(.grouped)
        .onReceive(poll) { _ in
            micAllowed = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
            axAllowed = AXIsProcessTrusted()
        }
        .onAppear { devices = AudioDevices.inputDevices() }
        // A reopened window doesn't get onAppear again; refresh when it comes to the front (a mic plugged in since).
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            devices = AudioDevices.inputDevices()
        }
    }

    private func permissionRow(_ name: String, allowed: Bool, why: String, fix: @escaping () -> Void) -> some View {
        HStack {
            StatusIcon(kind: allowed ? .ok : .warning)
            Text(name)
            Text(allowed ? "Allowed" : "Needed \(why)").foregroundColor(.secondary)
            Spacer()
            if !allowed { Button("Allow…", action: fix) }
        }
    }

    private func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            loginError = nil
        } catch {
            loginError = error.localizedDescription
        }
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }
}

// MARK: - Speech

struct SpeechPane: View {
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        Form {
            Section {
                ForEach(ModelCatalog.whisper) { model in
                    ModelRow(model: model, selected: $settings.whisperModel)
                }
            } header: {
                Text("Speech model")
            } footer: {
                HStack {
                    Text("Whisper runs on this Mac: your audio never leaves it. English only for now.")
                    Spacer()
                    Button("Show in Finder") {
                        try? FileManager.default.createDirectory(at: ModelCatalog.whisperDirectory, withIntermediateDirectories: true)
                        NSWorkspace.shared.activateFileViewerSelecting([ModelCatalog.whisperDirectory])
                    }
                    .buttonStyle(.link)
                }
                .font(.caption).foregroundColor(.secondary)
            }
            Section {
                Text("Words Whisper and Claude should spell exactly (names, products, jargon) are in the Dictionary tab.")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Cleanup

struct CleanupPane: View {
    let actions: AppActions
    @ObservedObject private var settings = AppSettings.shared
    @ObservedObject private var models = ModelManager.shared
    @State private var testResult: (text: String, engine: String, seconds: Double)?
    @State private var testing = false
    private static let sample = "um so i think we should uh move the team meeting to thursday at 3 pm "
        + "and can you also invite sarah from the design team"

    var body: some View {
        let _ = models.revision
        Form {
            Section {
                Toggle("Clean up text with AI", isOn: $settings.refine)
                Picker("Clean up with", selection: $settings.cleanupEngine) {
                    Text("Claude (your Claude Code)").tag("claude")
                    Text("An OpenAI-compatible API (Ollama, LM Studio, OpenAI, Groq…)").tag("openai")
                    Text("S1-mini (on this Mac, offline)").tag("s1")
                }
                .pickerStyle(.radioGroup)
                .disabled(!settings.refine)
            } footer: {
                Text("Cleanup removes filler words, fixes punctuation and formats lists. Off pastes Whisper's text as is. "
                    + "If a cleanup drops a number or a \u{201C}not\u{201D}, Whisper's text is pasted instead.")
                    .font(.caption).foregroundColor(.secondary)
            }
            if settings.cleanupEngine == "openai" || settings.commandEngine == "openai" {
                APIEndpointSection()
            }
            Section {
                Picker("Command Mode uses", selection: $settings.commandEngine) {
                    Text("Claude").tag("claude")
                    Text("The API").tag("openai")
                }
                .pickerStyle(.segmented)
            } footer: {
                Text("Command Mode (\(settings.commandHotKey.label)) edits the selected text by voice, so it needs a model that "
                    + "follows instructions: Claude, or a capable API model. The selection and your instruction are sent to it.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Claude") {
                ClaudeStatusView()
                Picker("Model", selection: $settings.claudeModel) {
                    Text("Haiku (fastest)").tag("haiku")
                    Text("Sonnet (smarter, slower)").tag("sonnet")
                }
                HStack {
                    Text("Only the transcript text is sent to Claude, never your audio. Your plan's usage limits apply.")
                        .font(.caption).foregroundColor(.secondary)
                    Spacer()
                    Button("Claude's terms") { NSWorkspace.shared.open(Updater.termsPage) }
                        .buttonStyle(.link).font(.caption)
                }
            }
            Section("S1-mini (offline)") {
                // Not deletable while it's the cleanup engine: every dictation would lose its cleanup.
                ModelRow(model: ModelCatalog.s1Mini, allowDelete: !settings.usesS1)
                Toggle("Use S1-mini when Claude or the API is unavailable (offline, signed out, an error)",
                       isOn: $settings.s1Fallback)
                    .disabled(!ModelCatalog.s1Mini.isInstalled || settings.cleanupEngine == "s1")
                Text("S1-mini by Superwhisper. English only; it leaves Code mode (editors, terminals) as raw text.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section("Test") {
                HStack {
                    Button(testing ? "Testing…" : "Test Cleanup") { runTest() }.disabled(testing)
                    Text("Runs a sample sentence through your current settings.").font(.caption).foregroundColor(.secondary)
                }
                if let testResult {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(testResult.text).textSelection(.enabled)
                        Text("\(testResult.engine) · \(String(format: "%.1f", testResult.seconds)) s")
                            .font(.caption).foregroundColor(.secondary)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func runTest() {
        testing = true
        actions.testCleanup(Self.sample) { text, engine, seconds in
            testResult = (text, engine, seconds)
            testing = false
        }
    }
}

/// The OpenAI-compatible endpoint: a service or your own base URL, the model, and the key (kept in the Keychain, per host).
struct APIEndpointSection: View {
    @ObservedObject private var settings = AppSettings.shared
    @State private var key = ""
    @State private var keyStatus: String?
    @State private var models: [String] = []
    @State private var loadStatus: String?
    @State private var loading = false

    static let presets: [(name: String, url: String)] = [
        ("Ollama (on this Mac)", "http://localhost:11434/v1"),
        ("LM Studio (on this Mac)", "http://localhost:1234/v1"),
        ("OpenAI", "https://api.openai.com/v1"),
        ("Groq", "https://api.groq.com/openai/v1"),
        ("OpenRouter", "https://openrouter.ai/api/v1"),
    ]

    private var baseURL: String { settings.openaiBaseURL.trimmingCharacters(in: .whitespaces) }
    private var isLocal: Bool {
        let host = URL(string: baseURL)?.host ?? ""
        return host == "localhost" || host.hasPrefix("127.") || host == "::1"
    }

    var body: some View {
        Section {
            Picker("Service", selection: Binding(
                get: { Self.presets.first { $0.url == baseURL }?.url ?? "custom" },
                set: { url in
                    guard url != "custom" else { return }
                    settings.openaiBaseURL = url
                    models = []
                    loadStatus = nil
                    loadKey()
                })) {
                ForEach(Self.presets, id: \.url) { Text($0.name).tag($0.url) }
                Text("Other (enter its base URL)").tag("custom")
            }
            TextField("Base URL", text: $settings.openaiBaseURL)
                .onSubmit(loadKey)
            HStack {
                TextField("Model", text: $settings.openaiModel, prompt: Text("Load Models lists them"))
                if !models.isEmpty {
                    Menu("Choose") {
                        ForEach(models, id: \.self) { model in Button(model) { settings.openaiModel = model } }
                    }
                    .fixedSize()
                }
                Button(loading ? "Loading…" : "Load Models", action: loadModels).disabled(loading || baseURL.isEmpty)
            }
            HStack {
                SecureField("API key", text: $key, prompt: Text(isLocal ? "Not needed for Ollama or LM Studio" : "Paste your key"))
                Button("Save") {
                    keyStatus = APIKeychain.setKey(key, for: baseURL)
                        ? (key.isEmpty ? "Key removed" : "Saved in your Keychain") : "The Keychain refused the key"
                }
            }
            if let text = keyStatus ?? loadStatus {
                Text(text).font(.caption).foregroundColor(.secondary)
            }
        } header: {
            Text("API")
        } footer: {
            Text(isLocal ? "Runs on this Mac: the transcript stays here. Your audio never leaves the Mac."
                : "Only the transcript text is sent to \(APIKeychain.account(baseURL)), with your key; never your audio. "
                    + "That service's terms and prices apply.")
                .font(.caption).foregroundColor(.secondary)
        }
        .onAppear(perform: loadKey)
    }

    private func loadKey() {
        key = APIKeychain.key(for: baseURL) ?? ""
        keyStatus = nil
    }

    /// `GET <base>/models`: every OpenAI-compatible server lists its models there.
    private func loadModels() {
        guard let url = URL(string: baseURL.hasSuffix("/") ? baseURL + "models" : baseURL + "/models") else {
            loadStatus = "That base URL isn't valid"
            return
        }
        var request = URLRequest(url: url, timeoutInterval: 10)
        if let saved = APIKeychain.key(for: baseURL) { request.setValue("Bearer \(saved)", forHTTPHeaderField: "Authorization") }
        loading = true
        URLSession.shared.dataTask(with: request) { data, response, error in
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            let ids = (data.flatMap { try? JSONSerialization.jsonObject(with: $0) } as? [String: Any])?["data"]
                .flatMap { $0 as? [[String: Any]] }?.compactMap { $0["id"] as? String }.sorted() ?? []
            DispatchQueue.main.async {
                loading = false
                keyStatus = nil
                models = ids
                if !ids.isEmpty {
                    loadStatus = "Found \(ids.count) model\(ids.count == 1 ? "" : "s"). Choose one."
                } else if let error {
                    loadStatus = "Couldn't reach it: \(error.localizedDescription)"
                } else {
                    loadStatus = status == 401 || status == 403 ? "The server wants an API key (save it first)"
                        : "No models found (HTTP \(status))"
                }
            }
        }.resume()
    }
}

// MARK: - Dictionary

struct DictionaryPane: View {
    @StateObject private var dictionary = DictionaryFile()
    @State private var newTerm = ""
    @State private var newHeard = ""
    @State private var newWanted = ""

    var body: some View {
        Form {
            Section {
                ForEach(Array(dictionary.terms.enumerated()), id: \.offset) { index, term in
                    HStack {
                        Text(term)
                        Spacer()
                        removeButton { dictionary.terms.remove(at: index) }
                    }
                }
                HStack {
                    TextField("Add a word or name", text: $newTerm).onSubmit(addTerm)
                    Button("Add", action: addTerm).disabled(newTerm.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            } header: {
                Text("Spell these exactly")
            } footer: {
                Text("Names, products and jargon, e.g. PostgreSQL or Claude Code. Whisper and Claude both get this list.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section {
                ForEach(dictionary.replacements) { rule in
                    HStack {
                        Text(rule.heard)
                        Image(systemName: "arrow.right").foregroundColor(.secondary)
                        Text(rule.wanted)
                        Spacer()
                        removeButton { dictionary.replacements.removeAll { $0.id == rule.id } }
                    }
                }
                HStack {
                    TextField("Heard", text: $newHeard)
                    Image(systemName: "arrow.right").foregroundColor(.secondary)
                    TextField("Replace with", text: $newWanted).onSubmit(addReplacement)
                    Button("Add", action: addReplacement)
                        .disabled(newHeard.trimmingCharacters(in: .whitespaces).isEmpty
                            || newWanted.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            } header: {
                Text("Replacements")
            } footer: {
                HStack {
                    Text("Fixes a word Whisper keeps mishearing, after cleanup (e.g. cloud code → Claude Code).")
                    Spacer()
                    Button("Open File") { NSWorkspace.shared.open(DictionaryFile.url) }.buttonStyle(.link)
                }
                .font(.caption).foregroundColor(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear { dictionary.load() }
    }

    private func removeButton(_ action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: "minus.circle") }.buttonStyle(.borderless).help("Remove")
    }

    private func addTerm() {
        let term = newTerm.trimmingCharacters(in: .whitespaces)
        guard !term.isEmpty else { return }
        if !dictionary.terms.contains(term) { dictionary.terms.append(term) }
        newTerm = ""
    }

    private func addReplacement() {
        let heard = newHeard.trimmingCharacters(in: .whitespaces)
        let wanted = newWanted.trimmingCharacters(in: .whitespaces)
        guard !heard.isEmpty, !wanted.isEmpty else { return }
        dictionary.replacements.append(.init(heard: heard, wanted: wanted))
        newHeard = ""
        newWanted = ""
    }
}

// MARK: - Modes

struct ModesPane: View {
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        Form {
            Section {
                Picker("Mode", selection: $settings.modeOverride) {
                    Text("Auto (by app)").tag(DictationMode?.none)
                    ForEach(DictationMode.allCases, id: \.self) { mode in
                        Text(mode.title).tag(DictationMode?.some(mode))
                    }
                }
            } footer: {
                Text("Auto picks Chat for Slack and Teams, Email for Mail and Outlook, Code for editors and terminals, "
                    + "and Notes for Notes, Notion and Obsidian. Everything else uses Default.")
                    .font(.caption).foregroundColor(.secondary)
            }
            Section {
                ForEach(settings.appModes.keys.sorted { name(of: $0) < name(of: $1) }, id: \.self) { bundleID in
                    HStack {
                        Picker(name(of: bundleID), selection: Binding(
                            get: { settings.appModes[bundleID] ?? .default },
                            set: { settings.appModes[bundleID] = $0 })) {
                            ForEach(DictationMode.allCases, id: \.self) { Text($0.title).tag($0) }
                        }
                        Button {
                            settings.appModes[bundleID] = nil
                            settings.appNames[bundleID] = nil
                        } label: { Image(systemName: "minus.circle") }
                            .buttonStyle(.borderless).help("Remove")
                    }
                }
                Button("Add App…", action: addApp)
            } header: {
                Text("Your apps")
            } footer: {
                Text("Choose the mode for an app yourself. This wins over Auto's built-in list.")
                    .font(.caption).foregroundColor(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    private func name(of bundleID: String) -> String { settings.appNames[bundleID] ?? bundleID }

    private func addApp() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.application]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.prompt = "Add"
        guard panel.runModal() == .OK, let url = panel.url, let bundle = Bundle(url: url),
              let bundleID = bundle.bundleIdentifier else { return }
        settings.appNames[bundleID] = FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
        settings.appModes[bundleID] = DictationMode.forApp(bundleID: bundleID)
    }
}

// MARK: - About

struct AboutPane: View {
    let actions: AppActions
    @ObservedObject private var settings = AppSettings.shared
    @ObservedObject private var updater = Updater.shared

    var body: some View {
        Form {
            Section {
                HStack(spacing: 12) {
                    Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 48, height: 48)
                    VStack(alignment: .leading) {
                        Text("OpenVoiceType").font(.headline)
                        Text("Version \(Updater.currentVersion)").foregroundColor(.secondary)
                    }
                }
            }
            Section("Updates") {
                Toggle("Check for updates once a day", isOn: $settings.checkForUpdates)
                HStack {
                    if let release = updater.available {
                        Text("Version \(release.version) is available.")
                        Spacer()
                        Button("Download…") { updater.openReleasePage() }
                    } else {
                        Text(updater.checking ? "Checking…" : updater.lastChecked == nil ? "Not checked yet."
                            : "Up to date (checked \(updater.lastChecked!.formatted(.relative(presentation: .named)))).")
                            .foregroundColor(.secondary)
                        Spacer()
                        Button("Check Now") { updater.check() }.disabled(updater.checking)
                    }
                }
            }
            Section("Credits") {
                Text("Speech recognition: whisper.cpp (MIT). Offline cleanup: S1-mini by Superwhisper, run with llama.cpp (MIT). "
                    + "Cleanup with Claude uses your own Claude Code CLI.")
                    .font(.caption).fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button("Third-Party Licenses") {
                        if let url = Bundle.main.url(forResource: "ThirdPartyLicenses", withExtension: nil) {
                            NSWorkspace.shared.open(url)
                        }
                    }
                    Button("How OpenVoiceType Uses Claude Code") { NSWorkspace.shared.open(Updater.termsPage) }
                }
                .buttonStyle(.link)
            }
            Section("Help") {
                Toggle("Keep dictated text in the log (for debugging)", isOn: $settings.logText)
                Text("Off: the log keeps timings and outcomes only, and text an older version logged is removed.")
                    .font(.caption).foregroundColor(.secondary)
                HStack {
                    Button("Run Setup Again…") { actions.openSetup() }
                    Button("Open Log") {
                        NSWorkspace.shared.open(FileManager.default.homeDirectoryForCurrentUser
                            .appendingPathComponent("Library/Logs/voice-to-text"))
                    }
                    Button("Project Page") {
                        NSWorkspace.shared.open(URL(string: "https://github.com/\(Updater.repository)")!)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

/// Microphone and Accessibility helpers shared by the menu, Settings and setup.
enum Permissions {
    static func promptAccessibility() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        _ = AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    /// The Accessibility switch can show "on" for an older copy of the app with a different signature (a local build, or
    /// before the release certificate), and turning it off and on doesn't fix that. This removes the app's entry
    /// (`tccutil reset`, only for this app, no password needed) and asks again, so the list gets a fresh one.
    static func resetAccessibility() {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        process.arguments = ["reset", "Accessibility", Bundle.main.bundleIdentifier ?? "io.github.mahfuzur.openvoicetype"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try? process.run()
        process.waitUntilExit()
        AppLog.write("PERMISSION accessibility reset (exit \(process.terminationStatus))")
        promptAccessibility()
        openSettings("Privacy_Accessibility")
    }

    static func openSettings(_ pane: String) {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
            NSWorkspace.shared.open(url)
        }
    }
}
