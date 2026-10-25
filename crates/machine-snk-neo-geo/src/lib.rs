//! The SNK Neo Geo AES.
//!
//! A 68000 at the 24 MHz master clock divided by 2, a Z80 at the master
//! clock divided by 6, the LSPC2 video chip, and a YM2610 at the master
//! clock divided by 3, wired the way the home console wires them. One call
//! to [`NeoGeo::tick`] is one master clock, and every chip is stepped from
//! it.
//!
//! # The 68000's map
//!
//! | Range | Contents |
//! |---|---|
//! | `$000000-$00007F` | Vectors: the BIOS's or the cartridge's, by `$3A0003`/`$3A0013` |
//! | `$000080-$0FFFFF` | The first megabyte of P-ROM |
//! | `$100000-$1FFFFF` | 64 KB of work RAM, mirrored |
//! | `$200000-$2FFFFF` | A megabyte of P-ROM, banked by a write to `$2FFFF0-$2FFFFF` |
//! | `$300000` | Controller 1 (read, upper byte) |
//! | `$320000` | Sound command (write) and the Z80's reply (read), upper byte |
//! | `$340000` | Controller 2 (read, upper byte) |
//! | `$380000` | Start, select and memory-card status (read); output and card bank (write, odd bytes) |
//! | `$3A0000-$3A001F` | System latch, odd bytes: shadow, vectors, card, palette bank |
//! | `$3C0000-$3C000F` | LSPC2 registers |
//! | `$400000-$401FFF` | Palette RAM, mirrored to `$7FFFFF` |
//! | `$800000-$BFFFFF` | Memory card |
//! | `$C00000-$C1FFFF` | BIOS, mirrored to `$CFFFFF` |
//!
//! Unmapped reads return the word the 68000 last prefetched.
//!
//! # The Z80's map
//!
//! | Range | Contents |
//! |---|---|
//! | `$0000-$7FFF` | M-ROM |
//! | `$8000-$BFFF` | 16 KB M-ROM bank, set by `IN` from port `$0B` |
//! | `$C000-$DFFF` | 8 KB bank, port `$0A` |
//! | `$E000-$EFFF` | 4 KB bank, port `$09` |
//! | `$F000-$F7FF` | 2 KB bank, port `$08` |
//! | `$F800-$FFFF` | 2 KB of RAM |
//!
//! A bank is chosen by reading its port with the bank number on A8-A15.
//! Port `$00` reads the 68000's command, port `$0C` writes the reply, ports
//! `$04-$07` are the YM2610, and `OUT` to `$08` or `$18` enables or disables
//! the NMI that a new command raises. The YM2610 drives /INT.
//!
//! # What the AES lacks
//!
//! The arcade MVS has a uPD4990A clock, a watchdog, battery-backed RAM, the
//! SFIX and SM1 BIOS ROMs, and the latch bits that switch between them. The
//! AES has none of these: the fix layer and the Z80 always read the
//! cartridge. This crate models the AES, following MAME's `aes` driver
//! (`neogeo.cpp`, BSD-3-Clause).
//!
//! # Not yet modelled
//!
//! 68000 wait states other than the memory card's, the cartridge
//! protection and banking chips some later games carry, and Garou-style
//! fix-layer banking.

pub mod cartridge;
pub mod input;

use emu198x_snk_lspc2::{Lspc2, VideoRoms};
use emu198x_yamaha_ym2610::Ym2610;
use emu198x_zilog_z80::{BusOp, Z80};
use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use serde::{Deserialize, Serialize};

pub use cartridge::Cartridge;
pub use input::Buttons;

/// The master clock.
pub const MASTER_HZ: u64 = 24_000_000;
/// Master clocks per 68000 clock.
pub const MCLKS_PER_68K: u8 = 2;
/// Master clocks per YM2610 clock.
pub const MCLKS_PER_YM: u8 = 3;
/// Master clocks per Z80 half-cycle: the Z80 runs at the master clock over
/// 6, and its core steps half a clock at a time.
pub const MCLKS_PER_Z80_HALF: u8 = 3;
/// The rate [`NeoGeo::take_audio`] delivers.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// Master clocks per YM2610 sample: 3 per YM clock, 144 YM clocks a sample.
const MCLKS_PER_YM_SAMPLE: u64 = 3 * 144;
/// The BIOS's size.
pub const BIOS_SIZE: usize = 0x2_0000;
/// The 68000 clocks a memory-card access waits.
const CARD_WAIT: u8 = 2;
/// A standard memory card: 2 KB.
pub const MEMORY_CARD_SIZE: usize = 0x800;

