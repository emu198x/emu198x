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

    return crt_filter(in.uv, in.position.xy, color);
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

// ─── CRT-Lottes port ──────────────────────────────────────────────
// Port of Timothy Lottes' single-pass CRT shader. Reference:
//   http://timothylottes.blogspot.com/2014/05/
//   https://github.com/libretro/slang-shaders/blob/master/crt/
//     shaders/crt-lottes.slang
//
// The model approximates an electron-beam CRT: per-pixel beam spread
// via Gaussian filters on both axes, soft scanlines via vertical
// Gaussian, aperture-grille shadow mask, optional barrel distortion.
// All weights stay in linear-light space (gamma 2.0 approximation)
// so bright pixels mix correctly with their dark neighbours.

// Scanline hardness. Lottes default = -8.0; more negative = softer
// scanlines. The Spectrum scaled 3× looks better with softer than
// arcade-monitor defaults.
const LOTTES_HARD_SCAN: f32 = -10.0;
// Horizontal pixel hardness. Lottes default = -3.0; more negative
// = softer pixel edges.
const LOTTES_HARD_PIX: f32 = -3.0;
// Barrel distortion strength per axis. Slight curvature; full
// arcade-monitor warp would be ~0.031/0.041, halved here for a
// domestic-TV feel.
const LOTTES_WARP_X: f32 = 0.012;
const LOTTES_WARP_Y: f32 = 0.016;
// Aperture-grille mask weights.
const LOTTES_MASK_DARK: f32 = 0.5;
const LOTTES_MASK_LIGHT: f32 = 1.5;
// Brightness multiplier (compensates for mask + scanline darkening).
const LOTTES_BRIGHT_BOOST: f32 = 1.10;

fn lottes_source_size() -> vec2<f32> {
    return vec2<f32>(presentation.frame_width, presentation.frame_height);
}

// sRGB ↔ linear, Lottes' gamma-2.0 approximation. Faster than a full
// sRGB transfer-function lookup and visually indistinguishable at
// the Spectrum's 16-colour palette.
fn lottes_to_linear(c: vec3<f32>) -> vec3<f32> {
    return c * c;
}

fn lottes_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return sqrt(max(c, vec3<f32>(0.0, 0.0, 0.0)));
}

// Fetch a source pixel `off` pixels away (in source-pixel units).
// `pos` is the (warped) UV in [0,1].
fn lottes_fetch(pos: vec2<f32>, off: vec2<f32>) -> vec3<f32> {
    let size = lottes_source_size();
    let p = (floor(pos * size + off) + vec2<f32>(0.5, 0.5)) / size;
    if p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    return lottes_to_linear(textureSample(source_texture, source_sampler, p).rgb);
}

// Sign-flipped offset from the current source pixel's centre, in
// source-pixel units. Lottes convention: dist is in [-0.5, 0.5].
fn lottes_dist(pos: vec2<f32>) -> vec2<f32> {
    let p = pos * lottes_source_size();
    return -((p - floor(p)) - vec2<f32>(0.5, 0.5));
}

// Gaussian weight: exp2(scale × pos²) with negative scale.
fn lottes_gaus(pos: f32, scale: f32) -> f32 {
    return exp2(scale * pos * pos);
}

// 3-tap horizontal beam filter at vertical offset `off` (in source-
// pixel rows). Returns the blended RGB.
fn lottes_horz3(pos: vec2<f32>, off: f32) -> vec3<f32> {
    let b = lottes_fetch(pos, vec2<f32>(-1.0, off));
    let c = lottes_fetch(pos, vec2<f32>(0.0, off));
    let d = lottes_fetch(pos, vec2<f32>(1.0, off));
    let dst = lottes_dist(pos).x;
    let scale = LOTTES_HARD_PIX;
    let wb = lottes_gaus(dst - 1.0, scale);
    let wc = lottes_gaus(dst, scale);
    let wd = lottes_gaus(dst + 1.0, scale);
    return (b * wb + c * wc + d * wd) / (wb + wc + wd);
}

// Scanline weight at vertical offset `off`.
fn lottes_scan(pos: vec2<f32>, off: f32) -> f32 {
    let dst = lottes_dist(pos).y;
    return lottes_gaus(dst + off, LOTTES_HARD_SCAN);
}

// Combine three vertically-stacked horizontal filters, each weighted
// by its scanline contribution at the current sample point.
fn lottes_tri(pos: vec2<f32>) -> vec3<f32> {
    let a = lottes_horz3(pos, -1.0);
    let b = lottes_horz3(pos, 0.0);
    let c = lottes_horz3(pos, 1.0);
    let wa = lottes_scan(pos, -1.0);
    let wb = lottes_scan(pos, 0.0);
    let wc = lottes_scan(pos, 1.0);
    return a * wa + b * wb + c * wc;
}

// Barrel distortion. Input/output both in [0,1] UV. Out-of-range
// values are handled by the caller (clipped to black bezel).
fn lottes_warp(pos: vec2<f32>) -> vec2<f32> {
    var p = pos * 2.0 - vec2<f32>(1.0, 1.0);
    p = p * vec2<f32>(
        1.0 + (p.y * p.y) * LOTTES_WARP_X,
        1.0 + (p.x * p.x) * LOTTES_WARP_Y,
    );
    return p * 0.5 + vec2<f32>(0.5, 0.5);
}

// Aperture-grille shadow mask, period = 6 *window* pixels (not
// source pixels). At 3× source scale this gives 2 stripes per
// source pixel — visibly a mask, but fine enough not to dominate
// the image. Lottes' "stripe" mask variant.
fn lottes_mask(window_pos: vec2<f32>) -> vec3<f32> {
    let x = fract((window_pos.x + window_pos.y * 3.0) / 6.0);
    var mask = vec3<f32>(LOTTES_MASK_DARK, LOTTES_MASK_DARK, LOTTES_MASK_DARK);
    if x < 0.333 {
        mask.r = LOTTES_MASK_LIGHT;
    } else if x < 0.666 {
        mask.g = LOTTES_MASK_LIGHT;
    } else {
        mask.b = LOTTES_MASK_LIGHT;
    }
    return mask;
}

fn crt_filter(uv: vec2<f32>, window_pos: vec2<f32>, color: vec4<f32>) -> vec4<f32> {
    let warped = lottes_warp(uv);
    // Out of bounds after warp → black bezel.
    if warped.x < 0.0 || warped.x > 1.0 || warped.y < 0.0 || warped.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let pixels = lottes_tri(warped);
    let mask = lottes_mask(window_pos);
    let mixed = pixels * mask * LOTTES_BRIGHT_BOOST;
    return vec4<f32>(lottes_to_srgb(mixed), color.a);
}
