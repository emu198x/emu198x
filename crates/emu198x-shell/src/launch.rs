//! Shared entry point for the per-machine binaries.
//!
//! Every `emu198x-<machine>` binary has the same three modes — windowed,
//! `--script`, `--mcp` — and the same common flags. This module owns what is
//! the same for every machine: mode detection, the common flags, the
//! conventional firmware lookup, the headless run-and-report loop, and the
//! MCP server setup. A binary supplies the rest through [`MachineApp`]: its
//! own flags, how to build its runtime, and the machine-specific fields of
//! the headless report.
//!
//! Windowed mode needs `emu198x-ui`, which depends on this crate, so the
//! dispatcher stops at [`Outcome::WantsUi`] and lets the UI crate's launcher
//! take over. A binary built without its `ui` feature calls
//! [`main_headless`] instead and gets the "rebuild with --features ui"
//! message for that case.

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::str::FromStr;

use serde_json::{Map, Value};

use crate::machine::MachineCore;
use crate::mcp::{Server, ServerInfo, ToolRegistry, serve_stdio};
use crate::mcp_tools::{register_base_tools, register_keyboard_tools};
use crate::media::MediaKind;
use crate::query::SessionQueryProvider;
use crate::script::{HeadlessScript, ScriptObservation};
use crate::session::HeadlessSession;
use crate::startup_media;

/// Which of the three modes the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Interactive window — the default when no automation flag is present.
    Ui,
    /// Headless JSON script / frame run with captures and a report.
    Script,
    /// MCP server on stdio.
    Mcp,
}

/// Flags only the headless runner understands. Their presence routes to
/// script mode so existing invocations keep working; media flags such as
/// `--rom` are shared with the UI, so a bare `--rom game.rom` opens the
/// interactive window.
const SCRIPT_FLAGS: &[&str] = &[
    "--script",
    "--frames",
    "--screenshot",
    "--audio-capture",
    "--headless",
];

const MCP_FLAGS: &[&str] = &["--mcp", "--mcp-stdio"];

/// Pick the mode from the raw arguments.
#[must_use]
pub fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|arg| MCP_FLAGS.contains(&arg.as_str())) {
        Mode::Mcp
    } else if args.iter().any(|arg| SCRIPT_FLAGS.contains(&arg.as_str())) {
        Mode::Script
    } else {
        Mode::Ui
    }
}