/// Output gains: the FM and ADPCM mix is 16-bit; the SSG level is 0.0 to
/// 1.0 for its three channels together. The balance, 0.98 to 0.84 across
/// ymfm's full-scale SSG of three channels at half range, is MAME's.
const FM_GAIN: f32 = 0.5 * 0.98 / 32768.0;
const SSG_GAIN: f32 = 0.5 * 0.84 * 1.5;

/// A Neo Geo AES with a cartridge in.
#[derive(Serialize, Deserialize)]
pub struct NeoGeo {
    pub cpu: Cpu68000,
    pub z80: Z80,
    pub lspc: Lspc2,
    pub ym2610: Ym2610,
    /// The two controllers.
    pub buttons: [Buttons; 2],

    /// The cartridge, the BIOS and the L0 shrink ROM are media, not state:
    /// a snapshot leaves them out and [`NeoGeo::attach_media`] puts them
    /// back.
    #[serde(skip)]
    cartridge: Cartridge,
    #[serde(skip)]
    bios: Vec<u8>,
    #[serde(skip)]
    zoom: Vec<u8>,

    ram: Vec<u8>,
    z80_ram: Vec<u8>,
    memory_card: Option<Vec<u8>>,

    mclk: u64,
    phase: u8,

    /// The eight outputs of the system latch.
    latch: u8,
    card_bank: u8,
    card_wait: u8,
    /// Where the `$200000` window starts in P-ROM.
    p_bank_base: u32,

    sound_command: u8,
    sound_pending: bool,
    sound_reply: u8,
    z80_nmi_enabled: bool,
    /// The M-ROM bank in each window: `$F000`, `$E000`, `$C000`, `$8000`.
    z80_banks: [u8; 4],

    /// The YM2610 resampler: a phase in master clocks times 48000, and the
    /// sums and count of the samples in the output sample being built.
    ym_phase: u64,
    ym_sum: [f32; 2],
    ym_count: u32,
    #[serde(skip)]
    audio: Vec<f32>,
}

/// System latch outputs.
const LATCH_SHADOW: u8 = 1 << 0;
const LATCH_CART_VECTORS: u8 = 1 << 1;
const LATCH_CARD_LOCK1: u8 = 1 << 2;
const LATCH_CARD_UNLOCK2: u8 = 1 << 3;
const LATCH_PALETTE_BANK: u8 = 1 << 7;

impl NeoGeo {
    /// A console with `cartridge` in, running `bios` (the 128 KB system
    /// ROM, in the order the 68000 reads it), powered on. `zoom` is the L0
    /// ROM, or empty for the LSPC2's computed shrink table.
    #[must_use]
    pub fn new(cartridge: Cartridge, bios: Vec<u8>, zoom: Vec<u8>) -> Self {
        let mut machine = Self {
            cpu: Cpu68000::new(),
            z80: Z80::new(),
            lspc: Lspc2::new(),
            ym2610: Ym2610::new(),
            buttons: [Buttons::default(); 2],
            cartridge,
            bios,
            zoom,
            ram: vec![0; 0x1_0000],
            z80_ram: vec![0; 0x800],
            memory_card: None,
            mclk: 0,
            phase: 0,
            latch: 0,
            card_bank: 0,
            card_wait: 0,
            p_bank_base: 0x10_0000,
            sound_command: 0,
            sound_pending: false,
            sound_reply: 0,
            z80_nmi_enabled: false,
            z80_banks: Z80_RESET_BANKS,
            ym_phase: 0,
            ym_sum: [0.0; 2],
            ym_count: 0,
            audio: Vec::new(),
        };
        machine.reset();
        machine
    }

