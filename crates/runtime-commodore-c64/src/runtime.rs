//! Runtime wrapper for the fresh-workspace Commodore 64.

use std::borrow::Cow;

use common_commodore_iec::IecBus;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, InputEvent,
    MachineCore, MachineError, MachineProfile, MachineTime, MediaKind, MediaSet,
    MediaTransportAction, QueryError, QueryResult, ResetKind, RunResult, SessionQueryProvider,
    StopReason, TraceEvent,
};
use machine_commodore_1541::{Drive1541, Drive1541Config, Drive1541Snapshot, DRIVE1541_CPU_HZ};
use machine_commodore_c64::{C64Config, C64Model, C64Snapshot, C64};
use serde::Serialize;
use serde_json::json;

use crate::{profile_for, Model};

const C64_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.row",
    "boot.reason",
    "boot.offset",
    "c64.cpu.a",
    "c64.cpu.addr",
    "c64.cpu.data",
    "c64.cpu.irq",
    "c64.cpu.instruction_complete",
    "c64.cpu.nmi",
    "c64.cpu.p",
    "c64.cpu.pc",
    "c64.cpu.rdy",
    "c64.cpu.rw",
    "c64.cpu.sp",
    "c64.cpu.sync",
    "c64.cpu.total_cycles",
    "c64.cpu.x",
    "c64.cpu.y",
    "c64.cia1.flag",
    "c64.cia1.irq",
    "c64.cia1.icr_mask",
    "c64.cia1.icr_status",
    "c64.cia1.timer_a",
    "c64.cia1.timer_a_latch",
    "c64.cia1.timer_b",
    "c64.cia1.timer_b_latch",
    "c64.cia2.irq",
    "c64.cia2.pa",
    "c64.cia2.pb",
    "c64.cia2.port_a_latch",
    "c64.cia2.port_b_latch",
    "c64.cia2.ddra",
    "c64.cia2.ddrb",
    "c64.cia2.port_a_drive_state",
    "c64.cia2.port_b_drive_state",
    "c64.cia2.cra",
    "c64.cia2.crb",
    "c64.cia2.icr_mask",
    "c64.cia2.icr_status",
    "c64.cia2.timer_a",
    "c64.cia2.timer_a_latch",
    "c64.cia2.timer_b",
    "c64.cia2.timer_b_latch",
    "c64.drive8.attached",
    "c64.drive8.cpu.addr",
    "c64.drive8.cpu.cycles",
    "c64.drive8.cpu.data",
    "c64.drive8.cpu.instruction_complete",
    "c64.drive8.cpu.p",
    "c64.drive8.cpu.pc",
    "c64.drive8.cpu.rw",
    "c64.drive8.cpu.sp",
    "c64.drive8.cpu.sync",
    "c64.drive8.cpu.x",
    "c64.drive8.cpu.y",
    "c64.drive8.via1.irq",
    "c64.drive8.via1.ca1",
    "c64.drive8.via1.pa",
    "c64.drive8.via1.pb",
    "c64.drive8.via1.ora",
    "c64.drive8.via1.orb",
    "c64.drive8.via1.ddra",
    "c64.drive8.via1.ddrb",
    "c64.drive8.via1.acr",
    "c64.drive8.via1.pcr",
    "c64.drive8.via1.t1_counter",
    "c64.drive8.via1.t1_latch",
    "c64.drive8.via2.irq",
    "c64.drive8.via2.ca1",
    "c64.drive8.via2.pa",
    "c64.drive8.via2.pb",
    "c64.drive8.via2.ora",
    "c64.drive8.via2.orb",
    "c64.drive8.via2.ddra",
    "c64.drive8.via2.ddrb",
    "c64.drive8.via2.acr",
    "c64.drive8.via2.pcr",
    "c64.drive8.gcr_read",
    "c64.drive8.byte_ready",
    "c64.drive8.byte_ready_events",
    "c64.drive8.sync_detected",
    "c64.drive8.sync_events",
    "c64.drive8.motor_on",
    "c64.drive8.activity_led",
    "c64.drive8.head_position",
    "c64.drive8.density_code",
    "c64.drive8.disk.inserted",
    "c64.drive8.disk.name",
    "c64.drive8.disk.id",
    "c64.drive8.disk.write_protected",
    "c64.drive8.disk.directory",
    "c64.drive8.trace.recent_writes",
    "c64.drive8.mem.<hex16>",
    "c64.iec.cpu_port",
    "c64.iec.drive_port",
    "c64.memory.effective_port",
    "c64.memory.io_visible",
    "c64.memory.port_data",
    "c64.memory.port_ddr",
    "c64.machine.cycle_in_line",
    "c64.machine.frame_count",
    "c64.memory.ram.<hex16>",
    "c64.machine.raster_line",
    "c64.tape.loaded",
    "c64.tape.motor_on",
    "c64.tape.pulse_count",
    "c64.tape.pulse_index",
    "c64.tape.playing",
    "c64.tape.sense",
    "c64.vic.background_colour",
    "c64.vic.ba_low",
    "c64.vic.border_colour",
    "c64.vic.irq",
    "screen.text.lines",
];

const READY_SCREEN_CODES: [u8; 6] = [18, 5, 1, 4, 25, 46];
const KERNAL_ROM_SIZE: usize = 0x2000;
const BASIC_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;
const DOS1541_ROM_SIZE: usize = 0x4000;
const SCREEN_RAM_BASE: u16 = 0x0400;
const SCREEN_TEXT_WIDTH: usize = 40;
const SCREEN_TEXT_HEIGHT: usize = 25;

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

#[derive(serde::Serialize, serde::Deserialize)]
struct SnapshotEnvelopeV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
    drive8: Option<Drive1541Snapshot>,
    iec_bus: IecBus,
    drive8_cycle_accum: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct C64BootStatus {
    detected: bool,
    reason: String,
    offset: Option<u16>,
    row: Option<u64>,
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

/// C64-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct C64SessionQueryProvider;

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

    #[must_use]
    pub fn drive8(&self) -> Option<&Drive1541> {
        self.drive8.as_ref()
    }

    /// Returns the current runtime time in `phi2` cycles.
    #[must_use]
    pub const fn time(&self) -> MachineTime {
        self.time
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
                    drive.load_d64_bytes(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: image.slot.as_ref().to_owned(),
                            reason: reason.to_string(),
                        }
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
                        kind: Cow::Borrowed("c64.vic.colour_write"),
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
                        kind: Cow::Borrowed("c64.vic.colour_write"),
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
                            kind: Cow::Borrowed("c64.drive8.rom_trace"),
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
        postcard::to_allocvec(&SnapshotEnvelopeV1 {
            version: 1,
            profile_id: self.profile.profile_id.as_str().to_owned(),
            time: self.time,
            machine: self.machine.snapshot_state(),
            drive8: self.drive8.as_ref().map(Drive1541::snapshot_state),
            iec_bus: self.iec_bus.clone(),
            drive8_cycle_accum: self.drive8_cycle_accum,
        })
        .map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("encode failed: {reason}"),
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let snapshot: SnapshotEnvelopeV1 =
            postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
                reason: format!("decode failed: {reason}"),
            })?;

        if snapshot.version != 1 {
            return Err(MachineError::InvalidSnapshot {
                reason: format!("unsupported snapshot version {}", snapshot.version),
            });
        }

        if snapshot.profile_id != self.profile.profile_id.as_str() {
            return Err(MachineError::InvalidSnapshot {
                reason: format!(
                    "snapshot profile {} does not match runtime profile {}",
                    snapshot.profile_id,
                    self.profile.profile_id.as_str()
                ),
            });
        }

        self.machine
            .restore_snapshot_state(snapshot.machine)
            .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
        self.drive8 = snapshot
            .drive8
            .map(Drive1541::from_snapshot)
            .transpose()
            .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
        self.iec_bus = snapshot.iec_bus;
        self.drive8_cycle_accum = snapshot.drive8_cycle_accum;
        self.time = snapshot.time;
        Ok(())
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
}

