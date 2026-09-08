//! Interactive UI mode — the Dragon 32/64 on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke winit + wgpu runner with a thin
//! [`UiSystem`] descriptor over [`DragonRuntime`]. The harness owns the window,
//! video filters, framed audio, gamepad/keyboard plumbing, the native menu,
//! save-states, tape transport, and live variant switching; this file supplies
//! only the Dragon-specific knobs:
//!
//! - **Keyboard**: [`map_dragon_keys`] maps each physical host key to one or
//!   more Dragon-matrix key names (physical-layout-only — like the Spectrum, no
//!   logical/character path; shifted symbols are entered with the host's own
//!   Shift + the base key).
//! - **Gamepad**: the left analogue stick / d-pad drives Dragon joystick 1, and
//!   South/East fire — lifted verbatim from the bespoke runner.
//! - **Variants**: Dragon 32 and Dragon 64 as the Machine-menu radio;
//!   [`switch_variant`](UiSystem::switch_variant) rebuilds the runtime from the
//!   staged ROM bundle via `from_firmware`.
//! - **Tape**: F9/F10 transport + F11 turbo come free from the harness, gated on
//!   the `tape-1` slot; [`tape_playing`](UiSystem::tape_playing) drives turbo.
//!
//! Compiled only with the `ui` Cargo feature; the shared launcher opens the
//! window when no automation or harness flag is given.

use std::borrow::Cow;
use std::time::Duration;

use emu198x_shell::launch::LaunchError;
use emu198x_shell::{
    FamilyRuntime, FirmwareOverrides, HeadlessSession, InputEvent, MachineError, MediaImage,
    MediaKind, MediaSet, SessionError, build_variant, read_media_asset,
};
use emu198x_ui::launch::UiApp;
use emu198x_ui::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl, KeyCode,
    UiSystem, VariantInfo, VideoFilter,
};
use motorola_vdg_6847::{VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT, VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};
use thiserror::Error;

use crate::app::Dragon;

const DEFAULT_SCALE: u32 = 2;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = Model::Dragon32Pal.native_frame_ticks();
const INPUT_SLICES_PER_FRAME: u32 = 4;
const AUTOLOAD_BOOT_FRAMES: u32 = 100;
const AUTOLOAD_KEY_EDGE_FRAMES: u32 = 4;
const AUTOLOAD_START_SETTLE_FRAMES: u32 = 60;

// ---- Gamepad maps (lifted from the bespoke runner, unchanged) --------------

const DRAGON_GAMEPAD_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
    (HostControl::Start, ButtonTarget::new(1, "enter")),
    (HostControl::Select, ButtonTarget::new(1, "clear")),
]);
const DRAGON_GAMEPAD_AXIS_MAP: AxisInputMap = AxisInputMap::new(&[
    (HostAxis::LeftStickX, AxisTarget::new(1, "x")),
    (HostAxis::LeftStickY, AxisTarget::new(1, "y")),
]);

/// Maps one physical host key to one or more Dragon-matrix key names.
///
/// Physical-layout-only (no logical/character path): the host's own Shift plus
/// a base key produces the Dragon's shifted symbols, exactly as the Dragon
/// keyboard membrane does. Lifted from the bespoke `map_dragon_physical_fallback`,
/// reshaped to return a static slice. The numpad and platform-key aliases are
/// preserved.
fn map_dragon_keys(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
        KeyCode::Digit0 | KeyCode::Numpad0 => &["0"],
        KeyCode::Digit1 | KeyCode::Numpad1 => &["1"],
        KeyCode::Digit2 | KeyCode::Numpad2 => &["2"],
        KeyCode::Digit3 | KeyCode::Numpad3 => &["3"],
        KeyCode::Digit4 | KeyCode::Numpad4 => &["4"],
        KeyCode::Digit5 | KeyCode::Numpad5 => &["5"],
        KeyCode::Digit6 | KeyCode::Numpad6 => &["6"],
        KeyCode::Digit7 | KeyCode::Numpad7 => &["7"],
        KeyCode::Digit8 | KeyCode::Numpad8 => &["8"],
        KeyCode::Digit9 | KeyCode::Numpad9 => &["9"],
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
        KeyCode::ArrowUp => &["up"],
        KeyCode::ArrowDown => &["down"],
        KeyCode::ArrowLeft => &["left"],
        KeyCode::ArrowRight => &["right"],
        KeyCode::Space => &["space"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Backspace | KeyCode::Delete | KeyCode::NumpadBackspace | KeyCode::NumpadClear => {
            &["clear"]
        }
        KeyCode::F1 => &["break"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["shift"],
        KeyCode::Comma | KeyCode::NumpadComma => &[","],
        KeyCode::Minus | KeyCode::NumpadSubtract => &["-"],
        KeyCode::Period | KeyCode::NumpadDecimal => &["."],
        KeyCode::Slash | KeyCode::NumpadDivide => &["/"],
        KeyCode::Semicolon => &[";"],
        KeyCode::Quote => &["@"],
        _ => return None,
    })
}

