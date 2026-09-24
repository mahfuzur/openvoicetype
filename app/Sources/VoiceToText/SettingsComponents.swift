import AppKit
import SwiftUI

/// What the Settings and setup windows can ask the app to do.
struct AppActions {
    let testMicrophone: () -> Void
    let testCleanup: (_ sample: String, _ completion: @escaping (String, String, Double) -> Void) -> Void
    let openSetup: () -> Void
    let openSettings: () -> Void
}

/// A green check, or a grey or orange symbol, before a status line.
struct StatusIcon: View {
    enum Kind { case ok, missing, warning, working }
    let kind: Kind

    var body: some View {
        switch kind {
        case .ok: Image(systemName: "checkmark.circle.fill").foregroundColor(.green)
        case .missing: Image(systemName: "circle").foregroundColor(.secondary)
        case .warning: Image(systemName: "exclamationmark.circle.fill").foregroundColor(.orange)
        case .working: ProgressView().controlSize(.small)
        }
    }
}

/// One downloadable model: its name and size, and Download / progress / Use / Delete.
struct ModelRow: View {
    let model: ModelFile
    /// Shown as selected (a radio button) when the row is part of a choice; nil for S1-mini.
    var selected: Binding<String>?
    var allowDelete = true
    @ObservedObject private var manager = ModelManager.shared

    var body: some View {
        let _ = manager.revision // re-read isInstalled after a download or delete
        HStack(alignment: .top, spacing: 10) {
            if let selected {
                Image(systemName: selected.wrappedValue == model.fileName ? "largecircle.fill.circle" : "circle")
                    .foregroundColor(selected.wrappedValue == model.fileName ? .accentColor : .secondary)
                    .onTapGesture { if model.isInstalled { selected.wrappedValue = model.fileName } }
                    .padding(.top, 2)
            }
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text(model.title).fontWeight(.medium)
                    Text(model.sizeLabel).foregroundColor(.secondary)
                }
                Text(model.detail).font(.caption).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
                progress
            }
            Spacer(minLength: 8)
            actions
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder private var progress: some View {
        switch manager.downloads[model.fileName] {
        case .running(let fraction):
            ProgressView(value: fraction) {
                Text("Downloading… \(Int(fraction * 100))%").font(.caption)
            }
        case .verifying:
            Text("Checking the download…").font(.caption).foregroundColor(.secondary)
        case .failed(let message):
            Text(message).font(.caption).foregroundColor(.red)
        case nil:
            EmptyView()
        }
    }

    @ViewBuilder private var actions: some View {
        if manager.isDownloading(model) {
            Button("Cancel") { manager.cancel(model) }
        } else if model.isInstalled {
            HStack {
                if let selected, selected.wrappedValue != model.fileName {
                    Button("Use") { selected.wrappedValue = model.fileName }
                } else if selected != nil {
                    Text("In use").foregroundColor(.secondary)
                } else {
                    Label("Installed", systemImage: "checkmark.circle.fill").foregroundColor(.green)
                }
                if allowDelete, selected?.wrappedValue != model.fileName {
                    Button { manager.delete(model) } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless).help("Delete this model")
                }
            }
        } else {
            Button(manager.downloads[model.fileName] == nil ? "Download" : "Try Again") { manager.download(model) }
        }
    }
}

/// The Claude CLI's state, with Install, Sign In and Check Again.
struct ClaudeStatusView: View {
    @ObservedObject private var claude = ClaudeCLI.shared

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            StatusIcon(kind: icon).padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).fontWeight(.medium)
                Text(detail).font(.caption).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            switch claude.status {
            case .missing: Button("Install Claude Code…") { claude.install() }
            case .signedOut: Button("Sign In…") { claude.signIn() }
            case .checking, .ready: EmptyView()
            }
            Button { claude.refresh() } label: { Image(systemName: "arrow.clockwise") }
                .buttonStyle(.borderless).help("Check again")
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            claude.refresh() // back from Terminal after installing or signing in
        }
    }

    private var icon: StatusIcon.Kind {
        switch claude.status {
        case .checking: .working
        case .missing: .missing
        case .signedOut: .warning
        case .ready: .ok
        }
    }

    private var title: String {
        switch claude.status {
        case .checking: "Looking for Claude Code…"
        case .missing: "Claude Code isn't installed"
        case .signedOut: "Claude Code isn't signed in"
        case .ready(_, _, let plan): "Claude Code is ready" + (plan.map { " (\($0.capitalized) plan)" } ?? "")
        }
    }

    private var detail: String {
        switch claude.status {
        case .checking: "Checking your installed tools."
        case .missing: "Cleanup runs your own Claude Code CLI, signed in with your Claude account. No API key. "
            + "Installing runs Anthropic's official installer in Terminal."
        case .signedOut(let path, let version): "\(path) (\(version)). Sign in with your Claude account in Terminal."
        case .ready(let path, let version, _): "\(path), version \(version)"
        }
    }
}

/// Click, then press the new key combination. Esc cancels, and so does leaving the window.
struct HotKeyRecorder: View {
    @ObservedObject private var settings = AppSettings.shared
    @ObservedObject private var capture = HotKeyCapture.shared

    var body: some View {
        HStack {
            Button(action: { settings.isRecordingHotKey ? capture.stop() : capture.start() }) {
                Text(settings.isRecordingHotKey ? "Press a shortcut…" : settings.hotKey.label)
                    .frame(minWidth: 120)
            }
            if settings.hotKey != HotKey.Combo.defaultCombo && !settings.isRecordingHotKey {
                Button("Reset") { settings.hotKey = HotKey.Combo.defaultCombo }.buttonStyle(.link)
            }
            if let text = capture.hint ?? settings.hotKeyError {
                Text(text).font(.caption).foregroundColor(settings.hotKeyError != nil && capture.hint == nil ? .red : .secondary)
            }
        }
        // onDisappear doesn't fire when an AppKit window closes, so stop on the window events instead.
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)) { _ in capture.stop() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didResignKeyNotification)) { _ in capture.stop() }
    }
}

/// The one key monitor behind every hotkey recorder (Settings and setup can both be open).
final class HotKeyCapture: ObservableObject {
    static let shared = HotKeyCapture()
    @Published private(set) var hint: String?
    private var monitor: Any?
    private let settings = AppSettings.shared

    func start() {
        stop()
        hint = "Esc to cancel"
        settings.isRecordingHotKey = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            if event.keyCode == 53 && event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty {
                self.stop()
            } else if let combo = HotKey.Combo(event: event) {
                self.settings.hotKey = combo
                self.stop()
            } else {
                self.hint = "Use ⌃ or ⌥ with a key (⌘ shortcuts belong to apps), or a function key"
            }
            return nil
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        hint = nil
        if settings.isRecordingHotKey { settings.isRecordingHotKey = false }
    }
}
