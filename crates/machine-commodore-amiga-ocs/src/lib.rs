//! Commodore Amiga (OCS chipset) machine — incremental restart.
//!
//! Built milestone-by-milestone per
//! `wiki/decisions/amiga-restart-plan.md`. Each milestone adds the
//! minimum hardware behaviour the running ROM demands; nothing more.
//!
//! Current milestone: **M6 — beam counter + VBL interrupt.**

mod agnus;
mod chipset;
mod cia;
mod copper;
mod denise;
mod memory;

pub use agnus::{
    Agnus, PAL_FRAME_LINES, PAL_FRAME_TICKS, PAL_LINE_CCKS, PAL_LINE_TICKS, VBL_END_LINE,
};
pub use chipset::Chipset;
pub use cia::Cia;
pub use copper::Copper;
pub use denise::{Denise, FB_HEIGHT, FB_WIDTH};
pub use memory::{Memory, CHIP_RAM_SIZE};

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68000::Cpu68000;

const CUSTOM_BASE: u32 = 0x00DF_0000;
const CUSTOM_TOP: u32 = 0x00E0_0000;

/// CIA E-clock divider: real CIA E-clock runs at master/40 = 0.71 MHz.
/// Our primary tick unit is master/4 (= 68000 CPU clock = lores pixel
/// rate), so CIAs fire once every 10 ticks. Confirmed by HRM register
/// map: "CIAA timer A (.709379 MHz PAL)" = master/40 exactly.
const CIA_E_CLOCK_DIVISOR: u64 = 10;

/// Ticks per Agnus colour clock. A CCK (HRM beam-coordinate unit) is
/// two master/4 ticks — one tick per lores pixel.
const TICKS_PER_CCK: u64 = 2;

/// Amiga (OCS) machine.
pub struct AmigaOcs {
    cpu: Cpu68000,
    memory: Memory,
    chipset: Chipset,
    cia_a: Cia,
    cia_b: Cia,
    agnus: Agnus,
    copper: Copper,
    denise: Denise,
    tick_count: u64,
    /// Sub-CCK phase: 0 at the first tick of a CCK (fetch/reload
    /// events fire here), 1 at the second tick. Flips each tick.
    cck_phase: u8,
    /// Paula's latched state of Agnus's `/VERTB` level signal. Used
    /// to detect rising edges — INTREQ.VERTB is re-latched whenever
    /// the CPU clears it and the beam is still inside the blanking
    /// window.
    prev_vertb_level: bool,
    /// Paula's latched state of the CIA-A `/IRQ` line (level-
    /// sensitive on the CIA, edge-latched on Paula). Set to true
    /// when CIA-A has any unmasked ICR flag active.
    prev_cia_a_irq: bool,
    /// Same for CIA-B.
    prev_cia_b_irq: bool,
    e_clock_phase: u64,
    /// Diagnostic: count of unique custom-register read offsets seen
    /// since reset, indexed by offset / 2.
    pub debug_reg_read_counts: std::collections::HashMap<u16, u64>,
    /// Diagnostic: peak INTENA value seen during boot. Bit 14 set
    /// here would prove the boot has reached the master-enable code
    /// path even if INTENA is later cleared.
    pub debug_peak_intena: u16,
    /// Diagnostic: cumulative count of CPU writes to INTENA ($DFF09A).
    pub debug_intena_writes: u64,
    /// Diagnostic: per-write log of every INTENA store, captured to
    /// help trace the master-enable lifecycle. Each entry is
    /// `(cck, pc, written_word, intena_before, intena_after)`. Only
    /// writes that actually change INTENA are kept (purely-no-op
    /// writes still count toward `debug_intena_writes`).
    pub debug_intena_log: Vec<(u64, u32, u16, u16, u16)>,
}

