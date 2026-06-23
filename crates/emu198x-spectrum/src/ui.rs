//! Interactive UI mode — the Spectrum on the shared `emu198x-ui` harness.
//!
//! This replaces the former bespoke `ui/` module (winit + wgpu + muda by hand)
//! with a thin [`UiSystem`] descriptor over the family-enum runtime
//! ([`SpectrumRuntimeKind`]). The harness owns the window, video filters, framed
//! audio, gamepad/keyboard plumbing, the native menu, save-states, tape
//! transport, and live variant switching; this file supplies only the
//! Spectrum-specific knobs:
//!
//! - **Keyboard**: [`map_spectrum_keys`] maps host keys to the 8×5 matrix,
//!   including the membrane-wired cursor combos (Caps + 5/6/7/8).
//! - **Gamepad**: per-variant routing — Kempston (port 0) on the Kempston-
//!   bearing machines, Sinclair Interface 2 (port 1) on +2A/+2B/+3, which can't
//!   host a Kempston. Both [`button_map`](UiSystem::button_map) and
//!   [`axis_map`](UiSystem::axis_map) switch with the active variant.
//! - **Variants**: all 13 [`MachineKind`]s as the Machine-menu radio;
//!   [`switch_variant`](UiSystem::switch_variant) rebuilds the runtime via
//!   `from_firmware`, the same recipe as the MCP `set_machine` tool.
//! - **Tape**: F9/F10 transport + F11 turbo come free from the harness, gated on
//!   the `tape-1` slot; [`tape_playing`](UiSystem::tape_playing) drives turbo.
//! - **Snapshots**: File → Open State… loads `.sna`/`.z80` (parsed + applied) or
//!   the internal `.emu198x-state` (restored), via [`load_state_file`].
//!
//! Compiled only with the `ui` Cargo feature; `main.rs` routes here when no
//! automation flag is given.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_shell::{
    ControlCommand, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore,
    MachineError, MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand,
    read_firmware_asset, read_media_asset,
};
use emu198x_ui::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl, KeyCode,
    UiSystem, VariantInfo, VideoFilter,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpectrumLiveAccess,
    SpectrumRuntimeKind, SpectrumSessionQueryProvider, autoload_basic_tape,
};

use crate::machine::{MachineKind, read_variant_firmware, rom_root, variant_rom_bundle};
use crate::mcp::tools::kind_to_model;

const DEFAULT_SCALE: u32 = 2;
const DEFAULT_TAPE_SLOT: &str = "tape-1";

// ---- Gamepad maps (lifted from the bespoke runner, unchanged) --------------

/// Host gamepad → Kempston joystick (port 0). The runtime's
/// `kempston_button_from_name` aliases "fire"/"button1"/"a" to the fire bit.
const KEMPSTON_BUTTONS: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(0, "up")),
    (HostControl::Down, ButtonTarget::new(0, "down")),
    (HostControl::Left, ButtonTarget::new(0, "left")),
    (HostControl::Right, ButtonTarget::new(0, "right")),
    (HostControl::South, ButtonTarget::new(0, "fire")),
    (HostControl::East, ButtonTarget::new(0, "fire")),
]);
const KEMPSTON_AXES: AxisInputMap = AxisInputMap::new(&[
    (HostAxis::LeftStickX, AxisTarget::new(0, "horizontal")),
    (HostAxis::LeftStickY, AxisTarget::new(0, "vertical")),
]);

/// Host gamepad → Sinclair Interface 2 port 1 (keys 6/7/8/9/0). The +2A/+2B/+3
/// broke the rear-connector pinout in 1987 and can't host a Kempston; their
/// built-in side joystick ports are wired to the keyboard matrix.
const IF2_BUTTONS: ButtonInputMap = ButtonInputMap::new(&[
    (HostControl::Up, ButtonTarget::new(1, "up")),
    (HostControl::Down, ButtonTarget::new(1, "down")),
    (HostControl::Left, ButtonTarget::new(1, "left")),
    (HostControl::Right, ButtonTarget::new(1, "right")),
    (HostControl::South, ButtonTarget::new(1, "fire")),
    (HostControl::East, ButtonTarget::new(1, "fire")),
]);
const IF2_AXES: AxisInputMap = AxisInputMap::new(&[
    (HostAxis::LeftStickX, AxisTarget::new(1, "horizontal")),
    (HostAxis::LeftStickY, AxisTarget::new(1, "vertical")),
]);

