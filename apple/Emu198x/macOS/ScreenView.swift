import AppKit
import Emu198xKit
import MetalKit
import SwiftUI

/// The machine's picture, drawn by ``SharpRenderer``, and the keyboard
/// focus for typing into the machine.
struct ScreenView: NSViewRepresentable {
    let loop: EmulatorLoop
    let pixelAspect: Float
    let session: SpectrumSession

    func makeNSView(context: Context) -> ScreenNSView {
        ScreenNSView(loop: loop, pixelAspect: pixelAspect, session: session)
    }

    func updateNSView(_ view: ScreenNSView, context: Context) {
        view.show(loop, pixelAspect: pixelAspect)
    }
}

final class ScreenNSView: MTKView {
    private weak var session: SpectrumSession?
    private var renderer: SharpRenderer?
    private var heldModifiers: Set<String> = []

    init(loop: EmulatorLoop, pixelAspect: Float, session: SpectrumSession) {
        self.session = session
        super.init(frame: .zero, device: MTLCreateSystemDefaultDevice())
        (layer as? CAMetalLayer)?.colorspace = CGColorSpace(name: CGColorSpace.sRGB)
        renderer = SharpRenderer(view: self, frames: loop.frames, pixelAspect: pixelAspect)
        delegate = renderer
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used from a nib") }

    func show(_ loop: EmulatorLoop, pixelAspect: Float) {
        renderer?.show(loop.frames, pixelAspect: pixelAspect)
    }

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        NotificationCenter.default.removeObserver(self)
        guard let window else { return }
        window.makeFirstResponder(self)
        NotificationCenter.default.addObserver(
            self, selector: #selector(windowLostFocus),
            name: NSWindow.didResignKeyNotification, object: window)
    }

    @objc private func windowLostFocus() {
        heldModifiers.removeAll()
        session?.releaseAll()
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
