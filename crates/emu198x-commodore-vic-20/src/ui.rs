//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Commodore VIC-20's first native window, on the shared `emu198x-ui`
//! harness: wgpu video with `raw`/`lcd`/`crt` filters, framed VIC audio, and
//! keyboard/gamepad input. The VIC-20 is keyboard-led; its two real cursor
//! keys are matrix cells, so they type, and the single joystick port is reached
//! by a real gamepad through [`UiSystem::button_map`]. Compiled only with the
//! `ui` Cargo feature; the shared launcher opens the window when no automation
//! flag is given.

use emu198x_shell::{FamilyRuntime, FirmwareOverrides, MachineError, build_replacement};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_commodore_vic_20::{Model, Vic20Runtime};

use crate::app::Vic20;

const DEFAULT_SCALE: u32 = 3;

/// The VIC-20's single control port: four directions plus fire, named as
/// `runtime-commodore-vic-20`'s controller mirror expects. The cursor keys are
/// keyboard cells, so a real gamepad reaches the joystick through this map.
const VIC20_BUTTON_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);

/// Native adapter for the runtime-owned regional catalogue.
pub struct Vic20System {
    model: Model,
}

impl UiApp for Vic20 {
    type System = Vic20System;

    fn ui_system(&self) -> Vic20System {
        Vic20System { model: self.model }
    }
}

impl UiSystem for Vic20System {
    type Runtime = Vic20Runtime;

    fn window_title(&self) -> String {
        "Emu198x Commodore VIC-20".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The VIC-20 drove a 4:3 TV; its framebuffer stretches to fill it.

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.framebuffer_width(), machine.framebuffer_height()))
            .unwrap_or((230, 288))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(match runtime.model() {
            Model::Vic20Pal => 1.0 / 50.0,
            Model::Vic20Ntsc => 1.0 / 60.0,
        })
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .iter()
            .map(|model| VariantInfo::new(model.variant_id(), model.display_name()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.model.variant_id()))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        id: &str,
    ) -> Result<(), MachineError> {
        let model = Model::from_variant_id(id).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown VIC-20 variant",
        })?;
        *runtime =
            build_replacement(runtime, model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &VIC20_BUTTON_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_vic20_keys(code)
    }
}

/// Map a physical host key to its VIC-20 key name (matched by
/// `runtime-commodore-vic-20`'s `key_from_name`). The VIC-20 has only two
/// physical cursor keys (right and down — up/left are shifted), so only those
/// map; the joystick is the gamepad. Symbols that are shifted on a modern host
/// are omitted, like the other Commodore keyboards. Host Tab is RUN/STOP and
/// host Alt is the Commodore key.
fn map_vic20_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Equal => &["="],
        KeyCode::Semicolon => &[";"],
        KeyCode::Comma => &[","],
        KeyCode::Period => &["."],
        KeyCode::Slash => &["/"],
        KeyCode::NumpadAdd => &["+"],
        KeyCode::NumpadMultiply => &["*"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::Home => &["home"],
        KeyCode::Tab => &["stop"],
        KeyCode::ShiftLeft => &["shift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight => &["commodore"],
        KeyCode::F1 => &["f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F7 => &["f7"],
        // The VIC-20 has only right/down cursor keys (up/left are shifted).
        KeyCode::ArrowRight => &["crsr-right"],
        KeyCode::ArrowDown => &["crsr-down"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_constructs_both_regions_with_expansion_and_sys_startup() {
        let dir = std::env::temp_dir().join(format!("vic20-ui-catalogue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directory");
        let mut kernal = vec![0x4c; 8192];
        kernal[..3].copy_from_slice(&[0x4c, 0x00, 0xe0]);
        kernal[8188..8190].copy_from_slice(&[0x00, 0xe0]);
        std::fs::write(dir.join("kernal.rom"), kernal).expect("kernal");
        std::fs::write(dir.join("basic.rom"), vec![0x42; 8192]).expect("basic");
        std::fs::write(dir.join("chargen.rom"), vec![0x3c; 4096]).expect("char");
        std::fs::write(dir.join("sys.prg"), [0x01, 0x12, 0x5a]).expect("PRG");
        for model in Model::ALL {
            let app = Vic20 {
                model,
                firmware: FirmwareOverrides {
                    dir: Some(dir.clone()),
                    ..FirmwareOverrides::none()
                },
                ram_expansion: runtime_commodore_vic_20::Vic20RamExpansion::EXP_16K,
                prg: Some(dir.join("sys.prg")),
                prg_sys: true,
                esp_at_tcp: true,
            };
            let mut runtime = app.build_ui_runtime().expect("runtime");
            let mut system = app.ui_system();
            assert_eq!(system.variants().len(), 2);
            assert_eq!(
                system.current_variant().as_deref(),
                Some(model.variant_id())
            );
            assert_eq!(system.frame_ticks(&runtime), model.frame_ticks());
            assert_eq!(
                system.frame_duration(&runtime),
                Duration::from_secs_f64(if model == Model::Vic20Pal {
                    1.0 / 50.0
                } else {
                    1.0 / 60.0
                })
            );
            assert_eq!(runtime.ram_expansion_kb(), 16);
            assert!(runtime.esp_at_tcp_bridge().is_some());
            assert_eq!(runtime.machine().expect("machine").peek(0x1201), 0x5a);
            assert_eq!(runtime.machine().expect("machine").peek(0x277), b'S');
            assert!(system.switch_variant(&mut runtime, "unknown").is_err());
            assert!(runtime.esp_at_tcp_bridge().is_some());
        }
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn keyboard_maps_cursor_keys_and_specials() {
        assert_eq!(map_vic20_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Enter), Some(&["return"][..]));
        assert_eq!(map_vic20_keys(KeyCode::Tab), Some(&["stop"][..]));
        assert_eq!(map_vic20_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
        assert_eq!(
            map_vic20_keys(KeyCode::ArrowRight),
            Some(&["crsr-right"][..])
        );
        assert_eq!(map_vic20_keys(KeyCode::ArrowDown), Some(&["crsr-down"][..]));
        // Up/left have no unshifted cursor key on the VIC-20.
        assert_eq!(map_vic20_keys(KeyCode::ArrowUp), None);
    }
}
