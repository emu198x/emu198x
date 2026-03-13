// Fullscreen textured quad — renders the emulator framebuffer.
//
// Uses a fullscreen triangle (3 vertices, no vertex buffer) with UV
// coordinates that cover the [0,1] range. The fragment shader samples
// the framebuffer texture with nearest-neighbour filtering for sharp
// pixels.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Fullscreen triangle: 3 vertices cover the entire clip space.
    // Vertex 0: (-1, -1), Vertex 1: (3, -1), Vertex 2: (-1, 3)
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx & 2u) * 2 - 1);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // Map clip space to UV: x [-1,1] → [0,1], y [-1,1] → [1,0] (flip Y).
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var fb_texture: texture_2d<f32>;
@group(0) @binding(1) var fb_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(fb_texture, fb_sampler, in.uv);
}
