//! Headless / script execution mode — the body behind
//! [`MachineApp::run_script`](emu198x_shell::launch::MachineApp::run_script).
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
//! The command line itself is parsed by the shared launcher; `app.rs`
//! hands the parsed flags to [`runner::run_script`] as
//! [`runner::ScriptInputs`] and prints the result.

pub mod runner;
