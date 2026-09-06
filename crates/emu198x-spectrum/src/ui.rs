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

use crate::machine::{
    MachineKind, RomOverrides, read_variant_firmware, resolved_rom_bundle, rom_override_entry,
};
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

    fn supports_host_keyboard(&self) -> bool {
        true
    }

    fn host_character_keys(&self, runtime: &Self::Runtime, ch: char) -> Option<Vec<String>> {
        use emu198x_shell::KeyboardTarget;
        // Reuse the runtime/MCP character table, normalising to the aliases
        // used by the physical mapping so overlapping chords share ownership.
        runtime.keys_for_char(ch).map(|keys| {
            keys.into_iter()
                .map(|name| match name.as_str() {
                    "CapsShift" => "caps".to_owned(),
                    "SymbolShift" => "symbol".to_owned(),
                    _ => name.to_ascii_lowercase(),
                })
                .collect()
        })
    }

    fn host_keyword_modifier(&self) -> Option<emu198x_ui::KeywordModifier> {
        Some(emu198x_ui::KeywordModifier {
            key: KeyCode::AltLeft,
            keys: &["symbol"],
            shifted_keys: &["caps", "symbol"],
        })
    }

    fn host_control_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        match code {
            KeyCode::Home => Some(&["caps", "1"]), // EDIT
            KeyCode::Delete => Some(&["caps", "0"]),
            KeyCode::Pause => Some(&["caps", "space"]), // BREAK
            _ => map_spectrum_keys(code),
        }
    }

    fn map_keys(&self, code: KeyCode) -> Option<&'static [&'static str]> {
        map_spectrum_keys(code)
    }

    fn tape_export_filter(&self) -> Option<(&'static str, &'static str)> {
        Some(("Spectrum tape recording", "tap"))
    }

    fn export_tape(&self, runtime: &Self::Runtime) -> Result<Vec<u8>, String> {
        runtime.flush_tape_image().ok_or_else(|| {
            "No decodable tape recording is available. Run BASIC SAVE, press a key at the tape prompt, and wait for SAVE to finish before exporting.".to_owned()
        })
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
            &["sna", "z80", "szx", "zip", "emu198x-state"],
        ))
    }

    fn load_state_file(&mut self, runtime: &mut Self::Runtime, path: &Path) -> Result<(), String> {
        load_any_snapshot(runtime, path)
    }
}

