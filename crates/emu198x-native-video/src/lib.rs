//! Shared `wgpu` video presentation for native frontends.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use emu198x_shell::{CapturedFrame, PixelFormat};
use thiserror::Error;
use winit::window::Window;

// Re-exported because this crate's public types name wgpu's: a caller cannot
// build a `PresentationProfile` without `wgpu::Color`, nor a browser surface
// target without `wgpu::SurfaceTarget`.
pub use wgpu;

mod phosphor;
pub use phosphor::{PhosphorHistory, PhosphorSettings};

/// Provisional short-persistence CRT decay, not a calibrated monitor profile.
pub const DEFAULT_PHOSPHOR_TAU_MS: f32 = 6.0;

mod signal;
pub use signal::SignalDecoder;

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
    /// Machine electrical source decoded through a composite receiver, or RGB
    /// when the machine supplies an RGB monitor output. RF is not modelled.
    Signal,
    /// Direct RGB or separate luma/chroma monitor connection.
    Monitor,
    /// Decoded composite (or native RGB) on a modern integer-scaled display.
    Modern,
    /// Direct RGB or separate luma/chroma on a modern integer-scaled display.
    ModernMonitor,
    /// Compatibility name for the stable modern composite/native RGB display.
    ModernWeave,
    /// Compatibility name for the stable modern monitor display.
    ModernMonitorWeave,
}

impl VideoFilter {
    /// Whether this display receives the machine's electrical video source.
    pub fn uses_signal(self) -> bool {
        matches!(
            self,
            Self::Signal
                | Self::Monitor
                | Self::Modern
                | Self::ModernMonitor
                | Self::ModernWeave
                | Self::ModernMonitorWeave
        )
    }

    /// Whether the receiver uses a direct RGB or separate luma/chroma connection.
    pub fn uses_monitor_connection(self) -> bool {
        matches!(
            self,
            Self::Monitor | Self::ModernMonitor | Self::ModernMonitorWeave
        )
    }

    /// Whether the decoded raster is displayed without CRT presentation effects.
    pub fn is_modern(self) -> bool {
        matches!(
            self,
            Self::Modern | Self::ModernMonitor | Self::ModernWeave | Self::ModernMonitorWeave
        )
    }

    fn shader_value(self) -> f32 {
        match self {
            Self::Raw
            | Self::Modern
            | Self::ModernMonitor
            | Self::ModernWeave
            | Self::ModernMonitorWeave => 0.0,
            Self::Lcd => 1.0,
            Self::Crt | Self::Signal | Self::Monitor => 2.0,
        }
    }
}

impl fmt::Display for VideoFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Raw => "raw",
            Self::Lcd => "lcd",
            Self::Crt => "crt",
            Self::Signal => "signal",
            Self::Monitor => "monitor",
            Self::Modern => "modern",
            Self::ModernMonitor => "modern-monitor",
            Self::ModernWeave => "modern-weave",
            Self::ModernMonitorWeave => "modern-monitor-weave",
        };
        f.write_str(value)
    }
}

/// Parsing failure for a host video filter argument.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error(
    "expected raw, lcd, crt, signal, monitor, modern, modern-monitor, modern-weave, or modern-monitor-weave"
)]
pub struct ParseVideoFilterError;

impl FromStr for VideoFilter {
    type Err = ParseVideoFilterError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "raw" => Ok(Self::Raw),
            "lcd" => Ok(Self::Lcd),
            "crt" => Ok(Self::Crt),
            "signal" => Ok(Self::Signal),
            "monitor" => Ok(Self::Monitor),
            "modern" => Ok(Self::Modern),
            "modern-monitor" => Ok(Self::ModernMonitor),
            "modern-weave" => Ok(Self::ModernWeave),
            "modern-monitor-weave" => Ok(Self::ModernMonitorWeave),
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
    /// Authoritative frequency of frame timestamps. Zero bypasses persistence.
    pub clock_hz: f64,
    /// Exponential phosphor light-decay time (1/e), in milliseconds.
    /// Zero disables persistence; applies only to CRT presentation.
    pub phosphor_tau_ms: f32,
}

