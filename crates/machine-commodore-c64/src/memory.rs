//! C64 memory subsystem with 6510-controlled banking.

use format_commodore_c64_prg::RamAccess;
use mos_vic_ii::VicMemory;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BASIC_ROM_SIZE: usize = 0x2000;
const KERNAL_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;
const RAM_SIZE: usize = 0x10000;
const COLOUR_RAM_SIZE: usize = 0x0400;
/// 6510 port bits pulled high when configured as inputs: bits 0-2 (PLA
/// banking) and bit 4 (cassette sense). Bit 5 (cassette motor) has no
/// pull-up and reads 0 as an input — see `PORT_INPUT_PULLUPS` in
/// `machine.rs` (Lorenz trap17).
const PORT_PULLUPS: u8 = 0x17;

/// Memory-construction errors.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MemoryInitError {
    /// One ROM image had the wrong size.
    #[error("{which} ROM has {actual} bytes; expected exactly {expected}")]
    WrongRomSize {
        which: &'static str,
        expected: usize,
        actual: usize,
    },
}

/// One 8K cartridge bank: ROML (the `$8000-$9FFF` image) and, for 16K/Ultimax
/// banks, ROMH (`$A000-$BFFF` for 16K, `$E000-$FFFF` for Ultimax).
#[derive(Clone)]
pub(crate) struct CartBankPair {
    pub(crate) roml: Option<Box<[u8; 0x2000]>>,
    pub(crate) romh: Option<Box<[u8; 0x2000]>>,
}

/// Bank-switching scheme a cartridge drives through its `$DE00` I/O register.
///
/// Plain 8K/16K/Ultimax carts are [`CartBanking::None`]. The simple banked
/// schemes select one of several 8K ROML banks by writing the `$DE00` register.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CartBanking {
    /// No banking — a single fixed bank (plain 8K/16K/Ultimax).
    #[default]
    None,
    /// Ocean type 5: `bank = value & 0x3F`, always enabled.
    Ocean,
    /// Magic Desk type 19: `bank = value & 0x3F`; bit 7 set disables the cart
    /// (EXROM floats high, so the `$8000` window shows RAM).
    MagicDesk,
}

#[derive(Clone)]
struct Cartridge {
    exrom: bool,
    game: bool,
    banking: CartBanking,
    banks: Vec<CartBankPair>,
    current_bank: usize,
    /// Whether the cartridge currently maps ROM (Magic Desk can disable itself).
    enabled: bool,
}

impl Cartridge {
    /// Ultimax mode: EXROM high (not asserted), GAME low (asserted) — ROMH sits
    /// at `$E000` and the KERNAL/BASIC ROMs are hidden.
    const fn ultimax(&self) -> bool {
        !self.exrom && self.game
    }

    /// The currently selected bank, if in range.
    fn active_bank(&self) -> Option<&CartBankPair> {
        self.banks.get(self.current_bank)
    }
}

/// A GeoRAM-style paged RAM expansion. The CPU sees a 256-byte window at
/// `$DE00-$DEFF` into a much larger backing store, positioned by a block
/// register (`$DFFF`, 16 KiB granularity) and a page register (`$DFFE`,
/// 256-byte granularity within the block).
#[derive(Clone)]
struct GeoRam {
    ram: Vec<u8>,
    page: u8,
    block: u8,
}

impl GeoRam {
    fn new(size_bytes: usize) -> Self {
        Self {
            ram: vec![0; size_bytes.max(0x4000)],
            page: 0,
            block: 0,
        }
    }

    /// Mask for the block register, derived from the backing size (one bit per
    /// 16 KiB block; sizes are powers of two, so this is `blocks - 1`).
    fn block_mask(&self) -> u8 {
        let blocks = (self.ram.len() / 0x4000).max(1);
        u8::try_from(blocks - 1).unwrap_or(0xFF)
    }

    /// Backing-store index for one `$DE00` window offset.
    fn window_index(&self, offset: u8) -> usize {
        (usize::from(self.block) << 14) | (usize::from(self.page) << 8) | usize::from(offset)
    }
}

/// A 17xx-series RAM Expansion Unit: expansion RAM plus a DMA controller that
/// moves blocks between C64 RAM and expansion RAM. The register block sits at
/// `$DF00-$DF0A` (I/O-2). Transfers can start immediately or be armed to fire on
/// the first write to `$FF00`.
#[derive(Clone)]
struct Reu {
    ram: Vec<u8>,
    /// `$DF00` status flags (bit7 IRQ, bit6 end-of-block, bit5 verify fault).
    status: u8,
    /// `$DF01` command (bit7 execute, bit5 autoload, bit4 FF00-trigger disabled,
    /// bits1-0 transfer type).
    command: u8,
    /// Working transfer registers (advance during a transfer).
    c64_addr: u16,
    reu_addr: u32,
    length: u16,
    /// Shadow copies of the last-written base values, reloaded on autoload.
    c64_base: u16,
    reu_base: u32,
    length_base: u16,
    /// `$DF09` interrupt mask (bit7 enable, bit6 end-of-block, bit5 verify).
    irq_mask: u8,
    /// `$DF0A` address control (bit7 fix C64 address, bit6 fix REU address).
    addr_control: u8,
    /// A transfer armed with the execute bit but waiting for a `$FF00` write.
    ff00_armed: bool,
    /// Current `/IRQ` line state.
    irq: bool,
}

