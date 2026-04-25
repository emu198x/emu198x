//! iNES / NES 2.0 cartridge header parser and the [`Mapper`] trait.
//!
//! # Scope of this port
//!
//! The archive crate
//! (`Emu198x-archive/crates/format-nintendo-nes-ines`) implemented 48
//! mapper variants covering virtually every licensed NES/Famicom
//! game. This port currently carries **Mapper 0 (NROM)**,
//! **Mapper 1 (MMC1)**, **Mapper 2 (UxROM)**,
//! **Mapper 3 (CNROM)**, **Mapper 4 (MMC3)**, and
//! **Mapper 7 (AxROM)**, plus **Mapper 34 (BxROM/BNROM)** — the
//! trait, the header parser, and the first mappers needed to boot
//! flat-layout test ROMs plus common PRG/CHR-bank-switched
//! cartridges.
//!
//! The remaining mappers are archive-provenance (see
//! [archives-as-source.md](../../wiki/decisions/archives-as-source.md))
//! and will be lifted one at a time *once the PPU crate is back
//! online*, because there is no point porting address-translation
//! logic with no bus for the translated addresses to serve.
//!
//! # Scope of the `Mapper` trait
//!
//! The trait defined here is intentionally **leaner** than the
//! archive version. It carries the CPU/CHR bus methods, mirroring,
//! IRQ pending, and the MMC3 A12 notifier — everything the
//! [nes-clock-topology.md](../../wiki/decisions/nes-clock-topology.md)
//! decision record says the machine layer and the (future) PPU need
//! to call. It drops the archive's save-state, peek-chr, expansion
//! audio, and PRG-RAM accessor methods; those are features of
//! higher-layer mappers (MMC3, Sunsoft 5B, VRC6) and have no callers
//! yet. They will land back in the trait as default methods when the
//! mappers that need them get ported.
//!
//! # Mirroring
//!
//! [`Mirroring`] is defined here rather than re-exported from a PPU
//! crate because the PPU is not yet ported. When `ricoh-ppu-2c02` is
//! rewritten in the dot-driven architecture, the canonical
//! `Mirroring` will live in that crate and this one will re-export
//! it — matching the archive's shape. The enum is small enough
//! (five variants) that re-defining it here is cheap and the future
//! reconciliation will be a one-line re-export change.

#![allow(clippy::cast_possible_truncation)]

// ─── Mirroring ─────────────────────────────────────────────────────

/// Nametable mirroring mode.
///
/// Determined by the cartridge, not the PPU. The PPU queries the
/// mapper on every nametable access (`$2000-$2FFF`) to find which
/// physical nametable a given logical address should route to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    /// A-A / B-B — both horizontal strips share a nametable. Games
    /// with vertical scrolling (e.g. *Ice Climber*) use this.
    Horizontal,
    /// A-B / A-B — both vertical strips share a nametable. Games
    /// with horizontal scrolling (e.g. *Super Mario Bros.*) use this.
    Vertical,
    /// Four unique nametables — requires cartridge VRAM on top of
    /// the PPU's 2 KiB. Used by *Gauntlet* and a few others.
    FourScreen,
    /// All four logical nametables point at the lower physical
    /// bank. Set by MMC1 on power-up and via control register.
    SingleScreenLower,
    /// All four logical nametables point at the upper physical
    /// bank. MMC1 control register.
    SingleScreenUpper,
}

// ─── Mapper trait ──────────────────────────────────────────────────

/// Cartridge mapper: translates CPU addresses in `$4020-$FFFF` and
/// PPU addresses in `$0000-$1FFF` to ROM, RAM, or bank-switched
/// memory on the cartridge.
///
/// Implementations are per-mapper-number. The parser in
/// [`parse_ines`] inspects the iNES header's mapper field and
/// constructs the right concrete type. This port carries [`Nrom`]
/// (mapper 0), [`Mmc1`] (mapper 1), [`UxRom`] (mapper 2),
/// [`CnRom`] (mapper 3), [`Mmc3`] (mapper 4), and [`AxRom`]
/// (mapper 7), plus [`BxRom`] (mapper 34).
///
/// ## Design notes
///
/// - `chr_read` takes `&mut self` because some mappers (MMC2, MMC4)
///   update internal latches when the PPU reads from pattern table
///   addresses. NROM ignores the `&mut` but the trait keeps the
///   method signature uniform across all mappers.
///
/// - `irq_pending()` is the mapper's IRQ output pin, polled by the
///   machine layer once per CPU cycle and OR'd into the CPU's
///   `irq` input. Default returns `false` — most mappers don't do
///   IRQ.
///
/// - `notify_a12_rendering` is the MMC3 IRQ counter hook. Called
///   from inside the PPU tick when the PPU address bus transitions
///   A12 during background or sprite fetches. See
///   [nes-clock-topology.md](../../wiki/decisions/nes-clock-topology.md#pin-contracts)
///   for the rationale.
pub trait Mapper: Send {
    /// CPU-side bus read. Called by the machine layer's `cpu_read`
    /// for addresses in `$4020-$FFFF`. Returns the byte the
    /// cartridge would drive onto the CPU data bus.
    fn cpu_read(&self, addr: u16) -> u8;

    /// CPU-side bus write. Called by the machine layer's
    /// `cpu_write` for addresses in `$4020-$FFFF`. The mapper
    /// decides whether to latch the value (bank switching), write
    /// to PRG RAM, or ignore.
    fn cpu_write(&mut self, addr: u16, value: u8);

    /// PPU-side bus read for pattern table addresses
    /// (`$0000-$1FFF`). `&mut self` is required for mappers with
    /// read-side-effect latches (MMC2, MMC4).
    fn chr_read(&mut self, addr: u16) -> u8;

    /// PPU-side bus write for pattern table addresses
    /// (`$0000-$1FFF`). Ignored for CHR ROM cartridges; writes CHR
    /// RAM on cartridges without CHR ROM.
    fn chr_write(&mut self, addr: u16, value: u8);

    /// Current nametable mirroring mode. Queried by the PPU on
    /// every nametable access — mappers may change this on the fly
    /// (MMC1, MMC3) but NROM does not.
    fn mirroring(&self) -> Mirroring;

    /// Level-triggered IRQ output. Default: never asserted.
    ///
    /// The machine layer ORs this with other IRQ sources (e.g. APU
    /// frame IRQ, DMC IRQ) and drives the CPU's `irq` input.
    fn irq_pending(&self) -> bool {
        false
    }

    /// MMC3 IRQ counter hook. Called from inside the PPU tick when
    /// the PPU address bus A12 line changes during background or
    /// sprite fetches. The mapper applies its own debounce filter
    /// (MMC3 ignores transitions < 15 dots apart).
    ///
    /// Default: no-op. NROM has no IRQ counter.
    fn notify_a12_rendering(&mut self, _a12_high: bool) {}
}

// ─── NROM (Mapper 0) ───────────────────────────────────────────────