impl SessionQueryProvider<C64Runtime> for C64SessionQueryProvider {
    fn query_paths(&self, _machine: &C64Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = C64_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &C64Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let boot = c64_boot_status(machine.machine());

        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.row" => json!(boot.row),
            "boot.reason" => json!(boot.reason),
            "boot.offset" => json!(boot.offset),
            "c64.cpu.a" => json!(machine.machine().cpu().regs.a),
            "c64.cpu.addr" => json!(machine.machine().cpu().addr),
            "c64.cpu.data" => json!(machine.machine().cpu().data),
            "c64.cpu.irq" => json!(machine.machine().cpu().irq),
            "c64.cpu.instruction_complete" => {
                json!(machine.machine().cpu().instruction_complete())
            }
            "c64.cpu.nmi" => json!(machine.machine().cpu().nmi),
            "c64.cpu.p" => json!(machine.machine().cpu().regs.p),
            "c64.cpu.pc" => json!(machine.machine().cpu().regs.pc),
            "c64.cpu.rdy" => json!(machine.machine().cpu().rdy),
            "c64.cpu.rw" => json!(machine.machine().cpu().rw),
            "c64.cpu.sp" => json!(machine.machine().cpu().regs.sp),
            "c64.cpu.sync" => json!(machine.machine().cpu().sync),
            "c64.cpu.total_cycles" => json!(machine.machine().cpu().total_cycles),
            "c64.cpu.x" => json!(machine.machine().cpu().regs.x),
            "c64.cpu.y" => json!(machine.machine().cpu().regs.y),
            "c64.cia1.flag" => json!(machine.machine().cia1().flag),
            "c64.cia1.icr_mask" => json!(machine.machine().cia1().icr_mask()),
            "c64.cia1.icr_status" => json!(machine.machine().cia1().icr_status()),
            "c64.cia1.timer_a" => json!(machine.machine().cia1().timer_a()),
            "c64.cia1.timer_a_latch" => json!(machine.machine().cia1().timer_a_latch()),
            "c64.cia1.timer_b" => json!(machine.machine().cia1().timer_b()),
            "c64.cia1.timer_b_latch" => json!(machine.machine().cia1().timer_b_latch()),
            "c64.cia2.cra" => json!(machine.machine().cia2().cra()),
            "c64.cia2.crb" => json!(machine.machine().cia2().crb()),
            "c64.cia2.icr_mask" => json!(machine.machine().cia2().icr_mask()),
            "c64.cia2.icr_status" => json!(machine.machine().cia2().icr_status()),
            "c64.cia2.pa" => json!(machine.machine().cia2().pa),
            "c64.cia2.pb" => json!(machine.machine().cia2().pb),
            "c64.cia2.port_a_latch" => json!(machine.machine().cia2().port_a_latch()),
            "c64.cia2.port_b_latch" => json!(machine.machine().cia2().port_b_latch()),
            "c64.cia2.ddra" => json!(machine.machine().cia2().ddr_a()),
            "c64.cia2.ddrb" => json!(machine.machine().cia2().ddr_b()),
            "c64.cia2.port_a_drive_state" => {
                json!(machine.machine().cia2().port_a_drive_state())
            }
            "c64.cia2.port_b_drive_state" => {
                json!(machine.machine().cia2().port_b_drive_state())
            }
            "c64.cia2.timer_a" => json!(machine.machine().cia2().timer_a()),
            "c64.cia2.timer_a_latch" => json!(machine.machine().cia2().timer_a_latch()),
            "c64.cia2.timer_b" => json!(machine.machine().cia2().timer_b()),
            "c64.cia2.timer_b_latch" => json!(machine.machine().cia2().timer_b_latch()),
            "c64.drive8.attached" => json!(machine.drive8().is_some()),
            "c64.drive8.cpu.addr" => json!(machine.drive8().map(|drive| drive.cpu().addr)),
            "c64.drive8.cpu.cycles" => json!(machine.drive8().map(|drive| drive.cycles())),
            "c64.drive8.cpu.data" => json!(machine.drive8().map(|drive| drive.cpu().data)),
            "c64.drive8.cpu.instruction_complete" => {
                json!(machine
                    .drive8()
                    .map(|drive| drive.cpu().instruction_complete()))
            }
            "c64.drive8.cpu.p" => json!(machine.drive8().map(|drive| drive.cpu().regs.p)),
            "c64.drive8.cpu.pc" => json!(machine.drive8().map(|drive| drive.cpu().regs.pc)),
            "c64.drive8.cpu.rw" => json!(machine.drive8().map(|drive| drive.cpu().rw)),
            "c64.drive8.cpu.sp" => json!(machine.drive8().map(|drive| drive.cpu().regs.sp)),
            "c64.drive8.cpu.sync" => json!(machine.drive8().map(|drive| drive.cpu().sync)),
            "c64.drive8.cpu.x" => json!(machine.drive8().map(|drive| drive.cpu().regs.x)),
            "c64.drive8.cpu.y" => json!(machine.drive8().map(|drive| drive.cpu().regs.y)),
            "c64.drive8.via1.irq" => json!(machine.drive8().map(|drive| drive.via1().irq)),
            "c64.drive8.via1.ca1" => json!(machine.drive8().map(|drive| drive.via1().ca1)),
            "c64.drive8.via1.pa" => json!(machine.drive8().map(|drive| drive.via1().pa)),
            "c64.drive8.via1.pb" => json!(machine.drive8().map(|drive| drive.via1().pb)),
            "c64.drive8.via1.ora" => json!(machine.drive8().map(|drive| drive.via1().ora())),
            "c64.drive8.via1.orb" => json!(machine.drive8().map(|drive| drive.via1().orb())),
            "c64.drive8.via1.ddra" => {
                json!(machine.drive8().map(|drive| drive.via1().ddra()))
            }
            "c64.drive8.via1.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via1().ddrb()))
            }
            "c64.drive8.via1.acr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0B))),
            "c64.drive8.via1.pcr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0C))),
            "c64.drive8.via1.t1_counter" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x04)) | (u16::from(drive.via1().peek(0x05)) << 8)
            })),
            "c64.drive8.via1.t1_latch" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x06)) | (u16::from(drive.via1().peek(0x07)) << 8)
            })),
            "c64.drive8.via2.irq" => json!(machine.drive8().map(|drive| drive.via2().irq)),
            "c64.drive8.via2.ca1" => json!(machine.drive8().map(|drive| drive.via2().ca1)),
            "c64.drive8.via2.pa" => json!(machine.drive8().map(|drive| drive.via2().pa)),
            "c64.drive8.via2.pb" => json!(machine.drive8().map(|drive| drive.via2().pb)),
            "c64.drive8.via2.ora" => json!(machine.drive8().map(|drive| drive.via2().ora())),
            "c64.drive8.via2.orb" => json!(machine.drive8().map(|drive| drive.via2().orb())),
            "c64.drive8.via2.ddra" => {
                json!(machine.drive8().map(|drive| drive.via2().ddra()))
            }
            "c64.drive8.via2.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via2().ddrb()))
            }
            "c64.drive8.via2.acr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0B))),
            "c64.drive8.via2.pcr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0C))),
            "c64.drive8.gcr_read" => json!(machine.drive8().map(|drive| drive.gcr_read())),
            "c64.drive8.byte_ready" => json!(machine.drive8().map(|drive| drive.byte_ready())),
            "c64.drive8.byte_ready_events" => {
                json!(machine.drive8().map(|drive| drive.byte_ready_event_count()))
            }
            "c64.drive8.sync_detected" => {
                json!(machine.drive8().map(|drive| drive.sync_detected()))
            }
            "c64.drive8.sync_events" => {
                json!(machine.drive8().map(|drive| drive.sync_event_count()))
            }
            "c64.drive8.motor_on" => json!(machine.drive8().map(|drive| drive.motor_on())),
            "c64.drive8.activity_led" => {
                json!(machine.drive8().map(|drive| drive.activity_led()))
            }
            "c64.drive8.head_position" => {
                json!(machine.drive8().map(|drive| drive.head_position()))
            }
            "c64.drive8.density_code" => {
                json!(machine.drive8().map(|drive| drive.density_code()))
            }
            "c64.drive8.disk.inserted" => {
                json!(machine.drive8().is_some_and(|drive| drive.disk_inserted()))
            }
            "c64.drive8.disk.name" => json!(machine
                .drive8()
                .and_then(|drive| drive.disk())
                .map(|disk| disk.disk_name())),
            "c64.drive8.disk.id" => json!(machine
                .drive8()
                .and_then(|drive| drive.disk())
                .map(|disk| disk.disk_id())),
            "c64.drive8.disk.write_protected" => json!(machine
                .drive8()
                .and_then(|drive| drive.disk())
                .map(|disk| disk.write_protected())),
            "c64.drive8.disk.directory" => json!(machine
                .drive8()
                .and_then(|drive| drive.disk())
                .map(|disk| disk.directory_entries())),
            "c64.drive8.trace.recent_writes" => {
                json!(machine.drive8().map(|drive| drive.recent_io_writes()))
            }
            "c64.iec.cpu_port" => json!(machine.iec_bus.cpu_port()),
            "c64.iec.drive_port" => json!(machine.iec_bus.drive_port()),
            "c64.memory.effective_port" => json!(machine.machine().memory().effective_port()),
            "c64.memory.io_visible" => json!(machine.machine().memory().is_io_visible()),
            "c64.memory.port_data" => json!(machine.machine().memory().port_data()),
            "c64.memory.port_ddr" => json!(machine.machine().memory().port_ddr()),
            "c64.machine.raster_line" => json!(machine.machine().raster_line()),
            "c64.machine.cycle_in_line" => json!(machine.machine().cycle_in_line()),
            "c64.machine.frame_count" => json!(machine.machine().frame_count()),
            "c64.tape.loaded" => json!(machine.machine().tape_is_loaded()),
            "c64.tape.motor_on" => json!(machine.machine().tape_motor_on()),
            "c64.tape.pulse_count" => json!(machine.machine().tape_pulse_count()),
            "c64.tape.pulse_index" => json!(machine.machine().tape_pulse_index()),
            "c64.tape.playing" => json!(machine.machine().tape_is_playing()),
            "c64.tape.sense" => json!(machine.machine().tape_sense_active()),
            "c64.vic.background_colour" => json!(machine.machine().vic_register(0x21) & 0x0F),
            "c64.vic.ba_low" => json!(machine.machine().vic().ba_is_low()),
            "c64.vic.border_colour" => json!(machine.machine().vic_register(0x20) & 0x0F),
            "c64.vic.irq" => json!(machine.machine().vic().irq_active()),
            "c64.cia1.irq" => json!(machine.machine().cia1().irq_active()),
            "c64.cia2.irq" => json!(machine.machine().cia2().irq_active()),
            "screen.text.lines" => json!(decode_screen_text_lines(machine.machine())),
            _ if path.starts_with("c64.memory.ram.") => {
                let suffix = &path["c64.memory.ram.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(machine.machine().memory().ram_read(addr))
            }
            _ if path.starts_with("c64.drive8.mem.") => {
                let suffix = &path["c64.drive8.mem.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(machine
                    .drive8()
                    .map(|drive| drive.peek_with_iec_bus(addr, &machine.iec_bus)))
            }
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(trimmed, 16).ok()
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

fn c64_boot_status(machine: &C64) -> C64BootStatus {
    let end = 0x07E8u16 - 0x0400u16 - READY_SCREEN_CODES.len() as u16;
    for offset in 0..=end {
        let mut matched = true;
        for (index, expected) in READY_SCREEN_CODES.iter().copied().enumerate() {
            if machine.memory().ram_read(0x0400 + offset + index as u16) != expected {
                matched = false;
                break;
            }
        }

        if matched {
            let row = u64::from(offset / SCREEN_TEXT_WIDTH as u16);
            return C64BootStatus {
                detected: true,
                reason: format!("found READY. screen codes at offset ${offset:04X} on row {row}"),
                offset: Some(offset),
                row: Some(row),
            };
        }
    }

    C64BootStatus {
        detected: false,
        reason: "READY. screen codes not visible".to_owned(),
        offset: None,
        row: None,
    }
}

fn decode_screen_text_lines(machine: &C64) -> Vec<String> {
    let mut lines = Vec::with_capacity(SCREEN_TEXT_HEIGHT);
    for row in 0..SCREEN_TEXT_HEIGHT {
        let mut line = String::with_capacity(SCREEN_TEXT_WIDTH);
        for col in 0..SCREEN_TEXT_WIDTH {
            let address = SCREEN_RAM_BASE + (row * SCREEN_TEXT_WIDTH + col) as u16;
            let code = machine.memory().ram_read(address);
            line.push(decode_screen_code(code));
        }
        lines.push(line);
    }
    lines
}

fn decode_screen_code(code: u8) -> char {
    match code {
        0x00 => '@',
        0x01..=0x1A => char::from(b'A' + (code - 1)),
        0x20 => ' ',
        0x21..=0x3F => char::from(code),
        0x40..=0x5A => char::from(code),
        0x5B => '[',
        0x5C => '\\',
        0x5D => ']',
        0x5E => '^',
        0x5F => '_',
        0x60 => '`',
        0x61..=0x7A => char::from(code - 0x20),
        _ => '?',
    }
}

fn apply_input_event(machine: &mut C64, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, col)) = c64_key_position(name.as_ref()) {
                machine.keyboard_mut().set_key(row, col, *pressed);
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            let _ = machine.set_joystick_control(*port, name.as_ref(), *pressed);
        }
        _ => {}
    }
}

