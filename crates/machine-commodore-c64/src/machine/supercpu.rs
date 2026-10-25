//! CMD SuperCPU: a 20 MHz 65C816 accelerator on the expansion port.
//!
//! The SuperCPU takes the bus from the 6510, which then never runs, and puts
//! its own 65C816 in charge. The 65C816 works out of 128 KB of fast static
//! RAM: bank 0 is a cache of the C64's 64 KB, bank 1 holds the shadows of the
//! BASIC and KERNAL ROMs that the board's firmware copies there. The C64 side
//! still runs at its own rate, so anything the 65C816 reaches through the
//! expansion port -- the VIC-II, SID, CIAs, colour RAM, the character ROM --
//! waits for a 1 MHz bus cycle.
//!
//! # What reaches the C64
//!
//! A write to cached RAM goes to the static RAM and, for the pages the
//! optimisation mode selects, through to the C64's RAM as well, so the VIC-II
//! sees it. The write-through goes into a one-entry buffer that drains in the
//! next free bus cycle: the 65C816 carries on at full speed unless a second
//! write-through, or an I/O access, finds the buffer still full. An I/O access
//! takes the next free bus cycle and costs the 65C816 the rest of it.
//!
//! The optimisation modes are written at `$D074-$D077` and select which pages
//! are mirrored ([`MIRRORED_PAGES`]): VIC bank 2, VIC bank 1, the text screen
//! alone, or everything.
//!
//! # Registers
//!
//! Writes to `$D071-$D07F` and `$D0B0-$D0BF`, and reads of `$D0B0-$D0BF`,
//! reach the SuperCPU instead of the VIC-II. `$D07E` unlocks the hardware
//! registers and `$D07F` locks them again; the optimisation modes, the SIMM
//! configuration and the boot map only change while unlocked. `$D07A`/`$D07B`
//! set and clear software 1 MHz mode, `$D072`/`$D073` system 1 MHz mode.
//!
//! # Firmware
//!
//! The board boots from its own EPROM, the CMD ROM, which this repository never
//! carries: the user supplies the image. It appears at `$F80000-$FFFFFF` and,
//! while the boot map is on, over `$8000-$FFFF` of bank 0 as well, so the
//! reset vector comes from it.
//!
//! # SuperRAM
//!
//! An optional SIMM of up to 16 MB, from `$020000` upwards and through the
//! window at `$F60000-$F7FFFF`.
//!
//! # Sources
//!
//! No source in the reference library describes the board. The memory map,
//! register semantics, mirroring table and wait states follow VICE's `xscpu64`
//! (`src/scpu64/scpu64mem.c`, `scpu64meminit.c`, `scpu64cpu.c`), consulted for
//! facts only; it emulates the version 2 board, and so does this. The bus
//! timing here is coarser than VICE's: a stall or an I/O access resumes the
//! 65C816 at the start of the next C64 cycle rather than at VICE's measured
//! sub-cycle offsets.

use emu198x_wdc_65c816::Wdc65c816;
use serde::{Deserialize, Serialize};

use super::C64;

/// The 65C816's clock in fast mode.
pub const SUPERCPU_HZ: u64 = 20_000_000;

/// Bank 0 and bank 1 of static RAM.
const SRAM_SIZE: usize = 0x2_0000;

/// The largest SuperRAM SIMM the board takes.
pub const SUPERRAM_MAX: usize = 16 * 1024 * 1024;

/// Extra 20 MHz cycles an EPROM access costs (VICE
/// `scpu64_clock_read_stretch_eprom`, three times the cycle's own length).
const EPROM_WAIT: u32 = 3;

/// Extra 20 MHz cycles a SuperRAM access costs: one, the same-row case in
/// VICE's `scpu64_clock_*_stretch_simm`. The row and cell tracking that makes
/// some accesses free and others two and a half cycles is not modelled.
const SUPERRAM_WAIT: u32 = 1;

