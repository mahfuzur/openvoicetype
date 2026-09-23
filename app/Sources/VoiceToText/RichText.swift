import Foundation

/// Converts cleaned dictation text into simple HTML so rich-text apps (Notes, Mail, Google Docs, Slack)
/// paste real bulleted and numbered lists. Only used when the text contains list lines.
enum RichText {
    private static let bullet = try! NSRegularExpression(pattern: #"^\s*[-*•]\s+(.*)$"#)
    private static let numbered = try! NSRegularExpression(pattern: #"^\s*\d+[.)]\s+(.*)$"#)

    static func containsList(_ text: String) -> Bool {
        text.split(separator: "\n").contains { match(bullet, String($0)) != nil || match(numbered, String($0)) != nil }
    }

    static func html(from text: String) -> String {
        var html = ""
        var openList: String?
        var paragraph: [String] = []

        func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            html += "<p>" + paragraph.map(escape).joined(separator: "<br>") + "</p>"
            paragraph = []
        }
        func closeList() {
            if let tag = openList { html += "</\(tag)>" }
            openList = nil
        }

        for line in text.components(separatedBy: "\n") {
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                flushParagraph()
                closeList()
                continue
            }
            let (tag, item): (String?, String?) = if let item = match(bullet, line) {
                ("ul", item)
            } else if let item = match(numbered, line) {
                ("ol", item)
            } else {
                (nil, nil)
            }
            if let tag, let item {
                flushParagraph()
                if openList != tag {
                    closeList()
                    html += "<\(tag)>"
                    openList = tag
                }
                html += "<li>\(escape(item))</li>"
            } else {
                closeList()
                paragraph.append(line)
            }
        }
        flushParagraph()
        closeList()
        return "<meta charset=\"utf-8\"><div style=\"font-family: -apple-system, 'Helvetica Neue', sans-serif\">\(html)</div>"
    }

    private static func match(_ regex: NSRegularExpression, _ line: String) -> String? {
        let range = NSRange(line.startIndex..., in: line)
        guard let result = regex.firstMatch(in: line, range: range),
              let itemRange = Range(result.range(at: 1), in: line) else { return nil }
        return String(line[itemRange])
    }

    private static func escape(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }
}
