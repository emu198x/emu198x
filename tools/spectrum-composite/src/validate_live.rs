//! Exercise the production receiver and real runtime handoffs without a window.
use emu198x_native_video::{SignalDecoder, wgpu};
use emu198x_shell::{
    CapturedFrame, CapturedSignal, HostIo, LatestFrameCapture, MachineCore, MachineTime,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullTraceSink, PixelFormat, SignalEncoding,
    SignalTiming,
};
use std::{error::Error, fs, path::Path, sync::mpsc, time::Instant};
#[allow(dead_code)]
#[path = "lib.rs"]
mod oracle;
use oracle::{Connection, Experiment, HEIGHT, Receiver, WIDTH, colour, rgb};
fn capture(machine: &mut impl MachineCore, ticks: u64) -> Result<CapturedFrame, Box<dyn Error>> {
    let mut frames = LatestFrameCapture::default();
    let mut audio = NullAudioSink;
    let mut trace = NullTraceSink;
    let target = machine.time().saturating_add(ticks);
    machine.run_until(
        target,
        &mut HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        },
    )?;
    Ok(frames.frame().ok_or("no frame")?.clone())
}
fn decode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &CapturedFrame,
    separated: bool,
) -> Result<(Vec<u8>, f64), Box<dyn Error>> {
    let mut decoder = SignalDecoder::new(device, queue, frame, separated)?;
    let mut times = Vec::new();
    for i in 0..38 {
        let begin = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        decoder.encode(queue, &mut encoder, frame)?;
        let submission = queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })?;
        if i >= 8 {
            times.push(begin.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    let stride = (frame.width * 4).div_ceil(256) * 256;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("validation readback"),
        size: u64::from(stride) * u64::from(frame.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: decoder.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(frame.height),
            },
        },
        wgpu::Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    rx.recv()??;
    let mapped = staging.slice(..).get_mapped_range()?;
    let bytes = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..(frame.width * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    staging.unmap();
    Ok((bytes, times[times.len() / 2]))
}
fn export(
    dir: &Path,
    name: &str,
    frame: &CapturedFrame,
    bytes: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    let output = CapturedFrame {
        timestamp: frame.timestamp,
        signal: None,
        format: PixelFormat::Rgba8888,
        width: frame.width,
        height: frame.height,
        palette: None,
        pixels: bytes,
    };
    fs::write(dir.join(format!("{name}.png")), output.png_bytes()?)?;
    Ok(())
}
fn quantise(rgb: [f64; 3]) -> [u8; 3] {
    rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}
fn main() -> Result<(), Box<dyn Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/signal-live-validation".into());
    let dir = Path::new(&dir);
    fs::create_dir_all(dir)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    println!("adapter: {}", adapter.get_info().name);
    // Independent f64 oracle: same Spectrum input and field, through production GPU.
    for field in [0u64, 1, 7] {
        let input: Vec<u8> = (0..WIDTH * HEIGHT)
            .map(|i| {
                if ((i % WIDTH) / 8).is_multiple_of(2) {
                    15
                } else {
                    0
                }
            })
            .collect();
        let receiver = Receiver {
            field,
            ..Default::default()
        };
        let oracle = Experiment::new(receiver)?.decode(&input, Connection::Composite)?;
        let frame = CapturedFrame {
            timestamp: MachineTime::new(field * 139776),
            signal: Some(CapturedSignal {
                field: None,
                encoding: SignalEncoding::Yuv {
                    pal: true,
                    separate_chroma: false,
                },
                timing: SignalTiming {
                    pixel_hz: 7_000_000.0,
                    carrier_hz: 4_433_618.75,
                    line_pixels: 448,
                    first_pixel: 412,
                    first_line: 264,
                    phase_cycles: (field as f64 * 139776.0 * 4_433_618.75 / 7_000_000.0).fract(),
                },
                codes: input.iter().map(|&v| u16::from(v)).collect(),
                levels: (0..16)
                    .map(|v| {
                        let c = colour(v);
                        [c.y as f32, c.u as f32, c.v as f32, 0.0]
                    })
                    .collect(),
            }),
            format: PixelFormat::Indexed8,
            width: WIDTH as u32,
            height: HEIGHT as u32,
            palette: Some(vec![0; 16]),
            pixels: input,
        };
        let (bytes, ms) = decode(&device, &queue, &frame, false)?;
        let mut max = 0;
        for (actual, expected) in bytes.as_chunks::<4>().0.iter().zip(oracle.iter()) {
            for (&actual, expected) in actual[..3].iter().zip(quantise(rgb(*expected))) {
                max = max.max(actual.abs_diff(expected));
            }
        }
        if max > 1 {
            return Err(format!("Spectrum oracle mismatch: {max}").into());
        }
        println!("Spectrum oracle field {field}: max byte delta {max}, {ms:.3} ms");
    }
    let rom = fs::read("../roms/48.rom")?;
    let mut spectrum = runtime_sinclair_zx_spectrum::Spectrum48kRuntime::from_rom_bytes(&rom)?;
    let frame = capture(&mut spectrum, 279552 * 120)?;
    let (bytes, ms) = decode(&device, &queue, &frame, false)?;
    let live_oracle = Experiment::new(Receiver {
        field: frame.timestamp.get() / 279552 - 1,
        ..Default::default()
    })?
    .decode(&frame.pixels, Connection::Composite)?;
    let max_delta = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .zip(live_oracle)
        .flat_map(|(actual, expected)| {
            actual[..3]
                .iter()
                .zip(quantise(rgb(expected)))
                .map(|(&actual, expected)| actual.abs_diff(expected))
                .collect::<Vec<_>>()
        })
        .max()
        .ok_or("empty Spectrum")?;
    if max_delta > 1 {
        return Err(format!("live Spectrum master-clock phase mismatch: {max_delta}").into());
    }
    println!("live Spectrum master-clock phase: max byte delta {max_delta}");
    export(dir, "spectrum-signal", &frame, bytes)?;
    fs::write(dir.join("spectrum-raw.png"), frame.png_bytes()?)?;
    println!("live Spectrum: {ms:.3} ms");
    // ROM-free C64 handoff; real-ROM path optional through existing fixture files.
    let read_or_blank = |path: &str, size| fs::read(path).unwrap_or_else(|_| vec![0; size]);
    let mut c64 = runtime_commodore_c64::C64Runtime::new(
        runtime_commodore_c64::Model::C64PalBreadbin,
        read_or_blank("../roms/c64/kernal.rom", 8192),
        read_or_blank("../roms/c64/basic.rom", 8192),
        read_or_blank("../roms/c64/chargen.rom", 4096),
        None,
    )?;
    let frame = capture(&mut c64, 19656 * 120)?;
    assert_eq!(
        frame
            .signal
            .as_ref()
            .ok_or("C64 signal missing")?
            .codes
            .len(),
        frame.pixels.len() / 4
    );
    fs::write(dir.join("c64-raw.png"), frame.png_bytes()?)?;
    for separated in [false, true] {
        let (bytes, ms) = decode(&device, &queue, &frame, separated)?;
        let name = if separated {
            "c64-monitor"
        } else {
            "c64-signal"
        };
        export(dir, name, &frame, bytes)?;
        println!("{name}: {ms:.3} ms");
    }
    let cart = fs::read("test-data/synthetic-cartridges/nintendo-nes-logo.nes")?;
    let mut nes = runtime_nintendo_nes::NesRuntime::blank(runtime_nintendo_nes::Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &cart));
    nes.load_media(&media)?;
    let frame = capture(&mut nes, 89342 * 60)?;
    fs::write(dir.join("nes-raw.png"), frame.png_bytes()?)?;
    let (bytes, ms) = decode(&device, &queue, &frame, false)?;
    export(dir, "nes-signal", &frame, bytes)?;
    println!("live NES: {ms:.3} ms");
    let mut amiga = runtime_commodore_amiga::AmigaOcsRuntime::new(
        runtime_commodore_amiga::Model::A500OcsPal,
        fs::read("../roms/kick13.rom")?,
    )?;
    let mut frame = capture(&mut amiga, 2_500_000)?;
    assert!(matches!(
        frame
            .signal
            .as_ref()
            .ok_or("Amiga signal missing")?
            .encoding,
        SignalEncoding::Rgb
    ));
    fs::write(dir.join("amiga-raw.png"), frame.png_bytes()?)?;
    let (bytes, ms) = decode(&device, &queue, &frame, true)?;
    export(dir, "amiga-monitor", &frame, bytes)?;
    println!("live Amiga RGB: {ms:.3} ms");
    // A real-ROM frame establishes the runtime contract, then a deterministic
    // RGB plate exercises the monitor's bandwidth at that runtime's geometry.
    for (i, pixel) in frame.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i % frame.width as usize;
        let y = i / frame.width as usize;
        pixel.copy_from_slice(&[
            if (x / 16).is_multiple_of(2) { 255 } else { 0 },
            if (y / 16).is_multiple_of(2) { 255 } else { 0 },
            64,
            255,
        ]);
    }
    fs::write(dir.join("amiga-rgb-raw.png"), frame.png_bytes()?)?;
    let (bytes, ms) = decode(&device, &queue, &frame, true)?;
    export(dir, "amiga-rgb-monitor", &frame, bytes)?;
    println!("Amiga RGB plate (runtime geometry): {ms:.3} ms");
    Ok(())
}
