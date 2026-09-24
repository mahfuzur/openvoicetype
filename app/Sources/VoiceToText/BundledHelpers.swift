import Foundation

/// The bundled whisper-server, whisper-cli and llama-server, run from a copy in Application Support.
///
/// A downloaded DMG marks every file in it as quarantined. Opening the app with Open Anyway approves the app itself,
/// but not the programs it launches: a quarantined helper stays blocked by Gatekeeper, so transcription would hang.
/// The app can't clear the flag inside its own bundle (App Management protects signed apps), so it copies the helpers
/// out: a file the app writes itself isn't quarantined, and the copy keeps its code signature. Only when they changed.
enum BundledHelpers {
    static let names = ["whisper-server", "whisper-cli", "llama-server"]
    private static let bundled = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers")
    private static let installed = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("Voice to Text/Helpers")

    /// The directory to run the helpers from (`VTT_BIN_DIR`): the copy, or the bundle if copying failed; nil in a
    /// build without helpers (the script then uses Homebrew's).
    static let directory: URL? = prepare()

    private static func prepare() -> URL? {
        let fileManager = FileManager.default
        guard fileManager.fileExists(atPath: bundled.appendingPathComponent(names[0]).path) else { return nil }
        do {
            try fileManager.createDirectory(at: installed, withIntermediateDirectories: true)
            for name in names {
                let source = bundled.appendingPathComponent(name)
                let target = installed.appendingPathComponent(name)
                guard !sameFile(source, target) else { continue }
                // Write the bytes to a new file (copyItem would copy the quarantine flag too), then swap it in, so a
                // server still running the old copy keeps its file.
                let temporary = installed.appendingPathComponent(".\(name).new")
                try Data(contentsOf: source).write(to: temporary)
                try fileManager.setAttributes([.posixPermissions: 0o755], ofItemAtPath: temporary.path)
                _ = try fileManager.replaceItemAt(target, withItemAt: temporary)
                AppLog.write("HELPERS installed \(name)")
            }
            return installed
        } catch {
            AppLog.write("HELPERS couldn't copy (\(error.localizedDescription)); running them from the app bundle")
            return bundled
        }
    }

    /// Same size and contents; the helpers are a few MB, so comparing the bytes costs milliseconds.
    private static func sameFile(_ a: URL, _ b: URL) -> Bool {
        guard let sizeA = (try? FileManager.default.attributesOfItem(atPath: a.path))?[.size] as? Int,
              let sizeB = (try? FileManager.default.attributesOfItem(atPath: b.path))?[.size] as? Int,
              sizeA == sizeB else { return false }
        return FileManager.default.contentsEqual(atPath: a.path, andPath: b.path)
    }
}
