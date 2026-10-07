//! Shared offscreen use of the unmodified production WGSL.
pub const WIDTH: u32 = 352 * 3;
pub const HEIGHT: u32 = 296 * 3;

pub struct CrtStage {
    pipeline: wgpu::RenderPipeline,
    bindings: wgpu::BindGroup,
    pub output: wgpu::Texture,
    view: wgpu::TextureView,
}

impl CrtStage {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Texture) -> Self {
        Self::with_uniforms(
            device,
            queue,
            source,
            [2.0, 352.0, 296.0, 0.0],
            WIDTH,
            HEIGHT,
        )
    }

    pub fn with_uniforms(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &wgpu::Texture,
        uniforms: [f32; 4],
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("unmodified production CRT shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../crates/emu198x-native-video/src/shader.wgsl").into(),
            ),
        });
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("offline CRT comparison"),
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
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &uniform,
            0,
            &uniforms
                .into_iter()
                .flat_map(f32::to_ne_bytes)
                .collect::<Vec<_>>(),
        );
        let source_view = source.create_view(&Default::default());
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CRT source"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("CRT output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        Self {
            pipeline,
            bindings,
            output,
            view,
        }
    }

    pub fn draw(&self, commands: &mut wgpu::CommandEncoder) {
        let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("CRT presentation"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
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
        pass.set_bind_group(0, &self.bindings, &[]);
        pass.draw(0..6, 0..1);
    }
}