/// NROM (Mapper 0): no bank switching.
///
/// The simplest cartridge: 16 KiB or 32 KiB of PRG ROM wired
/// directly to `$8000-$FFFF`, and 8 KiB of CHR (ROM or RAM) wired
/// directly to `$0000-$1FFF` on the PPU bus. Used by *Super Mario
/// Bros.*, *Donkey Kong*, *Ice Climber*, *Excitebike*, *Balloon
/// Fight*, and most of Nintendo's first-party launch titles.
///
/// ## Memory map
///
/// - `$6000-$7FFF` — 8 KiB work RAM. Many test ROMs (blargg's) use
///   NROM and write their results to this region, so the port
///   carries the RAM even though *Super Mario Bros.* doesn't touch
///   it.
/// - `$8000-$BFFF` — first 16 KiB of PRG ROM.
/// - `$C000-$FFFF` — second 16 KiB of PRG ROM for 32 KiB carts, or
///   a mirror of `$8000-$BFFF` for 16 KiB carts.
/// - PPU `$0000-$1FFF` — 8 KiB CHR ROM, or 8 KiB CHR RAM if the
///   iNES header reports zero CHR banks.
pub struct Nrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_ram: [u8; 8192],
}

impl Nrom {
    /// Construct an NROM from the parsed iNES payload.
    ///
    /// `chr_data` is the raw CHR ROM bytes from the iNES file; pass
    /// an empty `Vec` for a CHR-RAM cartridge (8 KiB of writable
    /// RAM will be allocated).
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
            prg_ram: [0; 8192],
        }
    }
}

impl Mapper for Nrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                if self.prg_rom.len() == 16384 {
                    // 16 KiB cart — mirror $8000-$BFFF to
                    // $C000-$FFFF.
                    self.prg_rom[offset % 16384]
                } else {
                    // 32 KiB cart — direct mapping (modulo for
                    // safety against malformed headers).
                    self.prg_rom[offset % self.prg_rom.len()]
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.prg_ram[(addr - 0x6000) as usize] = value;
        }
        // Writes to $8000-$FFFF are ignored on NROM — there is no
        // bank-switching register to latch into.
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// ─── MMC1 (Mapper 1) ───────────────────────────────────────────────

/// MMC1 (Mapper 1, SxROM): serial-register PRG/CHR banking.
///
/// CPU writes load a 5-bit shift register one bit at a time. Once
/// complete, the address selects one of four internal registers:
/// control, CHR bank 0, CHR bank 1, or PRG bank. This supports MMC1's
/// 16 KiB and 32 KiB PRG modes, 4 KiB and 8 KiB CHR modes, dynamic
/// nametable mirroring, and the standard 8 KiB PRG-RAM window.
pub struct Mmc1 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    shift_register: u8,
    shift_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

impl Mmc1 {
    /// Construct MMC1 from parsed iNES payloads.
    ///
    /// `chr_data` is empty for CHR-RAM cartridges; in that case this
    /// allocates the standard 8 KiB CHR RAM window.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            shift_register: 0,
            shift_count: 0,
            control: 0x0C,
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }

    fn read_prg(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_bank_count();
        self.prg_rom[bank * 16384 + offset]
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            self.shift_register = 0;
            self.shift_count = 0;
            self.control |= 0x0C;
            return;
        }

        self.shift_register |= (value & 1) << self.shift_count;
        self.shift_count += 1;

        if self.shift_count == 5 {
            let data = self.shift_register;
            match (addr >> 13) & 0x03 {
                0 => self.control = data,
                1 => self.chr_bank_0 = data,
                2 => self.chr_bank_1 = data,
                3 => self.prg_bank = data,
                _ => unreachable!(),
            }
            self.shift_register = 0;
            self.shift_count = 0;
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let addr = usize::from(addr) & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;
        if chr_mode == 0 {
            let bank_base = (usize::from(self.chr_bank_0) & 0x1E) * 4096;
            (bank_base + addr) % self.chr.len()
        } else {
            let bank = if addr < 0x1000 {
                self.chr_bank_0
            } else {
                self.chr_bank_1
            };
            let offset = addr & 0x0FFF;
            (usize::from(bank) * 4096 + offset) % self.chr.len()
        }
    }
}

impl Mapper for Mmc1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xBFFF => {
                let offset = usize::from(addr - 0x8000);
                match (self.control >> 2) & 0x03 {
                    0 | 1 => self.read_prg(usize::from(self.prg_bank & 0x0E), offset),
                    2 => self.read_prg(0, offset),
                    3 => self.read_prg(usize::from(self.prg_bank & 0x0F), offset),
                    _ => unreachable!(),
                }
            }
            0xC000..=0xFFFF => {
                let offset = usize::from(addr - 0xC000);
                match (self.control >> 2) & 0x03 {
                    0 | 1 => self.read_prg(usize::from(self.prg_bank & 0x0E) + 1, offset),
                    2 => self.read_prg(usize::from(self.prg_bank & 0x0F), offset),
                    3 => self.read_prg(self.prg_bank_count() - 1, offset),
                    _ => unreachable!(),
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            0x8000..=0xFFFF => self.write_register(addr, value),
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let index = self.chr_index(addr);
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        }
    }
}

// ─── UxROM (Mapper 2) ──────────────────────────────────────────────

/// UxROM (Mapper 2): one switchable 16 KiB PRG bank and one fixed
/// 16 KiB PRG bank.
///
/// This common discrete-logic board family maps `$8000-$BFFF` to a
/// CPU-selected PRG bank and `$C000-$FFFF` to the final PRG bank.
/// Most UxROM cartridges use 8 KiB of CHR RAM; CHR ROM is also accepted
/// because the mapper trait can serve either layout.
pub struct UxRom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank: u8,
}

impl UxRom {
    /// Construct UxROM from parsed iNES payloads.
    ///
    /// `chr_data` is empty for CHR-RAM cartridges; in that case this
    /// allocates the standard 8 KiB CHR RAM window.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }
}

impl Mapper for UxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = usize::from(self.prg_bank) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let bank = self.prg_bank_count() - 1;
                let offset = usize::from(addr - 0xC000);
                self.prg_rom[bank * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Discrete UxROM boards have bus conflicts: the value
            // latched by the bank-select register is the CPU value
            // AND the ROM byte simultaneously driving the bus.
            let rom_byte = self.cpu_read(addr);
            self.prg_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[usize::from(addr) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[usize::from(addr) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// ─── CNROM (Mapper 3) ──────────────────────────────────────────────

/// CNROM (Mapper 3): fixed PRG ROM with switchable 8 KiB CHR ROM.
///
/// CNROM keeps PRG ROM unbanked at `$8000-$FFFF` and uses writes to
/// `$8000-$FFFF` to select the 8 KiB CHR bank visible to the PPU at
/// `$0000-$1FFF`. Most boards have bus conflicts, so the latched bank
/// value is the CPU value AND the ROM byte driving the bus.
pub struct CnRom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank: u8,
}

impl CnRom {
    /// Construct CNROM from parsed iNES payloads.
    ///
    /// CNROM is a CHR-ROM board. If a malformed image declares no CHR
    /// ROM, this allocates a zeroed 8 KiB bank so reads remain defined
    /// rather than panicking.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_rom = if chr_data.is_empty() {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            chr_bank: 0,
        }
    }
}