// ---- The UiSystem ----------------------------------------------------------

/// The Dragon as a [`UiSystem`]. Tracks the active model so the title and the
/// Machine-menu radio follow live switches.
pub struct DragonSystem {
    current: Model,
}

impl UiSystem for DragonSystem {
    type Runtime = DragonRuntime;

    fn window_title(&self) -> String {
        format!("Emu198x {}", self.current.display_name())
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    /// The Dragon's window has always opened on the CRT filter.
    fn default_video(&self) -> VideoFilter {
        VideoFilter::Crt
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (
            VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32,
            VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32,
        )
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        runtime.native_frame_ticks()
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(1.0 / DRAGON_FRAME_HZ as f64)
    }

    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &DRAGON_GAMEPAD_MAP
    }

    fn axis_map(&self) -> &'static AxisInputMap {
        &DRAGON_GAMEPAD_AXIS_MAP
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_dragon_keys(code)
    }

    fn tape_playing(&self, runtime: &Self::Runtime) -> bool {
        runtime.machine().cassette_motor_on()
    }

    fn variants(&self) -> Vec<VariantInfo> {
        Model::ALL
            .into_iter()
            .map(|model| VariantInfo::new(model.variant_id(), model.display_name()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.current.variant_id()))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = Model::from_variant_id(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown Dragon variant",
        })?;
        *runtime =
            build_variant::<DragonRuntime>(model, &FirmwareOverrides::none()).map_err(|err| {
                MachineError::Host {
                    reason: err.to_string(),
                }
            })?;
        self.current = model;
        Ok(())
    }
}

// ---- Construction ----------------------------------------------------------

impl UiApp for Dragon {
    type System = DragonSystem;

    fn ui_system(&self) -> DragonSystem {
        DragonSystem {
            current: self.model,
        }
    }

    /// Resolve catalogue firmware, mount startup media, then perform the
    /// window's existing tape-autoload sequence when requested.
    fn build_ui_runtime(&self) -> Result<DragonRuntime, LaunchError> {
        build_runtime(self).map_err(|err| LaunchError::Run(err.to_string()))
    }
}

/// Setup-phase errors building the runtime from the flags. Surfaced to the
/// launcher as a run error.
#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error("{reason}")]
    Setup { reason: String },
}

