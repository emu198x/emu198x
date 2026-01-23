//! Generic runner for emulated machines.
//!
//! Provides the main window, input handling, and run loop for any Machine.

use crate::audio::AudioOutput;
use crate::crt::CrtRenderer;
use emu_core::{JoystickState, KeyCode, Machine};
use gilrs::{Axis, Button, Event, GamepadId, Gilrs};
use pixels::{Pixels, SurfaceTexture};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::KeyCode as WinitKeyCode;
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

/// Configuration for the runner.
pub struct RunnerConfig {
    /// Window title.
    pub title: String,
    /// Integer scale factor for sharp pixels.
    pub scale: u32,
    /// Whether CRT shader is enabled by default.
    pub crt_enabled: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            title: "Emulator".to_string(),
            scale: 3,
            crt_enabled: false,
        }
    }
}

/// Run an emulated machine with the given configuration.
pub fn run<M: Machine + 'static>(machine: M, config: RunnerConfig) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut runner = Runner::new(machine, config);
    event_loop.run_app(&mut runner).expect("Event loop error");
}

/// Generic runner that handles the window and main loop for any Machine.
pub struct Runner<M: Machine> {
    machine: M,
    config: RunnerConfig,
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    crt_renderer: Option<CrtRenderer>,
    crt_enabled: bool,
    audio_output: Option<AudioOutput>,
    audio_samples: Vec<f32>,
    frame_count: u32,
    start_time: Instant,
    keys_pressed: HashSet<WinitKeyCode>,
    gilrs: Gilrs,
    active_gamepad: Option<GamepadId>,
}

impl<M: Machine> Runner<M> {
    /// Create a new runner for the given machine.
    pub fn new(machine: M, config: RunnerConfig) -> Self {
        let gilrs = Gilrs::new().expect("Failed to initialize gamepad support");
        let samples_per_frame = machine.audio_config().samples_per_frame;

        Self {
            machine,
            crt_enabled: config.crt_enabled,
            config,
            window: None,
            pixels: None,
            crt_renderer: None,
            audio_output: None,
            audio_samples: vec![0.0; samples_per_frame],
            frame_count: 0,
            start_time: Instant::now(),
            keys_pressed: HashSet::new(),
            gilrs,
            active_gamepad: None,
        }
    }

    fn toggle_crt(&mut self) {
        self.crt_enabled = !self.crt_enabled;
        let status = if self.crt_enabled { "ON" } else { "OFF" };
        println!("CRT shader: {}", status);
    }

    fn update_joystick(&mut self) {
        // Get keyboard joystick state
        let mut state = JoystickState::default();

        if self.keys_pressed.contains(&WinitKeyCode::Numpad6) {
            state.right = true;
        }
        if self.keys_pressed.contains(&WinitKeyCode::Numpad4) {
            state.left = true;
        }
        if self.keys_pressed.contains(&WinitKeyCode::Numpad2) {
            state.down = true;
        }
        if self.keys_pressed.contains(&WinitKeyCode::Numpad8) {
            state.up = true;
        }
        if self.keys_pressed.contains(&WinitKeyCode::Numpad0)
            || self.keys_pressed.contains(&WinitKeyCode::AltLeft)
            || self.keys_pressed.contains(&WinitKeyCode::AltRight)
        {
            state.fire = true;
        }

        // Combine with gamepad state
        if let Some(id) = self.active_gamepad {
            if let Some(gamepad) = self.gilrs.connected_gamepad(id) {
                // D-pad
                if gamepad.is_pressed(Button::DPadRight) {
                    state.right = true;
                }
                if gamepad.is_pressed(Button::DPadLeft) {
                    state.left = true;
                }
                if gamepad.is_pressed(Button::DPadDown) {
                    state.down = true;
                }
                if gamepad.is_pressed(Button::DPadUp) {
                    state.up = true;
                }

                // Left analog stick
                const AXIS_THRESHOLD: f32 = 0.5;
                if let Some(axis) = gamepad.axis_data(Axis::LeftStickX) {
                    if axis.value() > AXIS_THRESHOLD {
                        state.right = true;
                    } else if axis.value() < -AXIS_THRESHOLD {
                        state.left = true;
                    }
                }
                if let Some(axis) = gamepad.axis_data(Axis::LeftStickY) {
                    if axis.value() > AXIS_THRESHOLD {
                        state.up = true;
                    } else if axis.value() < -AXIS_THRESHOLD {
                        state.down = true;
                    }
                }

                // Fire buttons
                if gamepad.is_pressed(Button::South)
                    || gamepad.is_pressed(Button::East)
                    || gamepad.is_pressed(Button::West)
                    || gamepad.is_pressed(Button::North)
                    || gamepad.is_pressed(Button::LeftTrigger)
                    || gamepad.is_pressed(Button::RightTrigger)
                    || gamepad.is_pressed(Button::LeftTrigger2)
                    || gamepad.is_pressed(Button::RightTrigger2)
                {
                    state.fire = true;
                }
            }
        }

        self.machine.set_joystick(0, state);
    }
}

