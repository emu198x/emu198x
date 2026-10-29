//! Game Boy Color: the CGB-only registers, VRAM and WRAM banking, the
//! double-speed switch, and HDMA.
//!
//! The register behaviour follows SameBoy (MIT, `Core/memory.c`,
//! `Core/sm83_cpu.c`), checked against Pan Docs ("CGB Registers",
//! "LCD Color Palettes") and the library's
//! `reference/by-system/nintendo-gameboy/gameboy-reference.md`.
//!
//! A CGB runs in one of two modes, fixed when the boot ROM writes KEY0:
//! CGB mode for a cartridge whose header asks for it, and DMG
//! compatibility mode for everything else. In compatibility mode most of
//! the CGB registers read `$FF` and ignore writes; the colour hardware
//! still runs, using palettes the boot ROM chose.

use serde::{Deserialize, Serialize};

use crate::GameBoyState;

/// Size of the CGB boot ROM: `$0000-$00FF` and `$0200-$08FF`, with the
/// cartridge header showing through at `$0100-$01FF`.
pub const CGB_BOOT_ROM_SIZE: usize = 0x900;

/// The boot ROM's palettes for a monochrome cartridge it does not
/// recognise: background palette 29 and object palette 4, lightest
/// first, as RGB555. SameBoy's `BootROMs/cgb_boot.asm` (MIT) uses the
/// same choice (combination 0, `Palettes` entries 4 and 29). The real
/// boot ROM picks other palettes for some Nintendo titles by a
/// checksum of the title; that table is not modelled.
pub const COMPATIBILITY_BG_PALETTE: [u16; 4] = [0x7FFF, 0x1BEF, 0x6180, 0x0000];
/// Object palette used for both OBP0 and OBP1 in compatibility mode.
pub const COMPATIBILITY_OBJ_PALETTE: [u16; 4] = [0x7FFF, 0x421F, 0x1CF2, 0x0000];

/// CPU cycles (4 per m-cycle) a speed switch holds the CPU when no
/// interrupt is pending (SameBoy `stop`, `speed_switch_halt_countdown`).
const SPEED_SWITCH_HALT_M_CYCLES: u32 = 0x2_0008 / 4;

/// The CGB-only machine state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CgbState {
    /// CGB mode (true) or DMG compatibility mode (false).
    pub cgb_mode: bool,
    /// The boot ROM, when one was supplied.
    #[serde(default)]
    pub boot_rom: Option<Vec<u8>>,
    /// Whether the boot ROM is still mapped (until `$FF50` is written).
    #[serde(default)]
    pub boot_rom_mapped: bool,

    /// VBK: the VRAM bank the CPU sees at `$8000-$9FFF`.
    pub vram_bank: u8,
    /// SVBK: the WRAM bank at `$D000-$DFFF` (0 selects bank 1).
    pub wram_bank: u8,

    /// KEY1 bit 0: a speed switch is armed for the next STOP.
    pub speed_switch_armed: bool,
    /// Whether the CPU runs at double speed.
    pub double_speed: bool,
    /// Remaining m-cycles of the HALT that follows a speed switch.
    pub speed_switch_halt: u32,
    /// STOP mode: nothing runs until a selected button is pressed.
    pub stop_mode: bool,

    /// HDMA source, with HDMA1/HDMA2's masks applied.
    pub hdma_source: u16,
    /// HDMA destination (masked to `$1FFF` on use).
    pub hdma_dest: u16,
    /// Blocks of 16 bytes still to copy.
    pub hdma_blocks: u8,
    /// A transfer is copying now, holding the CPU.
    pub hdma_running: bool,
    /// An HBlank transfer is armed: a block starts at each HBlank.
    pub hdma_hblank: bool,
    /// The m-cycle a transfer spends before its first byte.
    pub hdma_start_cycle: bool,

    /// RP (`$FF56`): bits 0, 6 and 7 written by the CPU.
    pub rp: u8,
    /// OPRI (`$FF6C`) bit 0.
    pub opri: u8,
    /// The undocumented registers at `$FF72-$FF75`.
    pub undocumented: [u8; 4],
}

