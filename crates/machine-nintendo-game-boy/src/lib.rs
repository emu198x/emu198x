//! Nintendo Game Boy (DMG) machine.
//!
//! Composes the SM83 CPU, PPU, APU, timer, and cartridge into a
//! single tickable machine. The bus dispatch covers the full DMG
//! memory map; per-m-cycle orchestration drives the timer / APU /
//! PPU at T-cycle rate, the CPU at m-cycle rate, and OR's all IRQ
//! sources into `IF` for the CPU's interrupt-dispatch path.
//!
//! Deferred behaviour (will land alongside the relevant test ROMs):
//!
//! - Boot-ROM slot. The CPU resets at PC=$0100 with the documented
//!   post-boot register state instead of running through the
//!   256-byte boot ROM.
//! - OAM DMA bus blocking. Real hardware blocks all CPU access to
//!   non-HRAM memory for the 160 m-cycles a DMA takes; the DMA transfer
//!   itself is paced, but CPU bus gating is still deferred.
//! - Per-PPU-mode VRAM/OAM access blocking. The CPU sees `$FF` for
//!   reads of VRAM during mode 3 and OAM during modes 2/3 on real
//!   hardware; we always route the read.

#[cfg(test)]
mod tests;

use common_nintendo_game_boy::{JoypadButton, JoypadMatrix, MemoryBus};
use format_nintendo_game_boy_cartridge::{CartridgeHeader, HeaderError, load};
use nintendo_game_boy_apu::Apu;
use nintendo_game_boy_mbc::Cartridge;
use nintendo_game_boy_ppu::Ppu;
use nintendo_game_boy_timer::Timer;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use sharp_lr35902::Sm83;

const WRAM_SIZE: usize = 0x2000;
const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const HRAM_SIZE: usize = 0x7F;

const IF_VBLANK: u8 = 0x01;
const IF_STAT: u8 = 0x02;
const IF_TIMER: u8 = 0x04;
const IF_SERIAL: u8 = 0x08;
const IF_JOYPAD: u8 = 0x10;

/// A loaded DMG.
#[derive(Clone, Serialize, Deserialize)]
pub struct GameBoy {
    cpu: Sm83,
    timer: Timer,
    ppu: Ppu,
    apu: Apu,
    cartridge: Cartridge,

    #[serde(with = "BigArray")]
    wram: [u8; WRAM_SIZE],
    #[serde(with = "BigArray")]
    vram: [u8; VRAM_SIZE],
    #[serde(with = "BigArray")]
    oam: [u8; OAM_SIZE],
    #[serde(with = "BigArray")]
    hram: [u8; HRAM_SIZE],

    if_reg: u8,
    ie_reg: u8,

    joypad: JoypadMatrix,
    joypad_line_prev: bool,

    serial_data: u8,
    serial_control: u8,
    #[serde(default)]
    serial_irq_active: bool,
    #[serde(default)]
    serial_irq_bits_remaining: u8,
    /// Bytes "transmitted" via `SC = $81`. Drained by tests / the
    /// runtime layer to surface Blargg-style reporting.
    #[serde(skip)]
    serial_output: Vec<u8>,

    #[serde(default = "default_oam_dma_reg")]
    oam_dma_reg: u8,
    #[serde(default = "default_oam_dma_reg")]
    oam_dma_source_high: u8,
    #[serde(default = "default_oam_dma_index")]
    oam_dma_index: u8,
    #[serde(default)]
    oam_dma_start_delay: u8,
    #[serde(default = "default_oam_dma_reg")]
    oam_dma_pending_source_high: u8,
    #[serde(default)]
    oam_dma_pending_start_delay: u8,
}

const fn default_oam_dma_reg() -> u8 {
    0xFF
}

const fn default_oam_dma_index() -> u8 {
    OAM_SIZE as u8
}

impl GameBoy {
    /// Build a Game Boy around the given parsed cartridge.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();

