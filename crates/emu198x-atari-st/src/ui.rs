//! Interactive UI mode — the default when no automation flag is present.
//!
//! The Atari ST's native window, on the shared `emu198x-ui` harness: wgpu
//! video with `raw`/`lcd`/`crt` filters, 48 kHz audio, the keyboard, the
//! mouse and a gamepad.
//!
//! - **Keyboard**: [`map_st_key`] maps each physical host key to one ST key
//!   name, which the runtime turns into a scancode. The ST's Help and Undo
//!   keys have no host equivalent, so Page Up and Page Down stand in.
//! - **Mouse**: [`mouse_device`](UiSystem::mouse_device) opts the window into
//!   pointer capture as `mouse-1`; the left and right buttons pass through.
//! - **Joystick**: a gamepad drives the joystick port, port 1.
//! - **Variants**: the four models. A switch cold-boots the new model with
//!   the launch's TOS, the same monitor and the same disks.
//!
//! Compiled only with the `ui` Cargo feature; the shared launcher opens the
//! window when no automation flag is given.

use emu198x_shell::FamilyRuntime;
use emu198x_shell::{FirmwareOverrides, MachineError, build_replacement};
use std::borrow::Cow;
use std::time::Duration;

use emu198x_ui::launch::UiApp;
use emu198x_ui::{ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo};
use runtime_atari_st::{AtariStRuntime, Model};

use crate::app::AtariSt;

const DEFAULT_SCALE: u32 = 2;
const MOUSE_DEVICE: &str = "mouse-1";

/// The joystick port's stick, from a gamepad.
const ST_JOYSTICK_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::West, ButtonTarget::new(1, "fire")),
]);

