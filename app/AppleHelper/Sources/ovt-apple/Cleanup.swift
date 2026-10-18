import Foundation
import FoundationModels

/// Transcript cleanup with Apple's on-device language model (Apple Intelligence).
enum Cleanup {
    /// The model as cleanup uses it: Apple's permissive setting for transforming the user's own text (a dictation about
    /// anything must come back cleaned up, not blocked). Allowed by Apple's acceptable use requirements: it's Apple's
    /// own option, not a way around the guardrails.
    static let model = SystemLanguageModel(useCase: .general, guardrails: .permissiveContentTransformations)

    /// Why the model can't be used, or nil when it can.
    static func unavailableReason() -> String? {
        switch model.availability {
        case .available:
            return nil
        case .unavailable(let reason):
            switch reason {
            case .deviceNotEligible: return "deviceNotEligible"
            case .appleIntelligenceNotEnabled: return "appleIntelligenceNotEnabled"
            case .modelNotReady: return "modelNotReady"
            @unknown default: return "unavailable"
            }
        }
    }

    static func run(_ options: Options) async {
        if let reason = unavailableReason() { fail("unavailable \(reason)", status: 2) }
        guard let path = options.instructionsFile,
              let instructions = try? String(contentsOfFile: path, encoding: .utf8) else {
            fail("error no instructions file", status: 2)
        }
        // A new session for each dictation, so no earlier transcript stays in its context. Prewarming while the
        // message is still on its way (dictate.sh may start this before the transcript exists) loads the model and
        // processes the instructions.
        let session = LanguageModelSession(model: model, instructions: instructions)
        session.prewarm()
        let message = String(decoding: FileHandle.standardInput.readDataToEndOfFile(), as: UTF8.self)
        guard !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { exit(0) }
        do {
            let response = try await session.respond(to: message, options: GenerationOptions(sampling: .greedy))
            let text = response.content
            if isRefusal(text, message: message) { fail("refusal", status: 2) }
            FileHandle.standardOutput.write(Data(text.utf8))
        } catch {
            fail(classify(error), status: 2)
        }
    }

    /// The failure kinds dictate.sh reads. Matched on the error's description as well as the macOS 26 type, so the
    /// macOS 27 `LanguageModelError` (which replaces it) is classified too without needing its SDK.
    static func classify(_ error: Error) -> String {
        if let error = error as? LanguageModelSession.GenerationError {
            switch error {
            case .guardrailViolation: return "guardrail"
            case .refusal: return "refusal"
            case .exceededContextWindowSize: return "context"
            case .rateLimited: return "rate-limited"
            case .assetsUnavailable: return "unavailable modelNotReady"
            default: break
            }
        }
        let description = String(describing: error)
        for (needle, kind) in [("guardrail", "guardrail"), ("refusal", "refusal"), ("context", "context"),
                               ("rateLimited", "rate-limited"), ("assetsUnavailable", "unavailable modelNotReady")]
        where description.localizedCaseInsensitiveContains(needle) {
            return kind
        }
        return "error \(description.replacingOccurrences(of: "\n", with: " ").prefix(200))"
    }

    /// With permissive guardrails a refusal comes back as text ("I'm sorry, but I can't…"). It counts as one when the
    /// answer opens like a refusal and the transcript itself doesn't.
    static func isRefusal(_ text: String, message: String) -> Bool {
        let openings = ["i'm sorry", "i am sorry", "sorry, but", "i apologize", "i can't assist", "i cannot assist",
                        "i can't help", "i cannot help", "i'm unable to", "i am unable to", "as an ai"]
        let answer = text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            .replacingOccurrences(of: "\u{2019}", with: "'")
        guard let opening = openings.first(where: { answer.hasPrefix($0) }) else { return false }
        let transcript = message.lowercased().replacingOccurrences(of: "\u{2019}", with: "'")
        return !transcript.contains(opening)
    }
}
