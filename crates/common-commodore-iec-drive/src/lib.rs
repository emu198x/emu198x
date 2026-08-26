//! The shared Commodore IEC drive board: the 6502 + two 6522 VIAs + 2KB RAM and
//! the drive-side IEC serial glue that the 1541 and 1571 board layers share
//! verbatim.
//!
//! This is phase 2 of the drive unification (#806). Phase 1 (#764) hoisted the
//! GCR rotation/serialiser engine into `common-commodore-drive-gcr`; the board
//! *around* it — the CPU bus loop's components, the VIA register file, and the
//! byte-identical VIA↔IEC wiring — was still duplicated. Each drive **embeds** an
//! [`IecDriveBoard`] and keeps only what genuinely differs: its ROM, its address
//! map, its tick sequencing, and (the 1571) its CIA/WD1770/side.
//!
//! The board owns the components and the engine-free glue. The composition
//! methods that also read the rotation engine or the mounted disk (VIA2 Port A/B
//! inputs, byte-ready→CA1) are threaded the engine/disk state by the drive — see
//! the plan's board↔engine coupling contract — and land here in a later step.

use common_commodore_drive_gcr::GcrRotationEngine;
use common_commodore_iec::IecBus;
use emu198x_mos_6502::M6502;
use mos_via_6522::Via6522;

/// 2KB of drive RAM, mirrored across `$0000-$17FF` (identical on the 1541/1571).
pub const RAM_SIZE: usize = 0x0800;
/// The IEC device number a drive powers on as (device 8).
pub const DEFAULT_DEVICE_NUMBER: u8 = 8;

/// The shared IEC drive board: 6502, VIA1, VIA2, 2KB RAM, and the device number,
/// plus the byte-identical drive-side IEC/VIA glue. Embedded by each drive.
#[derive(Clone)]
pub struct IecDriveBoard {
    cpu: M6502,
    via1: Via6522,
    via2: Via6522,
    ram: [u8; RAM_SIZE],
    device_number: u8,
}

/// The board's persistent state as a transfer struct, mirroring the rotation
/// engine's `RotationState`. The drive flattens these fields into its own
/// `Snapshot` (preserving its exact postcard layout) rather than serialising the
/// board directly — this DTO just moves them across the crate boundary.
#[derive(Clone)]
pub struct BoardState {
    pub cpu: M6502,
    pub via1: Via6522,
    pub via2: Via6522,
    pub ram: [u8; RAM_SIZE],
    pub device_number: u8,
}

impl IecDriveBoard {
    /// Constructs a powered-on board (CPU reset, VIAs cleared, RAM zeroed) at the
    /// default device number.
    #[must_use]
    pub fn new() -> Self {
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            via1: Via6522::new(),
            via2: Via6522::new(),
            ram: [0; RAM_SIZE],
            device_number: DEFAULT_DEVICE_NUMBER,
        }
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub const fn via1(&self) -> &Via6522 {
        &self.via1
    }

    pub const fn via1_mut(&mut self) -> &mut Via6522 {
        &mut self.via1
    }

    #[must_use]
    pub const fn via2(&self) -> &Via6522 {
        &self.via2
    }

    pub const fn via2_mut(&mut self) -> &mut Via6522 {
        &mut self.via2
    }

    /// The 2KB RAM (mirrored across `$0000-$17FF` by the drive's address decode).
    #[must_use]
    pub const fn ram(&self) -> &[u8; RAM_SIZE] {
        &self.ram
    }

    pub const fn ram_mut(&mut self) -> &mut [u8; RAM_SIZE] {
        &mut self.ram
    }

    #[must_use]
    pub const fn device_number(&self) -> u8 {
        self.device_number
    }

    /// Sets the IEC device number (8-11); the drive derives its bus address from
    /// this on every tick, so it takes effect immediately.
    pub const fn set_device_number(&mut self, device_number: u8) {
        self.device_number = device_number;
    }

    /// Captures the board's persistent state for a snapshot.
    #[must_use]
    pub fn state(&self) -> BoardState {
        BoardState {
            cpu: self.cpu.clone(),
            via1: self.via1.clone(),
            via2: self.via2.clone(),
            ram: self.ram,
            device_number: self.device_number,
        }
    }

