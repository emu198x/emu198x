// Shared receiver: machine timing and electrical palettes arrive as data.
struct Params {
    width:u32, height:u32, line_pixels:u32, spp:u32,
    mode:u32, pal:u32, separated:u32, phases:u32,
    first_pixel:u32, padding0:u32, padding1:u32, padding2:u32,
}
@group(0) @binding(0) var<uniform> params:Params;
@group(0) @binding(1) var<storage,read> codes:array<u32>;
@group(0) @binding(2) var<storage,read> levels:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read> carrier:array<vec2<f32>>;
@group(0) @binding(4) var<storage,read> references:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read> luma_taps:array<f32>;
@group(0) @binding(6) var<storage,read> chroma_taps:array<f32>;
@group(0) @binding(7) var<storage,read_write> signal:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read_write> mixed:array<vec4<f32>>;
@group(0) @binding(9) var<storage,read_write> decoded:array<vec4<f32>>;
@group(0) @binding(10) var picture:texture_storage_2d<rgba8unorm,write>;
fn samples()->u32 {return params.line_pixels*params.spp;}
fn physical(x:u32)->u32 {return (params.first_pixel+x)%params.line_pixels;}
@compute @workgroup_size(64)
fn encode(@builtin(global_invocation_id) id:vec3<u32>) {
    let sample=id.x; let row=id.y;
    if sample>=samples() || row>=params.height {return;}
    let p=sample/params.spp;
    let x=(p+params.line_pixels-params.first_pixel)%params.line_pixels;
    let reference=references[row];
    let a=carrier[sample];
    let oscillator=vec2<f32>(a.x*reference.x-a.y*reference.y,a.x*reference.y+a.y*reference.x);
    var c=vec3<f32>(0.0);
    var voltage=0.0;
    if x<params.width {
        let code=codes[row*params.width+x];
        if params.mode==2u {
            c=vec3<f32>(f32(code&255u),f32((code>>8u)&255u),f32((code>>16u)&255u))/255.0;
        } else if params.mode==1u {
            // Native waveform phase remains distinct from receiver hue.
            let phase=(u32(reference.z)+sample)%params.phases;
            voltage=levels[code*params.phases+phase].x;
        } else {c=levels[code].xyz;}
    }
    if params.mode==2u || params.separated!=0u {
        signal[row*samples()+sample]=vec4<f32>(c,0.0);
    } else {
        if params.mode==0u {voltage=c.x+c.y*oscillator.x+c.z*oscillator.y*reference.w;}
        signal[row*samples()+sample]=vec4<f32>(voltage,oscillator,0.0);
    }
}
@compute @workgroup_size(64)
fn separate(@builtin(global_invocation_id) id:vec3<u32>) {
    let sample=id.x;let row=id.y;
    if sample>=samples() || row>=params.height {return;}
    let offset=row*samples();let radius=params.spp*8u;
    var filtered=vec3<f32>(0.0);
    for(var tap=0u;tap<=radius*2u;tap++) {
        let x=u32(clamp(i32(sample)+i32(tap)-i32(radius),0,i32(samples())-1));
        filtered+=signal[offset+x].xyz*luma_taps[tap];
    }
    let source=signal[offset+sample];
    if params.mode==2u {mixed[offset+sample]=vec4<f32>(filtered,0.0);}
    else if params.separated!=0u {mixed[offset+sample]=vec4<f32>(filtered.x,source.yz,0.0);}
    else {
        let chroma=2.0*(source.x-filtered.x);
        mixed[offset+sample]=vec4<f32>(filtered.x,chroma*source.y,chroma*source.z*references[row].w,0.0);
    }
}
@compute @workgroup_size(64)
fn demodulate(@builtin(global_invocation_id) id:vec3<u32>) {
    let x=id.x;let row=id.y;
    if x>=params.width || row>=params.height {return;}
    let sample=physical(x)*params.spp+params.spp/2u;
    let offset=row*samples();let radius=params.spp*12u;
    if params.mode==2u {
        decoded[row*params.width+x]=0.5*(mixed[offset+sample-1u]+mixed[offset+sample]);
        return;
    }
    var uv=vec2<f32>(0.0);
    for(var tap=0u;tap<=radius*2u;tap++) {
        let a=u32(clamp(i32(sample)-1+i32(tap)-i32(radius),0,i32(samples())-1));
        let b=u32(clamp(i32(sample)+i32(tap)-i32(radius),0,i32(samples())-1));
        uv+=0.5*(mixed[offset+a].yz+mixed[offset+b].yz)*chroma_taps[tap];
    }
    let y=0.5*(mixed[offset+sample-1u].x+mixed[offset+sample].x);
    decoded[row*params.width+x]=vec4<f32>(y,uv,0.0);
}
@compute @workgroup_size(64)
fn display(@builtin(global_invocation_id) id:vec3<u32>) {
    let x=id.x;let row=id.y;
    if x>=params.width || row>=params.height {return;}
    var c=decoded[row*params.width+x].xyz;
    if params.pal!=0u && row>0u {
        c=vec3<f32>(c.x,0.5*(c.yz+decoded[(row-1u)*params.width+x].yz));
    }
    var rgb=c;
    if params.mode==0u {
        rgb=vec3<f32>(c.x+c.z/0.877,
          c.x-0.114/(0.587*0.493)*c.y-0.299/(0.587*0.877)*c.z,c.x+c.y/0.493);
    } else if params.mode==1u {
        // NTSC receiver quadrature is YIQ, with nominal FCC decoding.
        rgb=vec3<f32>(c.x+0.946882*c.y+0.623557*c.z,
          c.x-0.274788*c.y-0.635691*c.z,c.x-1.108545*c.y+1.709007*c.z);
    }
    textureStore(picture,vec2<u32>(x,row),vec4<f32>(round(clamp(rgb,vec3<f32>(0.0),vec3<f32>(1.0))*255.0)/255.0,1.0));
}
