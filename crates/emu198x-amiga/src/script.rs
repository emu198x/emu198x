//! Headless Amiga runner — `--script` / `--headless` mode.
//!
//! Boots the chosen Amiga model from Kickstart firmware, runs native
//! frames, executes shared JSON session steps, inserts a DF0 ADF, and
//! captures screenshots / audio / boot-state queries. The
//! non-interactive half of the `emu198x-amiga` binary; the dispatcher
//! in `main.rs` routes here when a headless-only flag is present. The
//! rich chip-level debugging surface lives in `--mcp` mode.

use std::path::PathBuf;
use std::process;

use emu198x_shell::{
    BootArtifacts, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession,
    MediaImage, MediaKind, MediaSet, ScriptObservation, ScriptStep, boot_machine,
    read_firmware_asset, read_media_asset,
};
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AmigaSessionQueryProvider};
use serde::Serialize;
use serde_json::Value;

// Model selection + Kickstart resolution are shared with the UI and MCP
// modes (`crate::model`); script mode used to carry a parallel copy whose
// ROM-candidate lists had drifted (A500+ tried KS1.3 before KS2.04). All
// three modes now resolve through `crate::find_rom_path`.
use crate::mcp::tools::AmigaCtx;
use crate::{ModelArg, find_rom_path, firmware_id_for_model_arg, parse_model_arg};

const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    model: ModelArg,
    rom_dir: Option<PathBuf>,
    kickstart: Option<PathBuf>,
    disk: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
    wait_for_boot: Option<u32>,
    print_queries: Vec<String>,
    frames: u32,
}

#[derive(Debug, Serialize)]
struct RunnerReport {
    observations: Vec<ScriptObservation>,
    time: u64,
    boot_detected: bool,
    boot_reason: String,
    query_values: Vec<ReportedQuery>,
}

#[derive(Debug, Serialize)]
struct ReportedQuery {
    path: String,
    value: Value,
}

const USAGE: &str = "\
Usage: emu198x-amiga --headless [OPTIONS]   (add --no-default-features for graphics-free builds)

Firmware:
    --rom-dir DIR             directory containing Kickstart ROM images
    --kickstart PATH          explicit ROM path (Kickstart on A500, bootstrap on A1000)
    --model MODEL             a1000 | a500 | a500-gvp-a530 | a500-a501
                              | a500-plus | a500-maxed | a600 | a1200 | a2000
                              [default: a500]

Media:
    --disk PATH               insert one ADF image into DF0:

Automation:
    --script PATH             execute shared JSON session steps
    --wait-for-boot N         run up to N frames until boot.detected is true
    --print-query PATH        resolve one query path after running (repeatable)
    --frames N                number of native video frames to run
    --screenshot PATH         write the last emitted frame as PNG
    --audio-capture PATH      write emitted audio as 16-bit PCM WAV

Other:
    --help, -h                show this help

ROM directory resolution (first match wins):
    1. --rom-dir DIR
    2. EMU198X_AMIGA_ROM_DIR
    3. ~/.emu198x/roms/commodore-amiga
    4. ~/.emu198x/roms/amiga

Filename resolution inside the ROM directory:
    A1000:
    - a1000-bootstrap.rom
    - a1000_bootstrap.rom
    - bootstrap.rom

    Other models:
    - kick13.rom
    - kick12.rom
    - kick31.rom
    - kickstart.rom
    - kick.rom

Examples:
    emu198x-amiga --headless --wait-for-boot 300 --screenshot kick13.png
    emu198x-amiga --headless --disk workbench13.adf --wait-for-boot 400
    emu198x-amiga --headless --model a500-a501 --disk workbench13.adf --frames 900 --screenshot wb13.png
";

/// Headless entry point. Parses the automation CLI, runs the session,
/// and prints the JSON (script mode) or summary report.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let cli = parse_cli(args);
    let script_mode = cli.script.is_some();
    let report = run_cli(cli)?;
    if script_mode {
        let json = serde_json::to_string(&report)
            .map_err(|err| format!("failed to serialize runner report: {err}"))?;
        println!("{json}");
    } else {
        println!(
            "Amiga runtime: time={} boot_detected={} boot_reason={}",
            report.time, report.boot_detected, report.boot_reason
        );
        for query in &report.query_values {
            println!("{}={}", query.path, query.value);
        }
    }
    Ok(())
}

fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom-dir" => cli.rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kickstart" => {
                cli.kickstart = Some(PathBuf::from(next_arg(&mut iter, "--kickstart")));
            }
            "--model" => cli.model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--disk" => cli.disk = Some(PathBuf::from(next_arg(&mut iter, "--disk"))),
            "--script" => cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script"))),
            "--wait-for-boot" => {
                cli.wait_for_boot = Some(
                    next_arg(&mut iter, "--wait-for-boot")
                        .parse()
                        .unwrap_or_else(|_| die("--wait-for-boot requires a non-negative integer")),
                );
            }
            "--print-query" => cli.print_queries.push(next_arg(&mut iter, "--print-query")),
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
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            "--headless" => {}
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a path or value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

fn run_cli(cli: Cli) -> Result<RunnerReport, String> {
    if cli.screenshot.is_some()
        && cli.frames == 0
        && cli.script.is_none()
        && cli.wait_for_boot.is_none()
    {
        return Err(
            "capture requests require --frames, --script, or --wait-for-boot so the machine emits output".into(),
        );
    }

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
    let artifacts = BootArtifacts {
        firmware,
        snapshot: None,
    };

    let machine = boot_machine(
        &artifacts,
        |images| AmigaRuntimeKind::from_firmware(model, images),
        || AmigaRuntimeKind::blank(model),
    )
    .map_err(|err| format!("machine construction failed: {err}"))?;

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        A500_PAL_FRAME_TICKS,
        AmigaSessionQueryProvider,
    );

    let mut media_storage = Vec::new();
    let mut media = MediaSet::new();
    if let Some(path) = &cli.disk {
        let loaded = read_media_asset(path, MediaKind::Disk)
            .map_err(|err| format!("failed to read disk {}: {err}", path.display()))?;
        media_storage.push(loaded);
        let bytes = &media_storage
            .last()
            .expect("media_storage just received one disk image")
            .bytes;
        media.push(MediaImage::new(DEFAULT_FLOPPY_SLOT, MediaKind::Disk, bytes));
    }

    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let mut observations = Vec::new();
    if let Some(max_frames) = cli.wait_for_boot {
        session
            .wait_for_boot(max_frames)
            .map_err(|err| format!("boot wait failed: {err}"))?;
    }

    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        // Only `set_machine` is intercepted; everything else delegates to the
        // shared executor. press_key / press_keys / type_string now run through
        // the machine's `KeyboardTarget` there (the Amiga keymap is wired in).
        for step in &script.steps {
            match step {
                // Shared with the MCP `set_machine` tool: resolve the model's
                // Kickstart by convention and swap the session's variant via
                // `HeadlessSession::swap_machine`. Script mode holds the family
                // enum (`AmigaRuntimeKind`), so mid-script model swaps work (#456).
                ScriptStep::SetMachine { machine } => {
                    let outcome = session
                        .set_machine(machine)
                        .map_err(|err| format!("set_machine to `{machine}` failed: {err}"))?;
                    observations.push(ScriptObservation::SetMachine {
                        machine: machine.clone(),
                        profile_id: outcome.profile_id,
                        display_name: outcome.display_name,
                    });
                }
                other => {
                    if let Some(observation) = other
                        .execute_collect(&mut session)
                        .map_err(|err| format!("script execution failed: {err}"))?
                    {
                        observations.push(observation);
                    }
                }
            }
        }
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

    let query_values = cli
        .print_queries
        .iter()
        .map(|path| {
            session
                .query(path)
                .map(|query| ReportedQuery {
                    path: path.clone(),
                    value: query.value,
                })
                .map_err(|err| format!("failed to resolve query {path}: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let boot_detected = query_bool(&session, "boot.detected")?;
    let boot_reason = query_string(&session, "boot.reason")?;

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        boot_detected,
        boot_reason,
        query_values,
    })
}

fn query_bool(
    session: &HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>,
    path: &str,
) -> Result<bool, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_bool()
        .ok_or_else(|| format!("query {path} did not resolve to a boolean"))
}

