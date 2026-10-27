import AppKit

/// Moves a user of the old "Voice to Text" app (bundle ID io.github.mahfuzur.voicetotext, v0.1.x) over to OpenVoiceType.
///
/// A new bundle ID is a new app to macOS: its own preferences, and no Microphone or Accessibility grants. The settings are
/// copied here; the permissions have to be granted again (the normal launch flow asks). The dictionary, config and logs
/// live in paths that didn't change (~/.config/voice-to-text, ~/Library/Logs/voice-to-text), so they need nothing.
enum Migration {
    static let oldBundleID = "io.github.mahfuzur.voicetotext"
    private static let copiedKey = "migratedFromVoiceToText"
    private static let askedKey = "askedAboutOldApp"
    private static let oldHelpers = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("Voice to Text")

    /// Copies the old app's preferences, once, if this app has none of its own yet. Runs before `AppSettings` loads.
    /// The parameters are for tests, which use throwaway domains instead of the real ones.
    static func copySettings(from oldDomain: String = oldBundleID, into defaults: UserDefaults = .standard) {
        guard !defaults.bool(forKey: copiedKey) else { return }
        defaults.set(true, forKey: copiedKey)
        guard let old = defaults.persistentDomain(forName: oldDomain), !old.isEmpty,
              defaults.object(forKey: "setupCompleted") == nil else { return }
        for (key, value) in old where defaults.object(forKey: key) == nil {
            defaults.set(value, forKey: key)
        }
        AppLog.write("MIGRATION copied \(old.count) settings from Voice to Text")
    }

    /// Copies of the old app where users keep apps (not build folders or a mounted DMG).
    static func oldApps() -> [URL] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return NSWorkspace.shared.urlsForApplications(withBundleIdentifier: oldBundleID).filter {
            $0.path.hasPrefix("/Applications/") || $0.path.hasPrefix(home + "/Applications/")
        }
    }

    /// Once: explains the rename and offers to move the old app to the Trash. Moving it also quits it (two apps would
    /// fight over the hotkey) and removes its stale Accessibility entry. Returns true if the old app was quit.
    @discardableResult
    static func offerToRemoveOldApp() -> Bool {
        let defaults = UserDefaults.standard
        let apps = oldApps()
        guard !apps.isEmpty else {
            try? FileManager.default.removeItem(at: oldHelpers) // left by an old app that's gone already
            return false
        }
        guard !defaults.bool(forKey: askedKey) else { return false }
        defaults.set(true, forKey: askedKey)

        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Voice to Text is now OpenVoiceType"
        alert.informativeText = "Your settings have been copied over. macOS sees the renamed app as a new app, so it asks "
            + "for Microphone and Accessibility access once more.\n\nMove the old Voice to Text app to the Trash? "
            + "If it opened when you logged in, turn that on again in Settings → General."
        alert.addButton(withTitle: "Move to Trash")
        alert.addButton(withTitle: "Keep It")
        guard alert.runModal() == .alertFirstButtonReturn else {
            AppLog.write("MIGRATION kept the old app")
            return false
        }

        quitOldApp()
        var failed: [String] = []
        for url in apps {
            do {
                try FileManager.default.trashItem(at: url, resultingItemURL: nil)
                AppLog.write("MIGRATION moved \(url.path) to the Trash")
            } catch {
                failed.append(url.path)
                AppLog.write("MIGRATION couldn't trash \(url.path): \(error.localizedDescription)")
            }
        }
        try? FileManager.default.removeItem(at: oldHelpers)
        // The old app's Accessibility entry would stay in the list, switched on but pointing at nothing.
        let tccutil = Process()
        tccutil.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        tccutil.arguments = ["reset", "Accessibility", oldBundleID]
        try? tccutil.run()
        tccutil.waitUntilExit()

        if !failed.isEmpty {
            let error = NSAlert()
            error.messageText = "Couldn't move the old app to the Trash"
            error.informativeText = "Drag it to the Trash yourself:\n" + failed.joined(separator: "\n")
            error.runModal()
        }
        return true
    }

    /// Asks a running old app to quit, and forces it after 3 s.
    private static func quitOldApp() {
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: oldBundleID)
        guard !running.isEmpty else { return }
        running.forEach { $0.terminate() }
        let deadline = Date().addingTimeInterval(3)
        while running.contains(where: { !$0.isTerminated }) && Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        running.filter { !$0.isTerminated }.forEach { $0.forceTerminate() }
        AppLog.write("MIGRATION quit the running old app")
    }
}
