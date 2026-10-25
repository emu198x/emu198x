//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Neo Geo's native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, 48 kHz stereo audio, and
//! keyboard/gamepad input. The controller's four buttons sit in a row,
//! A B C D, so a host pad's south, east, west and north buttons take them.
//! Compiled only with the `ui` Cargo feature.

use std::time::Duration;

use emu198x_shell::FamilyRuntime;
use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem};
use machine_snk_neo_geo::MASTER_HZ;
use runtime_snk_neo_geo::{FRAME_HEIGHT, FRAME_WIDTH, NeoGeoRuntime};

use crate::app::NeoGeo;

const DEFAULT_SCALE: u32 = 3;

/// Player-1 controller. The names are the ones `runtime-snk-neo-geo`'s
/// input mapping takes.
const NEO_GEO_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "a")),
    (HostControl::East, ButtonTarget::new(1, "b")),
    (HostControl::West, ButtonTarget::new(1, "c")),
    (HostControl::North, ButtonTarget::new(1, "d")),
    (HostControl::Start, ButtonTarget::new(1, "start")),
    (HostControl::Select, ButtonTarget::new(1, "select")),
]);

/// The Neo Geo as a [`UiSystem`] for the shared harness.
pub struct NeoGeoSystem;

impl UiApp for NeoGeo {
    type System = NeoGeoSystem;

    fn ui_system(&self) -> NeoGeoSystem {
        NeoGeoSystem
    }
}

impl UiSystem for NeoGeoSystem {
    type Runtime = NeoGeoRuntime;

    fn window_title(&self) -> String {
        "Emu198x SNK Neo Geo".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (FRAME_WIDTH, FRAME_HEIGHT)
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    /// A frame is 264 lines of 1536 master clocks: 59.19 Hz, not 60.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(runtime.native_frame_ticks() as f64 / MASTER_HZ as f64)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &NEO_GEO_BUTTON_MAP
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        Some(match code {
            KeyCode::ArrowUp => HostControl::Up,
            KeyCode::ArrowDown => HostControl::Down,
            KeyCode::ArrowLeft => HostControl::Left,
            KeyCode::ArrowRight => HostControl::Right,
            KeyCode::KeyZ => HostControl::South,
            KeyCode::KeyX => HostControl::East,
            KeyCode::KeyC => HostControl::West,
            KeyCode::KeyV => HostControl::North,
            KeyCode::Enter | KeyCode::NumpadEnter => HostControl::Start,
            KeyCode::ShiftRight => HostControl::Select,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_snk_neo_geo::test_support::runtime_with_program;

    #[test]
    fn pacing_and_keys() {
        let system = NeoGeoSystem;
        let runtime = runtime_with_program(&[0x60FE]);
        assert_eq!(system.framebuffer_size(&runtime), (320, 224));
        let hz = 1.0 / system.frame_duration(&runtime).as_secs_f64();
        assert!((hz - 59.19).abs() < 0.01, "{hz}");
        assert_eq!(system.map_key(KeyCode::KeyZ), Some(HostControl::South));
        assert_eq!(system.map_key(KeyCode::KeyV), Some(HostControl::North));
        assert_eq!(system.map_key(KeyCode::Enter), Some(HostControl::Start));
        assert_eq!(system.map_key(KeyCode::KeyQ), None);
    }
}