impl Reu {
    fn new(size_bytes: usize) -> Self {
        Self {
            ram: vec![0; size_bytes.max(0x20000)],
            status: 0,
            command: 0,
            c64_addr: 0,
            reu_addr: 0,
            length: 0,
            c64_base: 0,
            reu_base: 0,
            length_base: 0,
            irq_mask: 0,
            addr_control: 0,
            ff00_armed: false,
            irq: false,
        }
    }
}

/// Set the low byte of a working 16-bit REU register and its base shadow.
fn set_lo(reg: &mut u16, value: u8, base: &mut u16) {
    *reg = (*reg & 0xFF00) | u16::from(value);
    *base = *reg;
}

/// Set the high byte of a working 16-bit REU register and its base shadow.
fn set_hi(reg: &mut u16, value: u8, base: &mut u16) {
    *reg = (*reg & 0x00FF) | (u16::from(value) << 8);
    *base = *reg;
}

/// Set one byte of a working 24-bit REU register and its base shadow.
fn set_u24_byte(reg: &mut u32, byte: u32, value: u8, base: &mut u32) {
    let shift = byte * 8;
    let mask = !(0xFFu32 << shift);
    *reg = ((*reg & mask) | (u32::from(value) << shift)) & 0xFF_FFFF;
    *base = *reg;
}

