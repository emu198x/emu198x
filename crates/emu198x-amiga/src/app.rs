//! The Amiga as a [`MachineApp`]: its flags, runtime construction, and MCP
//! tool set.
//!
//! Script mode is the bespoke runner in `script.rs`, reached through
//! [`MachineApp::run_script`]: its report carries boot detection and
//! printed queries, and it intercepts `set_machine` steps to swap the
//! chipset variant mid-script. MCP mode is the launcher's default server
//! with the Amiga debugging tools registered on top. Model selection and
//! Kickstart resolution live in `model.rs`, which the MCP tools share.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_tools_for;
use emu198x_shell::{
    FirmwareOverrides, HeadlessSession, MachineCore, MediaImage, MediaKind, MediaSet,
    build_variant, read_media_asset, resolve_firmware,
};
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AmigaSessionQueryProvider, Model,
};
use serde_json::{Map, Value};

use crate::mcp::tools::register_amiga_tools;

pub(crate) const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";

/// The machine configuration the flags build up. The default is the
/// canonical Amiga — A500 OCS PAL with Kickstart 1.3 — that vAmiga /
/// FS-UAE / WinUAE also default to.
#[derive(Debug, PartialEq, Eq)]
pub struct Amiga {
    pub model: Model,
    pub rom_dir: Option<PathBuf>,
    pub kickstart: Option<PathBuf>,
    pub disk: Option<PathBuf>,
    pub wait_for_boot: Option<u32>,
    pub print_queries: Vec<String>,
}

impl Default for Amiga {
    fn default() -> Self {
        Self {
            model: Model::A500OcsPal,
            rom_dir: None,
            kickstart: None,
            disk: None,
            wait_for_boot: None,
            print_queries: Vec::new(),
        }
    }
}

impl Amiga {
    /// What the firmware flags add to the convention: `--rom-dir` as the
    /// directory, `--kickstart` pinning the model's one ROM (Kickstart, or
    /// the A1000's bootstrap).
    pub(crate) fn firmware_overrides(&self) -> FirmwareOverrides {
        let mut overrides = FirmwareOverrides {
            dir: self.rom_dir.clone(),
            ..FirmwareOverrides::none()
        };
        if let Some(path) = &self.kickstart {
            for source in self.model.firmware_sources() {
                overrides.pin(source.id, path.clone());
            }
        }
        overrides
    }
}

