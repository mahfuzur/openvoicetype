import AppKit
import Carbon

/// `VoiceToText --logic-selftest <report>`: checks the parts of pasting and cleanup that work without sending keystrokes
/// or recording (key codes for the current layout, reading the paste target, the Keychain, refine's result file and the
/// swap logic). One `OK` or `FAIL` line per check. Pasting itself needs a person: see the manual test list in the plan.
enum LogicSelfTest {
    static func run() -> [String] {
        var lines: [String] = []
        func check(_ name: String, _ passed: Bool, _ detail: String = "") {
            lines.append("\(passed ? "OK  " : "FAIL") \(name)\(detail.isEmpty ? "" : ": \(detail)")")
        }

        // The key that types "v" and "z" with ⌘ in the current layout (9 and 6 in QWERTY).
        let v = KeyboardLayout.keyCode(for: "v")
        let z = KeyboardLayout.keyCode(for: "z")
        check("layout key for v", v != nil, v.map { "\($0)" } ?? "not found, falls back to \(kVK_ANSI_V)")
        check("layout key for z", z != nil, z.map { "\($0)" } ?? "not found, falls back to \(kVK_ANSI_Z)")

        // Capturing the frontmost app works with or without Accessibility, and a capture matches itself.
        let target = PasteTarget.capture()
        check("paste target", target.pid != 0, "app=\(target.appName) window=\(target.window != nil) "
            + "title=\(target.windowTitle != nil) secure=\(target.isSecure) accessibility=\(AXIsProcessTrusted())")
        check("paste target unchanged", target.check() == .same)
        check("title normalization", PasteTarget.normalized("(3) Inbox - Gmail") == PasteTarget.normalized("Inbox (12) - Gmail")
            && PasteTarget.normalized("#general - Slack") != PasteTarget.normalized("#random - Slack")
            && PasteTarget.normalized("⠋ claude") == PasteTarget.normalized("✳ claude")
            && PasteTarget.normalized("Notes — Edited") == "Notes" && PasteTarget.normalized("#general") == "#general")

        // Keychain: save, read back and remove a key for a host nobody uses.
        let host = "https://vtt-logic-selftest.invalid/v1"
        let saved = APIKeychain.setKey("sk-selftest-\(UUID().uuidString.prefix(8))", for: host)
        let read = APIKeychain.key(for: host)
        APIKeychain.setKey("", for: host)
        check("keychain round trip", saved && read?.hasPrefix("sk-selftest-") == true && APIKeychain.key(for: host) == nil)
        check("keychain account", APIKeychain.account("http://localhost:11434/v1") == "localhost:11434"
            && APIKeychain.account("https://api.openai.com/v1") == "api.openai.com")

        // refine's result file.
        let json = #"{"engine":"openai","error":"limit","guard":"a number (12)","rejected":"We have users.","resets":"3:45 PM","status":"guard-raw"}"#
        let details = try? JSONDecoder().decode(Dictation.CleanupDetails.self, from: Data(json.utf8))
        check("result file", details?.guardReason == "a number (12)" && details?.rejected == "We have users."
            && details?.resets == "3:45 PM" && details?.engine == "openai")

        // Swap: cleaned → Whisper's text → cleaned; nothing to swap without a cleanup.
        let history = DictationHistory()
        history.add(.init(date: Date(), appName: "Test", mode: .default, raw: "um we have 12 users", cleaned: "We have 12 users.",
                          guardReason: nil, target: target, wasPasted: true, showingRaw: false))
        let first = history.last?.alternative
        history.swappedLast(pasted: true)
        let second = history.last?.alternative
        history.add(.init(date: Date(), appName: "Test", mode: .default, raw: "hello", cleaned: nil, guardReason: nil, target: target,
                          wasPasted: true, showingRaw: true))
        check("swap alternatives", first == "um we have 12 users" && second == "We have 12 users."
            && history.last?.alternative == nil)
        for _ in 0..<12 {
            history.add(.init(date: Date(), appName: "", mode: .default, raw: "x", cleaned: nil, guardReason: nil, target: target,
                              wasPasted: false, showingRaw: true))
        }
        check("history keeps 10", history.entries.count == 10)

        // Command Mode: the line-copy guard and the planner's decisions.
        check("line-copy guard", SelectionReader.looksLikeLineCopy("foo()\n", bundleID: "com.microsoft.VSCode")
            && !SelectionReader.looksLikeLineCopy("foo\nbar\n", bundleID: "com.microsoft.VSCode")
            && !SelectionReader.looksLikeLineCopy("foo()\n", bundleID: "com.apple.TextEdit"))
        func decide(_ text: String?, editable: Bool = true, knowsFocus: Bool = true, session: CommandSession? = nil)
            -> CommandPlanner.Decision {
            CommandPlanner.decide(selection: Selection(text: text, source: .ax, editable: editable, knowsFocus: knowsFocus),
                                  target: target, session: session, lastDictation: nil, reselect: { _ in nil })
        }
        func planned(_ decision: CommandPlanner.Decision) -> CommandPlan? {
            if case .plan(let plan) = decision { return plan }
            return nil
        }
        check("command: edit a selection", planned(decide("hello world"))?.target == .selection
            && planned(decide("hello world"))?.chip == "2 words selected")
        check("command: read-only text is copy only", planned(decide("hello world", editable: false))?.target == .copy)
        if case .refuse = decide(String(repeating: "x", count: CommandPlanner.maxCharacters + 1)) {
            check("command: too long is refused", true)
        } else {
            check("command: too long is refused", false)
        }
        var unreadable = Selection(text: nil, source: .copy, editable: true, knowsFocus: false)
        unreadable.inconclusive = true
        if case .plan(let plan) = CommandPlanner.decide(selection: unreadable, target: target, session: nil, lastDictation: nil,
                                                        reselect: { _ in nil }) {
            check("command: an unreadable selection is copy only", plan.target == .copy)
        } else {
            check("command: an unreadable selection is copy only", false)
        }
        check("command: no selection writes", planned(decide(nil, knowsFocus: false))?.target == .write
            && planned(decide(nil, editable: false))?.target == .copy)
        let session = CommandSession(target: target, original: "hey can u send it", startedAs: .selection)
        session.instructions = ["make it formal"]
        session.current = "Could you please send it?"
        let followUp = planned(decide("Could you  please send it?", session: session))
        let payload = followUp?.payload
        check("command: a follow-up continues the session", followUp?.isFollowUp == true && followUp?.session === session
            && payload?["original"] as? String == "hey can u send it"
            && payload?["current"] as? String == "Could you please send it?"
            && (payload?["turns"] as? [[String: String]])?.first?["instruction"] == "make it formal")
        return lines
    }
}