/// Pages written through to the C64, by mirror index, as `(first, last)`
/// page. Index bits 0 and 1 are optimisation bits 0 and 2, bits 2 and 3 are
/// optimisation bits 6 and 7, exactly as VICE's `mem_set_mirroring` builds it
/// from its `mem_mirrors` table.
pub const MIRRORED_PAGES: [Option<(u8, u8)>; 16] = [
    Some((0x80, 0xBF)),
    Some((0x80, 0xBF)),
    Some((0x00, 0x3F)),
    Some((0x02, 0x3F)),
    Some((0x40, 0x7F)),
    Some((0x40, 0x7F)),
    Some((0xC0, 0xFF)),
    Some((0xC0, 0xFF)),
    Some((0x04, 0x07)),
    Some((0x04, 0x07)),
    None,
    None,
    Some((0x00, 0xFF)),
    Some((0x02, 0xFF)),
    Some((0x00, 0xFF)),
    Some((0x02, 0xFF)),
];

/// Where one 65C816 access lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// Static RAM at this offset.
    Sram(usize),
    /// The CMD EPROM at this offset.
    Rom(usize),
    /// SuperRAM at this offset.
    SuperRam(usize),
    /// The C64's character ROM, through the expansion port.
    CharRom(u16),
    /// The C64's I/O, through the expansion port.
    Io(u16),
    /// A SuperCPU register.
    Register(u16),
    /// Nothing answers: the 65C816 reads the bank byte it put on the bus.
    Open(u8),
}

/// What one 65C816 cycle did to the C64 cycle it ran in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// It ran, and the 65C816 can run again in this C64 cycle.
    Ran,
    /// It ran and used the C64 bus; the 65C816 waits for the next C64 cycle.
    Synced,
    /// It could not run until the next C64 cycle.
    Stalled,
}

/// The CMD SuperCPU.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuperCpu {
    cpu: Wdc65c816,
    rom: Vec<u8>,
    sram: Vec<u8>,
    superram: Vec<u8>,
    /// The emulated 6510 port's LORAM, HIRAM and CHAREN bits.
    port: u8,
    hw_enable: bool,
    system_1mhz: bool,
    software_1mhz: bool,
    /// The board's 1 MHz switch.
    switch_1mhz: bool,
    /// The board's JiffyDOS switch.
    switch_jiffy: bool,
    optimisation: u8,
    bootmap: bool,
    dos_extension: bool,
    simm_config: u8,
    /// 20 MHz ticks owed, in units of C64 cycles.
    clock_accumulator: u64,
    /// A write-through waiting for a free C64 bus cycle.
    write_buffer: Option<(u16, u8)>,
    /// Extra 20 MHz cycles the last access still owes.
    wait_states: u32,
}

impl SuperCpu {
    /// A SuperCPU with `rom` as its CMD firmware and `superram` bytes of
    /// SuperRAM, powered on and about to fetch its reset vector.
    ///
    /// # Errors
    ///
    /// Fails if the firmware is not a power-of-two size from 64 KB to 512 KB,
    /// or the SuperRAM is more than 16 MB or not a whole number of megabytes.
    pub fn new(rom: Vec<u8>, superram: usize) -> Result<Self, String> {
        if !rom.len().is_power_of_two() || !(0x1_0000..=0x8_0000).contains(&rom.len()) {
            return Err(format!(
                "SuperCPU ROM must be 64, 128, 256 or 512 KB, got {} bytes",
                rom.len()
            ));
        }
        if superram > SUPERRAM_MAX || !superram.is_multiple_of(1024 * 1024) {
            return Err(format!(
                "SuperRAM must be 0 to 16 MB in whole megabytes, got {superram} bytes"
            ));
        }
        let mut scpu = Self {
            cpu: Wdc65c816::new(),
            rom,
            sram: vec![0; SRAM_SIZE],
            superram: vec![0; superram],
            port: 7,
            hw_enable: false,
            system_1mhz: false,
            software_1mhz: false,
            switch_1mhz: false,
            switch_jiffy: true,
            optimisation: 0,
            bootmap: true,
            dos_extension: false,
            simm_config: 4,
            clock_accumulator: 0,
            write_buffer: None,
            wait_states: 0,
        };
        scpu.reset();
        Ok(scpu)
    }

    /// Press reset: the registers return to their power-on values (VICE
    /// `scpu64_hardware_reset`) and the 65C816 fetches its vector through the
    /// boot map.
    pub fn reset(&mut self) {
        self.optimisation = 0xC7;
        self.software_1mhz = false;
        self.system_1mhz = false;
        self.hw_enable = false;
        self.dos_extension = false;
        self.bootmap = true;
        self.simm_config = 4;
        self.port = 7;
        self.write_buffer = None;
        self.wait_states = 0;
        self.clock_accumulator = 0;
        self.cpu.reset();
    }