    /// Put the media back after a snapshot restore, and rebuild the Z80's
    /// step sequence.
    pub fn attach_media(&mut self, cartridge: Cartridge, bios: Vec<u8>, zoom: Vec<u8>) {
        self.cartridge = cartridge;
        self.bios = bios;
        self.zoom = zoom;
        self.z80.rehydrate_walker_sequence();
    }

    /// Insert a memory card's contents, or remove the card with `None`.
    pub fn set_memory_card(&mut self, card: Option<Vec<u8>>) {
        self.memory_card = card;
    }

    /// The memory card's contents, if one is in.
    #[must_use]
    pub fn memory_card(&self) -> Option<&[u8]> {
        self.memory_card.as_deref()
    }

    /// Press reset: the system latch clears, so the BIOS's vectors are
    /// back; both CPUs restart; the LSPC2 raises its reset interrupt.
    pub fn reset(&mut self) {
        self.latch = 0;
        self.apply_latch();
        self.lspc.reset();
        self.ym2610.reset();
        self.z80 = Z80::new();
        self.z80_nmi_enabled = false;
        self.sound_pending = false;
        self.card_wait = 0;
        let ssp = u32::from(self.vector_word(0)) << 16 | u32::from(self.vector_word(2));
        let pc = u32::from(self.vector_word(4)) << 16 | u32::from(self.vector_word(6));
        self.cpu.reset_to(ssp, pc);
    }

    /// One master clock.
    pub fn tick(&mut self) {
        let roms = VideoRoms {
            sprites: &self.cartridge.c,
            fix: &self.cartridge.s,
            zoom: &self.zoom,
        };
        self.lspc.tick(&roms);
        if self.phase.is_multiple_of(MCLKS_PER_68K) {
            self.tick_68000();
        }
        if self.phase.is_multiple_of(MCLKS_PER_YM) {
            let rom_b = if self.cartridge.v_b.is_empty() {
                &self.cartridge.v_a
            } else {
                &self.cartridge.v_b
            };
            self.ym2610.tick(&self.cartridge.v_a, rom_b);
        }
        if self.phase.is_multiple_of(MCLKS_PER_Z80_HALF) {
            self.tick_z80_half();
        }
        self.phase = (self.phase + 1) % 6;
        self.mclk += 1;
    }

