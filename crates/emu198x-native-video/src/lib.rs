//! Shared `wgpu` video presentation for native frontends.

use std::sync::Arc;

use emu198x_shell::{CapturedFrame, PixelFormat};
use thiserror::Error;
use winit::window::Window;

const SHADER: &str = include_str!("shader.wgsl");

/// How native frames should be scaled onto the host window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScalingMode {
    /// Preserve aspect ratio and use integer scaling when the window is large enough.
    Integer,
    /// Stretch the machine frame to the whole window surface.
    Stretch,
}

/// Presentation settings applied by the native GPU backend.
#[derive(Clone, Copy, Debug)]
pub struct PresentationProfile {
    /// Scaling behaviour for the machine framebuffer.
    pub scaling: ScalingMode,
    /// Clear colour used for borders or letterboxing.
    pub clear_color: wgpu::Color,
}

impl Default for PresentationProfile {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Integer,
            clear_color: wgpu::Color::BLACK,
        }
    }
}

/// Native video presentation failure.
#[derive(Debug, Error)]
pub enum VideoPresenterError {
    /// No compatible GPU adapter could present to this window surface.
    #[error("no compatible GPU adapter found for native video surface")]
    NoAdapter,

    /// The host window surface could not be created.
    #[error("failed to create native video surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    /// The GPU device could not be created.
    #[error("failed to create native video device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    /// The surface reported no usable output format.
    #[error("native video surface has no supported output formats")]
    MissingSurfaceFormat,

    /// The frame payload did not match the declared geometry.
    #[error(
        "frame data length {actual} does not match expected {expected} for {format:?} {width}x{height}"
    )]
    InvalidFrameData {
        /// Declared pixel format.
        format: PixelFormat,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
        /// Expected payload length in bytes.
        expected: usize,
        /// Actual payload length in bytes.
        actual: usize,
    },

    /// Indexed frames require a palette.
    #[error("indexed frame is missing a palette")]
    MissingPalette,

    /// The presenter does not know how to upload this pixel format yet.
    #[error("unsupported native video pixel format {format:?}")]
    UnsupportedPixelFormat {
        /// Unsupported pixel format.
        format: PixelFormat,
    },

    /// An indexed frame referenced a palette entry that does not exist.
    #[error("palette index {index} is out of range for palette length {palette_len}")]
    InvalidPaletteIndex {
        /// Offending palette index.
        index: u8,
        /// Available palette entries.
        palette_len: usize,
    },

    /// Rendering failed because the swapchain ran out of memory.
    #[error("native video surface ran out of memory")]
    SurfaceOutOfMemory,
}

/// Shared `wgpu` presenter for one native emulator window.
pub struct WgpuVideoPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    frame_texture: wgpu::Texture,
    frame_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    frame_width: u32,
    frame_height: u32,
    rgba_scratch: Vec<u8>,
}