    /// The 65C816.
    #[must_use]
    pub fn cpu(&self) -> &Wdc65c816 {
        &self.cpu
    }

    /// The 65C816, for test harnesses.
    pub fn cpu_mut(&mut self) -> &mut Wdc65c816 {
        &mut self.cpu
    }

    /// Whether the 65C816 is running at 20 MHz rather than 1 MHz.
    #[must_use]
    pub fn fast(&self) -> bool {
        !(self.system_1mhz || self.software_1mhz || (self.switch_1mhz && !self.hw_enable))
    }

    /// Set the board's 1 MHz switch.
    pub fn set_switch_1mhz(&mut self, on: bool) {
        self.switch_1mhz = on;
    }

    /// Whether the hardware registers are unlocked.
    #[must_use]
    pub fn hw_enabled(&self) -> bool {
        self.hw_enable
    }

    /// The optimisation register: mode in bits 6-7.
    #[must_use]
    pub fn optimisation(&self) -> u8 {
        self.optimisation
    }

    /// Whether the boot map is on.
    #[must_use]
    pub fn bootmap(&self) -> bool {
        self.bootmap
    }

    /// SuperRAM fitted, in bytes.
    #[must_use]
    pub fn superram_size(&self) -> usize {
        self.superram.len()
    }

    /// A byte of static RAM, bank 0 or 1, with no side effects.
    #[must_use]
    pub fn sram(&self, addr: u32) -> u8 {
        self.sram.get(addr as usize).copied().unwrap_or(0)
    }

    /// Store a byte of static RAM directly, bank 0 or 1, for loaders and
    /// test harnesses. Nothing is written through to the C64.
    pub fn write_sram(&mut self, addr: u32, value: u8) {
        if let Some(byte) = self.sram.get_mut(addr as usize) {
            *byte = value;
        }
    }

    /// A byte of SuperRAM, with no side effects.
    #[must_use]
    pub fn superram(&self, offset: usize) -> u8 {
        self.superram.get(offset).copied().unwrap_or(0)
    }

    /// Whether a write-through is waiting for the C64 bus.
    #[must_use]
    pub fn write_pending(&self) -> bool {
        self.write_buffer.is_some()
    }

    fn mirror_index(&self) -> usize {
        let o = self.optimisation;
        usize::from(o & 0x01)
            | (usize::from(o & 0x04 != 0) << 1)
            | (usize::from(o & 0x40 != 0) << 2)
            | (usize::from(o & 0x80 != 0) << 3)
    }

    /// Whether a write to bank-0 `addr` goes through to the C64.
    #[must_use]
    pub fn mirrored(&self, addr: u16) -> bool {
        let page = (addr >> 8) as u8;
        MIRRORED_PAGES[self.mirror_index()]
            .is_some_and(|(first, last)| (first..=last).contains(&page))
    }

    /// The SIMM address space the configuration decodes (VICE
    /// `mem_set_simm`).
    fn simm_space(&self) -> usize {
        match self.simm_config & 7 {
            0 => 1024 * 1024,
            1 => 4 * 1024 * 1024,
            2 => 8 * 1024 * 1024,
            _ => 16 * 1024 * 1024,
        }
    }

    fn rom_offset(&self, addr: u32) -> usize {
        addr as usize & (self.rom.len() - 1)
    }

    /// Bank 0 with the emulated port, the boot map and the DOS extension
    /// applied; no cartridge (VICE `scpu64meminit.c`, the columns with
    /// EXROM and GAME high).
    fn decode_bank0_read(&self, addr: u16) -> Target {
        let a = usize::from(addr);
        let bank1 = |offset: usize| Target::Sram(0x1_0000 + offset);
        match addr {
            0x0000..=0x0FFF | 0x6000..=0x7FFF => Target::Sram(a),
            0x1000..=0x5FFF if self.dos_extension => bank1(a),
            0x1000..=0x5FFF => Target::Sram(a),
            0x8000..=0xCFFF | 0xE000..=0xFFFF if self.bootmap => {
                Target::Rom(self.rom_offset(u32::from(addr)))
            }
            0x8000..=0x9FFF if self.dos_extension => bank1(a),
            0xA000..=0xBFFF if self.port & 3 == 3 => bank1(a),
            0x8000..=0xCFFF => Target::Sram(a),
            0xD000..=0xDFFF => match self.port {
                0 | 4 if self.bootmap => Target::Rom(self.rom_offset(u32::from(addr))),
                0 | 4 => Target::Sram(a),
                1..=3 => Target::CharRom(addr & 0x0FFF),
                _ if (0xD0B0..=0xD0BF).contains(&addr) => Target::Register(addr),
                _ => Target::Io(addr),
            },
            // The KERNAL: bank 1's shadow at $01E000, or at $016000 while
            // the hardware registers are unlocked.
            0xE000..=0xFFFF if self.port & 2 != 0 && self.hw_enable => Target::Sram(0x8000 + a),
            0xE000..=0xFFFF if self.port & 2 != 0 => bank1(a),
            0xE000..=0xFFFF => Target::Sram(a),
        }
    }

