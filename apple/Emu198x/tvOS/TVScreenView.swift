import Emu198xKit
import MetalKit
import SwiftUI

/// The machine's picture full screen, drawn by ``SharpRenderer``.
struct TVScreenView: UIViewRepresentable {
    let loop: EmulatorLoop
    let pixelAspect: Float

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> MTKView {
        let view = MTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        context.coordinator.renderer = SharpRenderer(
            view: view, frames: loop.frames, pixelAspect: pixelAspect)
        view.delegate = context.coordinator.renderer
        return view
    }

    func updateUIView(_ view: MTKView, context: Context) {
        context.coordinator.renderer?.show(loop.frames, pixelAspect: pixelAspect)
    }

    /// Holds the renderer, since an `MTKView` keeps its delegate weakly.
    final class Coordinator {
        var renderer: SharpRenderer?
    }
}
