//! Runtime wrapper around the A500 OCS machine.
//!
//! Implements `MachineCore` so the shell can drive the machine
//! through a common interface. The runtime owns the ROM bytes so
//! reset rebuilds from them, emits one frame per `run_until`
//! iteration, and delegates keyboard input and ADF insertion to
//! `AmigaOcs`.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, InputEvent,
    MachineCore, MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, QueryError,
    QueryResult, ResetKind, RunResult, SessionQueryProvider, StopReason,
};
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{AmigaOcs, FB_HEIGHT, FB_WIDTH};
use serde_json::json;

use crate::{A500_PAL_FRAME_TICKS, Model, profile_for};

const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
const VALID_KICKSTART_SIZES: &[usize] = &[256 * 1024, 512 * 1024];

/// Machine framebuffer width (= `FB_WIDTH`). Re-exported for host
/// integrations that size their output buffers without pulling in
/// the machine crate directly.
pub const DISPLAY_WIDTH: u32 = FB_WIDTH;
/// Machine framebuffer height (= `FB_HEIGHT`).
pub const DISPLAY_HEIGHT: u32 = FB_HEIGHT;

/// Query paths the runtime publishes through the session query
/// provider. Kept deliberately short — shell diagnostics start here
/// and can grow as the verifier UI adds panels.
const AMIGA_QUERY_PATHS: &[&str] = &[
    // Boot-status heuristic. `HeadlessSession::wait_for_boot` keys
    // off `boot.detected` so scripts can sleep-until-ready.
    "boot.detected",
    "boot.reason",
    "boot.row",
    "amiga.machine.frame_count",
    "amiga.memory.overlay",
    "amiga.cpu.pc",
    "amiga.cpu.sr",
    "amiga.cpu.ipl",
    "amiga.agnus.vpos",
    "amiga.agnus.hpos",
    "amiga.agnus.dmacon",
    "amiga.agnus.bplcon0",
    "amiga.paula.intena",
    "amiga.paula.intreq",
    "amiga.display.color00",
    "amiga.display.color01",
    "amiga.disk.inserted",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.motor_spinning",
    "amiga.keyboard.state",
    "amiga.keyboard.queued",
];

/// Amiga-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmigaSessionQueryProvider;

/// Firmware-backed Amiga runtime over the A500 OCS machine.
pub struct AmigaRuntime {
    profile: MachineProfile,
    model: Model,
    machine: AmigaOcs,
    time: MachineTime,
    kickstart_rom: Vec<u8>,
    floppy0_bytes: Option<Vec<u8>>,
    rgba_framebuffer: Vec<u8>,
    frame_count: u64,
    /// Pixel counts from the most recently emitted frame — drives the
    /// `boot.*` query set.
    non_black_pixels: u32,
    non_white_pixels: u32,
    first_active_row: Option<u32>,
}

/// Boot-status snapshot derived from the most recent frame. Matches
/// the archive's `AmigaBootStatus` heuristic: a mostly-coloured
/// framebuffer with visible pixels above row zero counts as boot-
/// detected, matching the Kickstart insert-disk screen.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AmigaBootStatus {
    detected: bool,
    reason: &'static str,
    row: Option<u32>,
}