    fn decode(&self, addr: u32, write: bool) -> Target {
        let bank = (addr >> 16) as u8;
        let offset = addr as usize & 0xFFFF;
        match bank {
            0x00 if write => {
                let a = offset as u16;
                let io = (0xD000..=0xDFFF).contains(&a) && self.port >= 5;
                if io && ((0xD071..=0xD07F).contains(&a) || (0xD0B0..=0xD0BF).contains(&a)) {
                    Target::Register(a)
                } else if io {
                    Target::Io(a)
                } else {
                    Target::Sram(offset)
                }
            }
            0x00 => self.decode_bank0_read(offset as u16),
            0x01 => Target::Sram(0x1_0000 + offset),
            0xF6 | 0xF7 if !self.superram.is_empty() => {
                Target::SuperRam(addr as usize & 0x1_FFFF & (self.superram.len() - 1))
            }
            0xF8..=0xFF => Target::Rom(self.rom_offset(addr)),
            _ if !self.superram.is_empty() && (addr as usize) < self.simm_space() => {
                Target::SuperRam(addr as usize & (self.superram.len() - 1))
            }
            _ => Target::Open(bank),
        }
    }

    /// A read of `$D0B0-$D0BF` (VICE `scpu64_hardware_read`, version 2).
    fn register_read(&self, addr: u16) -> u8 {
        let value = match addr {
            0xD0B0 => 0x40,
            0xD0B2 => (u8::from(self.hw_enable) << 7) | (u8::from(self.system_1mhz) << 6),
            0xD0B3 | 0xD0B4 => self.optimisation & 0xC0,
            0xD0B5 => (u8::from(self.switch_jiffy) << 7) | (u8::from(self.switch_1mhz) << 6),
            0xD0B6 => u8::from(self.cpu.e()) << 7,
            0xD0B8 | 0xD0B9 => (u8::from(self.software_1mhz) << 7) | (u8::from(!self.fast()) << 6),
            0xD0BC..=0xD0BF => u8::from(self.dos_extension) << 7,
            _ => 0x00,
        };
        value | (self.optimisation & 7)
    }

    /// A write to `$D071-$D07F` or `$D0B0-$D0BF` (VICE
    /// `scpu64_hardware_store`).
    fn register_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD072 => self.system_1mhz = true,
            0xD073 => self.system_1mhz = false,
            0xD074..=0xD077 if self.hw_enable => {
                self.optimisation = ((addr & 3) as u8) << 6;
            }
            0xD078 if self.hw_enable => self.simm_config = value,
            0xD07A => self.software_1mhz = true,
            0xD079 | 0xD07B => self.software_1mhz = false,
            0xD07E => self.hw_enable = true,
            0xD07D | 0xD07F => self.hw_enable = false,
            0xD0B2 if self.hw_enable => {
                self.system_1mhz = value & 0x40 != 0;
                if value & 0x80 == 0 {
                    self.hw_enable = false;
                }
            }
            0xD0B3 if self.hw_enable => {
                self.optimisation = (self.optimisation & 0x38) | (value & 0xC7);
            }
            0xD0B4 if self.hw_enable => {
                self.optimisation = (self.optimisation & 0x3F) | (value & 0xC0);
            }
            0xD0B6 if self.hw_enable => self.bootmap = false,
            0xD0B7 if self.hw_enable => self.bootmap = true,
            0xD0B8 if self.hw_enable => self.software_1mhz = value & 0x80 != 0,
            0xD0BC if self.hw_enable => self.dos_extension = value & 0x80 != 0,
            0xD0BE if self.hw_enable => self.dos_extension = true,
            0xD0BD | 0xD0BF => self.dos_extension = false,
            _ => {}
        }
    }

    /// A static-RAM write in bank 0 or 1. Bank 0 page 0's `$0001` also
    /// sets the emulated port.
    fn sram_write(&mut self, offset: usize, value: u8) {
        if let Some(byte) = self.sram.get_mut(offset) {
            *byte = value;
        }
        if offset == 0x0001 {
            self.port = value & 7;
        }
    }
}

