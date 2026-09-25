import AppKit
import AVFoundation
import SwiftUI

/// First-run setup: a checklist that opens on first launch (and from the menu), so a new Mac goes from the DMG to a
/// working dictation without Terminal. Every step shows a check once done, so running it again just confirms things.
final class SetupWindowController: NSWindowController {
    init(actions: AppActions, lastResult: LastResult) {
        // As tall as the design wants, but never taller than the screen (a 13" laptop with larger text), and resizable.
        let available = (NSScreen.main?.visibleFrame.height ?? 900) - 60
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 620, height: min(760, available)),
                              styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        super.init(window: window)
        let host = NSHostingController(rootView: SetupView(
            actions: actions, lastResult: lastResult, close: { [weak window] in window?.close() }))
        host.sizingOptions = [] // the window's size, not the view's ideal size
        window.contentViewController = host
        window.setContentSize(NSSize(width: 620, height: min(760, available)))
        window.contentMinSize = NSSize(width: 620, height: 420)
        window.title = "Set Up OpenVoiceType"
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show() {
        if window?.isVisible == false { window?.center() }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

/// The most recent dictation, so setup's "Try it" step can show what happened.
final class LastResult: ObservableObject {
    @Published var summary: String?
}

struct SetupView: View {
    let actions: AppActions
    @ObservedObject var lastResult: LastResult
    let close: () -> Void

    @ObservedObject private var settings = AppSettings.shared
    @ObservedObject private var models = ModelManager.shared
    @ObservedObject private var claude = ClaudeCLI.shared
    @State private var micStatus = AVCaptureDevice.authorizationStatus(for: .audio)
    @State private var axAllowed = AXIsProcessTrusted()
    @State private var tryText = ""
    @State private var moveError: String?
    private let poll = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        let _ = models.revision
        let first = AppLocation.needsMove ? 2 : 1 // step numbers, after the optional "Move to Applications"
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Welcome to OpenVoiceType").font(.title2).fontWeight(.semibold)
                Text("Press a hotkey, speak, and clean text appears where you type. A few steps and you're ready.")
                    .foregroundColor(.secondary)
            }
            .padding([.horizontal, .top], 20).padding(.bottom, 12)

            Form {
                if AppLocation.needsMove {
                    step(1, "Move to Applications", done: false,
                         detail: "The app is running from the disk image or Downloads. macOS only keeps its permissions "
                             + "once it's in Applications.") {
                        Button("Move and Reopen") {
                            AppLocation.moveToApplications { moveError = $0 }
                        }
                        if let moveError { Text(moveError).font(.caption).foregroundColor(.red) }
                    }
                }
                step(first, "Microphone", done: micStatus == .authorized,
                     detail: "To hear you. Audio stays on this Mac and is deleted after each dictation.") {
                    if micStatus == .notDetermined {
                        Button("Allow Microphone") {
                            AVCaptureDevice.requestAccess(for: .audio) { _ in
                                DispatchQueue.main.async { micStatus = AVCaptureDevice.authorizationStatus(for: .audio) }
                            }
                        }
                    } else if micStatus != .authorized {
                        Button("Open System Settings") { Permissions.openSettings("Privacy_Microphone") }
                    }
                }
                step(first + 1, "Accessibility", done: axAllowed,
                     detail: "To paste the text into the app you're using. Turn on OpenVoiceType in the list that opens.") {
                    if !axAllowed {
                        Button("Open System Settings") {
                            Permissions.promptAccessibility()
                            Permissions.openSettings("Privacy_Accessibility")
                        }
                        HStack {
                            Text("Already switched on, but this step isn't ticked? The entry belongs to an older copy.")
                                .font(.caption).foregroundColor(.secondary)
                            Spacer()
                            Button("Reset and Ask Again") { Permissions.resetAccessibility() }
                        }
                    }
                }
                Section {
                    stepHeader(first + 2, "Speech model", done: selectedModelInstalled,
                               detail: "Whisper turns your speech into text, on this Mac. Pick one; you can change it later in Settings.")
                    ForEach(ModelCatalog.whisper) { model in
                        ModelRow(model: model, selected: $settings.whisperModel, allowDelete: false)
                    }
                    if !selectedModelInstalled, let model = ModelCatalog.whisperModel(named: settings.whisperModel),
                       !models.isDownloading(model) {
                        Button("Download \(model.title) (\(model.sizeLabel))") { models.download(model) }
                    }
                }
                Section {
                    stepHeader(first + 3, "Cleanup", done: claude.isReady || ModelCatalog.s1Mini.isInstalled,
                               detail: "AI removes filler words and fixes punctuation. Claude runs through your own Claude Code; "
                                   + "S1-mini works offline. You can have both: S1-mini steps in when Claude can't.")
                    ClaudeStatusView()
                    ModelRow(model: ModelCatalog.s1Mini, allowDelete: false)
                    if !claude.isReady && ModelCatalog.s1Mini.isInstalled && settings.cleanupEngine == "s1" {
                        Text("Using S1-mini for now. Once Claude Code is ready, switch in Settings → Cleanup.")
                            .font(.caption).foregroundColor(.secondary)
                    }
                }
                step(first + 4, "Try it", done: lastResult.summary != nil,
                     detail: "Click in the box, \(settings.holdToTalk ? "hold" : "press") \(settings.hotKey.label), "
                         + "say a sentence, then \(settings.holdToTalk ? "let go" : "press it again").") {
                    TextEditor(text: $tryText).font(.body).frame(height: 70)
                    if let summary = lastResult.summary {
                        Text(summary).font(.caption).foregroundColor(.secondary)
                    }
                    LabeledContent("Hotkey") { HotKeyRecorder() }
                }
            }
            .formStyle(.grouped)

            HStack {
                Button("More Settings…") { actions.openSettings() }
                Spacer()
                if !ready { Text("Finish the steps above to start dictating.").font(.caption).foregroundColor(.secondary) }
                Button(ready ? "Done" : "Later") {
                    if ready { settings.setupCompleted = true }
                    close()
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding(16)
        }
        .frame(minWidth: 620, maxWidth: .infinity, minHeight: 420, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
        .onReceive(poll) { _ in
            micStatus = AVCaptureDevice.authorizationStatus(for: .audio)
            axAllowed = AXIsProcessTrusted()
        }
        .onAppear { claude.refresh() }
        .onChange(of: claude.status) { _ in chooseEngine() }
        .onChange(of: models.revision) { _ in chooseEngine() }
    }

    private var selectedModelInstalled: Bool {
        ModelCatalog.whisperModel(named: settings.whisperModel)?.isInstalled ?? false
    }

    private var ready: Bool {
        micStatus == .authorized && selectedModelInstalled
    }

    /// On a new Mac with no Claude yet, S1-mini cleans up so dictation works now; once Claude is ready it becomes the
    /// default again. It never changes the choice of someone who finished setup before (running it again).
    private func chooseEngine() {
        guard !settings.setupCompleted else { return }
        if claude.isReady {
            if settings.cleanupEngine == "s1" { settings.cleanupEngine = "claude" }
        } else if case .checking = claude.status {
            return
        } else if ModelCatalog.s1Mini.isInstalled && settings.cleanupEngine != "openai" {
            settings.cleanupEngine = "s1"
        }
    }

    private func step<Content: View>(_ number: Int, _ title: String, done: Bool, detail: String,
                                     @ViewBuilder content: () -> Content) -> some View {
        Section {
            stepHeader(number, title, done: done, detail: detail)
            content()
        }
    }

    private func stepHeader(_ number: Int, _ title: String, done: Bool, detail: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: done ? "checkmark.circle.fill" : "\(number).circle")
                .font(.title3).foregroundColor(done ? .green : .accentColor)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.headline)
                Text(detail).font(.caption).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// Whether the app runs from somewhere macOS won't keep its permissions for (the DMG, Downloads, or a randomized
/// App Translocation path), and moving it to /Applications.
enum AppLocation {
    static var needsMove: Bool {
        let path = Bundle.main.bundlePath
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return !(path.hasPrefix("/Applications/") || path.hasPrefix("\(home)/Applications/"))
            && (path.contains("/AppTranslocation/") || path.hasPrefix("/Volumes/") || path.hasPrefix("\(home)/Downloads/"))
    }

    /// Copies the app to /Applications and opens the copy, then quits. If either step fails, it stays open and
    /// calls `failed` (on main) with a message.
    static func moveToApplications(failed: @escaping (String) -> Void) {
        let source = Bundle.main.bundleURL
        let destination = URL(fileURLWithPath: "/Applications").appendingPathComponent(source.lastPathComponent)
        do {
            if FileManager.default.fileExists(atPath: destination.path) {
                try FileManager.default.trashItem(at: destination, resultingItemURL: nil)
            }
            try FileManager.default.copyItem(at: source, to: destination)
        } catch {
            return failed("Couldn't copy it: \(error.localizedDescription). Drag it to Applications in Finder instead.")
        }
        let config = NSWorkspace.OpenConfiguration()
        config.createsNewApplicationInstance = true
        NSWorkspace.shared.openApplication(at: destination, configuration: config) { app, error in
            DispatchQueue.main.async {
                if app != nil && error == nil {
                    NSApp.terminate(nil)
                } else {
                    failed("Copied to Applications, but it didn't open (\(error?.localizedDescription ?? "unknown error")). "
                        + "Quit this copy and open OpenVoiceType from Applications.")
                }
            }
        }
    }
}
