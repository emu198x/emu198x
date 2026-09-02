//! Shared `wgpu` video presentation for native frontends.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
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

/// GPU presentation filter applied after the machine framebuffer is uploaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum VideoFilter {
    /// Raw nearest-neighbour pixels for debugging and golden comparisons.
    #[default]
    Raw,
    /// DMG-style LCD tint and a subtle subpixel grid.
    Lcd,
    /// Simple CRT-style scanline and phosphor mask.
    Crt,
}

impl VideoFilter {
    fn shader_value(self) -> f32 {
        match self {
            Self::Raw => 0.0,
            Self::Lcd => 1.0,
            Self::Crt => 2.0,
        }
    }
}

impl fmt::Display for VideoFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Raw => "raw",
            Self::Lcd => "lcd",
            Self::Crt => "crt",
        };
        f.write_str(value)
    }
}

/// Parsing failure for a host video filter argument.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("expected raw, lcd, or crt")]
pub struct ParseVideoFilterError;

impl FromStr for VideoFilter {
    type Err = ParseVideoFilterError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "raw" => Ok(Self::Raw),
            "lcd" => Ok(Self::Lcd),
            "crt" => Ok(Self::Crt),
            _ => Err(ParseVideoFilterError),
        }
    }
}

/// Presentation settings applied by the native GPU backend.
#[derive(Clone, Copy, Debug)]
pub struct PresentationProfile {
    /// Scaling behaviour for the machine framebuffer.
    pub scaling: ScalingMode,
    /// Clear colour used for borders or letterboxing.
    pub clear_color: wgpu::Color,
    /// GPU filter applied to the uploaded machine framebuffer.
    pub filter: VideoFilter,
    /// Pixel aspect ratio (pixel width ÷ height) of the source framebuffer.
    /// `1.0` for square pixels; e.g. ≈`1.6` for the Atari 2600, whose 160
    /// pixels span a 4:3 picture. The presenter stretches width by this so the
    /// image displays with its true proportions instead of looking too narrow.
    pub pixel_aspect_ratio: f32,
}

impl Default for PresentationProfile {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Integer,
            clear_color: wgpu::Color::BLACK,
            filter: VideoFilter::Raw,
            pixel_aspect_ratio: 1.0,
        }
    }
}

impl PresentationProfile {
    /// Raw pixel presentation with centred integer scaling.
    #[must_use]
    pub fn raw() -> Self {
        Self::default()
    }

    /// DMG-style LCD presentation.
    #[must_use]
    pub fn lcd_dmg() -> Self {
        Self {
            filter: VideoFilter::Lcd,
            ..Self::default()
        }
    }

    /// Basic CRT presentation for TV/monitor systems.
    #[must_use]
    pub fn crt() -> Self {
        Self {
            filter: VideoFilter::Crt,
            ..Self::default()
        }
    }