/// `true` for the Amstrad-class machines that route the gamepad through IF2.
fn uses_if2(kind: MachineKind) -> bool {
    matches!(
        kind,
        MachineKind::SpectrumPlus2A | MachineKind::SpectrumPlus2B | MachineKind::SpectrumPlus3
    )
}

/// Maps one physical host key to one or more Spectrum-matrix key names.
/// Multiple names produce hardware-correct combos: the cursor keys close
/// Caps Shift + 5/6/7/8 exactly as a Spectrum+ membrane does.
fn map_spectrum_keys(code: KeyCode) -> Option<&'static [&'static str]> {
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
        KeyCode::Enter | KeyCode::NumpadEnter => &["enter"],
        KeyCode::Space => &["space"],
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &["caps"],
        KeyCode::AltLeft | KeyCode::AltRight => &["symbol"],
        // Membrane-wired cursor keys: Caps Shift + 5/6/7/8 — the exact contacts
        // the Spectrum+ and 128K family close; the ROM never sees a cursor code.
        KeyCode::ArrowLeft => &["caps", "5"],
        KeyCode::ArrowDown => &["caps", "6"],
        KeyCode::ArrowUp => &["caps", "7"],
        KeyCode::ArrowRight => &["caps", "8"],
        KeyCode::Backspace => &["caps", "0"],
        KeyCode::Quote => &["symbol", "p"],
        _ => return None,
    })
}

// ---- The UiSystem ----------------------------------------------------------

/// The Spectrum as a [`UiSystem`]. Tracks the active variant so the gamepad
/// routing, title, and Machine-menu radio follow live switches.
struct SpectrumSystem {
    current: MachineKind,
}

impl UiSystem for SpectrumSystem {
    type Runtime = SpectrumRuntimeKind;

    fn window_title(&self) -> String {
        format!("Emu198x | {}", self.current.label())
    }

    fn default_scale(&self) -> u32 {
        DEFAULT_SCALE
    }

    fn framebuffer_size(&self, _runtime: &Self::Runtime) -> (u32, u32) {
        (SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)
    }

    fn frame_ticks(&self, runtime: &Self::Runtime) -> u64 {
        u64::from(runtime.frame_halfcycles())
    }

    fn frame_duration(&self, runtime: &Self::Runtime) -> Duration {
        // halfcycles / master-clock Hz, as the bespoke LiveSpectrumRuntime did.
        let halfcycles = f64::from(runtime.frame_halfcycles());
        let rate = &runtime.profile().clock.rate;
        let master_hz = rate.numerator_hz as f64 / rate.denominator_hz as f64;
        Duration::from_secs_f64(halfcycles / master_hz)
    }

    fn button_map(&self) -> &'static ButtonInputMap {
        if uses_if2(self.current) {
            &IF2_BUTTONS
        } else {
            &KEMPSTON_BUTTONS
        }
    }

    fn axis_map(&self) -> &'static AxisInputMap {
        if uses_if2(self.current) {
            &IF2_AXES
        } else {
            &KEMPSTON_AXES
        }
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_spectrum_keys(code)
    }

    fn tape_playing(&self, runtime: &Self::Runtime) -> bool {
        runtime.tape_is_playing()
    }

    fn variants(&self) -> Vec<VariantInfo> {
        MachineKind::all()
            .iter()
            .map(|kind| VariantInfo::new(kind.script_id(), kind.label()))
            .collect()
    }

    fn current_variant(&self) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(self.current.script_id()))
    }

    fn switch_variant(
        &mut self,
        runtime: &mut Self::Runtime,
        variant: &str,
    ) -> Result<(), MachineError> {
        let kind =
            MachineKind::from_script_id(variant).ok_or(MachineError::UnsupportedOperation {
                operation: "unknown Spectrum variant",
            })?;
        // Same recipe as the MCP `set_machine`: load the variant's ROM bundle,
        // then rebuild the family-enum runtime in place. The harness re-paces
        // and refreshes; state/media are not preserved (a hardware swap).
        let images = read_variant_firmware(kind).map_err(|err| MachineError::Host {
            reason: format!("loading {} ROMs: {err}", kind.label()),
        })?;
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &images {
            firmware.push(FirmwareImage::new(*id, bytes));
        }
        *runtime = SpectrumRuntimeKind::from_firmware(kind_to_model(kind), &firmware)?;
        self.current = kind;
        Ok(())
    }

    fn state_open_filter(&self) -> Option<(&'static str, &'static [&'static str])> {
        Some((
            "Snapshots & states",
            &["sna", "z80", "zip", "emu198x-state"],
        ))
    }

    fn load_state_file(&mut self, runtime: &mut Self::Runtime, path: &Path) -> Result<(), String> {
        load_any_snapshot(runtime, path)
    }
}

