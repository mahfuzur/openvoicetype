import Foundation

/// The last 10 dictations, **in memory only** (never written to disk, gone when the app quits): Whisper's text next to
/// the cleaned text, for the Recent Dictations menu and for swapping the last paste between the two (⌃⌥Z).
final class DictationHistory {
    struct Entry {
        let date: Date
        let appName: String
        let mode: DictationMode
        /// Whisper's transcript, as it would be pasted (dictionary and output filter applied).
        let raw: String
        /// The cleanup, or nil when there was none. When the meaning guard turned it down, this is the rejected one.
        let cleaned: String?
        /// The meaning guard's reason, when it pasted Whisper's text instead of the cleanup.
        let guardReason: String?
        let target: PasteTarget
        /// It went into the app (not just onto the clipboard), so ⌘Z can take it back.
        var wasPasted: Bool
        /// Whisper's text is the one in the app now (the guard used it, or the user swapped to it).
        var showingRaw: Bool

        /// The version in the app now.
        var shown: String { showingRaw ? raw : cleaned ?? raw }

        /// The other version of the text, if there is one.
        var alternative: String? {
            guard let cleaned, cleaned != raw else { return nil }
            return showingRaw ? cleaned : raw
        }
    }

    private(set) var entries: [Entry] = []

    var last: Entry? { entries.first }

    func add(_ entry: Entry) {
        entries.insert(entry, at: 0)
        if entries.count > 10 { entries.removeLast() }
    }

    /// Something else was pasted since (a Command Mode result): ⌘Z would no longer undo the dictation, so a swap only copies.
    func invalidateLast() {
        guard !entries.isEmpty else { return }
        entries[0].wasPasted = false
    }

    /// After a swap: the other version is now in the app.
    func swappedLast(pasted: Bool) {
        guard !entries.isEmpty else { return }
        entries[0].showingRaw.toggle()
        entries[0].wasPasted = pasted
    }
}
