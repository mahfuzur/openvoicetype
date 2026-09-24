import Foundation

/// ~/.config/voice-to-text/dictionary.txt, shared with the CLI: one term per line (Whisper and Claude spell it exactly
/// like this), or `heard => wanted` replacements applied after cleanup. `#` lines are comments.
final class DictionaryFile: ObservableObject {
    struct Replacement: Identifiable, Equatable {
        let id = UUID()
        var heard: String
        var wanted: String
    }

    static let url = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/voice-to-text/dictionary.txt")

    @Published var terms: [String] = [] { didSet { save() } }
    @Published var replacements: [Replacement] = [] { didSet { save() } }

    private static let template = """
        # Voice to Text dictionary (also edited in Settings → Dictionary)
        #
        # One name or term per line: Whisper and Claude will spell it exactly like this.
        #   Claude Code
        # Replacements, applied after cleanup: heard => wanted
        #   cloud code => Claude Code
        """
    /// The file's own comment lines, written back unchanged at the top.
    private var header = template
    private var loading = false

    init() { load() }

    func load() {
        loading = true
        defer { loading = false }
        guard let text = try? String(contentsOf: Self.url, encoding: .utf8) else { return }
        var comments: [String] = []
        var terms: [String] = []
        var replacements: [Replacement] = []
        for raw in text.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("#") {
                comments.append(raw)
            } else if let range = line.range(of: "=>") {
                let heard = line[..<range.lowerBound].trimmingCharacters(in: .whitespaces)
                let wanted = line[range.upperBound...].trimmingCharacters(in: .whitespaces)
                if !heard.isEmpty && !wanted.isEmpty { replacements.append(Replacement(heard: heard, wanted: wanted)) }
            } else if !line.isEmpty {
                terms.append(line)
            }
        }
        header = comments.isEmpty ? Self.template : comments.joined(separator: "\n")
        self.terms = terms
        self.replacements = replacements
    }

    private func save() {
        guard !loading else { return }
        var lines = [header, ""]
        lines += terms.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        let rules = replacements.filter { !$0.heard.trimmingCharacters(in: .whitespaces).isEmpty
            && !$0.wanted.trimmingCharacters(in: .whitespaces).isEmpty }
        if !rules.isEmpty { lines.append("") }
        lines += rules.map { "\($0.heard.trimmingCharacters(in: .whitespaces)) => \($0.wanted.trimmingCharacters(in: .whitespaces))" }
        try? FileManager.default.createDirectory(at: Self.url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? (lines.joined(separator: "\n") + "\n").write(to: Self.url, atomically: true, encoding: .utf8)
    }
}
