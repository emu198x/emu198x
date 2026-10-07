//! Offscreen checks of the unmodified production field-presentation shader.
use emu198x_native_video::wgpu;
use std::{error::Error, fs, path::Path, sync::mpsc};
#[allow(dead_code)]
mod offscreen_crt;

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: &[u8],
    width: u32,
    height: u32,
    filter: f32,
    field: f32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic interlace diagnostic"),
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
    });
    queue.write_texture(
        source.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        source.size(),
    );
    render_texture(device, queue, &source, filter, field)
}

pub(crate) fn render_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Texture,
    filter: f32,
    field: f32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let (width, height) = (source.width(), source.height());
    let stage = offscreen_crt::CrtStage::with_uniforms(
        device,
        queue,
        source,
        [filter, width as f32, height as f32, field],
        width * 2,
        height * 2,
    );
    let stride = (width * 8).div_ceil(256) * 256;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("field presentation readback"),
        size: u64::from(stride) * u64::from(height * 2),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut commands = device.create_command_encoder(&Default::default());
    stage.draw(&mut commands);
    commands.copy_texture_to_buffer(
        stage.output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height * 2),
            },
        },
        stage.output.size(),
    );
    queue.submit([commands.finish()]);
    let (tx, rx) = mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = staging.slice(..).get_mapped_range()?;
    let result = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..width as usize * 8].iter().copied())
        .collect();
    drop(mapped);
    staging.unmap();
    Ok(result)
}
fn save(
    out: &Path,
    name: &str,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    let frame = emu198x_shell::CapturedFrame {
        timestamp: emu198x_shell::MachineTime::new(0),
        signal: None,
        format: emu198x_shell::PixelFormat::Rgba8888,
        width,
        height,
        palette: None,
        pixels,
    };
    fs::write(out.join(name), frame.png_bytes()?)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/interlace-validation".into());
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    println!("adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let (w, h) = (64_u32, 32_u32);
    let colour_rows: Vec<u8> = (0..h)
        .flat_map(|y| {
            (0..w).flat_map(move |_| {
                if y.is_multiple_of(2) {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
        })
        .collect();
    for (field, expected) in [(3.0, [255, 0, 0, 255]), (4.0, [0, 0, 255, 255])] {
        let result = render(&device, &queue, &colour_rows, w, h, 0.0, field)?;
        assert!(
            result
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel == &expected),
            "bob sampled the opposite field"
        );
    }
    let woven = render(&device, &queue, &colour_rows, w, h, 0.0, 0.0)?;
    for y in 0..h * 2 {
        let expected = if (y / 2).is_multiple_of(2) {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        assert!(
            woven[(y * w * 8) as usize..((y + 1) * w * 8) as usize]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel == &expected)
        );
    }
    for (field, absent_channel, present_channel) in [(1.0, 2, 0), (2.0, 0, 2)] {
        let result = render(&device, &queue, &colour_rows, w, h, 2.0, field)?;
        assert!(
            result
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[absent_channel] == 0),
            "CRT sampled the opposite field"
        );
        assert!(
            result
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[present_channel] > 0)
        );
    }
    let white = vec![255; (w * h * 4) as usize];
    let even = render(&device, &queue, &white, w, h, 2.0, 1.0)?;
    let odd = render(&device, &queue, &white, w, h, 2.0, 2.0)?;
    assert!(
        even.iter().zip(&odd).any(|(a, b)| a.abs_diff(*b) > 20),
        "CRT field beam did not move by half a line"
    );
    println!(
        "PASS: bob row duplication; weave row placement; CRT parity isolation; half-line beam offset"
    );

    // A deliberately synthetic 768×576 detail/motion plate. Static fine rows
    // expose field flicker; an offset bar exposes weave's motion combing.
    let (w, h) = (768, 576);
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
            let offset = ((y * w + x) * 4) as usize;
            plate[offset..offset + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    for (name, filter, field) in [
        ("crt-even.png", 2.0, 1.0),
        ("crt-odd.png", 2.0, 2.0),
        ("bob-even.png", 0.0, 3.0),
        ("bob-odd.png", 0.0, 4.0),
        ("weave.png", 0.0, 0.0),
    ] {
        save(
            out,
            name,
            w * 2,
            h * 2,
            render(&device, &queue, &plate, w, h, filter, field)?,
        )?;
    }
    // Project-owned diagnostic firmware: enable LACE, read LOF, and set
    // COLOR00 white in long fields and black in short fields. This exercises
    // the real CPU/register/field handoff without any external ROM.
    let mut rom = vec![0; 256 * 1024];
    let program = [
        0x00, 0x08, 0x00, 0x00, 0x00, 0xF8, 0x00, 0x08, 0x33, 0xFC, 0x00, 0x04, 0x00, 0xDF, 0xF1,
        0x00, 0x30, 0x39, 0x00, 0xDF, 0xF0, 0x04, 0x08, 0x00, 0x00, 0x0F, 0x66, 0x0A, 0x33, 0xFC,
        0x00, 0x00, 0x00, 0xDF, 0xF1, 0x80, 0x60, 0xEA, 0x33, 0xFC, 0x0F, 0xFF, 0x00, 0xDF, 0xF1,
        0x80, 0x60, 0xE0,
    ];
    rom[..program.len()].copy_from_slice(&program);
    fs::write(out.join("synthetic-lace.rom"), &rom)?;
    use emu198x_shell::{
        FieldParity, HostIo, LatestFrameCapture, MachineCore, NullAudioSink, NullTraceSink,
    };
    let mut machine = runtime_commodore_amiga::AmigaOcsRuntime::new(
        runtime_commodore_amiga::Model::A500OcsPal,
        rom,
    )?;
    let mut capture = LatestFrameCapture::default();
    let mut audio = NullAudioSink;
    let mut trace = NullTraceSink;
    let mut observed = Vec::new();
    for _ in 0..4 {
        machine.run_until(
            machine.time().saturating_add(1),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut capture,
                audio_sink: &mut audio,
                trace_sink: &mut trace,
            },
        )?;
        let frame = capture.frame().ok_or("no guest frame")?;
        if let Some(field) = frame.signal.as_ref().and_then(|signal| signal.field) {
            let y = 100 + u32::from(field.parity == FieldParity::Odd);
            let index = ((y * frame.width + frame.width / 2) * 4) as usize;
            let value = if field.parity == FieldParity::Even {
                255
            } else {
                0
            };
            assert_eq!(&frame.pixels[index..index + 4], &[value, value, value, 255]);
            observed.push(field);
        }
    }
    assert_eq!(
        observed.len(),
        3,
        "initial guest mode transition must not claim a stable field"
    );
    assert!(
        observed.windows(2).all(
            |pair| pair[1].sequence == pair[0].sequence + 1 && pair[1].parity != pair[0].parity
        )
    );
    fs::write(
        out.join("guest-lace-raw.png"),
        capture.frame().ok_or("no guest frame")?.png_bytes()?,
    )?;
    println!(
        "PASS: guest-enabled LACE, LOF-dependent colour, completed-field metadata and alternating retained rows"
    );
    println!("diagnostic images: {}", out.display());
    Ok(())
}
