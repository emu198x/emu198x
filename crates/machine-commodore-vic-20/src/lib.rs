//! Commodore VIC-20 (1981) — 6502 + VIC 6560/6561 (inline) + character ROM.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-commodore-vic-20/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`emu198x_mos_6502::M6502`]'s public pin fields.
//!
//! # The VIC-20
//!
//! Commodore's 1981 home-computer launch — the first computer to sell
//! over a million units, designed to a $300 price point with Robert
//! Yannes' MOS 6560/6561 VIC handling both video AND audio on a single
//! chip. Marketed as VIC-20 in North America and VC-20 in Germany;
//! sold under various names worldwide.
//!
//! - **CPU:** MOS 6502 at 1.108 MHz (PAL) / 1.023 MHz (NTSC)
//! - **VIC 6560/6561:** 22 × 23 character display (176 × 184),
//!   3-tone + noise audio. Inline as
//!   [`vic::Vic6560`].
//! - **RAM:** 5 KB total (1 KB zero page/stack + 4 KB main at `$1000`),
//!   expandable to 32 KB
//! - **ROMs:** 8 KB Kernal at `$E000`, 8 KB BASIC at `$C000`, 4 KB
//!   character ROM at `$8000` (unlike the C64, BASIC is at `$C000`, not
//!   `$A000` — `$A000-$BFFF` is the BLK5 cartridge block)
//!
//! The VIC chip lives in the dedicated [`mos_vic_i`] chip crate
//! (text-mode video plus the three-tone + noise sound sources). Two MOS
//! 6522 VIAs handle
//! I/O: VIA #1 at `$9110-$911F` (RESTORE key → NMI, user port) and
//! VIA #2 at `$9120-$912F` (keyboard scan + Timer 1 → the 60 Hz system
//! IRQ that drives the KERNAL keyboard/jiffy handler). The keyboard
//! matrix hangs off VIA #2: port B drives the columns, port A reads the
//! rows.

pub mod input;
mod keyboard;

pub use input::Vic20Key;
pub use keyboard::KeyboardState;
pub use mos_vic_i::{Vic6560, framebuffer_height, framebuffer_width};

use emu198x_mos_6502::M6502;
use mos_via_6522::Via6522;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// VIC-20 model selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vic20Model {
    Pal,
    Ntsc,
}

/// Which RAM expansion cartridges are plugged into the expansion port.
///
/// The 3K expander (VIC-1210) and the block RAM cartridges (VIC-1110 8K,
/// VIC-1111 16K) are separate products. Either can be plugged into the
/// expansion socket alone, or several together through the VIC-1010 Expansion
/// Module, so their presence is tracked independently rather than derived from
/// a single size.
///
/// An 8K cartridge carries four DIP switches selecting which block it answers:
/// `$2000-$3FFF` (the normal position for BASIC), `$4000-$5FFF`,
/// `$6000-$7FFF` or `$A000-$BFFF`. Two cartridges must never claim the same
/// block. BASIC only follows a continuous program area, so a second 8K has to
/// sit at `$4000` for BASIC to see it.
///
/// Source: Commodore, *Using the 3K, 8K and 16K Memory Expanders* (1982),
/// `reference/by-system/commodore-vic20/1982-vic-1210-1110-1111-memory-expanders.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Vic20RamExpansion {
    /// VIC-1210 3K expander at `$0400-$0FFF`.
    pub exp_3k: bool,
    /// BLK1, `$2000-$3FFF`.
    pub blk1: bool,
    /// BLK2, `$4000-$5FFF`.
    pub blk2: bool,
    /// BLK3, `$6000-$7FFF`.
    pub blk3: bool,
    /// BLK5, `$A000-$BFFF` — shared with the cartridge ROM window, where a
    /// ROM cartridge's decode lines win.
    pub blk5: bool,
}

impl Vic20RamExpansion {
    /// Unexpanded 5K machine.
    pub const NONE: Self = Self {
        exp_3k: false,
        blk1: false,
        blk2: false,
        blk3: false,
        blk5: false,
    };
    /// VIC-1210 3K expander on its own.
    pub const EXP_3K: Self = Self {
        exp_3k: true,
        ..Self::NONE
    };
    /// VIC-1110 8K expander in its normal `$2000` position.
    pub const EXP_8K: Self = Self {
        blk1: true,
        ..Self::NONE
    };
    /// VIC-1111 16K expander, BLK1 + BLK2.
    pub const EXP_16K: Self = Self {
        blk1: true,
        blk2: true,
        ..Self::NONE
    };
    /// 24K for BASIC, BLK1 + BLK2 + BLK3.
    pub const EXP_24K: Self = Self {
        blk1: true,
        blk2: true,
        blk3: true,
        ..Self::NONE
    };

    /// Total installed expansion RAM in KiB.
    #[must_use]
    pub const fn total_kib(self) -> usize {
        let mut kib = if self.exp_3k { 3 } else { 0 };
        if self.blk1 {
            kib += 8;
        }
        if self.blk2 {
            kib += 8;
        }
        if self.blk3 {
            kib += 8;
        }
        if self.blk5 {
            kib += 8;
        }
        kib
    }

