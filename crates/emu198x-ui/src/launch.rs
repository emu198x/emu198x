//! Windowed half of the shared machine launcher.
//!
//! [`emu198x_shell::launch`] parses the command line and runs `--script`
//! and `--mcp`; it cannot open a window because this crate depends on it,
//! not the other way round. [`main`] here runs the shell dispatcher and
//! picks up [`Outcome::WantsUi`] to build the runtime and open the window.

use std::env;
use std::process;

use emu198x_shell::MachineCore;
use emu198x_shell::launch::{self, LaunchError, MachineApp, Outcome};

use crate::{UiSystem, VideoFilter};

/// What a machine binary adds to [`MachineApp`] to open a window.
pub trait UiApp: MachineApp {
    /// The [`UiSystem`] that drives this machine's window.
    type System: UiSystem<Runtime = Self::Runtime>;

    /// The window driver for this configuration.
    fn ui_system(&self) -> Self::System;

    /// The runtime for the window, including parsed startup media. Defaults to the headless
    /// [`MachineApp::build_runtime`]; a machine whose window boots a
    /// different default from its script mode (the BBC Micro installs
    /// BASIC for the window and boots the bare MOS headlessly) overrides
    /// it.
    ///
    /// # Errors
    ///
    /// Returns a message when firmware or media cannot be read.
    fn build_ui_runtime(&self) -> Result<Self::Runtime, LaunchError> {
        let mut runtime = self.build_runtime()?;
        let loaded = self.startup_media()?;
        if !loaded.is_empty() {
            runtime
                .load_media(&emu198x_shell::startup_media::media_set(&loaded))
                .map_err(|err| LaunchError::Run(format!("failed to load startup media: {err}")))?;
        }
        Ok(runtime)
    }
}

/// Build the runtime and open the window. `scale` and `video` are the
/// `--scale` / `--video` flags as parsed by the shell, still optional.
///
/// # Errors
///
/// Returns a usage error for an unknown `--video` mode, and a run error
/// when the runtime cannot be built or the window fails.
pub fn run_windowed<A: UiApp>(
    app: A,
    scale: Option<u32>,
    video: Option<String>,
) -> Result<(), LaunchError> {
    let system = app.ui_system();
    let scale = scale.unwrap_or_else(|| system.default_scale());
    let video = match video {
        Some(mode) => mode.parse::<VideoFilter>().map_err(|_| {
            LaunchError::Usage(format!("--video expects raw, lcd, or crt, got {mode}"))
        })?,
        None => system.default_video(),
    };
    let runtime = app.build_ui_runtime()?;
    println!("Controls:\n{}", A::CONTROLS);
    crate::run(system, runtime, scale, video).map_err(|err| LaunchError::Run(err.to_string()))
}

/// `main` for a machine binary built with its `ui` feature: every mode,
/// with the shell's exit codes.
pub fn main<A: UiApp>() -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match launch::run_headless::<A>(args) {
        Ok(Outcome::Done) => Ok(()),
        Ok(Outcome::WantsUi { app, scale, video }) => run_windowed(app, scale, video),
        Err(err) => Err(err),
    };
    match result {
        Ok(()) => process::exit(0),
        Err(err) => launch::exit_with::<A>(&err),
    }
}