impl Default for PresentationProfile {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Integer,
            clear_color: wgpu::Color::BLACK,
            filter: VideoFilter::Raw,
            pixel_aspect_ratio: 1.0,
            clock_hz: 0.0,
            phosphor_tau_ms: DEFAULT_PHOSPHOR_TAU_MS,
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
            VideoFilter::Signal
            | VideoFilter::Monitor
            | VideoFilter::Modern
            | VideoFilter::ModernMonitor
            | VideoFilter::ModernWeave
            | VideoFilter::ModernMonitorWeave => Self {
                filter,
                ..Self::default()
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ShaderUniforms {
    filter: f32,
    frame_width: f32,
    frame_height: f32,
    field_mode: f32,
}

impl ShaderUniforms {
    fn new(filter: VideoFilter, frame_width: u32, frame_height: u32) -> Self {
        Self {
            filter: filter.shader_value(),
            frame_width: frame_width as f32,
            frame_height: frame_height as f32,
            field_mode: 0.0,
        }
    }
}

/// Native video presentation failure.
#[derive(Debug, Error)]
pub enum VideoPresenterError {
    /// The selected receiver needs a machine-supplied electrical frame.
    #[error("this machine or variant does not supply an electrical video source")]
    MissingSignal,
    /// Electrical palette, geometry or connection is malformed or unsupported.
    #[error("invalid or unsupported electrical video source or monitor connection")]
    InvalidSignal,
    /// The backend cannot execute the signal receiver's compute passes.
    #[error("signal display requires a compute-capable GPU backend")]
    SignalGpuUnsupported,

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
    /// An offscreen picture could not be read back from the GPU.
    #[error("the offscreen picture could not be read back from the GPU")]
    OffscreenReadback,
}

/// Shared `wgpu` presenter for one native emulator window.
pub struct WgpuVideoPresenter {
    target: Target,
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
    signal_decoder: Option<SignalDecoder>,
    signal_bind_group: Option<wgpu::BindGroup>,
    phosphor: Option<PhosphorHistory>,
    phosphor_bindings: Option<[wgpu::BindGroup; 2]>,
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

        // Detection rather than `Instance::default()`, which commits to WebGPU
        // whenever `navigator.gpu` exists. A browser can expose it and then
        // offer no adapter (wgpu names Chrome on Linux), and the presenter
        // then failed with `NoAdapter` instead of falling back to WebGL2.
        // Natively this builds the same instance `default()` does.
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
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
        Ok(Self::from_device(
            device,
            queue,
            config,
            Target::Surface(surface),
            frame_width,
            frame_height,
        ))
    }

    /// Creates a presenter that renders into an offscreen image of `width` ×
    /// `height` pixels instead of a window. After each present the picture is
    /// in [`Self::offscreen_pixels`], for a host that draws with its own
    /// toolkit, such as the GTK window.
    ///
    /// # Errors
    ///
    /// Returns an error if the host has no usable GPU adapter or device.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_offscreen(
        width: u32,
        height: u32,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, VideoPresenterError> {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|_| VideoPresenterError::NoAdapter)?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("emu198x-native-video offscreen device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                })
                .await?;
            let width = width.max(1);
            let height = height.max(1);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                // Not sRGB, like the surface format the window presenter
                // prefers, so both write the same bytes.
                format: OFFSCREEN_FORMAT,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            };
            let offscreen = Offscreen::new(&device, width, height);
            Ok(Self::from_device(
                device,
                queue,
                config,
                Target::Offscreen(offscreen),
                frame_width,
                frame_height,
            ))
        })
    }

    fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        target: Target,
        frame_width: u32,
        frame_height: u32,
    ) -> Self {
        let format = config.format;
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

        Self {
            target,
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
            signal_decoder: None,
            signal_bind_group: None,
            phosphor: None,
            phosphor_bindings: None,
        }
    }

    /// The last picture an offscreen presenter rendered, as tightly packed
    /// RGBA rows, with its width and height. `None` for a window presenter.
    #[must_use]
    pub fn offscreen_pixels(&self) -> Option<(&[u8], u32, u32)> {
        match &self.target {
            Target::Offscreen(offscreen) => {
                Some((&offscreen.pixels, offscreen.width, offscreen.height))
            }
            Target::Surface(_) => None,
        }
    }

    /// Resizes the host surface.
    pub fn resize_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        match &mut self.target {
            Target::Surface(surface) => surface.configure(&self.device, &self.config),
            Target::Offscreen(offscreen) => {
                *offscreen = Offscreen::new(&self.device, width, height)
            }
        }
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
        self.present_with_status(frame, profile).map(|_| ())
    }

    /// Present a frame and report whether it was submitted to the surface.
    /// Returns `false` for occlusion, timeout or a surface being reconfigured.
    /// A successful submission does not imply GPU completion or monitor scanout.
    ///
    /// # Errors
    /// Returns an error for malformed frame data or an unrecoverable surface.
    pub fn present_with_status(
        &mut self,
        frame: &CapturedFrame,
        profile: &PresentationProfile,
    ) -> Result<bool, VideoPresenterError> {
        if profile.filter.uses_signal() {
            if self.device.limits().max_storage_buffers_per_shader_stage < 5 {
                return Err(VideoPresenterError::SignalGpuUnsupported);
            }
            let separated = profile.filter.uses_monitor_connection();
            if !self
                .signal_decoder
                .as_ref()
                .is_some_and(|decoder| decoder.matches(frame, separated))
            {
                let decoder = SignalDecoder::new(&self.device, &self.queue, frame, separated)?;
                let view = decoder.texture().create_view(&Default::default());
                self.signal_bind_group = Some(create_frame_bind_group(
                    &self.device,
                    &self.bind_group_layout,
                    &self.sampler,
                    &self.uniform_buffer,
                    &view,
                ));
                self.signal_decoder = Some(decoder);
            }
            self.frame_width = frame.width;
            self.frame_height = frame.height;
        } else {
            // Signal dimensions may differ from the last raw texture's size.
            self.frame_width = self.frame_texture.width();
            self.frame_height = self.frame_texture.height();
            self.upload_frame(frame)?;
        }
        self.render(profile, frame)
    }

    /// Discard retained light after an explicit reset, restore or
    /// machine switch, including restores whose timestamps move forwards.
    pub fn reset_temporal_history(&mut self) {
        self.phosphor = None;
        self.phosphor_bindings = None;
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

    fn render(
        &mut self,
        profile: &PresentationProfile,
        frame: &CapturedFrame,
    ) -> Result<bool, VideoPresenterError> {
        let output = match &self.target {
            Target::Surface(surface) => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(output)
                | wgpu::CurrentSurfaceTexture::Suboptimal(output) => Some(output),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    surface.configure(&self.device, &self.config);
                    return Ok(false);
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(false);
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(VideoPresenterError::SurfaceUnrecoverable);
                }
            },
            Target::Offscreen(_) => None,
        };
        let mut uniforms = ShaderUniforms::new(profile.filter, self.frame_width, self.frame_height);

        let view = match (&output, &self.target) {
            (Some(output), _) => output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            (None, Target::Offscreen(offscreen)) => offscreen
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            (None, Target::Surface(_)) => return Ok(false),
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("emu198x-native-video encoder"),
            });
        let electrical = profile.filter.uses_signal();
        if electrical {
            self.signal_decoder
                .as_mut()
                .ok_or(VideoPresenterError::MissingSignal)?
                .encode(&self.queue, &mut encoder, frame)?;
        }
        let persistent = profile.filter.shader_value() > 1.5
            && profile.clock_hz.is_finite()
            && profile.clock_hz > 0.0
            && profile.phosphor_tau_ms.is_finite()
            && profile.phosphor_tau_ms > 0.0;
        if persistent {
            if !self.phosphor.as_ref().is_some_and(|history| {
                history.texture().width() == frame.width
                    && history.texture().height() == frame.height
            }) {
                let history = PhosphorHistory::new(&self.device, frame.width, frame.height);
                self.phosphor_bindings = Some(std::array::from_fn(|index| {
                    let view = history.textures()[index].create_view(&Default::default());
                    create_frame_bind_group(
                        &self.device,
                        &self.bind_group_layout,
                        &self.sampler,
                        &self.uniform_buffer,
                        &view,
                    )
                }));
                self.phosphor = Some(history);
            }
            let source = if electrical {
                self.signal_decoder
                    .as_ref()
                    .ok_or(VideoPresenterError::MissingSignal)?
                    .texture()
            } else {
                &self.frame_texture
            };
            if let Some(history) = self.phosphor.as_mut() {
                history.encode(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    source,
                    frame,
                    PhosphorSettings {
                        filter: profile.filter,
                        clock_hz: profile.clock_hz,
                        tau_ms: profile.phosphor_tau_ms,
                    },
                );
            }
            // Values >=10 select linear phosphor input in the shared shader.
            uniforms.field_mode += 10.0;
        } else {
            self.phosphor = None;
            self.phosphor_bindings = None;
        }
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
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
            let bindings = if let (Some(history), Some(bindings)) =
                (&self.phosphor, &self.phosphor_bindings)
            {
                &bindings[history.current_index()]
            } else if electrical {
                self.signal_bind_group
                    .as_ref()
                    .ok_or(VideoPresenterError::MissingSignal)?
            } else {
                &self.bind_group
            };
            pass.set_bind_group(0, bindings, &[]);
            pass.draw(0..6, 0..1);
        }
        if let Target::Offscreen(offscreen) = &self.target {
            offscreen.encode_copy(&mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        match (output, &mut self.target) {
            (Some(output), _) => self.queue.present(output),
            (None, Target::Offscreen(offscreen)) => offscreen.read_back(&self.device)?,
            (None, Target::Surface(_)) => {}
        }
        Ok(true)
    }
}

