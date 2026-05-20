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
    // Smith Ch 16 / Table 16-1: the Spectrum's luminance equation is
    //     Y = 0.299 R + 0.587 G + 0.151 B
    // Altwasser deliberately raised the blue coefficient above BT.601's
    // 0.114 because pure blue was "very dark and hardly visible" on
    // contemporary TVs. Using Smith's weights here makes the CRT
    // filter's analog-signal model match what real CRT-displayed
    // Spectrum output actually showed.
    let smith_y_weights = vec3<f32>(0.299, 0.587, 0.151);

    // Sample two horizontally-adjacent source pixels for chroma
    // bandwidth limit. Composite analog video carries U/V at a much
    // lower bandwidth than Y, producing the characteristic colour
    // smear on tight-pixel edges that defines the look of a real
    // Spectrum on a domestic CRT. We approximate this by blurring
    // only the colour residual (RGB − Y) while letting Y pass
    // through at full bandwidth — fine for the visual effect, even
    // though it isn't a strict BT.601 U/V blur.
    let pixel_step = vec2<f32>(1.0 / presentation.frame_width, 0.0);
    let c_left = textureSample(source_texture, source_sampler, uv - pixel_step).rgb;
    let c_right = textureSample(source_texture, source_sampler, uv + pixel_step).rgb;

    let y_center = dot(color.rgb, smith_y_weights);
    let y_left = dot(c_left, smith_y_weights);
    let y_right = dot(c_right, smith_y_weights);

    let chroma_center = color.rgb - vec3<f32>(y_center);
    let chroma_left = c_left - vec3<f32>(y_left);
    let chroma_right = c_right - vec3<f32>(y_right);

    // 3-tap [0.25, 0.5, 0.25] horizontal low-pass on the chroma
    // residual — a soft 2-pixel smear that maps to roughly the
    // bandwidth ratio of composite chroma vs luma signals.
    let chroma_blurred = 0.5 * chroma_center + 0.25 * chroma_left + 0.25 * chroma_right;

    // Q3 saturation modelling (Smith Ch 16): transistor Q3 in the
    // luminance circuit saturates when red and green currents are
    // both fully on, capping the /Y output at the same level for
    // Bright Yellow and Bright White. Our 8-bit RGB palette cannot
    // encode that — Bright Yellow (0xFFFF00) looks visibly less
    // luminous than Bright White on a digital display. Detecting
    // "R and G both fully on" via two step() functions lets us
    // pull Y up to the Bright-White level for those pixels and
    // recover the silicon-correct luminance.
    let rg_saturated = step(0.9, color.r) * step(0.9, color.g);
    let y_q3 = mix(y_center, 1.0, rg_saturated);

    var rgb = vec3<f32>(y_q3) + chroma_blurred;

    // Existing CRT envelope: scanline modulation + RGB triad mask +
    // bloom floor. Applied on top of the Spectrum-tuned luminance
    // and chroma so the analog look composes with the phosphor /
    // shadow-mask look.
    let source_pos = uv * vec2<f32>(presentation.frame_width, presentation.frame_height);
    let scanline = 0.74 + 0.26 * smoothstep(0.18, 0.62, fract(source_pos.y));

    let triad = u32(floor(source_pos.x * 3.0)) % 3u;
    var mask = vec3<f32>(0.88, 0.88, 0.88);
    if triad == 0u {
        mask.r = 1.08;
    } else if triad == 1u {
        mask.g = 1.08;
    } else {
        mask.b = 1.08;
    }

    let bloom_floor = vec3<f32>(0.018, 0.016, 0.014);
    rgb = min(rgb * mask * scanline + bloom_floor, vec3<f32>(1.0, 1.0, 1.0));
    return vec4<f32>(rgb, color.a);
}
