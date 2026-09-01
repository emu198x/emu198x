//! Runtime wrapper for the fresh-workspace Commodore 64.

use std::borrow::Cow;

use common_commodore_iec::IecBus;
use emu198x_esp_at_modem::EspAtTcpBridge;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    ResetKind, RunResult, StopReason, TraceEvent,
};
use machine_commodore_1541::Drive1541;
use machine_commodore_1581::Drive1581;
use machine_commodore_c64::{AudioControls, C64, C64Config, C64Model, SidChannel};
use serde::Serialize;
use serde_json::json;

use crate::drives::{DriveKind, IecDrive, IecDriveSnapshot};
use crate::input::apply_input_event;
use crate::snapshot;
use crate::{Model, profile_for};
use emu198x_shell::display::Display;

const KERNAL_ROM_SIZE: usize = 0x2000;
const BASIC_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;
const DOS1541_ROM_SIZE: usize = 0x4000;
const DOS1571_ROM_SIZE: usize = 0x8000;
const DOS1581_ROM_SIZE: usize = 0x8000;
/// The lowest IEC device number on the C64 serial bus. Device 8 is port 0.
const FIRST_IEC_DEVICE: u8 = 8;
/// Number of IEC ports the runtime models (devices 8–11).
const IEC_PORT_COUNT: usize = 4;
/// The 1541 defaults to device 8 (port 0).
const DRIVE_1541_DEVICE_NUMBER: u8 = 8;
/// The 1581 coexists with the 1541 by taking IEC device 9 (jumper 1).
const DRIVE_1581_DEVICE_NUMBER: u8 = 9;
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
    /// Retained DOS ROMs, one slot per drive model, so `set_port_drive` can
    /// build any model on demand and a reset rebuilds every configured port.
    dos_rom_1541: Option<Vec<u8>>,
    dos_rom_1571: Option<Vec<u8>>,
    dos_rom_1581: Option<Vec<u8>>,
    /// The drive on each IEC device 8–11 (index = device − 8), or `None` for an
    /// empty port. The user chooses the model per port via `set_port_drive`; the
    /// default layout is a 1541 on device 8 and, when its ROM is present, a 1581
    /// on device 9, so both can be on the bus at once.
    drives: [Option<IecDrive>; IEC_PORT_COUNT],
    /// Per-port cycle-accumulator phase, carried across snapshots.
    drive_cycle_accum: [u64; IEC_PORT_COUNT],
    /// Raw `.crt` image of the inserted cartridge, retained so a reset (which
    /// rebuilds the machine from ROMs) can re-insert it, matching hardware where
    /// the cartridge stays in the port across a reset.
    cartridge_image: Option<Vec<u8>>,
    /// Attached GeoRAM size in KiB, retained so a reset re-attaches the unit
    /// (the expansion stays plugged in across a reset).
    georam_kb: Option<usize>,
    /// Attached REU size in KiB, retained across a reset like `georam_kb`.
    reu_kb: Option<usize>,
    /// Control port a 1351 mouse is plugged into (1 or 2), retained across a
    /// reset so the mouse stays plugged in — and so host pointer events know
    /// which port to drive.
    mouse_1351_port: Option<u8>,
    iec_bus: IecBus,
    rgba_framebuffer: Vec<u8>,
    /// ESP-AT modem bridged to a real TCP socket, hanging off the user port
    /// (CIA #2 PA2 = pin M, computer TX; PB0 = pin C, computer RX). `None`
    /// leaves the port's pull-ups idling both lines high, as with nothing
    /// plugged in.
    esp_at_tcp_bridge: Option<EspAtTcpBridge>,
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
            Self::C64cPal => C64Model::PalC64c,
            Self::C64cNtsc => C64Model::NtscC64c,
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
        // Default port layout: a 1541 on device 8 when its ROM is present.
        let drive8 = match drive8_dos_rom.as_deref() {
            Some(rom) => Some(
                IecDrive::build(
                    DriveKind::C1541,
                    rom,
                    DRIVE_1541_DEVICE_NUMBER,
                    &mut iec_bus,
                )
                .map_err(|reason| MachineError::InvalidFirmware {
                    id: "commodore-1541-dos-rom".to_owned(),
                    reason,
                })?,
            ),
            None => None,
        };
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
            dos_rom_1541: drive8_dos_rom,
            dos_rom_1571: None,
            dos_rom_1581: None,
            drives: [drive8, None, None, None],
            drive_cycle_accum: [0; IEC_PORT_COUNT],
            cartridge_image: None,
            georam_kb: None,
            reu_kb: None,
            mouse_1351_port: None,
            iec_bus,
            rgba_framebuffer,
            esp_at_tcp_bridge: None,
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
        let dos_rom_1571 = firmware.bytes("commodore-1571-dos-rom").map(<[u8]>::to_vec);
        let dos_rom_1581 = firmware.bytes("commodore-1581-dos-rom").map(<[u8]>::to_vec);

        let mut runtime = Self::new(
            model,
            kernal.to_vec(),
            basic.to_vec(),
            character.to_vec(),
            drive8_dos_rom,
        )?;
        // Retain the 1571 ROM so the user can select it on any port later; the
        // default layout does not place a 1571 anywhere.
        if let Some(rom) = dos_rom_1571 {
            validate_rom_size("commodore-1571-dos-rom", &rom, DOS1571_ROM_SIZE)?;
            runtime.dos_rom_1571 = Some(rom);
        }
        // The 1581 defaults onto device 9 so it coexists with the 1541 on
        // device 8; software addresses it as `,9`.
        if let Some(rom) = dos_rom_1581 {
            validate_rom_size("commodore-1581-dos-rom", &rom, DOS1581_ROM_SIZE)?;
            runtime.dos_rom_1581 = Some(rom);
            runtime.set_port_drive(DRIVE_1581_DEVICE_NUMBER, Some(DriveKind::C1581))?;
        }
        Ok(runtime)
    }

    /// Sets — or clears, with `None` — the drive model on IEC device `device`
    /// (8–11), rebuilding it from the retained DOS ROM and assigning it that
    /// device number. This is the live per-port drive-type selector: a UI or MCP
    /// call swaps a port's drive without rebuilding the machine.
    ///
    /// # Errors
    ///
    /// Returns an error if `device` is outside 8–11, or if the DOS ROM for the
    /// requested model was not supplied at construction.
    pub fn set_port_drive(
        &mut self,
        device: u8,
        kind: Option<DriveKind>,
    ) -> Result<(), MachineError> {
        let port = port_index(device).ok_or_else(|| MachineError::InvalidRequest {
            reason: format!("IEC device {device} out of range (8-11)"),
        })?;
        self.drives[port] = match kind {
            Some(kind) => Some(self.make_port_drive(kind, device)?),
            None => {
                // Release the emptied port's lines, or its last low pull on
                // CLOCK/DATA stays folded into the shared bus and jams it.
                self.iec_bus.release_drive(device);
                None
            }
        };
        self.drive_cycle_accum[port] = 0;
        Ok(())
    }

    /// The drive model currently on IEC device `device` (8–11), or `None` for an
    /// empty or out-of-range port.
    #[must_use]
    pub fn port_drive_kind(&self, device: u8) -> Option<DriveKind> {
        let port = port_index(device)?;
        self.drives[port].as_ref().map(IecDrive::kind)
    }

    /// Whether a drive `kind` can be selected — i.e. its DOS ROM was supplied at
    /// construction, so [`Self::set_port_drive`] would accept it. A UI drive
    /// selector greys out the models this returns `false` for.
    #[must_use]
    pub fn drive_kind_available(&self, kind: DriveKind) -> bool {
        match kind {
            DriveKind::C1541 => self.dos_rom_1541.is_some(),
            DriveKind::C1571 => self.dos_rom_1571.is_some(),
            DriveKind::C1581 => self.dos_rom_1581.is_some(),
        }
    }

    /// Whether the drive on IEC device `device` (8–11) has a disk inserted,
    /// whatever the model. `None` when the port is empty or out of range. The
    /// disk-autoload paths use this instead of a model-specific accessor.
    #[must_use]
    pub fn port_disk_inserted(&self, device: u8) -> Option<bool> {
        let port = port_index(device)?;
        self.drives[port].as_ref().map(IecDrive::disk_inserted)
    }

    /// Builds one drive of `kind` for `device` from the retained DOS ROM,
    /// syncing it onto the IEC bus. Errors when that model's ROM is absent.
    fn make_port_drive(&mut self, kind: DriveKind, device: u8) -> Result<IecDrive, MachineError> {
        // Disjoint field borrows: the ROM slot is read immutably while the IEC
        // bus is borrowed mutably — direct field access keeps them independent.
        let rom = match kind {
            DriveKind::C1541 => self.dos_rom_1541.as_deref(),
            DriveKind::C1571 => self.dos_rom_1571.as_deref(),
            DriveKind::C1581 => self.dos_rom_1581.as_deref(),
        }
        .ok_or_else(|| MachineError::MissingFirmware {
            id: kind.dos_rom_id().to_owned(),
        })?;
        IecDrive::build(kind, rom, device, &mut self.iec_bus).map_err(|reason| {
            MachineError::InvalidFirmware {
                id: kind.dos_rom_id().to_owned(),
                reason,
            }
        })
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

    /// The 1541 on device 8, when port 0 holds one. Returns `None` if the port
    /// is empty or holds a different model; the `drive8.*` query surface and the
    /// disk-autoload path are 1541-shaped and read through here.
    #[must_use]
    pub fn drive8(&self) -> Option<&Drive1541> {
        self.drives[0].as_ref().and_then(IecDrive::as_1541)
    }

    /// The first attached 1581 drive, whichever port holds it. Defaults to
    /// device 9.
    #[must_use]
    pub fn drive_1581(&self) -> Option<&Drive1581> {
        self.drives.iter().flatten().find_map(IecDrive::as_1581)
    }

    /// The current 1581 disk image bytes, for a SAVE write-back.
    #[must_use]
    pub fn flush_drive_1581_image(&self) -> Option<Vec<u8>> {
        self.drive_1581()?.flush_image()
    }

    /// Decodes the device-8 drive's live surface back into an image so the host
    /// can persist a SAVE. Returns `None` when no drive or disk is present.
    /// See `knowledge/decisions/disk-save-write-back.md`.
    #[must_use]
    pub fn flush_drive8_image(&self) -> Option<Vec<u8>> {
        self.drives[0].as_ref()?.flush_image()
    }

    /// Flushes the recorded SAVE tape to `.tap` bytes for the writable work
    /// image, or `None` when no writable tape is mounted. Rides the same
    /// write-back model as [`Self::flush_drive8_image`].
    #[must_use]
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        self.machine.flush_tape_image()
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

    /// Per-port drive snapshots for the snapshot envelope.
    #[must_use]
    pub(crate) fn drives_snapshot(&self) -> [Option<IecDriveSnapshot>; IEC_PORT_COUNT] {
        std::array::from_fn(|port| self.drives[port].as_ref().map(IecDrive::snapshot))
    }

    /// Per-port cycle-accumulator phase for the snapshot envelope.
    #[must_use]
    pub(crate) fn drive_cycle_accum_all(&self) -> [u64; IEC_PORT_COUNT] {
        self.drive_cycle_accum
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Restores every port's drive from the snapshot envelope. Each drive
    /// snapshot is self-contained (it carries its own ROM and device number).
    pub(crate) fn restore_drives(
        &mut self,
        drives: [Option<IecDriveSnapshot>; IEC_PORT_COUNT],
    ) -> Result<(), String> {
        let mut restored: [Option<IecDrive>; IEC_PORT_COUNT] = [None, None, None, None];
        for (port, snapshot) in drives.into_iter().enumerate() {
            restored[port] = snapshot.map(IecDrive::from_snapshot).transpose()?;
        }
        self.drives = restored;
        Ok(())
    }

    pub(crate) fn set_iec_bus(&mut self, bus: IecBus) {
        self.iec_bus = bus;
    }

    pub(crate) fn set_drive_cycle_accum_all(&mut self, accum: [u64; IEC_PORT_COUNT]) {
        self.drive_cycle_accum = accum;
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
        // Rebuild every configured port from its retained ROM, preserving the
        // user's per-port model choice across the reset (the drives stay plugged
        // in, like hardware).
        let kinds: [Option<DriveKind>; IEC_PORT_COUNT] =
            std::array::from_fn(|port| self.drives[port].as_ref().map(IecDrive::kind));
        for (port, kind) in kinds.into_iter().enumerate() {
            let device = FIRST_IEC_DEVICE + port as u8;
            self.drives[port] = match kind {
                Some(kind) => Some(self.make_port_drive(kind, device)?),
                None => None,
            };
        }
        self.drive_cycle_accum = [0; IEC_PORT_COUNT];
        self.time = MachineTime::default();
        // The cartridge stays in the port across a reset, so re-insert it into
        // the freshly built machine before the KERNAL runs its cold start.
        if let Some(image) = self.cartridge_image.as_deref() {
            self.machine
                .insert_crt_bytes(image)
                .map_err(|reason| MachineError::InvalidMedia {
                    slot: "cartridge-1".to_owned(),
                    reason,
                })?;
        }
        // The GeoRAM expansion likewise stays plugged in across a reset.
        if let Some(size_kb) = self.georam_kb {
            self.machine.attach_georam(size_kb);
        }
        if let Some(size_kb) = self.reu_kb {
            self.machine.attach_reu(size_kb);
        }
        // A 1351 mouse stays plugged into its control port across a reset.
        if let Some(port) = self.mouse_1351_port {
            self.machine.attach_mouse_1351(port);
        }
        Ok(())
    }

    /// Attaches (`Some(size_kb)`) or detaches (`None`) a GeoRAM RAM expansion.
    /// The unit is retained so a later reset re-attaches it.
    pub fn set_georam(&mut self, size_kb: Option<usize>) {
        self.georam_kb = size_kb;
        match size_kb {
            Some(kb) => self.machine.attach_georam(kb),
            None => self.machine.detach_georam(),
        }
    }

    /// Attaches (`Some(size_kb)`) or detaches (`None`) a 17xx REU. Retained so a
    /// later reset re-attaches it.
    pub fn set_reu(&mut self, size_kb: Option<usize>) {
        self.reu_kb = size_kb;
        match size_kb {
            Some(kb) => self.machine.attach_reu(kb),
            None => self.machine.detach_reu(),
        }
    }

    /// Plugs a 1351 proportional mouse into control port `Some(1 | 2)`, or
    /// unplugs it with `None`. Retained so a later reset re-attaches it, and so
    /// host pointer events know which port to drive. An out-of-range port is
    /// ignored.
    pub fn set_mouse_1351(&mut self, port: Option<u8>) {
        if let Some(current) = self.mouse_1351_port {
            self.machine.detach_mouse_1351(current);
        }
        self.mouse_1351_port = match port {
            Some(p) if self.machine.attach_mouse_1351(p) => Some(p),
            _ => None,
        };
    }

    /// The control port a 1351 mouse is plugged into, if any. Host pointer
    /// events route here.
    #[must_use]
    pub fn mouse_1351_port(&self) -> Option<u8> {
        self.mouse_1351_port
    }

    /// Plug an ESP-AT modem into the user port, bridged to a real TCP
    /// transport. Connections open only once the emulated client sends
    /// `AT+CIPSTART`.
    ///
    /// `cycles_per_bit` is the modem's bit period in C64 `phi2` cycles, which
    /// differs by region because the external modem keeps real baud time while
    /// the CPU clock does not.
    pub fn attach_esp_at_tcp_bridge(&mut self, cycles_per_bit: u32, frame_size: usize) {
        self.esp_at_tcp_bridge = Some(EspAtTcpBridge::new(cycles_per_bit, frame_size));
    }

    /// The attached ESP-AT bridge, if any — for host-side diagnostics.
    #[must_use]
    pub fn esp_at_tcp_bridge(&self) -> Option<&EspAtTcpBridge> {
        self.esp_at_tcp_bridge.as_ref()
    }

    /// Unplug the ESP-AT modem, closing any open connection. The peripheral's
    /// query leaves stop being advertised, as they do for any absent hardware.
    pub fn detach_esp_at_tcp_bridge(&mut self) {
        self.esp_at_tcp_bridge = None;
        // Nothing is driving the line any more, so the port's pull-ups win.
        self.machine.set_user_port_pb0(true);
    }

    /// Attached GeoRAM size in KiB, if any. Retained across a reset.
    #[must_use]
    pub fn georam_kb(&self) -> Option<usize> {
        self.georam_kb
    }

    /// Attached REU size in KiB, if any. Retained across a reset.
    #[must_use]
    pub fn reu_kb(&self) -> Option<usize> {
        self.reu_kb
    }

    /// Raw `.crt` bytes of the inserted cartridge, if any. Retained across a
    /// reset (and captured into a snapshot so a restore survives the next one).
    pub(crate) fn cartridge_image_bytes(&self) -> Option<&[u8]> {
        self.cartridge_image.as_deref()
    }

    /// Restore the runtime-level expansion bookkeeping from a snapshot — the
    /// cartridge image and the GeoRAM/REU sizes and 1351-mouse port a reset
    /// rebuilds the machine from. Deliberately does NOT touch the machine: the
    /// machine's own snapshot already carries the live cartridge/expansion
    /// state, so re-attaching here would reinitialise the restored expansion
    /// RAM. Without this, a restored snapshot lost its cartridge and expansions
    /// on the next reset (the fields defaulted to `None`).
    pub(crate) fn restore_expansions(
        &mut self,
        cartridge_image: Option<Vec<u8>>,
        georam_kb: Option<usize>,
        reu_kb: Option<usize>,
        mouse_1351_port: Option<u8>,
    ) {
        self.cartridge_image = cartridge_image;
        self.georam_kb = georam_kb;
        self.reu_kb = reu_kb;
        self.mouse_1351_port = mouse_1351_port;
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

                    if image.writable {
                        // A writable tape is a blank SAVE work image; the KERNAL
                        // records onto it and it flushes to a `.tap` sidecar.
                        self.machine.insert_blank_writable_tape();
                    } else {
                        self.machine.load_tap_bytes(image.bytes).map_err(|reason| {
                            MachineError::InvalidMedia {
                                slot: image.slot.as_ref().to_owned(),
                                reason,
                            }
                        })?;
                    }
                }
                "cartridge-1" => {
                    if image.kind != MediaKind::Cartridge {
                        return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
                    }
                    self.machine
                        .insert_crt_bytes(image.bytes)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: image.slot.as_ref().to_owned(),
                            reason,
                        })?;
                    // Retain the image so a later reset re-inserts it, and so the
                    // KERNAL runs the cartridge cold-start on the next reset.
                    self.cartridge_image = Some(image.bytes.to_vec());
                }
                slot if drive_slot_device(slot).is_some() => {
                    if image.kind != MediaKind::Disk {
                        return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
                    }
                    let device = drive_slot_device(slot).expect("guarded above");
                    let port = port_index(device).expect("drive slot device is 8-11");
                    let drive = self.drives[port].as_mut().ok_or_else(|| {
                        MachineError::MissingFirmware {
                            id: format!("drive on IEC device {device}"),
                        }
                    })?;
                    drive
                        .load_disk(image.bytes, image.writable)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: image.slot.as_ref().to_owned(),
                            reason,
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

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        match slot {
            slot if drive_slot_device(slot).is_some() => {
                let device = drive_slot_device(slot).expect("guarded above");
                let port = port_index(device).expect("drive slot device is 8-11");
                let drive =
                    self.drives[port]
                        .as_mut()
                        .ok_or_else(|| MachineError::MissingFirmware {
                            id: format!("drive on IEC device {device}"),
                        })?;
                drive.eject_disk();
                Ok(())
            }
            "cartridge-1" => {
                self.cartridge_image = None;
                self.machine.remove_cartridge();
                Ok(())
            }
            // The datasette has no eject path on the C64 machine (it only
            // loads/plays/stops a tape), so tape eject stays unsupported until
            // an eject method exists on the core to surface here.
            "tape-1" => Err(MachineError::UnsupportedOperation {
                operation: "eject_media",
            }),
            _ => Err(MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            }),
        }
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
            let any_drive = self.drives.iter().any(Option::is_some);

            // Advance whichever of the C64 and the attached drives is furthest
            // behind in virtual time (cycles / clock-Hz), comparing by
            // cross-multiplication to stay integer-exact. A drive tick does not
            // advance the C64 clock, so it re-syncs the bus and `continue`s
            // without touching `self.time` or the per-frame trace below. Ties
            // favour the lowest port, then the C64 — matching the old
            // 1541-before-1581-before-C64 order.
            if any_drive {
                let mut best_next = u128::from(self.machine.phi2_cycles().saturating_add(1));
                let mut best_hz = u128::from(self.machine.timing().cpu_hz);
                // `None` = the C64 is furthest behind; `Some(port)` = that drive.
                let mut turn: Option<usize> = None;
                for (port, slot) in self.drives.iter().enumerate() {
                    if let Some(drive) = slot {
                        let next = u128::from(drive.cycles().saturating_add(1));
                        let hz = u128::from(drive.cpu_hz());
                        if next * best_hz < best_next * hz {
                            best_next = next;
                            best_hz = hz;
                            turn = Some(port);
                        }
                    }
                }

                if let Some(port) = turn {
                    if let Some(drive) = self.drives[port].as_mut() {
                        drive.tick_with_iec_bus(&mut self.iec_bus);
                    }
                    self.machine.sync_iec_bus(&mut self.iec_bus);
                    for (other, slot) in self.drives.iter_mut().enumerate() {
                        if other != port
                            && let Some(drive) = slot.as_mut()
                        {
                            drive.sync_iec_bus(&mut self.iec_bus);
                        }
                    }
                    continue;
                }
            }

            // One bit-bang step per C64 `phi2` cycle: the modem samples PA2
            // (pin M, computer TX) and answers on PB0 (pin C, computer RX).
            if let Some(bridge) = self.esp_at_tcp_bridge.as_mut() {
                let rx = bridge.tick(self.machine.user_port_pa2());
                self.machine.set_user_port_pb0(rx);
            }

            let frame_complete = if any_drive {
                let frame_complete = self.machine.tick_with_iec_bus(&mut self.iec_bus);
                for slot in self.drives.iter_mut().flatten() {
                    slot.sync_iec_bus(&mut self.iec_bus);
                }
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

            if let (Some((start, end)), Some(drive8)) = (self.trace_drive_rom_window, self.drive8())
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
                // Per-voice SID samples are diagnostic stems generated beside
                // the mixed stream. They currently have no shell sink, but
                // must still be drained on the same boundary so long-running
                // sessions do not retain every sample since power-on.
                drop(self.machine.take_audio_channel_buffers());
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

    /// 0.9365 on PAL and 0.7500 on NTSC, both widely published and both
    /// reproduced by the derivation to within a tenth of a percent.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            mos_vic_ii::PAL_DOT_CLOCK_HZ,
            mos_vic_ii::NTSC_DOT_CLOCK_HZ,
        )
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

/// Maps an IEC device number (8–11) to its port index (0–3), or `None` when out
/// of range.
const fn port_index(device: u8) -> Option<usize> {
    if device >= FIRST_IEC_DEVICE && (device as usize) < FIRST_IEC_DEVICE as usize + IEC_PORT_COUNT
    {
        Some((device - FIRST_IEC_DEVICE) as usize)
    } else {
        None
    }
}

/// Parses a `"drive-N"` media slot into its IEC device number (8–11), or `None`
/// for any other slot.
fn drive_slot_device(slot: &str) -> Option<u8> {
    let device: u8 = slot.strip_prefix("drive-")?.parse().ok()?;
    port_index(device).map(|_| device)
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