        Self {
            cpu,
            timer: Timer::new_post_bootrom_dmg(),
            ppu: Ppu::new(),
            apu: Apu::new_post_bootrom_dmg(),
            cartridge,
            wram: [0; WRAM_SIZE],
            vram: [0; VRAM_SIZE],
            oam: [0; OAM_SIZE],
            hram: [0; HRAM_SIZE],
            if_reg: IF_VBLANK,
            ie_reg: 0,
            joypad: JoypadMatrix::new_post_bootrom_dmg(),
            joypad_line_prev: false,
            serial_data: 0,
            serial_control: 0,
            serial_irq_active: false,
            serial_irq_bits_remaining: 0,
            serial_output: Vec::new(),
            oam_dma_reg: 0xFF,
            oam_dma_source_high: 0xFF,
            oam_dma_index: OAM_SIZE as u8,
            oam_dma_start_delay: 0,
            oam_dma_pending_source_high: 0xFF,
            oam_dma_pending_start_delay: 0,
        }
    }

    /// Convenience: parse a ROM image and build the Game Boy in one
    /// step. Returns the decoded header alongside the machine for
    /// the runtime layer's metadata.
    ///
    /// # Errors
    ///
    /// Forwards any [`HeaderError`] from the cartridge format crate.
    pub fn from_rom(rom: Vec<u8>) -> Result<(CartridgeHeader, Self), HeaderError> {
        let (header, cart) = load(rom)?;
        Ok((header, Self::new(cart)))
    }

    /// Returns the framebuffer (160 × 144 post-palette 2-bit shades).
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        self.ppu.framebuffer()
    }

    /// Current CPU program counter.
    #[must_use]
    pub const fn cpu_pc(&self) -> u16 {
        self.cpu.pc
    }

    /// Drain APU samples (stereo interleaved `f32`).
    pub fn drain_audio(&mut self, dest: &mut [f32]) -> usize {
        self.apu.drain_samples(dest)
    }

    /// Drain bytes "transmitted" via the serial port (`SC = $81`).
    /// Blargg's CPU test ROMs use this to report progress.
    pub fn drain_serial(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.serial_output)
    }

    /// Set the pressed state of one joypad button.
    pub fn set_button(&mut self, button: JoypadButton, pressed: bool) {
        self.joypad.set(button, pressed);
    }

    /// Returns the parsed cartridge metadata via the underlying
    /// cartridge.
    #[must_use]
    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// One CPU m-cycle (4 T-cycles): tick the per-T-cycle components,
    /// collect IRQ sources, service the CPU's bus pins, then tick
    /// the CPU.
    pub fn step_m_cycle(&mut self) {
        for _ in 0..4 {
            let serial_clock_before = self.serial_clock_bit();
            self.timer.tick_t();
            let serial_clock_after = self.serial_clock_bit();
            self.tick_serial_irq(serial_clock_before, serial_clock_after);
            self.apu.tick(self.timer.counter);
            self.ppu.tick(&self.vram, &self.oam);
        }

        if self.timer.consume_overflow() {
            self.if_reg |= IF_TIMER;
        }
        if self.ppu.consume_vblank_irq() {
            self.if_reg |= IF_VBLANK;
        }
        if self.ppu.consume_stat_irq() {
            self.if_reg |= IF_STAT;
        }

        // Joypad IRQ: rising edge of "any selected button pressed"
        // (the input nibble's high-to-low transition for any bit).
        let joypad_line = self.joypad.any_selected_pressed();
        if joypad_line && !self.joypad_line_prev {
            self.if_reg |= IF_JOYPAD;
        }
        self.joypad_line_prev = joypad_line;

        self.service_cpu();
        self.tick_oam_dma();
    }

    /// Run m-cycles until the PPU latches frame-ready (≈ 17 556
    /// m-cycles per frame). Returns the number of m-cycles the
    /// frame took.
    pub fn run_frame(&mut self) -> u32 {
        const SAFETY_LIMIT: u32 = 30_000; // ~1.7× a normal frame
        let mut count = 0;
        loop {
            self.step_m_cycle();
            count += 1;
            if self.ppu.consume_frame_ready() {
                return count;
            }
            if count >= SAFETY_LIMIT {
                return count;
            }
        }
    }

    fn service_cpu(&mut self) {
        if self.cpu.mreq {
            if self.cpu.rd {
                let value = self.read(self.cpu.addr);
                self.cpu.data_in = value;
            } else if self.cpu.wr {
                let addr = self.cpu.addr;
                let data = self.cpu.data;
                self.write(addr, data);
            }
        }

        if self.cpu.int_ack {
            self.if_reg &= !(1u8 << self.cpu.int_ack_bit);
        }

        self.cpu.irq_pending = self.if_reg & self.ie_reg & 0x1F;
        self.cpu.tick();
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joypad.read_p1(),
            0xFF01 => self.serial_data,
            0xFF02 => self.serial_control | 0x7E, // bits 1-6 wired high
            0xFF04 => self.timer.read_div(),
            0xFF05 => self.timer.read_tima(),
            0xFF06 => self.timer.read_tma(),
            0xFF07 => self.timer.read_tac() | 0xF8, // upper bits wired high
            0xFF0F => self.if_reg | 0xE0,           // upper 3 bits wired high
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF40 => self.ppu.lcdc,
            0xFF41 => self.ppu.read_stat() | 0x80, // bit 7 wired high
            0xFF42 => self.ppu.scy,
            0xFF43 => self.ppu.scx,
            0xFF44 => self.ppu.read_ly(),
            0xFF45 => self.ppu.lyc,
            0xFF46 => self.oam_dma_reg,
            0xFF47 => self.ppu.bgp,
            0xFF48 => self.ppu.obp0,
            0xFF49 => self.ppu.obp1,
            0xFF4A => self.ppu.wy,
            0xFF4B => self.ppu.wx,
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF00 => self.joypad.write_p1(value),
            0xFF01 => self.serial_data = value,
            0xFF02 => {
                self.serial_control = value;
                if (value & 0x81) == 0x81 {
                    // Keep Blargg/mooneye's emulator-reporting channel
                    // immediate, but schedule IF.SERIAL on the real
                    // DIV-derived serial clock.
                    self.serial_output.push(self.serial_data);
                    self.serial_control &= 0x7F;
                    self.serial_irq_active = true;
                    self.serial_irq_bits_remaining = 8;
                } else if (value & 0x80) == 0 {
                    self.serial_irq_active = false;
                    self.serial_irq_bits_remaining = 0;
                }
            }
            0xFF04 => self.timer.write_div(),
            0xFF05 => self.timer.write_tima(value),
            0xFF06 => self.timer.write_tma(value),
            0xFF07 => self.timer.write_tac(value),
            0xFF0F => self.if_reg = value & 0x1F,
            0xFF10..=0xFF3F => self.apu.write(addr, value),
            0xFF40 => self.ppu.write_lcdc(value),
            0xFF41 => self.ppu.write_stat(value),
            0xFF42 => self.ppu.scy = value,
            0xFF43 => self.ppu.scx = value,
            0xFF44 => {} // LY is read-only; writes ignored
            0xFF45 => self.ppu.write_lyc(value),
            0xFF46 => self.start_oam_dma(value),
            0xFF47 => self.ppu.bgp = value,
            0xFF48 => self.ppu.obp0 = value,
            0xFF49 => self.ppu.obp1 = value,
            0xFF4A => self.ppu.wy = value,
            0xFF4B => self.ppu.wx = value,
            _ => {}
        }
    }

    fn start_oam_dma(&mut self, page: u8) {
        self.oam_dma_reg = page;
        if self.oam_dma_active() {
            self.oam_dma_pending_source_high = page;
            self.oam_dma_pending_start_delay = 2;
            return;
        }

        self.oam_dma_source_high = page;
        self.oam_dma_index = 0;
        self.oam_dma_start_delay = 2;
    }

    fn tick_oam_dma(&mut self) {
        let previous_dma_active = self.oam_dma_active();

        if self.oam_dma_index < OAM_SIZE as u8 {
            if self.oam_dma_start_delay != 0 {
                self.oam_dma_start_delay -= 1;
            } else {
                let offset = u16::from(self.oam_dma_index);
                let source = (u16::from(self.oam_dma_source_high) << 8) | offset;
                let value = self.oam_dma_source_read(source);
                self.oam[usize::from(self.oam_dma_index)] = value;
                self.oam_dma_index = self.oam_dma_index.saturating_add(1);
            }
        }

        if self.oam_dma_pending_start_delay != 0 {
            self.oam_dma_pending_start_delay -= 1;
            if self.oam_dma_pending_start_delay == 0 {
                self.oam_dma_source_high = self.oam_dma_pending_source_high;
                self.oam_dma_index = 0;
                self.oam_dma_start_delay = 0;
            }
        } else if previous_dma_active && self.oam_dma_index >= OAM_SIZE as u8 {
            self.oam_dma_source_high = self.oam_dma_reg;
        }
    }

    fn oam_dma_active(&self) -> bool {
        self.oam_dma_start_delay == 0 && self.oam_dma_index < OAM_SIZE as u8
    }

    fn oam_dma_source_read(&self, addr: u16) -> u8 {
        let source = if addr >= 0xE000 {
            addr.wrapping_sub(0x2000)
        } else {
            addr
        };
        self.bus_read(source)
    }

    fn serial_clock_bit(&self) -> bool {
        ((self.timer.counter >> 8) & 1) != 0
    }

    fn tick_serial_irq(&mut self, clock_before: bool, clock_after: bool) {
        if !self.serial_irq_active || !clock_before || clock_after {
            return;
        }

        self.serial_irq_bits_remaining = self.serial_irq_bits_remaining.saturating_sub(1);
        if self.serial_irq_bits_remaining == 0 {
            self.serial_irq_active = false;
            self.if_reg |= IF_SERIAL;
        }
    }
}