impl CgbState {
    pub(crate) fn new(cgb_mode: bool, boot_rom: Option<Vec<u8>>) -> Self {
        let boot_rom_mapped = boot_rom.is_some();
        Self {
            cgb_mode,
            boot_rom,
            boot_rom_mapped,
            vram_bank: 0,
            wram_bank: 1,
            speed_switch_armed: false,
            double_speed: false,
            speed_switch_halt: 0,
            stop_mode: false,
            hdma_source: 0,
            hdma_dest: 0,
            hdma_blocks: 1,
            hdma_running: false,
            hdma_hblank: false,
            hdma_start_cycle: false,
            rp: 0,
            opri: u8::from(!cgb_mode),
            undocumented: [0; 4],
        }
    }

    /// Offset of the VRAM bank the CPU sees.
    pub(crate) const fn vram_offset(&self) -> usize {
        (self.vram_bank as usize & 1) * 0x2000
    }

    /// WRAM bank at `$D000-$DFFF`, 1-7.
    pub(crate) const fn high_wram_bank(&self) -> usize {
        match self.wram_bank & 7 {
            0 => 1,
            bank => bank as usize,
        }
    }

    /// The boot ROM byte at `addr`, while the boot ROM is mapped there.
    pub(crate) fn boot_rom_byte(&self, addr: u16) -> Option<u8> {
        if !self.boot_rom_mapped || (0x0100..0x0200).contains(&addr) {
            return None;
        }
        self.boot_rom.as_ref()?.get(usize::from(addr)).copied()
    }
}

impl GameBoyState {
    /// Reads a CGB register, or `None` for an address that is not one.
    pub(crate) fn read_cgb_io(&self, addr: u16) -> Option<u8> {
        let cgb = self.cgb.as_ref()?;
        let cgb_mode = cgb.cgb_mode;
        let value = match addr {
            0xFF4D if cgb_mode => {
                0x7E | (u8::from(cgb.double_speed) << 7) | u8::from(cgb.speed_switch_armed)
            }
            0xFF4F => 0xFE | cgb.vram_bank,
            0xFF50 => 0xFE | u8::from(!cgb.boot_rom_mapped),
            0xFF55 if cgb_mode => {
                let active = cgb.hdma_running || cgb.hdma_hblank;
                (if active { 0 } else { 0x80 }) | (cgb.hdma_blocks.wrapping_sub(1) & 0x7F)
            }
            // RP: bit 1 reads 1 (no light received); the IR LED and
            // receiver are not modelled.
            0xFF56 if cgb_mode => (cgb.rp & 0xC1) | 0x3E,
            0xFF68 => self.ppu.read_palette_index(false),
            0xFF6A => self.ppu.read_palette_index(true),
            0xFF69 | 0xFF6B if !cgb_mode && !cgb.boot_rom_mapped => 0xFF,
            0xFF69 => self.ppu.read_palette_data(false),
            0xFF6B => self.ppu.read_palette_data(true),
            0xFF6C => 0xFE | cgb.opri,
            0xFF70 if cgb_mode => 0xF8 | cgb.wram_bank,
            0xFF72 | 0xFF73 => cgb.undocumented[usize::from(addr - 0xFF72)],
            0xFF74 if cgb_mode => cgb.undocumented[2],
            0xFF75 => cgb.undocumented[3] | 0x8F,
            // PCM12/PCM34 report the channels' output levels; not
            // modelled, so they read as silence.
            0xFF76 | 0xFF77 => 0x00,
            _ => return None,
        };
        Some(value)
    }

