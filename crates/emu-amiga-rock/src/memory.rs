//! Memory management for the Amiga Rock.

use std::sync::atomic::{AtomicU32, Ordering};

/// Watchpoint trigger counter for BPL1 corruption detection.
static BPL1_WATCHPOINT_FIRED: AtomicU32 = AtomicU32::new(0);
/// Track ALL writes to a specific BPL1 address for debugging.
static BPL1_ADDR_TRACE_COUNT: AtomicU32 = AtomicU32::new(0);

pub const CHIP_RAM_BASE: u32 = 0x000000;
pub const CIA_A_BASE: u32 = 0xBFE001;
pub const CIA_B_BASE: u32 = 0xBFD000;
pub const CUSTOM_REGS_BASE: u32 = 0xDFF000;
pub const ROM_BASE: u32 = 0xF80000;

pub struct Memory {
    pub chip_ram: Vec<u8>,
    pub chip_ram_mask: u32,
    pub kickstart: Vec<u8>,
    pub kickstart_mask: u32,
    pub overlay: bool,
}

impl Memory {
    pub fn new(chip_ram_size: usize, kickstart: Vec<u8>) -> Self {
        let ks_len = kickstart.len();
        Self {
            chip_ram: vec![0; chip_ram_size],
            chip_ram_mask: (chip_ram_size as u32).wrapping_sub(1),
            kickstart,
            kickstart_mask: (ks_len as u32).wrapping_sub(1),
            overlay: true, // Amiga starts with ROM overlay at $0
        }
    }

    pub fn read_byte(&self, addr: u32) -> u8 {
        let addr = addr & 0xFFFFFF;

        if self.overlay && addr < 0x200000 {
            // Overlay maps ROM to $0
            return self.kickstart[(addr & self.kickstart_mask) as usize];
        }

        if addr <= self.chip_ram_mask {
            // Within installed chip RAM
            self.chip_ram[addr as usize]
        } else if addr >= ROM_BASE {
            self.kickstart[(addr & self.kickstart_mask) as usize]
        } else {
            0xFF // Open bus / unmapped
        }
    }

    pub fn read_chip_byte(&self, addr: u32) -> u8 {
        self.chip_ram[(addr & self.chip_ram_mask) as usize]
    }

    pub fn write_byte(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xFFFFFF;
        // Only addresses within installed chip RAM respond.
        // Agnus decodes A0-A18 for 512KB, A0-A19 for 1MB, etc.
        // Addresses above the installed size are unmapped (no DTACK).
        if addr <= self.chip_ram_mask {
            // Watchpoint: detect first $FF write to BPL1 range ($A572-$C4B1)
            if val == 0xFF && addr >= 0xA572 && addr <= 0xC4B1 {
                let count = BPL1_WATCHPOINT_FIRED.fetch_add(1, Ordering::Relaxed);
                if count < 10 {
                    let pc = crate::LAST_CPU_PC.load(Ordering::Relaxed);
                    let sr = crate::LAST_CPU_SR.load(Ordering::Relaxed);
                    let tick = crate::MASTER_TICK.load(Ordering::Relaxed);
                    let mode = if sr & 0x2000 != 0 { "SUPERVISOR" } else { "USER" };
                    eprintln!("BPL1 WATCHPOINT #{}: addr=${:06X} val=$FF PC=${:08X} SR=${:04X} ({}) tick={}",
                        count, addr, pc, sr, mode, tick);
                }
            }
            // Track ALL writes to a specific address ($AD78) for debugging
            if addr == 0xAD78 {
                let count = BPL1_ADDR_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
                if count < 20 {
                    let pc = crate::LAST_CPU_PC.load(Ordering::Relaxed);
                    let sr = crate::LAST_CPU_SR.load(Ordering::Relaxed);
                    let tick = crate::MASTER_TICK.load(Ordering::Relaxed);
                    let old_val = self.chip_ram[addr as usize];
                    eprintln!("BPL1 ADDR TRACE #{}: addr=${:06X} old=${:02X} new=${:02X} PC=${:08X} SR=${:04X} tick={}",
                        count, addr, old_val, val, pc, sr, tick);
                }
            }
            self.chip_ram[addr as usize] = val;
        }
        // ROM is read-only; addresses above chip RAM size are open bus.
    }
}
