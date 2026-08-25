//! Headless / script execution mode.
//!
//! Boots a Spectrum runtime, optionally translates surviving CLI
//! convenience flags (`--tape`, `--play-tape`, `--autoload-tape`) into
//! prepended `ScriptStep`s, then iterates the script (CLI-derived
//! steps + JSON-file steps if provided). System-specific steps —
//! `SetMachine`, `AutoloadTape` — are intercepted before the shell
//! executor sees them; everything else delegates to
//! `ScriptStep::execute_collect`.
//!
//! Default boot policy is **eager 48K**: a run that names no variant
//! uses the 48K runtime. Preserves Code198x's existing screenshot/video
//! pipelines, which assume 48K implicitly. Two things override it —
//! `--machine ID`, and a script whose first portable `LoadSnapshot`
//! targets a non-48K image. A mid-script `set_machine` step instead
//! swaps variant during the run.
//!
//! Output: a `RunnerReport` JSON document on stdout when a script file
//! is supplied, or a one-line tape-state summary otherwise.

pub mod runner;

use std::path::PathBuf;
use std::process;

use crate::AppError;
use crate::script::runner::{ScriptInputs, run_script};

const USAGE: &str = "\
Usage: emu198x-spectrum --headless [OPTIONS]
       emu198x-spectrum --script PATH [OPTIONS]

Boots a default 48K runtime headless and executes the supplied JSON
script (if any), plus any prepended convenience-flag steps.

