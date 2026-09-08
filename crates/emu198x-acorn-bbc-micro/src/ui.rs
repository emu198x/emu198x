//! Interactive UI mode — the default when no automation flag is present.
//!
//! The BBC Micro's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed SN76489 audio, and
//! keyboard/gamepad input. The BBC is keyboard-led; its cursor keys are genuine
//! matrix cells, so they type. The analogue joystick's fire button is reached
//! by a real gamepad through [`UiSystem::button_map`] (the proportional axes go
//! through a μPD7002 ADC path the harness gamepad doesn't drive yet). Compiled
//! only with the `ui` Cargo feature; the shared launcher opens the window when
//! no automation flag is given.

use emu198x_shell::FamilyRuntime;
use std::time::Duration;

use emu198x_shell::launch::LaunchError;
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_acorn_bbc_micro::BbcMicroRuntime;

use crate::app::Bbc;

const DEFAULT_SCALE: u32 = 3;
const PAL_FRAME_HZ: f64 = 50.0;
/// The analogue joystick's fire button. The proportional X/Y axes are read
/// through the μPD7002 ADC (a separate `Axis` path the harness gamepad doesn't
/// drive yet), so only fire is mapped here.
const BBC_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// The BBC Micro as a [`UiSystem`] for the shared harness. Single-model; a hard
/// reset rebuilds the machine from the firmware the runtime already holds.
pub struct BbcSystem;

impl UiApp for Bbc {
    type System = BbcSystem;

    fn ui_system(&self) -> BbcSystem {
        BbcSystem
    }

    /// The window boots to a language: BASIC goes into the highest-priority
    /// sideways bank (15) when a ROM is staged, so the machine comes up at
    /// the BASIC prompt rather than the bare MOS. Explicit `--sideways`
    /// banks are installed afterwards, so they win.
    fn build_ui_runtime(&self) -> Result<BbcMicroRuntime, LaunchError> {
        let mut runtime = self.build_with_basic()?;
        self.insert_sideways_roms(&mut runtime)?;
        Ok(runtime)
    }
}

impl UiSystem for BbcSystem {
    type Runtime = BbcMicroRuntime;

    fn window_title(&self) -> String {
        "Emu198x BBC Micro".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The BBC drove a 4:3 TV / monitor; its framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((640, 256))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / PAL_FRAME_HZ)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &BBC_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_bbc_keys(code)
    }
}

/// Map a physical host key to its BBC key name (matched by
/// `runtime-acorn-bbc-micro`'s `key_to_matrix`). The cursor keys are genuine
/// matrix cells, so they type. The red function keys f0-f9 map from host
/// F1-F10. Symbols shifted on a modern host are omitted. The BBC's own Escape
/// key is unreachable — the harness owns Esc for quit.
fn map_bbc_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::KeyA => &["a"],
        KeyCode::KeyB => &["b"],
        KeyCode::KeyC => &["c"],
        KeyCode::KeyD => &["d"],
        KeyCode::KeyE => &["e"],
        KeyCode::KeyF => &["f"],
        KeyCode::KeyG => &["g"],
        KeyCode::KeyH => &["h"],
        KeyCode::KeyI => &["i"],
        KeyCode::KeyJ => &["j"],
        KeyCode::KeyK => &["k"],
        KeyCode::KeyL => &["l"],
        KeyCode::KeyM => &["m"],
        KeyCode::KeyN => &["n"],
        KeyCode::KeyO => &["o"],
        KeyCode::KeyP => &["p"],
        KeyCode::KeyQ => &["q"],
        KeyCode::KeyR => &["r"],
        KeyCode::KeyS => &["s"],
        KeyCode::KeyT => &["t"],
        KeyCode::KeyU => &["u"],
        KeyCode::KeyV => &["v"],
        KeyCode::KeyW => &["w"],
        KeyCode::KeyX => &["x"],
        KeyCode::KeyY => &["y"],
        KeyCode::KeyZ => &["z"],
        KeyCode::Digit0 => &["0"],
        KeyCode::Digit1 => &["1"],
        KeyCode::Digit2 => &["2"],
        KeyCode::Digit3 => &["3"],
        KeyCode::Digit4 => &["4"],
        KeyCode::Digit5 => &["5"],
        KeyCode::Digit6 => &["6"],
        KeyCode::Digit7 => &["7"],
        KeyCode::Digit8 => &["8"],
        KeyCode::Digit9 => &["9"],
        KeyCode::Minus => &["-"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::BracketLeft => &["["],
        KeyCode::BracketRight => &["]"],
        KeyCode::Backslash => &["\\"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Tab => &["tab"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::End => &["copy"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::CapsLock => &["caps"],
        // The red function keys f0-f9 sit on host F1-F10.
        KeyCode::F1 => &["f0"],
        KeyCode::F2 => &["f1"],
        KeyCode::F3 => &["f2"],
        KeyCode::F4 => &["f3"],
        KeyCode::F5 => &["f4"],
        KeyCode::F6 => &["f5"],
        KeyCode::F7 => &["f6"],
        KeyCode::F8 => &["f7"],
        KeyCode::F9 => &["f8"],
        KeyCode::F10 => &["f9"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{FirmwareOverrides, MachineCore, ResetKind};

    #[test]
    fn window_defaults_to_basic_and_explicit_sideways_roms_win() {
        let dir = std::env::temp_dir().join(format!("bbc-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directory");
        for (name, bytes) in [
            ("os.rom", vec![0x4c; 16384]),
            ("basic.rom", vec![0x42; 16384]),
            ("saa5050.rom", vec![0x3c; 960]),
            ("dfs.rom", vec![0x5a; 16384]),
        ] {
            std::fs::write(dir.join(name), bytes).expect("firmware");
        }
        let mut app = Bbc {
            firmware: FirmwareOverrides {
                dir: Some(dir.clone()),
                ..FirmwareOverrides::none()
            },
            sideways: Vec::new(),
        };
        let mut runtime = app.build_ui_runtime().expect("window runtime");
        assert!(app.ui_system().variants().is_empty());
        assert_eq!(app.ui_system().frame_ticks(&runtime), 39_936);
        let machine = runtime.machine_mut().expect("machine");
        machine.poke(0xfe30, 15);
        assert_eq!(machine.peek(0x8000), 0x42);
        app.sideways.push((15, dir.join("dfs.rom")));
        let mut runtime = app.build_ui_runtime().expect("overridden language");
        runtime.reset(ResetKind::Hard);
        let machine = runtime.machine_mut().expect("machine");
        machine.poke(0xfe30, 15);
        assert_eq!(machine.peek(0x8000), 0x5a);
        std::fs::remove_file(dir.join("dfs.rom")).expect("remove sideways ROM");
        assert!(app.build_ui_runtime().is_err());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn cursor_keys_type_and_function_keys_map_to_red_keys() {
        assert_eq!(map_bbc_keys(KeyCode::ArrowUp), Some(&["up"][..]));
        assert_eq!(map_bbc_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_bbc_keys(KeyCode::Enter), Some(&["return"][..]));
        // Host F1 is the BBC's red f0.
        assert_eq!(map_bbc_keys(KeyCode::F1), Some(&["f0"][..]));
        assert_eq!(map_bbc_keys(KeyCode::F10), Some(&["f9"][..]));
        assert_eq!(map_bbc_keys(KeyCode::End), Some(&["copy"][..]));
        // Keys with no BBC position are ignored.
        assert_eq!(map_bbc_keys(KeyCode::PageUp), None);
    }
}