/// Build a [`DragonRuntime`] from the flags' launch model and media workflow.
/// A temporary [`HeadlessSession`] is used for the media load/autoload
/// (reusing the shared helpers), then unwrapped into the bare runtime the
/// harness drives.
fn build_runtime(cli: &Dragon) -> Result<DragonRuntime, AppError> {
    if cli.autoload && cli.tape.is_none() {
        return Err(AppError::Setup {
            reason: "--autoload requires --tape PATH".to_owned(),
        });
    }

    let runtime = cli
        .catalogue_runtime()
        .map_err(|reason| AppError::Setup { reason })?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );

    if let Some(tape) = &cli.tape {
        let loaded = read_media_asset(tape, MediaKind::Tape).map_err(|err| AppError::Setup {
            reason: format!("failed to load Dragon tape {}: {err}", tape.display()),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
        session.load_media(&media)?;
        if let Some(summary) = session.machine().tape_summary() {
            let name = summary.header_name.as_deref().unwrap_or("<no header>");
            println!(
                "Loaded tape: {name}, {} CAS blocks, checksums {}",
                summary.blocks,
                if summary.checksums_valid {
                    "valid"
                } else {
                    "invalid"
                }
            );
        }
    }

    if let Some(cart) = &cli.cart {
        let loaded =
            read_media_asset(cart, MediaKind::Cartridge).map_err(|err| AppError::Setup {
                reason: format!("failed to load Dragon cartridge {}: {err}", cart.display()),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "cartridge-1",
            MediaKind::Cartridge,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded cartridge: {} bytes", loaded.bytes.len());
    }

    if let Some(bin) = &cli.bin {
        let loaded = read_media_asset(bin, MediaKind::Program).map_err(|err| AppError::Setup {
            reason: format!(
                "failed to load Dragon binary program {}: {err}",
                bin.display()
            ),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "program-1",
            MediaKind::Program,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        if let Some(summary) = session.machine().program_summary() {
            println!(
                "Loaded DragonDOS BIN: {} bytes at ${:04X}, exec ${:04X}",
                summary.len, summary.load_address, summary.exec_address
            );
        }
    }

    if let Some(snapshot) = &cli.snapshot {
        let loaded =
            read_media_asset(snapshot, MediaKind::Snapshot).map_err(|err| AppError::Setup {
                reason: format!(
                    "failed to load Dragon snapshot {}: {err}",
                    snapshot.display()
                ),
            })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "snapshot-1",
            MediaKind::Snapshot,
            &loaded.bytes,
        ));
        session.load_media(&media)?;
        println!("Loaded snapshot: {} bytes", loaded.bytes.len());
    }

    if cli.autoload {
        autoload_tape(&mut session)?;
    }

    Ok(session.into_machine())
}

// ---- Autoload machinery (lifted from the bespoke runner) -------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragonAutoloadKind {
    Basic,
    MachineCode,
}

impl DragonAutoloadKind {
    fn load_command(self) -> &'static str {
        match self {
            Self::Basic => "CLOAD",
            Self::MachineCode => "CLOADM",
        }
    }

    fn start_command(self) -> &'static str {
        match self {
            Self::Basic => "RUN",
            Self::MachineCode => "EXEC",
        }
    }
}

fn autoload_kind(runtime: &DragonRuntime) -> Result<DragonAutoloadKind, AppError> {
    let summary = runtime.tape_summary().ok_or_else(|| AppError::Setup {
        reason: "--autoload requires a mounted CAS tape".to_owned(),
    })?;
    match summary.header_file_type {
        Some("basic") => Ok(DragonAutoloadKind::Basic),
        Some("machine-code") => Ok(DragonAutoloadKind::MachineCode),
        Some(file_type) => Err(AppError::Setup {
            reason: format!("--autoload does not support Dragon CAS file type {file_type}"),
        }),
        None => Err(AppError::Setup {
            reason: "--autoload requires a Dragon CAS namefile header".to_owned(),
        }),
    }
}

fn autoload_tape(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Result<(), AppError> {
    let kind = autoload_kind(session.machine())?;
    let boot = session.wait_for_boot(AUTOLOAD_BOOT_FRAMES)?;
    session.run_frames(30)?;

    println!("Autoload: typing {}", kind.load_command());
    type_basic_command(session, kind.load_command())?;
    wait_for_tape_position_above(session, 0, 180)?;
    let load_wait_frames =
        load_wait_frame_budget(session.machine().machine().cassette_len_bits() as u64);
    wait_for_tape_load_stop(session, load_wait_frames)?;

    println!("Autoload: typing {}", kind.start_command());
    type_basic_command(session, kind.start_command())?;
    session.run_frames(AUTOLOAD_START_SETTLE_FRAMES)?;
    println!("Autoload complete after BASIC boot: {}", boot.reason);
    Ok(())
}

fn load_wait_frame_budget(tape_length_bits: u64) -> u32 {
    let scaled = tape_length_bits / 16;
    u32::try_from(scaled.clamp(4_500, 20_000)).unwrap_or(20_000)
}

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    position_bits: usize,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        if session.machine().machine().cassette_position_bits() > position_bits {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not start consuming tape within {max_frames} frames"),
    })
}

