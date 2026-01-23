mod audio;
mod crt;
mod input;
mod video;

use audio::{AudioOutput, SAMPLES_PER_FRAME};
use crt::CrtRenderer;
use gilrs::{Event, GamepadId, Gilrs};
use machine_spectrum::Spectrum48K;
use pixels::{Pixels, SurfaceTexture};
use std::collections::HashSet;
use std::fs;
use std::sync::Arc;
use std::time::Instant;
use video::{HEIGHT, NATIVE_HEIGHT, NATIVE_WIDTH, WIDTH};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct Emulator {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    crt_renderer: Option<CrtRenderer>,
    crt_enabled: bool,
    spec: Spectrum48K,
    audio_output: Option<AudioOutput>,
    audio_samples: [f32; SAMPLES_PER_FRAME],
    prev_beeper_level: bool,
    frame_count: u32,
    start_time: Instant,
    keys_pressed: HashSet<KeyCode>,
    gilrs: Gilrs,
    active_gamepad: Option<GamepadId>,
    file_to_load: Option<(String, Vec<u8>)>,
}

impl Emulator {
    fn new(file_to_load: Option<(String, Vec<u8>)>) -> Self {
        let mut spec = Spectrum48K::new();

        // Load the ROM
        let rom = fs::read("roms/48.rom").expect("Failed to load ROM");
        spec.load_rom(&rom);

        // Initialize gamepad support
        let gilrs = Gilrs::new().expect("Failed to initialize gamepad support");

        // Initialize audio output
        let audio_output = AudioOutput::new();
        if audio_output.is_none() {
            eprintln!("Warning: No audio device available, sound disabled");
        }

        Self {
            window: None,
            pixels: None,
            crt_renderer: None,
            crt_enabled: false, // Start with CRT disabled
            spec,
            audio_output,
            audio_samples: [0.0f32; SAMPLES_PER_FRAME],
            prev_beeper_level: false,
            frame_count: 0,
            start_time: Instant::now(),
            keys_pressed: HashSet::new(),
            gilrs,
            active_gamepad: None,
            file_to_load,
        }
    }

    fn toggle_crt(&mut self) {
        self.crt_enabled = !self.crt_enabled;
        let status = if self.crt_enabled { "ON" } else { "OFF" };
        println!("CRT shader: {}", status);
    }
}

impl ApplicationHandler for Emulator {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window on first resume (or when resuming from suspend on mobile)
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Spectrum (F1: Toggle CRT)")
                        .with_inner_size(PhysicalSize::new(WIDTH as u32, HEIGHT as u32)),
                )
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, Arc::clone(&window));
        let pixels = Pixels::new(NATIVE_WIDTH as u32, NATIVE_HEIGHT as u32, surface)
            .expect("Failed to create pixels");

        // Create CRT renderer using pixels' wgpu context
        let crt_renderer = CrtRenderer::new(
            pixels.device(),
            size.width.max(1),
            size.height.max(1),
            pixels.render_texture_format(),
        );

        self.window = Some(window);
        // SAFETY: pixels lifetime is tied to window which lives for the program duration
        self.pixels = Some(unsafe { std::mem::transmute(pixels) });
        self.crt_renderer = Some(crt_renderer);
        self.start_time = Instant::now();

        // Load file after window is created (so we can see any error messages)
        if let Some((file_path, data)) = self.file_to_load.take() {
            let lower = file_path.to_lowercase();
            if lower.ends_with(".sna") {
                self.spec
                    .load_sna(&data)
                    .expect("Failed to load .SNA snapshot");
                println!("Loaded snapshot: {}", file_path);
            } else if lower.ends_with(".tap") {
                self.spec.load_tape(data);
                println!("Loaded tape: {}", file_path);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(pixels) = &mut self.pixels {
                        pixels.resize_surface(size.width, size.height).ok();

                        // Resize CRT renderer texture
                        if let Some(crt) = &mut self.crt_renderer {
                            crt.resize(
                                pixels.device(),
                                size.width,
                                size.height,
                                pixels.render_texture_format(),
                            );
                        }
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            // Toggle CRT on F1
                            if keycode == KeyCode::F1 && !event.repeat {
                                self.toggle_crt();
                            }
                            self.keys_pressed.insert(keycode);
                        }
                        ElementState::Released => {
                            self.keys_pressed.remove(&keycode);
                        }
                    }
                    // Check for Escape to exit
                    if keycode == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                // Render current frame
                if let Some(pixels) = &mut self.pixels {
                    let render_result = if self.crt_enabled {
                        // Render with CRT shader
                        if let Some(crt) = &self.crt_renderer {
                            let time = self.start_time.elapsed().as_secs_f32();
                            pixels.render_with(|encoder, render_target, context| {
                                // First, render the scaled pixel buffer to the CRT's texture
                                context.scaling_renderer.render(encoder, crt.texture_view());

                                // Update CRT uniforms
                                crt.update(&context.queue, time);

                                // Then render the CRT effect to the screen
                                crt.render(
                                    encoder,
                                    render_target,
                                    context.scaling_renderer.clip_rect(),
                                );

                                Ok(())
                            })
                        } else {
                            pixels.render()
                        }
                    } else {
                        // Render without CRT shader
                        pixels.render()
                    };

                    if render_result.is_err() {
                        event_loop.exit();
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Skip if window not yet created
        if self.window.is_none() {
            return;
        }

        // Poll gamepad events to track active gamepad
        while let Some(Event { id, .. }) = self.gilrs.next_event() {
            self.active_gamepad = Some(id);
        }

        // Update Spectrum keyboard from pressed keys
        self.spec.reset_keyboard();
        for &key in &self.keys_pressed {
            for &(row, bit) in input::keyboard::map_key(key) {
                self.spec.key_down(row, bit);
            }
        }

        // Combine keyboard and gamepad input for Kempston joystick
        let kempston = input::joystick::map_keyboard(&self.keys_pressed)
            | input::joystick::map_gamepad(&self.gilrs, self.active_gamepad);
        self.spec.set_kempston(kempston);

        // Run one frame
        self.spec.run_frame();

        // Generate audio from beeper transitions (this blocks for pacing)
        if let Some(ref mut audio) = self.audio_output {
            audio::generate_frame_samples(
                self.spec.beeper_transitions(),
                self.prev_beeper_level,
                &mut self.audio_samples,
            );
            audio.push_samples(&self.audio_samples);
        }
        self.prev_beeper_level = self.spec.beeper_level();

        // Render to pixels buffer
        if let Some(pixels) = &mut self.pixels {
            let flash_swap = (self.frame_count / 16) % 2 == 1;
            video::render_screen(
                self.spec.screen(),
                self.spec.border(),
                flash_swap,
                pixels.frame_mut(),
            );
        }
        self.frame_count = self.frame_count.wrapping_add(1);

        // Request redraw
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    // Load file if provided (supports .tap and .sna)
    let file_to_load = std::env::args().nth(1).map(|file_path| {
        let data = fs::read(&file_path).expect("Failed to load file");
        let lower = file_path.to_lowercase();

        if !lower.ends_with(".sna") && !lower.ends_with(".tap") {
            eprintln!("Unknown file type: {}", file_path);
            eprintln!("Supported formats: .tap, .sna");
            std::process::exit(1);
        }

        (file_path, data)
    });

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut emulator = Emulator::new(file_to_load);
    event_loop.run_app(&mut emulator).expect("Event loop error");
}
