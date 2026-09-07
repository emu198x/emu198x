//! Interactive UI mode — the Commodore 64 on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke winit + wgpu runner with a thin
//! [`UiSystem`] descriptor over [`C64Runtime`]. The harness owns the window,
//! video filters, framed audio, gamepad/keyboard plumbing, the native menu,
//! save-states, tape transport, and live variant switching; this file supplies
//! only the C64-specific knobs:
//!
//! - **Keyboard**: [`map_c64_keys`] maps each physical host key to one or more
//!   C64-matrix key names (the cursor combos, the shifted function keys, the
//!   platform-key Commodore alias). In keyboard-joystick mode (toggled with
//!   Page Up) the arrow keys + Space fall through to the gameport-2 joystick.
//! - **Gamepad / keyboard-joystick**: [`C64_JOYSTICK_MAP`] drives gameport 2
//!   (port 0); every face button is the single C64 fire.
//! - **Variants**: PAL and NTSC breadbins as the Machine-menu radio. Both share
//!   the same firmware — KERNAL/BASIC/CHARGEN plus the drive DOS ROMs (1541,
//!   and the optional 1571/1581 when present) — so
//!   [`switch_variant`](UiSystem::switch_variant) rebuilds the runtime from the
//!   stashed firmware bytes via `from_firmware`.
//! - **Drives**: [`drive_ports`](UiSystem::drive_ports) /
//!   [`set_port_drive`](UiSystem::set_port_drive) back the Machine → Drives menu,
//!   letting the user pick a 1541/1571/1581 (or none) per IEC device 8–11.
//!   Models whose DOS ROM was not loaded show disabled.
//! - **Tape**: F9/F10 transport + F11 turbo come free from the harness, gated on
//!   the `tape-1` slot; [`tape_playing`](UiSystem::tape_playing) drives turbo.
//!
//! Compiled only with the `ui` Cargo feature; the shared launcher opens the
//! window when no automation flag is given, with the runtime `app.rs` built.

use std::borrow::Cow;
use std::time::Duration;

use common_commodore_c64::timing::{C64Timing, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::{FirmwareImage, FirmwareSet, MachineError};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, DriveOption, DrivePortInfo, HostControl, KeyCode, UiSystem,
    VariantInfo,
};
use runtime_commodore_c64::{C64Runtime, DriveKind, Model};

use crate::app::{C64, FirmwareBundle};

const DEFAULT_SCALE: u32 = 2;
const INPUT_SLICES_PER_FRAME: u32 = 8;

const PAL_ID: &str = "pal";
const NTSC_ID: &str = "ntsc";
const C64C_PAL_ID: &str = "c64c-pal";
const C64C_NTSC_ID: &str = "c64c-ntsc";

// Seam-2 input port convention: port 0 = C64 gameport 2 (CIA1 PA,
// the main "gameport"). See runtime-commodore-c64/src/input.rs for
// the full mapping rationale. The host gamepad's face buttons all
// route to FIRE — the C64 stick is single-fire.
const C64_JOYSTICK_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(0, "up")),
    (HostControl::Down, ButtonTarget::new(0, "down")),
    (HostControl::Left, ButtonTarget::new(0, "left")),
    (HostControl::Right, ButtonTarget::new(0, "right")),
    (HostControl::South, ButtonTarget::new(0, "fire")),
    (HostControl::East, ButtonTarget::new(0, "fire")),
    (HostControl::West, ButtonTarget::new(0, "fire")),
    (HostControl::North, ButtonTarget::new(0, "fire")),
]);