    /// Writes a CGB register. Returns false for an address that is not
    /// one, so the DMG register map handles it.
    pub(crate) fn write_cgb_io(&mut self, addr: u16, value: u8) -> bool {
        let Some(cgb) = self.cgb.as_mut() else {
            return false;
        };
        let cgb_mode = cgb.cgb_mode;
        let boot = cgb.boot_rom_mapped;
        match addr {
            // KEY0: the boot ROM picks the mode; bits 2-3 select DMG
            // compatibility.
            0xFF4C if boot => {
                cgb.cgb_mode = value & 0x0C == 0;
                let mode = cgb.cgb_mode;
                self.ppu.set_cgb_mode(mode);
            }
            0xFF4D if cgb_mode => cgb.speed_switch_armed = value & 1 != 0,
            0xFF4F if cgb_mode => cgb.vram_bank = value & 1,
            0xFF50 if boot && value & 1 != 0 => cgb.boot_rom_mapped = false,
            0xFF51 if cgb_mode => {
                cgb.hdma_source = (cgb.hdma_source & 0x00F0) | (u16::from(value) << 8);
                // $E000-$FFFF reads like $F000-, which is never a source.
                if cgb.hdma_source >= 0xE000 {
                    cgb.hdma_source |= 0xF000;
                }
            }
            0xFF52 if cgb_mode => {
                cgb.hdma_source = (cgb.hdma_source & 0xFF00) | u16::from(value & 0xF0);
            }
            0xFF53 if cgb_mode => {
                cgb.hdma_dest = (cgb.hdma_dest & 0x00F0) | (u16::from(value) << 8);
            }
            0xFF54 if cgb_mode => {
                cgb.hdma_dest = (cgb.hdma_dest & 0xFF00) | u16::from(value & 0xF0);
            }
            0xFF55 if cgb_mode => {
                cgb.hdma_blocks = (value & 0x7F) + 1;
                if value & 0x80 == 0 && cgb.hdma_hblank {
                    // Bit 7 clear stops an HBlank transfer.
                    cgb.hdma_hblank = false;
                } else if value & 0x80 == 0 {
                    // General-purpose DMA: everything now.
                    cgb.hdma_running = true;
                    cgb.hdma_start_cycle = true;
                } else {
                    cgb.hdma_hblank = true;
                    // Armed during HBlank, the first block runs at once.
                    if self.ppu.mode() == 0 {
                        cgb.hdma_running = true;
                        cgb.hdma_start_cycle = true;
                    }
                }
            }
            0xFF56 if cgb_mode => cgb.rp = value & 0xC1,
            0xFF68 => self.ppu.write_palette_index(false, value),
            0xFF6A => self.ppu.write_palette_index(true, value),
            0xFF69 | 0xFF6B if !cgb_mode && !boot => {}
            0xFF69 => self.ppu.write_palette_data(false, value),
            0xFF6B => self.ppu.write_palette_data(true, value),
            0xFF6C => {
                cgb.opri = value & 1;
                // Only the boot ROM can change the object ordering.
                if boot {
                    self.ppu.set_object_priority_by_index(value & 1 == 0);
                }
            }
            0xFF70 if cgb_mode || boot => cgb.wram_bank = value & 7,
            0xFF72 | 0xFF73 => cgb.undocumented[usize::from(addr - 0xFF72)] = value,
            0xFF74 if cgb_mode => cgb.undocumented[2] = value,
            0xFF75 => cgb.undocumented[3] = value & 0x70,
            0xFF4C..=0xFF4F | 0xFF51..=0xFF56 | 0xFF70..=0xFF77 => {}
            0xFF50 => {}
            _ => return false,
        }
        true
    }

    /// Acts on a STOP the CPU has just executed (SameBoy `stop`).
    ///
    /// - A selected button already held: STOP exits at once.
    /// - Otherwise DIV resets and the machine enters STOP mode, unless
    ///   a CGB speed switch is armed: then the speed toggles and STOP
    ///   exits at once.
    /// - Exiting at once with no interrupt pending leaves the CPU in
    ///   HALT; after a speed switch that HALT ends by itself after
    ///   `$20008` CPU cycles if no interrupt ends it first.
    pub(crate) fn handle_stop(&mut self) {
        self.cpu.stopped = false;
        let exit_by_joypad = self.joypad.any_selected_pressed();
        let interrupt_pending = self.if_reg & self.ie_reg & 0x1F != 0;
        let speed_switch = !exit_by_joypad
            && self
                .cgb
                .as_ref()
                .is_some_and(|cgb| cgb.cgb_mode && cgb.speed_switch_armed);

        if !exit_by_joypad {
            self.timer.write_div();
        }
        if speed_switch && let Some(cgb) = self.cgb.as_mut() {
            cgb.double_speed = !cgb.double_speed;
            cgb.speed_switch_armed = false;
            cgb.speed_switch_halt = if interrupt_pending {
                0
            } else {
                SPEED_SWITCH_HALT_M_CYCLES
            };
        }
        if speed_switch || exit_by_joypad {
            if !interrupt_pending {
                self.cpu.halt_mode = true;
            }
        } else if let Some(cgb) = self.cgb.as_mut() {
            cgb.stop_mode = true;
        } else {
            self.dmg_stop_mode = true;
        }
    }

