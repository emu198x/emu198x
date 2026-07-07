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

use common_commodore_iec::IecBus;
use mos_6502::M6502;
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
}

impl Default for IecDriveBoard {
    fn default() -> Self {
        Self::new()
    }
}
