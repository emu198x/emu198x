//! The Sega Mega Drive / Genesis.
//!
//! A 68000 at the master clock divided by 7, a Z80 at the master clock
//! divided by 15, the 315-5313 VDP, a YM2612 and the SN76489 PSG inside the
//! VDP, wired the way the console wires them. One call to [`MegaDrive::tick`]
//! is one master clock (53.693175 MHz NTSC, 53.203424 MHz PAL), and every
//! chip is stepped from it.
//!
//! # The 68000's map
//!
//! | Range | Contents |
//! |---|---|
//! | `$000000-$3FFFFF` | Cartridge |
//! | `$A00000-$A0FFFF` | The Z80's space, while the 68000 holds the Z80's bus |
//! | `$A10000-$A1001F` | Version register and the three I/O ports |
//! | `$A11100` | Z80 bus request: write 1 to ask, read bit 0 as 0 once granted |
//! | `$A11200` | Z80 reset: write 0 to hold the Z80 and YM2612 in reset |
//! | `$C00000-$C0001F` | VDP ports and the PSG, mirrored to `$DFFFFF` |
//! | `$E00000-$FFFFFF` | 64 KB of work RAM, mirrored |
//!
//! # The Z80's map
//!
//! | Range | Contents |
//! |---|---|
//! | `$0000-$3FFF` | 8 KB of sound RAM, mirrored |
//! | `$4000-$5FFF` | YM2612 |
//! | `$6000-$60FF` | Bank register: each write shifts bit 0 into a 9-bit bank |
//! | `$7F00-$7FFF` | VDP and PSG |
//! | `$8000-$FFFF` | A 32 KB window into the 68000's space, at the bank |
//!
//! # Buses and waits
//!
//! The 68000 is driven through its bus-cycle pins. Its DTACK is withheld,
//! so the CPU waits, while:
//!
//! - the VDP's FIFO is full or a read is not ready;
//! - the VDP holds the bus for a DMA (the 68000 runs on until it next needs
//!   the bus, then waits, which is what a bus grant does);
//! - DRAM refresh takes its two clocks of every 128 from a ROM or RAM
//!   access.
//!
//! The Z80's BUSREQ follows the Zilog datasheet: sampled at the rising edge
//! of a machine cycle's last T-state, it stops the Z80 at the next machine
//! cycle's T1 and BUSACK reports it. The Z80 core has no BUSREQ pin, so the
//! machine models it from the core's public phase.
//!
//! # Not yet modelled
//!
//! Cartridge SRAM, TMSS, the extra wait a Z80 access to the 68000 window
//! costs both CPUs, and the 68000's own access to the Z80 bus while the Z80
//! runs, are follow-up work under emu198x/emu198x#2034.

pub mod io;

use emu198x_sega_315_5313::Vdp;
use emu198x_ti_sn76489::{NoiseLfsr, Sn76489};
use emu198x_yamaha_ym2612::{Variant, Ym2612};
use emu198x_zilog_z80::z80::{IntAckPhase, IoPhase, M1Phase, MemPhase, NmiAckPhase, Phase};
use emu198x_zilog_z80::{BusOp, Z80};
use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use serde::{Deserialize, Serialize};

pub use io::{Buttons, Device};

/// Master clocks per 68000 clock.
pub const MCLKS_PER_68K: u8 = 7;
/// Master clocks per Z80 clock.
pub const MCLKS_PER_Z80: u8 = 15;
/// The Z80's /INT pulse at V-int: about 171.5 Z80 clocks, measured.
const Z80_INT_PULSE_MCLKS: u64 = 2573;
/// 68000 clocks between DRAM refreshes, and the clocks each takes.
const REFRESH_INTERVAL: u32 = 128;
const REFRESH_WAIT: u8 = 2;

/// Where the console was sold: it sets the version register and the video
/// timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    /// Japan: domestic, NTSC.
    Japan,
    /// The Americas: overseas, NTSC.
    Americas,
    /// Europe: overseas, PAL.
    Europe,
}

impl Region {
    #[must_use]
    pub fn pal(self) -> bool {
        self == Self::Europe
    }

    fn overseas(self) -> bool {
        self != Self::Japan
    }
}

