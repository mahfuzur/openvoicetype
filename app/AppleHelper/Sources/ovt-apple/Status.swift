import Foundation
import Speech

/// What's available on this Mac, as one JSON line for dictate.sh and the app.
enum Status {
    static func json(_ options: Options) async -> String {
        var speech: [String: Any] = ["available": false, "locale": "", "assets": "unsupported"]
        if let transcriber = try? await SpeechEngine.transcriber(options), let locale = await SpeechEngine.locale(for: options.locale) {
            speech = ["available": true, "locale": locale.identifier, "assets": await SpeechEngine.assetStatus(transcriber)]
        }
        let reason = Cleanup.unavailableReason()
        let llm: [String: Any] = ["available": reason == nil, "reason": reason ?? ""]
        let object: [String: Any] = ["speech": speech, "llm": llm]
        let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
        return String(decoding: data, as: UTF8.self)
    }
}
