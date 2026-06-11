//! Runtime wrapper for the fresh-workspace Commodore 64.

use std::borrow::Cow;

use common_commodore_iec::IecBus;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    ResetKind, RunResult, StopReason, TraceEvent,
};
use machine_commodore_1541::{DRIVE1541_CPU_HZ, Drive1541, Drive1541Config};
use machine_commodore_c64::{AudioControls, C64, C64Config, C64Model, SidChannel};
use serde::Serialize;
use serde_json::json;

use crate::input::apply_input_event;
use crate::snapshot;
use crate::{Model, profile_for};

const KERNAL_ROM_SIZE: usize = 0x2000;
const BASIC_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;
const DOS1541_ROM_SIZE: usize = 0x4000;
pub(crate) const SCREEN_RAM_BASE: u16 = 0x0400;
pub(crate) const SCREEN_TEXT_WIDTH: usize = 40;
pub(crate) const SCREEN_TEXT_HEIGHT: usize = 25;

/// Firmware-backed Commodore 64 runtime.
pub struct C64Runtime {
    profile: MachineProfile,
    model: Model,
    machine: C64,
    time: MachineTime,
    kernal_rom: Vec<u8>,
    basic_rom: Vec<u8>,
    character_rom: Vec<u8>,
    drive8_dos_rom: Option<Vec<u8>>,
    drive8: Option<Drive1541>,
    iec_bus: IecBus,
    drive8_cycle_accum: u64,
    rgba_framebuffer: Vec<u8>,
    trace_vic_colour_writes: bool,
    trace_drive_rom_window: Option<(u16, u16)>,
    last_drive_trace_state: Option<DriveRomTraceState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct DriveRomTraceState {
    host_cpu_pc: u16,
    host_cia2_pa: u8,
    host_cia2_port_a_latch: u8,
    host_cia2_ddra: u8,
    pc: u16,
    opcode: u8,
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    p: u8,
    iec_cpu_bus: u8,
    iec_cpu_port: u8,
    iec_drive_bus: u8,
    iec_drive_data: u8,
    iec_drive_port: u8,
    via1_ifr: u8,
    via1_ier: u8,
    via1_pcr: u8,
    via1_pa: u8,
    via1_pb: u8,
    via1_ora: u8,
    via1_orb: u8,
    via1_ddrb: u8,
    via2_ifr: u8,
    via2_ier: u8,
    via2_pcr: u8,
    via2_pb: u8,
    via2_pa: u8,
    job_0000: u8,
    job_0001: u8,
    job_0002: u8,
    job_0003: u8,
    job_0004: u8,
    job_0005: u8,
    zp_007c: u8,
    zp_007d: u8,
    zp_001c: u8,
    zp_001d: u8,
    zp_006f: u8,
    zp_0070: u8,
    zp_0072: u8,
    zp_0077: u8,
    zp_0078: u8,
    zp_0079: u8,
    zp_007a: u8,
    zp_007f: u8,
    zp_0082: u8,
    zp_0083: u8,
    zp_0086: u8,
    mem_00e2: u8,
    mem_022b: u8,
    mem_024d: u8,
    mem_025b: u8,
    mem_028c: u8,
    mem_028d: u8,
    mem_028e: u8,
    mem_026c: u8,
    mem_026d: u8,
}

impl Model {
    const fn to_machine_model(self) -> C64Model {
        match self {
            Self::C64PalBreadbin => C64Model::PalBreadbin,
            Self::C64NtscBreadbin => C64Model::NtscBreadbin,
        }
    }
}

impl C64Runtime {
    /// Creates a C64 runtime from owned ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM image has the wrong size.
    pub fn new(
        model: Model,
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        character_rom: Vec<u8>,
        drive8_dos_rom: Option<Vec<u8>>,
    ) -> Result<Self, MachineError> {
        validate_rom_size("commodore-c64-kernal-rom", &kernal_rom, KERNAL_ROM_SIZE)?;
        validate_rom_size("commodore-c64-basic-rom", &basic_rom, BASIC_ROM_SIZE)?;
        validate_rom_size(
            "commodore-c64-character-rom",
            &character_rom,
            CHARACTER_ROM_SIZE,
        )?;
        if let Some(drive_rom) = drive8_dos_rom.as_deref() {
            validate_rom_size("commodore-1541-dos-rom", drive_rom, DOS1541_ROM_SIZE)?;
        }

        let machine = build_machine(model, &kernal_rom, &basic_rom, &character_rom)?;
        let mut iec_bus = IecBus::new();
        let drive8 = build_drive(&drive8_dos_rom, &mut iec_bus)?;
        let rgba_framebuffer = vec![
            0;
            (machine.vic().framebuffer_width() * machine.vic().framebuffer_height() * 4)
                as usize
        ];

        Ok(Self {
            profile: profile_for(model),
            model,
            machine,
            time: MachineTime::default(),
            kernal_rom,
            basic_rom,
            character_rom,
            drive8_dos_rom,
            drive8,
            iec_bus,
            drive8_cycle_accum: 0,
            rgba_framebuffer,
            trace_vic_colour_writes: false,
            trace_drive_rom_window: None,
            last_drive_trace_state: None,
        })
    }

