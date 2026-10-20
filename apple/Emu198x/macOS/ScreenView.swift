import AppKit
import Emu198xKit
import SwiftUI

/// The machine's picture, scaled by whole-pixel sampling to fit the window
/// at its own aspect, and the keyboard focus for typing into the machine.
struct ScreenView: NSViewRepresentable {
    let loop: EmulatorLoop
    let session: SpectrumSession

    func makeNSView(context: Context) -> ScreenNSView {
        ScreenNSView(loop: loop, session: session)
    }

    func updateNSView(_ view: ScreenNSView, context: Context) {
        view.loop = loop
    }
}

final class ScreenNSView: NSView {
    var loop: EmulatorLoop {
        didSet { sequence = 0 }
    }
    private weak var session: SpectrumSession?
    private var sequence = 0
    private var displayLink: CADisplayLink?
    private var heldModifiers: Set<String> = []

    init(loop: EmulatorLoop, session: SpectrumSession) {
        self.loop = loop
        self.session = session
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        layer?.contentsGravity = .resizeAspect
        layer?.magnificationFilter = .nearest
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used from a nib") }

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        displayLink?.invalidate()
        displayLink = nil
        NotificationCenter.default.removeObserver(self)
        guard let window else { return }
        window.makeFirstResponder(self)
        // The display only decides when a finished frame is shown; the loop's
        // own clock decides how many frames run.
        let link = displayLink(target: self, selector: #selector(refresh))
        link.add(to: .main, forMode: .common)
        displayLink = link
        NotificationCenter.default.addObserver(
            self, selector: #selector(windowLostFocus),
            name: NSWindow.didResignKeyNotification, object: window)
    }

    @objc private func refresh() {
        guard let (frame, sequence) = loop.frames.frame(after: sequence) else { return }
        self.sequence = sequence
        layer?.contents = Self.image(from: frame)
    }

    @objc private func windowLostFocus() {
        heldModifiers.removeAll()
        session?.releaseAll()
    }

    private static func image(from frame: Frame) -> CGImage? {
        guard frame.pixels.count == frame.width * frame.height * 4,
              let provider = CGDataProvider(data: Data(frame.pixels) as CFData)
        else { return nil }
        return CGImage(
            width: frame.width, height: frame.height,
            bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: frame.width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    override func keyDown(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command) else { return super.keyDown(with: event) }
        guard !event.isARepeat, let code = MacKeyCodes.browserCode(for: event.keyCode) else { return }
        session?.key(code: code, pressed: true)
    }

    override func keyUp(with event: NSEvent) {
        guard let code = MacKeyCodes.browserCode(for: event.keyCode) else { return }
        session?.key(code: code, pressed: false)
    }

    override func flagsChanged(with event: NSEvent) {
        for (code, isDown) in MacKeyCodes.modifiers(in: event.modifierFlags) where isDown != heldModifiers.contains(code) {
            if isDown { heldModifiers.insert(code) } else { heldModifiers.remove(code) }
            session?.key(code: code, pressed: isDown)
        }
    }
}