/// Why a launch stopped before the machine ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The command line was malformed. Exits 2 and points at `--help`.
    Usage(String),
    /// The machine could not be built or run. Exits 1.
    Run(String),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Run(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LaunchError {}

impl From<String> for LaunchError {
    /// A bare string from a machine's own code is a run-time failure; the
    /// launcher raises usage errors itself.
    fn from(message: String) -> Self {
        Self::Run(message)
    }
}

/// Cursor over the command line, handed to [`MachineApp::parse_flag`] so a
/// machine reads its flag values the same way the launcher does.
#[derive(Debug)]
pub struct Args {
    inner: std::vec::IntoIter<String>,
}

impl Args {
    #[must_use]
    pub fn new(args: Vec<String>) -> Self {
        Self {
            inner: args.into_iter(),
        }
    }

    /// The next raw argument, if any.
    pub fn next_flag(&mut self) -> Option<String> {
        self.inner.next()
    }

    /// The value that must follow `flag`.
    ///
    /// # Errors
    ///
    /// Returns a usage message when the command line ends after the flag.
    pub fn value(&mut self, flag: &str) -> Result<String, LaunchError> {
        self.inner
            .next()
            .ok_or_else(|| LaunchError::Usage(format!("{flag} requires a value")))
    }

    /// The path that must follow `flag`.
    ///
    /// # Errors
    ///
    /// Returns a usage message when the command line ends after the flag.
    pub fn path(&mut self, flag: &str) -> Result<PathBuf, LaunchError> {
        self.value(flag).map(PathBuf::from)
    }

    /// The value that must follow `flag`, parsed as `T`. `expects` names the
    /// accepted form in the error, e.g. `"a positive integer"`.
    ///
    /// # Errors
    ///
    /// Returns a usage message when the value is missing or does not parse.
    pub fn parse<T: FromStr>(&mut self, flag: &str, expects: &str) -> Result<T, LaunchError> {
        let value = self.value(flag)?;
        value
            .parse()
            .map_err(|_| LaunchError::Usage(format!("{flag} expects {expects}, got {value}")))
    }
}

/// The conventional location of a firmware image: `$env_var` when set and
/// non-empty, else `~/.emu198x/roms/<relative>`.
///
/// Every machine looked in the same two places with its own copy of this
/// function; the ROM directory layout is a family convention, so it lives
/// here.
#[must_use]
pub fn conventional_rom_path(env_var: &str, relative: &str) -> Option<PathBuf> {
    if let Ok(path) = env::var(env_var)
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms").join(relative))
}

/// The firmware path to use: `explicit` from the command line, else the
/// conventional one.
///
/// # Errors
///
/// Returns a message naming the flag and variable to set when neither is
/// available.
pub fn resolve_rom(
    explicit: Option<&Path>,
    env_var: &str,
    relative: &str,
) -> Result<PathBuf, LaunchError> {
    explicit
        .map(Path::to_path_buf)
        .or_else(|| conventional_rom_path(env_var, relative))
        .ok_or_else(|| LaunchError::Run(format!("no ROM: pass --rom PATH or set {env_var}")))
}

/// Read a firmware or media image, naming `what` in the error.
///
/// # Errors
///
/// Returns a message with the path when the file cannot be read.
pub fn read_rom(path: &Path, what: &str) -> Result<Vec<u8>, LaunchError> {
    fs::read(path)
        .map_err(|err| LaunchError::Run(format!("failed to read {what} {}: {err}", path.display())))
}

/// [`read_rom`] for an image that must be exactly `expected` bytes.
///
/// # Errors
///
/// Returns a message with the path when the file cannot be read or has the
/// wrong size.
pub fn read_rom_exact(path: &Path, what: &str, expected: usize) -> Result<Vec<u8>, LaunchError> {
    let bytes = read_rom(path, what)?;
    if bytes.len() != expected {
        return Err(LaunchError::Run(format!(
            "{what} at {} is {} bytes; expected {expected}",
            path.display(),
            bytes.len()
        )));
    }
    Ok(bytes)
}

/// The flags every binary accepts, parsed by the launcher.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CommonCli {
    /// `--scale N`; `None` leaves the machine's default in place.
    pub scale: Option<u32>,
    /// `--video MODE`, unparsed: the filter type lives in the UI crate.
    pub video: Option<String>,
    /// `--frames N` — frames to run headlessly after any script.
    pub frames: u32,
    /// `--screenshot PATH`.
    pub screenshot: Option<PathBuf>,
    /// `--audio-capture PATH`.
    pub audio_capture: Option<PathBuf>,
    /// `--script PATH`.
    pub script: Option<PathBuf>,
}

/// What one machine binary contributes to the shared launcher.
///
/// The launcher parses the common flags and hands every other flag to
/// [`parse_flag`](Self::parse_flag). It then builds the runtime through
/// [`build_runtime`](Self::build_runtime) (or
/// [`build_mcp_runtime`](Self::build_mcp_runtime) for `--mcp`) and runs the
/// mode the flags asked for.
pub trait MachineApp: Default {
    /// The machine runtime the binary drives.
    type Runtime: MachineCore + 'static;
    /// The session query provider for `--script` and MCP queries.
    type Query: SessionQueryProvider<Self::Runtime> + 'static;

    /// The binary's name, for usage text and the MCP server identity.
    const BIN_NAME: &'static str;
    /// The binary's version, for the MCP server identity. Pass
    /// `env!("CARGO_PKG_VERSION")` from the binary crate.
    const VERSION: &'static str;
    /// Help lines for the machine's own flags, one per line, indented four
    /// spaces to match the shared ones. Shown under `Options:`.
    const MACHINE_OPTIONS: &'static str;
    /// Help lines for the machine's controls, one per line, indented four
    /// spaces. Shown under `Controls:` and printed when the window opens.
    const CONTROLS: &'static str;

