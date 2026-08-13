//! Interactive UI mode — the Commodore Amiga on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke winit + wgpu runner with a thin
//! [`UiSystem`] descriptor over [`AmigaRuntimeKind`]. The harness owns the
//! window, video filters, framed audio, gamepad/keyboard plumbing, the native
//! menu, save-states, mouse capture, and live variant switching; this file
//! supplies only the Amiga-specific knobs:
//!
//! - **Mouse**: [`mouse_device`](UiSystem::mouse_device) opts the window into
//!   pointer capture as `mouse-1` (Amiga control port 1 / JOY0DAT). The harness
//!   translates cursor motion into relative `PointerMotion` deltas and clicks
//!   into `PointerButton`; [`map_mouse_button`](UiSystem::map_mouse_button) stays
//!   at the default left/right/middle.
//! - **Joystick / keyboard-joystick**: [`AMIGA_JOYSTICK_MAP`] drives control
//!   port 2 (JOY1DAT). In keyboard-joystick mode (toggled with Page Up) the arrow
//!   keys + Space fall through to that port.
//! - **Keyboard**: [`map_amiga_key`] maps each physical host key to one Amiga key
//!   name.
//! - **Variants**: all nine selectable PAL configurations (A1000 / A500 family,
//!   including the A530 research profile / A600 / A1200 / A2000) as the
//!   Machine-menu radio. Each switch resolves the target model's Kickstart via
//!   the staged ROM resolution in `model.rs` and rebuilds the runtime with
//!   `from_firmware` (the inserted disk is lost — a hardware swap).
//! - **Reset**: [`after_reset`](UiSystem::after_reset) re-inserts the DF0 ADF so
//!   F12 keeps the disk (the bespoke runner dropped it).
//!
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! headless-only flag is given.

use std::borrow::Cow;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, MachineError, MediaImage, MediaKind,
    MediaSet, read_firmware_asset, read_media_asset,
};
use emu198x_ui::{
    ButtonInputMap, ButtonTarget, HostControl, KeyCode, UiSystem, VariantInfo, VideoFilter,
};
use runtime_commodore_amiga::{
    A500_PAL_CCK_HZ, A500_PAL_FRAME_TICKS, AmigaRuntimeKind, DISPLAY_HEIGHT, DISPLAY_WIDTH,
};

use crate::{
    ModelArg, USAGE, die, find_rom_path, firmware_id_for_model_arg, next_arg, parse_model_arg,
};

const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";
const DEFAULT_SCALE: u32 = 1;
// `AmigaRuntime::run_until` publishes complete video fields. A sub-field
// target therefore still advances one whole field, so the UI must issue one
// run request per displayed field rather than four nominal input slices.
const INPUT_SLICES_PER_FRAME: u32 = 1;
const MOUSE_DEVICE: &str = "mouse-1";

// The joystick lives in Amiga control port 2 (JOY1DAT) per *Mapping the
// Amiga* p.460 — the runtime maps input port 2 onto the machine's
// joystick. (Mouse is port 1 / JOY0DAT, routed via pointer events.)
const AMIGA_JOYSTICK_MAP: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(2, "up")),
    (HostControl::Down, ButtonTarget::new(2, "down")),
    (HostControl::Left, ButtonTarget::new(2, "left")),
    (HostControl::Right, ButtonTarget::new(2, "right")),
    (HostControl::South, ButtonTarget::new(2, "fire")), // primary fire
    (HostControl::East, ButtonTarget::new(2, "button2")), // 2nd fire (POTGOR)
    (HostControl::North, ButtonTarget::new(2, "button3")), // 3rd fire (POTGOR)
    (HostControl::West, ButtonTarget::new(2, "fire")),  // alt primary fire
]);

/// Maps one physical host key to a single-element Amiga key-name slice (the
/// keyboard path; the harness wants `&'static [&'static str]`). Ported from the
/// bespoke `map_amiga_key`, which returned the bare name.
fn map_amiga_key(code: KeyCode) -> Option<&'static [&'static str]> {
    Some(match code {
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
        KeyCode::Space => &["space"],
        KeyCode::Backspace => &["backspace"],
        KeyCode::Tab => &["tab"],
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        _ => return None,
    })
}

