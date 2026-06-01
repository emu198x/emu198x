//! Memotech MTX500 / MTX512 (1983) — Z80A + TMS9918A + SN76489.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-memotech-mtx/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`zilog_z80::Z80`]'s public pin fields and `bus_request()`
//! collapse.
//!
//! # The Memotech MTX
//:
//! UK-built Z80A home computer from Memotech (1983) — aluminium case,
//! pro-grade keyboard, MTX BASIC + Noddy + SuperPascal ROMs.
//! Critically respected; commercially overshadowed by the cheaper
//! Spectrum.
//!
//! - **CPU:** Zilog Z80A at 4 MHz
//! - **VDP:** TI TMS9918A (PAL), interrupt drives Z80 INT
//! - **PSG:** TI SN76489 at 4 MHz (internal ÷16 to 250 kHz)
//! - **RAM:** 32 KB (MTX500) or 64 KB (MTX512)
//! - **ROM:** 16 KB total — 8 KB OS at page 0, 8 KB BASIC at page 1
//!   (both switchable to RAM via port `$00`)
//!
//! # I/O ports
//!
//! | Port  | Direction | Function                             |
//! |-------|-----------|--------------------------------------|
//! | `$00` | write     | Page register (bit 0 = page 0 RAM,   |
//! |       |           | bit 1 = page 1 RAM)                  |
//! | `$01` | read/write| VDP data                             |
//! | `$02` | read/write| VDP status (R) / register (W)        |
//! | `$03` | write     | PSG (SN76489)                        |
//! | `$05` | read/write| Keyboard row select (W) / data (R)   |

mod keyboard;
pub mod input;

pub use input::MtxKey;
pub use keyboard::KeyboardState;
pub use ti_tms9918::Tms9918;

use ti_sn76489::Sn76489;
use ti_tms9918::VdpRegion;
use zilog_z80::z80::{BusOp, Z80};

const VDP_CLOCK_HZ: u64 = 5_369_318;
const CPU_CLOCK_HZ: u64 = 4_000_000;

/// MTX model selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtxModel {
    /// 32 KB RAM.
    Mtx500,
    /// 64 KB RAM.
    Mtx512,
}

impl MtxModel {
    fn ram_size(self) -> usize {
        match self {
            Self::Mtx500 => 32768,
            Self::Mtx512 => 65536,
        }
    }
}

/// Memotech MTX machine.
pub struct Mtx {
    cpu: Z80,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_size: usize,
    page_reg: u8,
    vdp: Tms9918,
    psg: Sn76489,
    keyboard: KeyboardState,
    keyboard_row: u8,
    vdp_accum: i64,
    master_clock: u64,
    frame_count: u64,
}

impl Mtx {
    /// Create a new MTX. `rom` must be 16 KB (8 KB OS + 8 KB BASIC).
    pub fn new(rom: Vec<u8>, model: MtxModel) -> Result<Self, String> {
        if rom.len() != 0x4000 {
            return Err(format!(
                "MTX ROM must be 16384 bytes, got {}",
                rom.len()
            ));
        }
        let ram_size = model.ram_size();
        Ok(Self {
            cpu: Z80::new(),
            rom,
            ram: vec![0; ram_size],
            ram_size,
            page_reg: 0,
            vdp: Tms9918::new(VdpRegion::Pal),
            psg: Sn76489::new(4_000_000),
            keyboard: KeyboardState::new(),
            keyboard_row: 0,
            vdp_accum: 0,
            master_clock: 0,
            frame_count: 0,
        })
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target_frame = self.frame_count + 1;
        loop {
            self.tick_cpu();
            if self.vdp.frame_count >= target_frame {
                break;
            }
        }
        self.frame_count = target_frame;
        self.master_clock - start
    }