    /// Restores the board's persistent state from a snapshot.
    pub fn restore_state(&mut self, state: BoardState) {
        self.cpu = state.cpu;
        self.via1 = state.via1;
        self.via2 = state.via2;
        self.ram = state.ram;
        self.device_number = state.device_number;
    }

    // ---- VIA1 / IEC serial glue (engine-free) ----

    /// Composes VIA1 Port A for a CPU read (pulled high on a plain drive).
    #[must_use]
    pub fn via1_port_a_read(&self) -> u8 {
        self.via1.compose_port_a_read(self.via1_port_a_input())
    }

    /// Composes VIA1 Port B for a CPU read: the mixed IEC bus state XOR the
    /// open-collector inversion, with the device-select bits folded in.
    #[must_use]
    pub fn via1_port_b_read(&self, bus: Option<&IecBus>) -> u8 {
        (((self.via1.orb() & 0x1A) | self.via1_bus_port(bus)) ^ 0x85)
            | (self.device_select_bits() << 5)
    }

    /// VIA1 Port A input. On a plain drive Port A is not the dual-drive status
    /// port; VICE models it pulled high absent parallel-cable hardware.
    #[must_use]
    pub fn via1_port_a_input(&self) -> u8 {
        0xFF
    }

    /// VIA1 Port B input — the sensed IEC bus lines.
    #[must_use]
    pub fn via1_port_b_input(&self, bus: Option<&IecBus>) -> u8 {
        self.via1_bus_port(bus)
    }

    /// The device address offset from device 8, on VIA1 PB5/PB6.
    #[must_use]
    pub fn device_select_bits(&self) -> u8 {
        self.device_number.saturating_sub(DEFAULT_DEVICE_NUMBER) & 0x03
    }

    /// The sensed IEC bus lines (DATA/CLK/ATN) as VIA1 Port B bits.
    #[must_use]
    pub fn via1_bus_port(&self, bus: Option<&IecBus>) -> u8 {
        let mut value = 0;
        if self.bus_data_high(bus) {
            value |= 0x01;
        }
        if self.bus_clock_high(bus) {
            value |= 0x04;
        }
        if self.bus_atn_high(bus) {
            value |= 0x80;
        }
        value
    }

