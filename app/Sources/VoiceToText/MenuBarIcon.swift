import AppKit

/// The menu-bar icon: still waveform bars, like the app icon, plus a red dot while the app is working (recording,
/// transcribing or polishing). The overlay shows the details; the menu bar only says "busy or not".
///
/// Idle it's a template image, so macOS draws it white on a dark menu bar and black on a light one. A template can't
/// hold a red dot, so the busy image draws the bars itself in the menu bar's colour.
final class MenuBarIcon {
    private weak var button: NSStatusBarButton?
    private(set) var isActive = false

    init(button: NSStatusBarButton) {
        self.button = button
        button.imagePosition = .imageOnly
        render()
    }

    func setActive(_ active: Bool) {
        guard active != isActive else { return }
        isActive = active
        render()
    }

    private func render() {
        guard let button else { return }
        button.image = Self.image(active: isActive)
        button.setAccessibilityLabel(isActive ? "OpenVoiceType: working" : "OpenVoiceType")
    }

    /// 18 × 18 pt. Busy: the bars with a gap cut around a red dot at the bottom right.
    static func image(active: Bool) -> NSImage {
        let dot = NSRect(x: 11.2, y: 1, width: 6.8, height: 6.8)
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: false) { rect in
            // Drawn when the menu bar draws it, so the current appearance is the menu bar's.
            let dark = NSAppearance.currentDrawing().bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            (active ? (dark ? NSColor.white : NSColor.black) : NSColor.black).set()
            drawBars(in: rect)
            if active {
                NSGraphicsContext.current?.compositingOperation = .clear
                NSBezierPath(ovalIn: dot.insetBy(dx: -1.4, dy: -1.4)).fill()
                NSGraphicsContext.current?.compositingOperation = .sourceOver
                NSColor.systemRed.set()
                NSBezierPath(ovalIn: dot).fill()
            }
            return true
        }
        image.isTemplate = !active
        return image
    }

    private static func drawBars(in rect: NSRect) {
        let heights: [CGFloat] = [0.38, 0.7, 1.0, 0.7, 0.38]
        let barWidth: CGFloat = 2.2, gap: CGFloat = 1.3, maxHeight: CGFloat = 14
        let total = CGFloat(heights.count) * barWidth + CGFloat(heights.count - 1) * gap
        for (index, fraction) in heights.enumerated() {
            let height = maxHeight * fraction
            let bar = NSRect(x: rect.midX - total / 2 + CGFloat(index) * (barWidth + gap),
                             y: rect.midY - height / 2, width: barWidth, height: height)
            NSBezierPath(roundedRect: bar, xRadius: barWidth / 2, yRadius: barWidth / 2).fill()
        }
    }
}
