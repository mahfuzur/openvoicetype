import AppKit

// Writing a transcript to a dictate.sh that already exited must fail quietly (the raw text is used), not kill the app.
signal(SIGPIPE, SIG_IGN)

// Before anything reads AppSettings: a user coming from "Voice to Text" keeps their settings. Not in the developer modes
// (--settings-snapshots, --recorder-selftest, …), which must not use up the one-time copy on a maintainer's Mac.
if !CommandLine.arguments.dropFirst().contains(where: { $0.hasPrefix("--") }) {
    Migration.copySettings()
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
