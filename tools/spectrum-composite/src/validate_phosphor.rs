//! Production phosphor/history checks and an interlaced afterglow comparison.
use emu198x_native_video::{PhosphorHistory, PhosphorSettings, VideoFilter, wgpu};
use emu198x_shell::{
    CapturedFrame, CapturedSignal, FieldParity, MachineTime, PixelFormat, SignalEncoding,
    SignalTiming, VideoField,
};
use std::{error::Error, fs, path::Path, time::Instant};
#[allow(dead_code)]
#[path = "validate_interlace.rs"]
mod presentation;

fn frame(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    sequence: u64,
    interlace: bool,
) -> CapturedFrame {
    CapturedFrame {
        timestamp: MachineTime::new(sequence * 20_000),
        width,
        height,
        format: PixelFormat::Rgba8888,
        palette: None,
        pixels,
        signal: Some(CapturedSignal {
            field: interlace.then_some(VideoField {
                sequence,
                parity: if sequence.is_multiple_of(2) {
                    FieldParity::Even
                } else {
                    FieldParity::Odd
                },
            }),
            encoding: SignalEncoding::Rgb,
            timing: SignalTiming {
                pixel_hz: 14_187_580.0,
                carrier_hz: 0.0,
                line_pixels: width,
                first_pixel: 0,
                first_line: 0,
                phase_cycles: 0.0,
            },
            codes: Vec::new(),
            levels: Vec::new(),
        }),
    }
}
fn source(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("phosphor validation drive"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn update(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Texture,
    history: &mut PhosphorHistory,
    frame: &CapturedFrame,
    tau_ms: f32,
) -> Result<bool, Box<dyn Error>> {
    queue.write_texture(
        source.as_image_copy(),
        &frame.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(frame.width * 4),
            rows_per_image: Some(frame.height),
        },
        source.size(),
    );
    let mut commands = device.create_command_encoder(&Default::default());
    let changed = history.encode(
        device,
        queue,
        &mut commands,
        source,
        frame,
        PhosphorSettings {
            filter: VideoFilter::Monitor,
            clock_hz: 1_000_000.0,
            tau_ms,
        },
    );
    let submission = queue.submit([commands.finish()]);
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: None,
    })?;
    Ok(changed)
}
fn save(
    out: &Path,
    name: &str,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    fs::write(
        out.join(name),
        frame(width, height, pixels, 0, false).png_bytes()?,
    )?;
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/phosphor-validation".into());
    let out = Path::new(&output);
    fs::create_dir_all(out)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    println!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let (w, h) = (64_u32, 32_u32);
    let source = source(&device, w, h);
    let mut history = PhosphorHistory::new(&device, w, h);
    let white = frame(w, h, vec![255; (w * h * 4) as usize], 0, true);
    assert!(update(&device, &queue, &source, &mut history, &white, 6.0)?);
    let black = frame(w, h, vec![0; (w * h * 4) as usize], 1, true);
    assert!(update(&device, &queue, &source, &mut history, &black, 6.0)?);
    // Read linear light through the production shader's raw branch. At 20 ms
    // every row is exp(-20/6), regardless of the alternating field metadata.
    let light = presentation::render_texture(&device, &queue, history.texture(), 0.0, 0.0)?;
    let expected = ((-20.0_f32 / 6.0).exp() * 255.0).round() as u8;
    for y in 0..h * 2 {
        let value = expected;
        assert!(
            light[(y * w * 8) as usize..((y + 1) * w * 8) as usize]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[0].abs_diff(value) <= 1
                    && pixel[1].abs_diff(value) <= 1
                    && pixel[2].abs_diff(value) <= 1)
        );
    }
    assert!(
        !update(&device, &queue, &source, &mut history, &black, 6.0)?,
        "redraw recharged phosphor"
    );
    assert_eq!(
        light,
        presentation::render_texture(&device, &queue, history.texture(), 0.0, 0.0)?
    );
    let restored = frame(w, h, vec![0; (w * h * 4) as usize], 0, true);
    update(&device, &queue, &source, &mut history, &restored, 6.0)?;
    let reset = presentation::render_texture(&device, &queue, history.texture(), 0.0, 0.0)?;
    assert!(
        reset.as_chunks::<4>().0.iter().all(|p| p[..3] == [0, 0, 0]),
        "restore retained old light"
    );
    // sRGB drive is squared once, then reused as linear input by CRT sampling.
    let gray = frame(
        w,
        h,
        [128, 128, 128, 255].repeat((w * h) as usize),
        1,
        false,
    );
    update(&device, &queue, &source, &mut history, &gray, 6.0)?;
    let linear = presentation::render_texture(&device, &queue, history.texture(), 0.0, 0.0)?;
    assert!(
        linear
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0].abs_diff(64) <= 1)
    );
    let original = presentation::render_texture(&device, &queue, &source, 2.0, 0.0)?;
    let persistent = presentation::render_texture(&device, &queue, history.texture(), 2.0, 10.0)?;
    assert!(
        original
            .iter()
            .zip(&persistent)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "linear history changed first-exposure CRT output"
    );
    println!(
        "PASS: machine-time exponential decay; stable whole-raster afterglow; redraw idempotence; restore reset; linear-light history and first-exposure equivalence"
    );

    let (w, h) = (768_u32, 576_u32);
    let source = self::source(&device, w, h);
    let mut plate = vec![0; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let parity = y % 2;
            let rgb = if y < 384 {
                let value = if parity == 0 { 235 } else { 20 };
                if x > 96 && x < 672 {
                    [value, value, value]
                } else {
                    [30, 65, 100]
                }
            } else {
                let bar = 280 + parity * 24;
                if x >= bar && x < bar + 120 {
                    [240, 200, 40]
                } else {
                    [30, 65, 100]
                }
            };
            let i = ((y * w + x) * 4) as usize;
            plate[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    // Optional real-picture replay for reviewing the display model. The
    // raster is replayed as alternating fields; this does not assert that
    // the program which produced the picture had enabled hardware LACE.
    if let Some(path) = std::env::args().nth(2) {
        plate = fs::read(&path)?;
        if plate.len() != (w * h * 4) as usize {
            return Err("preview must be 768×576 RGBA".into());
        }
        println!("picture replay source: {path}");
    }
    for tau in [0_u32, 6, 20] {
        let mut history = PhosphorHistory::new(&device, w, h);
        for sequence in 0..6 {
            let frame = frame(w, h, plate.clone(), sequence, true);
            let mode = 0.0;
            update(
                &device,
                &queue,
                &source,
                &mut history,
                &frame,
                tau.max(1) as f32,
            )?;
            if sequence >= 4 {
                let input = if tau == 0 { &source } else { history.texture() };
                let flag = if tau == 0 { mode } else { mode + 10.0 };
                save(
                    out,
                    &format!(
                        "crt-{tau}-{}.png",
                        if sequence.is_multiple_of(2) {
                            "even"
                        } else {
                            "odd"
                        }
                    ),
                    w * 2,
                    h * 2,
                    presentation::render_texture(&device, &queue, input, 2.0, flag)?,
                )?;
            }
        }
    }
    // A static retained raster must remain identical as its field identity
    // alternates. This covers both the CRT shader and whole-raster history.
    for tau in [0_u32, 6, 20] {
        assert_eq!(
            fs::read(out.join(format!("crt-{tau}-even.png")))?,
            fs::read(out.join(format!("crt-{tau}-odd.png")))?,
            "static raster flickered between field parities"
        );
    }
    println!("PASS: static interlaced raster is stable across both field parities");
    let modern_frame = frame(w, h, plate.clone(), 0, true);
    update(
        &device,
        &queue,
        &source,
        &mut PhosphorHistory::new(&device, w, h),
        &modern_frame,
        6.0,
    )?;
    save(
        out,
        "modern-weave.png",
        w * 2,
        h * 2,
        presentation::render_texture(&device, &queue, &source, 0.0, 0.0)?,
    )?;

    // Completed update timing includes uploads, one source-space pass and GPU
    // completion, excluding source decoding, CRT rendering and readback.
    let mut history = PhosphorHistory::new(&device, w, h);
    let mut times = Vec::new();
    for sequence in 0..38 {
        let frame = frame(w, h, plate.clone(), sequence, true);
        let begin = Instant::now();
        update(&device, &queue, &source, &mut history, &frame, 6.0)?;
        if sequence >= 8 {
            times.push(begin.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    println!(
        "phosphor median completed update: {:.3} ms",
        times[times.len() / 2]
    );
    println!("images: {}", out.display());
    Ok(())
}