fn wait_for_tape_load_stop(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> Result<(), AppError> {
    for _ in 0..=max_frames {
        let machine = session.machine().machine();
        if !machine.cassette_motor_on() || machine.cassette_finished() {
            return Ok(());
        }
        session.run_frames(1)?;
    }
    Err(AppError::Setup {
        reason: format!("Dragon autoload did not finish loading within {max_frames} frames"),
    })
}

fn type_basic_command(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    command: &str,
) -> Result<(), AppError> {
    for ch in command.chars() {
        tap_key(session, &ch.to_ascii_lowercase().to_string())?;
    }
    tap_key(session, "enter")
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
) -> Result<(), AppError> {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: false,
    });
    session.run_frames(AUTOLOAD_KEY_EDGE_FRAMES)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoload_kind_commands_match_dragon_basic() {
        assert_eq!(DragonAutoloadKind::Basic.load_command(), "CLOAD");
        assert_eq!(DragonAutoloadKind::Basic.start_command(), "RUN");
        assert_eq!(DragonAutoloadKind::MachineCode.load_command(), "CLOADM");
        assert_eq!(DragonAutoloadKind::MachineCode.start_command(), "EXEC");
    }

    #[test]
    fn gamepad_map_targets_dragon_joystick_fire() {
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::South, true),
            Some(InputEvent::Button {
                port: 1,
                name: "fire".into(),
                pressed: true,
            })
        );
        assert_eq!(
            DRAGON_GAMEPAD_MAP.event(HostControl::Right, true),
            Some(InputEvent::Button {
                port: 1,
                name: "right".into(),
                pressed: true,
            })
        );
    }

    #[test]
    fn gamepad_axis_map_targets_dragon_analogue_axes() {
        assert_eq!(
            DRAGON_GAMEPAD_AXIS_MAP.event(HostAxis::LeftStickX, -1.0),
            Some(InputEvent::Axis {
                port: 1,
                name: "x".into(),
                value: i16::MIN,
            })
        );
        assert_eq!(
            DRAGON_GAMEPAD_AXIS_MAP.event(HostAxis::LeftStickY, 1.0),
            Some(InputEvent::Axis {
                port: 1,
                name: "y".into(),
                value: i16::MAX,
            })
        );
    }

    #[test]
    fn map_keys_covers_letters_digits_and_named_keys() {
        assert_eq!(map_dragon_keys(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Digit1), Some(&["1"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Quote), Some(&["@"][..]));
        assert_eq!(map_dragon_keys(KeyCode::ArrowLeft), Some(&["left"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Enter), Some(&["enter"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Backspace), Some(&["clear"][..]));
        assert_eq!(map_dragon_keys(KeyCode::F1), Some(&["break"][..]));
        assert_eq!(map_dragon_keys(KeyCode::ShiftLeft), Some(&["shift"][..]));
    }

    #[test]
    fn map_keys_covers_numpad_and_platform_aliases() {
        assert_eq!(map_dragon_keys(KeyCode::Numpad1), Some(&["1"][..]));
        assert_eq!(map_dragon_keys(KeyCode::NumpadEnter), Some(&["enter"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Delete), Some(&["clear"][..]));
        assert_eq!(map_dragon_keys(KeyCode::Semicolon), Some(&[";"][..]));
    }

    #[test]
    fn map_keys_ignores_unmapped_keys() {
        assert_eq!(map_dragon_keys(KeyCode::F5), None);
        assert_eq!(map_dragon_keys(KeyCode::Tab), None);
    }
}