/// The arrow keys + Space the keyboard-joystick mode steals from the keyboard
/// path and routes through the port-2 button map. Lifted from the bespoke
/// `map_amiga_joystick_key`.
fn map_amiga_joystick_key(code: KeyCode) -> Option<HostControl> {
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

/// The Amiga as a [`UiSystem`]. Tracks the active model so the title and the
/// Machine-menu radio follow live switches, the DF0 disk path (so a hard reset
/// can re-insert it), and whether the arrow keys / Space currently drive the
/// port-2 joystick (Page Up).
struct AmigaSystem {
    model: ModelArg,
    disk: Option<PathBuf>,
    keyboard_joystick: bool,
}

impl UiSystem for AmigaSystem {
    type Runtime = AmigaRuntimeKind;

    fn window_title(&self) -> String {
        let mut title = "Emu198x Amiga".to_owned();
        if self.keyboard_joystick {
            title.push_str(" | joy1 keys");
        }
        title
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (DISPLAY_WIDTH, DISPLAY_HEIGHT)
    }

    fn frame_ticks(&self, _runtime: &Self::Runtime) -> u64 {
        A500_PAL_FRAME_TICKS
    }

    fn frame_duration(&self, _runtime: &Self::Runtime) -> Duration {
        Duration::from_secs_f64(A500_PAL_FRAME_TICKS as f64 / (A500_PAL_CCK_HZ * 2) as f64)
    }

    fn input_slices_per_frame(&self) -> u32 {
        INPUT_SLICES_PER_FRAME
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        &AMIGA_JOYSTICK_MAP
    }

    fn mouse_device(&self) -> Option<&'static str> {
        Some(MOUSE_DEVICE)
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        // In keyboard-joystick mode the arrow keys + Space fall through to the
        // joystick path (returning `None` here so the harness routes them
        // through `map_key` + the button map instead).
        if self.keyboard_joystick && map_amiga_joystick_key(code).is_some() {
            return None;
        }
        map_amiga_key(code)
    }

    fn map_key(&self, code: KeyCode) -> Option<HostControl> {
        // Only meaningful in keyboard-joystick mode; otherwise the arrow keys +
        // Space are handled as keyboard keys by `map_keys`.
        if self.keyboard_joystick {
            map_amiga_joystick_key(code)
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
                        "enabled on port 2"
                    } else {
                        "disabled"
                    }
                );
            }
            return true;
        }
        false
    }

    fn after_reset(&mut self, runtime: &mut Self::Runtime) -> Result<(), MachineError> {
        // Re-insert the DF0 ADF after a hard reset — the bespoke runner dropped
        // the disk on F12; the harness's reset path runs this hook so the disk
        // survives.
        let Some(path) = &self.disk else {
            return Ok(());
        };
        let disk = read_media_asset(path, MediaKind::Disk).map_err(|err| MachineError::Host {
            reason: format!("failed to read disk {}: {err}", path.display()),
        })?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_FLOPPY_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        runtime.load_media(&media)
    }

    fn variants(&self) -> Vec<VariantInfo> {
        ModelArg::IDS
            .iter()
            .map(|id| {
                let model = ModelArg::from_id(id).expect("advertised id parses");
                VariantInfo::new(*id, model.to_model().display_name())
            })
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(model_arg_id(self.model)))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let model = ModelArg::from_id(variant).ok_or(MachineError::UnsupportedOperation {
            operation: "unknown Amiga variant",
        })?;
        // Resolve the target model's Kickstart by convention (env +
        // `~/.emu198x/roms/…`), mirroring the MCP `set_machine` path, and rebuild
        // the chipset variant. A launch-time --rom-dir / --kickstart override is
        // not carried across a switch (it's model-specific). The inserted disk is
        // lost — this is a hardware model change.
        *runtime = build_variant_runtime(model).map_err(|reason| MachineError::Host {
            reason: format!("switching to {}: {reason}", model.to_model().display_name()),
        })?;
        self.model = model;
        Ok(())
    }
}

