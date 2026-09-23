import AVFoundation
import CoreAudio
import ObjCSupport

/// Records an input device to a 16 kHz mono 16-bit WAV (what whisper.cpp expects) and reports
/// a 0...1 input level for every buffer (~45 times a second).
///
/// Bluetooth headsets are the tricky case: their mic only works in headset (HFP) mode, and macOS
/// switches them into it only when an app opens the mic. During that switch the device reports no
/// format, and once it completes the engine's configuration changes. So starting retries for ~2 s,
/// and a configuration change rebuilds the engine while writing on into the same file.
final class Recorder {
    enum RecorderError: LocalizedError {
        case noInputDevice
        case deviceNotReady(String)
        case noAudio(String)
        case unsupportedFormat

        var errorDescription: String? {
            switch self {
            case .noInputDevice: "No microphone found"
            case .deviceNotReady(let name): "\(name) didn't start. Try again or pick another mic"
            case .noAudio(let name): "No audio from \(name). Pick another mic in the menu"
            case .unsupportedFormat: "Microphone format not supported"
            }
        }
    }

    /// Called on the main queue.
    var onLevel: ((Float) -> Void)?
    private(set) var deviceName = "Microphone"

    private var engine: AVAudioEngine?
    private var configObserver: NSObjectProtocol?
    private var file: AVAudioFile?
    private var url: URL?
    private var deviceID: AudioDeviceID?
    private let fileLock = NSLock()
    /// Incremented on every start/stop so callbacks from an older session are ignored.
    private var session = 0
    private var receivedAudio = false
    private var tapFormat: AVAudioFormat?
    private var rebuilds = 0
    private let maxRebuilds = 5
    private var startedAt = Date()
    private var onReady: (() -> Void)?
    private var onFailure: ((Error) -> Void)?

    private let startAttempts = 10
    private let retryDelay: TimeInterval = 0.2
    private let noAudioTimeout: TimeInterval = 2.5