/// Loads any of the three state formats by extension: `.sna`/`.z80`/`.zip` are
/// portable snapshots (parsed + applied), anything else is the internal
/// postcard save-state (restored). Mirrors the bespoke `load_any_snapshot`.
fn load_any_snapshot(runtime: &mut SpectrumRuntimeKind, path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("sna") | Some("z80") | Some("zip") => {
            let loaded = read_media_asset(path, MediaKind::Snapshot).map_err(|e| e.to_string())?;
            let inner = loaded
                .archive_member
                .as_deref()
                .unwrap_or_else(|| path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
                .to_ascii_lowercase();
            let snapshot = if inner.ends_with(".sna") {
                format_sinclair_zx_spectrum_sna::parse_sna(&loaded.bytes)
                    .map_err(|e| e.to_string())?
            } else if inner.ends_with(".z80") {
                format_sinclair_zx_spectrum_z80::parse_z80(&loaded.bytes)
                    .map_err(|e| e.to_string())?
            } else {
                return Err(format!(
                    "unrecognised snapshot (expected .sna/.z80): {inner}"
                ));
            };
            runtime.apply_snapshot(&snapshot);
            Ok(())
        }
        _ => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            runtime.restore(&bytes).map_err(|e| e.to_string())
        }
    }
}

// ---- Construction + CLI ----------------------------------------------------

/// Parsed interactive CLI (preserved from the bespoke runner).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub rom: Option<PathBuf>,
    pub tape: Option<PathBuf>,
    pub play_tape: bool,
    pub autoload_tape: bool,
    pub turbo_tape: bool,
    pub scale: u32,
    pub video: VideoFilter,
}

const USAGE: &str = "\
Usage: emu198x-spectrum [OPTIONS]

Options:
    --rom PATH         48K ROM image or zip containing one ROM candidate
    --tape PATH        TAP/TZX image or zip containing one tape candidate
    --play-tape        start tape transport immediately after media load
    --autoload-tape    wait for boot, type LOAD \"\", and start tape-1
    --turbo-tape       (accepted; arm fast-load in the UI with F11)
    --scale N          integer window scale, default 2
    --video MODE       raw | lcd | crt [default: raw]
    --help, -h         show this help

Controls:
    Esc                quit
    F9 / F10 / F11     start / stop tape, toggle fast-load (turbo)
    F12                hard reset
    Cmd/Ctrl+S / +L    quick save / load state
    Arrow keys         Spectrum cursor keys (Caps Shift + 5/6/7/8)
    Alt                Symbol Shift
    Gamepad            Kempston on 16K/48K/+/128K/+2; IF2 on +2A/+2B/+3
    Machine menu       switch between the 13 variants live
    File > Open State  load a .sna / .z80 / .emu198x-state file

Examples:
    emu198x-spectrum
    emu198x-spectrum --rom 48.rom --tape manic_miner.zip
    emu198x-spectrum --tape manic_miner.zip --autoload-tape
";

/// Build the runtime from the CLI and open the window.
pub fn run(cli: Cli) -> Result<(), String> {
    if cli.play_tape && cli.autoload_tape {
        return Err("--play-tape and --autoload-tape are mutually exclusive".to_owned());
    }
    let runtime = build_runtime(&cli)?;
    println!(
        "Controls: Esc quit, F9/F10 tape start/stop, F11 fast-load, F12 reset, \
         Cmd/Ctrl+S/L save/load state; Machine menu switches variant."
    );
    emu198x_ui::run(
        SpectrumSystem {
            current: MachineKind::Spectrum48K,
        },
        runtime,
        cli.scale,
        cli.video,
    )
    .map_err(|err| err.to_string())
}