impl C64 {
    /// Fit a CMD SuperCPU with `rom` as its firmware and `superram` bytes of
    /// SuperRAM. From the next cycle the 65C816 runs the machine and the
    /// 6510 is held off the bus.
    ///
    /// # Errors
    ///
    /// Fails on a firmware or SuperRAM size the board does not take.
    pub fn fit_supercpu(&mut self, rom: Vec<u8>, superram: usize) -> Result<(), String> {
        self.supercpu = Some(Box::new(SuperCpu::new(rom, superram)?));
        Ok(())
    }

    /// Remove the SuperCPU; the 6510 has the bus again.
    pub fn remove_supercpu(&mut self) {
        self.supercpu = None;
    }

    /// The fitted SuperCPU.
    #[must_use]
    pub fn supercpu(&self) -> Option<&SuperCpu> {
        self.supercpu.as_deref()
    }

    /// The fitted SuperCPU, mutably.
    pub fn supercpu_mut(&mut self) -> Option<&mut SuperCpu> {
        self.supercpu.as_deref_mut()
    }

    /// Run the SuperCPU through one C64 cycle in place of the 6510.
    pub(super) fn tick_supercpu(&mut self, mut scpu: Box<SuperCpu>) -> Box<SuperCpu> {
        scpu.cpu.irq = self.vic.irq || self.cia1.irq || self.memory.reu_irq();
        scpu.cpu.nmi = self.cia2.irq || self.restore_nmi || self.memory.freeze_cart_nmi();
        // The C64 bus cycle this tick offers, unless the VIC-II has it.
        let mut bus_free = !self.vic.ba_low;

        if scpu.fast() {
            if bus_free && let Some((addr, value)) = scpu.write_buffer.take() {
                self.memory.ram_write(addr, value);
                bus_free = false;
            }
            scpu.clock_accumulator += SUPERCPU_HZ;
            let cpu_hz = self.model.timing().cpu_hz;
            while scpu.clock_accumulator >= cpu_hz {
                scpu.clock_accumulator -= cpu_hz;
                if scpu.wait_states > 0 {
                    scpu.wait_states -= 1;
                    continue;
                }
                match self.supercpu_cycle(&mut scpu, &mut bus_free, true) {
                    Step::Ran => {}
                    Step::Synced | Step::Stalled => {
                        scpu.clock_accumulator = 0;
                        break;
                    }
                }
            }
        } else {
            // At 1 MHz the 65C816 is locked to the C64's cycle, and like the
            // 6510 a read waits while the VIC-II holds BA low.
            scpu.clock_accumulator = 0;
            if let Some((addr, value)) = scpu.write_buffer.take() {
                self.memory.ram_write(addr, value);
            }
            let mut bus = true;
            if !self.vic.ba_low || !scpu.cpu.rw {
                self.supercpu_cycle(&mut scpu, &mut bus, false);
            }
        }
        scpu
    }

