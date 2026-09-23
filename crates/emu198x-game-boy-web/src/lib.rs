//! Canvas-free Game Boy binding. Emulation and pacing stay in the shared host.
use emu198x_shell::{InputEvent, MediaKind};
use emu198x_web::WebMachine;
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct GameBoy {
    machine: WebMachine<GameBoyRuntime>,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl GameBoy {
    /// Loads a cartridge using the selected skipped-boot profile. No boot ROM is bundled.
    /// # Errors
    /// Returns the runtime's cartridge validation error.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(model: &str, bytes: &[u8]) -> Result<Self, String> {
        let model = Model::from_variant_id(model)
            .ok_or_else(|| format!("unknown Game Boy model: {model}"))?;
        let mut machine = WebMachine::new(GameBoyRuntime::blank(model));
        machine
            .load_media_bytes("cartridge", MediaKind::Cartridge, bytes)
            .map_err(|e| e.to_string())?;
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
    /// Rejects names outside the Game Boy controller's eight buttons.
    pub fn button(&mut self, name: &str, pressed: bool) -> Result<(), String> {
        if !matches!(
            name,
            "a" | "b" | "start" | "select" | "left" | "right" | "up" | "down"
        ) {
            return Err(format!("unknown Game Boy button: {name}"));
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

impl GameBoy {
    fn queue_button(&mut self, name: &str, pressed: bool) {
        self.machine.queue_input(InputEvent::Button {
            port: 1,
            name: name.to_owned().into(),
            pressed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_model_and_cartridge() {
        assert!(GameBoy::new("cgb", &[]).is_err());
        assert!(GameBoy::new("dmg", b"not a cartridge").is_err());
    }
    #[test]
    fn dmg_runs_the_repository_cartridge_with_stereo_output() {
        let rom =
            include_bytes!("../../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb");
        let mut game = GameBoy::new("dmg", rom).expect("synthetic cartridge");
        assert!((game.frame_ms() - 16.742_706_298_828_125).abs() < 0.000_001);
        assert_eq!(game.advance(16.0).expect("partial frame"), 0);
        assert_eq!(game.advance(1.0).expect("one frame"), 1);
        game.configure_audio(48_000).expect("browser audio");
        assert!(game.button("unknown", true).is_err());
        assert!(game.configure_audio(0).is_err());
        for _ in 0..60 {
            game.step().expect("frame");
            game.audio();
        }
        assert_eq!((game.width(), game.height()), (160, 144));
        let pixels = game.pixels();
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p != &pixels[..4]));
    }
}
