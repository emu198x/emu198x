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
use runtime_commodore_amiga::{AmigaRuntimeKind, AmigaSessionQueryProvider, Model};
use serde_json::{Map, Value};

use crate::mcp::tools::register_amiga_tools;

pub(crate) const DEFAULT_FLOPPY_SLOT: &str = "floppy-0";

/// Media slots of the external drives, DF1 to DF3.
const EXTERNAL_FLOPPY_SLOTS: [&str; 3] = ["floppy-1", "floppy-2", "floppy-3"];

/// The most drives an Amiga takes: DF0 and three external drives.
const MAX_FLOPPY_DRIVES: usize = 4;

/// The machine configuration the flags build up. The default is the
/// canonical Amiga — A500 OCS PAL with Kickstart 1.3 — that vAmiga /
/// FS-UAE / WinUAE also default to.
#[derive(Debug, PartialEq, Eq)]
pub struct Amiga {
    pub model: Model,
    pub rom_dir: Option<PathBuf>,
    pub kickstart: Option<PathBuf>,
    pub disk: Option<PathBuf>,
    /// Disks for DF1 to DF3 (`--disk1` to `--disk3`); each fits its drive.
    pub external_disks: [Option<PathBuf>; 3],
    /// Drives to fit, DF0 included, whether or not they hold a disk.
    pub drives: Option<usize>,
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
            external_disks: [None, None, None],
            drives: None,
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

    /// Fit the drives `--drives` asks for, then load every disk named on
    /// the command line into its drive.
    pub(crate) fn load_disks(&self, runtime: &mut AmigaRuntimeKind) -> Result<(), LaunchError> {
        if let Some(count) = self.drives {
            runtime
                .fit_floppy_drives(count)
                .map_err(|err| err.to_string())?;
        }
        let disks = self.read_disks()?;
        let mut media = MediaSet::new();
        for (slot, bytes) in &disks {
            media.push(MediaImage::new(*slot, MediaKind::Disk, bytes));
        }
        runtime.load_media(&media).map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Every disk named on the command line, with the slot it goes in.
    pub(crate) fn read_disks(&self) -> Result<Vec<(&'static str, Vec<u8>)>, LaunchError> {
        std::iter::once((DEFAULT_FLOPPY_SLOT, &self.disk))
            .chain(EXTERNAL_FLOPPY_SLOTS.into_iter().zip(&self.external_disks))
            .filter_map(|(slot, path)| path.as_ref().map(|path| (slot, path)))
            .map(|(slot, path)| {
                read_media_asset(path, MediaKind::Disk)
                    .map(|disk| (slot, disk.bytes))
                    .map_err(|err| format!("failed to read disk {}: {err}", path.display()).into())
            })
            .collect()
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
                         Unsuffixed ids select PAL; append -ntsc for NTSC.
    --disk PATH          insert one ADF image into DF0:
    --disk1 PATH         insert an ADF into DF1:, fitting that drive
    --disk2 PATH         insert an ADF into DF2:, fitting that drive
    --disk3 PATH         insert an ADF into DF3:, fitting that drive
    --drives N           fit N drives, DF0 included, even if empty (1-4)
    --wait-for-boot N    run up to N frames until boot.detected is true (headless)
    --print-query PATH   resolve one query path after running (repeatable, headless)";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    Cmd/Ctrl+Shift+R     hard reset (keeps the inserted disk)
    Cmd/Ctrl+P           pause / resume
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
            "--disk1" => self.external_disks[0] = Some(args.path(flag)?),
            "--disk2" => self.external_disks[1] = Some(args.path(flag)?),
            "--disk3" => self.external_disks[2] = Some(args.path(flag)?),
            "--drives" => {
                let count: usize = args.parse(flag, "a drive count from 1 to 4")?;
                if !(1..=MAX_FLOPPY_DRIVES).contains(&count) {
                    return Err(LaunchError::Usage(format!(
                        "--drives expects 1 to {MAX_FLOPPY_DRIVES}, got {count}"
                    )));
                }
                self.drives = Some(count);
            }
            "--wait-for-boot" => {
                self.wait_for_boot = Some(args.parse(flag, "a non-negative integer")?);
            }
            "--print-query" => self.print_queries.push(args.value(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Initial frame budget follows the selected preset's region.
    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> AmigaSessionQueryProvider {
        AmigaSessionQueryProvider
    }

    /// The runtime for the window: resolve the model's Kickstart, build the
    /// chipset variant, fit the drives `--drives` asks for and insert the
    /// ADFs `--disk` to `--disk3` name. Script mode boots for itself in
    /// `script.rs` so its media load is part of the session it reports on.
    fn build_runtime(&self) -> Result<AmigaRuntimeKind, LaunchError> {
        let mut runtime = build_variant::<AmigaRuntimeKind>(self.model, &self.firmware_overrides())
            .map_err(|err| err.to_string())?;
        self.load_disks(&mut runtime)?;
        Ok(runtime)
    }

    /// The MCP session boots the chosen model from its raw ROM file, fits
    /// the drives `--drives` asks for and loads the disks `--disk` to
    /// `--disk3` name; the `insert_media` / `set_machine` tools change them
    /// at runtime.
    fn build_mcp_runtime(&self) -> Result<AmigaRuntimeKind, LaunchError> {
        let resolved = resolve_firmware::<AmigaRuntimeKind>(self.model, &self.firmware_overrides())
            .map_err(|err| LaunchError::Run(format!("Kickstart ROM not found: {err}")))?;
        let (_, rom_path) = resolved
            .into_iter()
            .next()
            .ok_or_else(|| LaunchError::Run("the model boots no ROM".to_owned()))?;
        let rom_bytes = std::fs::read(&rom_path).map_err(|err| err.to_string())?;
        let mut runtime =
            AmigaRuntimeKind::new(self.model, rom_bytes).map_err(|err| err.to_string())?;
        self.load_disks(&mut runtime)?;
        Ok(runtime)
    }

    /// [`build_mcp_runtime`](Self::build_mcp_runtime) has already loaded
    /// every disk flag, `--disk1` to `--disk3` included, which the
    /// launcher's generic `--disk` discovery does not know.
    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        Ok(Vec::new())
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
    fn every_preset_launches_with_its_regional_frame_budget() {
        use emu198x_shell::FamilyRuntime;
        for model in Model::VARIANTS {
            let (app, _, _) = parsed(&["--model", model.variant_id()]);
            assert_eq!(app.model, model);
            assert_eq!(
                app.frame_ticks(),
                AmigaRuntimeKind::blank(model).native_frame_ticks()
            );
        }
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
    fn flags_fit_external_drives() {
        let (app, _, mode) = parsed(&[
            "--disk", "one.adf", "--disk1", "two.adf", "--disk3", "four.adf", "--drives", "3",
        ]);
        assert_eq!(app.disk, Some(PathBuf::from("one.adf")));
        assert_eq!(
            app.external_disks,
            [
                Some(PathBuf::from("two.adf")),
                None,
                Some(PathBuf::from("four.adf"))
            ]
        );
        assert_eq!(app.drives, Some(3));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_drive_count_outside_one_to_four_is_a_usage_error() {
        for count in ["0", "5"] {
            let err = parse::<Amiga>(&args(&["--drives", count])).expect_err("rejects");
            assert!(matches!(err, LaunchError::Usage(ref message) if message.contains("1 to 4")));
        }
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Amiga>(&args(&["--model", "a4000"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(ref message) if message.contains("a4000")));
    }
}