/// C64 memory subsystem.
#[derive(Clone)]
pub struct C64Memory {
    ram: Box<[u8; RAM_SIZE]>,
    basic_rom: Box<[u8; BASIC_ROM_SIZE]>,
    kernal_rom: Box<[u8; KERNAL_ROM_SIZE]>,
    character_rom: Box<[u8; CHARACTER_ROM_SIZE]>,
    colour_ram: [u8; COLOUR_RAM_SIZE],
    port_ddr: u8,
    port_data: u8,
    cartridge: Option<Cartridge>,
    georam: Option<GeoRam>,
    reu: Option<Reu>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CartBankSnapshot {
    roml: Option<Vec<u8>>,
    romh: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CartridgeSnapshot {
    exrom: bool,
    game: bool,
    // Bank 0's images, kept as bare fields so pre-banking snapshots still load.
    roml: Option<Vec<u8>>,
    romh: Option<Vec<u8>>,
    #[serde(default)]
    banking: CartBanking,
    #[serde(default)]
    current_bank: u32,
    #[serde(default = "default_true")]
    enabled: bool,
    /// Banks 1.. (bank 0 lives in `roml`/`romh`).
    #[serde(default)]
    extra_banks: Vec<CartBankSnapshot>,
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GeoRamSnapshot {
    ram: Vec<u8>,
    page: u8,
    block: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ReuSnapshot {
    ram: Vec<u8>,
    status: u8,
    command: u8,
    c64_addr: u16,
    reu_addr: u32,
    length: u16,
    c64_base: u16,
    reu_base: u32,
    length_base: u16,
    irq_mask: u8,
    addr_control: u8,
    ff00_armed: bool,
    irq: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct C64MemorySnapshot {
    ram: Vec<u8>,
    basic_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    character_rom: Vec<u8>,
    colour_ram: Vec<u8>,
    port_ddr: u8,
    port_data: u8,
    #[serde(default)]
    cartridge: Option<CartridgeSnapshot>,
    #[serde(default)]
    georam: Option<GeoRamSnapshot>,
    #[serde(default)]
    reu: Option<ReuSnapshot>,
}

impl C64Memory {
    /// Constructs the memory subsystem from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM size is incorrect.
    pub fn new(
        kernal_rom: &[u8],
        basic_rom: &[u8],
        character_rom: &[u8],
    ) -> Result<Self, MemoryInitError> {
        Ok(Self {
            ram: Box::new([0; RAM_SIZE]),
            basic_rom: boxed_array_from_slice("BASIC", basic_rom)?,
            kernal_rom: boxed_array_from_slice("KERNAL", kernal_rom)?,
            character_rom: boxed_array_from_slice("character", character_rom)?,
            colour_ram: [0; COLOUR_RAM_SIZE],
            port_ddr: 0x2F,
            port_data: 0x37,
            cartridge: None,
            georam: None,
            reu: None,
        })
    }

    /// Inserts a cartridge, taking its ROML (`$8000`) and ROMH (`$A000`/`$E000`)
    /// 8K images and its `EXROM`/`GAME` line state (both "asserted = low").
    pub(crate) fn insert_cartridge(
        &mut self,
        exrom: bool,
        game: bool,
        roml: Option<Box<[u8; 0x2000]>>,
        romh: Option<Box<[u8; 0x2000]>>,
    ) {
        self.cartridge = Some(Cartridge {
            exrom,
            game,
            banking: CartBanking::None,
            banks: vec![CartBankPair { roml, romh }],
            current_bank: 0,
            enabled: true,
        });
    }

    /// Inserts a simple bank-switched cartridge: several 8K banks selected by a
    /// `$DE00` register. `banks[0]` is the power-on bank.
    pub(crate) fn insert_banked_cartridge(
        &mut self,
        exrom: bool,
        game: bool,
        banking: CartBanking,
        banks: Vec<CartBankPair>,
    ) {
        self.cartridge = Some(Cartridge {
            exrom,
            game,
            banking,
            banks,
            current_bank: 0,
            enabled: true,
        });
    }

    /// Removes any inserted cartridge.
    pub(crate) fn remove_cartridge(&mut self) {
        self.cartridge = None;
    }

    /// Applies a write to the cartridge I/O-1 area (`$DE00-$DEFF`), driving the
    /// bank register of a simple bank-switched cartridge. A no-op for unbanked
    /// carts and when no cartridge is inserted.
    pub(crate) fn cartridge_io_write(&mut self, addr: u16, value: u8) {
        if !(0xDE00..=0xDEFF).contains(&addr) {
            return;
        }
        let Some(cart) = self.cartridge.as_mut() else {
            return;
        };
        match cart.banking {
            CartBanking::None => {}
            CartBanking::Ocean => {
                cart.current_bank = usize::from(value & 0x3F);
            }
            CartBanking::MagicDesk => {
                cart.enabled = value & 0x80 == 0;
                cart.current_bank = usize::from(value & 0x3F);
            }
        }
    }

    /// Attaches a GeoRAM RAM expansion of `size_kb` KiB (rounded up to at least
    /// one 16 KiB block), zero-filled. Replaces any previously attached unit.
    pub(crate) fn attach_georam(&mut self, size_kb: usize) {
        self.georam = Some(GeoRam::new(size_kb.saturating_mul(1024)));
    }

    /// Detaches any GeoRAM expansion.
    pub(crate) fn detach_georam(&mut self) {
        self.georam = None;
    }

    /// Whether a GeoRAM expansion is attached.
    #[must_use]
    pub(crate) fn has_georam(&self) -> bool {
        self.georam.is_some()
    }

    /// GeoRAM byte visible at `addr` (the `$DE00-$DEFF` window), if a unit is
    /// attached. The `$DFFE`/`$DFFF` registers are write-only, so this returns
    /// `None` for them (the caller yields open-bus `$FF`).
    pub(crate) fn georam_read(&self, addr: u16) -> Option<u8> {
        let gr = self.georam.as_ref()?;
        match addr {
            0xDE00..=0xDEFF => {
                let index = gr.window_index((addr - 0xDE00) as u8);
                gr.ram.get(index).copied()
            }
            _ => None,
        }
    }

    /// Applies a write to the GeoRAM I/O area. Returns `true` if a GeoRAM unit
    /// handled it (the `$DE00` window or the `$DFFE`/`$DFFF` bank registers), so
    /// the caller can fall through to cartridge I/O otherwise.
    pub(crate) fn georam_write(&mut self, addr: u16, value: u8) -> bool {
        let Some(gr) = self.georam.as_mut() else {
            return false;
        };
        match addr {
            0xDE00..=0xDEFF => {
                let index = gr.window_index((addr - 0xDE00) as u8);
                if let Some(slot) = gr.ram.get_mut(index) {
                    *slot = value;
                }
                true
            }
            0xDFFE => {
                gr.page = value & 0x3F;
                true
            }
            0xDFFF => {
                gr.block = value & gr.block_mask();
                true
            }
            _ => false,
        }
    }

    /// Routes a write to the expansion I/O area (`$DE00-$DFFF`): the REU register
    /// block (`$DF00-$DF0A`) wins, then GeoRAM, then a bank-switched cartridge.
    pub(crate) fn expansion_io_write(&mut self, addr: u16, value: u8) {
        if self.reu_write(addr, value) {
            return;
        }
        if !self.georam_write(addr, value) {
            self.cartridge_io_write(addr, value);
        }
    }

    /// Attaches a 17xx REU of `size_kb` KiB (typically 128, 256, or 512),
    /// zero-filled. Replaces any previously attached unit.
    pub(crate) fn attach_reu(&mut self, size_kb: usize) {
        self.reu = Some(Reu::new(size_kb.saturating_mul(1024)));
    }

    /// Detaches any REU.
    pub(crate) fn detach_reu(&mut self) {
        self.reu = None;
    }

    /// Whether a REU is attached.
    #[must_use]
    pub(crate) fn has_reu(&self) -> bool {
        self.reu.is_some()
    }

    /// Current REU `/IRQ` line state (asserted after an enabled transfer
    /// completes until the status register is read).
    #[must_use]
    pub(crate) fn reu_irq(&self) -> bool {
        self.reu.as_ref().is_some_and(|reu| reu.irq)
    }

    /// REU register byte at `addr` (`$DF00-$DF0A`), or `None` when no REU is
    /// attached or `addr` is outside the register block.
    pub(crate) fn reu_read(&mut self, addr: u16) -> Option<u8> {
        let reu = self.reu.as_mut()?;
        let value = match addr {
            0xDF00 => {
                // Reading the status register returns the flags plus the 256K+
                // chip-present bit, then clears the flags and the IRQ line.
                let value = reu.status | 0x10;
                reu.status = 0;
                reu.irq = false;
                value
            }
            0xDF01 => reu.command,
            0xDF02 => (reu.c64_addr & 0xFF) as u8,
            0xDF03 => (reu.c64_addr >> 8) as u8,
            0xDF04 => (reu.reu_addr & 0xFF) as u8,
            0xDF05 => ((reu.reu_addr >> 8) & 0xFF) as u8,
            0xDF06 => ((reu.reu_addr >> 16) & 0xFF) as u8 | 0xF8,
            0xDF07 => (reu.length & 0xFF) as u8,
            0xDF08 => (reu.length >> 8) as u8,
            0xDF09 => reu.irq_mask | 0x1F,
            0xDF0A => reu.addr_control | 0x3F,
            _ => return None,
        };
        Some(value)
    }

    /// Applies a write to the REU register block (`$DF00-$DF0A`). Returns `true`
    /// when a REU handled it. Writing the command register with the execute bit
    /// either starts the transfer or arms it for the next `$FF00` write.
    pub(crate) fn reu_write(&mut self, addr: u16, value: u8) -> bool {
        if self.reu.is_none() || !(0xDF00..=0xDF0A).contains(&addr) {
            return false;
        }
        let mut execute_now = false;
        {
            let reu = self.reu.as_mut().expect("REU present");
            match addr {
                0xDF00 => {} // status is read-only
                0xDF01 => {
                    reu.command = value;
                    if value & 0x80 != 0 {
                        if value & 0x10 != 0 {
                            execute_now = true;
                        } else {
                            reu.ff00_armed = true;
                        }
                    }
                }
                0xDF02 => set_lo(&mut reu.c64_addr, value, &mut reu.c64_base),
                0xDF03 => set_hi(&mut reu.c64_addr, value, &mut reu.c64_base),
                0xDF04 => set_u24_byte(&mut reu.reu_addr, 0, value, &mut reu.reu_base),
                0xDF05 => set_u24_byte(&mut reu.reu_addr, 1, value, &mut reu.reu_base),
                0xDF06 => set_u24_byte(&mut reu.reu_addr, 2, value, &mut reu.reu_base),
                0xDF07 => set_lo(&mut reu.length, value, &mut reu.length_base),
                0xDF08 => set_hi(&mut reu.length, value, &mut reu.length_base),
                0xDF09 => reu.irq_mask = value,
                0xDF0A => reu.addr_control = value,
                _ => {}
            }
        }
        if execute_now {
            self.reu_execute();
        }
        true
    }

    /// A CPU write to `$FF00` fires any REU transfer armed for the FF00 trigger.
    pub(crate) fn reu_ff00_write(&mut self) {
        if self.reu.as_ref().is_some_and(|reu| reu.ff00_armed) {
            self.reu_execute();
        }
    }

    /// Runs the pending REU transfer against C64 RAM.
    fn reu_execute(&mut self) {
        let Self {
            ram,
            reu: Some(reu),
            ..
        } = self
        else {
            return;
        };
        reu.ff00_armed = false;
        let transfer = reu.command & 0x03;
        let fix_c64 = reu.addr_control & 0x80 != 0;
        let fix_reu = reu.addr_control & 0x40 != 0;
        let count = if reu.length == 0 {
            0x1_0000
        } else {
            usize::from(reu.length)
        };
        let reu_len = reu.ram.len();
        let mut c64 = reu.c64_addr;
        let mut ra = reu.reu_addr as usize;
        let mut fault = false;

        for _ in 0..count {
            let ci = usize::from(c64);
            let ri = ra % reu_len;
            match transfer {
                0 => reu.ram[ri] = ram[ci],
                1 => ram[ci] = reu.ram[ri],
                2 => std::mem::swap(&mut ram[ci], &mut reu.ram[ri]),
                _ => {
                    if ram[ci] != reu.ram[ri] {
                        fault = true;
                        break;
                    }
                }
            }
            if !fix_c64 {
                c64 = c64.wrapping_add(1);
            }
            if !fix_reu {
                ra = ra.wrapping_add(1);
            }
        }

        // Autoload restores the programmed base values; otherwise the working
        // registers hold where the transfer ended.
        if reu.command & 0x20 != 0 {
            reu.c64_addr = reu.c64_base;
            reu.reu_addr = reu.reu_base;
            reu.length = reu.length_base;
        } else {
            reu.c64_addr = c64;
            reu.reu_addr = (ra as u32) & 0xFF_FFFF;
            reu.length = 1;
        }
        reu.command &= !0x80;

        // End-of-block (or a verify fault) sets the status + raises IRQ if the
        // mask enables it.
        if fault {
            reu.status |= 0x20;
        } else {
            reu.status |= 0x40;
        }
        let irq_enabled = reu.irq_mask & 0x80 != 0
            && ((!fault && reu.irq_mask & 0x40 != 0) || (fault && reu.irq_mask & 0x20 != 0));
        if irq_enabled {
            reu.status |= 0x80;
            reu.irq = true;
        }
    }

    /// Cartridge ROM byte visible at `addr`, if a cartridge maps it under the
    /// current banking, else `None`.
    fn cartridge_read(&self, addr: u16) -> Option<u8> {
        let cart = self.cartridge.as_ref()?;
        if !cart.enabled {
            return None;
        }
        let bank = cart.active_bank()?;
        match addr {
            0x8000..=0x9FFF => {
                let roml = bank.roml.as_ref()?;
                // ROML is unconditionally visible in Ultimax; otherwise it needs
                // LORAM + HIRAM (the standard 8K/16K cartridge window).
                (cart.ultimax() || (self.loram() && self.hiram()))
                    .then(|| roml[usize::from(addr - 0x8000)])
            }
            0xA000..=0xBFFF => {
                // 16K carts (not Ultimax) place ROMH here, replacing BASIC.
                // The PLA maps it on HIRAM alone — LORAM only gates ROML
                // (Prince of Persia EF's launcher runs with $01=$36).
                let romh = bank.romh.as_ref()?;
                (!cart.ultimax() && self.hiram()).then(|| romh[usize::from(addr - 0xA000)])
            }
            0xE000..=0xFFFF => {
                // Ultimax carts place ROMH here, replacing the KERNAL.
                let romh = bank.romh.as_ref()?;
                cart.ultimax().then(|| romh[usize::from(addr - 0xE000)])
            }
            _ => None,
        }
    }

    const fn cart_ultimax(&self) -> bool {
        match &self.cartridge {
            Some(cart) => cart.ultimax(),
            None => false,
        }
    }

    /// Rebuilds one memory subsystem from a previously captured snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if any stored array has the wrong size.
    pub(crate) fn from_snapshot(snapshot: C64MemorySnapshot) -> Result<Self, String> {
        if snapshot.ram.len() != RAM_SIZE {
            return Err(format!(
                "snapshot RAM has {} bytes, expected {}",
                snapshot.ram.len(),
                RAM_SIZE
            ));
        }

        if snapshot.colour_ram.len() != COLOUR_RAM_SIZE {
            return Err(format!(
                "snapshot colour RAM has {} bytes, expected {}",
                snapshot.colour_ram.len(),
                COLOUR_RAM_SIZE
            ));
        }

        let mut memory = Self::new(
            &snapshot.kernal_rom,
            &snapshot.basic_rom,
            &snapshot.character_rom,
        )
        .map_err(|reason| reason.to_string())?;
        memory.ram.copy_from_slice(&snapshot.ram);
        memory.colour_ram.copy_from_slice(&snapshot.colour_ram);
        memory.port_ddr = snapshot.port_ddr;
        memory.port_data = snapshot.port_data;
        if let Some(cart) = snapshot.cartridge {
            let bank = |image: Option<Vec<u8>>| -> Result<Option<Box<[u8; 0x2000]>>, String> {
                image
                    .map(|bytes| {
                        boxed_array_from_slice::<0x2000>("cartridge bank", &bytes)
                            .map_err(|reason| reason.to_string())
                    })
                    .transpose()
            };
            let mut banks = vec![CartBankPair {
                roml: bank(cart.roml)?,
                romh: bank(cart.romh)?,
            }];
            for extra in cart.extra_banks {
                banks.push(CartBankPair {
                    roml: bank(extra.roml)?,
                    romh: bank(extra.romh)?,
                });
            }
            let current_bank = cart.current_bank as usize;
            memory.cartridge = Some(Cartridge {
                exrom: cart.exrom,
                game: cart.game,
                banking: cart.banking,
                banks,
                current_bank,
                enabled: cart.enabled,
            });
        }
        if let Some(gr) = snapshot.georam {
            memory.georam = Some(GeoRam {
                ram: gr.ram,
                page: gr.page,
                block: gr.block,
            });
        }
        if let Some(r) = snapshot.reu {
            memory.reu = Some(Reu {
                ram: r.ram,
                status: r.status,
                command: r.command,
                c64_addr: r.c64_addr,
                reu_addr: r.reu_addr,
                length: r.length,
                c64_base: r.c64_base,
                reu_base: r.reu_base,
                length_base: r.length_base,
                irq_mask: r.irq_mask,
                addr_control: r.addr_control,
                ff00_armed: r.ff00_armed,
                irq: r.irq,
            });
        }
        Ok(memory)
    }

    /// Captures the full memory state for runtime snapshot serialization.
    #[must_use]
    pub(crate) fn snapshot_state(&self) -> C64MemorySnapshot {
        C64MemorySnapshot {
            ram: self.ram.as_slice().to_vec(),
            basic_rom: self.basic_rom.as_slice().to_vec(),
            kernal_rom: self.kernal_rom.as_slice().to_vec(),
            character_rom: self.character_rom.as_slice().to_vec(),
            colour_ram: self.colour_ram.to_vec(),
            port_ddr: self.port_ddr,
            port_data: self.port_data,
            cartridge: self.cartridge.as_ref().map(|cart| {
                let bank0 = cart.banks.first();
                CartridgeSnapshot {
                    exrom: cart.exrom,
                    game: cart.game,
                    roml: bank0.and_then(|b| b.roml.as_ref().map(|b| b.to_vec())),
                    romh: bank0.and_then(|b| b.romh.as_ref().map(|b| b.to_vec())),
                    banking: cart.banking,
                    current_bank: cart.current_bank as u32,
                    enabled: cart.enabled,
                    extra_banks: cart
                        .banks
                        .iter()
                        .skip(1)
                        .map(|b| CartBankSnapshot {
                            roml: b.roml.as_ref().map(|b| b.to_vec()),
                            romh: b.romh.as_ref().map(|b| b.to_vec()),
                        })
                        .collect(),
                }
            }),
            georam: self.georam.as_ref().map(|gr| GeoRamSnapshot {
                ram: gr.ram.clone(),
                page: gr.page,
                block: gr.block,
            }),
            reu: self.reu.as_ref().map(|r| ReuSnapshot {
                ram: r.ram.clone(),
                status: r.status,
                command: r.command,
                c64_addr: r.c64_addr,
                reu_addr: r.reu_addr,
                length: r.length,
                c64_base: r.c64_base,
                reu_base: r.reu_base,
                length_base: r.length_base,
                irq_mask: r.irq_mask,
                addr_control: r.addr_control,
                ff00_armed: r.ff00_armed,
                irq: r.irq,
            }),
        }
    }

    /// Current 6510 port DDR value at `$0000`.
    #[must_use]
    pub const fn port_ddr(&self) -> u8 {
        self.port_ddr
    }

    /// Current 6510 port data value at `$0001`.
    #[must_use]
    pub const fn port_data(&self) -> u8 {
        self.port_data
    }

    /// Effective 6510 port value after applying DDR outputs and pull-ups.
    #[must_use]
    pub const fn effective_port(&self) -> u8 {
        (self.port_data & self.port_ddr) | (PORT_PULLUPS & !self.port_ddr)
    }

    /// Returns `true` when BASIC ROM is visible at `$A000-$BFFF`. Hidden in
    /// Ultimax mode.
    #[must_use]
    pub const fn basic_visible(&self) -> bool {
        self.hiram() && self.loram() && !self.cart_ultimax()
    }

    /// Returns `true` when KERNAL ROM is visible at `$E000-$FFFF`. Hidden in
    /// Ultimax mode (the cartridge's ROMH takes `$E000`).
    #[must_use]
    pub const fn kernal_visible(&self) -> bool {
        self.hiram() && !self.cart_ultimax()
    }

    /// Returns `true` when I/O is visible at `$D000-$DFFF`.
    #[must_use]
    pub const fn is_io_visible(&self) -> bool {
        self.charen() && (self.hiram() || self.loram())
    }

    /// Returns `true` when character ROM is visible to the CPU. The PLA
    /// maps char ROM at `$D000` for every mode with CHAREN low and at
    /// least one of LORAM/HIRAM high (%001, %010, %011) — not just the
    /// full-banking %011 (Lorenz `mmu` col-3 expectations; VICE
    /// `c64mem.c` read table).
    #[must_use]
    pub const fn is_character_rom_visible_to_cpu(&self) -> bool {
        !self.charen() && (self.hiram() || self.loram())
    }

    /// CPU-visible read with ROM overlays applied.
    #[must_use]
    pub fn cpu_read(&self, addr: u16) -> u8 {
        // Cartridge ROM overlays the CPU map first (ROML $8000, ROMH $A000/$E000).
        if addr >= 0x8000
            && !(0xD000..=0xDFFF).contains(&addr)
            && let Some(byte) = self.cartridge_read(addr)
        {
            return byte;
        }
        match addr {
            0x0000 => self.port_ddr,
            0x0001 => self.effective_port(),
            0xA000..=0xBFFF if self.basic_visible() => self.basic_rom[usize::from(addr - 0xA000)],
            0xD000..=0xDFFF if self.is_character_rom_visible_to_cpu() => {
                self.character_rom[usize::from(addr - 0xD000)]
            }
            0xE000..=0xFFFF if self.kernal_visible() => self.kernal_rom[usize::from(addr - 0xE000)],
            _ => self.ram[usize::from(addr)],
        }
    }

    /// CPU-visible write. ROM areas still write through to underlying RAM,
    /// matching real hardware.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            _ => self.ram[usize::from(addr)] = value,
        }
    }

    /// Direct RAM read bypassing overlays.
    #[must_use]
    pub fn ram_read(&self, addr: u16) -> u8 {
        self.ram[usize::from(addr)]
    }

    /// Borrows the full underlying RAM image.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        self.ram.as_slice()
    }

    /// Direct RAM write bypassing overlays.
    pub fn ram_write(&mut self, addr: u16, value: u8) {
        self.ram[usize::from(addr)] = value;
    }

    /// Reads the current VIC-visible byte from one 16 KiB bank-local offset.
    #[must_use]
    pub fn vic_read(&self, bank: u8, offset: u16) -> u8 {
        let bank = usize::from(bank & 0x03);
        let offset = usize::from(offset & 0x3FFF);
        if (bank == 0 || bank == 2) && (0x1000..0x2000).contains(&offset) {
            return self.character_rom[offset - 0x1000];
        }

        self.ram[(bank * 0x4000) + offset]
    }

    /// Reads one colour RAM nibble.
    #[must_use]
    pub fn colour_ram_read(&self, offset: u16) -> u8 {
        self.colour_ram
            .get(usize::from(offset))
            .copied()
            .map_or(0, |value| value & 0x0F)
    }

    /// Writes one colour RAM nibble.
    pub fn colour_ram_write(&mut self, offset: u16, value: u8) {
        if let Some(slot) = self.colour_ram.get_mut(usize::from(offset)) {
            *slot = value & 0x0F;
        }
    }

    /// Borrows the full underlying colour RAM image.
    #[must_use]
    pub fn colour_ram(&self) -> &[u8] {
        &self.colour_ram
    }

    const fn hiram(&self) -> bool {
        self.effective_port() & 0x02 != 0
    }

    const fn loram(&self) -> bool {
        self.effective_port() & 0x01 != 0
    }

    const fn charen(&self) -> bool {
        self.effective_port() & 0x04 != 0
    }
}

fn boxed_array_from_slice<const N: usize>(
    which: &'static str,
    bytes: &[u8],
) -> Result<Box<[u8; N]>, MemoryInitError> {
    if bytes.len() != N {
        return Err(MemoryInitError::WrongRomSize {
            which,
            expected: N,
            actual: bytes.len(),
        });
    }

    let mut array = Box::new([0; N]);
    array.copy_from_slice(bytes);
    Ok(array)
}

impl VicMemory for C64Memory {
    fn read_vram(&self, addr: u16) -> u8 {
        self.vic_read((addr >> 14) as u8, addr & 0x3FFF)
    }

    fn read_colour(&self, offset: u16) -> u8 {
        self.colour_ram_read(offset)
    }
}

impl RamAccess for C64Memory {
    fn ram_read(&self, addr: u16) -> u8 {
        self.ram_read(addr)
    }

    fn ram_write(&mut self, addr: u16, val: u8) {
        self.ram_write(addr, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> C64Memory {
        C64Memory::new(
            &[0xEE; KERNAL_ROM_SIZE],
            &[0xBB; BASIC_ROM_SIZE],
            &[0xCC; CHARACTER_ROM_SIZE],
        )
        .expect("stub ROM sizes should be valid")
    }

    #[test]
    fn default_banking_shows_basic_and_kernal() {
        let memory = make_memory();
        assert_eq!(memory.cpu_read(0xA000), 0xBB);
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
        assert!(memory.is_io_visible());
    }

    /// 8K cart: EXROM asserted, GAME not. ROML at $8000 when LORAM+HIRAM (the
    /// default banking); BASIC/KERNAL still visible.
    #[test]
    fn cart_8k_maps_roml_at_8000() {
        let mut memory = make_memory();
        memory.insert_cartridge(true, false, Some(Box::new([0xA1; 0x2000])), None);
        assert_eq!(memory.cpu_read(0x8000), 0xA1);
        assert_eq!(memory.cpu_read(0x9FFF), 0xA1);
        // BASIC + KERNAL unaffected by a plain 8K cart.
        assert_eq!(memory.cpu_read(0xA000), 0xBB);
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
        // With LORAM cleared the ROML window closes → RAM shows through.
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x36); // LORAM low
        memory.ram_write(0x8000, 0x77);
        assert_eq!(memory.cpu_read(0x8000), 0x77);
    }

    /// 16K cart: both lines asserted. ROML at $8000 + ROMH at $A000 (replacing
    /// BASIC); KERNAL still visible.
    #[test]
    fn cart_16k_maps_roml_and_romh() {
        let mut memory = make_memory();
        memory.insert_cartridge(
            true,
            true,
            Some(Box::new([0xA1; 0x2000])),
            Some(Box::new([0xB2; 0x2000])),
        );
        assert_eq!(memory.cpu_read(0x8000), 0xA1);
        assert_eq!(memory.cpu_read(0xA000), 0xB2); // ROMH, not BASIC (0xBB)
        assert_eq!(memory.cpu_read(0xE000), 0xEE); // KERNAL untouched
    }

    /// 16K cart with LORAM low ($01=$36): the PLA keeps ROMH at $A000 on
    /// HIRAM alone — only the ROML window closes (Prince of Persia EF's
    /// launcher runs in exactly this configuration).
    #[test]
    fn cart_16k_keeps_romh_when_loram_low() {
        let mut memory = make_memory();
        memory.insert_cartridge(
            true,
            true,
            Some(Box::new([0xA1; 0x2000])),
            Some(Box::new([0xB2; 0x2000])),
        );
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x36); // LORAM low, HIRAM high
        memory.ram_write(0x8000, 0x77);
        assert_eq!(memory.cpu_read(0x8000), 0x77); // ROML window closed
        assert_eq!(memory.cpu_read(0xA000), 0xB2); // ROMH stays
        assert_eq!(memory.cpu_read(0xE000), 0xEE); // KERNAL stays
        // HIRAM low closes ROMH too.
        memory.cpu_write(0x0001, 0x35);
        memory.ram_write(0xA000, 0x66);
        assert_eq!(memory.cpu_read(0xA000), 0x66);
    }

    /// Ultimax: EXROM high, GAME low. ROML at $8000 + ROMH at $E000 (replacing
    /// KERNAL); BASIC + KERNAL are hidden.
    #[test]
    fn cart_ultimax_maps_romh_at_e000_and_hides_roms() {
        let mut memory = make_memory();
        memory.insert_cartridge(
            false,
            true,
            Some(Box::new([0xA1; 0x2000])),
            Some(Box::new([0xE7; 0x2000])),
        );
        assert_eq!(memory.cpu_read(0x8000), 0xA1);
        assert_eq!(memory.cpu_read(0xE000), 0xE7); // ROMH, not KERNAL (0xEE)
        assert!(!memory.kernal_visible());
        assert!(!memory.basic_visible());
    }

    #[test]
    fn cart_removed_restores_plain_map() {
        let mut memory = make_memory();
        memory.insert_cartridge(true, false, Some(Box::new([0xA1; 0x2000])), None);
        memory.ram_write(0x8000, 0x55);
        memory.remove_cartridge();
        assert_eq!(memory.cpu_read(0x8000), 0x55); // RAM, no ROML overlay
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
    }

    #[test]
    fn writes_land_in_ram_under_roms() {
        let mut memory = make_memory();
        memory.cpu_write(0xA000, 0x42);
        memory.cpu_write(0xE000, 0x24);
        assert_eq!(memory.cpu_read(0xA000), 0xBB);
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
        assert_eq!(memory.ram_read(0xA000), 0x42);
        assert_eq!(memory.ram_read(0xE000), 0x24);
    }

    #[test]
    fn all_ram_banking_hides_roms_and_io() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x00);
        memory.ram_write(0xA000, 0x42);
        memory.ram_write(0xD000, 0x43);
        memory.ram_write(0xE000, 0x44);
        assert_eq!(memory.cpu_read(0xA000), 0x42);
        assert_eq!(memory.cpu_read(0xD000), 0x43);
        assert_eq!(memory.cpu_read(0xE000), 0x44);
        assert!(!memory.is_io_visible());
    }

