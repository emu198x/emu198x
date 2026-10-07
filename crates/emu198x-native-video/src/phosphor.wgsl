// Source-space fast recharge and exponential afterglow, in linear RGB.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var history: texture_2d<f32>;
struct Parameters { retention: f32, parity: f32, _pad0: f32, _pad1: f32, };
@group(0) @binding(2) var<uniform> parameters: Parameters;
@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex << 1u) & 2u);
    let y = f32(vertex & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let previous = textureLoad(history, pixel, 0).rgb * parameters.retention;
    var light = previous;
    if parameters.parity < 0.0 || f32(pixel.y % 2) == parameters.parity {
        let drive = textureLoad(source, pixel, 0).rgb;
        light = max(previous, drive * drive);
    }
    return vec4<f32>(light, 1.0);
}