    /// One 65C816 cycle, if the bus it needs is available.
    fn supercpu_cycle(&mut self, scpu: &mut SuperCpu, bus_free: &mut bool, fast: bool) -> Step {
        let cpu = &scpu.cpu;
        if !cpu.vda && !cpu.vpa {
            scpu.cpu.tick();
            return Step::Ran;
        }
        let write = !cpu.rw;
        let addr = cpu.addr;
        let value = cpu.data;
        let target = scpu.decode(addr, write);
        let needs_bus = matches!(target, Target::Io(_) | Target::CharRom(_))
            || (write && matches!(target, Target::Register(_)));
        if fast && needs_bus && (!*bus_free || scpu.write_buffer.is_some()) {
            return Step::Stalled;
        }
        let through = write
            && addr >> 16 == 0
            && matches!(target, Target::Sram(_))
            && scpu.mirrored(addr as u16);
        if fast && through && scpu.write_buffer.is_some() {
            return Step::Stalled;
        }

        if write {
            match target {
                Target::Sram(offset) => {
                    scpu.sram_write(offset, value);
                    if through {
                        if fast {
                            scpu.write_buffer = Some((addr as u16, value));
                        } else {
                            self.memory.ram_write(addr as u16, value);
                        }
                    }
                }
                Target::SuperRam(offset) => {
                    let windowed = matches!(addr >> 16, 0xF6 | 0xF7);
                    if (!windowed || scpu.hw_enable)
                        && let Some(byte) = scpu.superram.get_mut(offset)
                    {
                        *byte = value;
                    }
                    scpu.wait_states = SUPERRAM_WAIT;
                }
                Target::Rom(_) => scpu.wait_states = EPROM_WAIT,
                Target::Register(a) => scpu.register_write(a, value),
                Target::Io(a) => self.io_write(a, value),
                Target::CharRom(_) | Target::Open(_) => {}
            }
        } else {
            scpu.cpu.data_in = match target {
                Target::Sram(offset) => scpu.sram[offset],
                Target::Rom(offset) => {
                    scpu.wait_states = EPROM_WAIT;
                    scpu.rom[offset]
                }
                Target::SuperRam(offset) => {
                    scpu.wait_states = SUPERRAM_WAIT;
                    scpu.superram[offset]
                }
                Target::CharRom(offset) => self.memory.character_rom_byte(offset),
                Target::Io(a) => self.io_read(a),
                Target::Register(a) => scpu.register_read(a),
                Target::Open(bank) => bank,
            };
        }
        scpu.cpu.tick();
        if needs_bus {
            *bus_free = false;
            return Step::Synced;
        }
        Step::Ran
    }
}

#[cfg(test)]
mod tests {
    use super::SuperCpu;
    use crate::config::{C64Config, C64Model};
    use crate::machine::C64;

    /// Where every test program runs: bank 0 static RAM, which the boot map
    /// leaves alone.
    const ORIGIN: u16 = 0x1000;

    /// A 64 KB stand-in for the CMD ROM: the reset vector at `$FFFC` sends
    /// the 65C816 through the boot map to `JMP $1000`.
    fn rom() -> Vec<u8> {
        let mut rom = vec![0xFF; 0x1_0000];
        rom[0xE000..0xE003].copy_from_slice(&[0x4C, 0x00, 0x10]);
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0xE0;
        rom
    }