impl Mapper for CnRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[offset % self.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let rom_byte = self.cpu_read(addr);
            self.chr_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = usize::from(self.chr_bank) * 8192;
        let offset = usize::from(addr) & 0x1FFF;
        self.chr_rom[(bank_offset + offset) % self.chr_rom.len()]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // CNROM has CHR ROM, not CHR RAM.
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// ─── MMC3 (Mapper 4) ───────────────────────────────────────────────

/// MMC3 (Mapper 4, TxROM): 8 KiB PRG banking, 1 KiB CHR banking,
/// PRG RAM protection, dynamic mirroring, and scanline IRQs.
///
/// MMC3 is used by a large part of the later NES library, including
/// *Super Mario Bros. 3*. The IRQ counter is clocked by debounced PPU
/// A12 rising edges reported through [`Mapper::notify_a12_rendering`].
pub struct Mmc3 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    bank_select: u8,
    registers: [u8; 8],
    mirroring: Mirroring,
    prg_ram_enable: bool,
    prg_ram_write_protect: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
    dots_since_last_a12_rise: u16,
}

impl Mmc3 {
    /// Construct MMC3 from parsed iNES payloads.
    ///
    /// Empty CHR data means CHR RAM, allocated as an 8 KiB window.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            bank_select: 0,
            registers: [0; 8],
            mirroring: Mirroring::Vertical,
            prg_ram_enable: true,
            prg_ram_write_protect: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload_flag: false,
            irq_enabled: false,
            irq_pending: false,
            last_a12: false,
            dots_since_last_a12_rise: 0,
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn second_last_prg_bank(&self) -> usize {
        self.prg_8k_count().saturating_sub(2)
    }