    /// Run until the LSPC2 finishes a frame, returning the master clocks
    /// it took.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.mclk;
        loop {
            self.tick();
            if self.lspc.take_frame_complete() {
                return self.mclk - start;
            }
        }
    }

    /// Master clocks since power-on.
    #[must_use]
    pub fn mclk(&self) -> u64 {
        self.mclk
    }

    /// The picture: 320 by 224 `0xAARRGGBB` pixels.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.lspc.framebuffer
    }

    /// The sound made since the last call, as interleaved 48 kHz stereo.
    pub fn take_audio(&mut self) -> Vec<f32> {
        for sample in self.ym2610.take_samples() {
            let ssg = sample.ssg * SSG_GAIN;
            self.ym_sum[0] += f32::from(sample.fm[0]) * FM_GAIN + ssg;
            self.ym_sum[1] += f32::from(sample.fm[1]) * FM_GAIN + ssg;
            self.ym_count += 1;
            self.ym_phase += MCLKS_PER_YM_SAMPLE * u64::from(AUDIO_SAMPLE_RATE);
            if self.ym_phase >= MASTER_HZ {
                self.ym_phase -= MASTER_HZ;
                let count = self.ym_count as f32;
                self.audio.push(self.ym_sum[0] / count);
                self.audio.push(self.ym_sum[1] / count);
                self.ym_sum = [0.0; 2];
                self.ym_count = 0;
            }
        }
        std::mem::take(&mut self.audio)
    }

    // ---- The 68000. ----

    fn vector_word(&self, address: u32) -> u16 {
        if self.latch & LATCH_CART_VECTORS != 0 {
            self.cartridge.p_word(address)
        } else {
            word_at(&self.bios, address)
        }
    }

    fn apply_latch(&mut self) {
        self.lspc.shadow = self.latch & LATCH_SHADOW != 0;
        self.lspc.palette_bank = self.latch & LATCH_PALETTE_BANK != 0;
    }

    fn tick_68000(&mut self) {
        self.cpu.ipl = self.lspc.irq_level();
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
            // The LSPC2's interrupts are autovectored; software acknowledges
            // them through the LSPC2's register 6.
            let level = interrupt_acknowledge_level(addr);
            self.cpu.bus_status = BusStatus::Ready(24 + u16::from(level));
            return;
        }
        let address = addr & 0x00FF_FFFF;
        if (0x80_0000..0xC0_0000).contains(&address) {
            if self.card_wait == 0 {
                self.card_wait = CARD_WAIT + 1;
            }
            self.card_wait -= 1;
            if self.card_wait > 0 {
                self.cpu.bus_status = BusStatus::Wait;
                return;
            }
        }
        self.cpu.bus_status = if is_read {
            BusStatus::Ready(self.read_68000(address, is_word))
        } else {
            self.write_68000(address, is_word, data.unwrap_or(0));
            BusStatus::Ready(0)
        };
    }

    /// The word or byte the 68000 reads.
    fn read_68000(&mut self, address: u32, is_word: bool) -> u16 {
        let open_bus = self.cpu.irc;
        let word = match address {
            0x00_0000..=0x00_007F => self.vector_word(address),
            0x00_0080..=0x0F_FFFF => self.cartridge.p_word(address),
            0x10_0000..=0x1F_FFFF => word_at(&self.ram, address & 0xFFFF),
            0x20_0000..=0x2F_FFFF => self
                .cartridge
                .p_word(self.p_bank_base + (address & 0xF_FFFF)),
            0x30_0000..=0x31_FFFF => u16::from(self.buttons[0].port()) << 8 | 0xFF,
            0x32_0000..=0x33_FFFF => u16::from(self.sound_reply) << 8,
            0x34_0000..=0x35_FFFF => u16::from(self.buttons[1].port()) << 8 | 0xFF,
            0x38_0000..=0x39_FFFF => self.status_b(),
            0x3C_0000..=0x3D_FFFF => {
                if !is_word && address & 1 == 1 {
                    // The low byte alone is not decoded.
                    open_bus
                } else {
                    self.lspc.read_register(((address >> 1) & 7) as u8)
                }
            }
            0x40_0000..=0x7F_FFFF => self.lspc.read_palette(((address >> 1) & 0xFFF) as u16),
            0x80_0000..=0xBF_FFFF => {
                // The card answers on the upper data strobe only.
                if is_word || address & 1 == 0 {
                    self.card_read(address)
                } else {
                    0xFFFF
                }
            }
            0xC0_0000..=0xCF_FFFF => word_at(&self.bios, address & 0x1_FFFF),
            _ => open_bus,
        };
        if is_word {
            word
        } else if address & 1 == 0 {
            word >> 8
        } else {
            word & 0xFF
        }
    }

    /// The `$380000` word: start and select in bits 8-11, memory-card
    /// detect and write protect in bits 12-14 (set when there is no card),
    /// and bit 15 clear for an AES.
    fn status_b(&self) -> u16 {
        let card = if self.memory_card.is_some() {
            0
        } else {
            0x7000
        };
        0x00FF
            | u16::from(self.buttons[0].start_select()) << 8
            | u16::from(self.buttons[1].start_select()) << 10
            | card
    }

    fn card_offset(&self, address: u32, len: usize) -> usize {
        ((u32::from(self.card_bank) << 21 | (address & 0x3F_FFFF) >> 1) as usize) % len
    }

    fn card_read(&self, address: u32) -> u16 {
        match &self.memory_card {
            Some(card) if !card.is_empty() => {
                0xFF00 | u16::from(card[self.card_offset(address, card.len())])
            }
            _ => 0xFFFF,
        }
    }

    /// A 68000 write. A byte write puts the byte on both halves of the
    /// data bus.
    fn write_68000(&mut self, address: u32, is_word: bool, value: u16) {
        let value = if is_word {
            value
        } else {
            (value & 0xFF) << 8 | (value & 0xFF)
        };
        let upper = is_word || address & 1 == 0;
        let lower = is_word || address & 1 == 1;
        match address {
            0x10_0000..=0x1F_FFFF => {
                let a = (address & 0xFFFE) as usize;
                if upper {
                    self.ram[a] = (value >> 8) as u8;
                }
                if lower {
                    self.ram[a + 1] = value as u8;
                }
            }
            0x2F_FFF0..=0x2F_FFFF => self.select_p_bank(value as u8),
            0x32_0000..=0x33_FFFF if upper => {
                self.sound_command = (value >> 8) as u8;
                self.sound_pending = true;
            }
            0x38_0000..=0x39_FFFF if lower => {
                // Bits 3-5 of the byte offset pick the register; register 1
                // is the memory card's bank. Register 0 drives controller
                // outputs nothing standard reads.
                if (address >> 1) & 0x38 == 0x08 {
                    self.card_bank = value as u8 & 7;
                }
            }
            0x3A_0000..=0x3B_FFFF if lower => {
                // An addressable latch: A1-A3 pick the output, A4 is its
                // new level; the data is ignored.
                let bit = (address >> 1) & 7;
                if address & 0x10 != 0 {
                    self.latch |= 1 << bit;
                } else {
                    self.latch &= !(1 << bit);
                }
                self.apply_latch();
            }
            0x3C_0000..=0x3D_FFFF if upper => {
                self.lspc.write_register(((address >> 1) & 7) as u8, value);
            }
            0x40_0000..=0x7F_FFFF => {
                self.lspc
                    .write_palette(((address >> 1) & 0xFFF) as u16, value);
            }
            0x80_0000..=0xBF_FFFF if upper => {
                let writable =
                    self.latch & LATCH_CARD_LOCK1 == 0 && self.latch & LATCH_CARD_UNLOCK2 != 0;
                let bank = self.card_bank;
                if let Some(card) = self.memory_card.as_mut().filter(|c| !c.is_empty())
                    && writable
                {
                    let offset = ((u32::from(bank) << 21 | (address & 0x3F_FFFF) >> 1) as usize)
                        % card.len();
                    card[offset] = value as u8;
                }
            }
            _ => {}
        }
    }

    /// Point the `$200000` window at P-ROM bank `bank` (0-7), each bank a
    /// megabyte from the second megabyte on. A bank past the ROM's end
    /// falls back to bank 0.
    fn select_p_bank(&mut self, bank: u8) {
        let len = self.cartridge.p.len() as u32;
        if len <= 0x10_0000 {
            return;
        }
        let mut bank = u32::from(bank & 7);
        if (bank + 1) * 0x10_0000 >= len {
            bank = 0;
        }
        self.p_bank_base = (bank + 1) * 0x10_0000;
    }

    // ---- The Z80. ----

    fn tick_z80_half(&mut self) {
        self.z80.irq = self.ym2610.irq();
        self.z80.nmi = self.sound_pending && self.z80_nmi_enabled;
        self.z80.tick();
        match self.z80.bus_request() {
            Some(BusOp::MemRead) => self.z80.data_in = self.read_z80(self.z80.addr),
            Some(BusOp::MemWrite) => {
                if self.z80.addr >= 0xF800 {
                    self.z80_ram[usize::from(self.z80.addr & 0x7FF)] = self.z80.data;
                }
            }
            Some(BusOp::IoRead) => self.z80.data_in = self.read_z80_port(self.z80.addr),
            Some(BusOp::IoWrite) => self.write_z80_port(self.z80.addr, self.z80.data),
            // Mode 1 interrupts: the bus floats.
            Some(BusOp::IntAck) => self.z80.data_in = 0xFF,
            None => {}
        }
    }

    fn read_z80(&self, address: u16) -> u8 {
        let m = &self.cartridge.m;
        let window = match address {
            0x0000..=0x7FFF => return m.get(usize::from(address)).copied().unwrap_or(0xFF),
            0xF800..=0xFFFF => return self.z80_ram[usize::from(address & 0x7FF)],
            0xF000..=0xF7FF => 0,
            0xE000..=0xEFFF => 1,
            0xC000..=0xDFFF => 2,
            _ => 3,
        };
        if m.is_empty() {
            return 0xFF;
        }
        let shift = 11 + window;
        let size = 1u32 << shift;
        let mask = (m.len() as u32 - 1) & 0x3_FFFF;
        let base = (u32::from(self.z80_banks[window]) << shift) & mask;
        let offset = base + (u32::from(address) & (size - 1));
        m.get(offset as usize).copied().unwrap_or(0xFF)
    }

    fn read_z80_port(&mut self, port: u16) -> u8 {
        match port & 0xFF {
            0x00 => {
                self.sound_pending = false;
                self.sound_command
            }
            0x04..=0x07 => self.ym2610.read(port as u8 & 3),
            low if low & 0x0C == 0x08 => {
                // A bank select: the window in A0-A1, the bank on A8-A15.
                self.z80_banks[usize::from(low & 3)] = (port >> 8) as u8;
                0
            }
            _ => 0,
        }
    }

    fn write_z80_port(&mut self, port: u16, value: u8) {
        match port & 0xFF {
            0x00 => self.sound_command = 0,
            0x04..=0x07 => self.ym2610.write(port as u8 & 3, value),
            0x08 => self.z80_nmi_enabled = true,
            0x18 => self.z80_nmi_enabled = false,
            0x0C => self.sound_reply = value,
            _ => {}
        }
    }

    // ---- Inspection. ----

    /// Work RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// The Z80's RAM.
    #[must_use]
    pub fn z80_ram(&self) -> &[u8] {
        &self.z80_ram
    }

    /// The cartridge.
    #[must_use]
    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// The last command the 68000 sent the Z80, and whether the Z80 has
    /// still to read it.
    #[must_use]
    pub fn sound_command(&self) -> (u8, bool) {
        (self.sound_command, self.sound_pending)
    }

    /// The Z80's last reply.
    #[must_use]
    pub fn sound_reply(&self) -> u8 {
        self.sound_reply
    }

    /// The byte at `address` on the 68000's bus as a debugger sees it: ROM,
    /// RAM, palette and BIOS, read without side effects. The chips' ports
    /// read `$FF`.
    #[must_use]
    pub fn peek(&self, address: u32) -> u8 {
        let address = address & 0x00FF_FFFF;
        let word = match address {
            0x00_0000..=0x00_007F => self.vector_word(address),
            0x00_0080..=0x0F_FFFF => self.cartridge.p_word(address),
            0x10_0000..=0x1F_FFFF => word_at(&self.ram, address & 0xFFFF),
            0x20_0000..=0x2F_FFFF => self
                .cartridge
                .p_word(self.p_bank_base + (address & 0xF_FFFF)),
            0x40_0000..=0x7F_FFFF => self.lspc.read_palette(((address >> 1) & 0xFFF) as u16),
            0xC0_0000..=0xCF_FFFF => word_at(&self.bios, address & 0x1_FFFF),
            _ => 0xFFFF,
        };
        if address & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// Write `value` into work RAM at `address`; everything else is left
    /// alone.
    pub fn poke(&mut self, address: u32, value: u8) {
        let address = address & 0x00FF_FFFF;
        if (0x10_0000..=0x1F_FFFF).contains(&address) {
            self.ram[(address & 0xFFFF) as usize] = value;
        }
    }
}

/// The M-ROM banks at power-on. The NEO-ZMC on the cartridge starts with
/// every bank at 0, but early cartridges with 64 KB or less of Z80 code
/// have no banking chip and see the M-ROM straight through. These banks
/// give that straight-through view, which banked games replace before use;
/// MAME does the same.
const Z80_RESET_BANKS: [u8; 4] = [0x1E, 0x0E, 0x06, 0x02];

/// The big-endian word at `address` (rounded down to even) in `bytes`, or
/// `$FFFF` past the end.
fn word_at(bytes: &[u8], address: u32) -> u16 {
    let a = address as usize & !1;
    match bytes.get(a..a + 2) {
        Some(&[hi, lo]) => u16::from_be_bytes([hi, lo]),
        _ => 0xFFFF,
    }
}

#[cfg(test)]
mod tests;
