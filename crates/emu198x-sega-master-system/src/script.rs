//! Headless SMS / Game Gear runner.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use emu198x_shell::{HeadlessScript, HeadlessSession, MediaSet, ScriptObservation};
use runtime_sega_master_system::{Model, SmsSessionQueryProvider, with_cartridge};
use serde_json::json;

const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;

const USAGE: &str = "\
Usage: emu198x-sega-master-system [OPTIONS]

Cartridge:
    --cart PATH                cartridge ROM (required)

Variant:
    --variant KIND             sms-ntsc | sms-pal | sms1-ntsc | sms1-pal
                               [default: sms-ntsc; sms1 is the early 315-5124 VDP]
    --frames N                 native video frames to run [default: 0]

Capture:
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV

Shared:
    --script PATH              execute shared JSON session steps
    --help, -h                 show this help
";

#[derive(Debug)]
struct Cli {
    cart: Option<PathBuf>,
    variant: Variant,
    frames: u32,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            cart: None,
            variant: Variant::SmsNtsc,
            frames: 0,
            screenshot: None,
            audio_capture: None,
            script: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    SmsNtsc,
    SmsPal,
    Sms1Ntsc,
    Sms1Pal,
}

impl Variant {
    const fn model(self) -> Model {
        match self {
            Self::SmsNtsc => Model::SmsNtsc,
            Self::SmsPal => Model::SmsPal,
            Self::Sms1Ntsc => Model::Sms1Ntsc,
            Self::Sms1Pal => Model::Sms1Pal,
        }
    }
    const fn frame_ticks(self) -> u64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => FRAME_TICKS_PAL,
            Self::SmsNtsc | Self::Sms1Ntsc => FRAME_TICKS_NTSC,
        }
    }
}

fn parse_cli<I: IntoIterator<Item = String>>(args: I) -> Cli {
    let mut cli = Cli::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--cart" => cli.cart = Some(PathBuf::from(next_arg(&mut iter, "--cart"))),
            "--variant" => {
                cli.variant = match next_arg(&mut iter, "--variant").as_str() {
                    "sms-ntsc" | "sms" => Variant::SmsNtsc,
                    "sms-pal" => Variant::SmsPal,
                    "sms1-ntsc" | "sms1" => Variant::Sms1Ntsc,
                    "sms1-pal" => Variant::Sms1Pal,
                    other => die(&format!(
                        "--variant expects sms-ntsc|sms-pal|sms1-ntsc|sms1-pal, got {other}"
                    )),
                };
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--audio-capture" => {
                cli.audio_capture = Some(PathBuf::from(next_arg(&mut iter, "--audio-capture")));
            }
            "--script" => cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script"))),
            "--headless" => {}
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            other => die(&format!("unknown argument: {other}")),
        }
    }
    cli
}

fn next_arg<I: Iterator<Item = String>>(iter: &mut I, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

/// Headless entry point.
///
/// # Errors
///
/// Returns an error for unreadable cart, script parse / execution
/// failures, or capture I/O.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let report = run_cli(cli)?;
    println!("{}", serde_json::to_string(&report).unwrap_or_default());
    Ok(())
}

fn run_cli(cli: Cli) -> Result<serde_json::Value, String> {
    let cart_path = cli
        .cart
        .clone()
        .ok_or_else(|| "--cart PATH is required".to_string())?;
    let cart_bytes = load_cart_bytes(&cart_path)?;

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let save_path = default_battery_save_path(&cart_path);
    let mut runtime = with_cartridge(cli.variant.model(), cart_bytes);
    load_battery_save(&mut runtime, &save_path)?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        cli.variant.frame_ticks(),
        SmsSessionQueryProvider,
    );
    let media = MediaSet::new();
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let mut observations: Vec<ScriptObservation> = Vec::new();
    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| format!("script execution failed: {err}"))?,
        );
    }

    if cli.frames > 0 {
        session
            .run_frames(cli.frames)
            .map_err(|err| format!("run failed: {err}"))?;
    }
    if let Some(path) = &cli.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    if let Some(path) = &cli.audio_capture {
        session
            .save_audio_capture(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    let machine = session.machine();
    let cart_loaded = machine.machine().is_some();
    let frame_count = machine.machine().map(|m| m.frame_count()).unwrap_or(0);
    observations.extend(session.blank_frame_observation());
    write_battery_save(session.machine(), &save_path)?;
    Ok(json!({
        "cart_loaded": cart_loaded,
        "frames_run":  frame_count,
        "time":        session.time().get(),
        "observations": observations,
    }))
}

fn default_battery_save_path(cart_path: &Path) -> PathBuf {
    let mut path = cart_path.to_path_buf();
    path.set_extension("sav");
    path
}

fn load_battery_save(
    runtime: &mut runtime_sega_master_system::SmsRuntime,
    path: &Path,
) -> Result<(), String> {
    match fs::read(path) {
        Ok(bytes) => runtime
            .restore_cartridge_save_image(&bytes)
            .map_err(|err| format!("failed to restore battery save {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to read battery save {}: {err}",
            path.display()
        )),
    }
}

fn write_battery_save(
    runtime: &runtime_sega_master_system::SmsRuntime,
    path: &Path,
) -> Result<(), String> {
    let Some(image) = runtime.cartridge_save_image() else {
        return Ok(());
    };
    fs::write(path, image)
        .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
}

fn load_cart_bytes(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|err| format!("failed to read --cart {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_save_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "emu198x-sms-save-{}-{}.sav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow Unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(Vec::<String>::new());
        assert!(cli.cart.is_none());
        assert_eq!(cli.variant, Variant::SmsNtsc);
        assert_eq!(cli.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let argv = vec![
            "--cart".into(),
            "/tmp/cart".into(),
            "--variant".into(),
            "sms-pal".into(),
            "--frames".into(),
            "60".into(),
        ];
        let cli = parse_cli(argv);
        assert_eq!(cli.cart.expect("parsed by CLI"), Path::new("/tmp/cart"));
        assert_eq!(cli.variant, Variant::SmsPal);
    }

    #[test]
    fn battery_save_is_created_only_after_sram_changes_and_loads_cleanly() {
        let path = temporary_save_path();
        let mut runtime = with_cartridge(Model::SmsNtsc, vec![0; 0x10000]);

        write_battery_save(&runtime, &path).expect("clean cartridge should be skipped");
        assert!(!path.exists());

        let machine = runtime.machine_mut().expect("cartridge should be loaded");
        machine.poke(0xFFFC, 0x08);
        machine.poke(0x8123, 0x5A);
        write_battery_save(&runtime, &path).expect("changed SRAM should save");
        assert_eq!(fs::metadata(&path).expect("save should exist").len(), 32768);

        let mut restored = with_cartridge(Model::SmsNtsc, vec![0; 0x10000]);
        load_battery_save(&mut restored, &path).expect("save should load");
        let machine = restored.machine_mut().expect("cartridge should be loaded");
        machine.poke(0xFFFC, 0x08);
        assert_eq!(machine.peek(0x8123), 0x5A);
        assert!(restored.cartridge_save_image().is_none());

        fs::remove_file(path).expect("temporary save should be removable");
    }
}
