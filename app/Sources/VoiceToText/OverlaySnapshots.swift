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
            ("3-polishing", .polishing(offline: false)),
            ("3-polishing-offline", .polishing(offline: true)),
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
        renderMenuBarIcons(to: directory.appendingPathComponent("7-menubar-icons.png"))
    }

    /// The menu-bar icon, idle and busy, on a dark and a light menu bar, drawn the way the menu bar draws them.
    private static func renderMenuBarIcons(to url: URL) {
        let cell: CGFloat = 36, scale: CGFloat = 4
        let size = NSSize(width: cell * 2, height: cell * 2)
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
                                   bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                   colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = size
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        let bars: [(NSAppearance.Name, NSColor, NSColor)] = [(.darkAqua, NSColor(white: 0.12, alpha: 1), .white),
                                                            (.aqua, NSColor(white: 0.93, alpha: 1), .black)]
        for (row, (name, background, foreground)) in bars.enumerated() {
            background.setFill()
            NSRect(x: 0, y: CGFloat(row) * cell, width: size.width, height: cell).fill()
            NSAppearance(named: name)?.performAsCurrentDrawingAppearance {
                for (column, active) in [false, true].enumerated() {
                    let icon = MenuBarIcon.image(active: active)
                    let target = NSRect(x: CGFloat(column) * cell + 9, y: CGFloat(row) * cell + 9, width: 18, height: 18)
                    guard icon.isTemplate else { icon.draw(in: target); continue }
                    NSImage(size: icon.size, flipped: false) { rect in // a template is tinted by the menu bar
                        icon.draw(in: rect)
                        foreground.set()
                        rect.fill(using: .sourceAtop)
                        return true
                    }.draw(in: target)
                }
            }
        }
        NSGraphicsContext.restoreGraphicsState()
        try? rep.representation(using: .png, properties: [:])?.write(to: url)
    }
}

/// Renders each Settings pane and the setup window to PNG files (`VoiceToText --settings-snapshots <dir>`).
/// Forms use AppKit controls, which `ImageRenderer` can't draw, so each view is shown in an offscreen window.
enum SettingsSnapshots {
    static func render(to directory: URL, actions: AppActions, lastResult: LastResult, done: @escaping () -> Void) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let views: [(String, AnyView, CGSize)] = [
            ("settings-1-general", AnyView(GeneralPane(actions: actions)), CGSize(width: 600, height: 640)),
            ("settings-2-speech", AnyView(SpeechPane()), CGSize(width: 600, height: 400)),
            ("settings-3-cleanup", AnyView(CleanupPane(actions: actions)), CGSize(width: 600, height: 620)),
            ("settings-4-dictionary", AnyView(DictionaryPane()), CGSize(width: 600, height: 540)),
            ("settings-5-modes", AnyView(ModesPane()), CGSize(width: 600, height: 500)),
            ("settings-6-about", AnyView(AboutPane(actions: actions)), CGSize(width: 600, height: 520)),
            ("setup", AnyView(SetupView(actions: actions, lastResult: lastResult, close: {})), CGSize(width: 620, height: 760)),
        ]
        var remaining = views[...]
        func next() {
            guard let (name, view, size) = remaining.popFirst() else { return done() }
            let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: -4000, y: 0), size: size),
                                  styleMask: [.borderless], backing: .buffered, defer: false)
            let host = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
            window.contentView = host
            window.orderFrontRegardless()
            // Let SwiftUI lay out and the Claude check finish before capturing.
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
                if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
                    host.cacheDisplay(in: host.bounds, to: rep)
                    try? rep.representation(using: .png, properties: [:])?
                        .write(to: directory.appendingPathComponent("\(name).png"))
                }
                window.orderOut(nil)
                next()
            }
        }
        next()
    }
}