impl AmigaRuntime {
    /// Construct a runtime from owned Kickstart ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not a supported A500-era size.
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
            non_black_pixels: 0,
            non_white_pixels: 0,
            first_active_row: None,
        };
        runtime.update_rgba_framebuffer();
        Ok(runtime)
    }

    /// Construct from the profile's firmware set.
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

    /// Construct with a zero-filled placeholder Kickstart image.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, vec![0; 256 * 1024]).expect("blank Kickstart image should be valid")
    }

    /// Read-only access to the wrapped machine.
    #[must_use]
    pub fn machine(&self) -> &AmigaOcs {
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
        self.machine.insert_adf(adf);
        self.floppy0_bytes = Some(bytes.to_vec());
        Ok(())
    }

    /// Copy the machine's ARGB framebuffer into the RGBA frame
    /// packet buffer the shell expects. ARGB → RGBA is a simple
    /// byte reorder. Side-effect: refreshes the pixel-based boot
    /// heuristic (`non_black_pixels` / `non_white_pixels` /
    /// `first_active_row`) so the next `boot.detected` query reads
    /// consistent values.
    fn update_rgba_framebuffer(&mut self) {
        let fb = self.machine.denise().framebuffer();
        let expected = (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize;
        debug_assert_eq!(fb.len(), expected);
        if self.rgba_framebuffer.len() != expected * 4 {
            self.rgba_framebuffer.resize(expected * 4, 0);
        }

        let mut non_black = 0u32;
        let mut non_white = 0u32;
        let mut first_active_row: Option<u32> = None;

        for (i, &pixel) in fb.iter().enumerate() {
            let base = i * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xFF) as u8; // R
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xFF) as u8; // G
            self.rgba_framebuffer[base + 2] = (pixel & 0xFF) as u8; // B
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xFF) as u8; // A

            let rgb = pixel & 0x00FF_FFFF;
            if rgb != 0 {
                non_black = non_black.saturating_add(1);
                if first_active_row.is_none() {
                    first_active_row = Some(i as u32 / DISPLAY_WIDTH);
                }
            }
            if rgb != 0x00FF_FFFF {
                non_white = non_white.saturating_add(1);
            }
        }

        self.non_black_pixels = non_black;
        self.non_white_pixels = non_white;
        self.first_active_row = first_active_row;
    }

    /// Boot-status heuristic matching the archive's semantics:
    ///   - `display-active` once the framebuffer has mostly non-
    ///     white content and a non-zero first active row (Kickstart
    ///     insert-disk screen or beyond)
    ///   - `monochrome-framebuffer` if some pixels lit but below
    ///     the threshold
    ///   - `no-visible-output` before the copper has programmed the
    ///     palette at all
    fn boot_status(&self) -> AmigaBootStatus {
        if let Some(row) = self.first_active_row
            && self.non_white_pixels > 1_000
        {
            AmigaBootStatus {
                detected: true,
                reason: "display-active",
                row: Some(row),
            }
        } else if self.non_black_pixels > 0 {
            AmigaBootStatus {
                detected: false,
                reason: "monochrome-framebuffer",
                row: self.first_active_row,
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

    fn query(
        &self,
        machine: &AmigaRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let amiga = &machine.machine;
        let drive = amiga.drive();
        let drive_status = drive.status();
        let boot = machine.boot_status();
        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.reason" => json!(boot.reason),
            "boot.row" => json!(boot.row),
            "amiga.machine.frame_count" => json!(machine.frame_count),
            "amiga.memory.overlay" => json!(amiga.memory().overlay()),
            "amiga.cpu.pc" => json!(amiga.cpu().regs.pc),
            "amiga.cpu.sr" => json!(amiga.cpu().regs.sr),
            "amiga.cpu.ipl" => json!(amiga.cpu().ipl),
            "amiga.agnus.vpos" => json!(amiga.agnus().vpos),
            "amiga.agnus.hpos" => json!(amiga.agnus().hpos),
            "amiga.agnus.dmacon" => json!(amiga.dmacon()),
            "amiga.agnus.bplcon0" => json!(amiga.bplcon0()),
            "amiga.paula.intena" => json!(amiga.intena()),
            "amiga.paula.intreq" => json!(amiga.intreq()),
            "amiga.display.color00" => json!(amiga.color(0)),
            "amiga.display.color01" => json!(amiga.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.keyboard.state" => json!(amiga.keyboard().debug_state_name()),
            "amiga.keyboard.queued" => json!(amiga.keyboard().queued_key_count()),
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
        // Apply queued input at the top of the run window. Keyboard
        // is the only input kind wired right now; mouse / joystick
        // come later.
        for event in host.input_events {
            apply_input_event(&mut self.machine, event);
        }

        while self.time < target {
            // Run one PAL frame.
            for _ in 0..A500_PAL_FRAME_TICKS {
                self.machine.tick();
            }
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

            // Empty-audio placeholder until the runtime grows a
            // resample buffer sourcing Paula's mix_audio_stereo
            // output.
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: 48_000,
                channels: 2,
                samples: &[],
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

fn build_machine(model: Model, kickstart_rom: &[u8]) -> AmigaOcs {
    match model {
        // A500 Kickstart 1.2+ boot paths rely on 512 KiB trapdoor
        // slow RAM at $C00000 so ExecBase lands there rather than
        // consuming scarce chip RAM.
        Model::A500OcsPal => AmigaOcs::with_slow_ram(kickstart_rom.to_vec(), 512 * 1024),
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

fn apply_input_event(machine: &mut AmigaOcs, event: &InputEvent) {
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
        "1" => 0x01, "2" => 0x02, "3" => 0x03, "4" => 0x04,
        "5" => 0x05, "6" => 0x06, "7" => 0x07, "8" => 0x08,
        "9" => 0x09, "0" => 0x0A,
        "q" => 0x10, "w" => 0x11, "e" => 0x12, "r" => 0x13,
        "t" => 0x14, "y" => 0x15, "u" => 0x16, "i" => 0x17,
        "o" => 0x18, "p" => 0x19,
        "a" => 0x20, "s" => 0x21, "d" => 0x22, "f" => 0x23,
        "g" => 0x24, "h" => 0x25, "j" => 0x26, "k" => 0x27,
        "l" => 0x28,
        "z" => 0x31, "x" => 0x32, "c" => 0x33, "v" => 0x34,
        "b" => 0x35, "n" => 0x36, "m" => 0x37,
        "space" => 0x40, "backspace" => 0x41, "tab" => 0x42,
        "enter" | "return" => 0x45,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{
        FirmwareImage, MediaImage, NullAudioSink, NullFrameSink, NullTraceSink,
    };
    use format_commodore_amiga_adf::ADF_SIZE_DD;

    fn dummy_kickstart() -> Vec<u8> {
        let mut kickstart = vec![0u8; 256 * 1024];
        // Initial SSP + PC: high half of a valid address so the CPU
        // fetches a reset vector that points somewhere in-ROM.
        kickstart[0] = 0x00; kickstart[1] = 0x08; kickstart[2] = 0x00; kickstart[3] = 0x00;
        kickstart[4] = 0x00; kickstart[5] = 0xF8; kickstart[6] = 0x00; kickstart[7] = 0x08;
        // Branch-to-self loop at the reset PC.
        kickstart[8] = 0x60; kickstart[9] = 0xFE;
        kickstart
    }

    fn dummy_firmware() -> FirmwareSet<'static> {
        let kickstart = dummy_kickstart().into_boxed_slice();
        let bytes: &'static [u8] = Box::leak(kickstart);
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(KICKSTART_ROM_ID, bytes));
        firmware
    }

    #[test]
    fn from_firmware_accepts_supported_kickstart_size() {
        let runtime = AmigaRuntime::from_firmware(Model::A500OcsPal, &dummy_firmware());
        assert!(runtime.is_ok());
    }

    #[test]
    fn new_rejects_undersized_rom() {
        match AmigaRuntime::new(Model::A500OcsPal, vec![0; 128 * 1024]) {
            Err(MachineError::InvalidFirmware { id, .. }) => assert_eq!(id, KICKSTART_ROM_ID),
            Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
            Ok(_) => panic!("expected InvalidFirmware, got Ok"),
        }
    }

    #[test]
    fn load_media_accepts_dd_adf() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
            .expect("dummy Kickstart should construct");
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
        runtime.load_media(&media).expect("ADF bytes should insert into DF0");
        assert!(runtime.machine().drive().has_disk());
    }

    #[test]
    fn load_media_rejects_unknown_slot() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).unwrap();
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-1", MediaKind::Disk, &disk));
        let err = runtime.load_media(&media).unwrap_err();
        matches!(err, MachineError::UnknownMediaSlot { .. });
    }

    #[test]
    fn run_until_advances_time_and_emits_frame() {
        let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).unwrap();
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
        runtime.run_until(target, &mut host).expect("one frame should run");
        assert_eq!(runtime.time(), target);
        assert_eq!(runtime.frame_count, 1);
    }

    #[test]
    fn query_provider_returns_declared_paths() {
        let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).unwrap();
        let provider = AmigaSessionQueryProvider;
        let paths = provider.query_paths(&runtime, None);
        assert!(paths.contains(&"amiga.cpu.pc".to_owned()));
        assert!(paths.contains(&"amiga.disk.inserted".to_owned()));
        assert!(paths.contains(&"amiga.keyboard.state".to_owned()));
    }

    #[test]
    fn query_cpu_pc_returns_initial_reset_vector() {
        let runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).unwrap();
        let result = AmigaSessionQueryProvider
            .query(&runtime, "amiga.cpu.pc")
            .expect("query succeeds")
            .expect("path present");
        assert_eq!(result.path, "amiga.cpu.pc");
        assert_eq!(result.value, json!(0x00F8_0008u32));
    }
}
