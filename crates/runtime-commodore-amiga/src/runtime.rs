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
use machine_commodore_amiga_ocs::{AmigaOcs, FB_HEIGHT, FB_WIDTH, RamConfig};
use serde_json::json;

use crate::{A500_PAL_CCK_HZ, A500_PAL_FRAME_TICKS, Model, profile_for};

const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
const A1000_BOOTSTRAP_ROM_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
const VALID_KICKSTART_SIZES: &[usize] = &[256 * 1024, 512 * 1024];
const VALID_A1000_BOOTSTRAP_SIZES: &[usize] = &[64 * 1024];
const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
const AUDIO_CHANNELS: u8 = 2;
const A500_PAL_TICK_HZ: u64 = A500_PAL_CCK_HZ * 2;

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
    "amiga.a1000.boot_rom_visible",
    "amiga.a1000.wom_locked",
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
    "amiga.debug.dsk_write_count",
    "amiga.debug.last_dsk_write",
    "amiga.display.color00",
    "amiga.display.color01",
    "amiga.disk.inserted",
    "amiga.disk.change_pending",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.motor_on",
    "amiga.disk.motor_spinning",
    "amiga.disk.step_events",
    "amiga.keyboard.state",
    "amiga.keyboard.queued",
];

/// Amiga-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmigaSessionQueryProvider;

