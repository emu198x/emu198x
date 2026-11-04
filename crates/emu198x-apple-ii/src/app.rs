//! Firmware flags and board construction; the shared launcher owns control.
use emu198x_runtime_apple_ii::{AppleIIQueries, AppleIIRuntime, profiles};
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareImage, FirmwareSet, MediaKind, MediaSlot};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Default)]
pub struct AppleII {
    firmware: [Option<PathBuf>; 4],
    disks: [Option<PathBuf>; 2],
}
impl MachineApp for AppleII {
    type Runtime = AppleIIRuntime;
    type Query = AppleIIQueries;
    const BIN_NAME: &'static str = "emu198x-apple-ii";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH         original E000-FFFF Monitor/Integer BASIC ROM (8192 bytes)
    --characters PATH  normalized 64-glyph character bitmap (512 bytes)
    --p5 PATH          optional slot-6 Disk II bootstrap PROM (256 bytes)
    --p6 PATH          optional Disk II sequencer PROM (256 bytes; requires P5)
    --disk PATH        drive 1 WOZ image (read-only)
    --disk-2 PATH      drive 2 WOZ image (read-only)
    Explicit firmware is required in window, script and MCP modes.
    Writable work copies use the shared load_media operation.";
    const CONTROLS: &'static str =
        "    Host-layout typing encodes the original keyboard; letters become uppercase.
    Left Alt/Option holds Apple Ctrl (Left Alt+B, Return enters Integer BASIC).
    Machine menu: Pause/Resume and Reset; shared View menu controls presentation.
    Shared type_string and press_key tools encode the original keyboard.
    ASCII letters become uppercase; Ctrl+B enters Integer BASIC from Monitor.
    RESET preserves RAM/media; save states are not available.";
    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        let target = match flag {
            "--rom" => &mut self.firmware[0],
            "--characters" => &mut self.firmware[1],
            "--p5" => &mut self.firmware[2],
            "--p6" => &mut self.firmware[3],
            "--disk" => &mut self.disks[0],
            "--disk-2" => &mut self.disks[1],
            _ => return Ok(false),
        };
        *target = Some(args.path(flag)?);
        Ok(true)
    }
    fn frame_ticks(&self) -> u64 {
        profiles::FRAME_TICKS
    }
    fn query_provider(&self) -> AppleIIQueries {
        AppleIIQueries
    }
    fn build_runtime(&self) -> Result<AppleIIRuntime, LaunchError> {
        let mut bytes = Vec::new();
        for (i, path) in self.firmware.iter().enumerate() {
            if let Some(path) = path {
                bytes.push((
                    profiles::FIRMWARE[i],
                    read_rom(path, profiles::FIRMWARE[i])?,
                ));
            }
        }
        let mut set = FirmwareSet::new();
        for (id, data) in &bytes {
            set.push(FirmwareImage::new(*id, data));
        }
        AppleIIRuntime::from_firmware(&set).map_err(|e| LaunchError::Run(e.to_string()))
    }
    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let mut images = Vec::new();
        for (i, path) in self.disks.iter().enumerate() {
            if let Some(path) = path {
                images.push((
                    format!("floppy-{i}"),
                    MediaKind::Disk,
                    read_rom(path, "WOZ disk")?,
                ));
            }
        }
        Ok(images)
    }
    fn mcp_startup_media(
        &self,
        _slots: &[MediaSlot],
        _args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }
    fn report(&self, runtime: &AppleIIRuntime, report: &mut Map<String, Value>) {
        report.insert("rom_loaded".into(), true.into());
        report.insert("frames_run".into(), runtime.machine().frame_count().into());
        report.insert(
            "master_clock".into(),
            runtime.machine().master_clock().into(),
        );
    }
}