Options:
    --script PATH        execute the JSON session at PATH
    --headless           run without a window (implied by --script)
    --machine ID         boot this variant instead of the default 48K.
                         One of: spectrum_16k, spectrum_48k,
                         spectrum_plus, spectrum_128k, spectrum_plus2,
                         spectrum_plus2a, spectrum_plus2b,
                         spectrum_plus3, pentagon_128, scorpion_zs256,
                         timex_tc2048, timex_tc2068, timex_ts2068.
                         Selects the boot variant; use a mid-script
                         { \"action\": \"set_machine\" } step to swap
                         variant during a run.
    --tape PATH          load a tape image into slot tape-1
                         (== { \"action\": \"load_media\", \"slot\": \"tape-1\",
                              \"kind\": \"tape\", \"path\": PATH })
    --play-tape          start tape transport on tape-1
                         (== { \"action\": \"media_transport\", \"slot\": \"tape-1\",
                              \"transport\": \"start\" })
    --autoload-tape      wait for boot, type LOAD \"\", and start tape-1
                         (== { \"action\": \"autoload_tape\", \"slot\": \"tape-1\",
                              \"max_boot_frames\": 250 })
    --rom ID=PATH        boot this ROM instead of the conventional one.
                         Repeatable; each ID names one entry of the
                         variant's bundle and the rest still resolve
                         under ~/.emu198x/roms. An ID the variant does
                         not have is an error, not a silent fallback.
                         48K family:  sinclair-zx-spectrum-48k-rom
                         128K:        sinclair-zx-spectrum-128k-rom-{0,1}
                         +2:          sinclair-zx-spectrum-plus2-rom-{0,1}
                         +2A/+3:      sinclair-zx-spectrum-plus3-rom-{0..3}
    --help, -h           show this help

For richer automation (snapshots, screenshots, audio capture, frame
counts, query waits) write a JSON script. `ScriptStep` in
`crates/emu198x-shell/src/script.rs` is the schema.

Examples:
    emu198x-spectrum --script boot.json
    emu198x-spectrum --headless --tape manic.tzx --autoload-tape --script run.json
    emu198x-spectrum --headless --machine spectrum_128k --tape testInt.tap --script run.json
    emu198x-spectrum --headless --rom sinclair-zx-spectrum-48k-rom=/roms/48.rom --script run.json
";

/// CLI surface for headless / script mode.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScriptCli {
    /// Optional JSON session file to execute.
    pub script: Option<PathBuf>,
    /// Variant to boot, as a `MachineKind` script identifier. `None`
    /// keeps the default 48K boot policy.
    pub machine: Option<String>,
    /// Tape media to load into `tape-1` before script execution.
    pub tape: Option<PathBuf>,
    /// Start tape transport on `tape-1` immediately.
    pub play_tape: bool,
    /// Run the BASIC autoload sequence on `tape-1` once boot is detected.
    pub autoload_tape: bool,
    /// Raw `--rom` values, resolved against the boot variant's bundle.
    pub rom: Vec<String>,
    /// Frames to run before capturing. The other twenty-nine binaries
    /// take this; the Spectrum did not, so the machine this project's
    /// curriculum leads with was the one that needed a JSON file to
    /// take a picture (#1187).
    pub frames: u32,
    /// Write a PNG of the final frame here.
    pub screenshot: Option<PathBuf>,
    /// Write a WAV of the captured audio here.
    pub audio_capture: Option<PathBuf>,
}

/// Parses CLI args for headless / script mode. Mode flags (`--headless`,
/// `--script`) are consumed here; the dispatcher in `main.rs` detects
/// the mode by scanning for the same flags before calling this.
pub fn parse_cli<I>(args: I) -> ScriptCli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = ScriptCli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            // Mode markers; already detected by the dispatcher. `--mcp` is
            // here because MCP mode reuses this parser to pick up `--rom`,
            // and the catch-all below would otherwise kill the process on
            // the very flag that selected the mode.
            "--headless" | "--mcp" => {}
            "--script" => {
                cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script")));
            }
            "--machine" => cli.machine = Some(next_arg(&mut iter, "--machine")),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames").parse().unwrap_or_else(|_| {
                    die("--frames expects a number of frames");
                });
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--audio-capture" => {
                cli.audio_capture = Some(PathBuf::from(next_arg(&mut iter, "--audio-capture")));
            }
            "--play-tape" => cli.play_tape = true,
            "--autoload-tape" => cli.autoload_tape = true,
            "--rom" => {
                // Resolution needs the boot variant, which `--machine` may
                // not have supplied yet, so keep the raw spec and resolve in
                // `run` once the variant is known.
                cli.rom.push(next_arg(&mut iter, "--rom"));
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

/// Runs script mode. Eager 48K boot; runs the supplied script (if any)
/// after prepending convenience-flag steps.
pub fn run(cli: ScriptCli) -> Result<(), AppError> {
    let inputs = ScriptInputs {
        script: cli.script.clone(),
        frames: cli.frames,
        screenshot: cli.screenshot.clone(),
        audio_capture: cli.audio_capture.clone(),
        machine: cli.machine.clone(),
        tape: cli.tape.clone(),
        play_tape: cli.play_tape,
        autoload_tape: cli.autoload_tape,
        rom: cli.rom.clone(),
    };

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        die("capture requests require either --frames or --script so the machine emits output");
    }

    let report = run_script(inputs)?;

    if cli.script.is_some() {
        let json = serde_json::to_string(&report).map_err(|err| {
            AppError::Io(std::io::Error::other(format!(
                "failed to serialize runner report: {err}"
            )))
        })?;
        println!("{json}");
    } else {
        println!(
            "Spectrum runtime: time={} tape_loaded={} tape_playing={}",
            report.time, report.tape_loaded, report.tape_playing
        );
    }

    Ok(())
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
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
    fn parse_cli_defaults_are_empty() {
        let cli = parse_cli(std::iter::empty::<String>());
        assert_eq!(cli, ScriptCli::default());
    }

    #[test]
    fn parse_cli_reads_script_path() {
        let cli = parse_cli(["--script".to_owned(), "boot.json".to_owned()]);
        assert_eq!(cli.script, Some(PathBuf::from("boot.json")));
    }

    #[test]
    fn parse_cli_accepts_convenience_aliases() {
        let cli = parse_cli([
            "--headless".to_owned(),
            "--tape".to_owned(),
            "manic.tzx".to_owned(),
            "--autoload-tape".to_owned(),
            "--script".to_owned(),
            "run.json".to_owned(),
        ]);
        assert_eq!(
            cli,
            ScriptCli {
                script: Some(PathBuf::from("run.json")),
                machine: None,
                tape: Some(PathBuf::from("manic.tzx")),
                play_tape: false,
                autoload_tape: true,
                rom: Vec::new(),
                frames: 0,
                screenshot: None,
                audio_capture: None,
            }
        );
    }

    #[test]
    fn parse_cli_reads_machine_id() {
        let cli = parse_cli([
            "--headless".to_owned(),
            "--machine".to_owned(),
            "spectrum_128k".to_owned(),
            "--tape".to_owned(),
            "testInt.tap".to_owned(),
        ]);
        assert_eq!(cli.machine.as_deref(), Some("spectrum_128k"));
        assert_eq!(cli.tape, Some(PathBuf::from("testInt.tap")));
    }

    /// The flag is not validated at parse time — `run_script` resolves
    /// it against `MachineKind::from_script_id` so the error carries the
    /// enum-derived list of accepted identifiers.
    #[test]
    fn parse_cli_does_not_validate_machine_id() {
        let cli = parse_cli(["--machine".to_owned(), "spectrum_999k".to_owned()]);
        assert_eq!(cli.machine.as_deref(), Some("spectrum_999k"));
    }

    #[test]
    fn parse_cli_play_tape_alone_is_supported() {
        let cli = parse_cli([
            "--tape".to_owned(),
            "demo.tap".to_owned(),
            "--play-tape".to_owned(),
        ]);
        assert!(cli.play_tape);
        assert!(!cli.autoload_tape);
        assert_eq!(cli.tape, Some(PathBuf::from("demo.tap")));
    }
}

#[cfg(test)]
mod capture_flag_tests {
    use super::*;

    /// #1187: twenty-nine binaries took these and the Spectrum did not,
    /// so the machine the curriculum leads with was the only one that
    /// needed a JSON file to take a picture.
    #[test]
    fn the_capture_flags_parse() {
        let cli = parse_cli([
            "--headless".to_owned(),
            "--frames".to_owned(),
            "120".to_owned(),
            "--screenshot".to_owned(),
            "boot.png".to_owned(),
            "--audio-capture".to_owned(),
            "boot.wav".to_owned(),
        ]);
        assert_eq!(cli.frames, 120);
        assert_eq!(cli.screenshot, Some(PathBuf::from("boot.png")));
        assert_eq!(cli.audio_capture, Some(PathBuf::from("boot.wav")));
    }

    #[test]
    fn capture_flags_default_to_off() {
        let cli = parse_cli(["--headless".to_owned()]);
        assert_eq!(cli.frames, 0);
        assert!(cli.screenshot.is_none());
        assert!(cli.audio_capture.is_none());
    }
}
