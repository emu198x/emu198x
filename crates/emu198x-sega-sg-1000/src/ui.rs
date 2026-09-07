//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Sega SG-1000's first native window, on the shared `emu198x-ui` harness:
//! wgpu video with `raw`/`lcd`/`crt` filters, framed PSG audio, and
//! keyboard/gamepad input. The SG-1000 is a console — its pad is the harness's
//! console path ([`UiSystem::map_key`] + [`UiSystem::button_map`]) — plus the
//! Pause button, which the runtime takes as an [`InputEvent::Key`] (`pause`),
//! routed through [`UiSystem::map_keys`]. Compiled only with the `ui` Cargo
//! feature; the shared launcher opens the window when no automation flag is
//! given.

use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use runtime_sega_sg_1000::Sg1000Runtime;

use crate::app::{Region, Sg1000};

const DEFAULT_SCALE: u32 = 3;

/// Player-1 control pad: directions plus the two face buttons. `south`/`east`
/// are the names `runtime-sega-sg-1000`'s `apply_button` maps to the pad's
/// button 1 / button 2. A real gamepad reaches these through the same map; the
/// keyboard does via [`UiSystem::map_key`].
const SG1000_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "south")),
    (HostControl::East, ButtonTarget::new(1, "east")),
]);

/// The Sega SG-1000 as a [`UiSystem`] for the shared harness. The region is
/// fixed at construction; a hard reset rebuilds the machine from the cartridge
/// the runtime already holds.
pub struct Sg1000System {
    region: Region,
}

impl UiApp for Sg1000 {
    type System = Sg1000System;

    fn ui_system(&self) -> Sg1000System {
        Sg1000System {
            region: self.region,
        }
    }
}

impl UiSystem for Sg1000System {
    type Runtime = Sg1000Runtime;

    fn window_title(&self) -> String {
        "Emu198x Sega SG-1000".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The SG-1000's TMS9918 drove a 4:3 TV; its 288×240 framebuffer stretches
    // to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            // Before a machine exists, the NTSC window: 5.369318 MHz over
            // 52.148 µs by 240 lines. Was 288 x 240, a fixed border.
            .unwrap_or((280, 240))
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        self.region.frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / self.region.frame_hz())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &SG1000_BUTTON_MAP
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
        // The console Pause button — a named key event, not a pad control.
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(&["pause"]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_and_pause_map() {
        let sys = Sg1000System {
            region: Region::Ntsc,
        };
        assert_eq!(sys.map_key(KeyCode::ArrowLeft), Some(HostControl::Left));
        assert_eq!(sys.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(sys.map_key(KeyCode::KeyX), Some(HostControl::East));
        assert_eq!(sys.map_keys(KeyCode::Enter), Some(&["pause"][..]));
        // Pad keys aren't keyboard keys (no double-routing).
        assert_eq!(sys.map_keys(KeyCode::ArrowLeft), None);
    }
}
