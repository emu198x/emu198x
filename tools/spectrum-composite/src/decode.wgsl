// The CPU experiment remains the numerical oracle. Same pin table, FIR taps,
// ideal carrier, PAL alternation, pixel-centre interpolation and delay line.
// Only scheduling and floating-point precision change here.
struct Params { spp: u32, samples: u32, delay_line: u32, padding: u32 }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> indices: array<u32>;
@group(0) @binding(2) var<storage, read> palette: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> carrier: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> references: array<vec2<f32>>;
@group(0) @binding(5) var<storage, read> luma_taps: array<f32>;
@group(0) @binding(6) var<storage, read> chroma_taps: array<f32>;
@group(0) @binding(7) var<storage, read_write> signal: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read_write> mixed: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> decoded: array<vec4<f32>>;
@group(0) @binding(10) var picture: texture_storage_2d<rgba8unorm, write>;

fn polarity(row: u32) -> f32 { return select(1.0, -1.0, (row & 1u) != 0u); }
fn pixel(x: u32) -> u32 {
    if x < 36u { return x + 412u; }
    return x - 36u;
}
fn index_at(x: u32, row: u32) -> u32 {
    let position = row * 352u + x;
    return (indices[position / 4u] >> ((position & 3u) * 8u)) & 255u;
}

@compute @workgroup_size(64)
fn encode(@builtin(global_invocation_id) id: vec3<u32>) {
    let sample = id.x;
    let row = id.y;
    if sample >= params.samples || row >= 296u { return; }
    let p = sample / params.spp;
    var c = vec3<f32>(0.0);
    if p < 316u { c = palette[index_at(p + 36u, row)].xyz; }
    if p >= 412u { c = palette[index_at(p - 412u, row)].xyz; }
    if p >= 340u && p < 372u { c = vec3<f32>(-0.3 / 0.7, 0.0, 0.0); }
    if p >= 380u && p < 396u { c = vec3<f32>(0.0, -0.2, 0.2); }
    // Fixed within-line oscillator, rotated by a small per-line reference.
    // CPU computes those references in f64 before reducing to f32. Large
    // running timestamps never enter a GPU sin/cos or phase accumulator.
    let a = carrier[sample];
    let b = references[row];
    let cos_phase = a.x * b.x - a.y * b.y;
    let sin_phase = a.x * b.y + a.y * b.x;
    signal[row * params.samples + sample] = vec4<f32>(
        c.x + c.y * cos_phase + polarity(row) * c.z * sin_phase,
        cos_phase, sin_phase, 0.0);
}

@compute @workgroup_size(64)
fn separate(@builtin(global_invocation_id) id: vec3<u32>) {
    let sample = id.x;
    let row = id.y;
    if sample >= params.samples || row >= 296u { return; }
    let offset = row * params.samples;
    let radius = params.spp * 8u;
    var y = 0.0;
    for (var tap = 0u; tap <= radius * 2u; tap++) {
        let x = u32(clamp(i32(sample) + i32(tap) - i32(radius), 0, i32(params.samples) - 1));
        y += signal[offset + x].x * luma_taps[tap];
    }
    let source = signal[offset + sample];
    let chroma = 2.0 * (source.x - y);
    mixed[offset + sample] = vec4<f32>(y, chroma * source.y, chroma * source.z * polarity(row), 0.0);
}

@compute @workgroup_size(64)
fn demodulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x;
    let row = id.y;
    if x >= 352u || row >= 296u { return; }
    let sample = pixel(x) * params.spp + params.spp / 2u;
    let offset = row * params.samples;
    let radius = params.spp * 12u;
    var uv = vec2<f32>(0.0);
    for (var tap = 0u; tap <= radius * 2u; tap++) {
        let a = u32(clamp(i32(sample) - 1 + i32(tap) - i32(radius), 0, i32(params.samples) - 1));
        let b = u32(clamp(i32(sample) + i32(tap) - i32(radius), 0, i32(params.samples) - 1));
        uv += 0.5 * (mixed[offset + a].yz + mixed[offset + b].yz) * chroma_taps[tap];
    }
    let y = 0.5 * (mixed[offset + sample - 1u].x + mixed[offset + sample].x);
    decoded[row * 352u + x] = vec4<f32>(y, uv, 0.0);
}

@compute @workgroup_size(64)
fn display(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x;
    let row = id.y;
    if x >= 352u || row >= 296u { return; }
    var c = decoded[row * 352u + x].xyz;
    if params.delay_line != 0u && row > 0u {
        c.y = 0.5 * (c.y + decoded[(row - 1u) * 352u + x].y);
        c.z = 0.5 * (c.z + decoded[(row - 1u) * 352u + x].z);
    }
    let rgb = vec3<f32>(c.x + c.z / 0.877,
        c.x - 0.114 / (0.587 * 0.493) * c.y - 0.299 / (0.587 * 0.877) * c.z,
        c.x + c.y / 0.493);
    let quantised = round(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0) / 255.0;
    textureStore(picture, vec2<u32>(x, row), vec4<f32>(quantised, 1.0));
}