    /// Creates a runtime from the profile-declared firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware is missing or invalid.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;

        let kernal = firmware.bytes("commodore-c64-kernal-rom").ok_or_else(|| {
            MachineError::MissingFirmware {
                id: "commodore-c64-kernal-rom".to_owned(),
            }
        })?;
        let basic = firmware.bytes("commodore-c64-basic-rom").ok_or_else(|| {
            MachineError::MissingFirmware {
                id: "commodore-c64-basic-rom".to_owned(),
            }
        })?;
        let character = firmware
            .bytes("commodore-c64-character-rom")
            .ok_or_else(|| MachineError::MissingFirmware {
                id: "commodore-c64-character-rom".to_owned(),
            })?;
        let drive8_dos_rom = firmware.bytes("commodore-1541-dos-rom").map(<[u8]>::to_vec);

        Self::new(
            model,
            kernal.to_vec(),
            basic.to_vec(),
            character.to_vec(),
            drive8_dos_rom,
        )
    }

    /// Creates a runtime backed by zero-filled ROMs.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        match Self::new(
            model,
            vec![0; KERNAL_ROM_SIZE],
            vec![0; BASIC_ROM_SIZE],
            vec![0; CHARACTER_ROM_SIZE],
            None,
        ) {
            Ok(runtime) => runtime,
            Err(reason) => unreachable!(
                "blank C64 runtime should always construct from fixed-size ROM images: {reason}"
            ),
        }
    }

    /// Returns the wrapped C64 machine.
    #[must_use]
    pub fn machine(&self) -> &C64 {
        &self.machine
    }

    /// Returns mutable access to the wrapped C64 machine.
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut C64 {
        &mut self.machine
    }

    /// Current host-side SID audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        self.machine.audio_controls()
    }

    /// Replace all host-side SID audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.machine.set_audio_controls(controls);
    }

    /// Enable or disable one SID voice in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        self.machine.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one SID voice's host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        self.machine.set_audio_channel_gain(channel, gain);
    }

    #[must_use]
    pub fn drive8(&self) -> Option<&Drive1541> {
        self.drive8.as_ref()
    }

    /// Decodes drive 8's live GCR surface back into a `D64` image so the host
    /// can persist a SAVE. Returns `None` when no drive or disk is present.
    /// See `knowledge/decisions/disk-save-write-back.md`.
    #[must_use]
    pub fn flush_drive8_image(&self) -> Option<Vec<u8>> {
        self.drive8.as_ref()?.flush_image()
    }

    /// Returns the current runtime time in `phi2` cycles.
    #[must_use]
    pub const fn time(&self) -> MachineTime {
        self.time
    }

    /// Read-only access to the runtime's IEC bus state. Used by the
    /// query module for `c64.iec.*` paths and by the snapshot module
    /// for envelope encoding.
    #[must_use]
    pub(crate) fn iec_bus(&self) -> &IecBus {
        &self.iec_bus
    }

    /// Read-only access to the runtime profile descriptor.
    #[must_use]
    pub(crate) fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    /// Drive-8 cycle accumulator used by the snapshot envelope.
    #[must_use]
    pub(crate) fn drive8_cycle_accum(&self) -> u64 {
        self.drive8_cycle_accum
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_drive8(&mut self, drive: Option<Drive1541>) {
        self.drive8 = drive;
    }

    pub(crate) fn set_iec_bus(&mut self, bus: IecBus) {
        self.iec_bus = bus;
    }

    pub(crate) fn set_drive8_cycle_accum(&mut self, accum: u64) {
        self.drive8_cycle_accum = accum;
    }

    /// Imports one PRG byte stream into raw RAM and returns its load address.
    ///
    /// This is a host-side convenience path, not emulated media.
    ///
    /// # Errors
    ///
    /// Returns an error if the PRG header is malformed.
    pub fn load_prg_bytes(&mut self, data: &[u8]) -> Result<u16, String> {
        self.machine.load_prg(data)
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        self.machine = build_machine(
            self.model,
            &self.kernal_rom,
            &self.basic_rom,
            &self.character_rom,
        )?;
        self.iec_bus = IecBus::new();
        self.drive8 = build_drive(&self.drive8_dos_rom, &mut self.iec_bus)?;
        self.drive8_cycle_accum = 0;
        self.time = MachineTime::default();
        Ok(())
    }

    /// Resync the RGBA framebuffer from the machine's native buffer. Named for
    /// the debug-target macros, which call it after a poke/step.
    fn update_rgba_framebuffer(&mut self) {
        repack_rgba8888(self.machine.framebuffer(), &mut self.rgba_framebuffer);
    }

    fn emit_frame(&mut self, host: &mut HostIo<'_>) -> Result<(), MachineError> {
        repack_rgba8888(self.machine.framebuffer(), &mut self.rgba_framebuffer);
        host.frame_sink.push_frame(FramePacket {
            timestamp: self.time,
            format: emu198x_shell::PixelFormat::Rgba8888,
            width: self.machine.vic().framebuffer_width(),
            height: self.machine.vic().framebuffer_height(),
            palette: None,
            pixels: &self.rgba_framebuffer,
        })?;
        Ok(())
    }

    /// Enables or disables one targeted VIC colour-write trace stream.
    pub fn set_trace_vic_colour_writes(&mut self, enabled: bool) {
        self.trace_vic_colour_writes = enabled;
    }

    /// Enables or disables one targeted 1541 DOS-ROM trace window.
    pub fn set_trace_drive_rom_window(&mut self, window: Option<(u16, u16)>) {
        self.trace_drive_rom_window = window;
        self.last_drive_trace_state = None;
    }

    fn drive_trace_state(&self, drive: &Drive1541) -> DriveRomTraceState {
        let cpu = drive.cpu();
        DriveRomTraceState {
            host_cpu_pc: self.machine.cpu().regs.pc,
            host_cia2_pa: self.machine.cia2().pa,
            host_cia2_port_a_latch: self.machine.cia2().port_a_latch(),
            host_cia2_ddra: self.machine.cia2().ddr_a(),
            pc: cpu.regs.pc,
            opcode: drive.peek_with_iec_bus(cpu.regs.pc, &self.iec_bus),
            a: cpu.regs.a,
            x: cpu.regs.x,
            y: cpu.regs.y,
            sp: cpu.regs.sp,
            p: cpu.regs.p,
            iec_cpu_bus: self.iec_bus.cpu_bus(),
            iec_cpu_port: self.iec_bus.cpu_port(),
            iec_drive_bus: self.iec_bus.drive_bus(8).unwrap_or(0xFF),
            iec_drive_data: self.iec_bus.drive_data(8).unwrap_or(0xFF),
            iec_drive_port: self.iec_bus.drive_port(),
            via1_ifr: drive.via1().peek(0x0D),
            via1_ier: drive.via1().peek(0x0E),
            via1_pcr: drive.via1().peek(0x0C),
            via1_pa: drive.via1().pa,
            via1_pb: drive.via1().pb,
            via1_ora: drive.via1().ora(),
            via1_orb: drive.via1().orb(),
            via1_ddrb: drive.via1().ddrb(),
            via2_ifr: drive.via2().peek(0x0D),
            via2_ier: drive.via2().peek(0x0E),
            via2_pcr: drive.via2().peek(0x0C),
            via2_pb: drive.via2().pb,
            via2_pa: drive.via2().pa,
            job_0000: drive.peek_with_iec_bus(0x0000, &self.iec_bus),
            job_0001: drive.peek_with_iec_bus(0x0001, &self.iec_bus),
            job_0002: drive.peek_with_iec_bus(0x0002, &self.iec_bus),
            job_0003: drive.peek_with_iec_bus(0x0003, &self.iec_bus),
            job_0004: drive.peek_with_iec_bus(0x0004, &self.iec_bus),
            job_0005: drive.peek_with_iec_bus(0x0005, &self.iec_bus),
            zp_007c: drive.peek_with_iec_bus(0x007C, &self.iec_bus),
            zp_007d: drive.peek_with_iec_bus(0x007D, &self.iec_bus),
            zp_001c: drive.peek_with_iec_bus(0x001C, &self.iec_bus),
            zp_001d: drive.peek_with_iec_bus(0x001D, &self.iec_bus),
            zp_006f: drive.peek_with_iec_bus(0x006F, &self.iec_bus),
            zp_0070: drive.peek_with_iec_bus(0x0070, &self.iec_bus),
            zp_0072: drive.peek_with_iec_bus(0x0072, &self.iec_bus),
            zp_0077: drive.peek_with_iec_bus(0x0077, &self.iec_bus),
            zp_0078: drive.peek_with_iec_bus(0x0078, &self.iec_bus),
            zp_0079: drive.peek_with_iec_bus(0x0079, &self.iec_bus),
            zp_007a: drive.peek_with_iec_bus(0x007A, &self.iec_bus),
            zp_007f: drive.peek_with_iec_bus(0x007F, &self.iec_bus),
            zp_0082: drive.peek_with_iec_bus(0x0082, &self.iec_bus),
            zp_0083: drive.peek_with_iec_bus(0x0083, &self.iec_bus),
            zp_0086: drive.peek_with_iec_bus(0x0086, &self.iec_bus),
            mem_00e2: drive.peek_with_iec_bus(0x00E2, &self.iec_bus),
            mem_022b: drive.peek_with_iec_bus(0x022B, &self.iec_bus),
            mem_024d: drive.peek_with_iec_bus(0x024D, &self.iec_bus),
            mem_025b: drive.peek_with_iec_bus(0x025B, &self.iec_bus),
            mem_028c: drive.peek_with_iec_bus(0x028C, &self.iec_bus),
            mem_028d: drive.peek_with_iec_bus(0x028D, &self.iec_bus),
            mem_028e: drive.peek_with_iec_bus(0x028E, &self.iec_bus),
            mem_026c: drive.peek_with_iec_bus(0x026C, &self.iec_bus),
            mem_026d: drive.peek_with_iec_bus(0x026D, &self.iec_bus),
        }
    }
}