    /// A stub C64 with a SuperCPU whose static RAM holds `program` at
    /// `$1000`.
    fn machine(program: &[u8], superram: usize) -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        let mut c64 = C64::new(C64Config {
            model: C64Model::PalBreadbin,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROMs");
        c64.fit_supercpu(rom(), superram).expect("fit");
        let scpu = c64.supercpu_mut().expect("fitted");
        for (offset, &byte) in program.iter().enumerate() {
            scpu.write_sram(u32::from(ORIGIN) + offset as u32, byte);
        }
        c64
    }

    fn scpu(c64: &C64) -> &SuperCpu {
        c64.supercpu().expect("fitted")
    }

    /// The 16-bit counter the programs keep at `$10/$11`.
    fn counter(c64: &C64) -> u32 {
        u32::from(scpu(c64).sram(0x10)) | (u32::from(scpu(c64).sram(0x11)) << 8)
    }

    /// `prologue`, then a loop that runs `body` and counts at `$10/$11`.
    fn counting(prologue: &[u8], body: &[u8]) -> Vec<u8> {
        let mut program = prologue.to_vec();
        let top = ORIGIN + program.len() as u16;
        program.extend_from_slice(body);
        program.extend_from_slice(&[0xE6, 0x10]); // INC $10
        let back = i32::from(top) - (i32::from(ORIGIN) + program.len() as i32 + 2);
        program.extend_from_slice(&[0xD0, back as u8]); // BNE top
        program.extend_from_slice(&[0xE6, 0x11]); // INC $11
        program.extend_from_slice(&[0x4C, top as u8, (top >> 8) as u8]); // JMP top
        program
    }

    #[test]
    fn the_firmware_and_superram_sizes_are_checked() {
        assert!(SuperCpu::new(vec![0; 1000], 0).is_err());
        assert!(SuperCpu::new(vec![0; 0x1_0000], 3).is_err());
        assert!(SuperCpu::new(vec![0; 0x1_0000], 32 * 1024 * 1024).is_err());
        assert!(SuperCpu::new(vec![0; 0x2_0000], 16 * 1024 * 1024).is_ok());
    }

    /// The reset vector comes from the CMD ROM through the boot map, and the
    /// 6510 never runs.
    #[test]
    fn the_supercpu_boots_through_its_rom_and_holds_the_6510() {
        let mut c64 = machine(&counting(&[], &[]), 0);
        let pc_6510 = c64.cpu().regs.pc;
        c64.advance_phi2_cycles(100);
        assert!(counter(&c64) > 0, "the loop at $1000 is running");
        assert_eq!(c64.cpu().regs.pc, pc_6510, "the 6510 is held");
    }

    /// Running from static RAM, the 65C816 does about twenty times the work
    /// in a C64 cycle at 20 MHz as it does in software 1 MHz mode.
    #[test]
    fn fast_mode_runs_twenty_times_the_1mhz_rate() {
        let mut fast = machine(&counting(&[], &[]), 0);
        // STA $D07A: software 1 MHz on.
        let mut slow = machine(&counting(&[0x8D, 0x7A, 0xD0], &[]), 0);
        fast.advance_phi2_cycles(10_000);
        slow.advance_phi2_cycles(10_000);
        assert!(scpu(&fast).fast());
        assert!(!scpu(&slow).fast());
        let ratio = f64::from(counter(&fast)) / f64::from(counter(&slow));
        assert!((18.0..22.0).contains(&ratio), "ratio {ratio}");
        // INC zp and a taken BNE are 8 cycles: 1250 in 10,000 at 1 MHz.
        assert!((1200..1300).contains(&counter(&slow)), "{}", counter(&slow));
    }

    /// An I/O access waits for a C64 bus cycle and costs the 65C816 the rest
    /// of it, so a loop that reads the VIC-II once runs once per C64 cycle.
    #[test]
    fn an_io_access_slows_the_loop_to_the_c64_cycle() {
        let mut c64 = machine(&counting(&[], &[0xAD, 0x20, 0xD0]), 0); // LDA $D020
        c64.advance_phi2_cycles(10_000);
        let count = counter(&c64);
        assert!((9_900..=10_000).contains(&count), "{count}");
    }

    /// The default mode mirrors `$0200-$FFFF`: a store to the screen reaches
    /// C64 RAM once the write buffer drains, a store to zero page does not.
    #[test]
    fn the_default_mode_writes_through_above_page_one() {
        let program = [
            0xA9, 0x42, //       LDA #$42
            0x8D, 0x00, 0x04, // STA $0400
            0x85, 0x20, //       STA $20
            0x4C, 0x07, 0x10, // JMP *
        ];
        let mut c64 = machine(&program, 0);
        c64.advance_phi2_cycles(20);
        assert_eq!(scpu(&c64).sram(0x0400), 0x42);
        assert_eq!(c64.memory().ram_read(0x0400), 0x42, "written through");
        assert_eq!(scpu(&c64).sram(0x20), 0x42);
        assert_ne!(c64.memory().ram_read(0x20), 0x42, "zero page stays cached");
    }

    /// `$D074`, VIC bank 2 optimisation, mirrors only `$8000-$BFFF`, and
    /// only once `$D07E` has unlocked the registers.
    #[test]
    fn the_optimisation_mode_selects_the_mirrored_pages() {
        let program = [
            0x8D, 0x74, 0xD0, // STA $D074 (locked: ignored)
            0xA9, 0x11, //       LDA #$11
            0x8D, 0x00, 0x04, // STA $0400
            0x8D, 0x7E, 0xD0, // STA $D07E (unlock)
            0x8D, 0x74, 0xD0, // STA $D074 (VIC bank 2 mode)
            0xA9, 0x22, //       LDA #$22
            0x8D, 0x00, 0x05, // STA $0500
            0x8D, 0x00, 0x80, // STA $8000
            0x4C, 0x16, 0x10, // JMP *
        ];
        let mut c64 = machine(&program, 0);
        c64.advance_phi2_cycles(50);
        assert_eq!(c64.memory().ram_read(0x0400), 0x11, "default mode still");
        assert_eq!(scpu(&c64).optimisation() & 0xC0, 0x00);
        assert_eq!(scpu(&c64).sram(0x0500), 0x22);
        assert_ne!(c64.memory().ram_read(0x0500), 0x22, "outside VIC bank 2");
        assert_eq!(c64.memory().ram_read(0x8000), 0x22, "inside VIC bank 2");
    }

    /// One write-through can wait in the buffer; a second waits for the
    /// first to drain, so a loop that stores to the screen runs at most once
    /// per C64 cycle.
    #[test]
    fn back_to_back_write_throughs_wait_for_the_buffer() {
        let mut c64 = machine(&counting(&[], &[0x8D, 0x00, 0x04]), 0); // STA $0400
        c64.advance_phi2_cycles(10_000);
        let count = counter(&c64);
        assert!((9_900..=10_001).contains(&count), "{count}");
    }

    /// The registers: the version byte, the unlock bit, system 1 MHz and
    /// the speed it reports, and the boot map going off.
    #[test]
    fn the_registers_read_back() {
        let program = [
            0xAD, 0xB0, 0xD0, // LDA $D0B0
            0x85, 0x30, //       STA $30
            0x8D, 0x7E, 0xD0, // STA $D07E (unlock)
            0xAD, 0xB2, 0xD0, // LDA $D0B2
            0x85, 0x31, //       STA $31
            0x8D, 0x72, 0xD0, // STA $D072 (system 1 MHz on)
            0xAD, 0xB8, 0xD0, // LDA $D0B8
            0x85, 0x32, //       STA $32
            0x8D, 0xB6, 0xD0, // STA $D0B6 (boot map off)
            0x8D, 0x7F, 0xD0, // STA $D07F (lock)
            0x4C, 0x1B, 0x10, // JMP *
        ];
        let mut c64 = machine(&program, 0);
        c64.advance_phi2_cycles(200);
        let scpu = scpu(&c64);
        assert_eq!(
            scpu.sram(0x30),
            0x47,
            "version 2, with the reset optimisation bits"
        );
        assert_eq!(scpu.sram(0x31) & 0x80, 0x80, "unlocked");
        assert_eq!(scpu.sram(0x32) & 0x40, 0x40, "running at 1 MHz");
        assert!(!scpu.fast());
        assert!(!scpu.bootmap());
        assert!(!scpu.hw_enabled());
    }

    /// With the boot map off, `$E000` reads the KERNAL shadow in bank 1
    /// rather than the CMD ROM.
    #[test]
    fn without_the_boot_map_the_kernal_comes_from_bank_1() {
        let program = [
            0x8D, 0x7E, 0xD0, // STA $D07E
            0x8D, 0xB6, 0xD0, // STA $D0B6 (boot map off)
            0x8D, 0x7F, 0xD0, // STA $D07F
            0xAD, 0x00, 0xE0, // LDA $E000
            0x85, 0x40, //       STA $40
            0x4C, 0x0E, 0x10, // JMP *
        ];
        let mut c64 = machine(&program, 0);
        c64.supercpu_mut()
            .expect("fitted")
            .write_sram(0x1_E000, 0x5A);
        c64.advance_phi2_cycles(100);
        assert_eq!(scpu(&c64).sram(0x40), 0x5A);
    }

    /// SuperRAM answers from `$020000` with long addressing.
    #[test]
    fn superram_is_reached_with_long_addressing() {
        let program = [
            0xA9, 0x99, //             LDA #$99
            0x8F, 0x34, 0x12, 0x02, // STA $021234
            0xAF, 0x34, 0x12, 0x02, // LDA $021234
            0x85, 0x50, //             STA $50
            0x4C, 0x0C, 0x10, //       JMP *
        ];
        let mut c64 = machine(&program, 1024 * 1024);
        c64.advance_phi2_cycles(20);
        assert_eq!(scpu(&c64).superram(0x2_1234), 0x99);
        assert_eq!(scpu(&c64).sram(0x50), 0x99);
    }

    #[test]
    fn the_supercpu_survives_a_snapshot() {
        let mut c64 = machine(&counting(&[], &[]), 0);
        c64.advance_phi2_cycles(500);
        let snapshot = c64.snapshot_state();
        let count = counter(&c64);
        c64.advance_phi2_cycles(500);
        let mut restored = machine(&[], 0);
        restored.restore_snapshot_state(snapshot).expect("restore");
        assert_eq!(counter(&restored), count);
        restored.advance_phi2_cycles(500);
        assert_eq!(counter(&restored), counter(&c64), "resumes identically");
    }
}