/// The colour format of an offscreen picture: RGBA bytes, not sRGB.
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Where a presenter draws.
enum Target {
    /// A window's surface.
    Surface(wgpu::Surface<'static>),
    /// An image the host reads back and draws itself.
    Offscreen(Offscreen),
}

/// An offscreen picture and the buffer it is copied into for reading.
struct Offscreen {
    texture: wgpu::Texture,
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    /// Row pitch in the readback buffer, padded to wgpu's copy alignment.
    padded_row: u32,
    /// The last picture, tightly packed.
    pixels: Vec<u8>,
}

impl Offscreen {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("emu198x-native-video offscreen picture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OFFSCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let padded_row = padded_row_bytes(width);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("emu198x-native-video offscreen readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            texture,
            buffer,
            width,
            height,
            padded_row,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Wait for the copy, then unpad its rows into `pixels`.
    fn read_back(&mut self, device: &wgpu::Device) -> Result<(), VideoPresenterError> {
        let slice = self.buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        match receiver.recv() {
            Ok(Ok(())) => {}
            _ => return Err(VideoPresenterError::OffscreenReadback),
        }
        {
            let mapped = slice
                .get_mapped_range()
                .map_err(|_| VideoPresenterError::OffscreenReadback)?;
            let row = self.width as usize * 4;
            for (out, padded) in self
                .pixels
                .chunks_exact_mut(row)
                .zip(mapped.chunks_exact(self.padded_row as usize))
            {
                out.copy_from_slice(&padded[..row]);
            }
            // A window surface shows the picture opaque whatever alpha the
            // filters leave, and a host compositing this image would not, so
            // make it opaque here to match.
            for pixel in self.pixels.as_chunks_mut::<4>().0 {
                pixel[3] = 0xFF;
            }
        }
        self.buffer.unmap();
        Ok(())
    }
}

/// Bytes per row in a texture-to-buffer copy: four per pixel, rounded up to
/// wgpu's required alignment.
fn padded_row_bytes(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    (width.saturating_mul(4)).div_ceil(align) * align
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
    let bind_group =
        create_frame_bind_group(device, bind_group_layout, sampler, uniform_buffer, &view);
    (texture, view, bind_group)
}

fn create_frame_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buffer: &wgpu::Buffer,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("emu198x-native-video bind group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
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
    })
}

