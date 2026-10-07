//! Render the production WGSL unmodified, offscreen, for an honest baseline.
use std::{error::Error, fs, sync::mpsc};
mod offscreen_crt;

fn main() -> Result<(), Box<dyn Error>> {
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: render-crt INPUT.rgba OUTPUT.rgba".into());
    }
    let (w, h) = (352u32, 296u32);
    let (ow, oh) = (offscreen_crt::WIDTH, offscreen_crt::HEIGHT);
    let pixels = fs::read(&args[1])?;
    if pixels.len() != (w * h * 4) as usize {
        return Err("expected 352 by 296 RGBA".into());
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?;
    eprintln!("CRT comparison GPU: {}", adapter.get_info().name);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let make_texture = |width, height, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let source = make_texture(
        w,
        h,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    queue.write_texture(
        source.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    let crt = offscreen_crt::CrtStage::new(&device, &queue, &source);
    let stride = (ow * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride * oh),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut commands = device.create_command_encoder(&Default::default());
    crt.draw(&mut commands);
    commands.copy_texture_to_buffer(
        crt.output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(oh),
            },
        },
        wgpu::Extent3d {
            width: ow,
            height: oh,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([commands.finish()]);
    let (tx, rx) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = readback.slice(..).get_mapped_range()?;
    let mut rgba = Vec::with_capacity((ow * oh * 4) as usize);
    for row in mapped.chunks_exact(stride as usize) {
        rgba.extend_from_slice(&row[..(ow * 4) as usize]);
    }
    fs::write(&args[2], rgba)?;
    drop(mapped);
    readback.unmap();
    Ok(())
}