    /// Whether the IEC ATN line is released (high). No bus → treated as high.
    #[must_use]
    pub fn bus_atn_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(IecBus::drive_atn_high)
    }

    /// Whether the IEC CLK line is released (high). No bus → treated as high.
    #[must_use]
    pub fn bus_clock_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(|bus| bus.drive_port() & 0x04 != 0)
    }

    /// Whether the IEC DATA line is released (high). No bus → treated as high.
    #[must_use]
    pub fn bus_data_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(|bus| bus.drive_port() & 0x01 != 0)
    }

    /// Drives the board's IEC contribution onto the bus from VIA1 Port B's mixed
    /// output state, so input-configured bits release the open-collector lines.
    pub fn drive_iec_outputs(&self, bus: &mut IecBus) {
        bus.write_drive_port_b(self.device_number, self.via1.port_b_drive_state());
    }

    // ---- VIA2 status lines (engine-free) ----

    /// VIA2 CA2 line level (byte-ready enable), honouring the DDR/latch.
    #[must_use]
    pub fn ca2_line_high(&self) -> bool {
        if self.via2.ca2_drive {
            self.via2.ca2_out
        } else {
            self.via2.peek(0x0C) & 0x02 != 0
        }
    }

    /// VIA2 CB2 line level (read/write mode select), honouring the DDR/latch.
    #[must_use]
    pub fn cb2_line_high(&self) -> bool {
        if self.via2.cb2_drive {
            self.via2.cb2_out
        } else {
            self.via2.peek(0x0C) & 0x20 != 0
        }
    }

    /// The head is in read mode (assembling bytes) rather than write mode.
    #[must_use]
    pub fn is_read_mode(&self) -> bool {
        self.cb2_line_high()
    }

    /// Byte-ready is enabled (VIA2 CA2), so an assembled byte pulses the line.
    #[must_use]
    pub fn byte_ready_active(&self) -> bool {
        self.ca2_line_high()
    }

    // ---- VIA2 status lines (engine-coupled) ----
    //
    // These reach the rotation engine and the mounted disk, which the board does
    // not own. Mirroring #764's `RotationContext`, the drive threads the engine
    // by ref plus the small drive-computed inputs (`present`, `write_protected`)
    // it derives from the selected drive and the mounted image.

    /// VIA2 Port A input: the GCR read byte the head is over, or `0` when the
    /// selected drive is absent (the dual-drive DOS heritage reads the alternate
    /// unit's data port low).
    #[must_use]
    pub const fn via2_port_a_input(&self, engine: &GcrRotationEngine, present: bool) -> u8 {
        if present { engine.gcr_read() } else { 0 }
    }

    /// VIA2 Port B input: the drive-status lines. SYNC (PB7) reads high when no
    /// sync is under the head; write-protect (PB4) reads high when the photocell
    /// sees light — through a writable disk's notch *or* an empty drive. Only a
    /// mounted, write-protected disk pulls PB4 low. Reporting an empty drive as
    /// protected would make mounting a writable disk a phantom WP transition,
    /// which the DOS reads as a disk change and uses to slam every open channel
    /// shut (breaking a SAVE onto a disk inserted after power-up). Matches VICE
    /// drive-writeprotect.c ("No disk in drive, write protection is off").
    #[must_use]
    pub fn via2_port_b_input(
        &self,
        engine: &GcrRotationEngine,
        present: bool,
        write_protected: bool,
    ) -> u8 {
        let mut value = 0x6F;
        if self.sync_not_detected(engine) {
            value |= 0x80;
        }
        if !present || !write_protected {
            value |= 0x10;
        }
        value
    }

    /// No sync mark is under the head — true in write mode, or when the engine
    /// reports the head is not over a sync region.
    #[must_use]
    pub fn sync_not_detected(&self, engine: &GcrRotationEngine) -> bool {
        !self.is_read_mode() || !engine.sync_active()
    }

    /// VIA2 CA1 is de-asserted (high) unless byte-ready is enabled *and* the
    /// engine has an assembled byte pending (level or fresh edge).
    #[must_use]
    pub fn byte_ready_not_asserted(&self, engine: &GcrRotationEngine) -> bool {
        !(self.byte_ready_active() && (engine.byte_ready_level() || engine.byte_ready_edge()))
    }

    /// Drives the VIA input pins for one settle pass: VIA1 from the IEC bus
    /// (engine-free) and VIA2 from the rotation engine plus the drive-computed
    /// `present`/`write_protected` inputs. The 1571's side latch and the
    /// mechanics refresh stay in the drive — they touch the drive's own devices.
    pub fn apply_drive_inputs(
        &mut self,
        engine: &GcrRotationEngine,
        bus: Option<&IecBus>,
        present: bool,
        write_protected: bool,
    ) {
        let pa_in = self.via1_port_a_input();
        let pb_in = self.via1_port_b_input(bus);
        // The 1541 serial glue presents IEC ATN to VIA1 CA1 inverted: ATN low
        // becomes a CA1 rising edge, matching VICE's `viacore_signal(...,
        // VIA_SIG_CA1, VIA_SIG_RISE)` path for 1541-style drives.
        let atn_high = self.bus_atn_high(bus);
        self.via1.pa_in = pa_in;
        self.via1.pb_in = pb_in;
        self.via1.set_ca1_level(!atn_high);
        let via2_pa_in = self.via2_port_a_input(engine, present);
        let via2_pb_in = self.via2_port_b_input(engine, present, write_protected);
        let via2_ca1 = self.byte_ready_not_asserted(engine);
        self.via2.pa_in = via2_pa_in;
        self.via2.pb_in = via2_pb_in;
        self.via2.ca1 = via2_ca1;
    }
}

impl Default for IecDriveBoard {
    fn default() -> Self {
        Self::new()
    }
}