/// Maps one physical host key to one or more C64-matrix key names. Lifted
/// verbatim from the bespoke `map_c64_keys`.
fn map_c64_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["return"],
        KeyCode::Space => &["space"],
        KeyCode::Backspace | KeyCode::Delete => &["delete"],
        KeyCode::ShiftLeft => &["lshift"],
        KeyCode::ShiftRight => &["rshift"],
        KeyCode::ControlLeft | KeyCode::ControlRight => &["ctrl"],
        KeyCode::AltLeft | KeyCode::AltRight | KeyCode::SuperLeft | KeyCode::SuperRight => {
            &["commodore"]
        }
        KeyCode::ArrowRight => &["right"],
        KeyCode::ArrowLeft => &["lshift", "right"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowUp => &["lshift", "down"],
        KeyCode::Home => &["home"],
        KeyCode::F1 => &["f1"],
        KeyCode::F2 => &["lshift", "f1"],
        KeyCode::F3 => &["f3"],
        KeyCode::F4 => &["lshift", "f3"],
        KeyCode::F5 => &["f5"],
        KeyCode::F6 => &["lshift", "f5"],
        KeyCode::F7 => &["f7"],
        KeyCode::F8 => &["lshift", "f7"],
        KeyCode::Minus => &["minus"],
        KeyCode::Equal => &["equals"],
        KeyCode::Comma => &["comma"],
        KeyCode::Period => &["period"],
        KeyCode::Slash => &["slash"],
        KeyCode::Semicolon => &["semicolon"],
        KeyCode::Quote => &["colon"],
        KeyCode::BracketLeft => &["at"],
        KeyCode::BracketRight => &["asterisk"],
        KeyCode::Backslash => &["plus"],
        KeyCode::Backquote => &["leftarrow"],
        KeyCode::Tab => &["runstop"],
        _ => return None,
    })
}

/// The arrow keys + Space the keyboard-joystick mode steals from the keyboard
/// path and routes through the gameport-2 button map. Lifted from the bespoke
/// `map_c64_joystick_key`.
fn map_c64_joystick_key(code: KeyCode) -> Option<HostControl> {
    Some(match code {
        KeyCode::ArrowUp => HostControl::Up,
        KeyCode::ArrowDown => HostControl::Down,
        KeyCode::ArrowLeft => HostControl::Left,
        KeyCode::ArrowRight => HostControl::Right,
        KeyCode::Space => HostControl::South,
        _ => return None,
    })
}

// ---- The UiSystem ----------------------------------------------------------

/// The C64 as a [`UiSystem`]. Tracks the active model so the title and the
/// Machine-menu radio follow live switches, the resolved firmware (so a variant
/// switch can rebuild without re-reading ROMs), and whether the arrow keys /
/// Space currently drive the gameport-2 joystick (Page Up).
pub struct C64System {
    model: Model,
    firmware: FirmwareBundle,
    keyboard_joystick: bool,
}

impl C64System {
    fn timing(&self) -> &'static C64Timing {
        match self.model {
            Model::C64NtscBreadbin | Model::C64cNtsc => &TIMING_NTSC_BREADBIN,
            Model::C64PalBreadbin | Model::C64cPal => &TIMING_PAL_BREADBIN,
        }
    }
}

impl UiSystem for C64System {
    type Runtime = C64Runtime;

    fn window_title(&self) -> String {
        format!("Emu198x | Commodore 64 ({})", model_label(self.model))
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    fn framebuffer_size(&self, runtime: &Self::Runtime) -> (u32, u32) {
        let vic = runtime.machine().vic();
        (vic.framebuffer_width(), vic.framebuffer_height())
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        u64::from(self.timing().cycles_per_frame)
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        let timing = self.timing();
        Duration::from_secs_f64(f64::from(timing.cycles_per_frame) / timing.cpu_hz as f64)
    }

    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &C64_JOYSTICK_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // In keyboard-joystick mode the arrow keys + Space fall through to the
        // joystick path (returning `None` here so the harness routes them
        // through `map_key` + the button map instead).
        if self.keyboard_joystick && map_c64_joystick_key(code).is_some() {
            return None;
        }
        map_c64_keys(code)
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        // Only meaningful in keyboard-joystick mode; otherwise the arrow keys +
        // Space are handled as keyboard keys by `map_keys`.
        if self.keyboard_joystick {
            map_c64_joystick_key(code)
        } else {
            None
        }
    }