    /// Parse one machine-specific flag, reading its value from `args`.
    /// Returns `Ok(false)` when `flag` is not one the machine knows.
    ///
    /// # Errors
    ///
    /// Returns a usage error for a malformed or missing value.
    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError>;

    /// Native frame length in machine ticks — the budget one `run_frames(1)`
    /// spends.
    fn frame_ticks(&self) -> u64;

    /// The query provider for this configuration.
    fn query_provider(&self) -> Self::Query;

    /// The runtime for windowed and script modes, firmware loaded.
    ///
    /// # Errors
    ///
    /// Returns a message when firmware or media cannot be read.
    fn build_runtime(&self) -> Result<Self::Runtime, LaunchError>;

    /// The runtime for MCP mode. Defaults to [`build_runtime`](Self::build_runtime);
    /// a machine that may start blank and take firmware over MCP overrides it.
    ///
    /// # Errors
    ///
    /// Returns a message when required firmware cannot be read.
    fn build_mcp_runtime(&self) -> Result<Self::Runtime, LaunchError> {
        self.build_runtime()
    }

    /// Media to load before a script runs, as `(slot, kind, bytes)`.
    ///
    /// # Errors
    ///
    /// Returns a message when an image cannot be read.
    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        Ok(Vec::new())
    }

    /// Add the machine-specific fields to the headless report. The launcher
    /// adds `time` and `observations` itself.
    fn report(&self, runtime: &Self::Runtime, report: &mut Map<String, Value>);

    /// Register the MCP tools this machine serves. The default is the base
    /// set plus the keyboard verbs, which is right for any machine with a
    /// keyboard; override to add family tools or drop the keyboard.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<Self::Runtime, Self::Query>>,
    ) {
        register_base_tools(registry);
        register_keyboard_tools(registry);
    }
}

/// The full `--help` text for a machine.
#[must_use]
pub fn usage<A: MachineApp>() -> String {
    format!(
        "\
Usage: {bin} [OPTIONS]

Options:
{machine}
    --scale N       integer window scale
    --video MODE    raw | lcd | crt [default: raw]
    --help, -h      show this help

Automation:
    --script PATH   run a JSON session headlessly and print a report
    --frames N      frames to run headlessly [default: 0]
    --screenshot PATH       write the last emitted frame as PNG
    --audio-capture PATH    write emitted audio as WAV
    --headless      run without a window (implied by --script)
    --mcp           serve this machine over MCP on stdio

Controls:
{controls}
",
        bin = A::BIN_NAME,
        machine = A::MACHINE_OPTIONS,
        controls = A::CONTROLS,
    )
}

/// A parsed command line.
#[derive(Debug)]
pub enum Parsed<A> {
    /// Run `mode` with this machine configuration.
    Run {
        /// The machine's own configuration.
        app: A,
        /// The shared flags.
        common: CommonCli,
        /// The mode the flags selected.
        mode: Mode,
    },
    /// `--help` was given; the caller prints [`usage`] and exits.
    Help,
}

/// Parse the command line: common flags here, the rest through
/// [`MachineApp::parse_flag`].
///
/// # Errors
///
/// Returns a usage error for an unknown flag or a malformed value.
pub fn parse<A: MachineApp>(args: &[String]) -> Result<Parsed<A>, LaunchError> {
    let mode = detect_mode(args);
    let mut app = A::default();
    let mut common = CommonCli::default();
    let mut cursor = Args::new(args.to_vec());
    while let Some(arg) = cursor.next_flag() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--scale" => common.scale = Some(cursor.parse("--scale", "a positive integer")?),
            "--video" => common.video = Some(cursor.value("--video")?),
            "--frames" => common.frames = cursor.parse("--frames", "a non-negative integer")?,
            "--screenshot" => common.screenshot = Some(cursor.path("--screenshot")?),
            "--audio-capture" => common.audio_capture = Some(cursor.path("--audio-capture")?),
            "--script" => common.script = Some(cursor.path("--script")?),
            "--headless" | "--mcp" | "--mcp-stdio" => {}
            flag => {
                if !app.parse_flag(flag, &mut cursor)? {
                    return Err(LaunchError::Usage(format!("unknown flag: {flag}")));
                }
            }
        }
    }
    Ok(Parsed::Run { app, common, mode })
}

