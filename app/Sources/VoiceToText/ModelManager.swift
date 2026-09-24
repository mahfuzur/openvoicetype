import CryptoKit
import Foundation

/// A model file the app can download: a Whisper speech model or S1-mini. URLs are pinned to a Hugging Face commit,
/// so the SHA-256 (Hugging Face's LFS ETag) always matches.
struct ModelFile: Identifiable, Equatable {
    let fileName: String
    let title: String
    let detail: String
    let bytes: Int64
    let sha256: String
    let url: URL
    let directory: URL

    var id: String { fileName }
    var path: URL { directory.appendingPathComponent(fileName) }
    /// A symlink to a copy elsewhere (older install.sh versions linked another app's model) counts as installed.
    var isInstalled: Bool { FileManager.default.fileExists(atPath: path.path) }
    var sizeLabel: String { ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file) }
}

enum ModelCatalog {
    private static let home = FileManager.default.homeDirectoryForCurrentUser
    static let whisperDirectory = home.appendingPathComponent(".local/share/whisper")
    static let s1Directory = home.appendingPathComponent(".local/share/s1-mini")
    private static let whisperRepo = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/"
    private static let s1Repo = "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/34add00a48a2e5d24e5a4ee5405a99620a3a240c/"

    static let defaultWhisper = whisperModel(
        "ggml-large-v3-turbo-q5_0.bin", "Compressed (recommended)",
        "large-v3-turbo, compressed. Same accuracy in our tests, less than half the memory.",
        574_041_195, "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2")
    static let fullWhisper = whisperModel(
        "ggml-large-v3-turbo.bin", "Full",
        "large-v3-turbo at full precision. A bigger download and about 1.7 GB of memory while loaded.",
        1_624_555_275, "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69")
    static let fastWhisper = whisperModel(
        "ggml-base.en.bin", "Fast",
        "base.en: small and quick, but noticeably less accurate. For older or low-memory Macs.",
        147_964_211, "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002")
    static let whisper = [defaultWhisper, fullWhisper, fastWhisper]

    static let s1Mini = ModelFile(
        fileName: "s1-mini-q4_k_m.gguf", title: "S1-mini by Superwhisper",
        detail: "Cleans up text on this Mac, with no internet. English only.",
        bytes: 484_219_808, sha256: "3b41ebe2502cbd03e811d5d16b022f5ab551eda58d62597d152f89535003c634",
        url: URL(string: s1Repo + "s1-mini-q4_k_m.gguf")!, directory: s1Directory)

    static func whisperModel(named fileName: String) -> ModelFile? { whisper.first { $0.fileName == fileName } }

    private static func whisperModel(_ fileName: String, _ title: String, _ detail: String,
                                     _ bytes: Int64, _ sha256: String) -> ModelFile {
        ModelFile(fileName: fileName, title: title, detail: detail, bytes: bytes, sha256: sha256,
                  url: URL(string: whisperRepo + fileName)!, directory: whisperDirectory)
    }
}

/// Downloads models in the background with progress, resumes after a dropped connection, and checks the SHA-256
/// before the file gets its final name, so a half-downloaded or corrupt model is never used.
final class ModelManager: NSObject, ObservableObject, URLSessionDownloadDelegate {
    static let shared = ModelManager()

    enum Download: Equatable {
        case running(fraction: Double)
        case verifying
        case failed(String)
    }

    /// Downloads in progress or failed, by file name. A finished download is removed from here.
    @Published private(set) var downloads: [String: Download] = [:]
    /// Bumped whenever a model is added or deleted, so views re-read `isInstalled`.
    @Published private(set) var revision = 0
    /// Called on main when a download finished and the file is in place.
    var onInstalled: ((ModelFile) -> Void)?

    private lazy var session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
    private var tasks: [Int: ModelFile] = [:]
    private var resumeData: [String: Data] = [:]
    private var retries: [String: Int] = [:]
    /// Cancelled while being checked: the check finishes in the background, but the file isn't installed.
    private var cancelled: Set<String> = []

    func isDownloading(_ model: ModelFile) -> Bool {
        if case .running = downloads[model.fileName] { return true }
        return downloads[model.fileName] == .verifying
    }