impl MachineCore for C64Runtime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine()
            .expect("C64 runtime reset should rebuild from already-validated ROMs");
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match image.slot.as_ref() {
                "tape-1" => {
                    if image.kind != MediaKind::Tape {
                        return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
                    }

                    self.machine.load_tap_bytes(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: image.slot.as_ref().to_owned(),
                            reason,
                        }
                    })?;
                }
                "drive-8" => {
                    if image.kind != MediaKind::Disk {
                        return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
                    }
                    let drive =
                        self.drive8
                            .as_mut()
                            .ok_or_else(|| MachineError::MissingFirmware {
                                id: "commodore-1541-dos-rom".to_owned(),
                            })?;
                    drive
                        .load_d64_bytes_writable(image.bytes, image.writable)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: image.slot.as_ref().to_owned(),
                            reason: reason.to_string(),
                        })?;
                }
                _ => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: image.slot.as_ref().to_owned(),
                    });
                }
            }
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

        let mut prev_border = self.machine.vic_register(0x20) & 0x0F;
        let mut prev_background = self.machine.vic_register(0x21) & 0x0F;

        while self.time < target {
            let frame_complete = if let Some(drive8) = self.drive8.as_mut() {
                let c64_hz = u128::from(self.machine.timing().cpu_hz);
                let next_c64_tick = u128::from(self.machine.phi2_cycles().saturating_add(1))
                    * u128::from(DRIVE1541_CPU_HZ);
                let next_drive_tick = u128::from(drive8.cycles().saturating_add(1)) * c64_hz;

                if next_drive_tick <= next_c64_tick {
                    drive8.tick_with_iec_bus(&mut self.iec_bus);
                    self.machine.sync_iec_bus(&mut self.iec_bus);
                    continue;
                }

                let frame_complete = self.machine.tick_with_iec_bus(&mut self.iec_bus);
                drive8.sync_iec_bus(&mut self.iec_bus);
                frame_complete
            } else {
                self.machine.tick()
            };
            self.time = MachineTime::new(self.machine.phi2_cycles());

            if self.trace_vic_colour_writes {
                let border = self.machine.vic_register(0x20) & 0x0F;
                if border != prev_border {
                    let payload = serde_json::to_vec(&json!({
                        "register": "d020",
                        "value": border,
                        "raster_line": self.machine.raster_line(),
                        "cycle_in_line": self.machine.cycle_in_line(),
                        "cpu_pc": self.machine.cpu().regs.pc,
                    }))
                    .map_err(|reason| MachineError::Host {
                        reason: format!("failed to encode trace payload: {reason}"),
                    })?;
                    host.trace_sink.push_trace(TraceEvent {
                        timestamp: self.time,
                        kind: Cow::Borrowed("vic.colour_write"),
                        payload: &payload,
                    })?;
                    prev_border = border;
                }

                let background = self.machine.vic_register(0x21) & 0x0F;
                if background != prev_background {
                    let payload = serde_json::to_vec(&json!({
                        "register": "d021",
                        "value": background,
                        "raster_line": self.machine.raster_line(),
                        "cycle_in_line": self.machine.cycle_in_line(),
                        "cpu_pc": self.machine.cpu().regs.pc,
                    }))
                    .map_err(|reason| MachineError::Host {
                        reason: format!("failed to encode trace payload: {reason}"),
                    })?;
                    host.trace_sink.push_trace(TraceEvent {
                        timestamp: self.time,
                        kind: Cow::Borrowed("vic.colour_write"),
                        payload: &payload,
                    })?;
                    prev_background = background;
                }
            }

            if let (Some((start, end)), Some(drive8)) =
                (self.trace_drive_rom_window, self.drive8.as_ref())
            {
                let pc = drive8.cpu().regs.pc;
                if drive8.cpu().sync && (start..=end).contains(&pc) {
                    let state = self.drive_trace_state(drive8);
                    if self.last_drive_trace_state.as_ref() != Some(&state) {
                        let payload =
                            serde_json::to_vec(&state).map_err(|reason| MachineError::Host {
                                reason: format!("failed to encode drive trace payload: {reason}"),
                            })?;
                        host.trace_sink.push_trace(TraceEvent {
                            timestamp: self.time,
                            kind: Cow::Borrowed("drive8.rom_trace"),
                            payload: &payload,
                        })?;
                        self.last_drive_trace_state = Some(state);
                    }
                } else {
                    self.last_drive_trace_state = None;
                }
            }

            if frame_complete {
                self.emit_frame(host)?;
                let audio = self.machine.take_audio_buffer();
                if !audio.is_empty() {
                    host.audio_sink.push_audio(AudioPacket {
                        timestamp: self.time,
                        sample_rate: self.machine.audio_sample_rate(),
                        channels: 1,
                        samples: &audio,
                    })?;
                }
            }
        }

        let _ = &host.trace_sink;

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match command {
            ControlCommand::MediaTransport(command) => {
                if command.slot.as_ref() != "tape-1" {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: command.slot.as_ref().to_owned(),
                    });
                }

                match command.action {
                    MediaTransportAction::Start => self.machine.play_tape(),
                    MediaTransportAction::Stop => self.machine.stop_tape(),
                    _ => {
                        return Err(MachineError::UnsupportedOperation {
                            operation: "media-transport",
                        });
                    }
                }

                Ok(())
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            }),
        }
    }

    // Eager machine (firmware-backed at construction) — the `direct` arm.
    emu198x_shell::debug_target_hooks!(direct);

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        Some(self)
    }
}