/// The frame's pixels as RGBA, expanding an indexed frame through its palette
/// into `scratch`. Hosts that draw frames without this presenter use it too.
///
/// # Errors
///
/// Returns an error for a frame whose data does not match its size, or an
/// indexed frame without a palette or with an index outside it.
pub fn frame_rgba_pixels<'a>(
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
    let fractional_scale = x_scale.min(y_scale);
    // The host rounds aspect-corrected window widths to physical pixels.
    // Allow that half-pixel rounding before choosing an integer scale;
    // otherwise a requested 4× window can accidentally select 3×.
    let integer_scale = ((surface_width as f32 + 0.5) / effective_width)
        .floor()
        .min(y_scale.floor());
    let scale = if integer_scale >= 1.0 {
        integer_scale
    } else {
        fractional_scale
    };
    let width = (effective_width * scale).round().min(surface_width as f32);
    let height = (frame_height as f32 * scale)
        .round()
        .min(surface_height as f32);

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
            signal: None,
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
    fn offscreen_raw_frames_scale_by_whole_pixels() {
        // Four colours in a 2 × 2 frame, drawn raw into a 4 × 4 picture:
        // each source pixel must come back as a solid 2 × 2 block.
        let colours: [[u8; 4]; 4] = [
            [0xFF, 0x00, 0x00, 0xFF],
            [0x00, 0xFF, 0x00, 0xFF],
            [0x00, 0x00, 0xFF, 0xFF],
            [0xFF, 0xFF, 0xFF, 0xFF],
        ];
        let frame = CapturedFrame {
            signal: None,
            timestamp: MachineTime::new(0),
            format: PixelFormat::Rgba8888,
            width: 2,
            height: 2,
            palette: None,
            pixels: colours.concat(),
        };
        let mut presenter = match WgpuVideoPresenter::new_offscreen(4, 4, 2, 2) {
            Ok(presenter) => presenter,
            Err(VideoPresenterError::NoAdapter) => {
                emu198x_test_skip::skip!("no GPU adapter for an offscreen presenter");
            }
            Err(err) => panic!("offscreen presenter: {err}"),
        };
        assert!(
            presenter
                .present_with_status(&frame, &PresentationProfile::raw())
                .expect("present")
        );
        let (pixels, width, height) = presenter.offscreen_pixels().expect("offscreen");
        assert_eq!((width, height), (4, 4));
        let at = |x: usize, y: usize| &pixels[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        for (x, y, colour) in [
            (0, 0, 0),
            (1, 1, 0),
            (2, 0, 1),
            (3, 1, 1),
            (0, 2, 2),
            (1, 3, 2),
            (2, 2, 3),
            (3, 3, 3),
        ] {
            assert_eq!(at(x, y), colours[colour], "pixel ({x}, {y})");
        }
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
    fn modern_displays_decode_the_connection_without_crt_effects() {
        for (modern, crt) in [
            (VideoFilter::Modern, VideoFilter::Signal),
            (VideoFilter::ModernMonitor, VideoFilter::Monitor),
        ] {
            assert!(modern.uses_signal());
            assert_eq!(
                modern.uses_monitor_connection(),
                crt.uses_monitor_connection()
            );
            assert_eq!(modern.shader_value(), VideoFilter::Raw.shader_value());
            assert_ne!(modern.shader_value(), crt.shader_value());
            assert_eq!(
                PresentationProfile::for_filter(modern).scaling,
                ScalingMode::Integer
            );
        }
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
    fn rounded_window_width_does_not_lose_an_integer_scale_step() {
        // Aspect-corrected windows rounded to whole physical pixels.
        // Previously their width fit was just below the requested scale,
        // leaving an entire source row count of black space vertically.
        for (frame_width, frame_height, par, scale, surface_width) in [
            (352, 296, 1.054_945_1, 4, 1485),
            (416, 312, 0.936_898_05, 3, 1169),
            (256, 240, 1.142_860_4, 4, 1170),
        ] {
            let surface_height = frame_height * scale;
            let viewport = viewport_for(
                surface_width,
                surface_height,
                frame_width,
                frame_height,
                ScalingMode::Integer,
                par,
            );
            assert_eq!(viewport.height, surface_height as f32);
            assert_eq!(viewport.width, surface_width as f32);
            assert_eq!(viewport.x, 0.0);
            assert_eq!(viewport.y, 0.0);

            // A genuinely narrower window still selects the smaller scale.
            let narrower = viewport_for(
                surface_width - 2,
                surface_height,
                frame_width,
                frame_height,
                ScalingMode::Integer,
                par,
            );
            assert_eq!(narrower.height, (frame_height * (scale - 1)) as f32);
        }
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
