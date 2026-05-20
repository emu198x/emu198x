struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    switch vertex_index {
        case 0u: {
            out.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
            out.uv = vec2<f32>(0.0, 1.0);
        }
        case 1u: {
            out.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            out.uv = vec2<f32>(1.0, 1.0);
        }
        case 2u: {
            out.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            out.uv = vec2<f32>(0.0, 0.0);
        }
        case 3u: {
            out.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            out.uv = vec2<f32>(0.0, 0.0);
        }
        case 4u: {
            out.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            out.uv = vec2<f32>(1.0, 1.0);
        }
        default: {
            out.position = vec4<f32>(1.0, 1.0, 0.0, 1.0);
            out.uv = vec2<f32>(1.0, 0.0);
        }
    }

    return out;
}

@group(0) @binding(0)
var source_texture: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

struct PresentationUniforms {
    filter_mode: f32,
    frame_width: f32,
    frame_height: f32,
    _pad: f32,
};

@group(0) @binding(2)
var<uniform> presentation: PresentationUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv);

    if presentation.filter_mode < 0.5 {
        return color;
    }

    if presentation.filter_mode < 1.5 {
        return lcd_filter(in.uv, color);
    }

    return crt_filter(in.uv, color);
}

fn lcd_filter(uv: vec2<f32>, color: vec4<f32>) -> vec4<f32> {
    let luminance = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let dark = vec3<f32>(0.055, 0.110, 0.055);
    let light = vec3<f32>(0.690, 0.770, 0.430);
    var rgb = mix(dark, light, luminance);

    let source_pos = uv * vec2<f32>(presentation.frame_width, presentation.frame_height);
    let cell = fract(source_pos);
    let vertical_gap = select(1.0, 0.86, cell.x < 0.080);
    let horizontal_gap = select(1.0, 0.90, cell.y < 0.080);
    rgb *= vertical_gap * horizontal_gap;

    return vec4<f32>(rgb, color.a);
}

fn crt_filter(uv: vec2<f32>, color: vec4<f32>) -> vec4<f32> {
    let source_pos = uv * vec2<f32>(presentation.frame_width, presentation.frame_height);

    // Sample a 3×3 neighbourhood plus two further-out horizontal
    // taps. The wide horizontal sampling models the electron beam
    // spreading sideways as it scans (the dominant CRT blur axis);
    // the smaller vertical sampling captures phosphor persistence
    // and adjacent-line crosstalk. Weighted blend toward a softer
    // average gives the base blur, then a max-based pass adds the
    // glow on top.
    let hstep = vec2<f32>(1.0 / presentation.frame_width, 0.0);
    let vstep = vec2<f32>(0.0, 1.0 / presentation.frame_height);
    let left_1 = textureSample(source_texture, source_sampler, uv - hstep).rgb;
    let right_1 = textureSample(source_texture, source_sampler, uv + hstep).rgb;
    let left_2 = textureSample(source_texture, source_sampler, uv - 2.0 * hstep).rgb;
    let right_2 = textureSample(source_texture, source_sampler, uv + 2.0 * hstep).rgb;
    let up = textureSample(source_texture, source_sampler, uv - vstep).rgb;
    let down = textureSample(source_texture, source_sampler, uv + vstep).rgb;

    // Base soft blur — weighted average with the centre pixel
    // dominating, so pixel art stays legible while edges soften.
    // Horizontal weighting heavier than vertical, matching real
    // CRT beam-spread anisotropy. Kernel sums to ~0.98 (very mild
    // overall darkening; bloom restores brightness on bright
    // pixels).
    let soft = color.rgb * 0.60
        + (left_1 + right_1) * 0.10
        + (left_2 + right_2) * 0.03
        + (up + down) * 0.06;

    // Phosphor bloom — bright pixels lift their darker neighbours.
    // `max` (not weighted average) means bright-on-dark text glows
    // outward as on a real CRT, while dark-on-bright stays crisp.
    let glow = max(max(left_1, right_1), max(up, down)) * 0.22;

    var rgb = soft + glow;

    // Scanline modulation — soft sinusoidal dip across each
    // emulated pixel row.
    let scan_phase = fract(source_pos.y);
    let scanline = 1.0 - 0.10 * pow(sin(scan_phase * 3.141593), 2.0);
    rgb *= scanline;

    // Subtle vignette — ~11% corner darkening.
    let centre_offset = uv - vec2<f32>(0.5, 0.5);
    let vignette = 1.0 - 0.22 * dot(centre_offset, centre_offset);
    rgb *= vignette;

    // Bloom floor lifts pure black to a soft phosphor glow.
    let bloom_floor = vec3<f32>(0.012, 0.010, 0.008);
    rgb = min(rgb + bloom_floor, vec3<f32>(1.0, 1.0, 1.0));

    return vec4<f32>(rgb, color.a);
}