impl AmigaOcs {
    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// and chip RAM only (no expansion).
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::with_slow_ram(kickstart, 0)
    }

    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// plus a trapdoor slow-RAM expansion at `$C00000` (common A500
    /// config: 512 KiB).
    #[must_use]
    pub fn with_slow_ram(kickstart: Vec<u8>, slow_ram_bytes: usize) -> Self {
        let memory = Memory::new_with_slow_ram(kickstart, slow_ram_bytes);
        let mut cpu = Cpu68000::new();
        let ssp = memory.read_long(0x000000);
        let pc = memory.read_long(0x000004);
        cpu.reset_to(ssp, pc);
        Self {
            cpu,
            memory,
            chipset: Chipset::new(),
            cia_a: Cia::new(),
            cia_b: Cia::new(),
            agnus: Agnus::new(),
            copper: Copper::new(),
            denise: Denise::new(),
            tick_count: 0,
            cck_phase: 0,
            prev_vertb_level: false,
            prev_cia_a_irq: false,
            prev_cia_b_irq: false,
            e_clock_phase: 0,
            debug_reg_read_counts: std::collections::HashMap::new(),
            debug_peak_intena: 0,
            debug_intena_writes: 0,
            debug_intena_log: Vec::new(),
        }
    }

    /// Read-only Agnus access.
    #[must_use]
    pub fn agnus(&self) -> &Agnus {
        &self.agnus
    }

    /// Read-only Copper access.
    #[must_use]
    pub fn copper(&self) -> &Copper {
        &self.copper
    }

    /// Read-only Denise access.
    #[must_use]
    pub fn denise(&self) -> &Denise {
        &self.denise
    }

    /// Read-only chipset access.
    #[must_use]
    pub fn chipset(&self) -> &Chipset {
        &self.chipset
    }

    /// Read-only memory access (for tests inspecting OVL state etc.).
    #[must_use]
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Read-only CIA-A access.
    #[must_use]
    pub fn cia_a(&self) -> &Cia {
        &self.cia_a
    }

    /// Read-only CIA-B access.
    #[must_use]
    pub fn cia_b(&self) -> &Cia {
        &self.cia_b
    }

    /// Convenience: CIA-A PRA byte.
    #[must_use]
    pub fn cia_a_pra(&self) -> u8 {
        self.cia_a.pra
    }

    /// Convenience: CIA-A DDRA byte.
    #[must_use]
    pub fn cia_a_ddra(&self) -> u8 {
        self.cia_a.ddra
    }

    /// Convenience: current INTENA value.
    #[must_use]
    pub fn intena(&self) -> u16 {
        self.chipset.intena
    }

    /// Convenience: current INTREQ value.
    #[must_use]
    pub fn intreq(&self) -> u16 {
        self.chipset.intreq
    }

    /// Convenience: current DMACON value.
    #[must_use]
    pub fn dmacon(&self) -> u16 {
        self.chipset.dmacon
    }

    /// Convenience: current BPLCON0 value.
    #[must_use]
    pub fn bplcon0(&self) -> u16 {
        self.chipset.bplcon0
    }

    /// Convenience: a colour table entry.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        self.chipset.color[idx]
    }

    /// Backdoor for tests: write a word as if the CPU did it.
    pub fn poke_word(&mut self, addr: u32, val: u16) {
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr) {
            let offset = (addr - CUSTOM_BASE) as u16 & 0x1FE;
            self.dispatch_custom_write(offset, val);
        } else {
            self.memory.write_word(addr, val);
        }
    }

    /// Dispatch a custom-register word write to the right submodule.
    /// Shared between `poke_word` and the CPU bus servicer.
    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        let intena_before = self.chipset.intena;
        match offset {
            0x080 => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            0x082 => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
            }
            0x084 => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            0x086 => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
            }
            0x088 => self.copper.jump1(),
            0x08A => self.copper.jump2(),
            _ => self.chipset.write_word(offset, val),
        }
        if offset == 0x09A {
            self.debug_intena_writes += 1;
            let intena_after = self.chipset.intena;
            if intena_after > self.debug_peak_intena {
                self.debug_peak_intena = intena_after;
            }
            if intena_after != intena_before {
                // Log CCKs (HRM beam-coordinate units) to keep these
                // timestamps comparable with HRM register descriptions
                // — tick_count / 2.
                self.debug_intena_log.push((
                    self.tick_count / TICKS_PER_CCK,
                    self.cpu.regs.pc,
                    val,
                    intena_before,
                    intena_after,
                ));
            }
        }
    }

    /// Backdoor for tests: write a byte as if the CPU did it.
    pub fn poke_byte(&mut self, addr: u32, val: u8) {
        if let Some(reg) = cia::decode_cia_a(addr) {
            self.cia_a.write_register(reg, val);
            self.memory.set_overlay(self.cia_a.ovl());
        } else if let Some(reg) = cia::decode_cia_b(addr) {
            self.cia_b.write_register(reg, val);
        } else if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr) {
            // Custom registers are word-only; byte writes pad with
            // the same byte in both halves on real hardware. For our
            // purposes a byte write just writes the byte value.
            let offset = (addr - CUSTOM_BASE) as u16 & 0x1FE;
            self.chipset.write_word(offset, u16::from(val) << 8 | u16::from(val));
        } else {
            self.memory.write_byte(addr, val);
        }
    }

    /// CPU access (read-only — mutating outside the tick loop breaks
    /// invariants).
    #[must_use]
    pub fn cpu(&self) -> &Cpu68000 {
        &self.cpu
    }

    /// Total master/4 ticks (= 68000 CPU clocks = lores pixels)
    /// elapsed since construction. This is the finest-grained clock
    /// in the machine.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Total Agnus CCKs (colour clocks, master/8) elapsed since
    /// construction. Derived from `tick_count` — 2 ticks per CCK.
    /// Useful for comparing timestamps against HRM beam-coordinate
    /// register values.
    #[must_use]
    pub fn cck_count(&self) -> u64 {
        self.tick_count / TICKS_PER_CCK
    }

    /// Read a word at the given 24-bit address — peeks state without
    /// side effects (does NOT clear ICR etc). For inspecting state
    /// during tests; not equivalent to a CPU bus cycle.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.bus_read_word(addr & 0xFF_FFFF)
    }

    /// Read a word as if the CPU did the bus cycle. Side-effecting:
    /// CIA-A ICR reads clear ICR; future read-side-effect registers
    /// behave like the CPU sees them.
    pub fn cpu_read_word(&mut self, addr: u32) -> u16 {
        let addr24 = addr & 0xFF_FFFF;
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.read_register(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.read_register(reg));
        }
        self.bus_read_word(addr24)
    }

    /// Read a longword (big-endian) at the given 24-bit address.
    #[must_use]
    pub fn read_long(&self, addr: u32) -> u32 {
        let hi = self.bus_read_word(addr & 0xFF_FFFF);
        let lo = self.bus_read_word(addr.wrapping_add(2) & 0xFF_FFFF);
        (u32::from(hi) << 16) | u32::from(lo)
    }

    fn bus_read_word(&self, addr24: u32) -> u16 {
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.peek_register(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.peek_register(reg));
        }
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr24) {
            let offset = (addr24 - CUSTOM_BASE) as u16 & 0x1FE;
            return match offset {
                0x004 => self.agnus.vposr(),
                0x006 => self.agnus.vhposr(),
                _ => self.chipset.read_word(offset),
            };
        }
        self.memory.read_word(addr24)
    }

    /// Read a chip-RAM byte directly, ignoring the OVL overlay.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        self.memory.read_chip_ram_byte(addr)
    }

    /// Tick one primary period — master/4 = 68000 CPU clock = lores
    /// pixel rate (7.09 MHz PAL). This is the finest granularity in
    /// the machine; everything coarser (CCK, CIA E-clock, 68000 bus
    /// cycle) derives from it.
    ///
    /// Two ticks make one Agnus CCK, so chip-side events that the HRM
    /// describes at CCK granularity (beam advance, copper fetch slot,
    /// bitplane fetch, shift-register reload) fire on alternate ticks
    /// (`cck_phase == 0`). Per-tick events (CPU clock, lores pixel
    /// output, CIA E-clock divisor, CPU bus service) fire every tick.
    pub fn tick(&mut self) {
        let phase = self.cck_phase;

        // ── CCK-granular events (phase 0 only) ───────────────────
        if phase == 0 {
            // Advance the beam.
            self.agnus.tick_cck();

            // Paula-style latch of Agnus's /VERTB level signal:
            // - On the rising edge (beam enters blanking window) we
            //   fire the copper restart — real Agnus reloads the
            //   copper PC from COP1LC at the start of every VBL.
            // - While the level stays high AND INTREQ.VERTB is
            //   clear, re-latch the bit. This models the subtle
            //   "handler clears INTREQ.VERTB mid-blanking" case —
            //   real hardware re-asserts because /VERTB is still
            //   high; a cleared-once-only pulse model would miss it.
            let vertb_level = self.agnus.vertb_level();
            let rising_edge = vertb_level && !self.prev_vertb_level;
            if rising_edge {
                self.copper.jump1();
            }
            if vertb_level && (self.chipset.intreq & 0x0020) == 0 {
                self.chipset.write_word(0x09C, 0x8020);
            }
            self.prev_vertb_level = vertb_level;

            // Copper runs when DMACON.COPEN (bit 7) AND DMAEN (bit 9)
            // are both set. Agnus arbitrates the chip bus; pass the
            // current CCK's claim so the copper yields to bitplane
            // DMA.
            let claim = denise::dma_claim(
                self.agnus.hpos,
                self.chipset.dmacon,
                self.chipset.bplcon0,
                self.chipset.ddfstrt,
                self.chipset.ddfstop,
            );
            if self.chipset.dmacon & 0x0280 == 0x0280 {
                self.copper.tick_cck(
                    &self.memory,
                    &mut self.chipset,
                    self.agnus.vpos,
                    self.agnus.hpos,
                    claim,
                );
            }
        }

        // ── Per-tick: Denise pixel + fetch/reload at phase 0 ────
        self.denise.tick(
            phase,
            self.agnus.vpos,
            self.agnus.hpos,
            &mut self.chipset,
            &self.memory,
        );

        // ── CIA E-clock: every 10 master/4 ticks = master/40 ────
        self.e_clock_phase += 1;
        if self.e_clock_phase >= CIA_E_CLOCK_DIVISOR {
            self.e_clock_phase = 0;
            self.cia_a.tick_e_clock();
            self.cia_b.tick_e_clock();
        }

        // ── Paula edge-latch of CIA /IRQ lines ──────────────────
        // CIA::irq_pending is now level-sensitive (asserted while
        // any unmasked ICR flag is set). Paula's interrupt input
        // uses a rising-edge detector, so we only set the INTREQ
        // bit on the transition from low to high. A handler that
        // clears INTREQ.PORTS / INTREQ.EXTER without reading the
        // CIA ICR will *not* trigger another interrupt until the
        // CIA line first goes low and then high again — matching
        // real hardware.
        let cia_a_irq = self.cia_a.irq_pending;
        if cia_a_irq && !self.prev_cia_a_irq {
            self.chipset.write_word(0x09C, 0x8008);
        }
        self.prev_cia_a_irq = cia_a_irq;

        let cia_b_irq = self.cia_b.irq_pending;
        if cia_b_irq && !self.prev_cia_b_irq {
            self.chipset.write_word(0x09C, 0xA000);
        }
        self.prev_cia_b_irq = cia_b_irq;

        // ── CPU: every master/4 tick = every CPU clock ──────────
        self.service_cpu_bus();
        self.cpu.ipl = self.chipset.compute_ipl();
        self.cpu.tick();

        self.tick_count += 1;
        self.cck_phase ^= 1;
    }

    fn service_cpu_bus(&mut self) {
        // Snapshot the bus-cycle parameters out of the CPU state so we
        // can mutate self.memory without borrow conflicts.
        let bus_info = match &self.cpu.state {
            State::BusCycle {
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
                ..
            } => Some((*addr, *fc, *is_read, *is_word, *data, *cycle_count)),
            _ => None,
        };

        let Some((addr, fc, is_read, is_word, data, cycle_count)) = bus_info else {
            return;
        };

        // 68000 bus cycle is 4 CCKs (S0-S7). DTACK is sampled at S4
        // = cycle 2. We complete the bus cycle on the first poll at
        // or after cycle 2 and then hold the result steady.
        if cycle_count < 2 {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        if matches!(self.cpu.bus_status, BusStatus::Ready(_) | BusStatus::Error) {
            return;
        }

        // Chip-bus arbitration. Agnus shares the chip-RAM bus between
        // DMA and the CPU; when a CCK is claimed by DMA (bitplane,
        // and later sprite/disk/audio/refresh) the CPU must stall
        // its chip-RAM access to the next free CCK.
        //
        // Only real chip-RAM accesses are contended:
        //   - Reads: low-memory reads with OVL on are routed to ROM
        //     by Gary and don't touch the chip bus — not contended.
        //   - Writes: always land in chip RAM when in the chip-RAM
        //     decode range (OVL only gates reads).
        //   - CIA / custom / slow-RAM / ROM / unmapped accesses are
        //     not on the chip-RAM arbitration path.
        let addr24 = addr & 0xFF_FFFF;
        let is_chip_ram_access = addr24 < 0x20_0000
            && (!is_read || !self.memory.overlay());
        if is_chip_ram_access {
            let claim = denise::dma_claim(
                self.agnus.hpos,
                self.chipset.dmacon,
                self.chipset.bplcon0,
                self.chipset.ddfstrt,
                self.chipset.ddfstop,
            );
            if !claim.is_free() {
                self.cpu.bus_status = BusStatus::Wait;
                return;
            }
        }

        // The Amiga uses 68000 autovectored interrupts: the chipset
        // drives /VPA during InterruptAck rather than supplying a
        // vector number, and the CPU then computes vector = 24 + IPL.
        // Our bus model returns the vector directly, so synthesise
        // (24 + ipl_being_acked). The IPL being acked lives in
        // `cpu.ipl` — the CPU sampled it just before driving this bus
        // cycle. Mask to 3 bits defensively.
        if fc == FunctionCode::InterruptAck {
            let ipl = self.cpu.ipl & 0x07;
            self.cpu.bus_status = BusStatus::Ready(24 + u16::from(ipl));
            return;
        }

        // CIA-A address space (odd bytes in $BFE000-$BFEFFF).
        if let Some(reg) = cia::decode_cia_a(addr24) {
            if is_read {
                let val = u16::from(self.cia_a.read_register(reg));
                self.memory.set_last_bus_value(val);
                self.cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                self.memory.set_last_bus_value(val);
                self.cia_a.write_register(reg, val as u8);
                self.memory.set_overlay(self.cia_a.ovl());
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        // CIA-B address space (even bytes in $BFD000-$BFDFFF).
        if let Some(reg) = cia::decode_cia_b(addr24) {
            if is_read {
                // CIA-B is on the high data byte; word reads put the
                // CIA value in the high byte. We expose the byte
                // value in the low byte for convenience to the bus.
                let val = u16::from(self.cia_b.read_register(reg));
                self.memory.set_last_bus_value(val);
                self.cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                self.memory.set_last_bus_value(val);
                // Word writes to CIA-B target the high byte; we take
                // the high byte if it's a word write, low byte if byte.
                let byte = if is_word { (val >> 8) as u8 } else { val as u8 };
                self.cia_b.write_register(reg, byte);
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        // Custom-register space dispatches to the chipset module.
        // Agnus owns the beam-position read-side registers; everything
        // else routes to Chipset.
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr24) {
            let offset = (addr24 - CUSTOM_BASE) as u16 & 0x1FE;
            if is_read {
                *self.debug_reg_read_counts.entry(offset).or_insert(0) += 1;
                let val = match offset {
                    0x004 => self.agnus.vposr(),
                    0x006 => self.agnus.vhposr(),
                    _ => self.chipset.read_word(offset),
                };
                self.memory.set_last_bus_value(val);
                self.cpu.bus_status = BusStatus::Ready(if is_word { val } else { val & 0xFF });
            } else {
                let val = data.unwrap_or(0);
                self.memory.set_last_bus_value(val);
                self.dispatch_custom_write(offset, val);
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        if is_read {
            let val = if is_word {
                self.memory.read_word(addr24)
            } else {
                u16::from(self.memory.read_byte(addr24))
            };
            self.cpu.bus_status = BusStatus::Ready(val);
        } else {
            let val = data.unwrap_or(0);
            if is_word {
                self.memory.write_word(addr24, val);
            } else {
                self.memory.write_byte(addr24, val as u8);
            }
            self.cpu.bus_status = BusStatus::Ready(0);
        }
    }
}
