//! Headless / script execution mode.
//!
//! Boots a default 48K runtime, optionally translates surviving CLI
//! convenience flags (`--tape`, `--play-tape`, `--autoload-tape`) into
//! prepended `ScriptStep`s, then iterates the script (CLI-derived
//! steps + JSON-file steps if provided). System-specific steps —
//! `SetMachine`, `AutoloadTape` — are intercepted before the shell
//! executor sees them; everything else delegates to
//! `ScriptStep::execute_collect`.
//!
//! Default boot policy is **eager 48K**: a script that doesn't include
//! `set_machine` runs on the default 48K runtime. Preserves Code198x's
//! existing screenshot/video pipelines, which assume 48K implicitly.
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
    --tape PATH          load a tape image into slot tape-1
                         (== { \"action\": \"load_media\", \"slot\": \"tape-1\",
                              \"kind\": \"tape\", \"path\": PATH })
    --play-tape          start tape transport on tape-1
                         (== { \"action\": \"media_transport\", \"slot\": \"tape-1\",
                              \"transport\": \"start\" })
    --autoload-tape      wait for boot, type LOAD \"\", and start tape-1
                         (== { \"action\": \"autoload_tape\", \"slot\": \"tape-1\",
                              \"max_boot_frames\": 250 })
    --help, -h           show this help

For richer automation (firmware overrides, snapshots, screenshots,
audio capture, frame counts, query waits) write a JSON script.
See `wiki/decisions/script-vocabulary.md` for the schema.

Examples:
    emu198x-spectrum --script boot.json
    emu198x-spectrum --headless --tape manic.tzx --autoload-tape --script run.json
";

/// CLI surface for headless / script mode.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScriptCli {
    /// Optional JSON session file to execute.
    pub script: Option<PathBuf>,
    /// Tape media to load into `tape-1` before script execution.
    pub tape: Option<PathBuf>,
    /// Start tape transport on `tape-1` immediately.
    pub play_tape: bool,
    /// Run the BASIC autoload sequence on `tape-1` once boot is detected.
    pub autoload_tape: bool,
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
            "--headless" => {} // Mode marker; already detected by the dispatcher.
            "--script" => {
                cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script")));
            }
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--play-tape" => cli.play_tape = true,
            "--autoload-tape" => cli.autoload_tape = true,
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
        tape: cli.tape.clone(),
        play_tape: cli.play_tape,
        autoload_tape: cli.autoload_tape,
    };

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
                tape: Some(PathBuf::from("manic.tzx")),
                play_tape: false,
                autoload_tape: true,
            }
        );
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