    /// Starts (or, after a failure, resumes) a download the user asked for.
    func download(_ model: ModelFile) {
        guard !model.isInstalled, !isDownloading(model) else { return }
        retries[model.fileName] = 0
        cancelled.remove(model.fileName)
        start(model)
    }

    func cancel(_ model: ModelFile) {
        for (id, file) in tasks where file == model {
            session.getAllTasks { all in all.first { $0.taskIdentifier == id }?.cancel() }
        }
        if downloads[model.fileName] == .verifying { cancelled.insert(model.fileName) }
        resumeData[model.fileName] = nil
        downloads[model.fileName] = nil
    }

    private func start(_ model: ModelFile) {
        guard !model.isInstalled, !tasks.values.contains(model) else { return }
        if case .running = downloads[model.fileName] {} else { downloads[model.fileName] = .running(fraction: 0) }
        let task = resumeData.removeValue(forKey: model.fileName).map { session.downloadTask(withResumeData: $0) }
            ?? session.downloadTask(with: model.url)
        tasks[task.taskIdentifier] = model
        task.resume()
    }

    /// Deletes the file (only the link, if it's a symlink to another app's copy).
    func delete(_ model: ModelFile) {
        try? FileManager.default.removeItem(at: model.path)
        revision += 1
    }

    // MARK: URLSessionDownloadDelegate (on main)

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let model = tasks[downloadTask.taskIdentifier] else { return }
        let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : model.bytes
        downloads[model.fileName] = .running(fraction: min(1, Double(totalBytesWritten) / Double(total)))
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        guard let model = tasks[downloadTask.taskIdentifier] else { return }
        // A resumed download ends with 206 (Partial Content) and the complete file, so any 2xx is fine.
        if let response = downloadTask.response as? HTTPURLResponse, !(200..<300).contains(response.statusCode) {
            downloads[model.fileName] = .failed("Download failed (HTTP \(response.statusCode))")
            return
        }
        // The temporary file is deleted when this method returns, so move it next to its final place first.
        let part = model.path.appendingPathExtension("part")
        do {
            try FileManager.default.createDirectory(at: model.directory, withIntermediateDirectories: true)
            try? FileManager.default.removeItem(at: part)
            try FileManager.default.moveItem(at: location, to: part)
        } catch {
            downloads[model.fileName] = .failed(error.localizedDescription)
            return
        }
        downloads[model.fileName] = .verifying
        DispatchQueue.global(qos: .utility).async {
            let ok = Self.sha256(of: part) == model.sha256
            DispatchQueue.main.async {
                if self.cancelled.remove(model.fileName) != nil {
                    try? FileManager.default.removeItem(at: part)
                    return
                }
                if ok, (try? FileManager.default.moveItem(at: part, to: model.path)) != nil {
                    // Downloads arrive owner-only (0600); match the other model files.
                    try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: model.path.path)
                    self.downloads[model.fileName] = nil
                    self.revision += 1
                    AppLog.write("MODEL installed \(model.fileName)")
                    self.onInstalled?(model)
                } else {
                    try? FileManager.default.removeItem(at: part)
                    self.downloads[model.fileName] = .failed(ok ? "Couldn't save the model" : "The download was corrupt. Try again.")
                }
            }
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let model = tasks.removeValue(forKey: task.taskIdentifier), let error else { return }
        let nsError = error as NSError
        if nsError.code == NSURLErrorCancelled && downloads[model.fileName] == nil { return }
        // A dropped connection: resume from where it stopped, a few times, before asking the user.
        if let data = nsError.userInfo[NSURLSessionDownloadTaskResumeData] as? Data {
            resumeData[model.fileName] = data
            let attempt = (retries[model.fileName] ?? 0) + 1
            retries[model.fileName] = attempt
            if attempt <= 3 {
                // The progress bar stays up while it waits to resume.
                DispatchQueue.main.asyncAfter(deadline: .now() + Double(attempt) * 2) {
                    if case .running = self.downloads[model.fileName] { self.start(model) }
                }
                return
            }
        }
        AppLog.write("MODEL download failed \(model.fileName): \(error.localizedDescription)")
        downloads[model.fileName] = .failed(error.localizedDescription)
    }

    private static func sha256(of url: URL) -> String? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        var hasher = SHA256()
        while let chunk = try? handle.read(upToCount: 8 << 20), !chunk.isEmpty {
            hasher.update(data: chunk)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
