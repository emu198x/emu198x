//! Cached GPU encoder/decoder benchmark with independent f64 CPU verification.
use emu198x_spectrum_composite_experiment::{
    CARRIER_HZ, Connection, Experiment, FRAME_LINES, HEIGHT, LINE_PIXELS, PIXEL_HZ, Receiver,
    WIDTH, colour, low_pass, rgb,
};
use std::{error::Error, f64::consts::TAU, fs, path::Path, sync::mpsc, time::Instant};
mod offscreen_crt;

fn f32_bytes(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn references(receiver: Receiver) -> Vec<u8> {
    f32_bytes((0..HEIGHT).flat_map(|row| {
        let tick = receiver.field as f64 * (FRAME_LINES * LINE_PIXELS) as f64
            + (row + 264) as f64 * LINE_PIXELS as f64;
        let phase = TAU * (tick * CARRIER_HZ / PIXEL_HZ + receiver.phase_cycles).fract();
        [phase.cos() as f32, phase.sin() as f32]
    }))
}

struct Stage {
    pipeline: wgpu::ComputePipeline,
    bindings: wgpu::BindGroup,
    width: u32,
}

struct Readback {
    yuv: Vec<u8>,
    rgba: Vec<u8>,
    crt_rgba: Vec<u8>,
}

struct Decoder {
    device: wgpu::Device,
    queue: wgpu::Queue,
    stages: Vec<Stage>,
    indices: wgpu::Buffer,
    references: wgpu::Buffer,
    decoded: wgpu::Buffer,
    picture: wgpu::Texture,
    crt: offscreen_crt::CrtStage,
    adapter: String,
}

impl Decoder {
    async fn new(receiver: Receiver) -> Result<Self, Box<dyn Error>> {
        // Reuse CPU validation rather than let GPU indexing accept bad settings.
        let _ = Experiment::new(receiver)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;
        let make_buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let upload = |label, bytes: &[u8], usage| {
            let buffer = make_buffer(
                label,
                bytes.len() as u64,
                usage | wgpu::BufferUsages::COPY_DST,
            );
            queue.write_buffer(&buffer, 0, bytes);
            buffer
        };
        let spp = receiver.samples_per_pixel;
        let samples = LINE_PIXELS * spp;
        let storage = wgpu::BufferUsages::STORAGE;
        let params = upload(
            "receiver parameters",
            &[
                spp as u32,
                samples as u32,
                u32::from(receiver.delay_line),
                0,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
            wgpu::BufferUsages::UNIFORM,
        );
        let indices = make_buffer(
            "packed palette indices",
            (WIDTH * HEIGHT) as u64,
            storage | wgpu::BufferUsages::COPY_DST,
        );
        let palette = upload(
            "shared analogue pin table",
            &f32_bytes((0..16).flat_map(|index| {
                let c = colour(index);
                [c.y as f32, c.u as f32, c.v as f32, 0.0]
            })),
            storage,
        );
        let carrier = upload(
            "within-line carrier",
            &f32_bytes((0..samples).flat_map(|sample| {
                let phase =
                    TAU * (((sample as f64 + 0.5) / spp as f64) * CARRIER_HZ / PIXEL_HZ).fract();
                [phase.cos() as f32, phase.sin() as f32]
            })),
            storage,
        );
        let references = upload("per-line phase reference", &references(receiver), storage);
        let fs = PIXEL_HZ * spp as f64;
        let luma = upload(
            "shared luma FIR",
            &f32_bytes(
                low_pass(fs, receiver.luma_hz, 8 * spp)
                    .into_iter()
                    .map(|v| v as f32),
            ),
            storage,
        );
        let chroma = upload(
            "shared chroma FIR",
            &f32_bytes(
                low_pass(fs, receiver.chroma_hz, 12 * spp)
                    .into_iter()
                    .map(|v| v as f32),
            ),
            storage,
        );
        let signal = make_buffer(
            "encoded composite and reference",
            (samples * HEIGHT * 16) as u64,
            storage,
        );
        let mixed = make_buffer(
            "luma and demodulated chroma",
            (samples * HEIGHT * 16) as u64,
            storage,
        );
        let decoded = make_buffer(
            "decoded sample-centre YUV",
            (WIDTH * HEIGHT * 16) as u64,
            storage | wgpu::BufferUsages::COPY_SRC,
        );
        // Usable directly as the production CRT shader's source texture.
        let picture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("decoded RGBA picture"),
            size: wgpu::Extent3d {
                width: WIDTH as u32,
                height: HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = picture.create_view(&Default::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Spectrum composite compute"),
            source: wgpu::ShaderSource::Wgsl(include_str!("decode.wgsl").into()),
        });
        let buffers = [
            &params,
            &indices,
            &palette,
            &carrier,
            &references,
            &luma,
            &chroma,
            &signal,
            &mixed,
            &decoded,
        ];
        let mut stages = Vec::new();
        for (entry, bindings, width) in [
            ("encode", vec![0, 1, 2, 3, 4, 7], samples as u32),
            ("separate", vec![0, 5, 7, 8], samples as u32),
            ("demodulate", vec![0, 6, 8, 9], WIDTH as u32),
            ("display", vec![0, 9, 10], WIDTH as u32),
        ] {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            let entries: Vec<_> = bindings
                .into_iter()
                .map(|binding| wgpu::BindGroupEntry {
                    binding,
                    resource: if binding == 10 {
                        wgpu::BindingResource::TextureView(&view)
                    } else {
                        buffers[binding as usize].as_entire_binding()
                    },
                })
                .collect();
            let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(entry),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            });
            stages.push(Stage {
                pipeline,
                bindings,
                width,
            });
        }
        let crt = offscreen_crt::CrtStage::new(&device, &queue, &picture);
        Ok(Self {
            device,
            queue,
            stages,
            indices,
            references,
            decoded,
            picture,
            crt,
            adapter: name,
        })
    }

    fn dispatch(
        &self,
        frame: &[u8],
        receiver: Receiver,
        present: bool,
    ) -> Result<(), Box<dyn Error>> {
        self.queue.write_buffer(&self.indices, 0, frame);
        self.queue
            .write_buffer(&self.references, 0, &references(receiver));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for stage in &self.stages {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&stage.pipeline);
            pass.set_bind_group(0, &stage.bindings, &[]);
            pass.dispatch_workgroups(stage.width.div_ceil(64), HEIGHT as u32, 1);
        }
        if present {
            self.crt.draw(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    fn readback(&self) -> Result<Readback, Box<dyn Error>> {
        let stride = (WIDTH as u32 * 4).div_ceil(256) * 256;
        let staging = |size| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("verification readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        };
        let yuv = staging((WIDTH * HEIGHT * 16) as u64);
        let rgba = staging(u64::from(stride) * HEIGHT as u64);
        let crt_stride = (offscreen_crt::WIDTH * 4).div_ceil(256) * 256;
        let crt = staging(u64::from(crt_stride) * u64::from(offscreen_crt::HEIGHT));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.decoded, 0, &yuv, 0, yuv.size());
        encoder.copy_texture_to_buffer(
            self.picture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &rgba,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(HEIGHT as u32),
                },
            },
            wgpu::Extent3d {
                width: WIDTH as u32,
                height: HEIGHT as u32,
                depth_or_array_layers: 1,
            },
        );
        encoder.copy_texture_to_buffer(
            self.crt.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &crt,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(crt_stride),
                    rows_per_image: Some(offscreen_crt::HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: offscreen_crt::WIDTH,
                height: offscreen_crt::HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        for buffer in [&yuv, &rgba, &crt] {
            let tx = tx.clone();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
        }
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        for _ in 0..3 {
            rx.recv()??;
        }
        let floats = yuv.slice(..).get_mapped_range()?.to_vec();
        let mapped = rgba.slice(..).get_mapped_range()?;
        let pixels = mapped
            .chunks_exact(stride as usize)
            .flat_map(|row| row[..WIDTH * 4].iter().copied())
            .collect();
        drop(mapped);
        let mapped = crt.slice(..).get_mapped_range()?;
        let crt_pixels = mapped
            .chunks_exact(crt_stride as usize)
            .flat_map(|row| row[..offscreen_crt::WIDTH as usize * 4].iter().copied())
            .collect();
        drop(mapped);
        yuv.unmap();
        rgba.unmap();
        crt.unmap();
        Ok(Readback {
            yuv: floats,
            rgba: pixels,
            crt_rgba: crt_pixels,
        })
    }
}

fn benchmark(
    decoder: &Decoder,
    frame: &[u8],
    receiver: Receiver,
    present: bool,
) -> Result<Vec<f64>, Box<dyn Error>> {
    for field in 0..8 {
        decoder.dispatch(
            frame,
            Receiver {
                field: receiver.field + field,
                ..receiver
            },
            present,
        )?;
    }
    let mut times = Vec::new();
    for field in 8..68 {
        let start = Instant::now();
        decoder.dispatch(
            frame,
            Receiver {
                field: receiver.field + field,
                ..receiver
            },
            present,
        )?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(times)
}

fn main() -> Result<(), Box<dyn Error>> {
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: decode-gpu INPUT.idx OUTPUT_DIR [samples=4] [phase=0] [field=0]".into(),
        );
    }
    let receiver = Receiver {
        samples_per_pixel: args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(4),
        phase_cycles: args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(0.0),
        field: args.get(5).map(|v| v.parse()).transpose()?.unwrap_or(0),
        ..Receiver::default()
    };
    let frame = fs::read(&args[1])?;
    if frame.len() != WIDTH * HEIGHT || frame.iter().any(|i| *i > 15) {
        return Err("invalid index frame".into());
    }
    let oracle = Experiment::new(receiver)?.decode(&frame, Connection::Composite)?;
    let decoder = Decoder::new(receiver).await?;
    eprintln!("Composite decoder GPU: {}", decoder.adapter);
    // Seed a different picture before reusing the same resources. Final parity
    // must still match the requested input, catching stale index uploads/data.
    let alternate: Vec<_> = frame.iter().map(|index| index ^ 7).collect();
    decoder.dispatch(&alternate, receiver, false)?;
    // Warm eight changing fields. Time sixty completed frames individually:
    // per-frame uploads, encoding, filtering, resolve, submission and wait.
    // Pipeline creation, fixed LUT uploads and verification readback excluded.
    let times = benchmark(&decoder, &frame, receiver, false)?;
    let presented_times = benchmark(&decoder, &frame, receiver, true)?;
    decoder.dispatch(&frame, receiver, true)?;
    let Readback {
        yuv: unaveraged,
        rgba: pixels,
        crt_rgba: crt_pixels,
    } = decoder.readback()?;
    let floats: Vec<_> = unaveraged
        .as_chunks::<16>()
        .0
        .iter()
        .map(|pixel| {
            let f: Vec<_> = pixel
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b) as f64)
                .collect();
            [f[0], f[1], f[2]]
        })
        .collect();
    let mut max_yuv: f64 = 0.0;
    let mut yuv_mse = 0.0;
    let mut rgb_mse = 0.0;
    let mut max_byte = 0;
    let mut different = 0;
    for (i, (sample, expected)) in floats.iter().zip(&oracle).enumerate() {
        let mut gpu = *sample;
        if receiver.delay_line && i >= WIDTH {
            gpu[1] = (gpu[1] + floats[i - WIDTH][1]) * 0.5;
            gpu[2] = (gpu[2] + floats[i - WIDTH][2]) * 0.5;
        }
        for (a, b) in gpu.into_iter().zip([expected.y, expected.u, expected.v]) {
            let delta = (a - b).abs();
            max_yuv = max_yuv.max(delta);
            yuv_mse += delta * delta;
        }
        for (channel, value) in rgb(*expected).into_iter().enumerate() {
            let expected = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            let delta = pixels[i * 4 + channel].abs_diff(expected);
            max_byte = max_byte.max(delta);
            different += usize::from(delta != 0);
            rgb_mse += f64::from(delta).powi(2);
        }
        if pixels[i * 4 + 3] != 255 {
            return Err("GPU output alpha differs from reference".into());
        }
    }
    if max_yuv > 0.0001 || max_byte > 1 {
        return Err(format!("GPU parity failed: max YUV {max_yuv}, max byte {max_byte}").into());
    }
    let dir = Path::new(&args[2]);
    fs::create_dir_all(dir)?;
    fs::write(dir.join("gpu-composite.rgba"), pixels)?;
    fs::write(dir.join("gpu-composite-crt.rgba"), crt_pixels)?;
    fs::write(
        dir.join("gpu-metrics.json"),
        format!(
            "{{\"adapter\":\"{}\",\"samples_per_pixel\":{},\"completed_frames\":60,\"median_frame_ms\":{:.6},\"p95_frame_ms\":{:.6},\"max_frame_ms\":{:.6},\"median_decode_crt_ms\":{:.6},\"p95_decode_crt_ms\":{:.6},\"max_decode_crt_ms\":{:.6},\"max_yuv_delta\":{max_yuv:.9},\"yuv_rmse\":{:.9},\"display_rgb_rmse_8bit\":{:.9},\"max_rgb_byte_delta\":{max_byte},\"different_rgb_channel_fraction\":{:.9}}}\n",
            decoder.adapter.replace('"', "\\\""),
            receiver.samples_per_pixel,
            times[30],
            times[56],
            times[59],
            presented_times[30],
            presented_times[56],
            presented_times[59],
            (yuv_mse / (WIDTH * HEIGHT * 3) as f64).sqrt(),
            (rgb_mse / (WIDTH * HEIGHT * 3) as f64).sqrt(),
            different as f64 / (WIDTH * HEIGHT * 3) as f64
        ),
    )?;
    println!(
        "GPU decode {:.3} ms / decode+CRT {:.3} ms median ({:.3} ms p95); max YUV delta {max_yuv:.8}, max RGB byte delta {max_byte}",
        times[30], presented_times[30], presented_times[56]
    );
    Ok(())
}