/// The stable variant id for a [`ModelArg`] (round-trips through
/// [`ModelArg::from_id`]); matches the `--model` arg strings.
fn model_arg_id(model: ModelArg) -> &'static str {
    ModelArg::IDS[match model {
        ModelArg::A1000 => 0,
        ModelArg::A500 => 1,
        ModelArg::A500GvpA530 => 2,
        ModelArg::A500A501 => 3,
        ModelArg::A500Plus => 4,
        ModelArg::A500Maxed => 5,
        ModelArg::A600 => 6,
        ModelArg::A1200 => 7,
        ModelArg::A2000 => 8,
    }]
}

/// Resolve a model's Kickstart by convention (no CLI override) and build the
/// chipset variant, as a live variant switch needs. Mirrors the MCP
/// `set_machine` firmware resolution. `FirmwareSet` borrows the ROM bytes, so
/// the runtime is built here (where the bytes live) rather than returning a
/// borrowing firmware set.
fn build_variant_runtime(model: ModelArg) -> Result<AmigaRuntimeKind, String> {
    let path = find_rom_path(model, None, None)?;
    let loaded = read_firmware_asset(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        firmware_id_for_model_arg(model),
        &loaded.bytes,
    ));
    AmigaRuntimeKind::from_firmware(model.to_model(), &firmware).map_err(|err| err.to_string())
}

// ---- Construction + CLI ----------------------------------------------------

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kickstart: Option<PathBuf>,
    disk: Option<PathBuf>,
    scale: u32,
    video: VideoFilter,
}

/// Build the [`AmigaRuntimeKind`] from the CLI: resolve the model's Kickstart,
/// build the chipset variant, and insert the DF0 ADF if `--disk` was given.
/// Returns the bare runtime the harness drives (no tape autoload, so no
/// `HeadlessSession` is needed).
fn build_runtime(cli: &Cli) -> Result<AmigaRuntimeKind, String> {
    let model = cli.model.to_model();
    let firmware_path = find_rom_path(cli.model, cli.rom_dir.as_deref(), cli.kickstart.as_deref())?;
    let firmware_bytes = read_firmware_asset(&firmware_path).map_err(|err| {
        format!(
            "failed to read Amiga firmware {}: {err}",
            firmware_path.display()
        )
    })?;

    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        firmware_id_for_model_arg(cli.model),
        &firmware_bytes.bytes,
    ));
    let mut runtime =
        AmigaRuntimeKind::from_firmware(model, &firmware).map_err(|err| err.to_string())?;

    if let Some(path) = &cli.disk {
        let disk = read_media_asset(path, MediaKind::Disk)
            .map_err(|err| format!("failed to read disk {}: {err}", path.display()))?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_FLOPPY_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        runtime.load_media(&media).map_err(|err| err.to_string())?;
    }

    Ok(runtime)
}

/// Build the runtime from the CLI and open the window.
pub fn run(cli: Cli) -> Result<(), String> {
    println!(
        "Controls: Esc quit, F12 reset (keeps disk), Cmd/Ctrl+S/L save/load state, \
         mouse port 1, gamepad joystick port 2, Page Up toggles joystick arrows/space, \
         A-Z/0-9/Space/Enter/Tab/Backspace keyboard; Machine menu switches model live."
    );
    let runtime = build_runtime(&cli)?;
    emu198x_ui::run(
        AmigaSystem {
            model: cli.model,
            disk: cli.disk.clone(),
            keyboard_joystick: false,
        },
        runtime,
        cli.scale,
        cli.video,
    )
    .map_err(|err| err.to_string())
}