    /// Address the KERNAL reports as the BASIC program start for this
    /// configuration. BLK1 moves screen memory to `$1000` and BASIC to
    /// `$1200`; the 3K expander alone moves BASIC down to `$0400`. With both
    /// fitted the 3K area is not part of the BASIC program area, so BLK1 wins.
    #[must_use]
    pub const fn basic_start(self) -> u16 {
        if self.blk1 {
            0x1200
        } else if self.exp_3k {
            0x0400
        } else {
            0x1000
        }
    }

    /// Storage slot backing an address, if a block covers it: BLK1, BLK2,
    /// BLK3, BLK5.
    const fn slot_for(addr: u16) -> Option<usize> {
        match addr {
            0x2000..=0x3FFF => Some(0),
            0x4000..=0x5FFF => Some(1),
            0x6000..=0x7FFF => Some(2),
            0xA000..=0xBFFF => Some(3),
            _ => None,
        }
    }

    const fn slot_populated(self, slot: usize) -> bool {
        match slot {
            0 => self.blk1,
            1 => self.blk2,
            2 => self.blk3,
            3 => self.blk5,
            _ => false,
        }
    }

    fn claim(&mut self, slot: usize) -> Result<(), String> {
        let (present, name) = match slot {
            0 => (&mut self.exp_3k, "the 3K area at $0400"),
            1 => (&mut self.blk1, "BLK1 at $2000"),
            2 => (&mut self.blk2, "BLK2 at $4000"),
            3 => (&mut self.blk3, "BLK3 at $6000"),
            _ => (&mut self.blk5, "BLK5 at $A000"),
        };
        if *present {
            return Err(format!("two cartridges claim {name}"));
        }
        *present = true;
        Ok(())
    }

    /// Parse a hardware-shaped expansion spec: `none`, `3k`, `8k`, `16k`,
    /// `24k`, or DIP-switched blocks such as `8k@4000`, joined with `+`
    /// (`3k+8k`, `8k@2000+8k@6000`).
    ///
    /// # Errors
    ///
    /// Returns a readable message for an unknown term, or for two terms
    /// claiming the same block.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let spec = spec.trim().to_ascii_lowercase();
        if spec.is_empty() {
            return Err("empty RAM expansion spec".to_owned());
        }
        if spec == "none" || spec == "0" {
            return Ok(Self::NONE);
        }
        let mut out = Self::NONE;
        for term in spec.split('+') {
            let slots: &[usize] = match term.trim() {
                "3k" => &[0],
                "8k" | "8k@2000" => &[1],
                "8k@4000" => &[2],
                "8k@6000" => &[3],
                "8k@a000" => &[4],
                "16k" => &[1, 2],
                "24k" => &[1, 2, 3],
                other => {
                    return Err(format!(
                        "unknown RAM expansion term `{other}`; expected none, 3k, 8k, 16k, 24k, \
                         or a DIP-switched 8k@2000/8k@4000/8k@6000/8k@a000"
                    ));
                }
            };
            for &slot in slots {
                out.claim(slot)?;
            }
        }
        Ok(out)
    }
}

impl core::fmt::Display for Vic20RamExpansion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut terms: Vec<String> = Vec::new();
        if self.exp_3k {
            terms.push("3k".to_owned());
        }
        let mut named = [false; 3];
        if self.blk1 {
            if self.blk2 && self.blk3 {
                terms.push("24k".to_owned());
                named = [true, true, true];
            } else if self.blk2 {
                terms.push("16k".to_owned());
                named = [true, true, false];
            } else {
                terms.push("8k".to_owned());
                named[0] = true;
            }
        }
        for (slot, (present, at)) in [
            (self.blk1, "2000"),
            (self.blk2, "4000"),
            (self.blk3, "6000"),
        ]
        .into_iter()
        .enumerate()
        {
            if present && !named[slot] {
                terms.push(format!("8k@{at}"));
            }
        }
        if self.blk5 {
            terms.push("8k@a000".to_owned());
        }
        if terms.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&terms.join("+"))
        }
    }
}

/// VIC-20 machine.
///
/// Fully serialisable for save-states: the 6502, both 6522 VIAs, the VIC-I,
/// the keyboard matrix, all RAM banks, colour RAM, and the ROMs carry live
/// state so a restore resumes exactly.
#[derive(Serialize, Deserialize)]
pub struct Vic20 {
    cpu: M6502,
    #[serde(with = "BigArray")]
    ram_low: [u8; 0x0400],
    #[serde(with = "BigArray")]
    ram_exp_low: [u8; 0x0C00],
    #[serde(with = "BigArray")]
    ram_main: [u8; 0x1000],
    /// BLK1 `$2000`, BLK2 `$4000`, BLK3 `$6000`, BLK5 `$A000`. A slot holds
    /// 8 KiB when its cartridge is fitted and is empty when it is not.
    ram_blocks: [Vec<u8>; 4],
    expansion: Vic20RamExpansion,
    #[serde(with = "BigArray")]
    colour_ram: [u8; 0x0400],
    char_rom: Vec<u8>,
    basic_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    /// Statically decoded ROM regions supplied by a generic cartridge.
    cartridge_rom: Vec<(u16, Vec<u8>)>,
    vic: Vic6560,
    keyboard: KeyboardState,
    /// VIA #1 ($9110-$911F): RESTORE key (CA1 → NMI), user port. Its IRQ
    /// output is wired to the 6502 NMI pin.
    via1: Via6522,
    /// VIA #2 ($9120-$912F): keyboard scan (PB columns, PA rows) and the
    /// Timer 1 free-run that generates the 60 Hz system IRQ.
    via2: Via6522,
    model: Vic20Model,
    master_clock: u64,
    frame_count: u64,
    /// DE-9 control-port switch state, active low. Up/down/left/fire sit on
    /// VIA #1 PA2-PA5 (`$9111`); right is the awkward one, on VIA #2 PB7
    /// (`$9120`). `joy_via1_pa` carries the active-low PA2-PA5 pattern (other
    /// bits held high so it merges cleanly); `joy_right_low` is the PB7 line.
    /// Both default to idle and are merged into the VIA input latches each
    /// tick. See the VIC-20 Programmer's Reference Guide control-port table.
    joy_via1_pa: u8,
    joy_right_low: bool,
    /// External level presented at user-port PB0 (pin C). The line idles high,
    /// matching an unattached RS-232 adapter, and is folded through VIA #1's
    /// DDR on every cycle rather than injected into a register read.
    #[serde(default = "default_high")]
    user_port_pb0_input: bool,
}