/// Run script mode: build the runtime, load startup media, execute the
/// script and frame run, write captures, and return the report.
///
/// # Errors
///
/// Returns a message for unreadable firmware or media, a capture request
/// with nothing to capture, a script that fails to load or execute, or a
/// capture that cannot be written.
pub fn run_script<A: MachineApp>(app: &A, common: &CommonCli) -> Result<Value, LaunchError> {
    if (common.screenshot.is_some() || common.audio_capture.is_some())
        && common.frames == 0
        && common.script.is_none()
    {
        return Err(LaunchError::Usage(
            "capture requests require either --frames or --script so the machine emits output"
                .to_owned(),
        ));
    }

    let runtime = app.build_runtime()?;
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, app.frame_ticks(), app.query_provider());

    let loaded = app.startup_media()?;
    let media = startup_media::media_set(&loaded);
    session
        .prepare(&media, &[])
        .map_err(|err| LaunchError::Run(format!("machine preparation failed: {err}")))?;

    let mut observations: Vec<ScriptObservation> = Vec::new();
    if let Some(path) = &common.script {
        let script = HeadlessScript::from_path(path).map_err(|err| {
            LaunchError::Run(format!("failed to load script {}: {err}", path.display()))
        })?;
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| LaunchError::Run(format!("script execution failed: {err}")))?,
        );
    }

    if common.frames > 0 {
        session
            .run_frames(common.frames)
            .map_err(|err| LaunchError::Run(format!("run failed: {err}")))?;
    }
    if let Some(path) = &common.screenshot {
        session.save_screenshot(path).map_err(|err| {
            LaunchError::Run(format!("failed to write {}: {err}", path.display()))
        })?;
    }
    if let Some(path) = &common.audio_capture {
        session.save_audio_capture(path).map_err(|err| {
            LaunchError::Run(format!("failed to write {}: {err}", path.display()))
        })?;
    }

    observations.extend(session.blank_frame_observation());
    let mut report = Map::new();
    app.report(session.machine(), &mut report);
    report.insert("time".to_owned(), Value::from(session.time().get()));
    report.insert(
        "observations".to_owned(),
        serde_json::to_value(observations)
            .map_err(|err| LaunchError::Run(format!("failed to encode report: {err}")))?,
    );
    Ok(Value::Object(report))
}

/// Run MCP mode: build the runtime, load any media named on the command
/// line, register the tools, and serve stdio until the client goes away.
///
/// # Errors
///
/// Returns a message for unreadable firmware, a media flag the profile has
/// no slot for, or an I/O failure on the JSON-RPC loop.
pub fn run_mcp<A: MachineApp>(app: &A, raw_args: &[String]) -> Result<(), LaunchError> {
    let runtime = app.build_mcp_runtime()?;
    let mut session =
        HeadlessSession::new_with_query_provider(runtime, app.frame_ticks(), app.query_provider());
    // Media named on the command line is loaded here so `--rom` means the
    // same thing in MCP mode as in the other two (#1180).
    startup_media::load_into(&mut session, raw_args)?;
    let mut server = Server::new(ServerInfo::new(A::BIN_NAME, A::VERSION));
    app.register_mcp_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| LaunchError::Run(err.to_string()))
}

/// Where the headless dispatcher stopped.
#[derive(Debug)]
pub enum Outcome<A> {
    /// The mode ran to completion (or `--help` printed).
    Done,
    /// The flags ask for a window; the UI crate takes it from here.
    WantsUi {
        /// The machine's own configuration.
        app: A,
        /// `--scale`, if given.
        scale: Option<u32>,
        /// `--video`, if given, still unparsed.
        video: Option<String>,
    },
}