/// Loads any of the portable state formats by extension:
/// `.sna`/`.z80`/`.szx`/`.zip` are parsed and applied, anything else is the
/// internal postcard save-state (restored). Mirrors the bespoke
/// `load_any_snapshot`.
fn load_any_snapshot(runtime: &mut SpectrumRuntimeKind, path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("sna") | Some("z80") | Some("szx") | Some("zip") => {
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
            } else if inner.ends_with(".szx") {
                format_sinclair_zx_spectrum_szx::parse_szx(&loaded.bytes)
                    .map_err(|e| e.to_string())?
            } else {
                return Err(format!(
                    "unrecognised snapshot (expected .sna/.z80/.szx): {inner}"
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
    /// Raw `--rom` values, resolved against the boot variant's bundle.
    pub rom: Vec<String>,
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
    --rom PATH         ROM image or zip, for a single-ROM variant
    --rom ID=PATH      one entry of a multi-ROM variant's bundle;
                       repeatable, the rest still resolve conventionally
    --tape PATH        TAP/TZX image or zip containing one tape candidate
    --play-tape        start tape transport immediately after media load
    --autoload-tape    wait for boot, type LOAD \"\", and start tape-1
    --turbo-tape       (accepted; arm fast-load in the UI with F11)
    --scale N          integer window scale, default 2
    --video MODE       raw | lcd | crt [default: raw]
    --help, -h         show this help

Automation:
    --script PATH   run a JSON session headlessly and print a report
    --headless      run without a window (implied by --script)
    --mcp           serve this machine over MCP on stdio

Controls:
    Esc                quit
    F9 / F10 / F11     start / stop tape, toggle fast-load (turbo)
    Cmd/Ctrl+Shift+E  export tape recording to a new .tap file
    Cmd/Ctrl+Shift+K  toggle Host / Original keyboard (also Machine → Keyboard)
    Home / Pause      EDIT / BREAK in Host keyboard mode
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

/// The variant's firmware: every `--rom` override applied over the staged
/// bundle.
///
/// This used to take the first bundle entry, read the one `--rom` file into
/// it, and return a single-image set — which on a 128K handed a two-ROM
/// machine one ROM. Routing through `resolved_rom_bundle` means every
/// variant gets its whole bundle, with only the named entries replaced
/// (#842).
fn resolve_firmware(cli: &Cli, kind: MachineKind) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut overrides = RomOverrides::new();
    for spec in &cli.rom {
        let (id, path) = rom_override_entry(spec, kind).map_err(|e| e.to_string())?;
        overrides.insert(id, path);
    }
    if overrides.is_empty() {
        return read_variant_firmware(kind)
            .map(|images| {
                images
                    .into_iter()
                    .map(|(id, b)| (id.to_owned(), b))
                    .collect()
            })
            .map_err(|e| e.to_string());
    }
    let bundle = resolved_rom_bundle(kind, &overrides).map_err(|e| e.to_string())?;
    let mut images = Vec::with_capacity(bundle.len());
    for (id, path) in bundle {
        let bytes = read_firmware_asset(&path)
            .map_err(|e| e.to_string())?
            .bytes
            .to_vec();
        images.push((id.to_owned(), bytes));
    }
    Ok(images)
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
            "--rom" => cli.rom.push(next_arg(&mut iter, "--rom")),
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
    #[ignore = "FIXTURE: requires configured local Spectrum 48K ROM"]
    fn host_characters_enter_basic_through_the_rom() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(check_host_characters)
            .expect("test thread")
            .join()
            .expect("host character entry");
    }

    fn check_host_characters() {
        use emu198x_shell::InputEvent;
        let system = SpectrumSystem {
            current: MachineKind::Spectrum48K,
        };
        let runtime =
            build_runtime(&parse_cli(std::iter::empty::<String>())).expect("configured ROM");
        assert!(system.host_character_keys(&runtime, '€').is_none());
        assert!(system.host_character_keys(&runtime, '`').is_none());
        assert_eq!(
            system.host_control_keys(KeyCode::Home),
            Some(&["caps", "1"][..])
        );
        let ticks = u64::from(runtime.frame_halfcycles());
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, ticks, SpectrumSessionQueryProvider);
        session
            .wait_for_boot(DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
            .expect("boot");
        // P/L/R remain keyword keys in K mode. Everything after each keyword
        // is passed through the desktop host-character adapter, including quotes.
        for text in [
            "10p\"Hello, \";12.5+3.5\n",
            "20p\"!@#$%&'()_+-*/=<>?:;\"\n",
            "30ln$=\"Sam\"\n",
            "40pn$\n",
            "50p\"£\"\n",
            "r\n",
        ] {
            for ch in text.chars() {
                let names = system
                    .host_character_keys(session.machine(), ch)
                    .expect("representable character");
                for name in &names {
                    session.queue_input(InputEvent::Key {
                        name: name.clone().into(),
                        pressed: true,
                    });
                }
                session.run_frames(3).expect("press");
                for name in names {
                    session.queue_input(InputEvent::Key {
                        name: name.into(),
                        pressed: false,
                    });
                }
                session
                    .run_frames(if ch == '\n' { 30 } else { 8 })
                    .expect("release");
            }
        }
        // The text-query decoder currently labels glyph 96 with ASCII's `;
        // the Spectrum ROM renders it as a pound sign.
        for expected in ["Hello, 16", "!@#$%&'()_+-*/=<>?:;", "Sam", "`", "0 OK"] {
            session
                .wait_for_query_text_contains("screen.text.lines", expected, 120)
                .expect(expected);
        }
    }

    #[test]
    fn host_keyword_modifier_is_left_alt_and_shift_left_alt() {
        let system = SpectrumSystem {
            current: MachineKind::Spectrum48K,
        };
        let keyword = system
            .host_keyword_modifier()
            .expect("Spectrum keyword modifier");
        assert_eq!(keyword.key, KeyCode::AltLeft);
        assert_eq!(keyword.keys, &["symbol"]);
        assert_eq!(keyword.shifted_keys, &["caps", "symbol"]);
        // Original Keyboard retains its physical mappings.
        assert_eq!(system.map_keys(KeyCode::Tab), None);
        assert_eq!(system.map_keys(KeyCode::AltLeft), Some(&["symbol"][..]));
    }

    #[test]
    #[ignore = "FIXTURE: requires configured local Spectrum 48K ROM"]
    fn host_keyword_chords_enter_then_not_equal_int_and_stop() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(check_host_keyword_chords)
            .expect("test thread")
            .join()
            .expect("host keyword entry");
    }

    fn check_host_keyword_chords() {
        use emu198x_shell::InputEvent;
        type Session = HeadlessSession<SpectrumRuntimeKind, SpectrumSessionQueryProvider>;
        fn tap(session: &mut Session, keys: &[String]) {
            for name in keys {
                session.queue_input(InputEvent::Key {
                    name: name.clone().into(),
                    pressed: true,
                });
            }
            session.run_frames(3).expect("press");
            for name in keys {
                session.queue_input(InputEvent::Key {
                    name: name.clone().into(),
                    pressed: false,
                });
            }
            session.run_frames(8).expect("release");
        }
        fn text(system: &SpectrumSystem, session: &mut Session, text: &str) {
            for ch in text.chars() {
                let keys = system
                    .host_character_keys(session.machine(), ch)
                    .expect("host character");
                tap(session, &keys);
                if ch == '\n' {
                    session.run_frames(30).expect("enter");
                }
            }
        }
        fn keyword(system: &SpectrumSystem, session: &mut Session, code: KeyCode) {
            let modifier = system.host_keyword_modifier().expect("keyword modifier");
            let keys: Vec<String> = modifier
                .keys
                .iter()
                .copied()
                .chain(system.map_keys(code).expect("physical key").iter().copied())
                .map(str::to_owned)
                .collect();
            tap(session, &keys);
        }
        let system = SpectrumSystem {
            current: MachineKind::Spectrum48K,
        };
        let runtime =
            build_runtime(&parse_cli(std::iter::empty::<String>())).expect("configured ROM");
        let ticks = u64::from(runtime.frame_halfcycles());
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, ticks, SpectrumSessionQueryProvider);
        session
            .wait_for_boot(DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
            .expect("boot");
        text(&system, &mut session, "10lg=7\n20ug=7");
        keyword(&system, &mut session, KeyCode::KeyG); // Left Alt+G: THEN
        text(&system, &mut session, "p\"Correct!\"\n30ug");
        keyword(&system, &mut session, KeyCode::KeyW); // Left Alt+W: <>
        text(&system, &mut session, "8");
        keyword(&system, &mut session, KeyCode::KeyG);
        text(&system, &mut session, "p\"Different\"\n40p");
        let modifier = system.host_keyword_modifier().expect("keyword modifier");
        let keys: Vec<String> = modifier
            .shifted_keys
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        tap(&mut session, &keys); // Shift+Left Alt, released before R: extended mode
        text(&system, &mut session, "r6.5\n50");
        keyword(&system, &mut session, KeyCode::KeyA); // Left Alt+A: STOP
        text(&system, &mut session, "\nr\n");
        for expected in ["Correct!", "Different", "6", "9 STOP statement"] {
            session
                .wait_for_query_text_contains("screen.text.lines", expected, 120)
                .expect(expected);
        }
        // Ordinary punctuation still works immediately after the keyword chord.
        text(&system, &mut session, "p\"@#$\"\n");
        session
            .wait_for_query_text_contains("screen.text.lines", "@#$", 120)
            .expect("normal host punctuation");
    }

    #[test]
    #[ignore = "FIXTURE: requires configured local Spectrum 48K ROM"]
    fn desktop_tape_export_is_repeatable_and_reloads() {
        // The family enum includes large machine variants, as in the runtime's
        // variant tests; the test harness's default stack is too small.
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(check_desktop_tape_roundtrip)
            .expect("test thread")
            .join()
            .expect("desktop export round trip");
    }

    fn check_desktop_tape_roundtrip() {
        use runtime_sinclair_zx_spectrum::{tap_key, tap_symbol_combo};
        let system = SpectrumSystem {
            current: MachineKind::Spectrum48K,
        };
        let cli = parse_cli(std::iter::empty::<String>());
        let runtime = build_runtime(&cli).expect("configured 48K ROM");
        assert!(system.export_tape(&runtime).is_err());
        let ticks = u64::from(runtime.frame_halfcycles());
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, ticks, SpectrumSessionQueryProvider);
        session
            .wait_for_boot(DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
            .expect("boot");
        for key in ["1", "0", "e", "enter"] {
            tap_key(&mut session, key).expect("enter 10 REM");
        }
        session
            .wait_for_query_text_contains("screen.text.lines", "10>REM", 120)
            .expect("stored line");
        tap_key(&mut session, "s").expect("SAVE");
        tap_symbol_combo(&mut session, "p").expect("quote");
        tap_key(&mut session, "a").expect("filename");
        tap_symbol_combo(&mut session, "p").expect("quote");
        tap_key(&mut session, "enter").expect("SAVE prompt");
        session.run_frames(20).expect("settle");
        tap_key(&mut session, "enter").expect("start recording");
        session.run_frames(800).expect("finish SAVE");
        let bytes = system
            .export_tape(session.machine())
            .expect("desktop export bytes");
        assert_eq!(
            bytes,
            system.export_tape(session.machine()).expect("export again")
        );
        let runtime = build_runtime(&cli).expect("fresh runtime");
        let mut loaded =
            HeadlessSession::new_with_query_provider(runtime, ticks, SpectrumSessionQueryProvider);
        let mut media = MediaSet::new();
        media.push(MediaImage::new(DEFAULT_TAPE_SLOT, MediaKind::Tape, &bytes));
        loaded
            .machine_mut()
            .load_media(&media)
            .expect("mount exported tape");
        autoload_basic_tape(
            &mut loaded,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        )
        .expect("autoload");
        loaded.run_frames(1200).expect("finish loading");
        tap_key(&mut loaded, "k").expect("LIST");
        tap_key(&mut loaded, "enter").expect("run LIST");
        loaded
            .wait_for_query_text_contains("screen.text.lines", "REM", 120)
            .expect("recovered program");
    }

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
        assert_eq!(cli.rom, vec!["48.rom".to_owned()]);
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