impl WgpuVideoPresenter {
    /// Creates a presenter attached to `window`.
    ///
    /// # Errors
    ///
    /// Returns an error if the host cannot create a GPU surface, adapter, or device.
    pub fn new(
        window: Arc<Window>,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, VideoPresenterError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .ok_or(VideoPresenterError::NoAdapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("emu198x-native-video device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))?;

        let caps = surface.get_capabilities(&adapter);
        let format =
            choose_surface_format(&caps).ok_or(VideoPresenterError::MissingSurfaceFormat)?;
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            caps.present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::AutoVsync)
        };
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("emu198x-native-video shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("emu198x-native-video bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("emu198x-native-video pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("emu198x-native-video pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("emu198x-native-video nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let (frame_texture, frame_view, bind_group) = create_frame_texture(
            &device,
            &bind_group_layout,
            &sampler,
            frame_width,
            frame_height,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group_layout,
            sampler,
            frame_texture,
            frame_view,
            bind_group,
            frame_width,
            frame_height,
            rgba_scratch: Vec::new(),
        })
    }

    /// Resizes the host surface.
    pub fn resize_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Uploads and presents one machine frame.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame payload is malformed or the GPU surface cannot present.
    pub fn present(
        &mut self,
        frame: &CapturedFrame,
        profile: &PresentationProfile,
    ) -> Result<(), VideoPresenterError> {
        self.upload_frame(frame)?;
        self.render(profile)
    }

    fn upload_frame(&mut self, frame: &CapturedFrame) -> Result<(), VideoPresenterError> {
        let rgba = frame_rgba_pixels(frame, &mut self.rgba_scratch)?;
        if frame.width != self.frame_width || frame.height != self.frame_height {
            let (texture, view, bind_group) = create_frame_texture(
                &self.device,
                &self.bind_group_layout,
                &self.sampler,
                frame.width,
                frame.height,
            );
            self.frame_texture = texture;
            self.frame_view = view;
            self.bind_group = bind_group;
            self.frame_width = frame.width;
            self.frame_height = frame.height;
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.frame_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.width.saturating_mul(4)),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    fn render(&mut self, profile: &PresentationProfile) -> Result<(), VideoPresenterError> {
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(VideoPresenterError::SurfaceOutOfMemory);
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("emu198x-native-video encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("emu198x-native-video render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(profile.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let viewport = viewport_for(
                self.config.width,
                self.config.height,
                self.frame_width,
                self.frame_height,
                profile.scaling,
            );
            pass.set_viewport(
                viewport.x,
                viewport.y,
                viewport.width,
                viewport.height,
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn choose_surface_format(caps: &wgpu::SurfaceCapabilities) -> Option<wgpu::TextureFormat> {
    caps.formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .or_else(|| caps.formats.first().copied())
}

fn create_frame_texture(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("emu198x-native-video frame texture"),
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
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("emu198x-native-video bind group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (texture, view, bind_group)
}

fn frame_rgba_pixels<'a>(
    frame: &'a CapturedFrame,
    scratch: &'a mut Vec<u8>,
) -> Result<&'a [u8], VideoPresenterError> {
    let pixel_count =
        pixel_count(frame.width, frame.height).ok_or(VideoPresenterError::InvalidFrameData {
            format: frame.format,
            width: frame.width,
            height: frame.height,
            expected: usize::MAX,
            actual: frame.pixels.len(),
        })?;

    match frame.format {
        PixelFormat::Rgba8888 => {
            let expected = pixel_count.saturating_mul(4);
            if frame.pixels.len() != expected {
                return Err(VideoPresenterError::InvalidFrameData {
                    format: frame.format,
                    width: frame.width,
                    height: frame.height,
                    expected,
                    actual: frame.pixels.len(),
                });
            }
            Ok(&frame.pixels)
        }
        PixelFormat::Indexed8 => {
            if frame.pixels.len() != pixel_count {
                return Err(VideoPresenterError::InvalidFrameData {
                    format: frame.format,
                    width: frame.width,
                    height: frame.height,
                    expected: pixel_count,
                    actual: frame.pixels.len(),
                });
            }
            let palette = frame
                .palette
                .as_ref()
                .ok_or(VideoPresenterError::MissingPalette)?;
            scratch.resize(pixel_count.saturating_mul(4), 0);
            for (&index, rgba) in frame.pixels.iter().zip(scratch.chunks_exact_mut(4)) {
                let value = palette.get(index as usize).ok_or(
                    VideoPresenterError::InvalidPaletteIndex {
                        index,
                        palette_len: palette.len(),
                    },
                )?;
                rgba[0] = (value >> 24) as u8;
                rgba[1] = (value >> 16) as u8;
                rgba[2] = (value >> 8) as u8;
                rgba[3] = *value as u8;
            }
            Ok(scratch)
        }
        _ => Err(VideoPresenterError::UnsupportedPixelFormat {
            format: frame.format,
        }),
    }
}

fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    width.checked_mul(height)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Viewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn viewport_for(
    surface_width: u32,
    surface_height: u32,
    frame_width: u32,
    frame_height: u32,
    scaling: ScalingMode,
) -> Viewport {
    if scaling == ScalingMode::Stretch || frame_width == 0 || frame_height == 0 {
        return Viewport {
            x: 0.0,
            y: 0.0,
            width: surface_width as f32,
            height: surface_height as f32,
        };
    }

    let x_scale = surface_width as f32 / frame_width as f32;
    let y_scale = surface_height as f32 / frame_height as f32;
    let scale = x_scale.min(y_scale);
    let scale = if scale >= 1.0 { scale.floor() } else { scale };
    let width = frame_width as f32 * scale;
    let height = frame_height as f32 * scale;

    Viewport {
        x: (surface_width as f32 - width) * 0.5,
        y: (surface_height as f32 - height) * 0.5,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::MachineTime;

    #[test]
    fn indexed_frames_expand_to_rgba() {
        let frame = CapturedFrame {
            timestamp: MachineTime::new(0),
            format: PixelFormat::Indexed8,
            width: 2,
            height: 1,
            palette: Some(vec![0x0102_03FF, 0xA0B0_C0FF]),
            pixels: vec![1, 0],
        };
        let mut scratch = Vec::new();

        let rgba = frame_rgba_pixels(&frame, &mut scratch).expect("frame should expand");

        assert_eq!(rgba, &[0xA0, 0xB0, 0xC0, 0xFF, 0x01, 0x02, 0x03, 0xFF]);
    }

    #[test]
    fn integer_viewport_centres_with_whole_scale() {
        let viewport = viewport_for(800, 600, 160, 144, ScalingMode::Integer);

        assert_eq!(
            viewport,
            Viewport {
                x: 80.0,
                y: 12.0,
                width: 640.0,
                height: 576.0,
            }
        );
    }

    #[test]
    fn small_integer_viewport_scales_fractionally_when_needed() {
        let viewport = viewport_for(80, 80, 160, 144, ScalingMode::Integer);

        assert_eq!(
            viewport,
            Viewport {
                x: 0.0,
                y: 4.0,
                width: 80.0,
                height: 72.0,
            }
        );
    }
}
