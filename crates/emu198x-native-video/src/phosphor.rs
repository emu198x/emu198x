//! Cached source-space phosphor history; no host-frame-count timebase.
use crate::VideoFilter;
use emu198x_shell::{CapturedFrame, MachineTime, VideoField};

#[derive(Default)]
struct DecayClock {
    last: Option<(MachineTime, Option<VideoField>)>,
    settings: Option<(VideoFilter, u64, u32)>,
}
impl DecayClock {
    fn advance(
        &mut self,
        frame: &CapturedFrame,
        filter: VideoFilter,
        clock_hz: f64,
        tau_ms: f32,
    ) -> Option<f32> {
        let field = frame.signal.as_ref().and_then(|signal| signal.field);
        let current = (frame.timestamp, field);
        let settings = (filter, clock_hz.to_bits(), tau_ms.to_bits());
        if self.last == Some(current) && self.settings == Some(settings) {
            return None;
        }
        let retention = self
            .last
            .filter(|(previous, old_field)| {
                self.settings == Some(settings)
                    && *previous < frame.timestamp
                    && match (*old_field, field) {
                        (None, None) => true,
                        (Some(old), Some(new)) => old.sequence < new.sequence,
                        _ => false,
                    }
            })
            .map_or(0.0, |(previous, _)| {
                let seconds = (frame.timestamp.get() - previous.get()) as f64 / clock_hz;
                (-seconds / (f64::from(tau_ms) * 0.001)).exp() as f32
            });
        self.last = Some(current);
        self.settings = Some(settings);
        Some(retention)
    }
}

/// Receiver-independent phosphor approximation and machine timebase.
#[derive(Clone, Copy, Debug)]
pub struct PhosphorSettings {
    /// Display/connection identity, used to clear incompatible history.
    pub filter: VideoFilter,
    /// Authoritative machine timestamp frequency in Hz.
    pub clock_hz: f64,
    /// Exponential 1/e light-decay time in milliseconds; zero disables decay.
    pub tau_ms: f32,
}

/// Source-space phosphor history shared by native and offscreen presentation.
/// The output is linear RGB and must go through the CRT shader's linear path.
pub struct PhosphorHistory {
    textures: [wgpu::Texture; 2],
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    current: usize,
    clock: DecayClock,
}
impl PhosphorHistory {
    /// Allocate two cached half-float history textures at retained-raster size.
    #[must_use]
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("phosphor recharge and decay"),
            source: wgpu::ShaderSource::Wgsl(include_str!("phosphor.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("phosphor recharge and decay"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let textures = std::array::from_fn(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("linear phosphor history"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("phosphor parameters"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            textures,
            pipeline,
            uniform,
            current: 0,
            clock: DecayClock::default(),
        }
    }
    /// Current linear-light output, suitable for the shared CRT presenter.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.textures[self.current]
    }

    pub(crate) fn textures(&self) -> &[wgpu::Texture; 2] {
        &self.textures
    }
    pub(crate) fn current_index(&self) -> usize {
        self.current
    }

    /// Update exactly once per new machine frame/field. Returns whether the
    /// output texture changed. Call once per command-buffer submission.
    /// Unknown clocks or invalid/disabled decay refuse
    /// the update; callers should use the original presentation in that case.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        commands: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        frame: &CapturedFrame,
        settings: PhosphorSettings,
    ) -> bool {
        let PhosphorSettings {
            filter,
            clock_hz,
            tau_ms,
        } = settings;
        if !clock_hz.is_finite() || clock_hz <= 0.0 || !tau_ms.is_finite() || tau_ms <= 0.0 {
            return false;
        }
        let Some(retention) = self.clock.advance(frame, filter, clock_hz, tau_ms) else {
            return false;
        };
        // Native presentation treats the complete retained raster as stable,
        // including both rows of an interlaced pair. Field metadata is timing
        // information; it must not select which rows receive light.
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&[retention, -1.0, 0.0, 0.0]),
        );
        let source_view = source.create_view(&Default::default());
        let history_view = self.texture().create_view(&Default::default());
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("phosphor source and previous light"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        self.current ^= 1;
        let output = self.texture().create_view(&Default::default());
        let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("phosphor recharge and decay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{CapturedSignal, FieldParity, PixelFormat, SignalEncoding, SignalTiming};
    fn frame(time: u64, sequence: u64) -> CapturedFrame {
        CapturedFrame {
            timestamp: MachineTime::new(time),
            width: 2,
            height: 2,
            format: PixelFormat::Rgba8888,
            palette: None,
            pixels: vec![0; 16],
            signal: Some(CapturedSignal {
                field: Some(VideoField {
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
                    line_pixels: 2,
                    first_pixel: 0,
                    first_line: 0,
                    phase_cycles: 0.0,
                },
                codes: Vec::new(),
                levels: Vec::new(),
            }),
        }
    }
    #[test]
    fn decay_uses_elapsed_machine_time_and_redraws_do_not_recharge() {
        let mut clock = DecayClock::default();
        assert_eq!(
            clock.advance(&frame(20_000, 0), VideoFilter::Monitor, 1_000_000.0, 6.0),
            Some(0.0)
        );
        let next = frame(40_000, 1);
        let retention = clock
            .advance(&next, VideoFilter::Monitor, 1_000_000.0, 6.0)
            .expect("new field");
        assert!((retention - (-20.0_f32 / 6.0).exp()).abs() < 1e-6);
        assert_eq!(
            clock.advance(&next, VideoFilter::Monitor, 1_000_000.0, 6.0),
            None
        );
        // Skipping presentation still decays for the full elapsed interval.
        let retention = clock
            .advance(&frame(100_000, 4), VideoFilter::Monitor, 1_000_000.0, 6.0)
            .expect("new field");
        assert!((retention - (-60.0_f32 / 6.0).exp()).abs() < 1e-6);
    }
    #[test]
    fn incompatible_histories_reset_instead_of_ghosting() {
        let mut clock = DecayClock::default();
        clock.advance(&frame(20_000, 0), VideoFilter::Monitor, 1_000_000.0, 6.0);
        assert_eq!(
            clock.advance(&frame(40_000, 1), VideoFilter::Signal, 1_000_000.0, 6.0),
            Some(0.0)
        );
        assert_eq!(
            clock.advance(&frame(60_000, 2), VideoFilter::Signal, 2_000_000.0, 6.0),
            Some(0.0)
        );
        assert_eq!(
            clock.advance(&frame(80_000, 3), VideoFilter::Signal, 2_000_000.0, 12.0),
            Some(0.0)
        );
        assert_eq!(
            clock.advance(&frame(20_000, 0), VideoFilter::Signal, 2_000_000.0, 12.0),
            Some(0.0)
        );
        let mut progressive = frame(40_000, 1);
        progressive.signal.as_mut().expect("signal").field = None;
        assert_eq!(
            clock.advance(&progressive, VideoFilter::Signal, 2_000_000.0, 12.0),
            Some(0.0)
        );
    }
}
