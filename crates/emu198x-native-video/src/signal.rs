//! Cached GPU receiver for machine-supplied electrical frames.
use crate::VideoPresenterError;
use emu198x_shell::{CapturedFrame, CapturedSignal, SignalEncoding};
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    width: u32,
    height: u32,
    line_pixels: u32,
    first_pixel: u32,
    pixel_hz: f64,
    carrier_hz: f64,
    encoding: SignalEncoding,
    separated: bool,
    levels_len: usize,
}
struct Stage {
    pipeline: wgpu::ComputePipeline,
    bindings: wgpu::BindGroup,
    width: u32,
}
/// Receiver resources can also be driven offscreen for numerical validation.
pub struct SignalDecoder {
    key: Key,
    stages: Vec<Stage>,
    codes: wgpu::Buffer,
    references: wgpu::Buffer,
    levels: wgpu::Buffer,
    picture: wgpu::Texture,
    scratch: Vec<u32>,
    phase_scratch: Vec<[f32; 4]>,
}
fn low_pass(sample_hz: f64, cutoff: f64, radius: usize) -> Vec<f32> {
    let f = cutoff / sample_hz;
    let mut taps: Vec<f64> = (0..=2 * radius)
        .map(|i| {
            let x = i as f64 - radius as f64;
            let sinc = if x == 0.0 {
                2.0 * f
            } else {
                (TAU * f * x).sin() / (std::f64::consts::PI * x)
            };
            sinc * (0.54 - 0.46 * (TAU * i as f64 / (2 * radius) as f64).cos())
        })
        .collect();
    let total: f64 = taps.iter().sum();
    taps.iter_mut().for_each(|v| *v /= total);
    taps.into_iter().map(|v| v as f32).collect()
}
impl Key {
    fn from_frame(frame: &CapturedFrame, separated: bool) -> Result<Self, VideoPresenterError> {
        let source = frame
            .signal
            .as_ref()
            .ok_or(VideoPresenterError::MissingSignal)?;
        let t = source.timing;
        if frame.width == 0
            || frame.height == 0
            || frame.width > 2048
            || frame.height > 1024
            || t.line_pixels < frame.width
            || t.line_pixels > 4096
            || t.first_pixel >= t.line_pixels
            || !t.pixel_hz.is_finite()
            || t.pixel_hz <= 0.0
            || !t.carrier_hz.is_finite()
            || t.carrier_hz < 0.0
            || !t.phase_cycles.is_finite()
        {
            return Err(VideoPresenterError::InvalidSignal);
        }
        let count = (frame.width * frame.height) as usize;
        match source.encoding {
            SignalEncoding::Rgb => {
                if frame.pixels.len() != count * 4
                    || frame.format != emu198x_shell::PixelFormat::Rgba8888
                {
                    return Err(VideoPresenterError::InvalidSignal);
                }
            }
            SignalEncoding::Yuv {
                separate_chroma, ..
            } => {
                if separated && !separate_chroma {
                    return Err(VideoPresenterError::InvalidSignal);
                }
                if source.codes.len() != count
                    || source.levels.is_empty()
                    || source.levels.len() > 65536
                    || source
                        .codes
                        .iter()
                        .any(|&code| usize::from(code) >= source.levels.len())
                    || t.carrier_hz <= 0.0
                {
                    return Err(VideoPresenterError::InvalidSignal);
                }
            }
            SignalEncoding::Waveform { phases } => {
                if phases != 12
                    || source.codes.len() != count
                    || source.levels.is_empty()
                    || source.levels.len() > 65536
                    || source.levels.len() % phases as usize != 0
                    || source
                        .codes
                        .iter()
                        .any(|&code| usize::from(code) >= source.levels.len() / phases as usize)
                    || t.carrier_hz <= 0.0
                    || separated
                {
                    return Err(VideoPresenterError::InvalidSignal);
                }
                // Eight source samples per pixel must be twelve per carrier.
                if (t.carrier_hz / t.pixel_hz - 2.0 / 3.0).abs() > 1e-6 {
                    return Err(VideoPresenterError::InvalidSignal);
                }
            }
        }
        if source.levels.iter().flatten().any(|v| !v.is_finite()) {
            return Err(VideoPresenterError::InvalidSignal);
        }
        Ok(Self {
            width: frame.width,
            height: frame.height,
            line_pixels: t.line_pixels,
            first_pixel: t.first_pixel,
            pixel_hz: t.pixel_hz,
            carrier_hz: t.carrier_hz,
            encoding: source.encoding,
            separated,
            levels_len: source.levels.len(),
        })
    }
}
impl SignalDecoder {
    /// Build cached receiver resources. Call again when `matches` returns false.
    /// # Errors
    /// Rejects missing or malformed electrical data before creating GPU buffers.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &CapturedFrame,
        separated: bool,
    ) -> Result<Self, VideoPresenterError> {
        let key = Key::from_frame(frame, separated)?;
        let spp = if matches!(key.encoding, SignalEncoding::Waveform { .. }) {
            8u32
        } else {
            4
        };
        if u64::from(key.line_pixels) * u64::from(spp) * u64::from(key.height) * 16
            > device.limits().max_storage_buffer_binding_size
            || device.limits().max_storage_buffers_per_shader_stage < 5
        {
            return Err(VideoPresenterError::SignalGpuUnsupported);
        }
        let source = frame
            .signal
            .as_ref()
            .ok_or(VideoPresenterError::MissingSignal)?;
        let spp = if matches!(key.encoding, SignalEncoding::Waveform { .. }) {
            8u32
        } else {
            4
        };
        let samples = key.line_pixels * spp;
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let upload = |label, bytes: &[u8], usage| {
            let b = buffer(
                label,
                bytes.len().max(16) as u64,
                usage | wgpu::BufferUsages::COPY_DST,
            );
            if !bytes.is_empty() {
                queue.write_buffer(&b, 0, bytes);
            }
            b
        };
        let storage = wgpu::BufferUsages::STORAGE;
        let (mode, pal, phases) = match key.encoding {
            SignalEncoding::Yuv { pal, .. } => (0, u32::from(pal), 1),
            SignalEncoding::Waveform { phases } => (1, 0, phases),
            SignalEncoding::Rgb => (2, 0, 1),
        };
        let params = upload(
            "signal receiver parameters",
            bytemuck::cast_slice(&[
                key.width,
                key.height,
                key.line_pixels,
                spp,
                mode,
                pal,
                u32::from(separated),
                phases,
                key.first_pixel,
                0,
                0,
                0,
            ]),
            wgpu::BufferUsages::UNIFORM,
        );
        let codes = buffer(
            "electrical pixel codes",
            u64::from(key.width) * u64::from(key.height) * 4,
            storage | wgpu::BufferUsages::COPY_DST,
        );
        let levels = upload(
            "electrical levels",
            bytemuck::cast_slice(&source.levels),
            storage,
        );
        let oscillator: Vec<[f32; 2]> = (0..samples)
            .map(|sample| {
                let phase = TAU
                    * (((f64::from(sample) + 0.5) / f64::from(spp)) * key.carrier_hz
                        / key.pixel_hz)
                        .fract();
                [phase.cos() as f32, phase.sin() as f32]
            })
            .collect();
        let carrier = upload(
            "within-line oscillator",
            bytemuck::cast_slice(&oscillator),
            storage,
        );
        let references = buffer(
            "physical line references",
            u64::from(key.height) * 16,
            storage | wgpu::BufferUsages::COPY_DST,
        );
        let fs = key.pixel_hz * f64::from(spp);
        // These describe a nominal receiver, not a measured individual set.
        let bandwidth = if mode == 2 { 5_000_000.0 } else { 3_000_000.0 };
        let luma = upload(
            "receiver luma FIR",
            bytemuck::cast_slice(&low_pass(fs, bandwidth, 8 * spp as usize)),
            storage,
        );
        let chroma = upload(
            "receiver chroma FIR",
            bytemuck::cast_slice(&low_pass(fs, 1_300_000.0, 12 * spp as usize)),
            storage,
        );
        let signal = buffer(
            "electrical samples",
            u64::from(samples) * u64::from(key.height) * 16,
            storage,
        );
        let mixed = buffer(
            "receiver separated samples",
            u64::from(samples) * u64::from(key.height) * 16,
            storage,
        );
        let decoded = buffer(
            "receiver pixel components",
            u64::from(key.width) * u64::from(key.height) * 16,
            storage,
        );
        let picture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("signal decoded picture"),
            size: wgpu::Extent3d {
                width: key.width,
                height: key.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = picture.create_view(&Default::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("electrical receiver"),
            source: wgpu::ShaderSource::Wgsl(include_str!("signal.wgsl").into()),
        });
        let buffers = [
            &params,
            &codes,
            &levels,
            &carrier,
            &references,
            &luma,
            &chroma,
            &signal,
            &mixed,
            &decoded,
        ];
        let mut stages = Vec::with_capacity(4);
        for (entry, bindings, width) in [
            ("encode", vec![0, 1, 2, 3, 4, 7], samples),
            ("separate", vec![0, 4, 5, 7, 8], samples),
            ("demodulate", vec![0, 6, 8, 9], key.width),
            ("display", vec![0, 9, 10], key.width),
        ] {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            // WGSL eliminates unused bindings per entry point.
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
        Ok(Self {
            key,
            stages,
            codes,
            references,
            levels,
            picture,
            scratch: Vec::with_capacity((key.width * key.height) as usize),
            phase_scratch: Vec::with_capacity(key.height as usize),
        })
    }
    /// Whether resources fit this frame and connection. Phase may change freely.
    #[must_use]
    pub fn matches(&self, frame: &CapturedFrame, separated: bool) -> bool {
        Key::from_frame(frame, separated).is_ok_and(|key| key == self.key)
    }
    /// Decoded texture, sampled directly by the CRT presentation stage.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.picture
    }
    /// Upload changing electrical inputs and record all four receiver passes.
    /// # Errors
    /// Rejects incompatible geometry or malformed frame data.
    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &CapturedFrame,
    ) -> Result<(), VideoPresenterError> {
        if !self.matches(frame, self.key.separated) {
            return Err(VideoPresenterError::InvalidSignal);
        }
        let source = frame
            .signal
            .as_ref()
            .ok_or(VideoPresenterError::MissingSignal)?;
        self.scratch.clear();
        if self.key.encoding == SignalEncoding::Rgb {
            self.scratch.extend(
                frame
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]])),
            );
        } else {
            self.scratch
                .extend(source.codes.iter().map(|&v| u32::from(v)));
        }
        queue.write_buffer(&self.codes, 0, bytemuck::cast_slice(&self.scratch));
        if !source.levels.is_empty() {
            queue.write_buffer(&self.levels, 0, bytemuck::cast_slice(&source.levels));
        }
        self.fill_references(source);
        queue.write_buffer(
            &self.references,
            0,
            bytemuck::cast_slice(&self.phase_scratch),
        );
        for stage in &self.stages {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("signal receiver pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&stage.pipeline);
            pass.set_bind_group(0, &stage.bindings, &[]);
            pass.dispatch_workgroups(stage.width.div_ceil(64), self.key.height, 1);
        }
        Ok(())
    }
    fn fill_references(&mut self, source: &CapturedSignal) {
        self.phase_scratch.clear();
        for row in 0..self.key.height {
            let line = u64::from(source.timing.first_line) + u64::from(row);
            let cycles = (source.timing.phase_cycles
                + line as f64 * f64::from(self.key.line_pixels) * self.key.carrier_hz
                    / self.key.pixel_hz)
                .rem_euclid(1.0);
            let waveform_phase = (cycles * 12.0).round() as u32 % 12;
            let hue = if matches!(self.key.encoding, SignalEncoding::Waveform { .. }) {
                3.9 / 12.0
            } else {
                0.0
            };
            let phase = TAU * (cycles + hue);
            let sign = if matches!(self.key.encoding, SignalEncoding::Yuv { pal: true, .. })
                && line & 1 != 0
            {
                -1.0
            } else {
                1.0
            };
            self.phase_scratch.push([
                phase.cos() as f32,
                phase.sin() as f32,
                waveform_phase as f32,
                sign,
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{MachineTime, PixelFormat, SignalTiming};
    fn frame() -> CapturedFrame {
        CapturedFrame {
            timestamp: MachineTime::new(0),
            format: PixelFormat::Indexed8,
            width: 2,
            height: 1,
            palette: Some(vec![0; 2]),
            pixels: vec![0, 1],
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
                    phase_cycles: 0.0,
                },
                codes: vec![0, 1],
                levels: vec![[0.0; 4], [1.0, 0.0, 0.0, 0.0]],
            }),
        }
    }
    #[test]
    fn receiver_rejects_impossible_connection_and_malformed_sources() {
        let mut frame = frame();
        assert!(Key::from_frame(&frame, false).is_ok());
        assert!(Key::from_frame(&frame, true).is_err());
        frame.signal.as_mut().expect("source").codes[0] = 2;
        assert!(Key::from_frame(&frame, false).is_err());
        frame.signal.as_mut().expect("source").codes[0] = 0;
        frame.signal.as_mut().expect("source").timing.phase_cycles = f64::NAN;
        assert!(Key::from_frame(&frame, false).is_err());
    }
    #[test]
    fn phase_changes_reuse_resources_but_clock_and_geometry_changes_do_not() {
        let mut frame = frame();
        let original = Key::from_frame(&frame, false).expect("source");
        frame.signal.as_mut().expect("source").timing.phase_cycles = 0.5;
        frame.signal.as_mut().expect("source").timing.first_line = 265;
        assert_eq!(Key::from_frame(&frame, false).expect("source"), original);
        frame.signal.as_mut().expect("source").timing.pixel_hz = 7_100_000.0;
        assert_ne!(Key::from_frame(&frame, false).expect("source"), original);
    }
    #[test]
    fn receiver_fir_preserves_dc_and_rejects_pal_subcarrier() {
        let taps = low_pass(28_000_000.0, 3_000_000.0, 32);
        assert!((taps.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        let response: f64 = taps
            .iter()
            .enumerate()
            .map(|(i, &tap)| {
                f64::from(tap) * (TAU * 4_433_618.75 / 28_000_000.0 * (i as f64 - 32.0)).cos()
            })
            .sum();
        assert!(response.abs() < 0.01);
    }
}
