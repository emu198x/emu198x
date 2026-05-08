//! Interactive UI mode — winit window, native muda menu, wgpu video,
//! cpal audio, frame-paced runtime loop.
//!
//! Entry point is [`run`]; the dispatcher in `main.rs` calls it after
//! parsing CLI args. Mode-flag parsing for `--mcp` / `--headless` /
//! `--script PATH` lives in `main.rs`; this module owns only the
//! UI-specific arguments (display config + convenience media aliases).

pub mod app;
pub mod input;
pub mod menu;
pub mod runner;

use std::path::PathBuf;
use std::process;

use emu198x_native_video::VideoFilter;
use winit::event_loop::EventLoop;

use crate::AppError;
use crate::ui::app::SpectrumApp;
use crate::ui::runner::SpectrumRunner;

const DEFAULT_SCALE: u32 = 2;

const USAGE: &str = "\
Usage: emu198x-spectrum [OPTIONS]

Options:
    --rom PATH         48K ROM image or zip containing one ROM candidate
    --tape PATH        TAP/TZX image or zip containing one tape candidate
    --play-tape        start tape transport immediately after media load
    --autoload-tape    wait for boot, type LOAD \"\", and start tape-1
    --turbo-tape       run unthrottled while the tape is playing
    --scale N          integer window scale, default 2
    --video MODE       raw | lcd | crt [default: raw]
    --help, -h         show this help

Controls:
    Esc                quit
    F9                 start tape
    F10                stop tape
    F11                toggle tape turbo
    F12                hard reset
    Numpad 1           toggle speaker output
    Numpad 2           cycle speaker gain
    Numpad 0           reset speaker controls
    Left/Down/Up/Right physical Spectrum cursor keys (Caps Shift + 5/6/7/8)
    Alt                Symbol Shift

Examples:
    emu198x-spectrum
    emu198x-spectrum --rom 48.rom --tape manic_miner.zip
    emu198x-spectrum --tape manic_miner.zip --autoload-tape
";

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

/// Runs the UI mode. Constructs the runner from the parsed CLI,
/// builds the App, and hands off to winit's event loop. Surfaces any
/// fatal error captured during the loop run.
pub fn run(cli: Cli) -> Result<(), AppError> {
    println!(
        "Controls: Esc quit, F9 start tape, F10 stop tape, F11 tape turbo, F12 reset, numpad 1 toggle speaker, numpad 2 cycle speaker gain, numpad 0 reset audio."
    );

    let runner = SpectrumRunner::from_cli(&cli)?;
    let mut app = SpectrumApp::new(runner, cli.scale, cli.turbo_tape, cli.video)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    if let Some(err) = app.take_error() {
        return Err(err);
    }

    Ok(())
}

/// Parses one CLI invocation into [`Cli`]. Currently UI-only; mode-flag
/// parsing for `--mcp` / `--headless` / `--script PATH` will move into
/// the dispatcher in `main.rs` when other modes land.
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
                cli.video = parse_video_arg(&next_arg(&mut iter, "--video"));
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

fn parse_video_arg(video: &str) -> VideoFilter {
    video
        .parse()
        .unwrap_or_else(|_| die("--video expects raw, lcd, or crt"))
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
    process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults_to_scale_two() {
        let cli = parse_cli(std::iter::empty::<String>());

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: None,
                play_tape: false,
                autoload_tape: false,
                turbo_tape: false,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_rom_tape_and_scale() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "48.rom".to_owned(),
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--play-tape".to_owned(),
            "--scale".to_owned(),
            "3".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("48.rom")),
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: true,
                autoload_tape: false,
                turbo_tape: false,
                scale: 3,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_autoload() {
        let cli = parse_cli([
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--autoload-tape".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: true,
                turbo_tape: false,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_tape_turbo() {
        let cli = parse_cli([
            "--tape".to_owned(),
            "manic.zip".to_owned(),
            "--turbo-tape".to_owned(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: false,
                turbo_tape: true,
                scale: 2,
                video: VideoFilter::Raw,
            }
        );
    }

    #[test]
    fn parse_cli_accepts_positional_tape_path() {
        let cli = parse_cli(["manic.zip".to_owned()]);

        assert_eq!(
            cli,
            Cli {
                rom: None,
                tape: Some(PathBuf::from("manic.zip")),
                play_tape: false,
                autoload_tape: false,
                turbo_tape: false,
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
}