/// Boot a 48K `SpectrumRuntimeKind` and apply the CLI's tape workflow. A
/// temporary [`HeadlessSession`] is used for the tape load/autoload (reusing the
/// shared helpers), then unwrapped into the bare runtime the harness drives.
fn build_runtime(cli: &Cli) -> Result<SpectrumRuntimeKind, String> {
    let kind = MachineKind::Spectrum48K;
    let images = resolve_firmware(cli, kind)?;
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    let runtime = SpectrumRuntimeKind::from_firmware(kind_to_model(kind), &firmware)
        .map_err(|e| e.to_string())?;

    let frame_ticks = u64::from(runtime.frame_halfcycles());
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        frame_ticks,
        SpectrumSessionQueryProvider,
    );

    if let Some(tape_path) = &cli.tape {
        let tape = read_media_asset(tape_path, MediaKind::Tape).map_err(|e| e.to_string())?;
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session.load_media(&media).map_err(|e| e.to_string())?;
    }

    if cli.autoload_tape {
        if cli.tape.is_none() {
            return Err("--autoload-tape needs a --tape".to_owned());
        }
        autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        )
        .map_err(|e| e.to_string())?;
    } else if cli.play_tape {
        if cli.tape.is_none() {
            return Err("--play-tape needs a --tape".to_owned());
        }
        session
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_SLOT,
                MediaTransportAction::Start,
            )))
            .map_err(|e| e.to_string())?;
    }

    Ok(session.into_machine())
}

/// The 48K firmware: the `--rom` override (a single 48K image) or the staged
/// variant bundle.
fn resolve_firmware(cli: &Cli, kind: MachineKind) -> Result<Vec<(String, Vec<u8>)>, String> {
    if let Some(rom_path) = &cli.rom {
        let root = rom_root().ok_or("$HOME unset; cannot locate the 48K ROM id")?;
        let id = variant_rom_bundle(kind, &root)
            .first()
            .map(|(id, _)| (*id).to_owned())
            .ok_or("no firmware id for the 48K")?;
        let bytes = read_firmware_asset(rom_path)
            .map_err(|e| e.to_string())?
            .bytes
            .to_vec();
        return Ok(vec![(id, bytes)]);
    }
    read_variant_firmware(kind)
        .map(|images| {
            images
                .into_iter()
                .map(|(id, b)| (id.to_owned(), b))
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// Parse the interactive CLI. Exits the process on `--help` or a malformed flag.
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
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--play-tape" => cli.play_tape = true,
            "--autoload-tape" => cli.autoload_tape = true,
            "--turbo-tape" => cli.turbo_tape = true,
            "--scale" => {
                cli.scale = next_arg(&mut iter, "--scale")
                    .parse()
                    .unwrap_or_else(|_| die("--scale requires a positive integer"));
            }
            "--video" => {
                cli.video = next_arg(&mut iter, "--video")
                    .parse()
                    .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ if arg.starts_with('-') => die(&format!("unknown flag: {arg}")),
            _ => {
                if cli.tape.is_none() {
                    cli.tape = Some(PathBuf::from(arg));
                } else {
                    die("only one positional tape path is supported");
                }
            }
        }
    }
    cli
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults_to_scale_two() {
        let cli = parse_cli(std::iter::empty::<String>());
        assert_eq!(cli.scale, 2);
        assert_eq!(cli.video, VideoFilter::Raw);
        assert_eq!(cli.tape, None);
    }

    #[test]
    fn parse_cli_accepts_rom_tape_scale_and_positional() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "48.rom".to_owned(),
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--autoload-tape".to_owned(),
            "--scale".to_owned(),
            "3".to_owned(),
        ]);
        assert_eq!(cli.rom, Some(PathBuf::from("48.rom")));
        assert_eq!(cli.tape, Some(PathBuf::from("manic.zip")));
        assert!(cli.autoload_tape);
        assert_eq!(cli.scale, 3);

        let positional = parse_cli(["manic.zip".to_owned()]);
        assert_eq!(positional.tape, Some(PathBuf::from("manic.zip")));
    }

    #[test]
    fn cursor_keys_map_to_caps_shift_combos() {
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowLeft),
            Some(&["caps", "5"][..])
        );
        assert_eq!(
            map_spectrum_keys(KeyCode::ArrowRight),
            Some(&["caps", "8"][..])
        );
        assert_eq!(map_spectrum_keys(KeyCode::AltLeft), Some(&["symbol"][..]));
    }

    #[test]
    fn amstrad_class_uses_if2_others_kempston() {
        assert!(uses_if2(MachineKind::SpectrumPlus2A));
        assert!(uses_if2(MachineKind::SpectrumPlus3));
        assert!(!uses_if2(MachineKind::Spectrum48K));
        assert!(!uses_if2(MachineKind::Spectrum128K));
    }
}
