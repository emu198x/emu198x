//! Shared wgpu renderer for emulator frontends.
//!
//! Provides a fullscreen-triangle renderer that uploads a CPU-side ARGB32
//! framebuffer to a GPU texture each frame and draws it with nearest-neighbour
//! filtering. All emulator binaries share this code — only the framebuffer
//! dimensions differ.

#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use wgpu;
use winit::window::Window;

/// GPU renderer for emulator framebuffers.
///
/// Owns the wgpu device, surface, pipeline, and framebuffer texture.
/// Call [`upload_framebuffer`](Renderer::upload_framebuffer) each frame with
/// the emulator's ARGB32 pixel data, then [`render`](Renderer::render) to
/// present it.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    texture: wgpu::Texture,
    rgba_buf: Vec<u8>,
    fb_width: u32,
    fb_height: u32,
}

impl Renderer {
    /// Create a new renderer for the given window and framebuffer dimensions.
    ///
    /// # Panics
    ///
    /// Panics if wgpu cannot find a suitable adapter or create a device.
    #[must_use]
    pub fn new(window: Arc<Window>, fb_width: u32, fb_height: u32) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }))
            .expect("find adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("emu-core renderer"),
                ..Default::default()
            },
        ))
        .expect("create device");

        let inner = window.inner_size();
        let surface_config = surface
            .get_default_config(&adapter, inner.width.max(1), inner.height.max(1))
            .expect("surface config");
        surface.configure(&device, &surface_config);

        // Framebuffer texture — updated each frame.
        let fb_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer"),
            size: wgpu::Extent3d {
                width: fb_width,
                height: fb_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let fb_view = fb_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let fb_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Bind group layout + bind group.
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("display"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("display"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fb_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&fb_sampler),
                },
            ],
        });

        // Shader + pipeline.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("display"),
            source: wgpu::ShaderSource::Wgsl(include_str!("display.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("display"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("display"),
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
                    format: surface_config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let rgba_buf = vec![0u8; (fb_width * fb_height * 4) as usize];

        Self {
            device,
            queue,
            surface,
            surface_config,
            pipeline,
            bind_group,
            texture: fb_texture,
            rgba_buf,
            fb_width,
            fb_height,
        }
    }

    /// Convert an ARGB32 framebuffer to RGBA8 and upload to the GPU texture.
    pub fn upload_framebuffer(&mut self, fb: &[u32]) {
        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            self.rgba_buf[offset] = ((argb >> 16) & 0xFF) as u8;
            self.rgba_buf[offset + 1] = ((argb >> 8) & 0xFF) as u8;
            self.rgba_buf[offset + 2] = (argb & 0xFF) as u8;
            self.rgba_buf[offset + 3] = 0xFF;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.rgba_buf,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.fb_width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: self.fb_width,
                height: self.fb_height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Present the current framebuffer texture to the window surface.
    pub fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("display"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Reconfigure the surface after a window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
}
