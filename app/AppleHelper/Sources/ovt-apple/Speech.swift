import AVFoundation
import Foundation
import Speech

enum TranscriberKind: String {
    /// `SpeechTranscriber`: the newer long-form model (macOS 26).
    case speech
    /// `DictationTranscriber`: the system dictation model; the only one that uses contextual strings (the vocabulary).
    case dictation
}

/// One transcriber module, with a way to collect its final text.
struct Transcriber {
    let module: any SpeechModule
    /// Reads the module's results until the analyzer finishes and returns the final text.
    let collect: @Sendable () async throws -> String

    init(_ kind: TranscriberKind, locale: Locale) {
        switch kind {
        case .speech:
            let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [],
                                                attributeOptions: [])
            module = transcriber
            collect = {
                var parts: [String] = []
                for try await result in transcriber.results where result.isFinal {
                    parts.append(String(result.text.characters))
                }
                return SpeechEngine.join(parts)
            }
        case .dictation:
            let transcriber = DictationTranscriber(locale: locale, contentHints: [], transcriptionOptions: [.punctuation],
                                                   reportingOptions: [], attributeOptions: [])
            module = transcriber
            collect = {
                var parts: [String] = []
                for try await result in transcriber.results where result.isFinal {
                    parts.append(String(result.text.characters))
                }
                return SpeechEngine.join(parts)
            }
        }
    }
}

enum SpeechEngine {
    /// The supported locale for a Whisper-style language setting: "en", "de"…, "auto" (the system language), or a
    /// full identifier ("en-GB"). In order: the system's own locale when it speaks that language and is supported (a
    /// British Mac gets en-GB); the region asked for; the language's main region (en → en-US); any of that language.
    /// (`supportedLocale(equivalentTo:)` returns an arbitrary variant for an unsupported region such as en-BD.)
    static func locale(for language: String, kind: TranscriberKind) async -> Locale? {
        let supported = kind == .speech ? await SpeechTranscriber.supportedLocales : await DictationTranscriber.supportedLocales
        func find(_ locale: Locale) -> Locale? {
            let tag = locale.identifier(.bcp47)
            return supported.first { $0.identifier(.bcp47) == tag }
        }
        let current = Locale.current
        let wanted = language.isEmpty || language == "auto" ? current : Locale(identifier: language)
        guard let code = wanted.language.languageCode?.identifier else { return nil }
        if current.language.languageCode?.identifier == code, wanted.region == nil || wanted.region == current.region,
           let match = find(current) {
            return match
        }
        if wanted.region != nil, let match = find(wanted) { return match }
        if let region = Locale.Language(identifier: code).maximalIdentifier.split(separator: "-").last,
           let match = find(Locale(identifier: "\(code)-\(region)")) {
            return match
        }
        return supported.filter { $0.language.languageCode?.identifier == code }
            .min { $0.identifier(.bcp47) < $1.identifier(.bcp47) }
    }

    static func transcriber(_ options: Options) async throws -> Transcriber {
        guard SpeechTranscriber.isAvailable else { throw HelperError("speech recognition isn't available on this Mac") }
        guard let locale = await locale(for: options.locale, kind: options.transcriber) else {
            throw HelperError("no speech model for \(options.locale)")
        }
        return Transcriber(options.transcriber, locale: locale)
    }

    /// "installed", "downloading", "supported" (can be downloaded) or "unsupported".
    static func assetStatus(_ transcriber: Transcriber) async -> String {
        switch await AssetInventory.status(forModules: [transcriber.module]) {
        case .installed: "installed"
        case .downloading: "downloading"
        case .supported: "supported"
        default: "unsupported"
        }
    }

