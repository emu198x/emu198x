//! Runtime wrapper for the fresh-workspace Commodore Amiga baseline.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, InputEvent,
    MachineCore, MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, QueryError,
    QueryResult, ResetKind, RunResult, SessionQueryProvider, StopReason,
};
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga::{AUDIO_SAMPLE_RATE, Amiga};
use serde_json::json;

use crate::{A500_PAL_FRAME_TICKS, Model, profile_for};

const AMIGA_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "boot.row",
    "amiga.cpu.pc",
    "amiga.display.non_black_pixels",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.inserted",
    "amiga.disk.motor_on",
    "amiga.disk.motor_spinning",
    "amiga.keyboard.queued",
    "amiga.keyboard.state",
    "amiga.machine.cck_count",
    "amiga.machine.frame_count",
];

const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
const VALID_KICKSTART_SIZES: &[usize] = &[256 * 1024, 512 * 1024];

/// Display-area crop from the Denise raster framebuffer.
pub const DISPLAY_WIDTH: u32 = 724;
/// Display-area crop from the Denise raster framebuffer.
pub const DISPLAY_HEIGHT: u32 = 568;
const DISPLAY_X_START: u32 = 456;
const DISPLAY_Y_START: u32 = 26;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AmigaBootStatus {
    detected: bool,
    reason: &'static str,
    row: Option<u32>,
}

/// Amiga-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmigaSessionQueryProvider;

/// Firmware-backed Amiga runtime over the imported A500 OCS machine crate.
pub struct AmigaRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Amiga,
    time: MachineTime,
    kickstart_rom: Vec<u8>,
    floppy0_bytes: Option<Vec<u8>>,
    rgba_framebuffer: Vec<u8>,
    frame_count: u64,
    first_active_row: Option<u32>,
    non_black_pixels: u32,
}

impl AmigaRuntime {
    /// Creates an Amiga runtime from owned Kickstart ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not one of the supported A500-era
    /// sizes.
    pub fn new(model: Model, kickstart_rom: Vec<u8>) -> Result<Self, MachineError> {
        validate_kickstart_rom(&kickstart_rom)?;
        let machine = build_machine(model, &kickstart_rom);
        let mut runtime = Self {
            profile: profile_for(model),
            model,
            machine,
            time: MachineTime::default(),
            kickstart_rom,
            floppy0_bytes: None,
            rgba_framebuffer: vec![0; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 4) as usize],
            frame_count: 0,
            first_active_row: None,
            non_black_pixels: 0,
        };
        runtime.update_rgba_framebuffer();
        Ok(runtime)
    }

    /// Creates a runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware is missing or invalid.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;

        let kickstart =
            firmware
                .bytes(KICKSTART_ROM_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: KICKSTART_ROM_ID.to_owned(),
                })?;

        Self::new(model, kickstart.to_vec())
    }

    /// Creates a runtime backed by a zero-filled placeholder Kickstart image.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, vec![0; 256 * 1024]).expect("blank Kickstart image should be valid")
    }

    /// Returns the wrapped Amiga machine.
    #[must_use]
    pub fn machine(&self) -> &Amiga {
        &self.machine
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        validate_kickstart_rom(&self.kickstart_rom)?;
        self.machine = build_machine(self.model, &self.kickstart_rom);
        if let Some(bytes) = self.floppy0_bytes.clone() {
            self.insert_floppy_bytes("floppy-0", &bytes)?;
        }
        self.time = MachineTime::default();
        self.frame_count = 0;
        self.update_rgba_framebuffer();
        Ok(())
    }

    fn insert_floppy_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let adf = Adf::from_bytes(bytes.to_vec()).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason: reason.to_string(),
        })?;
        self.machine.insert_disk(adf);
        self.floppy0_bytes = Some(bytes.to_vec());
        Ok(())
    }

    fn update_rgba_framebuffer(&mut self) {
        let framebuffer = self.machine.framebuffer();
        let (source_width, source_height) = self.machine.framebuffer_size();
        let mut non_black_pixels = 0u32;
        let mut first_active_row = None;

        for dy in 0..DISPLAY_HEIGHT {
            let src_y = DISPLAY_Y_START + dy;
            if src_y >= source_height {
                break;
            }
            for dx in 0..DISPLAY_WIDTH {
                let src_x = DISPLAY_X_START + dx;
                if src_x >= source_width {
                    break;
                }

                let src_index = (src_y * source_width + src_x) as usize;
                let pixel = framebuffer.get(src_index).copied().unwrap_or(0xFF00_0000);
                let base = ((dy * DISPLAY_WIDTH + dx) * 4) as usize;

                self.rgba_framebuffer[base] = ((pixel >> 16) & 0xFF) as u8;
                self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xFF) as u8;
                self.rgba_framebuffer[base + 2] = (pixel & 0xFF) as u8;
                self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xFF) as u8;

                if pixel & 0x00FF_FFFF != 0 {
                    non_black_pixels = non_black_pixels.saturating_add(1);
                    if first_active_row.is_none() {
                        first_active_row = Some(dy);
                    }
                }
            }
        }

        self.non_black_pixels = non_black_pixels;
        self.first_active_row = first_active_row;
    }

    fn boot_status(&self) -> AmigaBootStatus {
        if let Some(row) = self.first_active_row {
            AmigaBootStatus {
                detected: true,
                reason: "display-active",
                row: Some(row),
            }
        } else {
            AmigaBootStatus {
                detected: false,
                reason: "no-visible-output",
                row: None,
            }
        }
    }
}

