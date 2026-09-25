import Foundation

/// What a Command Mode press acts on, decided when the key goes down (the overlay says it before you speak).
enum CommandTarget: String {
    /// Replace the selected text.
    case selection
    /// Replace what was just dictated (selected again through Accessibility).
    case lastDictation = "last_dictation"
    /// New text at the cursor.
    case write
    /// The answer goes to the clipboard: text you can't edit (a web page, a PDF), a terminal, or no text field.
    case copy
}

/// One Command Mode edit and its follow-ups ("shorter still", "go back to the original"): where it happened, the text
/// before the first edit, the instructions so far and the latest result. In memory only; a follow-up has to come within
/// a minute, and Restore Original works for 5 minutes.
final class CommandSession {
    let target: PasteTarget
    let original: String
    let startedAs: CommandTarget
    var instructions: [String] = []
    var current: String?
    /// The latest result went into the app (not just onto the clipboard).
    var pastedLast = false
    var lastUsed = Date()

    init(target: PasteTarget, original: String, startedAs: CommandTarget) {
        self.target = target
        self.original = original
        self.startedAs = startedAs
    }

    var isAlive: Bool { Date().timeIntervalSince(lastUsed) < 300 }
    var canFollowUp: Bool { Date().timeIntervalSince(lastUsed) < 60 && current != nil }
    /// There's an original to put back (not for Write, where there was none).
    var canRestore: Bool { isAlive && !original.isEmpty && current != nil && current != original }
}

/// A decided Command Mode press.
struct CommandPlan {
    let target: CommandTarget
    /// The text the instruction applies to ("" for Write).
    let text: String
    let session: CommandSession
    /// For a replace: the text that must still be selected when the result arrives.
    let expectedSelection: String?
    let source: Selection.Source
    /// When the command selected our last paste itself: where the cursor was, to put it back if nothing is pasted.
    var restoreCursor: Int?

    var isFollowUp: Bool { !session.instructions.isEmpty }

    /// The overlay's chip while recording.
    var chip: String {
        let words = text.split(whereSeparator: { $0.isWhitespace || $0.isNewline }).count
        let count = "\(words) word\(words == 1 ? "" : "s")"
        switch target {
        case .selection: return isFollowUp ? "Follow-up · \(count)" : "\(count) selected"
        case .lastDictation: return "Last dictation · \(count)"
        case .write: return "Write at cursor"
        case .copy where source == .copy && text.isEmpty: return "Copy only · couldn't read the selection"
        case .copy: return text.isEmpty ? "Copy only · no text field" : "Copy only · \(count)"
        }
    }

    /// While Claude works.
    var workingLabel: String { target == .write || (target == .copy && text.isEmpty) ? "Writing" : "Editing" }

    /// `VTT_COMMAND_FILE` for `dictate.sh command`.
    var payload: [String: Any] {
        var json: [String: Any] = ["target": target.rawValue, "original": session.original]
        if isFollowUp {
            json["current"] = session.current ?? ""
            json["turns"] = session.instructions.map { ["instruction": $0] }
        }
        return json
    }
}

enum CommandPlanner {
    enum Decision {
        case plan(CommandPlan)
        /// Command Mode can't run here; the message says why.
        case refuse(String)
    }

    static let maxCharacters = 6_000

    /// Decides what a press acts on. `reselect` selects a text that ends at the cursor (our last paste) and returns where
    /// the cursor was, or nil if it couldn't; it's only called when nothing is selected.
    static func decide(selection: Selection, target: PasteTarget, session: CommandSession?,
                       lastDictation: DictationHistory.Entry?, reselect: @escaping (String) -> Int?) -> Decision {
        let terminal = SelectionReader.terminals.contains(target.bundleID?.lowercased() ?? "")
        if let text = selection.text, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            guard (text as NSString).length <= maxCharacters else {
                return .refuse("Selection too long (6,000 characters max)")
            }
            let replaceable = !terminal && selection.editable
            // Our last result, selected again: a follow-up in the same session.
            if let session, session.isAlive, let current = session.current, squeezed(text) == squeezed(current),
               session.target.check() == .same {
                return .plan(CommandPlan(target: replaceable ? .selection : .copy, text: current, session: session,
                                         expectedSelection: replaceable ? text : nil, source: selection.source))
            }
            let kind: CommandTarget = replaceable ? .selection : .copy
            return .plan(CommandPlan(target: kind, text: text,
                                     session: CommandSession(target: target, original: text, startedAs: kind),
                                     expectedSelection: kind == .selection ? text : nil, source: selection.source))
        }

        // The selection couldn't be read at all: there may be one, so nothing is pasted (the answer is copied).
        if selection.inconclusive {
            return .plan(CommandPlan(target: .copy, text: "", session: CommandSession(target: target, original: "",
                                                                                      startedAs: .copy),
                                     expectedSelection: nil, source: selection.source))
        }

        // Nothing selected: continue the latest of our last result and the last dictation, if it's still right before
        // the cursor and can be selected again.
        if !terminal {
            let sessionDate = session?.canFollowUp == true && session?.pastedLast == true ? session?.lastUsed : nil
            let dictationDate = lastDictation.flatMap { entry in
                entry.wasPasted && Date().timeIntervalSince(entry.date) < 60 ? entry.date : nil
            }
            var candidates: [(Date, () -> Decision?)] = []
            if let session, let date = sessionDate, let current = session.current {
                candidates.append((date, {
                    guard session.target.check() == .same, let cursor = reselect(current) else { return nil }
                    return .plan(CommandPlan(target: .selection, text: current, session: session,
                                             expectedSelection: current, source: .ax, restoreCursor: cursor))
                }))
            }
            if let entry = lastDictation, let date = dictationDate {
                candidates.append((date, {
                    guard entry.target.check() == .same, let cursor = reselect(entry.shown) else { return nil }
                    return .plan(CommandPlan(target: .lastDictation, text: entry.shown,
                                             session: CommandSession(target: target, original: entry.shown,
                                                                     startedAs: .lastDictation),
                                             expectedSelection: entry.shown, source: .ax, restoreCursor: cursor))
                }))
            }
            for (_, attempt) in candidates.sorted(by: { $0.0 > $1.0 }) {
                if let decision = attempt() { return decision }
            }
        }

        // Write: at the cursor in a text field, or onto the clipboard where there's none (or in a terminal).
        let noField = terminal || (selection.knowsFocus && !selection.editable)
        let kind: CommandTarget = noField ? .copy : .write
        return .plan(CommandPlan(target: kind, text: "", session: CommandSession(target: target, original: "", startedAs: kind),
                                 expectedSelection: nil, source: selection.source))
    }

    static func squeezed(_ text: String) -> String {
        text.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.joined(separator: " ")
    }
}
