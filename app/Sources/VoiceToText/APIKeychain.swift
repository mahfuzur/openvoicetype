import Foundation
import Security

/// API keys for the OpenAI-compatible cleanup endpoint, in the login Keychain, one per host (an OpenAI key and a
/// Groq key can both be kept). Only this app can read them without asking. They reach `dictate.sh` as a private file
/// that exists for one call (see `Dictation`), never through a command line or a setting.
enum APIKeychain {
    private static let service = "OpenVoiceType cleanup API key"

    static func key(for baseURL: String) -> String? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
                                    kSecAttrAccount as String: account(baseURL), kSecReturnData as String: true,
                                    kSecMatchLimit as String: kSecMatchLimitOne]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data else { return nil }
        let key = String(decoding: data, as: UTF8.self)
        return key.isEmpty ? nil : key
    }

    /// Saves the key for this endpoint's host, or removes it when `key` is empty. Returns false if the Keychain refused.
    @discardableResult
    static func setKey(_ key: String, for baseURL: String) -> Bool {
        let match: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
                                    kSecAttrAccount as String: account(baseURL)]
        SecItemDelete(match as CFDictionary)
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        var add = match
        add[kSecValueData as String] = Data(trimmed.utf8)
        add[kSecAttrLabel as String] = "OpenVoiceType: API key for \(account(baseURL))"
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    /// The host (and port) of the base URL: the account name the key is stored under.
    static func account(_ baseURL: String) -> String {
        guard let url = URL(string: baseURL.trimmingCharacters(in: .whitespaces)), let host = url.host else { return baseURL }
        return url.port.map { "\(host):\($0)" } ?? host
    }
}