    /// Whether the machine is in STOP mode.
    pub(crate) fn in_stop_mode(&self) -> bool {
        self.dmg_stop_mode || self.cgb.as_ref().is_some_and(|cgb| cgb.stop_mode)
    }

    /// One m-cycle in STOP mode: a selected button press ends it.
    ///
    /// The timer, OAM DMA and CPU stand still. A DMG's PPU stops too; a
    /// CGB's keeps drawing (SameBoy `timing.c`, `display.c`).
    pub(crate) fn tick_stop_mode(&mut self) {
        if self.cgb.is_some() {
            let dots = if self.double_speed() { 2 } else { 4 };
            for _ in 0..dots {
                self.ppu.tick(&self.vram, &self.oam);
            }
            if self.ppu.consume_vblank_irq() {
                self.if_reg |= super::IF_VBLANK;
            }
            if self.ppu.consume_stat_irq() {
                self.if_reg |= super::IF_STAT;
            }
        }
        if self.joypad.any_selected_pressed() {
            self.dmg_stop_mode = false;
            if let Some(cgb) = self.cgb.as_mut() {
                cgb.stop_mode = false;
            }
        }
    }

    /// Counts down the HALT that follows a speed switch.
    pub(crate) fn tick_speed_switch_halt(&mut self) {
        let Some(cgb) = self.cgb.as_mut() else {
            return;
        };
        if cgb.speed_switch_halt == 0 {
            return;
        }
        if !self.cpu.halt_mode {
            // An interrupt ended the HALT first.
            cgb.speed_switch_halt = 0;
            return;
        }
        cgb.speed_switch_halt -= 1;
        if cgb.speed_switch_halt == 0 {
            self.cpu.halt_mode = false;
        }
    }

    /// Starts an armed HBlank transfer's next block as mode 0 begins,
    /// unless the CPU is halted (SameBoy `GB_display_run`).
    pub(crate) fn hblank_started(&mut self) {
        let halted = self.cpu.halt_mode;
        if let Some(cgb) = self.cgb.as_mut()
            && cgb.hdma_hblank
            && !cgb.hdma_running
            && !halted
        {
            cgb.hdma_running = true;
            cgb.hdma_start_cycle = true;
        }
    }

    /// Runs one m-cycle of HDMA. Returns true when the transfer used
    /// the m-cycle, so the CPU stalls.
    ///
    /// A transfer spends one m-cycle before its first byte, then copies
    /// two bytes per m-cycle at single speed or one at double speed
    /// (SameBoy `GB_hdma_run`: 2 or 4 CPU cycles per byte). Only ROM,
    /// cartridge RAM and WRAM can be read; other sources give `$FF`.
    pub(crate) fn tick_hdma(&mut self) -> bool {
        let Some(cgb) = self.cgb.as_mut() else {
            return false;
        };
        if !cgb.hdma_running {
            return false;
        }
        if cgb.hdma_start_cycle {
            cgb.hdma_start_cycle = false;
            return true;
        }
        let bytes = if cgb.double_speed { 1 } else { 2 };
        for _ in 0..bytes {
            let Some(cgb) = self.cgb.as_ref() else {
                return false;
            };
            let source = cgb.hdma_source;
            let dest = cgb.hdma_dest;
            let bank = cgb.vram_offset();
            let readable = source < 0x8000 || matches!(source & 0xE000, 0xA000 | 0xC000);
            let byte = if readable {
                self.bus_read(source)
            } else {
                0xFF
            };
            self.vram[bank + usize::from(dest & 0x1FFF)] = byte;

            let Some(cgb) = self.cgb.as_mut() else {
                return false;
            };
            cgb.hdma_source = source.wrapping_add(1);
            cgb.hdma_dest = dest.wrapping_add(1);
            if cgb.hdma_dest & 0xF == 0 {
                cgb.hdma_blocks = cgb.hdma_blocks.wrapping_sub(1);
                if cgb.hdma_blocks == 0 || cgb.hdma_dest == 0 {
                    cgb.hdma_running = false;
                    cgb.hdma_hblank = false;
                } else if cgb.hdma_hblank {
                    cgb.hdma_running = false;
                }
                if !cgb.hdma_running {
                    break;
                }
            }
        }
        true
    }
}