const fn default_high() -> bool {
    true
}

impl Vic20 {
    /// Create a new VIC-20. ROMs: `kernal` 8 KB, `basic` 8 KB, `char_rom` 4 KB.
    /// `expansion` names the RAM cartridges fitted to the expansion port; each
    /// is independent, so [`Vic20RamExpansion::EXP_8K`] installs BLK1 and
    /// leaves `$0400-$0FFF` as open bus, exactly as a VIC-1110 on its own does.
    pub fn new(
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        char_rom: Vec<u8>,
        model: Vic20Model,
        expansion: Vic20RamExpansion,
    ) -> Self {
        let pal = model == Vic20Model::Pal;
        let ram_blocks = core::array::from_fn(|slot| {
            if expansion.slot_populated(slot) {
                vec![0u8; 0x2000]
            } else {
                Vec::new()
            }
        });
        // Run the 6502 reset sequence so the first fetch comes from the KERNAL
        // reset vector ($FFFC). Without this the CPU powers on at PC=$0000,
        // executes the BRK there, and storms in the IRQ/BRK handler instead of
        // cold-starting the KERNAL.
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            ram_low: [0; 0x0400],
            ram_exp_low: [0; 0x0C00],
            ram_main: [0; 0x1000],
            ram_blocks,
            expansion,
            colour_ram: [0; 0x0400],
            char_rom,
            basic_rom,
            kernal_rom,
            cartridge_rom: Vec::new(),
            vic: Vic6560::new(pal),
            keyboard: KeyboardState::new(),
            via1: Via6522::new(),
            via2: Via6522::new(),
            model,
            master_clock: 0,
            frame_count: 0,
            joy_via1_pa: 0xFF,
            joy_right_low: false,
            user_port_pb0_input: true,
        }
    }

    /// Set the DE-9 control-port joystick switches (`true` = pressed). The VIC-20
    /// has a single control port: up/down/left/fire land on VIA #1 PA2-PA5 and
    /// right lands on VIA #2 PB7, all active low. The values are merged into the
    /// VIA input latches on the next tick, leaving the IEC, cassette, and
    /// keyboard-column bits untouched.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool, fire: bool) {
        let mut pa = 0xFFu8;
        for (pressed, bit) in [(up, 0x04), (down, 0x08), (left, 0x10), (fire, 0x20)] {
            if pressed {
                pa &= !bit;
            }
        }
        self.joy_via1_pa = pa;
        self.joy_right_low = right;
    }

    /// Drive the external level at user-port PB0 (pin C).
    ///
    /// The level reaches VIA #1's port-B input latch on the next machine
    /// cycle. When DDRB0 is configured as an output the VIA's output latch
    /// wins, exactly as it does at the physical pin.
    pub fn set_user_port_pb0(&mut self, high: bool) {
        self.user_port_pb0_input = high;
    }

    /// Read the effective logic level at user-port PB0 (pin C), after DDRB.
    #[must_use]
    pub fn user_port_pb0(&self) -> bool {
        self.via1.compose_port_b_read(self.via1.pb_in) & 0x01 != 0
    }

    /// Read the effective logic level at user-port CB2 (pin M).
    ///
    /// This exposes the VIA pin, not the PCR register encoding, so a host
    /// peripheral observes bit-banged transitions produced by real VIA
    /// execution.
    #[must_use]
    pub fn user_port_cb2(&self) -> bool {
        if self.via1.cb2_drive {
            self.via1.cb2_out
        } else {
            self.via1.cb2
        }
    }

    pub fn run_frame(&mut self) -> u64 {
        self.run_frame_with_user_port(&mut |_| true)
    }

    /// Run one frame while exchanging the physical user-port serial lines on
    /// every CPU cycle. The callback observes CB2 (pin M, computer TX) and
    /// returns the external PB0 level (pin C, computer RX).
    pub fn run_frame_with_user_port<F>(&mut self, exchange: &mut F) -> u64
    where
        F: FnMut(bool) -> bool,
    {
        let start = self.master_clock;
        for _ in 0..200_000 {
            let pb0 = exchange(self.user_port_cb2());
            self.set_user_port_pb0(pb0);
            self.tick_cycle();
            if self.vic.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_cycle(&mut self) {
        self.master_clock += 1;
        // Tick VIC chip with callbacks for screen RAM, colour RAM, char ROM reads.
        let ram_low = &self.ram_low;
        let ram_exp_low = &self.ram_exp_low;
        let ram_main = &self.ram_main;
        let has_exp_low = self.expansion.exp_3k;
        let colour_ram = &self.colour_ram;
        let char_rom = &self.char_rom;
        self.vic.tick(
            |addr| {
                read_vic_memory(
                    addr,
                    ram_low,
                    ram_exp_low,
                    ram_main,
                    has_exp_low,
                    colour_ram,
                    char_rom,
                )
            },
            |addr| colour_ram[(addr & 0x03FF) as usize],
            |addr| {
                read_vic_memory(
                    addr,
                    ram_low,
                    ram_exp_low,
                    ram_main,
                    has_exp_low,
                    colour_ram,
                    char_rom,
                )
            },
        );

        // Refresh the keyboard row input: VIA #2 port B drives the
        // column-select pattern (active low); the matrix returns the row
        // lines for the selected columns, which the KERNAL reads on PA.
        self.via2.pa_in = self.keyboard.read(self.via2.port_b_drive_state());

        // Merge the control-port joystick into the VIA input latches:
        // up/down/left/fire on VIA #1 PA2-PA5, right on VIA #2 PB7 (all active
        // low). Read-modify-write leaves the IEC / cassette bits on PA and the
        // keyboard-column bits on PB untouched.
        self.via1.pa_in = (self.via1.pa_in & !0x3C) | (self.joy_via1_pa & 0x3C);
        if self.user_port_pb0_input {
            self.via1.pb_in |= 0x01;
        } else {
            self.via1.pb_in &= !0x01;
        }
        if self.joy_right_low {
            self.via2.pb_in &= !0x80;
        } else {
            self.via2.pb_in |= 0x80;
        }

        self.via1.tick();
        self.via2.tick();
        // VIA #2 generates the system IRQ (Timer 1 jiffy / keyboard scan);
        // VIA #1 generates the NMI (RESTORE key / RS-232).
        self.cpu.irq = self.via2.irq;
        self.cpu.nmi = self.via1.irq;

        self.cpu.tick();
        if self.cpu.rw {
            let addr = self.cpu.addr;
            // VIA register reads have side effects (reading Timer 1 clears
            // its interrupt flag — how the KERNAL acks the jiffy IRQ), so
            // the live CPU path uses the mutating `read`; `mem_read`/`peek`
            // stay non-mutating for host debugging.
            self.cpu.data_in = match addr {
                0x9110..=0x911F => self.via1.read((addr & 0x0F) as u8),
                0x9120..=0x912F => self.via2.read((addr & 0x0F) as u8),
                _ => self.mem_read(addr),
            };
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        if let Some(byte) = self.cartridge_read(addr) {
            return byte;
        }
        match addr {
            0x0000..=0x03FF => self.ram_low[addr as usize],
            0x0400..=0x0FFF => {
                if self.expansion.exp_3k {
                    self.ram_exp_low[(addr - 0x0400) as usize]
                } else {
                    0xFF
                }
            }
            0x1000..=0x1FFF => self.ram_main[(addr - 0x1000) as usize],
            0x2000..=0x7FFF => self.block_read(addr),
            0x8000..=0x8FFF => self
                .char_rom
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x9000..=0x90FF => self.vic.read((addr & 0x0F) as u8),
            0x9110..=0x911F => self.via1.peek((addr & 0x0F) as u8),
            0x9120..=0x912F => self.via2.peek((addr & 0x0F) as u8),
            0x9100..=0x910F | 0x9130..=0x93FF => 0xFF,
            0x9400..=0x97FF => self.colour_ram[(addr - 0x9400) as usize] & 0x0F,
            0x9800..=0x9FFF => 0xFF,
            // $A000-$BFFF is BLK5. A ROM cartridge (autostart carts) is read
            // above via `cartridge_read`, so what is left here is a RAM
            // cartridge DIP-switched to BLK5, else open bus. The VIC-20 —
            // unlike the C64 — puts BASIC at $C000-$DFFF and KERNAL at
            // $E000-$FFFF.
            0xA000..=0xBFFF => self.block_read(addr),
            0xC000..=0xDFFF => self
                .basic_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xE000..=0xFFFF => self
                .kernal_rom
                .get((addr - 0xE000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    /// Read an expansion-block address: the fitted cartridge's RAM, else open
    /// bus. A ROM cartridge in the same window is resolved before this by
    /// `cartridge_read`.
    fn block_read(&self, addr: u16) -> u8 {
        Vic20RamExpansion::slot_for(addr)
            .and_then(|slot| self.ram_blocks[slot].get((addr & 0x1FFF) as usize).copied())
            .unwrap_or(0xFF)
    }

    fn block_write(&mut self, addr: u16, value: u8) {
        if let Some(slot) = Vic20RamExpansion::slot_for(addr)
            && let Some(cell) = self.ram_blocks[slot].get_mut((addr & 0x1FFF) as usize)
        {
            *cell = value;
        }
    }

    /// RAM expansion cartridges fitted to the expansion port.
    #[must_use]
    pub const fn expansion(&self) -> Vic20RamExpansion {
        self.expansion
    }

    fn cartridge_read(&self, addr: u16) -> Option<u8> {
        self.cartridge_rom.iter().find_map(|(start, bytes)| {
            let offset = addr.checked_sub(*start).map(usize::from)?;
            bytes.get(offset).copied()
        })
    }

    /// Insert a generic VICE CRT or raw BLK5 cartridge image.
    ///
    /// Static CHIP packets may map into BLK1, BLK2, BLK3, and BLK5. They take
    /// priority over RAM in those windows, as a ROM cartridge's decode lines
    /// do on the machine. Bank-switched hardware types are rejected by the
    /// format parser until their I/O latch behaviour is modelled.
    ///
    /// # Errors
    ///
    /// Returns a readable parse/mapping error for malformed or unsupported
    /// cartridge images.
    pub fn insert_cartridge_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let cartridge = format_commodore_vic_20_crt::parse(bytes).map_err(|e| e.to_string())?;
        self.cartridge_rom = cartridge
            .blocks
            .into_iter()
            .map(|block| (block.load_address, block.data))
            .collect();
        Ok(())
    }

    /// Remove the cartridge, exposing RAM/open bus in its blocks again.
    pub fn remove_cartridge(&mut self) {
        self.cartridge_rom.clear();
    }

    /// Whether BLK5 carries the VIC-20 KERNAL's `A0` + high-bit `CBM` signature.
    #[must_use]
    pub fn cartridge_is_autostart(&self) -> bool {
        self.cartridge_rom.iter().any(|(start, bytes)| {
            let Some(offset) = 0xA004u16.checked_sub(*start).map(usize::from) else {
                return false;
            };
            offset + 5 <= bytes.len() && bytes[offset..offset + 5] == [0x41, 0x30, 0xC3, 0xC2, 0xCD]
        })
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x03FF => self.ram_low[addr as usize] = value,
            0x0400..=0x0FFF if self.expansion.exp_3k => {
                self.ram_exp_low[(addr - 0x0400) as usize] = value;
            }
            0x1000..=0x1FFF => self.ram_main[(addr - 0x1000) as usize] = value,
            0x2000..=0x7FFF | 0xA000..=0xBFFF => self.block_write(addr, value),
            0x9000..=0x90FF => self.vic.write((addr & 0x0F) as u8, value),
            0x9110..=0x911F => self.via1.write((addr & 0x0F) as u8, value),
            0x9120..=0x912F => self.via2.write((addr & 0x0F) as u8, value),
            0x9100..=0x910F | 0x9130..=0x93FF => {}
            0x9400..=0x97FF => {
                self.colour_ram[(addr - 0x9400) as usize] = value & 0x0F;
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vic.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vic.framebuffer_height()
    }

    pub fn press_key(&mut self, key: Vic20Key) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, true);
    }

    pub fn release_key(&mut self, key: Vic20Key) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Write one byte through the bus (RAM accepts it; ROM / unmapped
    /// addresses ignore it). For host debugging (`poke_*` MCP tools).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502 instruction, returning the cycles it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let start = self.master_clock;
        let cap = start + 1024;
        while self.cpu.instruction_complete() && self.master_clock < cap {
            self.tick_cycle();
        }
        while !self.cpu.instruction_complete() && self.master_clock < cap {
            self.tick_cycle();
        }
        self.master_clock - start
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub fn model(&self) -> Vic20Model {
        self.model
    }

    #[must_use]
    pub fn vic(&self) -> &Vic6560 {
        &self.vic
    }

    /// Drains the VIC's host-rate audio samples produced since the last call
    /// (mono f32). The runtime pumps these into the host audio sink each frame.
    #[must_use]
    pub fn take_vic_audio(&mut self) -> Vec<f32> {
        self.vic.take_audio()
    }

    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

/// Read the VIC-I's 14-bit address space.
///
/// Motherboard wiring maps VIC `$0000-$1FFF` to CPU `$8000-$9FFF` and VIC
/// `$2000-$3FFF` to CPU `$0000-$1FFF`. Keeping this translation at the
/// machine boundary lets programmable screen and character bases reach lower
/// RAM instead of aliasing every fetch into CPU `$1000-$1FFF`.
#[allow(clippy::too_many_arguments)]
fn read_vic_memory(
    addr: u16,
    ram_low: &[u8; 0x0400],
    ram_exp_low: &[u8; 0x0C00],
    ram_main: &[u8; 0x1000],
    has_exp_low: bool,
    colour_ram: &[u8; 0x0400],
    char_rom: &[u8],
) -> u8 {
    let addr = addr & 0x3FFF;
    match addr {
        0x0000..=0x0FFF => char_rom.get(addr as usize).copied().unwrap_or(0xFF),
        0x1000..=0x13FF | 0x1800..=0x1FFF => 0xFF,
        0x1400..=0x17FF => colour_ram[(addr - 0x1400) as usize] & 0x0F,
        0x2000..=0x23FF => ram_low[(addr - 0x2000) as usize],
        0x2400..=0x2FFF => {
            if has_exp_low {
                ram_exp_low[(addr - 0x2400) as usize]
            } else {
                0xFF
            }
        }
        0x3000..=0x3FFF => ram_main[(addr - 0x3000) as usize],
        _ => unreachable!("VIC address is masked to 14 bits"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vic20() -> Vic20 {
        let mut kernal = vec![0xEAu8; 0x2000];
        // Reset vector at $FFFC/$FFFD → $E000.
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        Vic20::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Pal,
            Vic20RamExpansion::NONE,
        )
    }

    /// Save-state must capture LIVE machine state (6502 + both 6522 VIAs +
    /// VIC-I + keyboard + all RAM banks + colour RAM + ROMs), not cold-boot
    /// from the ROMs. Run a frame, poke a work-RAM byte, advance, serialise;
    /// advance again and confirm the serialised state changed; then deserialise
    /// the first snapshot and confirm re-serialising it is byte-identical.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = make_vic20();
        sys.run_frame();
        sys.poke(0x1000, 0xA5); // a main-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0x1000), 0xA5);
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Vic20 = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn ram_round_trips() {
        let mut sys = make_vic20();
        sys.mem_write(0x0000, 0x42);
        assert_eq!(sys.mem_read(0x0000), 0x42);
    }

    #[test]
    fn main_ram_round_trips() {
        let mut sys = make_vic20();
        sys.mem_write(0x1000, 0xAB);
        assert_eq!(sys.mem_read(0x1000), 0xAB);
    }

    #[test]
    fn colour_ram_masks_to_nibble() {
        let mut sys = make_vic20();
        sys.mem_write(0x9400, 0xFF);
        assert_eq!(sys.mem_read(0x9400), 0x0F);
    }

    #[test]
    fn vic_address_bus_reaches_rom_colour_and_each_visible_ram_bank() {
        let mut sys = make_vic20();
        sys.char_rom[0x0123] = 0x11;
        sys.colour_ram[0x0234] = 0xF2;
        sys.ram_low[0x0345] = 0x33;
        sys.ram_main[0x0456] = 0x44;

        let read = |addr| {
            read_vic_memory(
                addr,
                &sys.ram_low,
                &sys.ram_exp_low,
                &sys.ram_main,
                sys.expansion.exp_3k,
                &sys.colour_ram,
                &sys.char_rom,
            )
        };
        assert_eq!(read(0x0123), 0x11, "VIC $0123 maps to character ROM");
        assert_eq!(read(0x1634), 0x02, "VIC $1634 maps to colour RAM");
        assert_eq!(read(0x2345), 0x33, "VIC $2345 maps to CPU $0345");
        assert_eq!(read(0x3456), 0x44, "VIC $3456 maps to CPU $1456");
        assert_eq!(read(0x2456), 0xFF, "absent 3 KiB expansion is open bus");

        let mut expanded = Vic20::new(
            vec![0; 0x2000],
            vec![0; 0x2000],
            vec![0; 0x1000],
            Vic20Model::Pal,
            Vic20RamExpansion::EXP_3K,
        );
        expanded.ram_exp_low[0x0567] = 0x55;
        assert_eq!(
            read_vic_memory(
                0x2967,
                &expanded.ram_low,
                &expanded.ram_exp_low,
                &expanded.ram_main,
                expanded.expansion.exp_3k,
                &expanded.colour_ram,
                &expanded.char_rom,
            ),
            0x55,
            "VIC $2967 maps to expanded CPU $0967"
        );
    }

    #[test]
    fn rom_writes_ignored() {
        let mut sys = make_vic20();
        sys.mem_write(0xE000, 0xFF);
        assert_eq!(sys.mem_read(0xE000), 0xEA);
    }

    #[test]
    fn frame_advances_count() {
        let mut sys = make_vic20();
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ntsc_runs() {
        let mut kernal = vec![0xEAu8; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        let mut sys = Vic20::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Ntsc,
            Vic20RamExpansion::NONE,
        );
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
        assert_eq!(sys.model(), Vic20Model::Ntsc);
    }

    #[test]
    fn keyboard_scan_reads_through_via2() {
        // ROM-free guard for the VIA #2 keyboard path: configure port B as
        // the column-drive output and port A as the row-read input (as the
        // KERNAL does), press a key, drive its column low, and confirm the
        // key's row line reads low on port A.
        let mut sys = make_vic20();
        sys.mem_write(0x9122, 0xFF); // VIA2 DDRB: port B all outputs (columns)
        sys.mem_write(0x9123, 0x00); // VIA2 DDRA: port A all inputs (rows)

        // 'A' is matrix (row 1, col 2): pressing it shorts PA1 to PB2.
        sys.press_key(Vic20Key::A);
        sys.mem_write(0x9120, !(1 << 2)); // drive column 2 low, others high
        let _ = sys.step_instruction(); // let tick refresh the row input

        let port_a = sys.mem_read(0x9121);
        assert_eq!(
            port_a & (1 << 1),
            0,
            "PA1 should read low for 'A'; got {port_a:#04X}"
        );

        // A column that no pressed key occupies leaves every row line high.
        sys.mem_write(0x9120, !(1 << 5)); // drive an unrelated column low
        let _ = sys.step_instruction();
        assert_eq!(
            sys.mem_read(0x9121),
            0xFF,
            "no key on column 5 → all rows high"
        );
    }

    #[test]
    fn joystick_reads_through_the_vias() {
        // Up/down/left/fire are VIA #1 PA2-PA5 (read at $9111, port A is input
        // by default); right is VIA #2 PB7 (read at $9120). All active low.
        let mut sys = make_vic20();

        sys.set_joystick(true, false, true, false, true); // up + left + fire
        let _ = sys.step_instruction(); // let tick merge the switches

        let pa = sys.mem_read(0x9111);
        assert_eq!(pa & (1 << 2), 0, "up → PA2 low");
        assert_eq!(pa & (1 << 4), 0, "left → PA4 low");
        assert_eq!(pa & (1 << 5), 0, "fire → PA5 low");
        assert_eq!(pa & (1 << 3), 1 << 3, "down idle → PA3 high");

        // Right lives on the awkward VIA #2 PB7 line, not PA.
        sys.set_joystick(false, false, false, true, false);
        let _ = sys.step_instruction();
        assert_eq!(sys.mem_read(0x9120) & (1 << 7), 0, "right → PB7 low");
        // PA directions all idle again.
        assert_eq!(sys.mem_read(0x9111) & 0x3C, 0x3C, "no PA directions held");

        // Release everything → all control lines idle high.
        sys.set_joystick(false, false, false, false, false);
        let _ = sys.step_instruction();
        assert_eq!(sys.mem_read(0x9111) & 0x3C, 0x3C, "PA2-PA5 idle high");
        assert_eq!(sys.mem_read(0x9120) & (1 << 7), 1 << 7, "PB7 idle high");
    }

    #[test]
    fn user_port_pb0_external_level_reaches_via1_through_ddr() {
        let mut sys = make_vic20();
        sys.mem_write(0x9112, 0x00); // VIA1 DDRB: PB0 input

        sys.set_user_port_pb0(false);
        sys.tick_cycle();
        assert_eq!(sys.mem_read(0x9110) & 0x01, 0, "external PB0 low");
        assert!(!sys.user_port_pb0());

        sys.set_user_port_pb0(true);
        sys.tick_cycle();
        assert_eq!(sys.mem_read(0x9110) & 0x01, 1, "external PB0 high");
        assert!(sys.user_port_pb0());

        // Output mode must expose the VIA latch, not the external input.
        sys.mem_write(0x9112, 0x01);
        sys.mem_write(0x9110, 0x00);
        assert!(!sys.user_port_pb0(), "DDRB0 output low wins");
        sys.mem_write(0x9110, 0x01);
        assert!(sys.user_port_pb0(), "DDRB0 output high wins");
    }

    #[test]
    fn user_port_cb2_exposes_via1_pin_transitions() {
        let mut sys = make_vic20();

        sys.mem_write(0x911C, 0xC0); // PCR: CB2 manual output low
        assert!(!sys.user_port_cb2());

        sys.mem_write(0x911C, 0xE0); // PCR: CB2 manual output high
        assert!(sys.user_port_cb2());
    }

    #[test]
    fn frame_user_port_exchange_runs_at_cycle_granularity() {
        let mut sys = make_vic20();
        sys.mem_write(0x9112, 0x00); // PB0 input
        let mut calls = 0u64;
        let ticks = sys.run_frame_with_user_port(&mut |cb2| {
            assert!(cb2, "unattached CB2 should idle high");
            calls += 1;
            calls & 1 == 0
        });
        assert_eq!(calls, ticks);
        assert_eq!(sys.user_port_pb0(), calls & 1 == 0);
    }

    #[test]
    fn expansion_ram_low() {
        let mut sys = Vic20::new(
            vec![0xEA; 0x2000],
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Pal,
            Vic20RamExpansion::EXP_3K,
        );
        sys.mem_write(0x0400, 0x55);
        assert_eq!(sys.mem_read(0x0400), 0x55);
    }

    fn machine_with(expansion: Vic20RamExpansion) -> Vic20 {
        Vic20::new(
            vec![0xEA; 0x2000],
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Pal,
            expansion,
        )
    }

    #[test]
    fn an_8k_cartridge_alone_leaves_the_3k_area_as_open_bus() {
        // A VIC-1110 fills BLK1 only. The 3K expander is a separate product,
        // so $0400-$0FFF must stay unpopulated.
        let mut sys = machine_with(Vic20RamExpansion::EXP_8K);
        sys.mem_write(0x0400, 0x55);
        sys.mem_write(0x0FFF, 0x55);
        assert_eq!(sys.mem_read(0x0400), 0xFF, "$0400 has no RAM behind it");
        assert_eq!(sys.mem_read(0x0FFF), 0xFF, "$0FFF has no RAM behind it");

        sys.mem_write(0x2000, 0x42);
        sys.mem_write(0x3FFF, 0x43);
        assert_eq!(sys.mem_read(0x2000), 0x42, "BLK1 is fitted");
        assert_eq!(sys.mem_read(0x3FFF), 0x43, "BLK1 runs to $3FFF");
        assert_eq!(sys.mem_read(0x4000), 0xFF, "BLK2 is not fitted");
    }

    #[test]
    fn a_3k_expander_alone_leaves_the_blocks_as_open_bus() {
        let mut sys = machine_with(Vic20RamExpansion::EXP_3K);
        sys.mem_write(0x0400, 0x55);
        assert_eq!(sys.mem_read(0x0400), 0x55, "the 3K expander is fitted");
        sys.mem_write(0x2000, 0x42);
        assert_eq!(sys.mem_read(0x2000), 0xFF, "BLK1 is not fitted");
    }

    #[test]
    fn a_dip_switched_cartridge_populates_only_its_own_block() {
        // "8k@4000": BLK2 answers, BLK1 and BLK3 stay empty.
        let expansion = Vic20RamExpansion::parse("8k@4000").expect("valid spec");
        let mut sys = machine_with(expansion);
        for (addr, fitted) in [(0x2000u16, false), (0x4000, true), (0x6000, false)] {
            sys.mem_write(addr, 0x42);
            let expected = if fitted { 0x42 } else { 0xFF };
            assert_eq!(sys.mem_read(addr), expected, "${addr:04X}");
        }
    }

    #[test]
    fn blk5_ram_answers_when_no_rom_cartridge_claims_it() {
        let expansion = Vic20RamExpansion::parse("8k@a000").expect("valid spec");
        let mut sys = machine_with(expansion);
        sys.mem_write(0xA000, 0x37);
        sys.mem_write(0xBFFF, 0x38);
        assert_eq!(sys.mem_read(0xA000), 0x37);
        assert_eq!(sys.mem_read(0xBFFF), 0x38);

        let mut bare = machine_with(Vic20RamExpansion::NONE);
        bare.mem_write(0xA000, 0x37);
        assert_eq!(bare.mem_read(0xA000), 0xFF, "empty BLK5 is open bus");
    }

    #[test]
    fn expansion_spec_round_trips_and_rejects_a_double_claim() {
        for (spec, expected) in [
            ("none", Vic20RamExpansion::NONE),
            ("3k", Vic20RamExpansion::EXP_3K),
            ("8k", Vic20RamExpansion::EXP_8K),
            ("16k", Vic20RamExpansion::EXP_16K),
            ("24k", Vic20RamExpansion::EXP_24K),
        ] {
            assert_eq!(Vic20RamExpansion::parse(spec), Ok(expected), "{spec}");
            assert_eq!(expected.to_string(), spec, "display of {spec}");
        }

        let module = Vic20RamExpansion::parse("3k+8k").expect("VIC-1010 combination");
        assert!(module.exp_3k && module.blk1);
        assert_eq!(module.to_string(), "3k+8k");
        assert_eq!(module.total_kib(), 11);

        let split = Vic20RamExpansion::parse("8k@2000+8k@6000").expect("two DIP-switched packs");
        assert_eq!(split.to_string(), "8k+8k@6000");
        assert!(
            !split.blk2,
            "BLK2 stays empty, so BASIC cannot follow past $3FFF"
        );

        let clash = Vic20RamExpansion::parse("8k+8k@2000").expect_err("same block twice");
        assert!(clash.contains("BLK1"), "{clash}");
        let unknown = Vic20RamExpansion::parse("32k").expect_err("no such cartridge");
        assert!(unknown.contains("unknown RAM expansion term"), "{unknown}");
    }

    #[test]
    fn basic_start_follows_the_fitted_cartridges() {
        assert_eq!(Vic20RamExpansion::NONE.basic_start(), 0x1000);
        assert_eq!(Vic20RamExpansion::EXP_3K.basic_start(), 0x0400);
        assert_eq!(Vic20RamExpansion::EXP_8K.basic_start(), 0x1200);
        assert_eq!(Vic20RamExpansion::EXP_24K.basic_start(), 0x1200);
        // With both fitted the 3K area is outside the BASIC program area, so
        // BASIC still starts at $1200.
        let module = Vic20RamExpansion::parse("3k+8k").expect("valid spec");
        assert_eq!(module.basic_start(), 0x1200);
    }

    #[test]
    fn cartridge_blocks_overlay_ram_and_blk5_and_can_be_removed() {
        fn crt(chips: &[(u16, u8)]) -> Vec<u8> {
            let mut bytes = Vec::from(*b"VIC20 CARTRIDGE ");
            bytes.extend_from_slice(&0x40u32.to_be_bytes());
            bytes.extend_from_slice(&0x0100u16.to_be_bytes());
            bytes.extend_from_slice(&0u16.to_be_bytes());
            bytes.extend_from_slice(&[0; 8 + 32]);
            for (address, fill) in chips {
                bytes.extend_from_slice(b"CHIP");
                bytes.extend_from_slice(&0x2010u32.to_be_bytes());
                bytes.extend_from_slice(&[0; 4]);
                bytes.extend_from_slice(&address.to_be_bytes());
                bytes.extend_from_slice(&0x2000u16.to_be_bytes());
                bytes.extend(std::iter::repeat_n(*fill, 0x2000));
            }
            bytes
        }

        let mut sys = make_vic20();
        sys.mem_write(0x2000, 0x11);
        let image = crt(&[(0x2000, 0x22), (0xA000, 0x33)]);
        sys.insert_cartridge_bytes(&image).expect("generic CRT");
        assert_eq!(sys.peek(0x2000), 0x22, "BLK1 cartridge overlays RAM");
        assert_eq!(sys.peek(0xA000), 0x33, "BLK5 cartridge replaces open bus");

        sys.remove_cartridge();
        assert_eq!(
            sys.peek(0x2000),
            0xFF,
            "unexpanded BLK1 returns to open bus"
        );
        assert_eq!(sys.peek(0xA000), 0xFF, "BLK5 returns to open bus");
    }

    #[test]
    fn raw_blk5_autostart_signature_is_visible_to_the_kernal() {
        let mut sys = make_vic20();
        let mut image = vec![0; 0x2000];
        image[0..4].copy_from_slice(&[0x09, 0xA0, 0x09, 0xA0]);
        image[4..9].copy_from_slice(&[0x41, 0x30, 0xC3, 0xC2, 0xCD]);
        sys.insert_cartridge_bytes(&image).expect("raw BLK5 ROM");
        assert!(sys.cartridge_is_autostart());
        assert_eq!(&[sys.peek(0xA004), sys.peek(0xA005)], &[0x41, 0x30]);
    }
}
