import AppKit
import SwiftUI

// MARK: - Model

final class OverlayModel: ObservableObject {
    enum Phase: Equatable {
        case connecting(String)
        case micTest(String)
        case recording
        case transcribing
        case polishing(offline: Bool)
        case success(String)
        case message(String, isError: Bool)
    }

    static let barCount = 18

    @Published var phase: Phase = .recording
    @Published var levels: [CGFloat] = Array(repeating: 0, count: barCount)
    @Published var recordingStartedAt = Date()
    @Published var shakes: CGFloat = 0

    func push(level: Float) {
        levels.removeFirst()
        levels.append(CGFloat(level))
    }

    func resetLevels() {
        levels = Array(repeating: 0, count: OverlayModel.barCount)
    }
}

// MARK: - Panel controller

/// A click-through, non-activating floating pill that never steals focus from the app you dictate into.
final class OverlayController {
    enum Position: String { case bottom, top }

    let model = OverlayModel()
    var isEnabled = true
    var position: Position = .bottom

    private let panel: NSPanel
    private var hideWork: DispatchWorkItem?
    private let size = NSSize(width: 560, height: 72)

    init() {
        panel = NSPanel(contentRect: NSRect(origin: .zero, size: size),
                        styleMask: [.nonactivatingPanel, .borderless],
                        backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .statusBar
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.contentView = NSHostingView(rootView: OverlayView(model: model))
    }

    func showRecording() {
        model.resetLevels()
        model.recordingStartedAt = Date()
        show(.recording)
    }

    func showMicTest(deviceName: String) {
        model.resetLevels()
        show(.micTest(deviceName))
    }

    func show(_ phase: OverlayModel.Phase) {
        guard isEnabled else { return }
        hideWork?.cancel()
        model.phase = phase
        guard !panel.isVisible || panel.alphaValue < 1 else { return }
        place()
        if !panel.isVisible { panel.alphaValue = 0 }
        panel.orderFrontRegardless()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.18
            panel.animator().alphaValue = 1
        }
    }

    /// Shows a final state, then fades out.
    func finish(_ phase: OverlayModel.Phase, after delay: TimeInterval) {
        guard isEnabled else { return }
        show(phase)
        if case .message(_, true) = phase { model.shakes += 1 }
        let work = DispatchWorkItem { [weak self] in self?.hide() }
        hideWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    func hide() {
        hideWork?.cancel()
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.25
            panel.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self, self.panel.alphaValue == 0 else { return }
            self.panel.orderOut(nil)
        })
    }

    func push(level: Float) {
        guard isEnabled else { return }
        switch model.phase {
        case .recording, .micTest: model.push(level: level)
        default: break
        }
    }

    /// Centers the pill on the screen that has the mouse pointer.
    private func place() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }
        let x = frame.midX - size.width / 2
        let y = position == .bottom ? frame.minY + 28 : frame.maxY - size.height - 8
        panel.setFrame(NSRect(origin: NSPoint(x: x, y: y), size: size), display: false)
    }
}

// MARK: - Views

struct OverlayView: View {
    @ObservedObject var model: OverlayModel

