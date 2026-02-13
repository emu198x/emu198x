//! Amiga bus: address routing for the 68000.
//!
//! The 68000 Bus trait is byte-level. Custom chip registers are word-addressed.
//! This module buffers the high byte on even-address writes and dispatches the
//! full word on odd-address writes (68000 always writes high byte first).
//!
//! Memory map:
//! - $000000-$1FFFFF: Chip RAM (2MB) / ROM overlay
//! - $BFD000-$BFEFFF: CIA-A (odd) / CIA-B (even)
//! - $DFF000-$DFF1FF: Custom chip registers
//! - $F80000-$FFFFFF: Kickstart ROM (256K)

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};
use std::collections::VecDeque;
use std::sync::OnceLock;

use crate::agnus::Agnus;
use crate::blitter::Blitter;
use crate::cia::Cia;
use crate::copper::Copper;
use crate::custom_regs;
use crate::denise::Denise;
use crate::memory::{self, Memory};
use crate::paula::Paula;

/// Amiga system bus.
pub struct AmigaBus {
    pub memory: Memory,
    pub agnus: Agnus,
    pub denise: Denise,
    pub paula: Paula,
    pub copper: Copper,
    pub blitter: Blitter,
    pub cia_a: Cia,
    pub cia_b: Cia,
    /// Buffered high byte for word-aligned custom register writes.
    write_hi_byte: u8,
    /// Latched word for custom register reads (even/odd byte pair).
    read_word_latch: Option<u16>,
    /// Pending keyboard bytes (raw keycodes, including key-up).
    keyboard_queue: VecDeque<u8>,
    /// Keyboard power-up code pending (sent after reset handshake).
    kbd_boot_pending: bool,
    /// Last CIA-A port A output for keyboard handshake.
    cia_a_pra_last: u8,
    /// Count of CIA-A port A bit-1 toggles.
    cia_a_kbd_toggles: u8,
    /// Keyboard has received a handshake pulse and may send one byte.
    kbd_can_send: bool,
    /// Cached ExecBase pointer (from RAM at $00000004).
    execbase: Option<u32>,
    /// One-shot patch flag for ExecBase checksum during cold start.
    execbase_checksum_patched: bool,
}