/// Firmware-backed Amiga runtime over the OCS machine family.
pub struct AmigaRuntime {
    profile: MachineProfile,
    model: Model,
    /// Active RAM layout. Defaults to `model.ram_config()` for the
    /// standard model presets; `from_ram_config` overrides it with a
    /// caller-supplied layout. Held here so `reset` / `rebuild_machine`
    /// reconstructs with the same sizes.
    ram_config: RamConfig,
    machine: AmigaOcs,
    time: MachineTime,
    firmware_rom: Vec<u8>,
    floppy0_bytes: Option<Vec<u8>>,
    rgba_framebuffer: Vec<u8>,
    frame_count: u64,
    /// Pixel counts from the most recently emitted frame — drives the
    /// `boot.*` query set.
    non_black_pixels: u32,
    non_white_pixels: u32,
    first_active_row: Option<u32>,
    /// Fractional 48 kHz resampler phase. The source advances once
    /// per machine tick (master/4); Paula output itself only changes
    /// on CCK boundaries, but sampling at the finer runtime tick keeps
    /// this phase stable across frame boundaries.
    audio_sample_accumulator: u64,
    audio_buffer: Vec<f32>,
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
    /// Construct a runtime from owned model-specific firmware bytes,
    /// using the model's preset RAM layout.
    ///
    /// # Errors
    ///
    /// Returns an error if the firmware size is not valid for the
    /// selected model.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        Self::with_ram_config(model, firmware_rom, model.ram_config())
    }

    /// Construct a runtime with an explicit RAM layout, bypassing the
    /// model's preset. Useful for matching custom hardware profiles
    /// (e.g. A500 + custom Zorro-II fast-RAM size) or driving tests
    /// over ranges the enum doesn't cover. The model still determines
    /// the profile metadata (display name, firmware, media slots).
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is invalid. Panics if the RAM
    /// layout is not one of the supported size combinations — see
    /// `RamConfig::is_valid`.
    pub fn with_ram_config(
        model: Model,
        firmware_rom: Vec<u8>,
        ram_config: RamConfig,
    ) -> Result<Self, MachineError> {
        validate_firmware_rom(model, &firmware_rom)?;
        let machine = build_machine(model, ram_config, &firmware_rom);
        let mut runtime = Self {
            profile: profile_for(model),
            model,
            ram_config,
            machine,
            time: MachineTime::default(),
            firmware_rom,
            floppy0_bytes: None,
            rgba_framebuffer: vec![0; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 4) as usize],
            frame_count: 0,
            non_black_pixels: 0,
            non_white_pixels: 0,
            first_active_row: None,
            audio_sample_accumulator: 0,
            audio_buffer: Vec::with_capacity(audio_buffer_capacity_for_frame()),
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
        let firmware_id = firmware_id_for_model(model);
        let image = firmware
            .bytes(firmware_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: firmware_id.to_owned(),
            })?;
        Self::new(model, image.to_vec())
    }

    /// Construct with a zero-filled placeholder model-specific ROM.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, blank_firmware_rom(model))
            .expect("blank model firmware image should be valid")
    }

    /// Read-only access to the wrapped machine.
    #[must_use]
    pub fn machine(&self) -> &AmigaOcs {
        &self.machine
    }

    /// Mutable access to the wrapped machine. Only for tests /
    /// integrations that need to drive the tick loop directly (e.g.
    /// autoconfig boot tests that run the machine outside `run_until`).
    pub fn machine_mut(&mut self) -> &mut AmigaOcs {
        &mut self.machine
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        validate_firmware_rom(self.model, &self.firmware_rom)?;
        self.machine = build_machine(self.model, self.ram_config, &self.firmware_rom);
        if let Some(bytes) = self.floppy0_bytes.clone() {
            self.insert_floppy_bytes("floppy-0", &bytes)?;
        }
        self.time = MachineTime::default();
        self.frame_count = 0;
        self.audio_sample_accumulator = 0;
        self.audio_buffer.clear();
        self.update_rgba_framebuffer();
        Ok(())
    }

    /// RAM layout currently installed — read back for diagnostics or
    /// for tests asserting a preset was honoured.
    #[must_use]
    pub fn ram_config(&self) -> RamConfig {
        self.ram_config
    }

    /// Active model (affects profile metadata, not the RAM layout).
    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    fn insert_floppy_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let adf = Adf::from_bytes(bytes.to_vec()).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason: reason.to_string(),
        })?;
        if self.model == Model::A1000OcsPal {
            self.machine.insert_adf_with_change_pending(adf);
        } else {
            self.machine.insert_adf(adf);
        }
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

    fn tick_and_sample_audio(&mut self) {
        self.machine.tick();
        self.audio_sample_accumulator = self
            .audio_sample_accumulator
            .saturating_add(u64::from(AUDIO_SAMPLE_RATE_HZ));

        while self.audio_sample_accumulator >= A500_PAL_TICK_HZ {
            self.audio_sample_accumulator -= A500_PAL_TICK_HZ;
            let (left, right) = self.machine.paula().mix_audio_stereo();
            self.audio_buffer.push(left);
            self.audio_buffer.push(right);
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
        let amiga = &machine.machine;
        let drive = amiga.drive();
        let drive_status = drive.status();
        let boot = machine.boot_status();
        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.reason" => json!(boot.reason),
            "boot.row" => json!(boot.row),
            "amiga.a1000.boot_rom_visible" => json!(amiga.memory().a1000_boot_rom_visible()),
            "amiga.a1000.wom_locked" => json!(amiga.memory().a1000_wom_locked()),
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
            "amiga.debug.dsk_write_count" => json!(amiga.debug_dsk_log.len()),
            "amiga.debug.last_dsk_write" => {
                json!(amiga.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({
                        "cck": cck,
                        "pc": pc,
                        "reg": reg,
                        "val": val,
                    })
                }))
            }
            "amiga.display.color00" => json!(amiga.color(0)),
            "amiga.display.color01" => json!(amiga.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.change_pending" => json!(drive_status.disk_change),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_on" => json!(drive.motor_on()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.disk.step_events" => json!(drive.step_event_counter()),
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
            self.audio_buffer.clear();
            for _ in 0..A500_PAL_FRAME_TICKS {
                self.tick_and_sample_audio();
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

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE_HZ,
                channels: AUDIO_CHANNELS,
                samples: &self.audio_buffer,
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

fn build_machine(model: Model, ram_config: RamConfig, firmware_rom: &[u8]) -> AmigaOcs {
    match model {
        Model::A1000OcsPal => AmigaOcs::with_a1000_bootstrap_rom(firmware_rom.to_vec(), ram_config),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => {
            // Every A500-family layout in the current `Model`
            // catalogue routes through the same autoconfig-aware
            // constructor. A Zorro-II fast-RAM board is attached
            // automatically when `ram_config.fast_kb > 0`; the ROM's
            // `expansion.library` picks it up during boot without
            // runtime cooperation.
            AmigaOcs::with_ram_config(firmware_rom.to_vec(), ram_config)
        }
    }
}

fn firmware_id_for_model(model: Model) -> &'static str {
    match model {
        Model::A1000OcsPal => A1000_BOOTSTRAP_ROM_ID,
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => KICKSTART_ROM_ID,
    }
}

fn blank_standard_kickstart_rom() -> Vec<u8> {
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

fn blank_a1000_bootstrap_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 64 * 1024];
    rom[0] = 0x11;
    rom[1] = 0x11;
    rom[2] = 0x4E;
    rom[3] = 0xF9;
    rom[4] = 0x00;
    rom[5] = 0xF8;
    rom[6] = 0x00;
    rom[7] = 0x08;
    rom[8] = 0x60;
    rom[9] = 0xFE;
    rom
}

fn blank_firmware_rom(model: Model) -> Vec<u8> {
    match model {
        Model::A1000OcsPal => blank_a1000_bootstrap_rom(),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => blank_standard_kickstart_rom(),
    }
}

fn validate_firmware_rom(model: Model, firmware_rom: &[u8]) -> Result<(), MachineError> {
    let (valid_sizes, firmware_id) = match model {
        Model::A1000OcsPal => (VALID_A1000_BOOTSTRAP_SIZES, A1000_BOOTSTRAP_ROM_ID),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => (VALID_KICKSTART_SIZES, KICKSTART_ROM_ID),
    };
    if valid_sizes.contains(&firmware_rom.len()) {
        return Ok(());
    }
    Err(MachineError::InvalidFirmware {
        id: firmware_id.to_owned(),
        reason: format!(
            "expected one of {:?} bytes, got {}",
            valid_sizes,
            firmware_rom.len()
        ),
    })
}

fn audio_sample_frames_for_ticks(ticks: u64) -> usize {
    usize::try_from((ticks.saturating_mul(u64::from(AUDIO_SAMPLE_RATE_HZ))) / A500_PAL_TICK_HZ)
        .unwrap_or(usize::MAX)
}

fn audio_buffer_capacity_for_frame() -> usize {
    audio_sample_frames_for_ticks(A500_PAL_FRAME_TICKS)
        .saturating_add(1)
        .saturating_mul(usize::from(AUDIO_CHANNELS))
}

fn apply_input_event(machine: &mut AmigaOcs, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(code) = key_name_to_raw_code(name.as_ref()) {
                machine.key_event(code, *pressed);
            }
        }
        InputEvent::PointerMotion { device, dx, dy } if device.as_ref() == "mouse-1" => {
            machine.move_mouse_port0(*dx, *dy);
        }
        InputEvent::PointerButton {
            device,
            button,
            pressed,
        } if device.as_ref() == "mouse-1" => {
            machine.set_mouse_button_port0(button.as_ref(), *pressed);
        }
        _ => {}
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
    use emu198x_shell::{AudioSink, FirmwareImage, MediaImage, NullFrameSink, NullTraceSink};
    use format_commodore_amiga_adf::ADF_SIZE_DD;

    #[derive(Default)]
    struct AudioCollector {
        packets: usize,
        last_timestamp: MachineTime,
        last_sample_rate: u32,
        last_channels: u8,
        last_samples: Vec<f32>,
    }

    impl AudioSink for AudioCollector {
        fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
            self.packets += 1;
            self.last_timestamp = packet.timestamp;
            self.last_sample_rate = packet.sample_rate;
            self.last_channels = packet.channels;
            self.last_samples.clear();
            self.last_samples.extend_from_slice(packet.samples);
            Ok(())
        }
    }

    fn dummy_kickstart() -> Vec<u8> {
        blank_standard_kickstart_rom()
    }

    fn dummy_a1000_bootstrap_rom() -> Vec<u8> {
        blank_a1000_bootstrap_rom()
    }

    fn dummy_firmware() -> FirmwareSet<'static> {
        let kickstart = dummy_kickstart().into_boxed_slice();
        let bytes: &'static [u8] = Box::leak(kickstart);
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(KICKSTART_ROM_ID, bytes));
        firmware
    }

    fn dummy_a1000_firmware() -> FirmwareSet<'static> {
        let bootstrap = dummy_a1000_bootstrap_rom().into_boxed_slice();
        let bytes: &'static [u8] = Box::leak(bootstrap);
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(A1000_BOOTSTRAP_ROM_ID, bytes));
        firmware
    }

    #[test]
    fn from_firmware_accepts_supported_kickstart_size() {
        let runtime = AmigaRuntime::from_firmware(Model::A500OcsPal, &dummy_firmware());
        assert!(runtime.is_ok());
    }

    #[test]
    fn from_firmware_accepts_supported_a1000_bootstrap_size() {
        let runtime = AmigaRuntime::from_firmware(Model::A1000OcsPal, &dummy_a1000_firmware());
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
    fn a1000_new_rejects_non_bootstrap_rom_size() {
        match AmigaRuntime::new(Model::A1000OcsPal, vec![0; 256 * 1024]) {
            Err(MachineError::InvalidFirmware { id, .. }) => {
                assert_eq!(id, A1000_BOOTSTRAP_ROM_ID)
            }
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
        runtime
            .load_media(&media)
            .expect("ADF bytes should insert into DF0");
        assert!(runtime.machine().drive().has_disk());
    }

    #[test]
    fn load_media_keeps_a1000_disk_change_pending() {
        let mut runtime = AmigaRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
            .expect("dummy bootstrap ROM should construct");
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
        runtime
            .load_media(&media)
            .expect("ADF bytes should insert into DF0");

        assert!(runtime.machine().drive().has_disk());
        assert!(
            runtime.machine().drive().status().disk_change,
            "A1000 bootstrap expects a fresh /DSKCHANGE event when Kickstart media is loaded"
        );
    }

    #[test]
    fn load_media_rejects_unknown_slot() {
        let mut runtime =
            AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
        let disk = vec![0u8; ADF_SIZE_DD];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("floppy-1", MediaKind::Disk, &disk));
        let err = runtime.load_media(&media).expect_err("unknown slot");
        matches!(err, MachineError::UnknownMediaSlot { .. });
    }

    #[test]
    fn run_until_advances_time_and_emits_frame() {
        let mut runtime =
            AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
        let target = MachineTime::new(A500_PAL_FRAME_TICKS);
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = AudioCollector::default();
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(target, &mut host)
            .expect("one frame should run");
        assert_eq!(runtime.time(), target);
        assert_eq!(runtime.frame_count, 1);
        assert_eq!(audio_sink.packets, 1);
        assert_eq!(audio_sink.last_timestamp, target);
        assert_eq!(audio_sink.last_sample_rate, AUDIO_SAMPLE_RATE_HZ);
        assert_eq!(audio_sink.last_channels, AUDIO_CHANNELS);
        assert_eq!(
            audio_sink.last_samples.len(),
            audio_sample_frames_for_ticks(A500_PAL_FRAME_TICKS) * usize::from(AUDIO_CHANNELS)
        );
        assert!(
            audio_sink
                .last_samples
                .iter()
                .all(|sample| sample.is_finite())
        );
    }

    #[test]
    fn run_until_applies_mouse_input_to_controller_port_zero() {
        let mut runtime =
            AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
        let input_events = [
            InputEvent::PointerMotion {
                device: "mouse-1".into(),
                dx: 3,
                dy: 4,
            },
            InputEvent::PointerButton {
                device: "mouse-1".into(),
                button: "left".into(),
                pressed: true,
            },
        ];
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = AudioCollector::default();
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &input_events,
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        runtime
            .run_until(MachineTime::new(A500_PAL_FRAME_TICKS), &mut host)
            .expect("one frame should run");

        assert_eq!(runtime.machine().read_word(0x00DF_F00A), 0x0403);
        assert_eq!(runtime.machine().read_word(0x00BF_E001) & 0x80, 0);
    }

    #[test]
    fn query_provider_returns_declared_paths() {
        let runtime =
            AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
        let provider = AmigaSessionQueryProvider;
        let paths = provider.query_paths(&runtime, None);
        assert!(paths.contains(&"amiga.a1000.boot_rom_visible".to_owned()));
        assert!(paths.contains(&"amiga.a1000.wom_locked".to_owned()));
        assert!(paths.contains(&"amiga.cpu.pc".to_owned()));
        assert!(paths.contains(&"amiga.debug.dsk_write_count".to_owned()));
        assert!(paths.contains(&"amiga.disk.change_pending".to_owned()));
        assert!(paths.contains(&"amiga.disk.inserted".to_owned()));
        assert!(paths.contains(&"amiga.disk.step_events".to_owned()));
        assert!(paths.contains(&"amiga.keyboard.state".to_owned()));
    }

    #[test]
    fn query_cpu_pc_returns_initial_reset_vector() {
        let runtime =
            AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
        let result = AmigaSessionQueryProvider
            .query(&runtime, "amiga.cpu.pc")
            .expect("query succeeds")
            .expect("path present");
        assert_eq!(result.path, "amiga.cpu.pc");
        assert_eq!(result.value, json!(0x00F8_0008u32));
    }

    #[test]
    fn a1000_queries_report_bootstrap_state() {
        let runtime = AmigaRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
            .expect("runtime init");
        let boot_rom_visible = AmigaSessionQueryProvider
            .query(&runtime, "amiga.a1000.boot_rom_visible")
            .expect("query succeeds")
            .expect("path present");
        assert_eq!(boot_rom_visible.value, json!(true));

        let wom_locked = AmigaSessionQueryProvider
            .query(&runtime, "amiga.a1000.wom_locked")
            .expect("query succeeds")
            .expect("path present");
        assert_eq!(wom_locked.value, json!(false));
    }
}
