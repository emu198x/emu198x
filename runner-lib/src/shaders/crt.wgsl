// CRT shader effect for ZX Spectrum emulator
// Simulates scanlines, curvature, vignette, and phosphor glow

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coord = fma(position, vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5));
    out.position = vec4<f32>(position, 0.0, 1.0);
    return out;
}

// Fragment shader bindings
@group(0) @binding(0) var r_tex_color: texture_2d<f32>;
@group(0) @binding(1) var r_tex_sampler: sampler;

struct Uniforms {
    screen_size: vec2<f32>,
    time: f32,
    _padding: f32,
}
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

// CRT curvature distortion
fn apply_curvature(uv: vec2<f32>, amount: f32) -> vec2<f32> {
    let centered = uv - 0.5;
    let dist = dot(centered, centered);
    let curved = centered * (1.0 + dist * amount);
    return curved + 0.5;
}

// Vignette darkening at edges
fn vignette(uv: vec2<f32>, intensity: f32) -> f32 {
    let centered = uv - 0.5;
    let dist = dot(centered, centered);
    return 1.0 - dist * intensity;
}

// Scanline effect
fn scanline(y: f32, intensity: f32) -> f32 {
    let line = sin(y * 3.14159265) * 0.5 + 0.5;
    return mix(1.0, line, intensity);
}

// Phosphor RGB mask (simulates shadow mask)
fn phosphor_mask(x: f32, intensity: f32) -> vec3<f32> {
    let phase = fract(x) * 3.0;
    var mask = vec3<f32>(1.0);
    if phase < 1.0 {
        mask = vec3<f32>(1.0, 1.0 - intensity * 0.5, 1.0 - intensity * 0.5);
    } else if phase < 2.0 {
        mask = vec3<f32>(1.0 - intensity * 0.5, 1.0, 1.0 - intensity * 0.5);
    } else {
        mask = vec3<f32>(1.0 - intensity * 0.5, 1.0 - intensity * 0.5, 1.0);
    }
    return mask;
}

@fragment
fn fs_main(@location(0) tex_coord: vec2<f32>) -> @location(0) vec4<f32> {
    // Apply barrel distortion for CRT curvature
    let curvature_amount = 0.08;
    let curved_uv = apply_curvature(tex_coord, curvature_amount);

    // Check if we're outside the curved screen area
    if curved_uv.x < 0.0 || curved_uv.x > 1.0 || curved_uv.y < 0.0 || curved_uv.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Sample the texture with slight chromatic aberration
    let aberration = 0.001;
    let r = textureSample(r_tex_color, r_tex_sampler, curved_uv + vec2<f32>(aberration, 0.0)).r;
    let g = textureSample(r_tex_color, r_tex_sampler, curved_uv).g;
    let b = textureSample(r_tex_color, r_tex_sampler, curved_uv - vec2<f32>(aberration, 0.0)).b;
    var color = vec3<f32>(r, g, b);

    // Apply scanlines based on screen Y position
    let scanline_count = uniforms.screen_size.y;
    let scanline_y = curved_uv.y * scanline_count;
    let scanline_intensity = 0.15;
    color *= scanline(scanline_y, scanline_intensity);

    // Apply phosphor mask based on screen X position
    let phosphor_x = curved_uv.x * uniforms.screen_size.x / 3.0;
    let phosphor_intensity = 0.1;
    color *= phosphor_mask(phosphor_x, phosphor_intensity);

    // Apply vignette
    let vignette_intensity = 0.4;
    color *= vignette(curved_uv, vignette_intensity);

    // Slight brightness boost to compensate for darkening effects
    color *= 1.15;

    // Add subtle glow/bloom by sampling neighbors (simple blur)
    let glow_amount = 0.02;
    let pixel_size = 1.0 / uniforms.screen_size;
    let glow = (
        textureSample(r_tex_color, r_tex_sampler, curved_uv + vec2<f32>(pixel_size.x, 0.0)).rgb +
        textureSample(r_tex_color, r_tex_sampler, curved_uv - vec2<f32>(pixel_size.x, 0.0)).rgb +
        textureSample(r_tex_color, r_tex_sampler, curved_uv + vec2<f32>(0.0, pixel_size.y)).rgb +
        textureSample(r_tex_color, r_tex_sampler, curved_uv - vec2<f32>(0.0, pixel_size.y)).rgb
    ) * 0.25;
    color = mix(color, color + glow * 0.3, glow_amount);

    return vec4<f32>(color, 1.0);
}