    var body: some View {
        HStack(spacing: 10) {
            content
        }
        .font(.system(size: 13, weight: .medium))
        .foregroundColor(.white)
        .padding(.horizontal, 16)
        .frame(height: 40)
        .background(Capsule().fill(Color.black.opacity(0.85)))
        .overlay(Capsule().strokeBorder(Color.white.opacity(0.14), lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 12, y: 4)
        .modifier(Shake(animatableData: model.shakes))
        .animation(.linear(duration: 0.45), value: model.shakes)
        .animation(.spring(response: 0.35, dampingFraction: 0.78), value: model.phase)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .connecting(let device):
            ProcessingBars(color: Color(red: 0.35, green: 0.65, blue: 1.0))
            ShimmerText(text: "Connecting to \(device)")
                .lineLimit(1)
        case .micTest(let device):
            Image(systemName: "mic.fill")
                .foregroundColor(Color(red: 0.35, green: 0.65, blue: 1.0))
            Waveform(levels: model.levels)
            Text("Say something · \(device)")
                .lineLimit(1)
                .foregroundColor(.white.opacity(0.8))
        case .recording:
            PulsingDot()
            Waveform(levels: model.levels)
            ElapsedTime(since: model.recordingStartedAt)
        case .transcribing:
            ProcessingBars(color: Color(red: 1.0, green: 0.62, blue: 0.2))
            ShimmerText(text: "Transcribing")
        case .polishing(let offline):
            Image(systemName: "sparkles")
                .foregroundColor(Color(red: 0.75, green: 0.6, blue: 1.0))
            ShimmerText(text: offline ? "Polishing offline" : "Polishing")
        case .success(let text):
            Image(systemName: "checkmark.circle.fill")
                .foregroundColor(.green)
                .transition(.scale.combined(with: .opacity))
            Text(text)
        case .message(let text, let isError):
            Image(systemName: isError ? "exclamationmark.triangle.fill" : "mic.slash.fill")
                .foregroundColor(isError ? .red : .white.opacity(0.7))
            Text(text)
                .lineLimit(1)
        }
    }
}

private struct PulsingDot: View {
    @State private var pulsing = false

    var body: some View {
        ZStack {
            Circle()
                .fill(Color.red.opacity(0.35))
                .frame(width: 18, height: 18)
                .scaleEffect(pulsing ? 1.0 : 0.5)
                .opacity(pulsing ? 0 : 1)
            Circle()
                .fill(Color.red)
                .frame(width: 9, height: 9)
        }
        .frame(width: 18, height: 18)
        .onAppear {
            withAnimation(.easeOut(duration: 1.1).repeatForever(autoreverses: false)) { pulsing = true }
        }
    }
}

/// Recent input levels, newest on the right, mirrored around the centre line.
private struct Waveform: View {
    let levels: [CGFloat]

    var body: some View {
        HStack(alignment: .center, spacing: 3) {
            ForEach(levels.indices, id: \.self) { index in
                Capsule()
                    .fill(Color.white.opacity(0.55 + 0.45 * levels[index]))
                    .frame(width: 3, height: 3 + levels[index] * 21)
            }
        }
        .frame(height: 24)
        .animation(.linear(duration: 0.08), value: levels)
    }
}

private struct ElapsedTime: View {
    let since: Date

    var body: some View {
        TimelineView(.periodic(from: since, by: 1)) { context in
            let seconds = max(0, Int(context.date.timeIntervalSince(since)))
            Text(String(format: "%d:%02d", seconds / 60, seconds % 60))
                .font(.system(size: 12, weight: .medium).monospacedDigit())
                .foregroundColor(.white.opacity(0.7))
        }
    }
}

/// A travelling sine wave, shown while the app is working.
private struct ProcessingBars: View {
    let color: Color

    var body: some View {
        TimelineView(.animation) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 3) {
                ForEach(0..<5, id: \.self) { index in
                    let phase = sin(time * 7 - Double(index) * 0.7)
                    Capsule()
                        .fill(color)
                        .frame(width: 3, height: 5 + 11 * CGFloat((phase + 1) / 2))
                }
            }
            .frame(height: 18)
        }
    }
}

private struct ShimmerText: View {
    let text: String

    var body: some View {
        TimelineView(.animation) { context in
            let progress = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.5) / 1.5
            Text(text)
                .foregroundColor(.white.opacity(0.5))
                .overlay(
                    LinearGradient(colors: [.clear, .white, .clear],
                                   startPoint: UnitPoint(x: progress * 2 - 1, y: 0.5),
                                   endPoint: UnitPoint(x: progress * 2, y: 0.5))
                        .mask(Text(text))
                )
        }
    }
}

private struct Shake: GeometryEffect {
    var animatableData: CGFloat

    func effectValue(size: CGSize) -> ProjectionTransform {
        ProjectionTransform(CGAffineTransform(translationX: 7 * sin(animatableData * .pi * 6), y: 0))
    }
}
