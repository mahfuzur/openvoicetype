import AppKit
import SwiftUI

/// Renders every overlay state to PNG files (`VoiceToText --overlay-snapshots <dir>`),
/// for checking the design without a microphone or screen-recording permission.
enum OverlaySnapshots {
    @MainActor
    static func render(to directory: URL) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let phases: [(String, OverlayModel.Phase)] = [
            ("0-connecting", .connecting("AirPods Pro")),
            ("0-mic-test", .micTest("MacBook Pro Microphone")),
            ("1-recording", .recording),
            ("2-transcribing", .transcribing),
            ("3-polishing", .polishing),
            ("4-success", .success("Pasted")),
            ("5-no-speech", .message("No speech detected", isError: false)),
            ("6-error", .message("Transcription failed", isError: true)),
        ]
        for (name, phase) in phases {
            let model = OverlayModel()
            model.phase = phase
            model.recordingStartedAt = Date().addingTimeInterval(-7)
            model.levels = (0..<OverlayModel.barCount).map { CGFloat(abs(sin(Double($0) / 2.2))) * 0.9 }
            let view = OverlayView(model: model)
                .frame(width: 560, height: 72)
                .background(Color(white: 0.93))
            let renderer = ImageRenderer(content: view)
            renderer.scale = 2
            guard let image = renderer.nsImage, let tiff = image.tiffRepresentation,
                  let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { continue }
            try? png.write(to: directory.appendingPathComponent("\(name).png"))
        }
    }
}