    fn handle_key(&mut self, _runtime: &mut Self::Runtime, code: KeyCode, pressed: bool) -> bool {
        if code == KeyCode::PageUp {
            if pressed {
                self.keyboard_joystick = !self.keyboard_joystick;
                eprintln!(
                    "input: keyboard joystick {}",
                    if self.keyboard_joystick {
                        "enabled on gameport 2"
                    } else {
                        "disabled"
                    }
                );
            }
            return true;
        }
        false
    }

    fn tape_playing(&self, runtime: &Self::Runtime) -> bool {
        runtime.machine().tape_is_playing()
    }

    /// Capture host mouse motion as `mouse-1` so a 1351 plugged in with
    /// `--mouse-1351` is drivable from the window. When no mouse is attached
    /// the runtime drops the events; the cursor is never grabbed, so
    /// keyboard/joystick users are unaffected.
    fn mouse_device(&self) -> Option<&'static str> {
        Some("mouse-1")
    }

    fn variants(&self) -> Vec<VariantInfo> {
        vec![
            VariantInfo::new(PAL_ID, model_label(Model::C64PalBreadbin)),
            VariantInfo::new(NTSC_ID, model_label(Model::C64NtscBreadbin)),
            VariantInfo::new(C64C_PAL_ID, model_label(Model::C64cPal)),
            VariantInfo::new(C64C_NTSC_ID, model_label(Model::C64cNtsc)),
        ]
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(variant_id(self.model)))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = model_for_variant(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown Commodore 64 variant",
        })?;
        // All four variants (PAL/NTSC breadbin and C64C) share the same firmware
        // — KERNAL/BASIC/CHARGEN plus whatever drive DOS ROMs were loaded (1541,
        // and the optional 1571/1581) — differing only in region and SID
        // revision, so rebuild from the stashed bytes rather than re-reading the
        // ROM files. The harness re-paces and refreshes; state/media are not
        // preserved (a hardware swap).
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &self.firmware {
            firmware.push(FirmwareImage::new(id.clone(), bytes));
        }
        *runtime = C64Runtime::from_firmware(model, &firmware)?;
        self.model = model;
        Ok(())
    }

    fn drive_ports(&self, runtime: &Self::Runtime) -> Vec<DrivePortInfo> {
        // The C64 IEC bus carries devices 8–11; each can hold a 1541, 1571, or
        // 1581 (or be empty). Availability follows the loaded DOS ROMs.
        (8u8..=11)
            .map(|device| DrivePortInfo {
                device,
                label: Cow::Owned(format!("Device {device}")),
                options: vec![
                    DriveOption {
                        id: Cow::Borrowed("none"),
                        label: Cow::Borrowed("Empty"),
                        available: true,
                    },
                    drive_option(runtime, DriveKind::C1541, "1541"),
                    drive_option(runtime, DriveKind::C1571, "1571"),
                    drive_option(runtime, DriveKind::C1581, "1581"),
                ],
                current: Cow::Borrowed(drive_kind_id(runtime.port_drive_kind(device))),
            })
            .collect()
    }

    fn set_port_drive(
        &mut self,
        runtime: &mut Self::Runtime,
        device: u8,
        kind_id: &str,
    ) -> Result<(), MachineError> {
        let kind = drive_kind_for_id(kind_id).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown drive model",
        })?;
        runtime.set_port_drive(device, kind)
    }
}

/// The Drives-menu option id for a drive model (or `"none"` for an empty port).
fn drive_kind_id(kind: Option<DriveKind>) -> &'static str {
    match kind {
        None => "none",
        Some(DriveKind::C1541) => "1541",
        Some(DriveKind::C1571) => "1571",
        Some(DriveKind::C1581) => "1581",
    }
}

/// Parse a Drives-menu option id into a drive selection. The outer `None` is an
/// unknown id; the inner `None` is the empty-port choice.
fn drive_kind_for_id(id: &str) -> Option<Option<DriveKind>> {
    match id {
        "none" => Some(None),
        "1541" => Some(Some(DriveKind::C1541)),
        "1571" => Some(Some(DriveKind::C1571)),
        "1581" => Some(Some(DriveKind::C1581)),
        _ => None,
    }
}