pub fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        scale: DEFAULT_SCALE,
        ..Cli::default()
    };
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom-dir" => cli.rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kickstart" => {
                cli.kickstart = Some(PathBuf::from(next_arg(&mut iter, "--kickstart")));
            }
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = parse_video_arg(&next_arg(&mut iter, "--video"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_commodore_amiga::Model;

    #[test]
    fn parse_cli_accepts_model_disk_and_scale() {
        let cli = parse_cli([
            "--model".to_owned(),
            "a500-a501".to_owned(),
            "--disk".to_owned(),
            "workbench13.adf".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::A500A501,
                rom_dir: None,
                kickstart: None,
                disk: Some(PathBuf::from("workbench13.adf")),
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_video_filter() {
        let cli = parse_cli(["--video".to_owned(), "crt".to_owned()]);

        assert_eq!(cli.video, VideoFilter::Crt);
    }

    #[test]
    fn maps_basic_keyboard_keys() {
        assert_eq!(map_amiga_key(KeyCode::KeyA), Some(&["a"][..]));
        assert_eq!(map_amiga_key(KeyCode::Digit1), Some(&["1"][..]));
        assert_eq!(map_amiga_key(KeyCode::Space), Some(&["space"][..]));
        assert_eq!(map_amiga_key(KeyCode::Enter), Some(&["enter"][..]));
    }

    #[test]
    fn maps_host_keys_to_joystick_mode_controls() {
        assert_eq!(
            map_amiga_joystick_key(KeyCode::ArrowUp),
            Some(HostControl::Up)
        );
        assert_eq!(
            map_amiga_joystick_key(KeyCode::Space),
            Some(HostControl::South)
        );
        // Page Up is never an Amiga matrix key (it toggles keyboard-joystick mode).
        assert_eq!(map_amiga_key(KeyCode::PageUp), None);
    }

    #[test]
    fn variant_ids_round_trip_through_model_args() {
        for id in ModelArg::IDS {
            let model = ModelArg::from_id(id).expect("advertised id parses");
            assert_eq!(model_arg_id(model), id, "id `{id}` must round-trip");
        }
    }

    #[test]
    fn variants_list_covers_all_nine_models() {
        let system = AmigaSystem {
            model: ModelArg::A500,
            disk: None,
            keyboard_joystick: false,
        };
        let variants = system.variants();
        assert_eq!(variants.len(), 9);
        let ids: Vec<_> = variants.iter().map(|v| v.id.as_ref()).collect();
        assert_eq!(ids, ModelArg::IDS);
        assert_eq!(
            system.current_variant().as_deref(),
            Some("a500"),
            "current variant tracks the active model"
        );
    }

    #[test]
    fn whole_field_runtime_uses_one_input_slice() {
        let system = AmigaSystem {
            model: ModelArg::A500,
            disk: None,
            keyboard_joystick: false,
        };

        assert_eq!(system.input_slices_per_frame(), 1);
    }

    #[test]
    fn page_up_toggles_keyboard_joystick_on_keydown_only() {
        let mut system = AmigaSystem {
            model: ModelArg::A500,
            disk: None,
            keyboard_joystick: false,
        };
        let mut runtime = blank_runtime();
        // Key-down flips the mode and consumes the key.
        assert!(system.handle_key(&mut runtime, KeyCode::PageUp, true));
        assert!(system.keyboard_joystick);
        // Key-up consumes the key but does not toggle again.
        assert!(system.handle_key(&mut runtime, KeyCode::PageUp, false));
        assert!(system.keyboard_joystick);
        // A second key-down flips it back off.
        assert!(system.handle_key(&mut runtime, KeyCode::PageUp, true));
        assert!(!system.keyboard_joystick);
    }

    #[test]
    fn keyboard_joystick_mode_steals_arrows_and_space_from_keyboard() {
        let mut system = AmigaSystem {
            model: ModelArg::A500,
            disk: None,
            keyboard_joystick: false,
        };
        // Off: arrows are unmapped keyboard keys, no host control.
        assert_eq!(system.map_key(KeyCode::ArrowUp), None);
        // On: arrows fall through to the joystick path.
        system.keyboard_joystick = true;
        assert_eq!(system.map_keys(KeyCode::ArrowUp), None);
        assert_eq!(system.map_key(KeyCode::ArrowUp), Some(HostControl::Up));
        assert_eq!(system.map_key(KeyCode::Space), Some(HostControl::South));
        // A non-joystick key is still a keyboard key in joystick mode.
        assert!(system.map_keys(KeyCode::KeyA).is_some());
    }

    #[test]
    fn window_title_reflects_keyboard_joystick_mode() {
        let mut system = AmigaSystem {
            model: ModelArg::A500,
            disk: None,
            keyboard_joystick: false,
        };
        assert_eq!(system.window_title(), "Emu198x Amiga");
        system.keyboard_joystick = true;
        assert_eq!(system.window_title(), "Emu198x Amiga | joy1 keys");
    }

    /// A blank Amiga runtime for tests that only exercise host-side state (key
    /// mapping, the Page-Up toggle) and never touch the runtime.
    fn blank_runtime() -> AmigaRuntimeKind {
        AmigaRuntimeKind::blank(Model::A500OcsPal)
    }
}