impl SessionQueryProvider<AmigaRuntime> for AmigaSessionQueryProvider {
    fn query_paths(&self, _machine: &AmigaRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = AMIGA_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &AmigaRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let boot = machine.boot_status();
        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.reason" => json!(boot.reason),
            "boot.row" => json!(boot.row),
            "amiga.cpu.pc" => json!(machine.machine.cpu.regs.pc),
            "amiga.display.non_black_pixels" => json!(machine.non_black_pixels),
            "amiga.disk.inserted" => json!(machine.machine.has_disk()),
            "amiga.disk.cylinder" => json!(machine.machine.floppy.cylinder()),
            "amiga.disk.head" => json!(machine.machine.floppy.head()),
            "amiga.disk.motor_on" => json!(machine.machine.floppy.motor_on()),
            "amiga.disk.motor_spinning" => json!(machine.machine.floppy.motor_spinning()),
            "amiga.keyboard.queued" => json!(machine.machine.keyboard.queued_key_count()),
            "amiga.keyboard.state" => json!(machine.machine.keyboard.debug_state_name()),
            "amiga.machine.frame_count" => json!(machine.frame_count),
            "amiga.machine.cck_count" => json!(machine.machine.cck_count),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

impl MachineCore for AmigaRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine()
            .expect("stored Kickstart image should remain valid across resets");
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "floppy-0" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }
            if image.kind != MediaKind::Disk {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }
            self.insert_floppy_bytes(image.slot.as_ref(), image.bytes)?;
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            apply_input_event(&mut self.machine, event);
        }

        while self.time < target {
            self.machine.run_frame();
            self.frame_count = self.frame_count.saturating_add(1);
            self.time = self.time.saturating_add(A500_PAL_FRAME_TICKS);
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: emu198x_shell::PixelFormat::Rgba8888,
                width: DISPLAY_WIDTH,
                height: DISPLAY_HEIGHT,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;

            let audio = self.machine.take_audio_buffer();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 2,
                samples: &audio,
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot-export",
        })
    }

    fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot-import",
        })
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

fn build_machine(model: Model, kickstart_rom: &[u8]) -> Amiga {
    match model {
        Model::A500OcsPal => Amiga::new(kickstart_rom.to_vec()),
    }
}

fn validate_kickstart_rom(kickstart_rom: &[u8]) -> Result<(), MachineError> {
    if VALID_KICKSTART_SIZES.contains(&kickstart_rom.len()) {
        return Ok(());
    }

    Err(MachineError::InvalidFirmware {
        id: KICKSTART_ROM_ID.to_owned(),
        reason: format!(
            "expected one of {:?} bytes, got {}",
            VALID_KICKSTART_SIZES,
            kickstart_rom.len()
        ),
    })
}

fn apply_input_event(machine: &mut Amiga, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some(code) = key_name_to_raw_code(name.as_ref())
    {
        machine.key_event(code, *pressed);
    }
}

