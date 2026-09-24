import AppKit

/// Finds the user's own `claude` CLI and checks that it's signed in. The app never reads or stores the login itself:
/// it only runs `claude auth status`, and sends installing and signing in to Terminal where the user can see them.
final class ClaudeCLI: ObservableObject {
    static let shared = ClaudeCLI()

    enum Status: Equatable {
        case checking
        case missing
        case signedOut(path: String, version: String)
        /// `plan` is the subscription type ("pro", "max", "team"), when the CLI reports one.
        case ready(path: String, version: String, plan: String?)
    }

    @Published private(set) var status: Status = .checking

    /// The resolved path, for `dictate.sh` (`CLAUDE_BIN`). nil until found.
    var path: String? {
        switch status {
        case .signedOut(let path, _), .ready(let path, _, _): path
        case .checking, .missing: nil
        }
    }

    var isReady: Bool { if case .ready = status { return true } else { return false } }

    private let queue = DispatchQueue(label: "VoiceToText.ClaudeCLI")
    private var checking = false

    /// Re-checks in the background (finding it can take a second: the login shell loads the user's profile).
    func refresh() {
        guard !checking else { return }
        checking = true
        if path == nil { status = .checking }
        queue.async {
            let result = Self.check()
            DispatchQueue.main.async {
                self.checking = false
                self.status = result
            }
        }
    }

    /// Opens Terminal and runs Anthropic's official installer, then the sign-in.
    func install() {
        runInTerminal(name: "install-claude-code", """
            echo "Installing Claude Code with Anthropic's official installer…"
            echo
            curl -fsSL https://claude.ai/install.sh | bash || exit 1
            echo
            echo "Now sign in with your Claude account (a browser window opens)…"
            "$HOME/.local/bin/claude" auth login --claudeai
            """)
    }

    func signIn() {
        guard let path else { return install() }
        runInTerminal(name: "sign-in-claude-code", """
            echo "Sign in with your Claude account (a browser window opens)…"
            \(Self.shellQuote(path)) auth login --claudeai
            """)
    }

    /// Writes a `.command` file and opens it: Terminal runs it visibly, and no Automation permission is needed.
    private func runInTerminal(name: String, _ body: String) {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("voice-to-text/\(name).command")
        let script = """
            #!/bin/bash
            clear
            \(body)
            echo
            echo "Done. You can close this window and go back to Voice to Text."

            """
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try script.write(to: url, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
            NSWorkspace.shared.open(url)
        } catch {
            AppLog.write("CLAUDE could not open Terminal: \(error.localizedDescription)")
        }
    }

    private static func check() -> Status {
        guard let path = find() else { return .missing }
        let version = run(path, ["--version"]).output
            .components(separatedBy: " ").first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let auth = run(path, ["auth", "status", "--json"])
        guard let data = auth.output.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              json["loggedIn"] as? Bool == true else {
            return .signedOut(path: path, version: version)
        }
        return .ready(path: path, version: version, plan: json["subscriptionType"] as? String)
    }

    /// The login shell's `PATH` finds npm, nvm and custom installs; then the usual places.
    private static func find() -> String? {
        // The last line: a profile may print its own output first.
        let shell = run("/bin/zsh", ["-lc", "command -v claude"], timeout: 5).output
            .components(separatedBy: .newlines).last { $0.hasPrefix("/") } ?? ""
        if FileManager.default.isExecutableFile(atPath: shell) { return shell }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return ["\(home)/.local/bin/claude", "/opt/homebrew/bin/claude", "/usr/local/bin/claude", "\(home)/.claude/local/claude"]
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    /// Runs a command with a timeout, from a neutral directory, with the directory of `claude` on `PATH` (an npm
    /// install is a node script and needs its `node` next to it).
    private static func run(_ executable: String, _ args: [String], timeout: TimeInterval = 10) -> (status: Int32, output: String) {
        let process = Process()
        let output = Pipe()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = args
        process.currentDirectoryURL = URL(fileURLWithPath: "/tmp")
        var env = ProcessInfo.processInfo.environment
        let dir = (executable as NSString).deletingLastPathComponent
        env["PATH"] = "\(dir):/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:" + (env["PATH"] ?? "")
        process.environment = env
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        do { try process.run() } catch { return (-1, "") }
        let timer = DispatchWorkItem { if process.isRunning { process.terminate() } }
        DispatchQueue.global().asyncAfter(deadline: .now() + timeout, execute: timer)
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        timer.cancel()
        return (process.terminationStatus, String(decoding: data, as: UTF8.self))
    }

    static func shellQuote(_ string: String) -> String { "'" + string.replacingOccurrences(of: "'", with: "'\\''") + "'" }
}