// 6502 debug target via the shared macro (`direct`: `machine: C64` is eager, not
// `Option`). Disassembles through the Asm198x spec crate. See
// 198x/decisions/rung1-wiring.md.
emu198x_shell::impl_6502_debug_primitives!(C64Runtime, direct);

// Keyboard description for the shared `press_key` / `type_string` tools. The
// key-name validation + char→keystroke table already live in `crate::input`
// (and back the BASIC loader's RUN step); this just surfaces them plus the
// C64's keyboard-scan timing.
impl emu198x_shell::KeyboardTarget for C64Runtime {
    fn key_name_is_valid(&self, name: &str) -> bool {
        crate::input::key_name_is_valid(name)
    }

    fn key_names_hint(&self) -> &'static str {
        "A-Z, 0-9, Space, Return, Delete, F1/F3/F5/F7, cursor Up/Down/Left/Right, \
         LShift, RShift, Ctrl, Commodore, RunStop, Restore"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        crate::input::keys_for_char(ch).map(|keys| keys.into_iter().map(str::to_owned).collect())
    }

    fn key_timing(&self) -> emu198x_shell::KeyTiming {
        emu198x_shell::KeyTiming {
            default_hold_frames: crate::DEFAULT_KEY_HOLD_FRAMES,
            max_hold_frames: crate::MAX_KEY_HOLD_FRAMES,
            // press_key settles 1 frame after release; type_string runs 2
            // (`INTER_CHAR_FRAMES`) between characters and has no extra
            // repeated-key settle (the 2-frame gap already separates them).
            press_settle_frames: 1,
            inter_key_settle_frames: 2,
            repeat_settle_frames: 0,
            default_type_settle_frames: crate::DEFAULT_TYPE_SETTLE_FRAMES,
        }
    }
}