fn key_name_to_raw_code(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    if let Some(raw) = lower.strip_prefix("raw-") {
        return u8::from_str_radix(raw.trim_start_matches("0x"), 16).ok();
    }

    Some(match lower.as_str() {
        "1" => 0x01,
        "2" => 0x02,
        "3" => 0x03,
        "4" => 0x04,
        "5" => 0x05,
        "6" => 0x06,
        "7" => 0x07,
        "8" => 0x08,
        "9" => 0x09,
        "0" => 0x0A,
        "q" => 0x10,
        "w" => 0x11,
        "e" => 0x12,
        "r" => 0x13,
        "t" => 0x14,
        "y" => 0x15,
        "u" => 0x16,
        "i" => 0x17,
        "o" => 0x18,
        "p" => 0x19,
        "a" => 0x20,
        "s" => 0x21,
        "d" => 0x22,
        "f" => 0x23,
        "g" => 0x24,
        "h" => 0x25,
        "j" => 0x26,
        "k" => 0x27,
        "l" => 0x28,
        "z" => 0x31,
        "x" => 0x32,
        "c" => 0x33,
        "v" => 0x34,
        "b" => 0x35,
        "n" => 0x36,
        "m" => 0x37,
        "space" => 0x40,
        "backspace" => 0x41,
        "tab" => 0x42,
        "enter" | "return" => 0x45,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{
        MediaImage, NullAudioSink, NullFrameSink, NullTraceSink, read_media_asset,
    };
    use format_commodore_amiga_adf::{ADF_SIZE_DD, ADF_SIZE_HD};
    use std::fs;
    use std::path::Path;

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

    fn dummy_firmware() -> FirmwareSet<'static> {
        let kickstart = dummy_kickstart().into_boxed_slice();
        let bytes: &'static [u8] = Box::leak(kickstart);
        let mut firmware = FirmwareSet::new();
        firmware.push(emu198x_shell::FirmwareImage::new(KICKSTART_ROM_ID, bytes));
        firmware
    }

    #[test]
    fn from_firmware_accepts_kickstart() {
        let runtime = AmigaRuntime::from_firmware(Model::A500OcsPal, &dummy_firmware());
        assert!(runtime.is_ok());
    }

    #[test]
    fn load_media_accepts_dd_adf() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
            .expect("dummy Kickstart should construct");
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));

        runtime
            .load_media(&media)
            .expect("ADF bytes should insert into DF0");

        assert!(runtime.machine().has_disk());
    }

    #[test]
    fn runtime_runs_one_frame() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
            .expect("dummy Kickstart should construct");
        let target = MachineTime::new(A500_PAL_FRAME_TICKS);
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        runtime
            .run_until(target, &mut host)
            .expect("running one frame should succeed");

        assert_eq!(runtime.time(), target);
        assert_eq!(runtime.frame_count, 1);
    }

    #[test]
    #[ignore]
    fn real_kickstart13_boot_reaches_visible_display() {
        let kickstart_path = Path::new("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom");
        if !kickstart_path.exists() {
            eprintln!("Skipping: cannot read {}", kickstart_path.display());
            return;
        }

        let kickstart = fs::read(kickstart_path).expect("Kickstart ROM should read");
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, kickstart)
            .expect("Kickstart 1.3 should construct");

        let disk_path = Path::new(
            "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip",
        );
        if disk_path.exists() {
            let loaded = read_media_asset(disk_path, MediaKind::Disk)
                .expect("Workbench disk archive should expand to one ADF");
            let mut media = MediaSet::new();
            media.push(MediaImage::new("floppy-0", MediaKind::Disk, &loaded.bytes));
            runtime
                .load_media(&media)
                .expect("Workbench ADF should insert into DF0");
        }

        let target = MachineTime::new(A500_PAL_FRAME_TICKS * 300);
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(target, &mut host)
            .expect("Kickstart boot should run");

        let boot = runtime.boot_status();
        assert!(boot.detected, "Kickstart 1.3 should produce visible output");
        assert!(
            runtime.non_black_pixels > 10_000,
            "Kickstart 1.3 should render a substantial visible screen"
        );
    }

    #[test]
    fn validate_kickstart_rejects_unknown_size() {
        let result = AmigaRuntime::new(Model::A500OcsPal, vec![0; 1234]);
        assert!(matches!(
            result,
            Err(MachineError::InvalidFirmware { ref id, .. }) if id == KICKSTART_ROM_ID
        ));
    }

    #[test]
    fn key_mapping_accepts_raw_hex_and_named_space() {
        assert_eq!(key_name_to_raw_code("raw-45"), Some(0x45));
        assert_eq!(key_name_to_raw_code("space"), Some(0x40));
        assert_eq!(key_name_to_raw_code("unknown"), None);
    }

    #[test]
    fn hd_adf_size_constant_stays_supported() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
            .expect("dummy Kickstart should construct");
        let disk = vec![0u8; ADF_SIZE_HD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
        runtime
            .load_media(&media)
            .expect("HD-sized ADF bytes should still insert");
        assert!(runtime.machine().has_disk());
    }
}