fn query_string(
    session: &HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>,
    path: &str,
) -> Result<String, String> {
    session
        .query(path)
        .map_err(|err| format!("failed to query {path}: {err}"))?
        .value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("query {path} did not resolve to a string"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const ADF_SIZE_DD: usize = 80 * 2 * 11 * 512;

    fn dummy_kickstart() -> Vec<u8> {
        let mut kickstart = vec![0u8; 256 * 1024];
        kickstart[0] = 0x00;
        kickstart[1] = 0x08;
        kickstart[2] = 0x00;
        kickstart[3] = 0x00;
        kickstart[4] = 0x00;
        kickstart[5] = 0xF8;
        kickstart[6] = 0x00;
        kickstart[7] = 0x08;
        kickstart[8] = 0x60;
        kickstart[9] = 0xFE;
        kickstart
    }

    #[test]
    fn parse_cli_accepts_kickstart_disk_and_capture_flags() {
        let cli = parse_cli([
            "--model".to_owned(),
            "a500-a501".to_owned(),
            "--kickstart".to_owned(),
            "kick13.rom".to_owned(),
            "--disk".to_owned(),
            "workbench.adf".to_owned(),
            "--frames".to_owned(),
            "12".to_owned(),
            "--screenshot".to_owned(),
            "frame.png".to_owned(),
            "--audio-capture".to_owned(),
            "audio.wav".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                model: ModelArg::A500A501,
                rom_dir: None,
                kickstart: Some(PathBuf::from("kick13.rom")),
                disk: Some(PathBuf::from("workbench.adf")),
                screenshot: Some(PathBuf::from("frame.png")),
                audio_capture: Some(PathBuf::from("audio.wav")),
                script: None,
                wait_for_boot: None,
                print_queries: vec![],
                frames: 12,
            }
        );
    }

    /// A `SetMachine` step in a `--script` run is intercepted and swaps
    /// the live variant (it used to fall through to the shell executor
    /// and error as `SystemSpecificStep`). Launches a dummy-ROM A500,
    /// runs a one-step script swapping to the AGA A1200, and asserts the
    /// SetMachine observation lands. The swap resolves the A1200
    /// Kickstart by convention, so it skips when that ROM is absent.
    #[test]
    fn script_set_machine_step_swaps_variant() {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let a1200_rom = PathBuf::from(&home).join(".emu198x/roms/commodore-amiga/kick31a1200.rom");
        if !a1200_rom.exists() {
            eprintln!(
                "skipping: A1200 Kickstart missing at {}",
                a1200_rom.display()
            );
            return;
        }

        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let kickstart_path = temp_dir.join(format!("emu198x-amiga-{pid}-setmachine-kick.rom"));
        let script_path = temp_dir.join(format!("emu198x-amiga-{pid}-setmachine.json"));
        fs::write(&kickstart_path, dummy_kickstart()).expect("write dummy Kickstart");
        fs::write(
            &script_path,
            r#"[{"action":"set_machine","machine":"a1200"}]"#,
        )
        .expect("write script");

        let result = run_cli(Cli {
            model: ModelArg::A500,
            rom_dir: None,
            kickstart: Some(kickstart_path.clone()),
            disk: None,
            screenshot: None,
            audio_capture: None,
            script: Some(script_path.clone()),
            wait_for_boot: None,
            print_queries: vec![],
            frames: 0,
        })
        .expect("script with SetMachine should run, not error as unsupported");

        let swapped = result.observations.iter().find_map(|obs| match obs {
            ScriptObservation::SetMachine {
                machine,
                profile_id,
                ..
            } => Some((machine.clone(), profile_id.clone())),
            _ => None,
        });
        let (machine, profile_id) = swapped.expect("a SetMachine observation must be emitted");
        assert_eq!(machine, "a1200");
        assert!(
            profile_id.contains("a1200"),
            "swapped profile should be an A1200 variant, got {profile_id}"
        );

        let _ = fs::remove_file(kickstart_path);
        let _ = fs::remove_file(script_path);
    }

    #[test]
    fn run_can_capture_png_and_wav() {
        let temp_dir = std::env::temp_dir();
        let kickstart_path =
            temp_dir.join(format!("emu198x-amiga-{}-kick13.rom", std::process::id()));
        let screenshot_path =
            temp_dir.join(format!("emu198x-amiga-{}-frame.png", std::process::id()));
        let audio_path = temp_dir.join(format!("emu198x-amiga-{}-audio.wav", std::process::id()));
        let disk_path = temp_dir.join(format!("emu198x-amiga-{}-disk.adf", std::process::id()));

        fs::write(&kickstart_path, dummy_kickstart())
            .expect("temporary Kickstart write should succeed");
        fs::write(&disk_path, vec![0u8; ADF_SIZE_DD]).expect("temporary ADF write should succeed");

        let result = run_cli(Cli {
            model: ModelArg::A500,
            rom_dir: None,
            kickstart: Some(kickstart_path.clone()),
            disk: Some(disk_path.clone()),
            screenshot: Some(screenshot_path.clone()),
            audio_capture: Some(audio_path.clone()),
            script: None,
            wait_for_boot: None,
            print_queries: vec!["disk.inserted".to_owned()],
            frames: 2,
        })
        .expect("runner should capture png and wav");

        assert_eq!(result.query_values.len(), 1);
        assert_eq!(result.query_values[0].path, "disk.inserted");
        assert_eq!(result.query_values[0].value, Value::Bool(true));
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());
        let wav = fs::read(&audio_path).expect("wav should be readable");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(
            wav.len() > 44,
            "runtime audio capture should contain sample data, not only a WAV header"
        );

        let _ = fs::remove_file(kickstart_path);
        let _ = fs::remove_file(disk_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
    }
}