fn build_machine(
    model: Model,
    kernal_rom: &[u8],
    basic_rom: &[u8],
    character_rom: &[u8],
) -> Result<C64, MachineError> {
    C64::new(C64Config {
        model: model.to_machine_model(),
        kernal_rom,
        basic_rom,
        character_rom,
    })
    .map_err(|reason| MachineError::InvalidRequest {
        reason: reason.to_string(),
    })
}

fn build_drive(
    drive8_dos_rom: &Option<Vec<u8>>,
    iec_bus: &mut IecBus,
) -> Result<Option<Drive1541>, MachineError> {
    let Some(rom) = drive8_dos_rom.as_deref() else {
        return Ok(None);
    };

    let mut drive = Drive1541::new(Drive1541Config { dos_rom: rom }).map_err(|reason| {
        MachineError::InvalidFirmware {
            id: "commodore-1541-dos-rom".to_owned(),
            reason: reason.to_string(),
        }
    })?;
    drive.sync_iec_bus(iec_bus);
    Ok(Some(drive))
}

fn validate_rom_size(id: &'static str, bytes: &[u8], expected: usize) -> Result<(), MachineError> {
    if bytes.len() == expected {
        return Ok(());
    }

    Err(MachineError::InvalidFirmware {
        id: id.to_owned(),
        reason: format!("expected exactly {expected} bytes, got {}", bytes.len()),
    })
}

fn repack_rgba8888(argb_pixels: &[u32], rgba: &mut Vec<u8>) {
    let required_len = argb_pixels.len() * 4;
    if rgba.len() != required_len {
        rgba.resize(required_len, 0);
    }

    for (index, pixel) in argb_pixels.iter().copied().enumerate() {
        let base = index * 4;
        rgba[base] = ((pixel >> 16) & 0xFF) as u8;
        rgba[base + 1] = ((pixel >> 8) & 0xFF) as u8;
        rgba[base + 2] = (pixel & 0xFF) as u8;
        rgba[base + 3] = ((pixel >> 24) & 0xFF) as u8;
    }
}