/// Parse the command line and run `--help`, `--script` or `--mcp`.
///
/// # Errors
///
/// Returns the usage or run failure of whichever mode ran.
pub fn run_headless<A: MachineApp>(args: Vec<String>) -> Result<Outcome<A>, LaunchError> {
    let (app, common, mode) = match parse::<A>(&args)? {
        Parsed::Help => {
            print!("{}", usage::<A>());
            return Ok(Outcome::Done);
        }
        Parsed::Run { app, common, mode } => (app, common, mode),
    };
    match mode {
        Mode::Ui => Ok(Outcome::WantsUi {
            app,
            scale: common.scale,
            video: common.video,
        }),
        Mode::Script => {
            let report = run_script(&app, &common)?;
            println!("{}", serde_json::to_string(&report).unwrap_or_default());
            Ok(Outcome::Done)
        }
        Mode::Mcp => {
            run_mcp(&app, &args)?;
            Ok(Outcome::Done)
        }
    }
}

/// Print a launch failure and exit with its code: 2 for a usage error
/// (with a pointer at `--help`), 1 for anything else.
pub fn exit_with<A: MachineApp>(err: &LaunchError) -> ! {
    eprintln!("error: {err}");
    match err {
        LaunchError::Usage(_) => {
            eprintln!("run `{} --help` for usage", A::BIN_NAME);
            process::exit(2)
        }
        LaunchError::Run(_) => process::exit(1),
    }
}

/// `main` for a binary built without its `ui` feature: the headless modes
/// work, and asking for a window explains what to rebuild with.
pub fn main_headless<A: MachineApp>() -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    match run_headless::<A>(args) {
        Ok(Outcome::Done) => process::exit(0),
        Ok(Outcome::WantsUi { .. }) => exit_with::<A>(&LaunchError::Run(
            "this binary was built without the `ui` feature; rebuild with `--features ui` for interactive mode, or use --script / --mcp instead"
                .to_owned(),
        )),
        Err(err) => exit_with::<A>(&err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn mode_defaults_to_ui() {
        assert_eq!(detect_mode(&[]), Mode::Ui);
    }

    #[test]
    fn mode_treats_bare_media_as_ui() {
        assert_eq!(detect_mode(&args(&["--cart", "game.rom"])), Mode::Ui);
    }

    #[test]
    fn mode_recognises_script_via_automation_flags() {
        for flag in [
            "--script",
            "--frames",
            "--screenshot",
            "--audio-capture",
            "--headless",
        ] {
            assert_eq!(
                detect_mode(&args(&["--cart", "game.rom", flag])),
                Mode::Script,
                "flag {flag} should be script"
            );
        }
    }

    #[test]
    fn mode_recognises_mcp_over_script() {
        assert_eq!(detect_mode(&args(&["--mcp"])), Mode::Mcp);
        assert_eq!(
            detect_mode(&args(&["--script", "s.json", "--mcp-stdio"])),
            Mode::Mcp
        );
    }

    #[test]
    fn args_report_a_missing_value_as_usage() {
        let mut cursor = Args::new(args(&["--rom"]));
        assert_eq!(cursor.next_flag().as_deref(), Some("--rom"));
        assert_eq!(
            cursor.value("--rom"),
            Err(LaunchError::Usage("--rom requires a value".to_owned()))
        );
    }

    #[test]
    fn args_parse_names_the_expected_form() {
        let mut cursor = Args::new(args(&["x"]));
        let parsed: Result<u32, _> = cursor.parse("--scale", "a positive integer");
        assert_eq!(
            parsed,
            Err(LaunchError::Usage(
                "--scale expects a positive integer, got x".to_owned()
            ))
        );
    }

    #[test]
    fn conventional_path_falls_back_to_the_rom_directory() {
        // Mutating the environment is unsafe in edition 2024 and this
        // workspace forbids unsafe code, so the test relies on a name no
        // environment sets.
        let key = "EMU198X_LAUNCH_TEST_ROM_THAT_NOBODY_SETS";
        assert!(env::var(key).is_err());
        let path = conventional_rom_path(key, "family/rom.bin").expect("HOME is set");
        assert!(
            path.ends_with(".emu198x/roms/family/rom.bin"),
            "{}",
            path.display()
        );
    }
}