impl MemoryBus for GameBoy {
    fn read(&mut self, addr: u16) -> u8 {
        // The trait declares a mutable receiver because IO reads can
        // have side effects on real hardware (e.g. wave-RAM access
        // during CH3 playback). We delegate to a private read that
        // handles the cases that are pure plus the few that aren't.
        self.bus_read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.bus_write(addr, value);
    }
}

impl GameBoy {
    fn bus_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cartridge.read_rom(addr),
            0x8000..=0x9FFF => self.vram[usize::from(addr - 0x8000)],
            0xA000..=0xBFFF => self.cartridge.read_ram(addr),
            0xC000..=0xDFFF => self.wram[usize::from(addr - 0xC000)],
            // Echo RAM mirrors $C000..=$DDFF.
            0xE000..=0xFDFF => self.wram[usize::from(addr - 0xE000)],
            0xFE00..=0xFE9F if self.oam_dma_active() => 0xFF,
            0xFE00..=0xFE9F => self.oam[usize::from(addr - 0xFE00)],
            0xFEA0..=0xFEFF => 0xFF, // unusable region
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)],
            0xFFFF => self.ie_reg,
        }
    }

    fn bus_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cartridge.write_rom(addr, value),
            0x8000..=0x9FFF => self.vram[usize::from(addr - 0x8000)] = value,
            0xA000..=0xBFFF => self.cartridge.write_ram(addr, value),
            0xC000..=0xDFFF => self.wram[usize::from(addr - 0xC000)] = value,
            0xE000..=0xFDFF => self.wram[usize::from(addr - 0xE000)] = value,
            0xFE00..=0xFE9F if self.oam_dma_active() => {}
            0xFE00..=0xFE9F => self.oam[usize::from(addr - 0xFE00)] = value,
            0xFEA0..=0xFEFF => {} // unusable region
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)] = value,
            0xFFFF => self.ie_reg = value,
        }
    }
}
