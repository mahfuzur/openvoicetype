import AppKit

/// Checks GitHub Releases for a newer version, at most once a day. It sends nothing about the user: one anonymous
/// request to the public API. Updating is downloading the new DMG and replacing the app (Sparkle can come later).
final class Updater: ObservableObject {
    static let shared = Updater()
    static let repository = "mahfuzur/voice-to-text"

    struct Release: Equatable {
        let version: String
        let page: URL
    }

    /// A release newer than this app, if the last check found one.
    @Published private(set) var available: Release?
    @Published private(set) var lastChecked: Date? = UserDefaults.standard.object(forKey: "updateLastChecked") as? Date
    @Published private(set) var checking = false

    static var currentVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
    }

    /// Checks if it's been a day since the last check (and automatic checks are on).
    func checkIfDue() {
        guard AppSettings.shared.checkForUpdates else { return }
        if let lastChecked, Date().timeIntervalSince(lastChecked) < 24 * 3600 { return }
        check()
    }

    func check() {
        guard !checking,
              let url = URL(string: "https://api.github.com/repos/\(Self.repository)/releases/latest") else { return }
        checking = true
        var request = URLRequest(url: url, timeoutInterval: 15)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let json = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
            DispatchQueue.main.async {
                self.checking = false
                guard let json, let tag = json["tag_name"] as? String,
                      let page = (json["html_url"] as? String).flatMap(URL.init(string:)) else { return }
                self.lastChecked = Date()
                UserDefaults.standard.set(self.lastChecked, forKey: "updateLastChecked")
                let version = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
                self.available = Self.isNewer(version, than: Self.currentVersion) ? Release(version: version, page: page) : nil
            }
        }.resume()
    }

    func openReleasePage() {
        NSWorkspace.shared.open(available?.page ?? URL(string: "https://github.com/\(Self.repository)/releases")!)
    }

    /// Compares dotted version numbers ("0.10.0" is newer than "0.9.1").
    static func isNewer(_ candidate: String, than current: String) -> Bool {
        let parse = { (v: String) in v.split(separator: ".").map { Int($0.prefix { $0.isNumber }) ?? 0 } }
        let (a, b) = (parse(candidate), parse(current))
        for i in 0..<max(a.count, b.count) {
            let (x, y) = (i < a.count ? a[i] : 0, i < b.count ? b[i] : 0)
            if x != y { return x > y }
        }
        return false
    }
}