impl<M: Machine> ApplicationHandler for Runner<M> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window on first resume (or when resuming from suspend on mobile)
        if self.window.is_some() {
            return;
        }

        let video_config = self.machine.video_config();
        let scaled_width = video_config.width * self.config.scale;
        let scaled_height = video_config.height * self.config.scale;

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(format!("{} (F1: Toggle CRT)", self.config.title))
                        .with_inner_size(LogicalSize::new(scaled_width, scaled_height)),
                )
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, Arc::clone(&window));
        let pixels = Pixels::new(video_config.width, video_config.height, surface)
            .expect("Failed to create pixels");

        // Create CRT renderer using pixels' wgpu context
        let crt_renderer = CrtRenderer::new(
            pixels.device(),
            size.width.max(1),
            size.height.max(1),
            pixels.render_texture_format(),
        );

        // Initialize audio output
        let audio_config = self.machine.audio_config();
        let audio_output =
            AudioOutput::new(audio_config.sample_rate, audio_config.samples_per_frame);
        if audio_output.is_none() {
            eprintln!("Warning: No audio device available, sound disabled");
        }

        self.window = Some(window);
        // SAFETY: pixels lifetime is tied to window which lives for the program duration
        self.pixels = Some(unsafe { std::mem::transmute(pixels) });
        self.crt_renderer = Some(crt_renderer);
        self.audio_output = audio_output;
        self.start_time = Instant::now();
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
                            if keycode == WinitKeyCode::F1 && !event.repeat {
                                self.toggle_crt();
                            }

                            // Track pressed keys and notify machine
                            if !self.keys_pressed.contains(&keycode) {
                                self.keys_pressed.insert(keycode);
                                if let Some(key) = convert_keycode(keycode) {
                                    self.machine.key_down(key);
                                }
                            }
                        }
                        ElementState::Released => {
                            self.keys_pressed.remove(&keycode);
                            if let Some(key) = convert_keycode(keycode) {
                                self.machine.key_up(key);
                            }
                        }
                    }
                    // Check for Escape to exit
                    if keycode == WinitKeyCode::Escape && event.state == ElementState::Pressed {
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

        // Update joystick state
        self.update_joystick();

        // Run one frame
        self.machine.run_frame();

        // Generate and output audio (this blocks for pacing)
        self.machine.generate_audio(&mut self.audio_samples);
        if let Some(ref mut audio) = self.audio_output {
            audio.push_samples(&self.audio_samples);
        }

        // Render to pixels buffer
        if let Some(pixels) = &mut self.pixels {
            self.machine.render(pixels.frame_mut());
        }
        self.frame_count = self.frame_count.wrapping_add(1);

        // Request redraw
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Convert winit KeyCode to our internal KeyCode.
fn convert_keycode(keycode: WinitKeyCode) -> Option<KeyCode> {
    match keycode {
        // Letters
        WinitKeyCode::KeyA => Some(KeyCode::KeyA),
        WinitKeyCode::KeyB => Some(KeyCode::KeyB),
        WinitKeyCode::KeyC => Some(KeyCode::KeyC),
        WinitKeyCode::KeyD => Some(KeyCode::KeyD),
        WinitKeyCode::KeyE => Some(KeyCode::KeyE),
        WinitKeyCode::KeyF => Some(KeyCode::KeyF),
        WinitKeyCode::KeyG => Some(KeyCode::KeyG),
        WinitKeyCode::KeyH => Some(KeyCode::KeyH),
        WinitKeyCode::KeyI => Some(KeyCode::KeyI),
        WinitKeyCode::KeyJ => Some(KeyCode::KeyJ),
        WinitKeyCode::KeyK => Some(KeyCode::KeyK),
        WinitKeyCode::KeyL => Some(KeyCode::KeyL),
        WinitKeyCode::KeyM => Some(KeyCode::KeyM),
        WinitKeyCode::KeyN => Some(KeyCode::KeyN),
        WinitKeyCode::KeyO => Some(KeyCode::KeyO),
        WinitKeyCode::KeyP => Some(KeyCode::KeyP),
        WinitKeyCode::KeyQ => Some(KeyCode::KeyQ),
        WinitKeyCode::KeyR => Some(KeyCode::KeyR),
        WinitKeyCode::KeyS => Some(KeyCode::KeyS),
        WinitKeyCode::KeyT => Some(KeyCode::KeyT),
        WinitKeyCode::KeyU => Some(KeyCode::KeyU),
        WinitKeyCode::KeyV => Some(KeyCode::KeyV),
        WinitKeyCode::KeyW => Some(KeyCode::KeyW),
        WinitKeyCode::KeyX => Some(KeyCode::KeyX),
        WinitKeyCode::KeyY => Some(KeyCode::KeyY),
        WinitKeyCode::KeyZ => Some(KeyCode::KeyZ),

        // Numbers
        WinitKeyCode::Digit0 => Some(KeyCode::Digit0),
        WinitKeyCode::Digit1 => Some(KeyCode::Digit1),
        WinitKeyCode::Digit2 => Some(KeyCode::Digit2),
        WinitKeyCode::Digit3 => Some(KeyCode::Digit3),
        WinitKeyCode::Digit4 => Some(KeyCode::Digit4),
        WinitKeyCode::Digit5 => Some(KeyCode::Digit5),
        WinitKeyCode::Digit6 => Some(KeyCode::Digit6),
        WinitKeyCode::Digit7 => Some(KeyCode::Digit7),
        WinitKeyCode::Digit8 => Some(KeyCode::Digit8),
        WinitKeyCode::Digit9 => Some(KeyCode::Digit9),

        // Modifiers
        WinitKeyCode::ShiftLeft => Some(KeyCode::ShiftLeft),
        WinitKeyCode::ShiftRight => Some(KeyCode::ShiftRight),
        WinitKeyCode::ControlLeft => Some(KeyCode::ControlLeft),
        WinitKeyCode::ControlRight => Some(KeyCode::ControlRight),
        WinitKeyCode::AltLeft => Some(KeyCode::AltLeft),
        WinitKeyCode::AltRight => Some(KeyCode::AltRight),

        // Special
        WinitKeyCode::Enter => Some(KeyCode::Enter),
        WinitKeyCode::Space => Some(KeyCode::Space),
        WinitKeyCode::Backspace => Some(KeyCode::Backspace),
        WinitKeyCode::Tab => Some(KeyCode::Tab),
        WinitKeyCode::Escape => Some(KeyCode::Escape),

        // Arrow keys
        WinitKeyCode::ArrowUp => Some(KeyCode::ArrowUp),
        WinitKeyCode::ArrowDown => Some(KeyCode::ArrowDown),
        WinitKeyCode::ArrowLeft => Some(KeyCode::ArrowLeft),
        WinitKeyCode::ArrowRight => Some(KeyCode::ArrowRight),

        // Numpad
        WinitKeyCode::Numpad0 => Some(KeyCode::Numpad0),
        WinitKeyCode::Numpad1 => Some(KeyCode::Numpad1),
        WinitKeyCode::Numpad2 => Some(KeyCode::Numpad2),
        WinitKeyCode::Numpad3 => Some(KeyCode::Numpad3),
        WinitKeyCode::Numpad4 => Some(KeyCode::Numpad4),
        WinitKeyCode::Numpad5 => Some(KeyCode::Numpad5),
        WinitKeyCode::Numpad6 => Some(KeyCode::Numpad6),
        WinitKeyCode::Numpad7 => Some(KeyCode::Numpad7),
        WinitKeyCode::Numpad8 => Some(KeyCode::Numpad8),
        WinitKeyCode::Numpad9 => Some(KeyCode::Numpad9),

        // Function keys
        WinitKeyCode::F1 => Some(KeyCode::F1),
        WinitKeyCode::F2 => Some(KeyCode::F2),
        WinitKeyCode::F3 => Some(KeyCode::F3),
        WinitKeyCode::F4 => Some(KeyCode::F4),
        WinitKeyCode::F5 => Some(KeyCode::F5),
        WinitKeyCode::F6 => Some(KeyCode::F6),
        WinitKeyCode::F7 => Some(KeyCode::F7),
        WinitKeyCode::F8 => Some(KeyCode::F8),
        WinitKeyCode::F9 => Some(KeyCode::F9),
        WinitKeyCode::F10 => Some(KeyCode::F10),
        WinitKeyCode::F11 => Some(KeyCode::F11),
        WinitKeyCode::F12 => Some(KeyCode::F12),

        _ => None,
    }
}