    /// Starts recording. `onReady` fires on the first audio buffer (the moment to start speaking);
    /// `onFailure` fires if the device never delivers audio. Both are called on the main queue.
    /// `deviceUID` nil means the system default input.
    func start(to url: URL, deviceUID: String?, onReady: @escaping () -> Void,
               onFailure: @escaping (Error) -> Void) {
        _ = stop()
        guard let device = deviceUID.flatMap(AudioDevices.device(uid:)) ?? AudioDevices.defaultInput() else {
            onFailure(RecorderError.noInputDevice)
            return
        }
        deviceName = device.name
        // Only pin a device the user picked; the system default is used as-is.
        deviceID = deviceUID == nil ? nil : device.id
        startedAt = Date()
        AppLog.write("MIC start device=\"\(device.name)\" bluetooth=\(device.isBluetooth) pinned=\(deviceUID != nil)")

        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: url)
        let fileSettings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: 16_000,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]
        do {
            setFile(try AVAudioFile(forWriting: url, settings: fileSettings,
                                    commonFormat: .pcmFormatFloat32, interleaved: false))
        } catch {
            onFailure(error)
            return
        }

        session += 1
        receivedAudio = false
        rebuilds = 0
        self.url = url
        self.onReady = onReady
        self.onFailure = onFailure
        attemptStart(session: session, attempt: 1)
    }

    /// Stops recording and returns the finished WAV, or nil if nothing was recording.
    func stop() -> URL? {
        guard let url else { return nil }
        session += 1
        stopEngine()
        setFile(nil) // releasing the AVAudioFile finalizes the WAV header
        self.url = nil
        onReady = nil
        onFailure = nil
        return url
    }

    func cancel() {
        if let url = stop() { try? FileManager.default.removeItem(at: url) }
    }

    // MARK: - Engine

    private func attemptStart(session: Int, attempt: Int) {
        guard session == self.session, url != nil else { return }
        do {
            try startEngine(session: session)
        } catch {
            stopEngine()
            guard attempt < startAttempts else {
                AppLog.write("MIC gave up after \(attempt) attempts: \(error.localizedDescription)")
                return fail(session, RecorderError.deviceNotReady(deviceName))
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + retryDelay) { [weak self] in
                self?.attemptStart(session: session, attempt: attempt + 1)
            }
            return
        }
        // Some Bluetooth mics start without error but never deliver audio.
        DispatchQueue.main.asyncAfter(deadline: .now() + noAudioTimeout) { [weak self] in
            guard let self, session == self.session, !self.receivedAudio else { return }
            AppLog.write("MIC no audio after \(self.noAudioTimeout)s")
            self.fail(session, RecorderError.noAudio(self.deviceName))
        }
    }

    private func startEngine(session: Int) throws {
        let engine = AVAudioEngine()
        if let deviceID {
            do {
                try engine.inputNode.auAudioUnit.setDeviceID(deviceID)
            } catch {
                throw RecorderError.deviceNotReady(deviceName)
            }
        }
        try run(engine, session: session)
        self.engine = engine

        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self, weak engine] _ in
            guard let self, let engine else { return }
            self.handleConfigurationChange(engine, session: session)
        }
    }

    /// Installs the tap for the input's current format and starts the engine.
    private func run(_ engine: AVAudioEngine, session: Int) throws {
        let input = engine.inputNode
        let hardwareFormat = input.inputFormat(forBus: 0)
        let inputFormat = input.outputFormat(forBus: 0)
        // While a Bluetooth headset is switching profile, the formats are zero or disagree, and
        // installTap raises an exception. Treat that as "not ready yet" and retry.
        guard hardwareFormat.sampleRate > 0, hardwareFormat.channelCount > 0,
              inputFormat.sampleRate > 0, inputFormat.channelCount > 0,
              hardwareFormat.sampleRate == inputFormat.sampleRate else {
            throw RecorderError.deviceNotReady(deviceName)
        }
        guard let targetFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000,
                                               channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: inputFormat, to: targetFormat) else {
            throw RecorderError.unsupportedFormat
        }

        let ratio = targetFormat.sampleRate / inputFormat.sampleRate
        let tap: AVAudioNodeTapBlock = { [weak self] buffer, _ in
            guard let self else { return }
            self.reportLevel(buffer, session: session)

            let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
            guard let converted = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return }
            var supplied = false
            var error: NSError?
            converter.convert(to: converted, error: &error) { _, status in
                if supplied {
                    status.pointee = .noDataNow
                    return nil
                }
                supplied = true
                status.pointee = .haveData
                return buffer
            }
            guard error == nil, converted.frameLength > 0 else { return }
            self.fileLock.lock()
            try? self.file?.write(from: converted)
            self.fileLock.unlock()
        }

        var startError: Error?
        do {
            try VTTObjC.catchException {
                input.removeTap(onBus: 0)
                input.installTap(onBus: 0, bufferSize: 1024, format: inputFormat, block: tap)
                engine.prepare()
                do { try engine.start() } catch { startError = error }
            }
        } catch {
            AppLog.write("MIC exception while starting: \(error.localizedDescription)")
            try? VTTObjC.catchException { input.removeTap(onBus: 0) }
            throw RecorderError.deviceNotReady(deviceName)
        }
        if let startError {
            try? VTTObjC.catchException { input.removeTap(onBus: 0) }
            throw startError
        }
        tapFormat = inputFormat
    }

    /// The input device was reconfigured: selecting a device triggers this once, and a Bluetooth
    /// headset switching into headset mode changes the format. Re-arm the same engine (re-selecting
    /// the device would trigger another change and loop); rebuild only if that fails.
    private func handleConfigurationChange(_ engine: AVAudioEngine, session: Int) {
        guard session == self.session, engine === self.engine else { return }
        let format = engine.inputNode.outputFormat(forBus: 0)
        AppLog.write("MIC configuration changed running=\(engine.isRunning) format=\(Int(format.sampleRate))Hz/\(format.channelCount)ch")
        if engine.isRunning, format == tapFormat { return }

        rebuilds += 1
        guard rebuilds <= maxRebuilds else {
            AppLog.write("MIC too many configuration changes, giving up")
            return fail(session, RecorderError.deviceNotReady(deviceName))
        }
        try? VTTObjC.catchException { engine.stop() }
        do {
            try run(engine, session: session)
        } catch {
            // The device is still switching: rebuild from scratch after a short pause.
            stopEngine()
            DispatchQueue.main.asyncAfter(deadline: .now() + retryDelay) { [weak self] in
                self?.attemptStart(session: session, attempt: 1)
            }
        }
    }

    private func stopEngine() {
        if let configObserver { NotificationCenter.default.removeObserver(configObserver) }
        configObserver = nil
        guard let engine else { return }
        try? VTTObjC.catchException {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
        }
        self.engine = nil
    }

    /// If audio was already captured, the file is kept so the caller can still `stop()` and use it
    /// (a mic that drops out mid-dictation must not lose what was said); otherwise it is discarded.
    private func fail(_ session: Int, _ error: Error) {
        guard session == self.session else { return }
        let onFailure = self.onFailure
        if receivedAudio {
            self.session += 1
            stopEngine()
        } else {
            cancel()
        }
        onFailure?(error)
    }

    private func setFile(_ newFile: AVAudioFile?) {
        fileLock.lock()
        file = newFile
        fileLock.unlock()
    }

    private func reportLevel(_ buffer: AVAudioPCMBuffer, session: Int) {
        guard let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return }
        let count = Int(buffer.frameLength)
        var sum: Float = 0
        for index in 0..<count { sum += samples[index] * samples[index] }
        let rms = (sum / Float(count)).squareRoot()
        let decibels = 20 * log10(max(rms, 1e-7))
        // -55 dB (room noise) -> 0, -10 dB (loud speech) -> 1
        let level = max(0, min(1, (decibels + 55) / 45))
        DispatchQueue.main.async { [weak self] in
            guard let self, session == self.session else { return }
            if !self.receivedAudio {
                self.receivedAudio = true
                AppLog.write(String(format: "MIC ready after %.2fs", Date().timeIntervalSince(self.startedAt)))
                self.onReady?()
            }
            self.onLevel?(level)
        }
    }
}