    static func install(_ options: Options) async throws {
        let transcriber = try await transcriber(options)
        guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber.module]) else {
            return // already installed
        }
        let observation = request.progress.observe(\.fractionCompleted) { progress, _ in
            printLine(String(format: "progress %.2f", progress.fractionCompleted))
        }
        defer { observation.invalidate() }
        try await request.downloadAndInstall()
    }

    static func transcribe(_ options: Options) async throws -> String {
        let transcriber = try await transcriber(options)
        guard await assetStatus(transcriber) == "installed" else { throw HelperError("the speech model isn't installed") }
        let analyzer = SpeechAnalyzer(modules: [transcriber.module])
        if !options.vocabulary.isEmpty {
            // Apple documents contextual strings for the dictation transcriber: short phrases, at most 100.
            let context = AnalysisContext()
            context.contextualStrings[.general] = Array(options.vocabulary.prefix(100))
            try await analyzer.setContext(context)
        }
        let reader = Task { try await transcriber.collect() }
        if options.pcm {
            try await streamStandardInput(to: analyzer, transcriber: transcriber, statsFile: options.statsFile)
        } else {
            guard let path = options.file else { throw HelperError("no WAV file given") }
            let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
            if let end = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: end)
            } else {
                await analyzer.cancelAndFinishNow()
            }
        }
        return try await reader.value
    }

    /// Live: converts the app's 16 kHz mono Int16 PCM to the analyzer's format as it arrives, and finalizes at end of
    /// input. Stdin is read from the start on its own thread (the stream buffers), so the app's writes never wait for
    /// the analyzer.
    private static func streamStandardInput(to analyzer: SpeechAnalyzer, transcriber: Transcriber,
                                            statsFile: String?) async throws {
        guard let target = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber.module]) else {
            throw HelperError("no audio format for the speech model")
        }
        let source = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16000, channels: 1, interleaved: true)!
        let converter = try PCMConverter(from: source, to: target)
        let (inputs, continuation) = AsyncStream.makeStream(of: AnalyzerInput.self)
        let reading = StdinReader(converter: converter, continuation: continuation)
        reading.start()

        try await analyzer.prepareToAnalyze(in: target)
        try await analyzer.start(inputSequence: inputs)
        let endOfInput = await reading.finished()
        try await analyzer.finalizeAndFinishThroughEndOfInput()
        if let statsFile {
            let milliseconds = Int(Date().timeIntervalSince(endOfInput.date) * 1000)
            let line = String(format: "%.2f %d\n", Double(endOfInput.bytes) / 32000, milliseconds)
            try? line.write(toFile: statsFile, atomically: false, encoding: .utf8)
        }
    }

    /// Final results come in segments: joined with single spaces.
    static func join(_ parts: [String]) -> String {
        parts.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }.joined(separator: " ")
    }
}

/// Converts Int16 chunks to the analyzer's format (sample rate and sample type), keeping state across chunks.
final class PCMConverter: @unchecked Sendable {
    private let converter: AVAudioConverter
    private let source: AVAudioFormat
    private let target: AVAudioFormat

    init(from source: AVAudioFormat, to target: AVAudioFormat) throws {
        guard let converter = AVAudioConverter(from: source, to: target) else {
            throw HelperError("can't convert audio to \(target)")
        }
        converter.primeMethod = .none // no priming frames, so timestamps don't drift
        self.converter = converter
        self.source = source
        self.target = target
    }

    func convert(_ data: Data) -> AVAudioPCMBuffer? {
        let frames = AVAudioFrameCount(data.count / 2)
        guard frames > 0, let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: frames),
              let samples = input.int16ChannelData else { return nil }
        input.frameLength = frames
        data.withUnsafeBytes { bytes in
            _ = memcpy(samples[0], bytes.baseAddress!, Int(frames) * 2)
        }
        if source == target { return input }
        let capacity = AVAudioFrameCount(Double(frames) * target.sampleRate / source.sampleRate) + 32
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return nil }
        var consumed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return input
        }
        return error == nil && output.frameLength > 0 ? output : nil
    }
}

/// Reads stdin on a thread, feeding the analyzer's input stream; `finished()` returns when stdin closes.
final class StdinReader: @unchecked Sendable {
    struct EndOfInput {
        let bytes: Int
        let date: Date
    }

    private let converter: PCMConverter
    private let continuation: AsyncStream<AnalyzerInput>.Continuation
    private var end: CheckedContinuation<EndOfInput, Never>?
    private var result: EndOfInput?
    private let lock = NSLock()

    init(converter: PCMConverter, continuation: AsyncStream<AnalyzerInput>.Continuation) {
        self.converter = converter
        self.continuation = continuation
    }

    func start() {
        let thread = Thread { [self] in
            var total = 0
            var pending = Data()
            let chunk = 3200 // 0.1 s
            while true {
                let data = FileHandle.standardInput.availableData
                if data.isEmpty { break }
                total += data.count
                pending.append(data)
                let usable = pending.count - pending.count % chunk
                guard usable > 0 else { continue }
                if let buffer = converter.convert(pending.prefix(usable)) {
                    continuation.yield(AnalyzerInput(buffer: buffer))
                }
                pending.removeFirst(usable)
            }
            let even = pending.count - pending.count % 2
            if even > 0, let buffer = converter.convert(pending.prefix(even)) {
                continuation.yield(AnalyzerInput(buffer: buffer))
            }
            continuation.finish()
            finish(EndOfInput(bytes: total, date: Date()))
        }
        thread.start()
    }

    func finished() async -> EndOfInput {
        await withCheckedContinuation { continuation in
            lock.lock()
            if let result {
                lock.unlock()
                continuation.resume(returning: result)
            } else {
                end = continuation
                lock.unlock()
            }
        }
    }

    private func finish(_ value: EndOfInput) {
        lock.lock()
        result = value
        let waiting = end
        end = nil
        lock.unlock()
        waiting?.resume(returning: value)
    }
}
