import AppKit

// Writing a transcript to a dictate.sh that already exited must fail quietly (the raw text is used), not kill the app.
signal(SIGPIPE, SIG_IGN)

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