    #[test]
    fn character_rom_appears_when_charen_is_clear() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x33);
        assert_eq!(memory.cpu_read(0xD000), 0xCC);
        assert!(memory.is_character_rom_visible_to_cpu());
    }

    #[test]
    fn port_inputs_float_high_when_ddr_is_clear() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0x00);
        memory.cpu_write(0x0001, 0x00);
        assert_eq!(memory.cpu_read(0x0001), PORT_PULLUPS);
    }

    #[test]
    fn vic_reads_character_rom_in_banks_zero_and_two() {
        let mut memory = make_memory();
        memory.ram_write(0x5000, 0xAA);
        memory.ram_write(0xD000, 0xBB);

        assert_eq!(memory.vic_read(0, 0x1000), 0xCC);
        assert_eq!(memory.vic_read(2, 0x1000), 0xCC);
        assert_eq!(memory.vic_read(1, 0x1000), 0xAA);
        assert_eq!(memory.vic_read(3, 0x1000), 0xBB);
    }

    #[test]
    fn colour_ram_stores_low_nibble_only() {
        let mut memory = make_memory();
        memory.colour_ram_write(0, 0x0F);
        memory.colour_ram_write(1, 0xFF);
        assert_eq!(memory.colour_ram_read(0), 0x0F);
        assert_eq!(memory.colour_ram_read(1), 0x0F);
    }

    #[test]
    fn wrong_rom_sizes_are_rejected() {
        let err = match C64Memory::new(&[0; 1], &[0; BASIC_ROM_SIZE], &[0; CHARACTER_ROM_SIZE]) {
            Ok(_) => panic!("wrong KERNAL size must fail"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            MemoryInitError::WrongRomSize {
                which: "KERNAL",
                expected: KERNAL_ROM_SIZE,
                actual: 1,
            }
        );
    }
}