/// A Drives menu option for `kind`, disabled when its DOS ROM was not loaded.
fn drive_option(runtime: &C64Runtime, kind: DriveKind, id: &'static str) -> DriveOption {
    DriveOption {
        id: Cow::Borrowed(id),
        label: Cow::Borrowed(id),
        available: runtime.drive_kind_available(kind),
    }
}

/// The Machine-menu label for a model (region + SID revision).
fn model_label(model: Model) -> &'static str {
    match model {
        Model::C64PalBreadbin => "PAL Breadbin (6581)",
        Model::C64NtscBreadbin => "NTSC Breadbin (6581)",
        Model::C64cPal => "PAL C64C (8580)",
        Model::C64cNtsc => "NTSC C64C (8580)",
    }
}

/// The stable variant id for a model (round-trips through [`model_for_variant`]).
fn variant_id(model: Model) -> &'static str {
    match model {
        Model::C64PalBreadbin => PAL_ID,
        Model::C64NtscBreadbin => NTSC_ID,
        Model::C64cPal => C64C_PAL_ID,
        Model::C64cNtsc => C64C_NTSC_ID,
    }
}

/// Resolve a variant id from the Machine menu back to a [`Model`].
fn model_for_variant(variant: &str) -> Option<Model> {
    match variant {
        PAL_ID => Some(Model::C64PalBreadbin),
        NTSC_ID => Some(Model::C64NtscBreadbin),
        C64C_PAL_ID => Some(Model::C64cPal),
        C64C_NTSC_ID => Some(Model::C64cNtsc),
        _ => None,
    }
}

// ---- The launcher's window driver -------------------------------------------

impl UiApp for C64 {
    type System = C64System;