    fn last_prg_bank(&self) -> usize {
        self.prg_8k_count() - 1
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn chr_1k_bank(&self, addr: u16) -> usize {
        let slot = (usize::from(addr) & 0x1FFF) >> 10;
        if self.bank_select & 0x80 != 0 {
            match slot {
                0 => usize::from(self.registers[2]),
                1 => usize::from(self.registers[3]),
                2 => usize::from(self.registers[4]),
                3 => usize::from(self.registers[5]),
                4 => usize::from(self.registers[0] & 0xFE),
                5 => usize::from(self.registers[0] | 1),
                6 => usize::from(self.registers[1] & 0xFE),
                7 => usize::from(self.registers[1] | 1),
                _ => unreachable!(),
            }
        } else {
            match slot {
                0 => usize::from(self.registers[0] & 0xFE),
                1 => usize::from(self.registers[0] | 1),
                2 => usize::from(self.registers[1] & 0xFE),
                3 => usize::from(self.registers[1] | 1),
                4 => usize::from(self.registers[2]),
                5 => usize::from(self.registers[3]),
                6 => usize::from(self.registers[4]),
                7 => usize::from(self.registers[5]),
                _ => unreachable!(),
            }
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let offset = usize::from(addr) & 0x03FF;
        (self.chr_1k_bank(addr) * 1024 + offset) % self.chr.len()
    }

    fn update_a12(&mut self, a12_high: bool) {
        if a12_high && !self.last_a12 {
            if self.dots_since_last_a12_rise >= 15 {
                self.clock_irq_counter();
            }
            self.dots_since_last_a12_rise = 0;
        } else {
            self.dots_since_last_a12_rise = self.dots_since_last_a12_rise.saturating_add(1);
        }
        self.last_a12 = a12_high;
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0 || self.irq_reload_flag {
            self.irq_counter = self.irq_latch;
            self.irq_reload_flag = false;
        } else {
            self.irq_counter -= 1;
        }

        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
}

impl Mapper for Mmc3 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable {
                    self.prg_ram[usize::from(addr - 0x6000)]
                } else {
                    0
                }
            }
            0x8000..=0x9FFF => {
                let offset = usize::from(addr - 0x8000);
                if self.bank_select & 0x40 == 0 {
                    self.read_prg_8k(usize::from(self.registers[6] & 0x3F), offset)
                } else {
                    self.read_prg_8k(self.second_last_prg_bank(), offset)
                }
            }
            0xA000..=0xBFFF => {
                let offset = usize::from(addr - 0xA000);
                self.read_prg_8k(usize::from(self.registers[7] & 0x3F), offset)
            }
            0xC000..=0xDFFF => {
                let offset = usize::from(addr - 0xC000);
                if self.bank_select & 0x40 == 0 {
                    self.read_prg_8k(self.second_last_prg_bank(), offset)
                } else {
                    self.read_prg_8k(usize::from(self.registers[6] & 0x3F), offset)
                }
            }
            0xE000..=0xFFFF => {
                let offset = usize::from(addr - 0xE000);
                self.read_prg_8k(self.last_prg_bank(), offset)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable && !self.prg_ram_write_protect {
                    self.prg_ram[usize::from(addr - 0x6000)] = value;
                }
            }
            0x8000..=0x9FFF if addr & 1 == 0 => self.bank_select = value,
            0x8000..=0x9FFF => {
                let register = usize::from(self.bank_select & 0x07);
                self.registers[register] = value;
            }
            0xA000..=0xBFFF if addr & 1 == 0 => {
                self.mirroring = if value & 1 == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            0xA000..=0xBFFF => {
                self.prg_ram_write_protect = value & 0x40 != 0;
                self.prg_ram_enable = value & 0x80 != 0;
            }
            0xC000..=0xDFFF if addr & 1 == 0 => self.irq_latch = value,
            0xC000..=0xDFFF => self.irq_reload_flag = true,
            0xE000..=0xFFFF if addr & 1 == 0 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE000..=0xFFFF => self.irq_enabled = true,
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let index = self.chr_index(addr);
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn notify_a12_rendering(&mut self, a12_high: bool) {
        self.update_a12(a12_high);
    }
}

// ─── AxROM (Mapper 7) ──────────────────────────────────────────────

/// AxROM (Mapper 7): switchable 32 KiB PRG bank with single-screen
/// mirroring.
///
/// AxROM boards use CHR RAM and switch the whole CPU `$8000-$FFFF`
/// PRG window at once. Bit 4 of the latched bank register selects
/// lower vs upper single-screen nametable mirroring.
pub struct AxRom {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    bank: u8,
    mirroring: Mirroring,
}

impl AxRom {
    /// Construct AxROM from parsed PRG ROM. CHR is always 8 KiB RAM.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            bank: 0,
            mirroring: Mirroring::SingleScreenLower,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }
}

impl Mapper for AxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = (usize::from(self.bank) & 0x07) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 32768 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = value & self.cpu_read(addr);
            self.bank = effective & 0x07;
            self.mirroring = if effective & 0x10 != 0 {
                Mirroring::SingleScreenUpper
            } else {
                Mirroring::SingleScreenLower
            };
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[usize::from(addr) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[usize::from(addr) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// ─── BxROM / BNROM (Mapper 34) ─────────────────────────────────────

/// BxROM / BNROM (Mapper 34): switchable 32 KiB PRG bank with CHR RAM.
///
/// The iNES mapper 34 assignment is historically ambiguous; this
/// implementation covers the common BNROM/BxROM layout, not the
/// NINA-001 CHR-ROM banking variant.
pub struct BxRom {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl BxRom {
    /// Construct BxROM from parsed PRG ROM and fixed header mirroring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 32768).max(1)
    }
}

impl Mapper for BxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = usize::from(self.prg_bank) % self.prg_bank_count();
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 32768 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & self.cpu_read(addr);
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[usize::from(addr) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[usize::from(addr) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

// ─── iNES header + parser ──────────────────────────────────────────

/// Parsed iNES header fields. Both iNES 1.0 and NES 2.0 files map
/// onto this struct; the NES 2.0 upper nibbles are folded into the
/// 12-bit `mapper_number` and the extended PRG/CHR sizes are
/// reported via the byte counts the parser passes to [`Nrom::new`].
#[derive(Debug, Clone, Copy)]
pub struct CartridgeHeader {
    /// Number of 16 KiB PRG ROM banks as reported by byte 4 of the
    /// header. For NES 2.0 the full size is in bytes.
    pub prg_rom_banks: u8,
    /// Number of 8 KiB CHR ROM banks. Zero means the cartridge has
    /// CHR RAM instead.
    pub chr_rom_banks: u8,
    /// 12-bit mapper number (iNES 1.0 uses only the low 8 bits).
    pub mapper_number: u16,
    /// Mirroring mode derived from header flag bits.
    pub mirroring: Mirroring,
    /// Whether the cartridge reports battery-backed PRG RAM.
    pub has_battery: bool,
}

/// Result of a successful [`parse_ines`] call.
///
/// The `mapper` field is boxed because different cartridges use
/// different mapper types; the machine layer only ever interacts
/// with the trait.
pub struct ParsedCartridge {
    /// The constructed mapper, ready to be plugged into the machine
    /// layer's `cpu_read`/`cpu_write`/`chr_read`/`chr_write`
    /// routers.
    pub mapper: Box<dyn Mapper>,
    /// Mirror of [`CartridgeHeader::has_battery`] — hoisted here
    /// because the machine layer needs it to decide whether to
    /// look for a `.sav` file.
    pub has_battery: bool,
    /// Full header, hoisted so callers can introspect the mapper
    /// number, CHR bank count, etc. without having to re-parse.
    pub header: CartridgeHeader,
}

/// Parse an iNES / NES 2.0 file and return a [`ParsedCartridge`]
/// ready to drive the NES machine layer.
///
/// # Errors
///
/// Returns a human-readable error string if:
/// - The file is shorter than 16 bytes (no header).
/// - The magic bytes are not `NES\x1A`.
/// - The declared PRG/CHR size exceeds the file's actual length.
/// - The mapper number is not supported by this port — see the
///   crate-level doc comment for the current scope.
pub fn parse_ines(data: &[u8]) -> Result<ParsedCartridge, String> {
    if data.len() < 16 {
        return Err("iNES file too short (< 16 bytes)".to_string());
    }
    if &data[0..4] != b"NES\x1a" {
        return Err("Invalid iNES magic (expected NES\\x1A)".to_string());
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];

    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;

    // NES 2.0 detection: bits 3-2 of flags7 == 0b10.
    let is_nes_2_0 = (flags7 & 0x0C) == 0x08;

    let (mapper_number, prg_size, chr_size) = if is_nes_2_0 {
        // NES 2.0: 12-bit mapper number + extended 12-bit bank
        // counts (byte 9 holds the high nibbles of each).
        let mapper8 = data[8];
        let mapper_number =
            u16::from(mapper_lo) | u16::from(mapper_hi) | (u16::from(mapper8 & 0x0F) << 8);

        let prg_hi = usize::from(data[9] & 0x0F);
        let prg_size = ((prg_hi << 8) | usize::from(prg_banks)) * 16384;

        let chr_hi = usize::from((data[9] >> 4) & 0x0F);
        let chr_size = ((chr_hi << 8) | usize::from(chr_banks)) * 8192;

        (mapper_number, prg_size, chr_size)
    } else {
        // iNES 1.0: 8-bit mapper number.
        let mapper_number = u16::from(mapper_hi | mapper_lo);
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        (mapper_number, prg_size, chr_size)
    };

    let mirroring = if flags6 & 0x08 != 0 {
        Mirroring::FourScreen
    } else if flags6 & 0x01 != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let has_battery = flags6 & 0x02 != 0;
    let has_trainer = flags6 & 0x04 != 0;

    let header = CartridgeHeader {
        prg_rom_banks: prg_banks,
        chr_rom_banks: chr_banks,
        mapper_number,
        mirroring,
        has_battery,
    };

    // iNES layout: [16-byte header][optional 512-byte trainer][PRG ROM][CHR ROM].
    let prg_start = if has_trainer { 16 + 512 } else { 16 };
    let chr_start = prg_start + prg_size;

    if data.len() < chr_start + chr_size {
        return Err(format!(
            "iNES file too short: expected {} bytes, got {}",
            chr_start + chr_size,
            data.len()
        ));
    }

    let prg_rom = data[prg_start..prg_start + prg_size].to_vec();
    let chr_data = if chr_size > 0 {
        data[chr_start..chr_start + chr_size].to_vec()
    } else {
        Vec::new() // CHR RAM cartridge
    };

    let mapper: Box<dyn Mapper> = match header.mapper_number {
        0 => Box::new(Nrom::new(prg_rom, chr_data, mirroring)),
        1 => Box::new(Mmc1::new(prg_rom, chr_data)),
        2 => Box::new(UxRom::new(prg_rom, chr_data, mirroring)),
        3 => Box::new(CnRom::new(prg_rom, chr_data, mirroring)),
        4 => Box::new(Mmc3::new(prg_rom, chr_data)),
        7 => Box::new(AxRom::new(prg_rom)),
        34 => Box::new(BxRom::new(prg_rom, mirroring)),
        n => {
            return Err(format!(
                "Unsupported mapper: {n} — this port currently carries Mapper 0 \
                 (NROM), Mapper 1 (MMC1), Mapper 2 (UxROM), and Mapper 3 \
                 (CNROM), Mapper 4 (MMC3), Mapper 7 (AxROM), and Mapper 34 \
                 (BxROM/BNROM). Additional mappers will land as compatibility \
                 expands."
            ));
        }
    };

    Ok(ParsedCartridge {
        mapper,
        has_battery,
        header,
    })
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal iNES 1.0 file with the given bank counts and
    /// flags6. PRG bytes are filled with their offset mod 256 so
    /// tests can identify which byte was served by a read.
    fn make_ines(prg_banks: u8, chr_banks: u8, flags6: u8) -> Vec<u8> {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_banks;
        data[5] = chr_banks;
        data[6] = flags6;
        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn parse_valid_nrom_16k() {
        let data = make_ines(1, 1, 0x00);
        let parsed = parse_ines(&data).expect("parse failed");
        assert_eq!(parsed.header.mapper_number, 0);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
        // PRG at $8000 is the first byte of the ROM.
        assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
        // 16 KiB cart: $C000 mirrors $8000.
        assert_eq!(parsed.mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn parse_valid_nrom_32k() {
        let data = make_ines(2, 1, 0x01);
        let parsed = parse_ines(&data).expect("parse failed");
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
        assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
        // 32 KiB cart: $C000 starts the second 16 KiB. Offset
        // 0x4000 mod 256 == 0.
        assert_eq!(parsed.mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn nrom_16k_mirrors_high_half() {
        // 16 KiB cart, distinct PRG bytes: confirm $C001 mirrors
        // $8001 (both return 0x01 from the offset-fill pattern).
        let data = make_ines(1, 1, 0x00);
        let mapper = parse_ines(&data).expect("parse failed").mapper;
        assert_eq!(mapper.cpu_read(0x8001), 0x01);
        assert_eq!(mapper.cpu_read(0xC001), 0x01);
    }

    #[test]
    fn nrom_cpu_write_prg_ram_roundtrip() {
        let data = make_ines(1, 1, 0x00);
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
        mapper.cpu_write(0x6123, 0x42);
        assert_eq!(mapper.cpu_read(0x6123), 0x42);
    }

    #[test]
    fn nrom_prg_rom_not_writable() {
        let data = make_ines(1, 1, 0x00);
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
        let before = mapper.cpu_read(0x8000);
        mapper.cpu_write(0x8000, 0xFF);
        assert_eq!(mapper.cpu_read(0x8000), before);
    }

    #[test]
    fn nrom_chr_ram_roundtrip() {
        let data = make_ines(1, 0, 0x00); // CHR RAM (chr_banks == 0)
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
        assert_eq!(mapper.chr_read(0x0000), 0);
        mapper.chr_write(0x0000, 0xAB);
        assert_eq!(mapper.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn nrom_chr_rom_not_writable() {
        let data = make_ines(1, 1, 0x00);
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
        let before = mapper.chr_read(0x0000);
        mapper.chr_write(0x0000, 0xFF);
        assert_eq!(mapper.chr_read(0x0000), before);
    }

    #[test]
    fn nrom_default_irq_not_pending() {
        let data = make_ines(1, 1, 0x00);
        let mapper = parse_ines(&data).expect("parse failed").mapper;
        assert!(!mapper.irq_pending());
    }

    fn make_mmc1(prg_banks: u8, chr_banks: u8) -> Mmc1 {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..usize::from(prg_banks) {
            for byte in &mut prg_rom[bank * 16384..(bank + 1) * 16384] {
                *byte = bank as u8;
            }
        }
        let chr_data = if chr_size > 0 {
            let mut chr = vec![0u8; chr_size];
            for page in 0..chr_size / 4096 {
                for byte in &mut chr[page * 4096..(page + 1) * 4096] {
                    *byte = page as u8;
                }
            }
            chr
        } else {
            Vec::new()
        };
        Mmc1::new(prg_rom, chr_data)
    }

    fn mmc1_write_5(mapper: &mut Mmc1, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_write(addr, (value >> bit) & 1);
        }
    }

    #[test]
    fn parse_valid_mmc1() {
        let data = make_ines(8, 2, 0x10);
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 1);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn mmc1_reset_write_clears_shift_register_and_sets_prg_mode_3() {
        let mut mapper = make_mmc1(8, 2);
        mapper.cpu_write(0x8000, 1);
        mapper.cpu_write(0x8000, 0);

        mapper.cpu_write(0x8000, 0x80);

        assert_eq!(mapper.shift_count, 0);
        assert_eq!(mapper.shift_register, 0);
        assert_eq!((mapper.control >> 2) & 0x03, 3);
    }

    #[test]
    fn mmc1_loads_registers_lsb_first() {
        let mut mapper = make_mmc1(8, 2);

        mmc1_write_5(&mut mapper, 0x8000, 0b10101);
        mmc1_write_5(&mut mapper, 0xA000, 3);
        mmc1_write_5(&mut mapper, 0xC000, 5);
        mmc1_write_5(&mut mapper, 0xE000, 2);

        assert_eq!(mapper.control, 0b10101);
        assert_eq!(mapper.chr_bank_0, 3);
        assert_eq!(mapper.chr_bank_1, 5);
        assert_eq!(mapper.prg_bank, 2);
    }

    #[test]
    fn mmc1_prg_mode_3_switches_low_and_fixes_last_bank() {
        let mut mapper = make_mmc1(8, 0);

        mmc1_write_5(&mut mapper, 0xE000, 2);

        assert_eq!(mapper.cpu_read(0x8000), 2);
        assert_eq!(mapper.cpu_read(0xC000), 7);
    }

    #[test]
    fn mmc1_prg_mode_2_fixes_first_and_switches_high_bank() {
        let mut mapper = make_mmc1(8, 0);

        mmc1_write_5(&mut mapper, 0x8000, 0b01000);
        mmc1_write_5(&mut mapper, 0xE000, 5);

        assert_eq!(mapper.cpu_read(0x8000), 0);
        assert_eq!(mapper.cpu_read(0xC000), 5);
    }

    #[test]
    fn mmc1_prg_32k_mode_ignores_low_bank_bit() {
        let mut mapper = make_mmc1(8, 0);

        mmc1_write_5(&mut mapper, 0x8000, 0b00000);
        mmc1_write_5(&mut mapper, 0xE000, 3);

        assert_eq!(mapper.cpu_read(0x8000), 2);
        assert_eq!(mapper.cpu_read(0xC000), 3);
    }

    #[test]
    fn mmc1_chr_4k_mode_selects_two_pages() {
        let mut mapper = make_mmc1(2, 2);

        mmc1_write_5(&mut mapper, 0x8000, 0b11100);
        mmc1_write_5(&mut mapper, 0xA000, 1);
        mmc1_write_5(&mut mapper, 0xC000, 3);

        assert_eq!(mapper.chr_read(0x0000), 1);
        assert_eq!(mapper.chr_read(0x1000), 3);
    }

    #[test]
    fn mmc1_chr_8k_mode_ignores_low_bank_bit() {
        let mut mapper = make_mmc1(2, 2);

        mmc1_write_5(&mut mapper, 0x8000, 0b01100);
        mmc1_write_5(&mut mapper, 0xA000, 3);

        assert_eq!(mapper.chr_read(0x0000), 2);
        assert_eq!(mapper.chr_read(0x1000), 3);
    }

    #[test]
    fn mmc1_chr_ram_writes_through_selected_bank() {
        let mut mapper = make_mmc1(2, 0);

        mmc1_write_5(&mut mapper, 0x8000, 0b11100);
        mmc1_write_5(&mut mapper, 0xA000, 1);
        mapper.chr_write(0x0004, 0xA5);

        assert_eq!(mapper.chr_read(0x0004), 0xA5);
    }

    #[test]
    fn mmc1_prg_ram_roundtrip() {
        let mut mapper = make_mmc1(2, 0);

        mapper.cpu_write(0x6000, 0x42);
        mapper.cpu_write(0x7FFF, 0xAB);

        assert_eq!(mapper.cpu_read(0x6000), 0x42);
        assert_eq!(mapper.cpu_read(0x7FFF), 0xAB);
    }

    #[test]
    fn mmc1_mirroring_is_dynamic() {
        let mut mapper = make_mmc1(2, 0);

        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
        mmc1_write_5(&mut mapper, 0x8000, 0b01110);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        mmc1_write_5(&mut mapper, 0x8000, 0b01111);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        mmc1_write_5(&mut mapper, 0x8000, 0b01101);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn parse_valid_uxrom() {
        let data = make_ines(8, 0, 0x20);
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 2);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn uxrom_switches_low_prg_bank_and_fixes_high_bank() {
        let mut prg = vec![0u8; 8 * 16384];
        for bank in 0..8usize {
            for byte in &mut prg[bank * 16384..(bank + 1) * 16384] {
                *byte = bank as u8;
            }
            prg[bank * 16384] = 0xFF; // bus-conflict-safe write target
        }
        let mut mapper = UxRom::new(prg, Vec::new(), Mirroring::Vertical);

        assert_eq!(mapper.cpu_read(0x8001), 0);
        assert_eq!(mapper.cpu_read(0xC001), 7);

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.cpu_read(0x8001), 3);
        assert_eq!(mapper.cpu_read(0xC001), 7);
    }

    #[test]
    fn uxrom_chr_ram_roundtrip() {
        let mut mapper = UxRom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal);

        mapper.chr_write(0x1000, 0xAB);

        assert_eq!(mapper.chr_read(0x1000), 0xAB);
    }

    #[test]
    fn parse_valid_cnrom() {
        let data = make_ines(2, 4, 0x30);
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 3);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn cnrom_prg_is_unbanked_and_16k_mirrors_high_half() {
        let mut prg = vec![0u8; 16384];
        prg[0] = 0xCC;
        prg[1] = 0xDD;
        let mapper = CnRom::new(prg, vec![0u8; 8192], Mirroring::Horizontal);

        assert_eq!(mapper.cpu_read(0x8000), 0xCC);
        assert_eq!(mapper.cpu_read(0xC000), 0xCC);
        assert_eq!(mapper.cpu_read(0x8001), 0xDD);
        assert_eq!(mapper.cpu_read(0xC001), 0xDD);
    }

    #[test]
    fn cnrom_prg_is_unbanked_32k() {
        let mut prg = vec![0u8; 32768];
        prg[0] = 0xAA;
        prg[0x4000] = 0xBB;
        let mapper = CnRom::new(prg, vec![0u8; 8192], Mirroring::Vertical);

        assert_eq!(mapper.cpu_read(0x8000), 0xAA);
        assert_eq!(mapper.cpu_read(0xC000), 0xBB);
    }

    #[test]
    fn cnrom_switches_8k_chr_banks() {
        let mut chr = vec![0u8; 4 * 8192];
        for bank in 0..4usize {
            for byte in &mut chr[bank * 8192..(bank + 1) * 8192] {
                *byte = bank as u8;
            }
        }
        let mut mapper = CnRom::new(vec![0xFFu8; 32768], chr, Mirroring::Vertical);

        assert_eq!(mapper.chr_read(0x0000), 0);

        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.chr_read(0x0000), 2);

        mapper.cpu_write(0xFFFF, 3);
        assert_eq!(mapper.chr_read(0x1FFF), 3);
    }

    #[test]
    fn cnrom_chr_bank_write_obeys_bus_conflict() {
        let mut chr = vec![0u8; 4 * 8192];
        for bank in 0..4usize {
            for byte in &mut chr[bank * 8192..(bank + 1) * 8192] {
                *byte = bank as u8;
            }
        }
        let mut prg = vec![0xFFu8; 32768];
        prg[0] = 0x01;
        let mut mapper = CnRom::new(prg, chr, Mirroring::Vertical);

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.chr_read(0x0000), 1);
    }

    #[test]
    fn cnrom_chr_rom_not_writable() {
        let mut mapper = CnRom::new(vec![0xFFu8; 32768], vec![0x44u8; 8192], Mirroring::Vertical);

        mapper.chr_write(0x0000, 0xAB);

        assert_eq!(mapper.chr_read(0x0000), 0x44);
    }

    fn make_mmc3(prg_8k_banks: usize, chr_1k_pages: usize) -> Mmc3 {
        let mut prg_rom = vec![0u8; prg_8k_banks * 8192];
        for bank in 0..prg_8k_banks {
            for byte in &mut prg_rom[bank * 8192..(bank + 1) * 8192] {
                *byte = bank as u8;
            }
        }

        let chr_data = if chr_1k_pages == 0 {
            Vec::new()
        } else {
            let mut chr = vec![0u8; chr_1k_pages * 1024];
            for page in 0..chr_1k_pages {
                for byte in &mut chr[page * 1024..(page + 1) * 1024] {
                    *byte = page as u8;
                }
            }
            chr
        };

        Mmc3::new(prg_rom, chr_data)
    }

    #[test]
    fn parse_valid_mmc3() {
        let data = make_ines(4, 4, 0x40);
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 4);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc3_prg_mode_0_maps_r6_r7_second_last_last() {
        let mut mapper = make_mmc3(32, 8);

        mapper.cpu_write(0x8000, 6);
        mapper.cpu_write(0x8001, 5);
        mapper.cpu_write(0x8000, 7);
        mapper.cpu_write(0x8001, 10);

        assert_eq!(mapper.cpu_read(0x8000), 5);
        assert_eq!(mapper.cpu_read(0xA000), 10);
        assert_eq!(mapper.cpu_read(0xC000), 30);
        assert_eq!(mapper.cpu_read(0xE000), 31);
    }

    #[test]
    fn mmc3_prg_mode_1_swaps_r6_with_second_last() {
        let mut mapper = make_mmc3(32, 8);

        mapper.cpu_write(0x8000, 0x46);
        mapper.cpu_write(0x8001, 5);
        mapper.cpu_write(0x8000, 0x47);
        mapper.cpu_write(0x8001, 10);

        assert_eq!(mapper.cpu_read(0x8000), 30);
        assert_eq!(mapper.cpu_read(0xA000), 10);
        assert_eq!(mapper.cpu_read(0xC000), 5);
        assert_eq!(mapper.cpu_read(0xE000), 31);
    }

    #[test]
    fn mmc3_chr_mode_0_maps_two_2k_then_four_1k_banks() {
        let mut mapper = make_mmc3(4, 256);

        mapper.cpu_write(0x8000, 0);
        mapper.cpu_write(0x8001, 4);
        mapper.cpu_write(0x8000, 1);
        mapper.cpu_write(0x8001, 8);
        mapper.cpu_write(0x8000, 2);
        mapper.cpu_write(0x8001, 20);
        mapper.cpu_write(0x8000, 3);
        mapper.cpu_write(0x8001, 21);
        mapper.cpu_write(0x8000, 4);
        mapper.cpu_write(0x8001, 22);
        mapper.cpu_write(0x8000, 5);
        mapper.cpu_write(0x8001, 23);

        assert_eq!(mapper.chr_read(0x0000), 4);
        assert_eq!(mapper.chr_read(0x0400), 5);
        assert_eq!(mapper.chr_read(0x0800), 8);
        assert_eq!(mapper.chr_read(0x0C00), 9);
        assert_eq!(mapper.chr_read(0x1000), 20);
        assert_eq!(mapper.chr_read(0x1400), 21);
        assert_eq!(mapper.chr_read(0x1800), 22);
        assert_eq!(mapper.chr_read(0x1C00), 23);
    }

    #[test]
    fn mmc3_chr_mode_1_inverts_chr_windows() {
        let mut mapper = make_mmc3(4, 256);

        mapper.cpu_write(0x8000, 0x80);
        mapper.cpu_write(0x8001, 4);
        mapper.cpu_write(0x8000, 0x81);
        mapper.cpu_write(0x8001, 8);
        mapper.cpu_write(0x8000, 0x82);
        mapper.cpu_write(0x8001, 20);
        mapper.cpu_write(0x8000, 0x83);
        mapper.cpu_write(0x8001, 21);
        mapper.cpu_write(0x8000, 0x84);
        mapper.cpu_write(0x8001, 22);
        mapper.cpu_write(0x8000, 0x85);
        mapper.cpu_write(0x8001, 23);

        assert_eq!(mapper.chr_read(0x0000), 20);
        assert_eq!(mapper.chr_read(0x0400), 21);
        assert_eq!(mapper.chr_read(0x0800), 22);
        assert_eq!(mapper.chr_read(0x0C00), 23);
        assert_eq!(mapper.chr_read(0x1000), 4);
        assert_eq!(mapper.chr_read(0x1400), 5);
        assert_eq!(mapper.chr_read(0x1800), 8);
        assert_eq!(mapper.chr_read(0x1C00), 9);
    }

    #[test]
    fn mmc3_prg_ram_respects_enable_and_write_protect() {
        let mut mapper = make_mmc3(4, 8);

        mapper.cpu_write(0x6000, 0x42);
        assert_eq!(mapper.cpu_read(0x6000), 0x42);

        mapper.cpu_write(0xA001, 0xC0);
        mapper.cpu_write(0x6000, 0x99);
        assert_eq!(mapper.cpu_read(0x6000), 0x42);

        mapper.cpu_write(0xA001, 0x00);
        assert_eq!(mapper.cpu_read(0x6000), 0x00);
        mapper.cpu_write(0x6000, 0xAB);
        assert_eq!(mapper.cpu_read(0x6000), 0x00);

        mapper.cpu_write(0xA001, 0x80);
        mapper.cpu_write(0x6000, 0xAB);
        assert_eq!(mapper.cpu_read(0x6000), 0xAB);
    }

    #[test]
    fn mmc3_mirroring_is_dynamic() {
        let mut mapper = make_mmc3(4, 8);

        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        mapper.cpu_write(0xA000, 1);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        mapper.cpu_write(0xA000, 0);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc3_chr_ram_writes_through_selected_bank() {
        let mut mapper = make_mmc3(4, 0);

        mapper.cpu_write(0x8000, 0);
        mapper.cpu_write(0x8001, 4);
        mapper.chr_write(0x0002, 0x5A);

        assert_eq!(mapper.chr_read(0x0002), 0x5A);
    }

    fn mmc3_a12_edge(mapper: &mut Mmc3) {
        for _ in 0..16 {
            mapper.notify_a12_rendering(false);
        }
        mapper.notify_a12_rendering(true);
    }

    #[test]
    fn mmc3_irq_counter_clocks_on_debounced_a12_edges() {
        let mut mapper = make_mmc3(4, 8);

        mapper.cpu_write(0xC000, 3);
        mapper.cpu_write(0xC001, 0);
        mapper.cpu_write(0xE001, 0);

        mmc3_a12_edge(&mut mapper);
        assert!(!mapper.irq_pending());
        mmc3_a12_edge(&mut mapper);
        assert!(!mapper.irq_pending());
        mmc3_a12_edge(&mut mapper);
        assert!(!mapper.irq_pending());
        mmc3_a12_edge(&mut mapper);
        assert!(mapper.irq_pending());
    }

    #[test]
    fn mmc3_irq_disable_acknowledges_pending_irq() {
        let mut mapper = make_mmc3(4, 8);

        mapper.cpu_write(0xC000, 0);
        mapper.cpu_write(0xC001, 0);
        mapper.cpu_write(0xE001, 0);
        mmc3_a12_edge(&mut mapper);
        assert!(mapper.irq_pending());

        mapper.cpu_write(0xE000, 0);

        assert!(!mapper.irq_pending());
    }

    #[test]
    fn parse_valid_axrom() {
        let data = make_ines(2, 0, 0x70);
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 7);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn axrom_switches_32k_prg_bank() {
        let mut prg = vec![0u8; 8 * 32768];
        for bank in 0..8usize {
            for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
                *byte = bank as u8;
            }
            prg[bank * 32768] = 0xFF;
        }
        let mut mapper = AxRom::new(prg);

        assert_eq!(mapper.cpu_read(0x8001), 0);
        assert_eq!(mapper.cpu_read(0xC001), 0);

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.cpu_read(0x8001), 3);
        assert_eq!(mapper.cpu_read(0xC001), 3);
    }

    #[test]
    fn axrom_bank_write_obeys_bus_conflict() {
        let mut prg = vec![0xFFu8; 8 * 32768];
        for bank in 0..8usize {
            for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
                *byte = bank as u8;
            }
        }
        prg[0] = 0x01;
        let mut mapper = AxRom::new(prg);

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.cpu_read(0x8001), 1);
    }

    #[test]
    fn axrom_selects_single_screen_mirroring() {
        let mut mapper = AxRom::new(vec![0xFFu8; 32768]);

        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
        mapper.cpu_write(0x8000, 0x10);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenUpper);
        mapper.cpu_write(0x8000, 0x02);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn axrom_chr_ram_roundtrip() {
        let mut mapper = AxRom::new(vec![0u8; 32768]);

        mapper.chr_write(0x0000, 0xAB);
        mapper.chr_write(0x1FFF, 0xCD);

        assert_eq!(mapper.chr_read(0x0000), 0xAB);
        assert_eq!(mapper.chr_read(0x1FFF), 0xCD);
    }

    #[test]
    fn parse_valid_bxrom() {
        let data = make_ines(4, 0, 0x20 | 0x01);
        let mut data = data;
        data[7] = 0x20; // mapper 34 high nibble
        let parsed = parse_ines(&data).expect("parse failed");

        assert_eq!(parsed.header.mapper_number, 34);
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn bxrom_switches_32k_prg_bank() {
        let mut prg = vec![0u8; 4 * 32768];
        for bank in 0..4usize {
            for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
                *byte = bank as u8;
            }
            prg[bank * 32768] = 0xFF;
        }
        let mut mapper = BxRom::new(prg, Mirroring::Horizontal);

        assert_eq!(mapper.cpu_read(0x8001), 0);

        mapper.cpu_write(0x8000, 2);

        assert_eq!(mapper.cpu_read(0x8001), 2);
        assert_eq!(mapper.cpu_read(0xC001), 2);
    }

    #[test]
    fn bxrom_bank_write_obeys_bus_conflict() {
        let mut prg = vec![0xFFu8; 4 * 32768];
        for bank in 0..4usize {
            for byte in &mut prg[bank * 32768..(bank + 1) * 32768] {
                *byte = bank as u8;
            }
        }
        prg[0] = 0x01;
        let mut mapper = BxRom::new(prg, Mirroring::Horizontal);

        mapper.cpu_write(0x8000, 3);

        assert_eq!(mapper.cpu_read(0x8001), 1);
    }

    #[test]
    fn bxrom_chr_ram_roundtrip() {
        let mut mapper = BxRom::new(vec![0u8; 32768], Mirroring::Horizontal);

        mapper.chr_write(0x0000, 0xAB);
        mapper.chr_write(0x1FFF, 0xCD);

        assert_eq!(mapper.chr_read(0x0000), 0xAB);
        assert_eq!(mapper.chr_read(0x1FFF), 0xCD);
    }

    #[test]
    fn parse_rejects_short_file() {
        let data = vec![0u8; 8];
        assert!(parse_ines(&data).is_err());
    }

    #[test]
    fn parse_rejects_bad_magic() {
        let data = vec![0u8; 32];
        assert!(parse_ines(&data).is_err());
    }

    /// `ParsedCartridge` contains a `Box<dyn Mapper>` which does
    /// not implement `Debug`, so `.expect_err()` can't be used in
    /// these negative tests. This helper unwraps the error arm
    /// with a custom message and drops the `Ok` side.
    fn expect_err(result: Result<ParsedCartridge, String>, ctx: &str) -> String {
        match result {
            Ok(_) => panic!("{ctx}: expected error, got Ok"),
            Err(e) => e,
        }
    }

    #[test]
    fn parse_rejects_unsupported_mapper() {
        // Put mapper number 15 in the high nibble of flags6.
        let mut data = make_ines(1, 1, 0xF0);
        // Ensure flags7 high nibble is zero.
        data[7] = 0;
        let err = expect_err(parse_ines(&data), "mapper 15 should be rejected");
        assert!(err.contains("Unsupported mapper: 15"), "got: {err}");
    }

    #[test]
    fn parse_rejects_truncated_prg() {
        // Header claims 2 PRG banks but the file only carries one.
        let mut data = make_ines(1, 1, 0x00);
        data[4] = 2;
        let err = expect_err(parse_ines(&data), "truncated file should be rejected");
        assert!(err.contains("too short"), "got: {err}");
    }

    #[test]
    fn parse_battery_flag() {
        let data = make_ines(1, 1, 0x02); // battery bit set
        let parsed = parse_ines(&data).expect("parse failed");
        assert!(parsed.has_battery);
        assert!(parsed.header.has_battery);
    }

    #[test]
    fn parse_four_screen_mirroring() {
        let data = make_ines(1, 1, 0x08);
        let parsed = parse_ines(&data).expect("parse failed");
        assert_eq!(parsed.mapper.mirroring(), Mirroring::FourScreen);
    }

    // ─── NES 2.0 header tests ──────────────────────────────────────

    /// Build a minimal NES 2.0 file with the given bank counts and
    /// 12-bit mapper number.
    fn make_nes2(prg_banks: u16, chr_banks: u16, mapper: u16) -> Vec<u8> {
        let prg_lo = (prg_banks & 0xFF) as u8;
        let prg_hi = ((prg_banks >> 8) & 0x0F) as u8;
        let chr_lo = (chr_banks & 0xFF) as u8;
        let chr_hi = ((chr_banks >> 8) & 0x0F) as u8;

        let mapper_lo = (mapper & 0x0F) as u8;
        let mapper_mid = ((mapper >> 4) & 0x0F) as u8;
        let mapper_hi = ((mapper >> 8) & 0x0F) as u8;

        let flags6 = mapper_lo << 4;
        let flags7 = (mapper_mid << 4) | 0x08; // NES 2.0 signature
        let byte8 = mapper_hi;

        let prg_size = prg_banks as usize * 16384;
        let chr_size = chr_banks as usize * 8192;

        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_lo;
        data[5] = chr_lo;
        data[6] = flags6;
        data[7] = flags7;
        data[8] = byte8;
        data[9] = (chr_hi << 4) | prg_hi;

        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn nes2_detected_and_parsed() {
        let data = make_nes2(2, 1, 0);
        let parsed = parse_ines(&data).expect("NES 2.0 parse failed");
        assert_eq!(parsed.header.mapper_number, 0);
        assert_eq!(parsed.mapper.cpu_read(0x8000), 0x00);
    }

    #[test]
    fn nes2_mapper_number_12bit() {
        // Mapper 256 is beyond the 8-bit range, so a correct NES
        // 2.0 parser sees it as mapper 256 and this port rejects
        // it as unsupported. This confirms the 12-bit extraction
        // is wired in.
        let data = make_nes2(1, 1, 256);
        let err = expect_err(parse_ines(&data), "mapper 256 should be rejected");
        assert!(err.contains("256"), "got: {err}");
    }

    #[test]
    fn ines1_still_works_after_nes2_support() {
        // An iNES 1.0 file must keep parsing correctly even though
        // the parser now has an NES 2.0 branch.
        let data = make_ines(2, 1, 0x01);
        let parsed = parse_ines(&data).expect("iNES 1.0 parse failed");
        assert_eq!(parsed.mapper.mirroring(), Mirroring::Vertical);
    }
}
