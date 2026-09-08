//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sega Master System / Game Gear's first native window, on the shared
//! `emu198x-ui` harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VDP
//! audio, and keyboard/gamepad input. The SMS is a console — its pad is the
//! harness's console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) —
//! plus the single Pause button, which the runtime takes as an
//! [`InputEvent::Key`] (`pause` on the SMS, `start` on the Game Gear), routed
//! through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo feature;
//! the shared launcher opens the window when no automation flag is given.

use emu198x_shell::FamilyRuntime;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_sega_game_gear::SmsRuntime;

use crate::app::GameGear;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names the class runtime's `controller_bit` maps to the pad's
/// button 1 / button 2. A real gamepad reaches these through the same
/// map; the keyboard does via [`UiSystem::map_key`].
const SMS_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

/// The Sega Game Gear as a [`UiSystem`] for the shared harness.
/// The variant is fixed at construction; a hard reset rebuilds the machine from
/// the cartridge the runtime already holds.
pub struct GameGearSystem;

impl UiApp for GameGear {
    type System = GameGearSystem;

    fn ui_system(&self) -> GameGearSystem {
        GameGearSystem
    }
}

impl UiSystem for GameGearSystem {
    type Runtime = SmsRuntime;

    fn window_title(&self) -> String {
        "Emu198x Sega Game Gear".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The Game Gear is a square-pixel LCD, so it needs no aspect correction.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((160, 144))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / 60.0)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SMS_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::South,
            KeyCode::KeyX => HostControl::East,
            _ => return None,
        })
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // The single console button. The Game Gear labels it Start where the
        // Master System labels it Pause; the runtime takes it as a named key
        // event either way.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["start"]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Game Gear labels the console button Start, where its Master System
    /// sibling labels it Pause.
    #[test]
    fn pad_maps_and_console_button_is_start() {
        let gg = GameGearSystem;
        assert_eq!(gg.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(gg.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(gg.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(gg.map_keys(KeyCode::Enter), Some(&["start"][..]));
    }
}