/// A Mega Drive with a cartridge in.
#[derive(Serialize, Deserialize)]
pub struct MegaDrive {
    pub cpu: Cpu68000,
    pub z80: Z80,
    pub vdp: Vdp,
    pub ym2612: Ym2612,
    pub psg: Sn76489,
    pub ports: [io::Port; 3],
    pub region: Region,

    rom: Vec<u8>,
    ram: Vec<u8>,
    z80_ram: Vec<u8>,

    mclk: u64,
    phase_68k: u8,
    phase_z80: u8,

    z80_bank: u32,
    z80_busreq: bool,
    z80_busreq_sampled: bool,
    z80_released: bool,
    z80_reset: bool,
    z80_int_until: u64,
    vint_seen: bool,

    refresh_clocks: u32,
    refresh_due: bool,
    refresh_wait: u8,
}

impl MegaDrive {
    /// A console of `region`'s with `rom` in the slot, powered on. The ROM
    /// is the cartridge's bytes in the order the 68000 reads them.
    #[must_use]
    pub fn new(rom: Vec<u8>, region: Region) -> Self {
        // The PSG and YM2612 are clocked from the master clock's dividers;
        // their constructors take the resulting rate.
        let z80_hz = if region.pal() {
            53_203_424 / 15
        } else {
            53_693_175 / 15
        };
        let mut machine = Self {
            cpu: Cpu68000::new(),
            z80: Z80::new(),
            vdp: Vdp::new(region.pal()),
            ym2612: Ym2612::new(Variant::Ym2612),
            psg: Sn76489::new(z80_hz, NoiseLfsr::Sega16),
            ports: [
                io::Port::new(Device::ThreeButton),
                io::Port::new(Device::ThreeButton),
                io::Port::new(Device::None),
            ],
            region,
            rom,
            ram: vec![0; 0x10000],
            z80_ram: vec![0; 0x2000],
            mclk: 0,
            phase_68k: 0,
            phase_z80: 0,
            z80_bank: 0,
            z80_busreq: false,
            z80_busreq_sampled: false,
            z80_released: false,
            z80_reset: true,
            z80_int_until: 0,
            vint_seen: false,
            refresh_clocks: 0,
            refresh_due: false,
            refresh_wait: 0,
        };
        machine.reset_68000();
        machine
    }

    /// Start the 68000 from the cartridge's reset vectors.
    fn reset_68000(&mut self) {
        let ssp = u32::from(self.rom_word(0)) << 16 | u32::from(self.rom_word(2));
        let pc = u32::from(self.rom_word(4)) << 16 | u32::from(self.rom_word(6));
        self.cpu.reset_to(ssp, pc);
    }

    /// Press the reset button: the 68000 restarts; the Z80 and YM2612 go back
    /// into reset; the VDP keeps running.
    pub fn reset(&mut self) {
        self.reset_68000();
        self.z80_reset = true;
        self.z80_busreq = false;
        self.z80_released = false;
        self.z80 = Z80::new();
        self.ym2612.reset();
    }

    fn rom_word(&self, address: u32) -> u16 {
        let a = address as usize & !1;
        let byte = |i: usize| self.rom.get(i).copied().unwrap_or(0xFF);
        u16::from(byte(a)) << 8 | u16::from(byte(a + 1))
    }

    /// One master clock.
    pub fn tick(&mut self) {
        self.vdp.tick();
        if let Some(address) = self.vdp.dma_read() {
            let word = self.read_dma_source(address);
            self.vdp.dma_word(word);
        }
        let vint = self.vdp.status() & 0x80 != 0;
        if vint && !self.vint_seen {
            self.z80_int_until = self.mclk + Z80_INT_PULSE_MCLKS;
        }
        self.vint_seen = vint;

        if self.phase_68k == 0 {
            self.tick_68000();
            self.ym2612.tick();
            for port in &mut self.ports {
                port.tick(u32::from(MCLKS_PER_68K));
            }
        }
        // Two Z80 half-cycles per 15 master clocks.
        if self.phase_z80 == 0 || self.phase_z80 == 8 {
            self.tick_z80_half();
        }
        if self.phase_z80 == 0 {
            self.psg.tick();
        }

        self.phase_68k = (self.phase_68k + 1) % MCLKS_PER_68K;
        self.phase_z80 = (self.phase_z80 + 1) % MCLKS_PER_Z80;
        self.mclk += 1;
    }