    fn tick_cpu(&mut self) {
        self.master_clock += 1;
        self.psg.tick();

        self.vdp_accum += VDP_CLOCK_HZ as i64;
        while self.vdp_accum >= CPU_CLOCK_HZ as i64 {
            self.vdp_accum -= CPU_CLOCK_HZ as i64;
            self.vdp.tick();
        }

        self.cpu.irq = self.vdp.interrupt;
        self.cpu.tick();
        self.handle_bus();
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                if self.page_reg & 0x01 != 0 {
                    self.ram[addr as usize]
                } else {
                    self.rom[addr as usize]
                }
            }
            0x2000..=0x3FFF => {
                if self.page_reg & 0x02 != 0 {
                    self.ram[addr as usize]
                } else {
                    self.rom[addr as usize]
                }
            }
            0x4000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => {
                if self.ram_size >= 65536 {
                    self.ram[addr as usize]
                } else {
                    0xFF
                }
            }
            0xC000..=0xFFFF => {
                if self.ram_size >= 65536 {
                    self.ram[addr as usize]
                } else {
                    self.ram[(addr as usize) & 0x7FFF]
                }
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.page_reg & 0x01 != 0 {
                    self.ram[addr as usize] = value;
                }
            }
            0x2000..=0x3FFF => {
                if self.page_reg & 0x02 != 0 {
                    self.ram[addr as usize] = value;
                }
            }
            0x4000..=0x7FFF => self.ram[addr as usize] = value,
            0x8000..=0xBFFF => {
                if self.ram_size >= 65536 {
                    self.ram[addr as usize] = value;
                }
            }
            0xC000..=0xFFFF => {
                if self.ram_size >= 65536 {
                    self.ram[addr as usize] = value;
                } else {
                    self.ram[(addr as usize) & 0x7FFF] = value;
                }
            }
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match (port & 0xFF) as u8 {
            0x01 => self.vdp.read_data(),
            0x02 => self.vdp.read_status(),
            0x05 => self.keyboard.read(self.keyboard_row as usize),
            0x06 | 0x08 => 0xFF,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match (port & 0xFF) as u8 {
            0x00 => self.page_reg = value,
            0x01 => self.vdp.write_data(value),
            0x02 => self.vdp.write_control(value),
            0x03 => self.psg.write(value),
            0x05 => self.keyboard_row = value & 0x07,
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    pub fn press_key(&mut self, key: MtxKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, true);
    }

    pub fn release_key(&mut self, key: MtxKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    #[must_use]
    pub fn vdp(&self) -> &Tms9918 {
        &self.vdp
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

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 0x4000];
        rom[0] = 0xF3;
        rom[1] = 0x76;
        rom
    }

    #[test]
    fn rom_size_validated() {
        assert!(Mtx::new(vec![0u8; 1024], MtxModel::Mtx500).is_err());
    }

    #[test]
    fn frame_advances_count() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn page_reg_drives_paging() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        assert_eq!(sys.mem_read(0x0000), 0xF3);
        sys.page_reg = 0x01;
        sys.mem_write(0x0000, 0x99);
        assert_eq!(sys.mem_read(0x0000), 0x99);
        sys.page_reg = 0;
        assert_eq!(sys.mem_read(0x0000), 0xF3);
    }

    #[test]
    fn mtx500_mirrors_high_ram() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        sys.mem_write(0xC000, 0x77);
        assert_eq!(sys.mem_read(0x4000), 0x77);
    }

    #[test]
    fn mtx512_has_full_ram() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx512).expect("init");
        sys.mem_write(0x8000, 0x88);
        sys.mem_write(0xC000, 0x99);
        assert_eq!(sys.mem_read(0x8000), 0x88);
        assert_eq!(sys.mem_read(0xC000), 0x99);
    }

    #[test]
    fn keyboard_via_io() {
        let mut sys = Mtx::new(trap_rom(), MtxModel::Mtx500).expect("init");
        sys.io_write(0x05, 3);
        assert_eq!(sys.io_read(0x05), 0xFF);
        sys.keyboard.set_key(3, 1, true);
        assert_eq!(sys.io_read(0x05) & 0x02, 0x00);
    }
}
