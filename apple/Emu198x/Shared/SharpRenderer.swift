import Emu198xKit
import MetalKit

/// Draws the machine's newest frame at its true pixel aspect with every
/// pixel the same width.
///
/// Nearest-neighbour sampling at a fractional scale makes some columns one
/// screen pixel wider than others, which shows as characters that look
/// slightly scaled. Sharp bilinear gives each source pixel a solid block of
/// screen pixels and blends only across the fractional edge between two
/// blocks. The display link decides when a frame is shown, never how many
/// frames run.
@MainActor
final class SharpRenderer: NSObject, MTKViewDelegate {
    private var frames: FrameSlot
    private var pixelAspect: Float
    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState
    private let sampler: MTLSamplerState
    private var texture: MTLTexture?
    private var sequence = 0

    init?(view: MTKView, frames: FrameSlot, pixelAspect: Float) {
        guard let device = view.device,
              let queue = device.makeCommandQueue(),
              let library = try? device.makeLibrary(source: Self.shader, options: nil)
        else { return nil }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "screen_vertex")
        descriptor.fragmentFunction = library.makeFunction(name: "screen_fragment")
        descriptor.colorAttachments[0].pixelFormat = view.colorPixelFormat
        let samplerDescriptor = MTLSamplerDescriptor()
        samplerDescriptor.minFilter = .linear
        samplerDescriptor.magFilter = .linear
        samplerDescriptor.sAddressMode = .clampToEdge
        samplerDescriptor.tAddressMode = .clampToEdge
        guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor),
              let sampler = device.makeSamplerState(descriptor: samplerDescriptor)
        else { return nil }
        self.frames = frames
        self.pixelAspect = pixelAspect
        self.queue = queue
        self.pipeline = pipeline
        self.sampler = sampler
        super.init()
        view.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
    }

    /// Switches to another machine's output.
    func show(_ frames: FrameSlot, pixelAspect: Float) {
        guard frames !== self.frames || pixelAspect != self.pixelAspect else { return }
        self.frames = frames
        self.pixelAspect = pixelAspect
        sequence = 0
        texture = nil
    }

    nonisolated func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    nonisolated func draw(in view: MTKView) {
        MainActor.assumeIsolated { render(in: view) }
    }

    private func render(in view: MTKView) {
        if let (frame, sequence) = frames.frame(after: sequence) {
            self.sequence = sequence
            upload(frame, device: view.device)
        }
        guard let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let commands = queue.makeCommandBuffer(),
              let encoder = commands.makeRenderCommandEncoder(descriptor: pass)
        else { return }
        if let texture {
            let source = SIMD2(Float(texture.width), Float(texture.height))
            let area = fit(source, in: view.drawableSize)
            encoder.setViewport(area)
            var uniforms = SIMD4(source.x, source.y,
                                 Float(area.width) / source.x, Float(area.height) / source.y)
            encoder.setRenderPipelineState(pipeline)
            encoder.setFragmentTexture(texture, index: 0)
            encoder.setFragmentSamplerState(sampler, index: 0)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout.size(ofValue: uniforms), index: 0)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
        encoder.endEncoding()
        commands.present(drawable)
        commands.commit()
    }

    /// The largest whole-pixel rectangle with the frame's displayed shape,
    /// centred in the drawable.
    private func fit(_ source: SIMD2<Float>, in drawable: CGSize) -> MTLViewport {
        let shown = SIMD2(Double(source.x * pixelAspect), Double(source.y))
        let scale = min(drawable.width / shown.x, drawable.height / shown.y)
        let width = (shown.x * scale).rounded(.down)
        let height = (shown.y * scale).rounded(.down)
        return MTLViewport(
            originX: ((drawable.width - width) / 2).rounded(.down),
            originY: ((drawable.height - height) / 2).rounded(.down),
            width: width, height: height, znear: 0, zfar: 1)
    }

    private func upload(_ frame: Frame, device: MTLDevice?) {
        guard frame.width > 0, frame.height > 0,
              frame.pixels.count == frame.width * frame.height * 4
        else { return }
        if texture?.width != frame.width || texture?.height != frame.height {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(
                pixelFormat: .rgba8Unorm, width: frame.width, height: frame.height,
                mipmapped: false)
            texture = device?.makeTexture(descriptor: descriptor)
        }
        frame.pixels.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return }
            texture?.replace(
                region: MTLRegionMake2D(0, 0, frame.width, frame.height),
                mipmapLevel: 0, withBytes: base, bytesPerRow: frame.width * 4)
        }
    }

    /// `scale` is screen pixels per source pixel on each axis. Within a
    /// source pixel the sample stays on its centre except in the last
    /// fraction of a screen pixel at each edge, where it slides linearly to
    /// the neighbour. Below 1× it falls back to ordinary bilinear.
    private static let shader = """
    #include <metal_stdlib>
    using namespace metal;

    struct Corner {
        float4 position [[position]];
        float2 uv;
    };

    vertex Corner screen_vertex(uint id [[vertex_id]]) {
        float2 corner = float2(id & 1, id >> 1);
        return { float4(corner.x * 2 - 1, 1 - corner.y * 2, 0, 1), corner };
    }

    struct Uniforms {
        float2 source_size;
        float2 scale;
    };

    fragment float4 screen_fragment(Corner in [[stage_in]],
                                    texture2d<float> frame [[texture(0)]],
                                    sampler smooth [[sampler(0)]],
                                    constant Uniforms &u [[buffer(0)]]) {
        float2 scale = max(u.scale, 1.0);
        float2 texel = in.uv * u.source_size;
        float2 offset = fract(texel) - 0.5;
        float2 flat = 0.5 - 0.5 / scale;
        float2 blend = (offset - clamp(offset, -flat, flat)) * scale + 0.5;
        float2 uv = (floor(texel) + blend) / u.source_size;
        return float4(frame.sample(smooth, uv).rgb, 1);
    }
    """
}