    fn ui_system(&self) -> C64System {
        // The launcher builds the window driver before the runtime and hands
        // neither to the other, so the driver resolves the same firmware
        // itself to stash for live variant switches. A failure here is left
        // for `build_runtime`, which reports it a moment later.
        let firmware = self
            .load_firmware_bytes()
            .map(|images| {
                images
                    .into_iter()
                    .map(|image| (image.id.to_owned(), image.bytes))
                    .collect()
            })
            .unwrap_or_default();
        C64System {
            model: self.model.to_model(),
            firmware,
            keyboard_joystick: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_map_covers_cursors_and_shifted_function_keys() {
        assert_eq!(
            map_c64_keys(KeyCode::ArrowLeft),
            Some(&["lshift", "right"][..])
        );
        assert_eq!(
            map_c64_keys(KeyCode::ArrowUp),
            Some(&["lshift", "down"][..])
        );
        assert_eq!(map_c64_keys(KeyCode::F2), Some(&["lshift", "f1"][..]));
        assert_eq!(map_c64_keys(KeyCode::F8), Some(&["lshift", "f7"][..]));
        assert_eq!(map_c64_keys(KeyCode::Tab), Some(&["runstop"][..]));
        assert_eq!(map_c64_keys(KeyCode::AltLeft), Some(&["commodore"][..]));
        // Page Up is never a C64 matrix key (it toggles keyboard-joystick mode).
        assert_eq!(map_c64_keys(KeyCode::PageUp), None);
    }

    #[test]
    fn joystick_key_map_is_host_only() {
        assert_eq!(
            map_c64_joystick_key(KeyCode::ArrowLeft),
            Some(HostControl::Left)
        );
        assert_eq!(
            map_c64_joystick_key(KeyCode::Space),
            Some(HostControl::South)
        );
        assert_eq!(map_c64_joystick_key(KeyCode::F8), None);
    }

    #[test]
    fn variant_ids_round_trip_through_models() {
        for model in [
            Model::C64PalBreadbin,
            Model::C64NtscBreadbin,
            Model::C64cPal,
            Model::C64cNtsc,
        ] {
            assert_eq!(model_for_variant(variant_id(model)), Some(model));
        }
        assert_eq!(model_for_variant("nonsense"), None);
    }

    #[test]
    fn page_up_toggles_keyboard_joystick_on_keydown_only() {
        let mut system = C64System {
            model: Model::C64PalBreadbin,
            firmware: Vec::new(),
            keyboard_joystick: false,
        };
        // Key-down flips the mode and consumes the key.
        assert!(c64system_handle_pageup(&mut system, true));
        assert!(system.keyboard_joystick);
        // Key-up consumes the key but does not toggle again.
        assert!(c64system_handle_pageup(&mut system, false));
        assert!(system.keyboard_joystick);
        // A second key-down flips it back off.
        assert!(c64system_handle_pageup(&mut system, true));
        assert!(!system.keyboard_joystick);
    }

    /// Test helper: exercise `handle_key` for Page Up without a live runtime
    /// (the C64 `handle_key` ignores the runtime for the Page-Up toggle).
    fn c64system_handle_pageup(system: &mut C64System, pressed: bool) -> bool {
        // The runtime argument is unused by the Page-Up branch, so a null
        // pointer read is never reached; route through a fresh blank runtime to
        // satisfy the signature without booting firmware.
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        system.handle_key(&mut runtime, KeyCode::PageUp, pressed)
    }

    #[test]
    fn keyboard_joystick_mode_steals_arrows_and_space_from_keyboard() {
        let mut system = C64System {
            model: Model::C64PalBreadbin,
            firmware: Vec::new(),
            keyboard_joystick: false,
        };
        // Off: arrows are keyboard keys, no host control.
        assert!(system.map_keys(KeyCode::ArrowUp).is_some());
        assert_eq!(system.map_key(KeyCode::ArrowUp), None);
        // On: arrows fall through to the joystick path.
        system.keyboard_joystick = true;
        assert_eq!(system.map_keys(KeyCode::ArrowUp), None);
        assert_eq!(system.map_key(KeyCode::ArrowUp), Some(HostControl::Up));
        assert_eq!(system.map_key(KeyCode::Space), Some(HostControl::South));
        // A non-joystick key is still a keyboard key in joystick mode.
        assert!(system.map_keys(KeyCode::KeyA).is_some());
    }

    fn blank_system() -> C64System {
        C64System {
            model: Model::C64PalBreadbin,
            firmware: Vec::new(),
            keyboard_joystick: false,
        }
    }

    #[test]
    fn drive_kind_ids_round_trip() {
        for kind in [
            None,
            Some(DriveKind::C1541),
            Some(DriveKind::C1571),
            Some(DriveKind::C1581),
        ] {
            assert_eq!(drive_kind_for_id(drive_kind_id(kind)), Some(kind));
        }
        assert_eq!(drive_kind_for_id("bogus"), None);
    }

    #[test]
    fn drive_ports_describes_all_four_iec_devices() {
        // A blank runtime has no drive DOS ROMs, so every model is unavailable
        // and every port is empty — the descriptor still lists all four devices.
        let system = blank_system();
        let runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let ports = system.drive_ports(&runtime);

        assert_eq!(ports.len(), 4);
        for (index, port) in ports.iter().enumerate() {
            assert_eq!(port.device, 8 + index as u8);
            assert_eq!(port.current, "none");
            let ids: Vec<&str> = port.options.iter().map(|o| o.id.as_ref()).collect();
            assert_eq!(ids, ["none", "1541", "1571", "1581"]);
            // "Empty" is always selectable; the models need their ROMs.
            assert!(port.options[0].available);
            assert!(port.options[1..].iter().all(|o| !o.available));
        }
    }

    #[test]
    fn set_port_drive_clears_maps_and_rejects_unknown_ids() {
        let mut system = blank_system();
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);

        // "none" empties a port and needs no firmware.
        assert!(system.set_port_drive(&mut runtime, 8, "none").is_ok());
        // A model whose ROM is absent is rejected (blank runtime has none).
        assert!(matches!(
            system.set_port_drive(&mut runtime, 8, "1541"),
            Err(MachineError::MissingFirmware { .. })
        ));
        // An unrecognised id is rejected before touching the runtime.
        assert!(matches!(
            system.set_port_drive(&mut runtime, 8, "bogus"),
            Err(MachineError::UnsupportedOperation { .. })
        ));
    }
}
