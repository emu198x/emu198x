//! Canvas-free NTSC NES binding. Emulation and pacing stay in the shared host.
use emu198x_shell::{InputEvent, MediaKind};
use emu198x_web::WebMachine;
use runtime_nintendo_nes::{Model, NesRuntime};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Nes {
    machine: WebMachine<NesRuntime>,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Nes {
    /// Loads a fresh NTSC cartridge. No firmware is required.
    /// # Errors
    /// Returns the runtime's cartridge validation error.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        let mut machine = WebMachine::new(NesRuntime::blank(Model::NesNtsc));
        machine
            .load_media_bytes("cartridge-1", MediaKind::Cartridge, bytes)
            .map_err(|e| e.to_string())?;
        machine.set_audio_enabled(false);
        Ok(Self { machine })
    }
    /// Advances with the shared bounded wall-clock pacer.
    /// # Errors
    /// Returns the runtime's execution error.
    pub fn advance(&mut self, elapsed_ms: f64) -> Result<u32, String> {
        self.machine.advance(elapsed_ms).map_err(|e| e.to_string())
    }
    /// Steps one frame for paused inspection and native/browser comparisons.
    /// # Errors
    /// Returns the runtime's execution error.
    pub fn step(&mut self) -> Result<(), String> {
        self.machine.run_one_frame().map_err(|e| e.to_string())
    }
    /// Queues controller one's A button for the next update.
    pub fn button_a(&mut self, pressed: bool) {
        self.queue_button("a", pressed);
    }
    /// Queues a named controller-one button.
    /// # Errors
    /// Rejects names outside the NES controller's eight buttons.
    pub fn button(&mut self, name: &str, pressed: bool) -> Result<(), String> {
        if !matches!(
            name,
            "a" | "b" | "start" | "select" | "left" | "right" | "up" | "down"
        ) {
            return Err(format!("unknown NES button: {name}"));
        }
        self.queue_button(name, pressed);
        Ok(())
    }
    /// Exports machine state, including cartridge RAM.
    /// # Errors
    /// Returns runtime serialisation errors.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        self.machine.save_state().map_err(|e| e.to_string())
    }
    /// Restores a compatible saved machine.
    /// # Errors
    /// Rejects invalid or incompatible state.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.machine.restore_state(bytes).map_err(|e| e.to_string())
    }
    /// Configures stereo output and enables audio for this host.
    /// # Errors
    /// Rejects sample rates outside the supported browser range.
    pub fn configure_audio(&mut self, rate: u32) -> Result<(), String> {
        if !(8_000..=192_000).contains(&rate) {
            return Err("invalid audio sample rate".into());
        }
        self.machine.configure_audio(rate, 2, rate as usize / 2);
        self.machine.set_audio_enabled(true);
        Ok(())
    }
    /// Drains interleaved stereo audio.
    pub fn audio(&mut self) -> Vec<f32> {
        self.machine.audio_drain()
    }
    /// Frame period derived from the selected machine profile.
    pub fn frame_ms(&self) -> f64 {
        self.machine.frame_ms()
    }
    /// Copies the RGBA framebuffer across the browser boundary.
    pub fn pixels(&self) -> Vec<u8> {
        self.machine.frame_rgba().to_vec()
    }
    pub fn width(&self) -> u32 {
        self.machine.frame_size().0
    }
    pub fn height(&self) -> u32 {
        self.machine.frame_size().1
    }
}

impl Nes {
    fn queue_button(&mut self, name: &str, pressed: bool) {
        self.machine.queue_input(InputEvent::Button {
            port: 1,
            name: name.to_owned().into(),
            pressed,
        });
    }
}
