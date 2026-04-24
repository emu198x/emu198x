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
    let rgb = min(color.rgb * mask * scanline + bloom_floor, vec3<f32>(1.0, 1.0, 1.0));
    return vec4<f32>(rgb, color.a);
}
