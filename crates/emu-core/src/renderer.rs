//! Shared wgpu renderer for emulator frontends.
//!
//! Provides a fullscreen-triangle renderer that uploads a CPU-side ARGB32
//! framebuffer to a GPU texture each frame and draws it with selectable
//! nearest or linear filtering. All emulator binaries share this code; the
//! runner decides window scale and fullscreen mode.

#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use winit::window::Window;

/// Texture sampling mode used when scaling the emulator framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    /// Sharp nearest-neighbour scaling.
    Nearest,
    /// Smooth linear filtering.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Viewport {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

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
    nearest_bind_group: wgpu::BindGroup,
    linear_bind_group: wgpu::BindGroup,
    texture: wgpu::Texture,
    rgba_buf: Vec<u8>,
    fb_width: u32,
    fb_height: u32,
    filter_mode: FilterMode,
}

impl Renderer {
    /// Create a new renderer for the given window and framebuffer dimensions.
    ///
    /// # Panics
    ///
    /// Panics if wgpu cannot find a suitable adapter or create a device.
    #[must_use]
    pub fn new(
        window: Arc<Window>,
        fb_width: u32,
        fb_height: u32,
        filter_mode: FilterMode,
    ) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("find adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("emu-core renderer"),
            ..Default::default()
        }))
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
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Bind group layout + bind group.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let nearest_bind_group =
            create_bind_group(&device, &bind_group_layout, &fb_view, &nearest_sampler);
        let linear_bind_group =
            create_bind_group(&device, &bind_group_layout, &fb_view, &linear_sampler);

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
            nearest_bind_group,
            linear_bind_group,
            texture: fb_texture,
            rgba_buf,
            fb_width,
            fb_height,
            filter_mode,
        }
    }

    /// Update the texture filtering mode used for scaled presentation.
    pub fn set_filter_mode(&mut self, filter_mode: FilterMode) {
        self.filter_mode = filter_mode;
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
        let viewport = compute_viewport(
            self.surface_config.width,
            self.surface_config.height,
            self.fb_width,
            self.fb_height,
            self.filter_mode,
        );
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
            pass.set_viewport(
                viewport.x as f32,
                viewport.y as f32,
                viewport.width as f32,
                viewport.height as f32,
                0.0,
                1.0,
            );
            pass.set_bind_group(0, self.active_bind_group(), &[]);
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

    fn active_bind_group(&self) -> &wgpu::BindGroup {
        match self.filter_mode {
            FilterMode::Nearest => &self.nearest_bind_group,
            FilterMode::Linear => &self.linear_bind_group,
        }
    }
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("display"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn compute_viewport(
    surface_width: u32,
    surface_height: u32,
    fb_width: u32,
    fb_height: u32,
    filter_mode: FilterMode,
) -> Viewport {
    if surface_width == 0 || surface_height == 0 || fb_width == 0 || fb_height == 0 {
        return Viewport {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
    }

    let (width, height) = match filter_mode {
        // Integer scaling keeps nearest-neighbour output clean when the window
        // is larger than the native framebuffer. If the window is smaller than
        // native, fall back to exact fit so the image still remains visible.
        FilterMode::Nearest => {
            let scale_x = surface_width / fb_width;
            let scale_y = surface_height / fb_height;
            let scale = scale_x.min(scale_y);
            if scale >= 1 {
                (fb_width * scale, fb_height * scale)
            } else {
                fit_viewport(surface_width, surface_height, fb_width, fb_height)
            }
        }
        FilterMode::Linear => fit_viewport(surface_width, surface_height, fb_width, fb_height),
    };

    Viewport {
        x: (surface_width - width) / 2,
        y: (surface_height - height) / 2,
        width,
        height,
    }
}

fn fit_viewport(
    surface_width: u32,
    surface_height: u32,
    fb_width: u32,
    fb_height: u32,
) -> (u32, u32) {
    let width_limited = u64::from(surface_width) * u64::from(fb_height)
        <= u64::from(surface_height) * u64::from(fb_width);
    if width_limited {
        let height = ((u64::from(surface_width) * u64::from(fb_height)) / u64::from(fb_width))
            .max(1)
            .min(u64::from(surface_height)) as u32;
        (surface_width.max(1), height)
    } else {
        let width = ((u64::from(surface_height) * u64::from(fb_width)) / u64::from(fb_height))
            .max(1)
            .min(u64::from(surface_width)) as u32;
        (width, surface_height.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::{FilterMode, Viewport, compute_viewport};

    #[test]
    fn nearest_filter_uses_integer_scaling_when_possible() {
        assert_eq!(
            compute_viewport(1_920, 1_080, 256, 192, FilterMode::Nearest),
            Viewport {
                x: 320,
                y: 60,
                width: 1_280,
                height: 960,
            }
        );
    }

    #[test]
    fn linear_filter_fills_available_aspect_preserving_space() {
        assert_eq!(
            compute_viewport(1_920, 1_080, 256, 192, FilterMode::Linear),
            Viewport {
                x: 240,
                y: 0,
                width: 1_440,
                height: 1_080,
            }
        );
    }

    #[test]
    fn smaller_surfaces_fall_back_to_fit_scaling() {
        assert_eq!(
            compute_viewport(200, 150, 256, 192, FilterMode::Nearest),
            Viewport {
                x: 0,
                y: 0,
                width: 200,
                height: 150,
            }
        );
    }
}