    /// Run until the VDP finishes a frame.
    pub fn run_frame(&mut self) {
        let start = self.vdp.frames();
        while self.vdp.frames() == start {
            self.tick();
        }
    }

    /// Master clocks since power-on.
    #[must_use]
    pub fn mclk(&self) -> u64 {
        self.mclk
    }

    // ---- The 68000. ----

    fn tick_68000(&mut self) {
        self.refresh_clocks += 1;
        if self.refresh_clocks >= REFRESH_INTERVAL {
            self.refresh_clocks -= REFRESH_INTERVAL;
            self.refresh_due = true;
        }
        self.cpu.ipl = self.vdp.interrupt_level();
        self.service_68000_bus();
        self.cpu.tick();
    }

    /// Answer the 68000's bus cycle, or hold it.
    fn service_68000_bus(&mut self) {
        let State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = self.cpu.state.clone()
        else {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        };
        if cycle_count < 3 {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        if fc == FunctionCode::InterruptAck {
            // The VDP's interrupts are autovectored.
            let level = interrupt_acknowledge_level(addr);
            self.vdp.acknowledge_interrupt();
            self.cpu.bus_status = BusStatus::Ready(24 + u16::from(level));
            return;
        }
        if self.vdp.bus_request() {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        let address = addr & 0x00FF_FFFF;
        let rom_or_ram = !(0x40_0000..0xE0_0000).contains(&address);
        if rom_or_ram && (self.refresh_due || self.refresh_wait > 0) {
            if self.refresh_due {
                self.refresh_due = false;
                self.refresh_wait = REFRESH_WAIT;
            }
            self.refresh_wait -= 1;
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        let result = if is_read {
            self.read_68000(address, is_word).map(BusStatus::Ready)
        } else {
            self.write_68000(address, is_word, data.unwrap_or(0))
                .map(|()| BusStatus::Ready(0))
        };
        self.cpu.bus_status = result.unwrap_or(BusStatus::Wait);
    }

    /// The word or byte the 68000 reads. `None` holds DTACK.
    fn read_68000(&mut self, address: u32, is_word: bool) -> Option<u16> {
        let open_bus = self.cpu.irc;
        let byte_of = |word: u16| {
            if address & 1 == 0 {
                word >> 8
            } else {
                word & 0xFF
            }
        };
        let word = match address {
            0x00_0000..=0x3F_FFFF => self.rom_word(address),
            0xA0_0000..=0xA0_FFFF => {
                if !self.z80_bus_granted() {
                    return Some(if is_word { open_bus } else { open_bus & 0xFF });
                }
                let byte = self.read_z80_space(address as u16 & 0x7FFF);
                // A word read sees the byte on both halves.
                return Some(if is_word {
                    u16::from(byte) << 8 | u16::from(byte)
                } else {
                    u16::from(byte)
                });
            }
            0xA1_0000..=0xA1_001F => {
                let byte = u16::from(self.read_io((address >> 1) & 0xF));
                byte << 8 | byte
            }
            0xA1_1100..=0xA1_1101 => {
                let granted = u16::from(!self.z80_bus_granted());
                (open_bus & 0xFEFE) | granted << 8 | granted
            }
            0xC0_0000..=0xDF_FFFF => {
                if address & 0x0700E0 != 0 {
                    return Some(open_bus);
                }
                match address & 0x1F {
                    0x00..=0x03 => self.vdp.read_data()?,
                    0x04..=0x07 => (self.vdp.read_control() & 0x03FF) | (open_bus & 0xFC00),
                    0x08..=0x0F => self.vdp.read_hv(),
                    _ => open_bus,
                }
            }
            0xE0_0000..=0xFF_FFFF => {
                let a = (address & 0xFFFE) as usize;
                u16::from(self.ram[a]) << 8 | u16::from(self.ram[a + 1])
            }
            _ => open_bus,
        };
        Some(if is_word { word } else { byte_of(word) })
    }

    /// A 68000 write. `None` holds DTACK.
    fn write_68000(&mut self, address: u32, is_word: bool, value: u16) -> Option<()> {
        // A byte write puts the byte on both halves of the data bus.
        let value = if is_word {
            value
        } else {
            (value & 0xFF) << 8 | (value & 0xFF)
        };
        let byte = if address & 1 == 0 {
            (value >> 8) as u8
        } else {
            value as u8
        };
        match address {
            0xA0_0000..=0xA0_FFFF => {
                if self.z80_bus_granted() {
                    // A word write stores its high byte.
                    let byte = if is_word { (value >> 8) as u8 } else { byte };
                    self.write_z80_space(address as u16 & 0x7FFF, byte);
                }
            }
            0xA1_0000..=0xA1_001F => self.write_io((address >> 1) & 0xF, byte),
            0xA1_1100..=0xA1_1101 => {
                if address & 1 == 0 || is_word {
                    self.z80_busreq = value & 0x0100 != 0;
                }
            }
            0xA1_1200..=0xA1_1201 => {
                if address & 1 == 0 || is_word {
                    let reset = value & 0x0100 == 0;
                    if reset && !self.z80_reset {
                        self.ym2612.reset();
                    }
                    if !reset && self.z80_reset {
                        self.z80 = Z80::new();
                        self.z80_busreq_sampled = false;
                        self.z80_released = false;
                    }
                    self.z80_reset = reset;
                }
            }
            0xC0_0000..=0xDF_FFFF => {
                if address & 0x0700E0 != 0 {
                    return Some(());
                }
                match address & 0x1F {
                    0x00..=0x03 => self.vdp.write_data(value)?,
                    0x04..=0x07 => self.vdp.write_control(value),
                    0x10..=0x17 if address & 1 == 1 || is_word => self.psg.write(value as u8),
                    _ => {}
                }
            }
            0xE0_0000..=0xFF_FFFF => {
                let a = (address & 0xFFFF) as usize;
                if is_word {
                    self.ram[a & !1] = (value >> 8) as u8;
                    self.ram[(a & !1) + 1] = value as u8;
                } else {
                    self.ram[a] = byte;
                }
            }
            _ => {}
        }
        Some(())
    }

    /// A VDP DMA source read: ROM or RAM, without side effects.
    fn read_dma_source(&self, address: u32) -> u16 {
        let address = address & 0x00FF_FFFE;
        if address >= 0xE0_0000 {
            let a = (address & 0xFFFE) as usize;
            u16::from(self.ram[a]) << 8 | u16::from(self.ram[a + 1])
        } else if address < 0x40_0000 {
            self.rom_word(address)
        } else {
            0xFFFF
        }
    }

    fn read_io(&self, register: u32) -> u8 {
        match register {
            // Version: overseas, PAL, no expansion unit, version 0.
            0 => {
                (u8::from(self.region.overseas()) << 7) | (u8::from(self.region.pal()) << 6) | 0x20
            }
            1..=3 => self.ports[register as usize - 1].read_data(),
            4..=6 => self.ports[register as usize - 4].control(),
            // Serial registers: idle.
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, register: u32, value: u8) {
        match register {
            1..=3 => self.ports[register as usize - 1].write_data(value),
            4..=6 => self.ports[register as usize - 4].write_control(value),
            _ => {}
        }
    }

    // ---- The Z80. ----

    /// Whether the 68000 may use the Z80's bus.
    fn z80_bus_granted(&self) -> bool {
        self.z80_released || (self.z80_reset && self.z80_busreq)
    }

    /// The Z80 is at the rising edge of a machine cycle's last T-state.
    fn z80_at_last_t_rise(&self) -> bool {
        match self.z80.phase {
            Phase::M1(p) => p == M1Phase::T4Rise,
            Phase::MemRead(p) | Phase::MemWrite(p) | Phase::Contend(p) => p == MemPhase::T3Rise,
            Phase::IoRead(p) | Phase::IoWrite(p) => p == IoPhase::T4Rise,
            Phase::IntAck(p) => p == IntAckPhase::T7Rise,
            Phase::NmiAck(p) => p == NmiAckPhase::T5Rise,
            Phase::Internal(p) => p.remaining == 2,
            Phase::NextStep => false,
        }
    }

    /// The Z80 is about to start a machine cycle that uses the bus.
    fn z80_at_cycle_start(&self) -> bool {
        matches!(
            self.z80.phase,
            Phase::M1(M1Phase::T1Rise)
                | Phase::MemRead(MemPhase::T1Rise)
                | Phase::MemWrite(MemPhase::T1Rise)
                | Phase::Contend(MemPhase::T1Rise)
                | Phase::IoRead(IoPhase::T1Rise)
                | Phase::IoWrite(IoPhase::T1Rise)
                | Phase::IntAck(IntAckPhase::T1Rise)
                | Phase::NmiAck(NmiAckPhase::T1Rise)
        )
    }

    fn tick_z80_half(&mut self) {
        if self.z80_reset {
            return;
        }
        if self.z80_released {
            if self.z80_busreq {
                return;
            }
            self.z80_released = false;
            self.z80_busreq_sampled = false;
        } else if self.z80_busreq_sampled && self.z80_at_cycle_start() {
            self.z80_released = true;
            return;
        }
        if self.z80_at_last_t_rise() {
            self.z80_busreq_sampled = self.z80_busreq;
        }
        self.z80.irq = self.mclk < self.z80_int_until;
        self.z80.tick();
        match self.z80.bus_request() {
            Some(BusOp::MemRead) => self.z80.data_in = self.read_z80_space(self.z80.addr),
            Some(BusOp::MemWrite) => self.write_z80_space(self.z80.addr, self.z80.data),
            // The Z80's I/O space is not connected.
            Some(BusOp::IoRead | BusOp::IntAck) => self.z80.data_in = 0xFF,
            Some(BusOp::IoWrite) | None => {}
        }
    }

    fn read_z80_space(&mut self, address: u16) -> u8 {
        match address {
            0x0000..=0x3FFF => self.z80_ram[usize::from(address) & 0x1FFF],
            0x4000..=0x5FFF => self.ym2612.read((address & 3) as u8),
            0x7F00..=0x7FFF => {
                let word = match address & 0x1F {
                    0x00..=0x03 => self.vdp.read_data().unwrap_or(0xFFFF),
                    0x04..=0x07 => self.vdp.read_control(),
                    0x08..=0x0F => self.vdp.read_hv(),
                    _ => 0xFFFF,
                };
                if address & 1 == 0 {
                    (word >> 8) as u8
                } else {
                    word as u8
                }
            }
            0x8000..=0xFFFF => {
                let target = (self.z80_bank << 15) | u32::from(address & 0x7FFF);
                let word = self.read_dma_source(target);
                if target & 1 == 0 {
                    (word >> 8) as u8
                } else {
                    word as u8
                }
            }
            _ => 0xFF,
        }
    }

    fn write_z80_space(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x3FFF => self.z80_ram[usize::from(address) & 0x1FFF] = value,
            0x4000..=0x5FFF => self.ym2612.write((address & 3) as u8, value),
            0x6000..=0x60FF => {
                self.z80_bank = ((self.z80_bank >> 1) | (u32::from(value & 1) << 8)) & 0x1FF;
            }
            0x7F00..=0x7FFF => match address & 0x1F {
                0x00..=0x03 => {
                    let _ = self
                        .vdp
                        .write_data(u16::from(value) << 8 | u16::from(value));
                }
                0x04..=0x07 => self
                    .vdp
                    .write_control(u16::from(value) << 8 | u16::from(value)),
                0x11 | 0x13 | 0x15 | 0x17 => self.psg.write(value),
                _ => {}
            },
            0x8000..=0xFFFF => {
                let target = (self.z80_bank << 15) | u32::from(address & 0x7FFF);
                if target >= 0xE0_0000 {
                    self.ram[(target & 0xFFFF) as usize] = value;
                }
            }
            _ => {}
        }
    }

    // ---- Inspection and input. ----

    /// Work RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// The Z80's sound RAM.
    #[must_use]
    pub fn z80_ram(&self) -> &[u8] {
        &self.z80_ram
    }

    /// The cartridge ROM.
    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// Whether the Z80 has stopped for the 68000's bus request.
    #[must_use]
    pub fn z80_bus_acknowledged(&self) -> bool {
        self.z80_released
    }

    /// Whether the Z80 is held in reset.
    #[must_use]
    pub fn z80_in_reset(&self) -> bool {
        self.z80_reset
    }

    /// Set the buttons on port `port` (0 or 1).
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        if let Some(p) = self.ports.get_mut(port) {
            p.buttons = buttons;
        }
    }

    /// Restore hook: rebuild the Z80's step sequence after a snapshot.
    pub fn finish_restore(&mut self) {
        self.z80.rehydrate_walker_sequence();
    }
}

#[cfg(test)]
mod tests;