/// One physical host key as an ST key name, as `runtime-atari-st` takes
/// them.
fn map_st_key(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::Escape => &["escape"],
        KeyCode::Digit1 => &["1"],
        KeyCode::Digit2 => &["2"],
        KeyCode::Digit3 => &["3"],
        KeyCode::Digit4 => &["4"],
        KeyCode::Digit5 => &["5"],
        KeyCode::Digit6 => &["6"],
        KeyCode::Digit7 => &["7"],
        KeyCode::Digit8 => &["8"],
        KeyCode::Digit9 => &["9"],
        KeyCode::Digit0 => &["0"],
        KeyCode::Minus => &["minus"],
        KeyCode::Equal => &["equals"],
        KeyCode::Backquote => &["backquote"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Tab => &["tab"],
        KeyCode::KeyQ => &["q"],
        KeyCode::KeyW => &["w"],
        KeyCode::KeyE => &["e"],
        KeyCode::KeyR => &["r"],
        KeyCode::KeyT => &["t"],
        KeyCode::KeyY => &["y"],
        KeyCode::KeyU => &["u"],
        KeyCode::KeyI => &["i"],
        KeyCode::KeyO => &["o"],
        KeyCode::KeyP => &["p"],
        KeyCode::BracketLeft => &["leftbracket"],
        KeyCode::BracketRight => &["rightbracket"],
        KeyCode::Enter => &["return"],
        KeyCode::Delete => &["delete"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["control"],
        KeyCode::KeyA => &["a"],
        KeyCode::KeyS => &["s"],
        KeyCode::KeyD => &["d"],
        KeyCode::KeyF => &["f"],
        KeyCode::KeyG => &["g"],
        KeyCode::KeyH => &["h"],
        KeyCode::KeyJ => &["j"],
        KeyCode::KeyK => &["k"],
        KeyCode::KeyL => &["l"],
        KeyCode::Semicolon => &["semicolon"],
        KeyCode::Quote => &["quote"],
        KeyCode::Backslash => &["backslash"],
        KeyCode::ShiftLeft => &["lshift"],
        KeyCode::IntlBackslash => &["iso"],
        KeyCode::KeyZ => &["z"],
        KeyCode::KeyX => &["x"],
        KeyCode::KeyC => &["c"],
        KeyCode::KeyV => &["v"],
        KeyCode::KeyB => &["b"],
        KeyCode::KeyN => &["n"],
        KeyCode::KeyM => &["m"],
        KeyCode::Comma => &["comma"],
        KeyCode::Period => &["period"],
        KeyCode::Slash => &["slash"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::AltLeft | KeyCode::AltRight => &["alt"],
        KeyCode::Space => &["space"],
        KeyCode::CapsLock => &["capslock"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["f2"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["f4"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["f6"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["f8"],
        KeyCode::F9 => &["f9"],
        KeyCode::F10 => &["f10"],
        KeyCode::PageUp => &["help"],
        KeyCode::PageDown => &["undo"],
        KeyCode::Insert => &["insert"],
        KeyCode::Home => &["clrhome"],
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::NumpadDivide => &["kp-/"],
        KeyCode::NumpadMultiply => &["kp-*"],
        KeyCode::NumpadSubtract => &["kp-minus"],
        KeyCode::NumpadAdd => &["kp-plus"],
        KeyCode::NumpadEnter => &["kp-enter"],
        KeyCode::NumpadDecimal => &["kp-."],
        KeyCode::Numpad0 => &["kp-0"],
        KeyCode::Numpad1 => &["kp-1"],
        KeyCode::Numpad2 => &["kp-2"],
        KeyCode::Numpad3 => &["kp-3"],
        KeyCode::Numpad4 => &["kp-4"],
        KeyCode::Numpad5 => &["kp-5"],
        KeyCode::Numpad6 => &["kp-6"],
        KeyCode::Numpad7 => &["kp-7"],
        KeyCode::Numpad8 => &["kp-8"],
        KeyCode::Numpad9 => &["kp-9"],
        _ => return None,
    })
}

/// The ST as a [`UiSystem`] for the shared harness.
pub struct AtariStSystem {
    model: Model,
    firmware: FirmwareOverrides,
}

impl UiApp for AtariSt {
    type System = AtariStSystem;

    fn ui_system(&self) -> AtariStSystem {
        AtariStSystem {
            model: self.model(),
            firmware: self.firmware.clone(),
        }
    }
}

impl UiSystem for AtariStSystem {
    type Runtime = AtariStRuntime;

    fn window_title(&self) -> String {
        "Emu198x Atari ST".to_owned()
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    // The display is CPU-generated; advance whole frames so a slice never
    // captures a half-drawn picture.
    fn input_slices_per_frame(&self) -> u32 {
        1
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        runtime
            .machine()
            .map(|machine| (machine.frame_width() as u32, machine.frame_height() as u32))
            .unwrap_or((768, 258))
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    /// A frame is a frame of CPU cycles, so a PAL ST runs at 50.05 Hz, an
    /// NTSC one at 59.96 and a monochrome one at 71.47.
    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        let model = runtime.model();
        Duration::from_secs_f64(model.frame_ticks(runtime.monitor()) as f64 / model.cpu_hz() as f64)
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
            operation: "unknown atari-st variant",
        })?;
        *runtime = build_replacement(runtime, model, &self.firmware).map_err(|err| {
            MachineError::Host {
                reason: err.to_string(),
            }
        })?;
        self.model = model;
        Ok(())
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &ST_JOYSTICK_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_st_key(code)
    }

    fn mouse_device(&self) -> Option<&'static str> {
        Some(MOUSE_DEVICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_names_the_runtime_knows() {
        assert_eq!(map_st_key(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_st_key(KeyCode::PageUp), Some(&["help"][..]));
        assert_eq!(map_st_key(KeyCode::F12), None, "F12 is the harness's reset");
        let system = AtariStSystem {
            model: Model::St520Pal,
            firmware: FirmwareOverrides::none(),
        };
        assert_eq!(system.mouse_device(), Some("mouse-1"));
        assert_eq!(
            system
                .variants()
                .iter()
                .map(|v| v.id.as_ref())
                .collect::<Vec<_>>(),
            Model::VARIANT_IDS
        );
        assert_eq!(system.current_variant().as_deref(), Some("520st-pal"));
    }

    #[test]
    fn frame_duration_follows_the_model_and_monitor() {
        let system = AtariStSystem {
            model: Model::St520Pal,
            firmware: FirmwareOverrides::none(),
        };
        for (model, hz) in [(Model::St520Pal, 50.05), (Model::St520Ntsc, 59.96)] {
            let runtime = AtariStRuntime::blank(model);
            let measured = 1.0 / system.frame_duration(&runtime).as_secs_f64();
            assert!((measured - hz).abs() < 0.01, "{model:?} at {measured} Hz");
            assert_eq!(system.framebuffer_size(&runtime), (768, 258));
        }
    }
}