impl MachineApp for Amiga {
    type Runtime = AmigaRuntimeKind;
    type Query = AmigaSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-amiga";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// Headless-only flags; their presence routes to script mode, so
    /// `--wait-for-boot 300 --print-query boot.reason` runs without an
    /// explicit `--headless`. `--disk` and the firmware flags are shared
    /// with the window and do not.
    const SCRIPT_FLAGS: &'static [&'static str] = &["--wait-for-boot", "--print-query"];
    const MACHINE_OPTIONS: &'static str =
        "    --rom-dir DIR        directory containing Amiga ROM images; default
                         EMU198X_AMIGA_ROM_DIR, ~/.emu198x/roms/commodore-amiga,
                         or ~/.emu198x/roms/amiga, searched for the model's
                         Kickstart (kick13.rom, kick204.rom, kick31a1200.rom, ...)
    --kickstart PATH     explicit ROM path (Kickstart on A500, bootstrap on A1000)
    --model MODEL        a1000 | a500 | a500-gvp-a530 | a500-a501 | a500-plus
                         | a500-maxed | a600 | a1200 | a2000 [default: a500]
    --disk PATH          insert one ADF image into DF0:
    --wait-for-boot N    run up to N frames until boot.detected is true (headless)
    --print-query PATH   resolve one query path after running (repeatable, headless)";
    const CONTROLS: &'static str = "    Esc                  quit
    F12                  hard reset (keeps the inserted disk)
    Cmd/Ctrl+S / +L      quick save / load state
    Mouse                port-1 Amiga mouse (JOY0DAT)
    Gamepad              port-2 Amiga joystick (JOY1DAT)
    Page Up              toggle arrow/space joystick mode for port 2
    A-Z, 0-9             Amiga keyboard
    Space, Enter, Tab    Amiga keyboard
    Backspace            Amiga keyboard
    Machine menu         switch model live (A1000 / A500 family / A600 / A1200 / A2000)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom-dir" => self.rom_dir = Some(args.path(flag)?),
            "--kickstart" => self.kickstart = Some(args.path(flag)?),
            "--model" => {
                let value = args.value(flag)?;
                self.model = Model::from_variant_id(&value).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "--model expects one of {}, got {value}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--disk" => self.disk = Some(args.path(flag)?),
            "--wait-for-boot" => {
                self.wait_for_boot = Some(args.parse(flag, "a non-negative integer")?);
            }
            "--print-query" => self.print_queries.push(args.value(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The PAL constant for every model; the script and MCP sessions were
    /// always paced this way.
    fn frame_ticks(&self) -> u64 {
        A500_PAL_FRAME_TICKS
    }

    fn query_provider(&self) -> AmigaSessionQueryProvider {
        AmigaSessionQueryProvider
    }

    /// The runtime for the window: resolve the model's Kickstart, build the
    /// chipset variant, and insert the DF0 ADF if `--disk` was given. Script
    /// mode boots for itself in `script.rs` so its media load is part of the
    /// session it reports on.
    fn build_runtime(&self) -> Result<AmigaRuntimeKind, LaunchError> {
        let mut runtime = build_variant::<AmigaRuntimeKind>(self.model, &self.firmware_overrides())
            .map_err(|err| err.to_string())?;

        if let Some(path) = &self.disk {
            let disk = read_media_asset(path, MediaKind::Disk)
                .map_err(|err| format!("failed to read disk {}: {err}", path.display()))?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                DEFAULT_FLOPPY_SLOT,
                MediaKind::Disk,
                &disk.bytes,
            ));
            runtime.load_media(&media).map_err(|err| err.to_string())?;
        }

        Ok(runtime)
    }

    /// The MCP session boots the chosen model from its raw ROM file; media
    /// named on the command line is loaded by the launcher, and the
    /// `insert_media` / `set_machine` tools change it at runtime.
    fn build_mcp_runtime(&self) -> Result<AmigaRuntimeKind, LaunchError> {
        let resolved = resolve_firmware::<AmigaRuntimeKind>(self.model, &self.firmware_overrides())
            .map_err(|err| LaunchError::Run(format!("Kickstart ROM not found: {err}")))?;
        let (_, rom_path) = resolved
            .into_iter()
            .next()
            .ok_or_else(|| LaunchError::Run("the model boots no ROM".to_owned()))?;
        let rom_bytes = std::fs::read(&rom_path).map_err(|err| err.to_string())?;
        AmigaRuntimeKind::new(self.model, rom_bytes).map_err(|err| err.to_string().into())
    }

    /// The Amiga's report is built by `script::run`, which
    /// [`run_script`](Self::run_script) calls; the shared loop and this hook
    /// never run for this machine.
    fn report(&self, _runtime: &AmigaRuntimeKind, _report: &mut Map<String, Value>) {}

    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        crate::script::run(self, common)
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>>,
        session: &HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>,
    ) {
        // The shared surface as the machine declares it (debug verbs via
        // DebugTarget, memory watch via WatchTarget, keyboard; no AY tier,
        // the Amiga has Paula), then the bespoke chip/exec/copper tools. The
        // richer Amiga overrides win on name collisions (last write).
        register_tools_for(registry, session);
        register_amiga_tools(registry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (Amiga, CommonCli, Mode) {
        match parse::<Amiga>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn flags_cover_kickstart_disk_and_capture() {
        let (app, common, mode) = parsed(&[
            "--model",
            "a500-a501",
            "--kickstart",
            "kick13.rom",
            "--disk",
            "workbench.adf",
            "--frames",
            "12",
            "--screenshot",
            "frame.png",
            "--audio-capture",
            "audio.wav",
        ]);

        assert_eq!(
            app,
            Amiga {
                model: Model::A500OcsPalA501,
                kickstart: Some(PathBuf::from("kick13.rom")),
                disk: Some(PathBuf::from("workbench.adf")),
                ..Amiga::default()
            }
        );
        assert_eq!(common.frames, 12);
        assert_eq!(common.screenshot, Some(PathBuf::from("frame.png")));
        assert_eq!(common.audio_capture, Some(PathBuf::from("audio.wav")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn flags_cover_the_window_and_default_to_the_a500() {
        let (app, common, mode) = parsed(&["--disk", "workbench13.adf", "--scale", "2"]);
        assert_eq!(app.model, Model::A500OcsPal);
        assert!(app.rom_dir.is_none());
        assert!(app.kickstart.is_none());
        assert_eq!(app.disk, Some(PathBuf::from("workbench13.adf")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_cover_the_headless_boot_wait_and_queries() {
        let (app, _, _) = parsed(&[
            "--wait-for-boot",
            "300",
            "--print-query",
            "boot.detected",
            "--print-query",
            "disk.inserted",
            "--rom-dir",
            "roms",
        ]);
        assert_eq!(app.wait_for_boot, Some(300));
        assert_eq!(app.print_queries, ["boot.detected", "disk.inserted"]);
        assert_eq!(app.rom_dir, Some(PathBuf::from("roms")));
    }

    #[test]
    fn model_flag_covers_the_full_family() {
        let (app, _, _) = parsed(&["--model", "a1200"]);
        assert_eq!(app.model, Model::A1200AgaPal);
        let (app, _, _) = parsed(&["--model", "a500-maxed"]);
        assert_eq!(app.model, Model::A500OcsPalMaxed);
        let (app, _, _) = parsed(&["--model", "a1000"]);
        assert_eq!(app.model, Model::A1000OcsPal);
    }

    #[test]
    fn headless_only_flags_route_to_script_mode() {
        for flags in [
            &["--wait-for-boot", "300"][..],
            &["--disk", "wb.adf", "--print-query", "disk.inserted"],
        ] {
            let (.., mode) = parsed(flags);
            assert_eq!(mode, Mode::Script, "{flags:?} should be script");
        }
        // --disk and the firmware flags are shared with the window.
        let (.., mode) = parsed(&["--model", "a500-a501", "--disk", "wb.adf"]);
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Amiga>(&args(&["--model", "a4000"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(ref message) if message.contains("a4000")));
    }
}