    /// Creates the default profile for a parsed filter.
    #[must_use]
    pub fn for_filter(filter: VideoFilter) -> Self {
        match filter {
            VideoFilter::Raw => Self::raw(),
            VideoFilter::Lcd => Self::lcd_dmg(),
            VideoFilter::Crt => Self::crt(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ShaderUniforms {
    filter: f32,
    frame_width: f32,
    frame_height: f32,
    _pad: f32,
}

impl ShaderUniforms {
    fn new(filter: VideoFilter, frame_width: u32, frame_height: u32) -> Self {
        Self {
            filter: filter.shader_value(),
            frame_width: frame_width as f32,
            frame_height: frame_height as f32,
            _pad: 0.0,
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

    /// The surface hit an unrecoverable error (e.g. a validation failure).
    #[error("native video surface hit an unrecoverable error")]
    SurfaceUnrecoverable,
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
    uniform_buffer: wgpu::Buffer,
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
        let surface_size = (size.width.max(1), size.height.max(1));
        pollster::block_on(Self::new_async(
            window,
            surface_size,
            frame_width,
            frame_height,
        ))
    }

    /// Creates a presenter attached to any wgpu surface target — a winit
    /// window natively, an `HtmlCanvasElement` in a browser.
    ///
    /// Async rather than blocking because adapter and device acquisition
    /// cannot be driven through `pollster::block_on` on the web: blocking the
    /// browser's main thread deadlocks it. [`Self::new`] keeps the blocking
    /// call for native callers, so their signature is unchanged.
    ///
    /// `surface_size` is a parameter rather than read from the target, because
    /// a canvas has no `inner_size`.
    ///
    /// # Errors
    ///
    /// Returns an error if the host cannot create a GPU surface, adapter, or device.
    pub async fn new_async<T>(
        target: T,
        surface_size: (u32, u32),
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, VideoPresenterError>
    where
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        let width = surface_size.0.max(1);
        let height = surface_size.1.max(1);

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })
            .await
            .map_err(|_| VideoPresenterError::NoAdapter)?;
        // WebGL2 advertises far lower limits than a native backend, so asking
        // for the desktop defaults there fails device creation outright.
        #[cfg(target_arch = "wasm32")]
        let required_limits = wgpu::Limits::downlevel_webgl2_defaults();
        #[cfg(not(target_arch = "wasm32"))]
        let required_limits = wgpu::Limits::default();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("emu198x-native-video device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await?;

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
            color_space: wgpu::SurfaceColorSpace::Auto,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("emu198x-native-video pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("emu198x-native-video pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("emu198x-native-video nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("emu198x-native-video uniforms"),
            size: std::mem::size_of::<ShaderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (frame_texture, frame_view, bind_group) = create_frame_texture(
            &device,
            &bind_group_layout,
            &sampler,
            &uniform_buffer,
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
            uniform_buffer,
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
                &self.uniform_buffer,
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
            wgpu::TexelCopyTextureInfo {
                texture: &self.frame_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
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
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(VideoPresenterError::SurfaceUnrecoverable);
            }
        };
        let uniforms = ShaderUniforms::new(profile.filter, self.frame_width, self.frame_height);
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
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
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(profile.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            let viewport = viewport_for(
                self.config.width,
                self.config.height,
                self.frame_width,
                self.frame_height,
                profile.scaling,
                profile.pixel_aspect_ratio,
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
        self.queue.present(output);
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
    uniform_buffer: &wgpu::Buffer,
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
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform_buffer.as_entire_binding(),
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
            for (&index, rgba) in frame
                .pixels
                .iter()
                .zip(scratch.as_chunks_mut::<4>().0.iter_mut())
            {
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
    pixel_aspect_ratio: f32,
) -> Viewport {
    if scaling == ScalingMode::Stretch || frame_width == 0 || frame_height == 0 {
        return Viewport {
            x: 0.0,
            y: 0.0,
            width: surface_width as f32,
            height: surface_height as f32,
        };
    }

    // Correct for non-square pixels: the framebuffer's effective display width
    // is its pixel count times the pixel aspect ratio. Height stays integer-
    // scaled; width follows the corrected aspect.
    let par = if pixel_aspect_ratio > 0.0 {
        pixel_aspect_ratio
    } else {
        1.0
    };
    let effective_width = frame_width as f32 * par;
    let x_scale = surface_width as f32 / effective_width;
    let y_scale = surface_height as f32 / frame_height as f32;
    let scale = x_scale.min(y_scale);
    let scale = if scale >= 1.0 { scale.floor() } else { scale };
    let width = effective_width * scale;
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
    fn video_filters_parse_from_cli_values() {
        assert_eq!("raw".parse::<VideoFilter>(), Ok(VideoFilter::Raw));
        assert_eq!("lcd".parse::<VideoFilter>(), Ok(VideoFilter::Lcd));
        assert_eq!("crt".parse::<VideoFilter>(), Ok(VideoFilter::Crt));
        assert!("bad".parse::<VideoFilter>().is_err());
    }

    #[test]
    fn presentation_profile_uses_requested_filter() {
        assert_eq!(
            PresentationProfile::for_filter(VideoFilter::Raw).filter,
            VideoFilter::Raw
        );
        assert_eq!(
            PresentationProfile::for_filter(VideoFilter::Lcd).filter,
            VideoFilter::Lcd
        );
        assert_eq!(
            PresentationProfile::for_filter(VideoFilter::Crt).filter,
            VideoFilter::Crt
        );
    }

    #[test]
    fn integer_viewport_centres_with_whole_scale() {
        let viewport = viewport_for(800, 600, 160, 144, ScalingMode::Integer, 1.0);

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
    fn aspect_ratio_stretches_width_and_recentres() {
        // 160×192 square pixels at PAR 1.6 ⇒ 256-wide effective image (4:3).
        // In an 800×600 surface the limiting axis is height (600/192 = 3.125 →
        // scale 3): width = 160·1.6·3 = 768, height = 192·3 = 576, centred.
        let viewport = viewport_for(800, 600, 160, 192, ScalingMode::Integer, 1.6);

        assert_eq!(
            viewport,
            Viewport {
                x: 16.0,
                y: 12.0,
                width: 768.0,
                height: 576.0,
            }
        );
    }

    #[test]
    fn small_integer_viewport_scales_fractionally_when_needed() {
        let viewport = viewport_for(80, 80, 160, 144, ScalingMode::Integer, 1.0);

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