fn c64_key_position(name: &str) -> Option<(u8, u8)> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "DELETE" | "DEL" | "BACKSPACE" => Some((0, 0)),
        "RETURN" | "ENTER" => Some((0, 1)),
        "RIGHT" | "CRSRRIGHT" => Some((0, 2)),
        "F7" => Some((0, 3)),
        "F1" => Some((0, 4)),
        "F3" => Some((0, 5)),
        "F5" => Some((0, 6)),
        "DOWN" | "CRSRDOWN" => Some((0, 7)),
        "3" => Some((1, 0)),
        "W" => Some((1, 1)),
        "A" => Some((1, 2)),
        "4" => Some((1, 3)),
        "Z" => Some((1, 4)),
        "S" => Some((1, 5)),
        "E" => Some((1, 6)),
        "LSHIFT" => Some((1, 7)),
        "5" => Some((2, 0)),
        "R" => Some((2, 1)),
        "D" => Some((2, 2)),
        "6" => Some((2, 3)),
        "C" => Some((2, 4)),
        "F" => Some((2, 5)),
        "T" => Some((2, 6)),
        "X" => Some((2, 7)),
        "7" => Some((3, 0)),
        "Y" => Some((3, 1)),
        "G" => Some((3, 2)),
        "8" => Some((3, 3)),
        "B" => Some((3, 4)),
        "H" => Some((3, 5)),
        "U" => Some((3, 6)),
        "V" => Some((3, 7)),
        "9" => Some((4, 0)),
        "I" => Some((4, 1)),
        "J" => Some((4, 2)),
        "0" => Some((4, 3)),
        "M" => Some((4, 4)),
        "K" => Some((4, 5)),
        "O" => Some((4, 6)),
        "N" => Some((4, 7)),
        "PLUS" => Some((5, 0)),
        "P" => Some((5, 1)),
        "L" => Some((5, 2)),
        "MINUS" => Some((5, 3)),
        "." | "PERIOD" => Some((5, 4)),
        ":" | "COLON" => Some((5, 5)),
        "@" | "AT" => Some((5, 6)),
        "," | "COMMA" => Some((5, 7)),
        "POUND" | "STERLING" => Some((6, 0)),
        "ASTERISK" | "STAR" => Some((6, 1)),
        "SEMICOLON" => Some((6, 2)),
        "HOME" => Some((6, 3)),
        "RSHIFT" => Some((6, 4)),
        "=" | "EQUALS" | "EQUAL" => Some((6, 5)),
        "UP" | "CRSRUP" => Some((6, 6)),
        "/" | "SLASH" => Some((6, 7)),
        "1" => Some((7, 0)),
        "LEFTARROW" => Some((7, 1)),
        "CTRL" | "CONTROL" => Some((7, 2)),
        "2" => Some((7, 3)),
        "SPACE" => Some((7, 4)),
        "COMMODORE" | "CBM" => Some((7, 5)),
        "Q" => Some((7, 6)),
        "RUNSTOP" | "RUN/STOP" => Some((7, 7)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        autoload_basic_disk, autoload_basic_tape, DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    };
    use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
    use emu198x_shell::{
        read_media_asset, AudioPacket, AudioSink, ControlCommand, FirmwareImage, FirmwareSet,
        FrameSink, HeadlessSession, MediaImage, MediaKind, MediaSet, MediaTransportAction,
        MediaTransportCommand, NullAudioSink, NullTraceSink, PixelFormat,
    };
    use std::fs;
    use std::path::PathBuf;

    #[derive(Default)]
    struct FrameCollector {
        count: usize,
        last_timestamp: MachineTime,
        last_width: u32,
        last_height: u32,
        last_format: Option<PixelFormat>,
    }

    #[derive(Default)]
    struct AudioCollector {
        count: usize,
        last_timestamp: MachineTime,
        last_sample_rate: u32,
        last_channels: u8,
        last_samples_len: usize,
    }

    impl FrameSink for FrameCollector {
        fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
            self.count += 1;
            self.last_timestamp = frame.timestamp;
            self.last_width = frame.width;
            self.last_height = frame.height;
            self.last_format = Some(frame.format);
            Ok(())
        }
    }

    impl AudioSink for AudioCollector {
        fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
            self.count += 1;
            self.last_timestamp = packet.timestamp;
            self.last_sample_rate = packet.sample_rate;
            self.last_channels = packet.channels;
            self.last_samples_len = packet.samples.len();
            Ok(())
        }
    }

    fn blank_firmware() -> FirmwareSet<'static> {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "commodore-c64-kernal-rom",
            &[0; KERNAL_ROM_SIZE],
        ));
        firmware.push(FirmwareImage::new(
            "commodore-c64-basic-rom",
            &[0; BASIC_ROM_SIZE],
        ));
        firmware.push(FirmwareImage::new(
            "commodore-c64-character-rom",
            &[0; CHARACTER_ROM_SIZE],
        ));
        firmware
    }

    fn stub_drive_rom_bytes() -> &'static [u8] {
        let mut rom = vec![0xEA; DOS1541_ROM_SIZE];
        let vector = DOS1541_ROM_SIZE - 4;
        rom[vector] = 0x00;
        rom[vector + 1] = 0xC0;
        Box::leak(rom.into_boxed_slice())
    }

    fn blank_firmware_with_drive() -> FirmwareSet<'static> {
        let mut firmware = blank_firmware();
        firmware.push(FirmwareImage::new(
            "commodore-1541-dos-rom",
            stub_drive_rom_bytes(),
        ));
        firmware
    }

    fn make_tap(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 20];
        bytes[..12].copy_from_slice(b"C64-TAPE-RAW");
        bytes[12] = 1;
        bytes[13] = 0;
        bytes[14] = 0;
        bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn d64_linear_sector_index(track: u8, sector_num: u8) -> usize {
        const TRACK_SECTOR_COUNTS: [u8; 35] = [
            21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19,
            19, 19, 18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17,
        ];
        TRACK_SECTOR_COUNTS[..usize::from(track - 1)]
            .iter()
            .map(|&count| usize::from(count))
            .sum::<usize>()
            + usize::from(sector_num)
    }

    fn write_d64_sector(bytes: &mut [u8], track: u8, sector_num: u8, sector: &[u8; 256]) {
        let offset = d64_linear_sector_index(track, sector_num) * 256;
        bytes[offset..offset + 256].copy_from_slice(sector);
    }

    fn make_d64() -> Vec<u8> {
        let mut bytes = vec![0u8; 174_848];

        let mut bam = [0u8; 256];
        bam[0] = 18;
        bam[1] = 1;
        bam[0x90..0x98].copy_from_slice(b"DEMO DIS");
        bam[0x98] = b'K';
        bam[0xA2..0xA4].copy_from_slice(b"42");
        write_d64_sector(&mut bytes, 18, 0, &bam);

        let mut directory = [0u8; 256];
        directory[2] = 0x82;
        directory[3] = 1;
        directory[4] = 0;
        directory[5..10].copy_from_slice(b"HELLO");
        directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
        write_d64_sector(&mut bytes, 18, 1, &directory);

        let mut file_sector = [0u8; 256];
        file_sector[0] = 0;
        file_sector[1] = 6;
        file_sector[2..7].copy_from_slice(&[0x01, 0x08, 0x11, 0x22, 0x33]);
        write_d64_sector(&mut bytes, 1, 0, &file_sector);

        bytes
    }

    fn local_rom_firmware() -> FirmwareSet<'static> {
        let rom_dir = PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 ROM tests"),
        )
        .join(".emu198x/roms/commodore-c64");

        let kernal = Box::leak(
            fs::read(rom_dir.join("kernal.rom"))
                .expect("local C64 KERNAL ROM should exist")
                .into_boxed_slice(),
        );
        let basic = Box::leak(
            fs::read(rom_dir.join("basic.rom"))
                .expect("local C64 BASIC ROM should exist")
                .into_boxed_slice(),
        );
        let chargen = Box::leak(
            fs::read(rom_dir.join("chargen.rom"))
                .expect("local C64 chargen ROM should exist")
                .into_boxed_slice(),
        );

        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", kernal));
        firmware.push(FirmwareImage::new("commodore-c64-basic-rom", basic));
        firmware.push(FirmwareImage::new("commodore-c64-character-rom", chargen));
        firmware
    }

    fn local_rom_firmware_with_drive() -> FirmwareSet<'static> {
        let rom_dir = PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 ROM tests"),
        )
        .join(".emu198x/roms/commodore-c64");
        let drive = Box::leak(
            fs::read(rom_dir.join("1541.rom"))
                .expect("local 1541 DOS ROM should exist")
                .into_boxed_slice(),
        );

        let mut firmware = local_rom_firmware();
        firmware.push(FirmwareImage::new("commodore-1541-dos-rom", drive));
        firmware
    }

    fn local_thinker_tap_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 TAP tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Educational/[TAP]/Thinker, The (1984)(Atlantis).zip",
        )
    }

    fn local_thomas_tap_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 TAP tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Educational/[TAP]/Thomas the Tank Engine (1990)(Alternative Software).zip",
        )
    }

    fn local_thing_on_a_spring_tap_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 TAP tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Games/Arcade/[TAP]/Thing on a Spring (1985)(Gremlin).zip",
        )
    }

    fn local_ghostbusters_tap_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 TAP tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Games/Arcade/[TAP]/Ghostbusters (1984)(Activision).zip",
        )
    }

    fn local_bruce_lee_d64_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 D64 tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Games/Arcade/[D64]/Bruce Lee (1984)(Datasoft).zip",
        )
    }

    fn local_aztec_challenge_d64_zip() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for local C64 D64 tests"),
        )
        .join(
            "Projects/Emu198x-Unclean/Reference/commodore/c64/Games/Arcade/[D64]/Aztec Challenge (1983)(Cosmi).zip",
        )
    }

    fn screen_text_lines(
        session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    ) -> Vec<String> {
        let result = session
            .query("screen.text.lines")
            .expect("screen.text.lines query should succeed");
        let lines = result
            .value
            .as_array()
            .expect("screen.text.lines should be an array");
        lines
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("screen.text.lines entries should be strings")
                    .to_owned()
            })
            .collect()
    }

    fn wait_for_screen_line_contains(
        session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
        row: usize,
        needle: &str,
        max_frames: u32,
    ) {
        for _ in 0..max_frames {
            if screen_text_lines(session)
                .get(row)
                .is_some_and(|line| line.contains(needle))
            {
                return;
            }
            session
                .run_frames(1)
                .expect("screen-line wait should be able to run one frame");
        }

        panic!("screen row {row} did not contain {needle:?} within {max_frames} frames");
    }

    fn press_key(
        session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
        key: &str,
        held_frames: u32,
    ) {
        session.queue_input(InputEvent::Key {
            name: key.to_ascii_lowercase().into(),
            pressed: true,
        });
        session
            .run_frames(held_frames)
            .expect("key press should advance the runtime");
        session.queue_input(InputEvent::Key {
            name: key.to_ascii_lowercase().into(),
            pressed: false,
        });
    }

    fn press_button(
        session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
        port: u8,
        name: &str,
        held_frames: u32,
    ) {
        session.queue_input(InputEvent::Button {
            port,
            name: name.to_ascii_lowercase().into(),
            pressed: true,
        });
        session
            .run_frames(held_frames)
            .expect("button press should advance the runtime");
        session.queue_input(InputEvent::Button {
            port,
            name: name.to_ascii_lowercase().into(),
            pressed: false,
        });
    }

    #[test]
    fn runtime_can_build_from_declared_firmware() {
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware());
        assert!(runtime.is_ok(), "blank C64 firmware set should construct");
    }

    #[test]
    fn runtime_can_attach_optional_drive_rom() {
        let runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
                .expect("blank C64 firmware with optional drive ROM should construct");
        let provider = C64SessionQueryProvider;

        assert!(runtime.drive8().is_some());
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.attached")
                .expect("drive attachment query should not fail")
                .expect("drive attachment query should resolve")
                .value,
            json!(true)
        );
    }

    #[test]
    fn runtime_run_until_emits_rgba_frame() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = AudioCollector::default();
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame));

        let result = runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("blank C64 runtime should run one frame");

        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        assert_eq!(result.reached, target);
        assert_eq!(frame_sink.count, 1);
        assert_eq!(frame_sink.last_timestamp, target);
        assert_eq!(frame_sink.last_width, 416);
        assert_eq!(frame_sink.last_height, 312);
        assert_eq!(frame_sink.last_format, Some(PixelFormat::Rgba8888));
        assert_eq!(audio_sink.count, 1);
        assert_eq!(audio_sink.last_timestamp, target);
        assert_eq!(
            audio_sink.last_sample_rate,
            runtime.machine().audio_sample_rate()
        );
        assert_eq!(audio_sink.last_channels, 1);
        assert!(audio_sink.last_samples_len > 0);
    }

    #[test]
    fn runtime_run_until_advances_attached_drive_cycles() {
        let mut runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
                .expect("blank C64 firmware with drive should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = AudioCollector::default();
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(64);

        runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("runtime with an attached drive should run");

        let drive = runtime.drive8().expect("drive should stay attached");
        assert!(drive.cycles() > 0);
        assert!(drive.cpu().regs.pc >= 0xC000);
    }

    #[test]
    fn query_provider_reports_blank_runtime_as_not_booted() {
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let provider = C64SessionQueryProvider;

        let paths = provider.query_paths(&runtime, Some("boot."));
        assert_eq!(
            paths,
            vec![
                "boot.detected".to_owned(),
                "boot.offset".to_owned(),
                "boot.reason".to_owned(),
                "boot.row".to_owned(),
            ]
        );

        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("boot.detected query should not fail")
            .expect("boot.detected should resolve");
        assert_eq!(detected.value, json!(false));

        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("boot.reason query should not fail")
            .expect("boot.reason should resolve");
        assert_eq!(reason.value, json!("READY. screen codes not visible"));

        let row = provider
            .query(&runtime, "boot.row")
            .expect("boot.row query should not fail")
            .expect("boot.row should resolve");
        assert_eq!(row.value, json!(null));

        let tape_loaded = provider
            .query(&runtime, "c64.tape.loaded")
            .expect("c64.tape.loaded query should not fail")
            .expect("c64.tape.loaded should resolve");
        assert_eq!(tape_loaded.value, json!(false));

        let text_lines = provider
            .query(&runtime, "screen.text.lines")
            .expect("screen.text.lines query should not fail")
            .expect("screen.text.lines should resolve");
        let lines = text_lines
            .value
            .as_array()
            .expect("screen.text.lines should be an array");
        assert_eq!(lines.len(), SCREEN_TEXT_HEIGHT);
        assert!(lines
            .iter()
            .all(|line| line.as_str().is_some_and(|line| line.len() == 40)));

        assert!(matches!(provider.query(&runtime, "not-a-path"), Ok(None)));
    }

    #[test]
    fn runtime_load_media_and_transport_update_tape_queries() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let tape = make_tap(&[0x01, 0x01]);
        let provider = C64SessionQueryProvider;
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &tape));

        runtime
            .load_media(&media)
            .expect("synthetic TAP should load through runtime");
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.loaded")
                .expect("c64.tape.loaded query should not fail")
                .expect("c64.tape.loaded should resolve")
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.playing")
                .expect("c64.tape.playing query should not fail")
                .expect("c64.tape.playing should resolve")
                .value,
            json!(false)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.sense")
                .expect("c64.tape.sense query should not fail")
                .expect("c64.tape.sense should resolve")
                .value,
            json!(false)
        );

        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("tape transport should start");
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.playing")
                .expect("c64.tape.playing query should not fail")
                .expect("c64.tape.playing should resolve")
                .value,
            json!(false)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.sense")
                .expect("c64.tape.sense query should not fail")
                .expect("c64.tape.sense should resolve")
                .value,
            json!(true)
        );

        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Stop,
            )))
            .expect("tape transport should stop");
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.playing")
                .expect("c64.tape.playing query should not fail")
                .expect("c64.tape.playing should resolve")
                .value,
            json!(false)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.tape.sense")
                .expect("c64.tape.sense query should not fail")
                .expect("c64.tape.sense should resolve")
                .value,
            json!(false)
        );
    }

    #[test]
    fn runtime_rejects_drive_media_without_attached_1541() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let disk = make_d64();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk));

        let err = runtime
            .load_media(&media)
            .expect_err("drive-8 should require an attached 1541 ROM");
        assert!(matches!(
            err,
            MachineError::MissingFirmware { ref id } if id == "commodore-1541-dos-rom"
        ));
    }

    #[test]
    fn runtime_load_media_mounts_d64_into_attached_drive() {
        let mut runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
                .expect("blank C64 firmware with drive should construct a runtime");
        let provider = C64SessionQueryProvider;
        let disk = make_d64();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk));

        runtime
            .load_media(&media)
            .expect("synthetic D64 should mount into the attached 1541");

        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.inserted")
                .expect("disk inserted query should not fail")
                .expect("disk inserted query should resolve")
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.name")
                .expect("disk name query should not fail")
                .expect("disk name query should resolve")
                .value,
            json!("DEMO DISK")
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.id")
                .expect("disk id query should not fail")
                .expect("disk id query should resolve")
                .value,
            json!("42")
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.write_protected")
                .expect("disk write-protect query should not fail")
                .expect("disk write-protect query should resolve")
                .value,
            json!(true)
        );
        let directory = provider
            .query(&runtime, "c64.drive8.disk.directory")
            .expect("disk directory query should not fail")
            .expect("disk directory query should resolve")
            .value;
        let entries = directory
            .as_array()
            .expect("disk directory should be an array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["name"], json!("HELLO"));
        assert_eq!(entries[0]["file_type"], json!("PRG"));
        assert_eq!(entries[0]["blocks"], json!(1));
    }

    #[test]
    fn input_mapping_covers_native_shell_keys() {
        assert_eq!(c64_key_position("delete"), Some((0, 0)));
        assert_eq!(c64_key_position("right"), Some((0, 2)));
        assert_eq!(c64_key_position("down"), Some((0, 7)));
        assert_eq!(c64_key_position("f1"), Some((0, 4)));
        assert_eq!(c64_key_position("f7"), Some((0, 3)));
        assert_eq!(c64_key_position("plus"), Some((5, 0)));
        assert_eq!(c64_key_position("home"), Some((6, 3)));
        assert_eq!(c64_key_position("equals"), Some((6, 5)));
        assert_eq!(c64_key_position("up"), Some((6, 6)));
        assert_eq!(c64_key_position("commodore"), Some((7, 5)));
        assert_eq!(c64_key_position("runstop"), Some((7, 7)));
    }

    #[test]
    fn snapshot_round_trip_preserves_mid_cycle_runtime_state() {
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
            .expect("blank C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(3);

        runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("blank C64 runtime should run a few cycles");

        let snapshot = runtime
            .snapshot()
            .expect("blank C64 runtime should snapshot");
        let mut expected_machine = runtime.machine.clone();
        let expected_time = runtime.time();

        let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
        restored
            .restore(&snapshot)
            .expect("snapshot restore should succeed");

        assert_eq!(restored.time(), expected_time);
        assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
        assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
        assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
        assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
        assert_eq!(
            restored.machine().raster_line(),
            expected_machine.raster_line()
        );
        assert_eq!(
            restored.machine().cycle_in_line(),
            expected_machine.cycle_in_line()
        );
        assert_eq!(
            restored.machine().framebuffer(),
            expected_machine.framebuffer()
        );

        for _ in 0..8 {
            let expected_frame_complete = expected_machine.tick();
            let restored_frame_complete = restored.machine_mut().tick();
            assert_eq!(restored_frame_complete, expected_frame_complete);
            assert_eq!(restored.machine().cpu().regs, expected_machine.cpu().regs);
            assert_eq!(restored.machine().cpu().addr, expected_machine.cpu().addr);
            assert_eq!(restored.machine().cpu().rw, expected_machine.cpu().rw);
            assert_eq!(restored.machine().cpu().sync, expected_machine.cpu().sync);
            assert_eq!(
                restored.machine().cpu().total_cycles,
                expected_machine.cpu().total_cycles
            );
            assert_eq!(
                restored.machine().raster_line(),
                expected_machine.raster_line()
            );
            assert_eq!(
                restored.machine().cycle_in_line(),
                expected_machine.cycle_in_line()
            );
            assert_eq!(restored.machine().vic().irq, expected_machine.vic().irq);
            assert_eq!(
                restored.machine().vic().ba_low,
                expected_machine.vic().ba_low
            );
            assert_eq!(
                restored.machine().framebuffer(),
                expected_machine.framebuffer()
            );
        }
    }

    #[test]
    fn snapshot_round_trip_preserves_attached_drive_state() {
        let mut runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
                .expect("blank C64 firmware with drive should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;

        runtime
            .run_until(
                MachineTime::new(64),
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("runtime with an attached drive should run");

        let expected_cycles = runtime
            .drive8()
            .expect("drive should be attached before snapshot")
            .cycles();
        let expected_pc = runtime
            .drive8()
            .expect("drive should be attached before snapshot")
            .cpu()
            .regs
            .pc;

        let snapshot = runtime.snapshot().expect("runtime should snapshot");
        let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
        restored
            .restore(&snapshot)
            .expect("snapshot restore should succeed");

        let drive = restored
            .drive8()
            .expect("drive should restore from snapshot");
        assert_eq!(drive.cycles(), expected_cycles);
        assert_eq!(drive.cpu().regs.pc, expected_pc);
    }

    #[test]
    #[ignore = "requires local C64 and 1541 ROMs at ~/.emu198x/roms/commodore-c64"]
    fn query_provider_reports_real_attached_drive_progress() {
        let mut runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
                .expect("local ROMs should construct a C64 runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let provider = C64SessionQueryProvider;

        runtime
            .run_until(
                MachineTime::new(512),
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("real ROM-backed runtime should run");

        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.attached")
                .expect("drive attachment query should not fail")
                .expect("drive attachment query should resolve")
                .value,
            json!(true)
        );
        let drive_cycles = provider
            .query(&runtime, "c64.drive8.cpu.cycles")
            .expect("drive cycle query should not fail")
            .expect("drive cycle query should resolve")
            .value
            .as_u64()
            .expect("drive cycles should be a u64");
        assert!(drive_cycles > 0);
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_mount_bruce_lee_reports_disk_metadata() {
        let mut runtime =
            C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
                .expect("local ROMs should construct a C64 runtime");
        let provider = C64SessionQueryProvider;
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk.bytes));

        runtime
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.attached")
                .expect("drive attachment query should not fail")
                .expect("drive attachment query should resolve")
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.inserted")
                .expect("disk inserted query should not fail")
                .expect("disk inserted query should resolve")
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.name")
                .expect("disk name query should not fail")
                .expect("disk name query should resolve")
                .value,
            json!("BRUCELEE")
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.id")
                .expect("disk id query should not fail")
                .expect("disk id query should resolve")
                .value,
            json!("00")
        );
        assert_eq!(
            provider
                .query(&runtime, "c64.drive8.disk.write_protected")
                .expect("disk write-protect query should not fail")
                .expect("disk write-protect query should resolve")
                .value,
            json!(true)
        );
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_autoload_bruce_lee_starts_drive_motion() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        let autoload = autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");
        assert_eq!(autoload.slot, DEFAULT_DISK_AUTOLOAD_SLOT);

        let start_head = session
            .query("c64.drive8.head_position")
            .expect("head position query should not fail")
            .value
            .as_u64()
            .expect("head position should be numeric");

        session
            .run_frames(2_000)
            .expect("Bruce Lee disk autoload should advance the attached drive");

        let end_head = session
            .query("c64.drive8.head_position")
            .expect("head position query should not fail")
            .value
            .as_u64()
            .expect("head position should stay numeric");

        assert!(
            end_head != start_head,
            "Bruce Lee disk autoload should move the 1541 head after SEARCHING FOR: start={start_head} end={end_head}"
        );
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_autoload_bruce_lee_reaches_loading() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");

        let loading = session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
            .expect("Bruce Lee disk autoload should reach LOADING");
        assert_eq!(loading.needle, "LOADING");
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_autoload_bruce_lee_starts_after_run() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");

        session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
            .expect("Bruce Lee disk autoload should reach LOADING");
        session
            .run_frames(2_400)
            .expect("Bruce Lee should return to BASIC after the initial disk stage");

        let ready_frame = session.machine().machine().framebuffer().to_vec();
        let ready_lines = screen_text_lines(&session);
        assert!(
            ready_lines[10].contains("READY."),
            "Bruce Lee should return to BASIC before RUN: {:?}",
            ready_lines[10]
        );
        assert!(!session
            .machine()
            .drive8()
            .expect("drive should stay attached")
            .motor_on());

        for key in ["r", "u", "n", "return"] {
            press_key(&mut session, key, 3);
        }

        session
            .run_frames(1_800)
            .expect("Bruce Lee should reach its title screen after RUN");

        let title_lines = screen_text_lines(&session);
        assert_eq!(
            title_lines[0], "????\"QQQQ?????Q1????R????&???L??\"\"R1\"\"\"F",
            "Bruce Lee should replace the BASIC screen with title-screen data"
        );
        assert_eq!(
            title_lines[1], "DDDDDDDD????R????&???L??'QQQQQQQQQQQQQQZ",
            "Bruce Lee should show the stable title-screen top rows after RUN"
        );
        assert_ne!(
            session.machine().machine().framebuffer(),
            ready_frame.as_slice(),
            "Bruce Lee framebuffer should change after RUN starts the title"
        );
        assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 14);
        assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 0);

        let drive = session
            .machine()
            .drive8()
            .expect("drive should stay attached");
        assert!(drive.motor_on());
        assert!(drive.activity_led());
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_autoload_bruce_lee_advances_after_fire() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");

        session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
            .expect("Bruce Lee disk autoload should reach LOADING");
        session
            .run_frames(2_400)
            .expect("Bruce Lee should return to BASIC after the initial disk stage");

        for key in ["r", "u", "n", "return"] {
            press_key(&mut session, key, 3);
        }

        session
            .run_frames(16_000)
            .expect("Bruce Lee should reach its stable title screen after RUN");

        let title_frame = session.machine().machine().framebuffer().to_vec();
        let title_lines = screen_text_lines(&session);
        assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 6);
        assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 6);
        assert!(!session
            .machine()
            .drive8()
            .expect("drive should stay attached")
            .motor_on());

        press_button(&mut session, 2, "fire", 6);
        session
            .run_frames(3_000)
            .expect("Bruce Lee should advance beyond the title after joystick fire");

        let post_fire_lines = screen_text_lines(&session);
        assert_ne!(
            session.machine().machine().framebuffer(),
            title_frame.as_slice(),
            "Bruce Lee framebuffer should change after joystick fire"
        );
        assert_ne!(
            post_fire_lines, title_lines,
            "Bruce Lee screen codes should change after joystick fire"
        );
        assert_eq!(
            post_fire_lines[0], "X?????Q??I?Q???C?CL?D?@?@??P??P???????O?",
            "Bruce Lee should reach the stable post-title scene after joystick fire"
        );
        assert_eq!(
            post_fire_lines[24], "@????????@?????C??G? ??P??A?@??8?X????X?",
            "Bruce Lee should keep the expected lower HUD row after joystick fire"
        );
        assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
        assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 12);

        let drive = session
            .machine()
            .drive8()
            .expect("drive should stay attached");
        assert!(!drive.motor_on());
        assert!(!drive.activity_led());
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Bruce Lee D64 archive"]
    fn real_d64_autoload_bruce_lee_responds_to_joystick_right_after_fire() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_bruce_lee_d64_zip(), MediaKind::Disk)
            .expect("local Bruce Lee D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Bruce Lee D64 should mount into drive-8");

        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");

        session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
            .expect("Bruce Lee disk autoload should reach LOADING");
        session
            .run_frames(2_400)
            .expect("Bruce Lee should return to BASIC after the initial disk stage");

        for key in ["r", "u", "n", "return"] {
            press_key(&mut session, key, 3);
        }

        session
            .run_frames(16_000)
            .expect("Bruce Lee should reach its stable title screen after RUN");

        press_button(&mut session, 2, "fire", 6);
        session
            .run_frames(3_000)
            .expect("Bruce Lee should advance beyond the title after joystick fire");

        let post_fire_frame = session.machine().machine().framebuffer().to_vec();
        let post_fire_lines = screen_text_lines(&session);
        assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
        assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 12);

        press_button(&mut session, 2, "right", 30);
        session
            .run_frames(300)
            .expect("Bruce Lee should keep running after joystick-right input");

        assert_eq!(
            screen_text_lines(&session),
            post_fire_lines,
            "Bruce Lee keeps the same screen-code overlay while the gameplay scene animates"
        );
        assert_ne!(
            session.machine().machine().framebuffer(),
            post_fire_frame.as_slice(),
            "Bruce Lee framebuffer should respond to joystick-right after the post-title scene starts"
        );

        let drive = session
            .machine()
            .drive8()
            .expect("drive should stay attached");
        assert!(!drive.motor_on());
        assert!(!drive.activity_led());
    }

    #[test]
    #[ignore = "requires local C64 ROMs, 1541 ROM, and Aztec Challenge D64 archive"]
    fn real_d64_autoload_aztec_challenge_reaches_instruction_screen() {
        let firmware = local_rom_firmware_with_drive();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("local ROMs should construct a C64 runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let disk = read_media_asset(&local_aztec_challenge_d64_zip(), MediaKind::Disk)
            .expect("local Aztec Challenge D64 archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_DISK_AUTOLOAD_SLOT,
            MediaKind::Disk,
            &disk.bytes,
        ));
        session
            .load_media(&media)
            .expect("Aztec Challenge D64 should mount into drive-8");

        autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("disk autoload should reach SEARCHING FOR");

        session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
            .expect("Aztec Challenge disk autoload should reach LOADING");
        session
            .run_frames(4_000)
            .expect("Aztec Challenge should return to BASIC after the initial disk stage");

        let ready_lines = screen_text_lines(&session);
        assert!(
            ready_lines[10].contains("READY."),
            "Aztec Challenge should return to BASIC before RUN: {:?}",
            ready_lines[10]
        );

        for key in ["r", "u", "n", "return"] {
            press_key(&mut session, key, 3);
        }

        session
            .run_frames(5_000)
            .expect("Aztec Challenge should reach the player-select screen after RUN");
        press_key(&mut session, "f1", 3);
        session
            .run_frames(2_000)
            .expect("Aztec Challenge should reach its instruction screen after F1");

        let lines = screen_text_lines(&session);
        assert_eq!(
            lines[3], "  PLAYER 1                  PLAYER 2    ",
            "Aztec Challenge should show the player headers on its instruction screen"
        );
        assert_eq!(
            lines[17], "            THE GAUNTLET                ",
            "Aztec Challenge should identify the first phase after F1"
        );
        assert_eq!(
            lines[24], "      PRESS FIRE BUTTON TO START        ",
            "Aztec Challenge should show the readable start prompt after F1"
        );

        let drive = session
            .machine()
            .drive8()
            .expect("drive should stay attached");
        assert!(!drive.motor_on());
        assert!(!drive.activity_led());
        assert_eq!(session.machine().machine().vic_register(0x20) & 0x0F, 0);
        assert_eq!(session.machine().machine().vic_register(0x21) & 0x0F, 0);
    }

    #[test]
    #[ignore = "requires local C64 ROMs at ~/.emu198x/roms/commodore-c64"]
    fn query_provider_detects_ready_on_real_pal_boot() {
        let firmware = local_rom_firmware();
        let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut frame_sink = FrameCollector::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame) * 200);

        runtime
            .run_until(
                target,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("real PAL C64 ROMs should run to boot window");

        let provider = C64SessionQueryProvider;
        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("boot.detected query should not fail")
            .expect("boot.detected should resolve");
        assert_eq!(detected.value, json!(true));

        let offset = provider
            .query(&runtime, "boot.offset")
            .expect("boot.offset query should not fail")
            .expect("boot.offset should resolve");
        assert_ne!(offset.value, json!(null));

        let row = provider
            .query(&runtime, "boot.row")
            .expect("boot.row query should not fail")
            .expect("boot.row should resolve");
        assert_ne!(row.value, json!(null));
    }

    #[test]
    #[ignore = "requires local C64 ROMs and Thinker TAP archive"]
    fn real_tap_autoload_reaches_post_load_ready() {
        let firmware = local_rom_firmware();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let tape = read_media_asset(&local_thinker_tap_zip(), MediaKind::Tape)
            .expect("local Thinker TAP archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session
            .load_media(&media)
            .expect("local Thinker TAP should insert");

        let autoload = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
        assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

        let found = session
            .wait_for_query_text_contains("screen.text.lines", "FOUND THINKER", 1500)
            .expect("Thinker tape should reach FOUND banner");
        assert_eq!(found.line, Some(12));

        let loading = session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 3000)
            .expect("Thinker tape should reach LOADING banner");
        assert_eq!(loading.line, Some(13));

        wait_for_screen_line_contains(&mut session, 14, "READY.", 5000);
        let lines = screen_text_lines(&session);
        assert!(
            lines[12].contains("FOUND THINKER"),
            "post-load screen should retain FOUND banner: {:?}",
            lines[12]
        );
        assert!(
            lines[13].contains("LOADING"),
            "post-load screen should retain LOADING banner: {:?}",
            lines[13]
        );
        assert!(
            lines[14].contains("READY."),
            "post-load screen should reach READY. line: {:?}",
            lines[14]
        );
        assert!(session.machine().machine().tape_is_playing());
    }

    #[test]
    #[ignore = "requires local C64 ROMs and Thomas the Tank Engine TAP archive"]
    fn real_tap_autoload_reaches_thomas_loading_ready_banner() {
        let firmware = local_rom_firmware();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let tape = read_media_asset(&local_thomas_tap_zip(), MediaKind::Tape)
            .expect("local Thomas TAP archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session
            .load_media(&media)
            .expect("local Thomas TAP should insert");

        let autoload = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
        assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

        let found = session
            .wait_for_query_text_contains("screen.text.lines", "FOUND THOMAS", 1500)
            .expect("Thomas tape should reach FOUND banner");
        assert_eq!(found.line, Some(12));

        let loading = session
            .wait_for_query_text_contains("screen.text.lines", "LOADING", 3000)
            .expect("Thomas tape should reach LOADING banner");
        assert_eq!(loading.line, Some(13));

        wait_for_screen_line_contains(&mut session, 14, "READY.", 3000);
        let lines = screen_text_lines(&session);
        assert!(
            lines[12].contains("FOUND THOMAS"),
            "Thomas screen should retain FOUND banner: {:?}",
            lines[12]
        );
        assert!(
            lines[13].contains("LOADING"),
            "Thomas screen should retain LOADING banner: {:?}",
            lines[13]
        );
        assert!(
            lines[14].contains("READY."),
            "Thomas screen should reach READY. line: {:?}",
            lines[14]
        );
        assert!(session.machine().machine().tape_is_playing());
    }

    #[test]
    #[ignore = "requires local C64 ROMs and Ghostbusters TAP archive"]
    fn real_tap_autoload_ghostbusters_reaches_later_loader_state() {
        let firmware = local_rom_firmware();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let tape = read_media_asset(&local_ghostbusters_tap_zip(), MediaKind::Tape)
            .expect("local Ghostbusters TAP archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session
            .load_media(&media)
            .expect("local Ghostbusters TAP should insert");

        let autoload = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
        assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

        let found = session
            .wait_for_query_text_contains("screen.text.lines", "FOUND MAIN", 1500)
            .expect("Ghostbusters tape should reach FOUND MAIN banner");
        assert_eq!(found.line, Some(12));

        session
            .run_frames(25_000)
            .expect("Ghostbusters loader should run past the first-stage banner");

        let lines = screen_text_lines(&session);
        assert!(
            !lines.iter().any(|line| line.contains("FOUND MAIN")),
            "Ghostbusters should move past FOUND MAIN: {:?}",
            lines
        );
        assert!(
            !lines.iter().any(|line| line.contains("LOADING")),
            "Ghostbusters should move past LOADING banner: {:?}",
            lines
        );

        let machine = session.machine().machine();
        assert!(
            machine.memory().is_io_visible(),
            "Ghostbusters loader should keep CIA/VIC I/O visible in the later state"
        );
        assert_eq!(
            machine.cia2().timer_a_latch(),
            280,
            "Ghostbusters later loader should have programmed CIA2 Timer A"
        );
        assert!(!machine.tape_is_playing());
        assert!(!machine.tape_motor_on());
        assert!(
            machine.tape_pulse_index() > 460_000,
            "Ghostbusters should consume almost the entire TAP before the later state"
        );
    }

    #[test]
    #[ignore = "requires local C64 ROMs and Thing on a Spring TAP archive"]
    fn real_tap_autoload_thing_on_a_spring_reaches_menu() {
        let firmware = local_rom_firmware();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let tape = read_media_asset(&local_thing_on_a_spring_tap_zip(), MediaKind::Tape)
            .expect("local Thing on a Spring TAP archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session
            .load_media(&media)
            .expect("local Thing on a Spring TAP should insert");

        let autoload = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("autoload should reach PRESS PLAY ON TAPE and start transport");
        assert_eq!(autoload.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);

        session
            .run_frames(25_000)
            .expect("Thing on a Spring should reach its post-load menu state");

        let lines = screen_text_lines(&session);
        assert!(
            lines[16].contains("600-MICRO"),
            "Thing on a Spring should show the score table: {:?}",
            lines[16]
        );
        assert!(
            lines[17].contains("500-PROJECTS"),
            "Thing on a Spring should show the score table: {:?}",
            lines[17]
        );
        assert!(
            lines[20].contains("200-GREMLIN"),
            "Thing on a Spring should show the publisher line: {:?}",
            lines[20]
        );
        assert!(
            lines[17].contains("RIGHT - X"),
            "Thing on a Spring should show the control legend: {:?}",
            lines[17]
        );
        assert!(
            lines[20].contains("FIRE  - SPACE"),
            "Thing on a Spring should show the fire control: {:?}",
            lines[20]
        );

        let machine = session.machine().machine();
        assert!(!machine.tape_is_playing());
        assert_eq!(
            machine.tape_pulse_index(),
            machine.tape_pulse_count(),
            "Thing on a Spring should consume the full TAP by the menu state"
        );
    }

    #[test]
    #[ignore = "requires local C64 ROMs and Thing on a Spring TAP archive"]
    fn real_tap_autoload_thing_on_a_spring_starts_after_space() {
        let firmware = local_rom_firmware();
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("real PAL C64 firmware should construct a runtime");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
            C64SessionQueryProvider,
        );
        let tape = read_media_asset(&local_thing_on_a_spring_tap_zip(), MediaKind::Tape)
            .expect("local Thing on a Spring TAP archive should load");
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaKind::Tape,
            &tape.bytes,
        ));
        session
            .load_media(&media)
            .expect("local Thing on a Spring TAP should insert");

        autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect("autoload should reach PRESS PLAY ON TAPE and start transport");

        session
            .run_frames(25_000)
            .expect("Thing on a Spring should reach its post-load menu state");

        let menu_lines = screen_text_lines(&session);
        assert!(
            menu_lines[16].contains("600-MICRO"),
            "Thing on a Spring should show the score table before start: {:?}",
            menu_lines[16]
        );
        let menu_frame = session.machine().machine().framebuffer().to_vec();

        session.queue_input(InputEvent::Key {
            name: "space".into(),
            pressed: true,
        });
        session
            .run_frames(3)
            .expect("Thing on a Spring should advance with SPACE held");
        session.queue_input(InputEvent::Key {
            name: "space".into(),
            pressed: false,
        });
        session
            .run_frames(480)
            .expect("Thing on a Spring should settle into its started state");

        let started_lines = screen_text_lines(&session);
        assert_eq!(
            started_lines[0], " @A!!!!!!!!!!!!DE  JKLMN  @A!!!!!!!!!DE ",
            "Thing on a Spring should replace the menu banner after SPACE"
        );
        assert_eq!(
            started_lines[8], " HI############LM QRSTUVW HI#########LM ",
            "Thing on a Spring should reach its stable started screen after SPACE"
        );
        assert!(
            !started_lines[16].contains("600-MICRO"),
            "Thing on a Spring should leave the score table after SPACE: {:?}",
            started_lines[16]
        );
        assert_ne!(
            session.machine().machine().framebuffer(),
            menu_frame.as_slice(),
            "Thing on a Spring framebuffer should change after SPACE starts the title"
        );

        let machine = session.machine().machine();
        assert!(!machine.tape_is_playing());
        assert_eq!(
            machine.tape_pulse_index(),
            machine.tape_pulse_count(),
            "Thing on a Spring should still have consumed the full TAP after SPACE"
        );
    }
}