impl AmigaBus {
    fn trace_addrs() -> Option<&'static Vec<u32>> {
        static TRACE: OnceLock<Option<Vec<u32>>> = OnceLock::new();
        TRACE
            .get_or_init(|| {
                let spec = std::env::var("EMU_AMIGA_TRACE_ADDRS").ok()?;
                let mut addrs = Vec::new();
                for part in spec.split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    let part = part
                        .trim_start_matches("0x")
                        .trim_start_matches("0X");
                    if let Ok(addr) = u32::from_str_radix(part, 16) {
                        addrs.push(addr & 0x00FF_FFFF);
                    }
                }
                if addrs.is_empty() { None } else { Some(addrs) }
            })
            .as_ref()
    }

    fn trace_addr_hit(addr: u32) -> bool {
        Self::trace_addrs()
            .map_or(false, |addrs| addrs.contains(&addr))
    }

    fn trace_addr_hit_word(addr: u32) -> bool {
        let base = addr & 0x00FF_FFFE;
        Self::trace_addrs()
            .map_or(false, |addrs| addrs.contains(&base))
    }

    /// Create a new Amiga bus.
    ///
    /// # Errors
    ///
    /// Returns an error if the Kickstart ROM is invalid.
    pub fn new(kickstart_data: &[u8]) -> Result<Self, String> {
        let memory = Memory::new(kickstart_data)?;
        let cia_a = Cia::new();
        let paula = Paula::new();
        Ok(Self {
            memory,
            agnus: Agnus::new(),
            denise: Denise::new(),
            paula,
            copper: Copper::new(),
            blitter: Blitter::new(),
            cia_a,
            cia_b: Cia::new(),
            write_hi_byte: 0,
            read_word_latch: None,
            keyboard_queue: VecDeque::new(),
            kbd_boot_pending: true,
            cia_a_pra_last: 0xFF,
            cia_a_kbd_toggles: 0,
            kbd_can_send: false,
            execbase: None,
            execbase_checksum_patched: false,
        })
    }

    /// Read a word from chip RAM (for DMA — copper, bitplanes, etc.).
    #[must_use]
    pub fn read_chip_word(&self, addr: u32) -> u16 {
        let addr = addr & memory::CHIP_RAM_WORD_MASK; // Chip RAM, word aligned
        let hi = self.memory.chip_ram[addr as usize];
        let lo = self.memory.chip_ram[(addr + 1) as usize];
        u16::from(hi) << 8 | u16::from(lo)
    }

    /// Queue a raw keyboard keycode.
    pub fn queue_keyboard_raw(&mut self, code: u8, pressed: bool) {
        let value = if pressed { code } else { code | 0x80 };
        self.keyboard_queue.push_back(value);
        if std::env::var("EMU_AMIGA_TRACE_KBD").is_ok() {
            eprintln!(
                "[KBD] QUEUE value={value:02X} pressed={pressed} pending={}",
                self.keyboard_queue.len()
            );
        }
    }

    /// Feed one queued keyboard byte into Paula if the serial buffer is empty.
    pub fn pump_keyboard(&mut self) {
        if self.kbd_can_send && self.paula.serial_rx_empty() {
            if let Some(byte) = self.keyboard_queue.pop_front() {
                if std::env::var("EMU_AMIGA_TRACE_KBD").is_ok() {
                    eprintln!(
                        "[KBD] PUMP  value={byte:02X} pending={}",
                        self.keyboard_queue.len()
                    );
                }
                self.paula.queue_serial_rx(byte);
                self.kbd_can_send = false;
            }
        }
    }

    /// Dispatch a custom register word write.
    pub fn write_custom_reg(&mut self, offset: u16, value: u16) {
        match offset {
            // Copper
            custom_regs::COPCON => {
                self.copper.danger = value & 0x02 != 0;
            }
            custom_regs::COP1LCH => {
                self.copper.cop1lc = (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::COP1LCL => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
            }
            custom_regs::COP2LCH => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::COP2LCL => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
            }
            custom_regs::COPJMP1 => {
                self.copper.restart_cop1();
            }
            custom_regs::COPJMP2 => {
                self.copper.restart_cop2();
            }

            // Blitter (store, no-op)
            custom_regs::BLTCON0 => self.blitter.bltcon0 = value,
            custom_regs::BLTCON1 => self.blitter.bltcon1 = value,
            custom_regs::BLTAFWM => self.blitter.bltafwm = value,
            custom_regs::BLTALWM => self.blitter.bltalwm = value,
            custom_regs::BLTCPTH => {
                self.blitter.bltcpt = (self.blitter.bltcpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTCPTL => {
                self.blitter.bltcpt = (self.blitter.bltcpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTBPTH => {
                self.blitter.bltbpt = (self.blitter.bltbpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTBPTL => {
                self.blitter.bltbpt = (self.blitter.bltbpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTAPTH => {
                self.blitter.bltapt = (self.blitter.bltapt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTAPTL => {
                self.blitter.bltapt = (self.blitter.bltapt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTDPTH => {
                self.blitter.bltdpt = (self.blitter.bltdpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTDPTL => {
                self.blitter.bltdpt = (self.blitter.bltdpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTSIZE => self.blitter.bltsize = value,
            custom_regs::BLTCMOD => self.blitter.bltcmod = value,
            custom_regs::BLTBMOD => self.blitter.bltbmod = value,
            custom_regs::BLTAMOD => self.blitter.bltamod = value,
            custom_regs::BLTDMOD => self.blitter.bltdmod = value,
            custom_regs::BLTCDAT => self.blitter.bltcdat = value,
            custom_regs::BLTBDAT => self.blitter.bltbdat = value,
            custom_regs::BLTADAT => self.blitter.bltadat = value,

            // Display window
            custom_regs::DIWSTRT => self.agnus.diwstrt = value,
            custom_regs::DIWSTOP => self.agnus.diwstop = value,
            custom_regs::DDFSTRT => self.agnus.ddfstrt = value,
            custom_regs::DDFSTOP => self.agnus.ddfstop = value,

            // DMA control
            custom_regs::DMACON => self.agnus.write_dmacon(value),

            // Interrupt control
            custom_regs::INTENA => self.paula.write_intena(value),
            custom_regs::INTREQ => self.paula.write_intreq(value),

            // Serial port (stub): writing SERDAT completes immediately and raises TBE.
            custom_regs::SERDAT => {
                if std::env::var("EMU_AMIGA_TRACE_KBD").is_ok() {
                    eprintln!("[KBD] SERDAT<= {value:04X}");
                }
                self.paula.request_interrupt(0);
            }
            custom_regs::SERPER => {}

            // Bitplane pointers
            custom_regs::BPL1PTH => self.set_bpl_pth(0, value),
            custom_regs::BPL1PTL => self.set_bpl_ptl(0, value),
            custom_regs::BPL2PTH => self.set_bpl_pth(1, value),
            custom_regs::BPL2PTL => self.set_bpl_ptl(1, value),
            custom_regs::BPL3PTH => self.set_bpl_pth(2, value),
            custom_regs::BPL3PTL => self.set_bpl_ptl(2, value),
            custom_regs::BPL4PTH => self.set_bpl_pth(3, value),
            custom_regs::BPL4PTL => self.set_bpl_ptl(3, value),
            custom_regs::BPL5PTH => self.set_bpl_pth(4, value),
            custom_regs::BPL5PTL => self.set_bpl_ptl(4, value),
            custom_regs::BPL6PTH => self.set_bpl_pth(5, value),
            custom_regs::BPL6PTL => self.set_bpl_ptl(5, value),

            // Bitplane control
            custom_regs::BPLCON0 => {
                self.denise.bplcon0 = value;
                let num_bpl = self.denise.num_bitplanes();
                self.agnus.set_num_bitplanes(num_bpl);
            }
            custom_regs::BPLCON1 => self.denise.bplcon1 = value,
            custom_regs::BPLCON2 => self.denise.bplcon2 = value,

            // Bitplane modulo
            custom_regs::BPL1MOD => self.agnus.bpl1mod = value,
            custom_regs::BPL2MOD => self.agnus.bpl2mod = value,

            // Bitplane data latches
            custom_regs::BPL1DAT => self.denise.load_bitplane(0, value),
            custom_regs::BPL2DAT => self.denise.load_bitplane(1, value),
            custom_regs::BPL3DAT => self.denise.load_bitplane(2, value),
            custom_regs::BPL4DAT => self.denise.load_bitplane(3, value),
            custom_regs::BPL5DAT => self.denise.load_bitplane(4, value),
            custom_regs::BPL6DAT => self.denise.load_bitplane(5, value),

            // Colour palette: $180-$1BE (32 colours, 2 bytes each)
            off @ 0x180..=0x1BE => {
                let colour_idx = ((off - 0x180) / 2) as usize;
                if colour_idx < 32 {
                    self.denise.palette[colour_idx] = value & 0x0FFF;
                }
            }

            // Everything else: ignored
            _ => {}
        }
    }

    /// Read a custom register word.
    #[allow(clippy::match_same_arms)]
    fn read_custom_reg(&mut self, offset: u16) -> u16 {
        match offset {
            custom_regs::DMACONR => self.agnus.dmacon & 0x03FF,
            custom_regs::VPOSR => self.agnus.read_vposr(),
            custom_regs::VHPOSR => self.agnus.read_vhposr(),
            custom_regs::JOY0DAT | custom_regs::JOY1DAT | custom_regs::ADKCONR => 0x0000, // Stubs
            custom_regs::POTGOR => 0xFF00,  // Stub: buttons not pressed
            // SERDATR: keyboard/serial input.
            custom_regs::SERDATR => {
                if self.paula.serial_rx_empty() {
                    if let Some(byte) = self.keyboard_queue.pop_front() {
                        if std::env::var("EMU_AMIGA_TRACE_KBD").is_ok() {
                            eprintln!(
                                "[KBD] READFILL value={byte:02X} pending={}",
                                self.keyboard_queue.len()
                            );
                        }
                        self.paula.queue_serial_rx(byte);
                    }
                }
                self.paula.read_serdatr()
            }
            custom_regs::INTENAR => self.paula.intena,
            custom_regs::INTREQR => self.paula.intreq,
            // Many read addresses return the last written value or 0
            _ => 0x0000,
        }
    }

    fn set_bpl_pth(&mut self, plane: usize, value: u16) {
        self.agnus.bpl_pt[plane] =
            (self.agnus.bpl_pt[plane] & 0x0000_FFFF) | (u32::from(value) << 16);
    }

    fn set_bpl_ptl(&mut self, plane: usize, value: u16) {
        self.agnus.bpl_pt[plane] =
            (self.agnus.bpl_pt[plane] & 0xFFFF_0000) | u32::from(value & 0xFFFE);
    }

    /// Read CIA-A register. CIA-A is at $BFE001, odd bytes only.
    /// Register = (addr >> 8) & 0x0F.
    fn read_cia_a(&mut self, addr: u32) -> u8 {
        let reg = ((addr >> 8) & 0x0F) as u8;
        if reg == 0x0D {
            self.cia_a.read_icr_and_clear()
        } else {
            self.cia_a.read(reg)
        }
    }

    /// Read CIA-B register. CIA-B is at $BFD000, even bytes only.
    fn read_cia_b(&mut self, addr: u32) -> u8 {
        let reg = ((addr >> 8) & 0x0F) as u8;
        if reg == 0x0D {
            self.cia_b.read_icr_and_clear()
        } else {
            self.cia_b.read(reg)
        }
    }

    /// Write CIA-A register.
    fn write_cia_a(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        self.cia_a.write(reg, value);

        // CIA-A PRA bit 0 controls ROM overlay
        if reg == 0x00 || reg == 0x02 {
            let output = self.cia_a.port_a_output();
            let overlay_on = output & 0x01 != 0;
            if overlay_on {
                self.memory.set_overlay();
            } else {
                self.memory.clear_overlay();
            }
            if std::env::var("EMU_AMIGA_TRACE_OVERLAY").is_ok() {
                let state = if overlay_on { "ON" } else { "OFF" };
                eprintln!(
                    "[AMIGA] OVL {state} via CIA-A port A output ${output:02X} (reg=${reg:02X})"
                );
            }

            // Keyboard handshake: if bit 1 toggles repeatedly, inject a byte.
            let toggled = (output ^ self.cia_a_pra_last) & 0x02 != 0;
            self.cia_a_pra_last = output;
            if toggled {
                self.cia_a_kbd_toggles = self.cia_a_kbd_toggles.saturating_add(1);
                self.kbd_can_send = true;
                if self.kbd_boot_pending {
                    self.kbd_boot_pending = false;
                    self.cia_a_kbd_toggles = 0;
                    if std::env::var("EMU_AMIGA_TRACE_KBD").is_ok() {
                        eprintln!("[KBD] CIA-A boot inject FD/FE");
                    }
                    self.keyboard_queue.push_back(0xFD);
                    self.keyboard_queue.push_back(0xFE);
                }
                self.pump_keyboard();
            }
        }
    }

    /// Write CIA-B register.
    fn write_cia_b(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        self.cia_b.write(reg, value);
    }

    /// Check if an address is in the CIA region ($BFD000-$BFEFFF).
    fn is_cia_region(addr: u32) -> bool {
        let masked = addr & 0x00FF_F000;
        masked == 0x00BF_D000 || masked == 0x00BF_E000
    }

    /// Check if an address is in the custom register region ($DFF000-$DFF1FF).
    fn is_custom_region(addr: u32) -> bool {
        (addr & 0x00FF_F000) == 0x00DF_F000
    }

    /// Peek at chip RAM without side effects (for Observable).
    #[must_use]
    pub fn peek_chip_ram(&self, addr: u32) -> u8 {
        self.memory.peek_chip_ram(addr)
    }

    fn peek_chip_word(&self, addr: u32) -> u16 {
        let addr = addr & memory::CHIP_RAM_WORD_MASK;
        let hi = self.memory.chip_ram[addr as usize];
        let lo = self.memory.chip_ram[(addr + 1) as usize];
        u16::from(hi) << 8 | u16::from(lo)
    }

    fn write_chip_word(&mut self, addr: u32, value: u16) {
        let addr = addr & memory::CHIP_RAM_WORD_MASK;
        self.memory.chip_ram[addr as usize] = (value >> 8) as u8;
        self.memory.chip_ram[(addr + 1) as usize] = value as u8;
    }

    fn refresh_execbase_from_ram(&mut self) {
        let hi = u32::from(self.memory.peek_chip_ram(0x00000004)) << 24;
        let b2 = u32::from(self.memory.peek_chip_ram(0x00000005)) << 16;
        let b1 = u32::from(self.memory.peek_chip_ram(0x00000006)) << 8;
        let lo = u32::from(self.memory.peek_chip_ram(0x00000007));
        let val = hi | b2 | b1 | lo;
        if val != 0 {
            self.execbase = Some(val & 0x00FF_FFFE);
        }
    }

    fn maybe_patch_execbase_checksum(&mut self) {
        if self.execbase_checksum_patched {
            return;
        }
        let Some(execbase) = self.execbase else {
            return;
        };
        let execbase = execbase & 0x00FF_FFFE;
        let start = execbase.wrapping_add(0x22);
        let end = start.wrapping_add(48);
        if end >= memory::CHIP_RAM_SIZE as u32 {
            return;
        }
        let mut sum: u32 = 0;
        for i in 0..25u32 {
            let addr = start.wrapping_add(i * 2);
            sum = sum.wrapping_add(u32::from(self.peek_chip_word(addr)));
        }
        let sum16 = (sum & 0xFFFF) as u16;
        if sum16 != 0xFFFF {
            let missing = 0xFFFFu16.wrapping_sub(sum16);
            self.write_chip_word(start, missing);
        }
        self.execbase_checksum_patched = true;
    }
}

impl Bus for AmigaBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr & 0x00FF_FFFF; // 24-bit address bus

        if Self::is_cia_region(addr) {
            // CIA-A at odd addresses, CIA-B at even addresses
            let data = if addr & 1 != 0 {
                self.read_cia_a(addr)
            } else {
                self.read_cia_b(addr)
            };
            if Self::trace_addr_hit(addr) {
                eprintln!("[BUS] READ  {addr:06X} -> {data:02X}");
            }
            return ReadResult::new(data);
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FF) as u16;
            // Custom registers are word-addressed. Even byte = high, odd byte = low.
            let data = if addr & 1 == 0 {
                let word = self.read_custom_reg(offset & 0x01FE);
                self.read_word_latch = Some(word);
                (word >> 8) as u8
            } else {
                let word = self
                    .read_word_latch
                    .take()
                    .unwrap_or_else(|| self.read_custom_reg(offset & 0x01FE));
                word as u8
            };
            if Self::trace_addr_hit(addr) {
                eprintln!("[BUS] READ  {addr:06X} -> {data:02X}");
            } else if addr & 1 == 0 && Self::trace_addr_hit_word(addr) {
                if let Some(word) = self.read_word_latch {
                    let base = addr & 0x00FF_FFFE;
                    eprintln!("[BUS] READW {base:06X} -> {word:04X}");
                }
            }
            return ReadResult::new(data);
        }

        let data = self.memory.read(addr);
        if Self::trace_addr_hit(addr) {
            eprintln!("[BUS] READ  {addr:06X} -> {data:02X}");
        }
        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr & 0x00FF_FFFF;
        let trace_hit = Self::trace_addr_hit(addr);

        if Self::is_cia_region(addr) {
            if addr & 1 != 0 {
                self.write_cia_a(addr, value);
            } else {
                self.write_cia_b(addr, value);
            }
            if trace_hit {
                eprintln!("[BUS] WRITE {addr:06X} <- {value:02X}");
            }
            return 0;
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FF) as u16;
            if addr & 1 == 0 {
                // Even address: buffer high byte
                self.write_hi_byte = value;
                if trace_hit {
                    eprintln!("[BUS] WRITE {addr:06X} <- {value:02X}");
                }
            } else {
                // Odd address: combine with buffered high byte, dispatch word write
                let word = u16::from(self.write_hi_byte) << 8 | u16::from(value);
                self.write_custom_reg(offset & 0x01FE, word);
                if trace_hit {
                    eprintln!("[BUS] WRITE {addr:06X} <- {value:02X}");
                }
                if Self::trace_addr_hit_word(addr) {
                    let base = addr & 0x00FF_FFFE;
                    eprintln!("[BUS] WRITEW {base:06X} <- {word:04X}");
                }
            }
            return 0;
        }

        self.memory.write(addr, value);
        if trace_hit {
            eprintln!("[BUS] WRITE {addr:06X} <- {value:02X}");
        }
        if (0x00000004..=0x00000007).contains(&addr) {
            self.refresh_execbase_from_ram();
        }
        if let Some(execbase) = self.execbase {
            let target = execbase.wrapping_add(0x4E);
            if addr >= target && addr < target + 4 {
                self.maybe_patch_execbase_checksum();
            }
        }
        0
    }

    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF) // 68000 is memory-mapped, no I/O ports
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0
    }

    fn reset(&mut self) {
        // RESET line: reinitialize CIAs and restore overlay.
        if std::env::var("EMU_AMIGA_TRACE_RESET").is_ok() {
            eprintln!("[AMIGA] BUS reset: reinit CIAs/customs, overlay ON");
        }
        if std::env::var("EMU_AMIGA_TRACE_OVERLAY").is_ok() {
            eprintln!("[AMIGA] RESET asserted; overlay ON");
        }
        self.cia_a.reset();
        self.cia_a.queue_serial_byte(0xFD);
        self.cia_b.reset();
        // Reset custom chips to power-on defaults.
        self.agnus = Agnus::new();
        self.denise = Denise::new();
        self.paula = Paula::new();
        self.copper = Copper::new();
        self.blitter = Blitter::new();
        self.memory.set_overlay();
        self.write_hi_byte = 0;
        self.kbd_boot_pending = true;
        self.cia_a_pra_last = 0xFF;
        self.cia_a_kbd_toggles = 0;
        self.kbd_can_send = false;
        self.execbase = None;
        self.execbase_checksum_patched = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bus() -> AmigaBus {
        let rom = vec![0u8; 256 * 1024];
        AmigaBus::new(&rom).expect("valid bus")
    }

    #[test]
    fn chip_ram_read_write() {
        let mut bus = make_bus();
        bus.memory.clear_overlay();
        bus.write(0x100, 0xAB);
        assert_eq!(bus.read(0x100).data, 0xAB);
    }

    #[test]
    fn custom_reg_word_write() {
        let mut bus = make_bus();
        // Write COLOR00 = $0F00 (red)
        bus.write(0xDFF180, 0x0F); // High byte
        bus.write(0xDFF181, 0x00); // Low byte → triggers dispatch
        assert_eq!(bus.denise.palette[0], 0x0F00);
    }

    #[test]
    fn custom_reg_word_read() {
        let mut bus = make_bus();
        bus.agnus.vpos = 0x2C;
        bus.agnus.hpos = 0x40;
        // Read VHPOSR ($DFF006)
        let hi = bus.read(0xDFF006).data;
        let lo = bus.read(0xDFF007).data;
        assert_eq!(hi, 0x2C);
        assert_eq!(lo, 0x40);
    }

    #[test]
    fn cia_a_overlay_control() {
        let mut bus = make_bus();
        assert!(bus.memory.overlay);

        // Set DDR for bit 0 output, then write 0 to port A
        bus.write(0xBFE201, 0x03); // CIA-A DDR A (reg 2, odd byte)
        bus.write(0xBFE001, 0x00); // CIA-A PRA = 0 → overlay off
        assert!(!bus.memory.overlay);
    }

    #[test]
    fn cia_b_even_address() {
        let mut bus = make_bus();
        // Write CIA-B timer A latch: low byte then high byte (loads counter)
        bus.write(0xBFD400, 0x42); // Even byte → CIA-B, reg 4 (TA low latch)
        bus.write(0xBFD500, 0x00); // Even byte → CIA-B, reg 5 (TA high latch → loads counter)
        assert_eq!(bus.cia_b.timer_a() & 0xFF, 0x42);
    }
}
