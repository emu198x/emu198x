//! MOS 8520 Complex Interface Adapter (CIA).
//!
//! The 8520 is a general-purpose I/O and timer chip used in the Amiga (two
//! instances: CIA-A and CIA-B). It provides two 8-bit I/O ports, two 16-bit
//! countdown timers, a 24-bit time-of-day counter, a serial shift register,
//! and an interrupt controller.
//!
//! ## 8520 vs 6526
//!
//! The 8520 is closely related to the 6526 (used in the C64 as the same
//! CIA block) but differs in three ways that matter for software:
//!
//! 1. **Binary TOD.** The 8520 counts the 24-bit time-of-day register
//!    in straight binary; the 6526 counts in BCD. Amiga timer.device
//!    reads/writes the register as a binary tick counter.
//! 2. **Any TOD register write halts the counter.** On the 6526 only a
//!    write to TODHI halts; the 8520 halts on TODHI, TODMID, *or*
//!    TODLO-targeting-alarm. Only a write to TODLO targeting the
//!    counter restarts it. Amiga HRM Appendix F is unambiguous here.
//! 3. **One-shot auto-start on TxHI write.** Writing the timer high
//!    byte when the timer is stopped and one-shot mode is selected
//!    auto-starts the timer. Kickstart 1.3 timer.device relies on
//!    this to arm MICROHZ waits.

/// Named bit masks for the control / ICR / status registers. Matches the
/// HRM Appendix F bit numbering so test sites can read like the spec.
pub mod bits {
    // Control register A / B (register $E / $F).
    pub const CR_START: u8 = 0x01;
    pub const CR_PBON: u8 = 0x02;
    pub const CR_OUTMODE: u8 = 0x04;
    pub const CR_RUNMODE: u8 = 0x08; // 0 = continuous, 1 = one-shot
    pub const CR_LOAD: u8 = 0x10; // strobe; does not read back
    pub const CRA_INMODE: u8 = 0x20; // 0 = PHI2, 1 = CNT
    pub const CRA_SPMODE: u8 = 0x40; // 0 = SP input, 1 = SP output
    pub const CRA_TOD_RATE: u8 = 0x80; // 0 = 60 Hz, 1 = 50 Hz (unused on Amiga)
    pub const CRB_INMODE_MASK: u8 = 0x60; // bits 5-6: 00=PHI2, 01=CNT, 10=TA, 11=CNT&TA
    pub const CRB_ALARM_SELECT: u8 = 0x80; // 0 = TOD writes clock, 1 = TOD writes alarm

    // Timer B count-source selector decoded from CRB bits 5-6.
    pub const CRB_INMODE_PHI2: u8 = 0x00;
    pub const CRB_INMODE_CNT: u8 = 0x20;
    pub const CRB_INMODE_TA: u8 = 0x40;
    pub const CRB_INMODE_CNT_TA: u8 = 0x60;

    // Interrupt control register ($D). Same bit positions in status
    // and mask; bit 7 is "master IR" in status, "SET-or-CLEAR" in
    // writes to the mask.
    pub const ICR_TA: u8 = 0x01;
    pub const ICR_TB: u8 = 0x02;
    pub const ICR_ALARM: u8 = 0x04;
    pub const ICR_SP: u8 = 0x08;
    pub const ICR_FLAG: u8 = 0x10;
    pub const ICR_ANY: u8 = 0x1F; // all five source bits
    pub const ICR_IR: u8 = 0x80; // master IR bit (read-only); also write-SET flag
}

use bits::*;
use serde::{Deserialize, Serialize};

/// Register offsets within the 16-register CIA decode space.
mod reg {
    pub const PRA: u8 = 0x00;
    pub const PRB: u8 = 0x01;
    pub const DDRA: u8 = 0x02;
    pub const DDRB: u8 = 0x03;
    pub const TA_LO: u8 = 0x04;
    pub const TA_HI: u8 = 0x05;
    pub const TB_LO: u8 = 0x06;
    pub const TB_HI: u8 = 0x07;
    pub const TOD_LO: u8 = 0x08;
    pub const TOD_MID: u8 = 0x09;
    pub const TOD_HI: u8 = 0x0A;
    pub const SDR: u8 = 0x0C;
    pub const ICR: u8 = 0x0D;
    pub const CRA: u8 = 0x0E;
    pub const CRB: u8 = 0x0F;
}

/// Side-effect-free snapshot of all implemented MOS 8520 state.
///
/// This view is intended for debuggers, traces, and runtime queries. Reading
/// it does not release timer or TOD read latches, clear interrupt status, or
/// otherwise perform a register read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cia8520DiagnosticSnapshot {
    /// Port A data-register latch.
    pub port_a: u8,
    /// Port B data-register latch.
    pub port_b: u8,
    /// Port A data-direction register.
    pub ddr_a: u8,
    /// Port B data-direction register.
    pub ddr_b: u8,
    /// Current externally driven Port A pin levels.
    pub external_a: u8,
    /// Current externally driven Port B pin levels.
    pub external_b: u8,
    /// Current Timer A counter.
    pub timer_a: u16,
    /// Timer A reload latch.
    pub timer_a_latch: u16,
    /// Whether Timer A is running.
    pub timer_a_running: bool,
    /// Whether Timer A is in one-shot mode.
    pub timer_a_oneshot: bool,
    /// Whether a Timer A force-load strobe is pending.
    pub timer_a_force_load: bool,
    /// Current Timer B counter.
    pub timer_b: u16,
    /// Timer B reload latch.
    pub timer_b_latch: u16,
    /// Whether Timer B is running.
    pub timer_b_running: bool,
    /// Whether Timer B is in one-shot mode.
    pub timer_b_oneshot: bool,
    /// Whether a Timer B force-load strobe is pending.
    pub timer_b_force_load: bool,
    /// Latched interrupt-source status bits.
    pub icr_status: u8,
    /// Enabled interrupt-source mask bits.
    pub icr_mask: u8,
    /// Control register A, with the LOAD strobe removed.
    pub cra: u8,
    /// Control register B, with the LOAD strobe removed.
    pub crb: u8,
    /// Timer A output level driven onto PB6 when CRA.PBON is enabled.
    pub pb6_out: bool,
    /// Timer B output level driven onto PB7 when CRB.PBON is enabled.
    pub pb7_out: bool,
    /// Serial data register.
    pub sdr: u8,
    /// Live 24-bit binary time-of-day counter.
    pub tod_counter: u32,
    /// Programmed 24-bit binary time-of-day alarm.
    pub tod_alarm: u32,
    /// TOD value captured by a TOD high-byte read.
    pub tod_latch: u32,
    /// Whether TOD middle/low reads currently use `tod_latch`.
    pub tod_latched: bool,
    /// Timer A high byte captured by a Timer A low-byte read.
    pub timer_a_read_hi_latch: u8,
    /// Whether the captured Timer A high byte is active.
    pub timer_a_read_hi_latched: bool,
    /// Timer B high byte captured by a Timer B low-byte read.
    pub timer_b_read_hi_latch: u8,
    /// Whether the captured Timer B high byte is active.
    pub timer_b_read_hi_latched: bool,
    /// Whether writes have halted the 8520 TOD counter.
    pub tod_halted: bool,
    /// Current Port A value after DDR and external-pin composition.
    pub port_a_output: u8,
    /// Current Port B value after DDR, external pins, and timer outputs.
    pub port_b_output: u8,
    /// Current level-sensitive `/IRQ` output.
    pub irq_active: bool,
    /// Whether CRB currently routes TOD writes to the alarm.
    pub tod_write_targets_alarm: bool,
}

/// MOS 8520 Complex Interface Adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cia8520 {
    port_a: u8,
    port_b: u8,
    ddr_a: u8,
    ddr_b: u8,
    external_a: u8,
    external_b: u8,

    timer_a: u16,
    timer_a_latch: u16,
    timer_a_running: bool,
    timer_a_oneshot: bool,
    timer_a_force_load: bool,

    timer_b: u16,
    timer_b_latch: u16,
    timer_b_running: bool,
    timer_b_oneshot: bool,
    timer_b_force_load: bool,

    icr_status: u8,
    icr_mask: u8,

    cra: u8,
    crb: u8,

    // Timer output pin levels (CRA/CRB PBON, HRM §F): Timer A drives PB6,
    // Timer B drives PB7 when PBON is set. OUTMODE selects toggle (flip
    // each underflow) vs pulse (high for the single underflow cycle).
    pb6_out: bool,
    pb7_out: bool,

    sdr: u8,
    tod_counter: u32,
    tod_alarm: u32,

    // TOD read latch: reading the MSB (reg A) freezes a snapshot.
    // Subsequent reads of regs 9/8 return latched values.
    // Reading reg 8 releases the latch.
    tod_latch: u32,
    tod_latched: bool,

    // Timer read latch: reading low byte latches the corresponding high byte
    // until the high byte register is read.
    timer_a_read_hi_latch: u8,
    timer_a_read_hi_latched: bool,
    timer_b_read_hi_latch: u8,
    timer_b_read_hi_latched: bool,

    // TOD write halt (8520-specific): any write to TOD regs $8/$9/$A
    // while the alarm-target bit (CRB bit 7) is clear stops the counter.
    // Only a subsequent write to $8 (LSB) restarts it. Used by
    // timer.device to program TOD atomically.
    tod_halted: bool,
}

impl Default for Cia8520 {
    fn default() -> Self {
        Self::new()
    }
}

impl Cia8520 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
            external_a: 0xFF,
            external_b: 0xFF,
            timer_a: 0xFFFF,
            timer_a_latch: 0xFFFF,
            timer_a_running: false,
            timer_a_oneshot: false,
            timer_a_force_load: false,
            timer_b: 0xFFFF,
            timer_b_latch: 0xFFFF,
            timer_b_running: false,
            timer_b_oneshot: false,
            timer_b_force_load: false,
            icr_status: 0,
            icr_mask: 0,
            cra: 0,
            crb: 0,
            pb6_out: false,
            pb7_out: false,
            sdr: 0,
            tod_counter: 0,
            tod_alarm: 0,
            tod_latch: 0,
            tod_latched: false,
            timer_a_read_hi_latch: 0,
            timer_a_read_hi_latched: false,
            timer_b_read_hi_latch: 0,
            timer_b_read_hi_latched: false,
            tod_halted: false,
        }
    }

    /// Advance one PHI2 (internal clock) pulse. Timers sourced from
    /// PHI2 count; timers sourced from CNT do not — call `cnt_pulse`
    /// for those. The chip's TOD input is independent (`tod_pulse`).
    pub fn phi2_pulse(&mut self) {
        self.apply_timer_force_loads();

        let ta_clocked = self.timer_a_running && (self.cra & CRA_INMODE == 0);
        let timer_a_underflow = ta_clocked && self.step_timer_a_count();
        if ta_clocked {
            self.drive_pb6(timer_a_underflow);
        }

        if self.timer_b_running {
            let src = self.crb & CRB_INMODE_MASK;
            let should_count = match src {
                CRB_INMODE_PHI2 => true,
                CRB_INMODE_TA | CRB_INMODE_CNT_TA => timer_a_underflow,
                _ => false,
            };
            if should_count {
                let timer_b_underflow = self.step_timer_b_count();
                self.drive_pb7(timer_b_underflow);
            }
        }
    }

    /// Pulse the CNT input (external clock edge). Advances:
    /// - Timer A when `CRA[5] = 1` (CNT-sourced)
    /// - Timer B when `CRB[6:5] = 01` (CNT-sourced)
    pub fn cnt_pulse(&mut self) {
        self.apply_timer_force_loads();

        let ta_clocked = self.timer_a_running && (self.cra & CRA_INMODE != 0);
        let timer_a_underflow = ta_clocked && self.step_timer_a_count();
        if ta_clocked {
            self.drive_pb6(timer_a_underflow);
        }

        let tb_clocked = self.timer_b_running && (self.crb & CRB_INMODE_MASK) == CRB_INMODE_CNT;
        let timer_b_underflow = tb_clocked && self.step_timer_b_count();
        if tb_clocked {
            self.drive_pb7(timer_b_underflow);
        }
    }

    /// Pulse the TOD input. Call when the appropriate external signal
    /// arrives:
    /// - CIA-A: /VSYNC (once per frame, ~50 Hz PAL)
    /// - CIA-B: /HSYNC (once per scanline, ~15,625 Hz PAL)
    pub fn tod_pulse(&mut self) {
        if self.tod_halted {
            return;
        }
        self.tod_counter = self.tod_counter.wrapping_add(1) & 0x00FF_FFFF;
        if self.tod_counter == self.tod_alarm {
            self.icr_status |= ICR_ALARM;
        }
    }

    /// FLAG pin negative edge. On the Amiga, CIA-B uses this for the
    /// floppy index pulse.
    pub fn flag_falling_edge(&mut self) {
        self.icr_status |= ICR_FLAG;
    }

    /// Inject a complete serial byte (keyboard clocks 8 bits via CNT).
    /// Sets ICR bit 3 (SP) and stores byte in SDR.
    pub fn receive_serial_byte(&mut self, byte: u8) {
        self.sdr = byte;
        self.icr_status |= ICR_SP;
    }

    /// Level-sensitive /IRQ output: asserted while any unmasked ICR
    /// status flag is set.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        self.icr_status & self.icr_mask & ICR_ANY != 0
    }

    /// Side-effect-free register peek. Mirrors `read` exactly except
    /// that it does not mutate any read-latches or clear the ICR
    /// status flags. Useful for debuggers, tracing, and UI instruments.
    #[must_use]
    pub fn peek(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            reg::PRA => Self::effective_port(self.port_a, self.ddr_a, self.external_a),
            reg::PRB => self.port_b_value(),
            reg::DDRA => self.ddr_a,
            reg::DDRB => self.ddr_b,
            reg::TA_LO => self.timer_a as u8,
            reg::TA_HI => {
                if self.timer_a_read_hi_latched {
                    self.timer_a_read_hi_latch
                } else {
                    (self.timer_a >> 8) as u8
                }
            }
            reg::TB_LO => self.timer_b as u8,
            reg::TB_HI => {
                if self.timer_b_read_hi_latched {
                    self.timer_b_read_hi_latch
                } else {
                    (self.timer_b >> 8) as u8
                }
            }
            reg::TOD_LO => {
                let val = if self.tod_latched {
                    self.tod_latch
                } else {
                    self.tod_counter
                };
                val as u8
            }
            reg::TOD_MID => {
                let val = if self.tod_latched {
                    self.tod_latch
                } else {
                    self.tod_counter
                };
                (val >> 8) as u8
            }
            reg::TOD_HI => {
                let val = if self.tod_latched {
                    self.tod_latch
                } else {
                    self.tod_counter
                };
                (val >> 16) as u8
            }
            reg::SDR => self.sdr,
            reg::ICR => {
                let ir = if self.irq_active() { ICR_IR } else { 0 };
                ir | self.icr_status
            }
            reg::CRA => self.cra,
            reg::CRB => self.crb,
            _ => 0xFF,
        }
    }

    /// Read a register, applying side effects (ICR clears, TOD/timer
    /// read-latches). `peek` is the side-effect-free sibling.
    pub fn read(&mut self, reg: u8) -> u8 {
        let value = self.peek(reg);
        match reg & 0x0F {
            reg::TA_LO => {
                self.timer_a_read_hi_latch = (self.timer_a >> 8) as u8;
                self.timer_a_read_hi_latched = true;
            }
            reg::TA_HI => self.timer_a_read_hi_latched = false,
            reg::TB_LO => {
                self.timer_b_read_hi_latch = (self.timer_b >> 8) as u8;
                self.timer_b_read_hi_latched = true;
            }
            reg::TB_HI => self.timer_b_read_hi_latched = false,
            reg::TOD_LO => self.tod_latched = false,
            reg::TOD_HI if !self.tod_latched => {
                self.tod_latch = self.tod_counter;
                self.tod_latched = true;
            }
            reg::ICR => self.icr_status = 0,
            _ => {}
        }
        value
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            reg::PRA => self.port_a = value,
            reg::PRB => self.port_b = value,
            reg::DDRA => self.ddr_a = value,
            reg::DDRB => self.ddr_b = value,
            reg::TA_LO => self.timer_a_latch = (self.timer_a_latch & 0xFF00) | u16::from(value),
            reg::TA_HI => {
                self.timer_a_latch = (self.timer_a_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_a_running {
                    self.timer_a = self.timer_a_latch;
                    // 8520: in one-shot mode, high-byte write auto-starts.
                    if self.timer_a_oneshot {
                        self.timer_a_running = true;
                        self.cra |= CR_START;
                    }
                }
            }
            reg::TB_LO => self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value),
            reg::TB_HI => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if !self.timer_b_running {
                    self.timer_b = self.timer_b_latch;
                    // 8520: in one-shot mode, high-byte write auto-starts.
                    if self.timer_b_oneshot {
                        self.timer_b_running = true;
                        self.crb |= CR_START;
                    }
                }
            }
            // TOD write-halt (8520): writes to $8/$9/$A that target the
            // counter stop it. Only a write to $8 restarts. Writes that
            // target the alarm (CRB.ALARM_SELECT=1) don't touch halt state.
            reg::TOD_LO => {
                let targets_alarm = self.tod_write_targets_alarm();
                self.write_tod_byte(0, value);
                if !targets_alarm {
                    self.tod_halted = false;
                }
            }
            reg::TOD_MID => {
                let targets_alarm = self.tod_write_targets_alarm();
                self.write_tod_byte(1, value);
                if !targets_alarm {
                    self.tod_halted = true;
                }
            }
            reg::TOD_HI => {
                let targets_alarm = self.tod_write_targets_alarm();
                self.write_tod_byte(2, value);
                if !targets_alarm {
                    self.tod_halted = true;
                }
            }
            reg::SDR => self.sdr = value,
            reg::ICR => {
                // Bit 7: 1 = SET the bits in `value & 0x1F`, 0 = CLEAR.
                if value & ICR_IR != 0 {
                    self.icr_mask |= value & ICR_ANY;
                } else {
                    self.icr_mask &= !(value & ICR_ANY);
                }
            }
            reg::CRA => {
                let was_running = self.timer_a_running;
                // LOAD (bit 4) is a strobe — does not read back.
                self.cra = value & !CR_LOAD;
                self.timer_a_running = value & CR_START != 0;
                self.timer_a_oneshot = value & CR_RUNMODE != 0;
                if value & CR_LOAD != 0 {
                    self.timer_a_force_load = true;
                }
                // Toggle output is set high when the timer is started
                // (HRM §F); pulse output idles low.
                if !was_running && self.timer_a_running && value & CR_OUTMODE != 0 {
                    self.pb6_out = true;
                }
            }
            reg::CRB => {
                let was_running = self.timer_b_running;
                self.crb = value & !CR_LOAD;
                self.timer_b_running = value & CR_START != 0;
                self.timer_b_oneshot = value & CR_RUNMODE != 0;
                if value & CR_LOAD != 0 {
                    self.timer_b_force_load = true;
                }
                // Toggle output set high on start (HRM §F); pulse idles low.
                if !was_running && self.timer_b_running && value & CR_OUTMODE != 0 {
                    self.pb7_out = true;
                }
            }
            _ => {}
        }
    }

    /// Hardware reset: clears registers to power-on state. Called when
    /// the 68000 RESET instruction asserts the reset line. TOD
    /// counter/alarm and external-pin latches are preserved (HRM
    /// Appendix F — the TOD register "is not affected by RES").
    pub fn reset(&mut self) {
        self.port_a = 0xFF;
        self.port_b = 0xFF;
        self.ddr_a = 0;
        self.ddr_b = 0;
        self.timer_a = 0xFFFF;
        self.timer_a_latch = 0xFFFF;
        self.timer_a_running = false;
        self.timer_a_oneshot = false;
        self.timer_a_force_load = false;
        self.timer_b = 0xFFFF;
        self.timer_b_latch = 0xFFFF;
        self.timer_b_running = false;
        self.timer_b_oneshot = false;
        self.timer_b_force_load = false;
        self.icr_status = 0;
        self.icr_mask = 0;
        self.cra = 0;
        self.crb = 0;
        self.pb6_out = false;
        self.pb7_out = false;
        self.sdr = 0;
        self.tod_latched = false;
        self.timer_a_read_hi_latched = false;
        self.timer_b_read_hi_latched = false;
        self.tod_halted = false;
    }

    // ── External-pin drive ───────────────────────────────────────────

    #[must_use]
    pub fn external_a(&self) -> u8 {
        self.external_a
    }

    #[must_use]
    pub fn external_b(&self) -> u8 {
        self.external_b
    }

    pub fn set_external_a(&mut self, value: u8) {
        self.external_a = value;
    }

    pub fn set_external_b(&mut self, value: u8) {
        self.external_b = value;
    }

    // ── Diagnostic accessors ─────────────────────────────────────────
    //
    // These expose live internal state for debuggers, tracing tools,
    // and tests. They must never be used by runtime-critical code: the
    // register-level API (`read`/`write`/`peek`) is the contract.

    #[must_use]
    pub fn timer_a(&self) -> u16 {
        self.timer_a
    }
    #[must_use]
    pub fn timer_b(&self) -> u16 {
        self.timer_b
    }
    #[must_use]
    pub fn timer_a_running(&self) -> bool {
        self.timer_a_running
    }
    #[must_use]
    pub fn timer_b_running(&self) -> bool {
        self.timer_b_running
    }
    #[must_use]
    pub fn icr_status(&self) -> u8 {
        self.icr_status
    }
    #[must_use]
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }
    #[must_use]
    pub fn port_a_latch(&self) -> u8 {
        self.port_a
    }
    #[must_use]
    pub fn port_b_latch(&self) -> u8 {
        self.port_b
    }
    #[must_use]
    pub fn ddr_a(&self) -> u8 {
        self.ddr_a
    }
    #[must_use]
    pub fn ddr_b(&self) -> u8 {
        self.ddr_b
    }
    #[must_use]
    pub fn cra(&self) -> u8 {
        self.cra
    }
    #[must_use]
    pub fn crb(&self) -> u8 {
        self.crb
    }
    #[must_use]
    pub fn sdr(&self) -> u8 {
        self.sdr
    }
    #[must_use]
    pub fn tod_counter(&self) -> u32 {
        self.tod_counter
    }
    #[must_use]
    pub fn tod_alarm(&self) -> u32 {
        self.tod_alarm
    }
    #[must_use]
    pub fn tod_halted(&self) -> bool {
        self.tod_halted
    }

    #[must_use]
    pub fn port_a_output(&self) -> u8 {
        Self::effective_port(self.port_a, self.ddr_a, self.external_a)
    }

    #[must_use]
    pub fn port_b_output(&self) -> u8 {
        self.port_b_value()
    }

    /// Return a side-effect-free snapshot of every implemented CIA register,
    /// latch, timer, port, TOD, serial, control, and interrupt field.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> Cia8520DiagnosticSnapshot {
        Cia8520DiagnosticSnapshot {
            port_a: self.port_a,
            port_b: self.port_b,
            ddr_a: self.ddr_a,
            ddr_b: self.ddr_b,
            external_a: self.external_a,
            external_b: self.external_b,
            timer_a: self.timer_a,
            timer_a_latch: self.timer_a_latch,
            timer_a_running: self.timer_a_running,
            timer_a_oneshot: self.timer_a_oneshot,
            timer_a_force_load: self.timer_a_force_load,
            timer_b: self.timer_b,
            timer_b_latch: self.timer_b_latch,
            timer_b_running: self.timer_b_running,
            timer_b_oneshot: self.timer_b_oneshot,
            timer_b_force_load: self.timer_b_force_load,
            icr_status: self.icr_status,
            icr_mask: self.icr_mask,
            cra: self.cra,
            crb: self.crb,
            pb6_out: self.pb6_out,
            pb7_out: self.pb7_out,
            sdr: self.sdr,
            tod_counter: self.tod_counter,
            tod_alarm: self.tod_alarm,
            tod_latch: self.tod_latch,
            tod_latched: self.tod_latched,
            timer_a_read_hi_latch: self.timer_a_read_hi_latch,
            timer_a_read_hi_latched: self.timer_a_read_hi_latched,
            timer_b_read_hi_latch: self.timer_b_read_hi_latch,
            timer_b_read_hi_latched: self.timer_b_read_hi_latched,
            tod_halted: self.tod_halted,
            port_a_output: self.port_a_output(),
            port_b_output: self.port_b_output(),
            irq_active: self.irq_active(),
            tod_write_targets_alarm: self.tod_write_targets_alarm(),
        }
    }

    // ── Internals ────────────────────────────────────────────────────

    fn effective_port(data: u8, direction: u8, external: u8) -> u8 {
        (data & direction) | (external & !direction)
    }

    /// Drive Timer A's underflow onto PB6 per CRA PBON/OUTMODE (HRM §F).
    /// A no-op unless PBON (CRA bit 1) is set. Toggle mode (OUTMODE = 1)
    /// flips the level on each underflow — a square wave at half the
    /// underflow rate; pulse mode (OUTMODE = 0) holds the level high for
    /// the single underflow cycle and low otherwise.
    fn drive_pb6(&mut self, underflow: bool) {
        if self.cra & CR_PBON == 0 {
            return;
        }
        if self.cra & CR_OUTMODE != 0 {
            self.pb6_out ^= underflow;
        } else {
            self.pb6_out = underflow;
        }
    }

    /// Drive Timer B's underflow onto PB7 per CRB PBON/OUTMODE. See
    /// [`Self::drive_pb6`].
    fn drive_pb7(&mut self, underflow: bool) {
        if self.crb & CR_PBON == 0 {
            return;
        }
        if self.crb & CR_OUTMODE != 0 {
            self.pb7_out ^= underflow;
        } else {
            self.pb7_out = underflow;
        }
    }

    /// Port-B read value with the timer outputs overlaid. When PBON is
    /// set the pin becomes a timer-driven output: PB6 (Timer A) / PB7
    /// (Timer B) read back the timer output level regardless of DDRB,
    /// per HRM §F.
    fn port_b_value(&self) -> u8 {
        let mut v = Self::effective_port(self.port_b, self.ddr_b, self.external_b);
        if self.cra & CR_PBON != 0 {
            v = (v & !0x40) | (u8::from(self.pb6_out) << 6);
        }
        if self.crb & CR_PBON != 0 {
            v = (v & !0x80) | (u8::from(self.pb7_out) << 7);
        }
        v
    }

    fn write_tod_byte(&mut self, byte_index: u8, value: u8) {
        let shift = u32::from(byte_index) * 8;
        let mask = !(0xFFu32 << shift);
        if self.tod_write_targets_alarm() {
            self.tod_alarm = ((self.tod_alarm & mask) | (u32::from(value) << shift)) & 0x00FF_FFFF;
        } else {
            self.tod_counter =
                ((self.tod_counter & mask) | (u32::from(value) << shift)) & 0x00FF_FFFF;
        }
    }

    fn tod_write_targets_alarm(&self) -> bool {
        self.crb & CRB_ALARM_SELECT != 0
    }

    fn apply_timer_force_loads(&mut self) {
        if self.timer_a_force_load {
            self.timer_a = self.timer_a_latch;
            self.timer_a_force_load = false;
        }
        if self.timer_b_force_load {
            self.timer_b = self.timer_b_latch;
            self.timer_b_force_load = false;
        }
    }

    fn step_timer_a_count(&mut self) -> bool {
        // Datasheet: "$0000 is visible for one cycle before the
        // underflow flag appears." A read on the zero tick observes 0;
        // the next tick reloads and raises the flag.
        if self.timer_a == 0 {
            self.icr_status |= ICR_TA;
            self.timer_a = self.timer_a_latch;
            if self.timer_a_oneshot {
                self.timer_a_running = false;
                self.cra &= !CR_START;
            }
            true
        } else {
            self.timer_a -= 1;
            false
        }
    }

    fn step_timer_b_count(&mut self) -> bool {
        if self.timer_b == 0 {
            self.icr_status |= ICR_TB;
            self.timer_b = self.timer_b_latch;
            if self.timer_b_oneshot {
                self.timer_b_running = false;
                self.crb &= !CR_START;
            }
            true
        } else {
            self.timer_b -= 1;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bits::*;
    use super::reg;
    use super::*;

    #[test]
    fn timer_low_read_latches_high_until_high_read() {
        let mut cia = Cia8520::new();
        cia.timer_a = 0x1234;
        cia.timer_a_running = true;
        cia.cra = CR_START;

        assert_eq!(cia.read(reg::TA_LO), 0x34);
        cia.phi2_pulse();
        assert_eq!(cia.timer_a, 0x1233);

        // High byte returns the value latched by the earlier low-byte read.
        assert_eq!(cia.read(reg::TA_HI), 0x12);

        // After the latch is consumed, reads return the live high byte.
        cia.timer_a = 0xABCD;
        assert_eq!(cia.read(reg::TA_HI), 0xAB);
    }

    #[test]
    fn timer_b_low_read_latches_high_until_high_read() {
        let mut cia = Cia8520::new();
        cia.timer_b = 0x5678;
        cia.timer_b_running = true;
        cia.crb = CR_START;

        assert_eq!(cia.read(reg::TB_LO), 0x78);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b, 0x5677);
        assert_eq!(cia.read(reg::TB_HI), 0x56);
    }

    #[test]
    fn cra_crb_load_bit_is_strobe_and_reads_back_clear() {
        let mut cia = Cia8520::new();

        cia.write(reg::TA_LO, 0x34);
        cia.write(reg::TA_HI, 0x12);
        cia.write(reg::CRA, CR_LOAD); // LOAD strobe only
        assert_eq!(cia.read(reg::CRA) & CR_LOAD, 0);
        assert!(cia.timer_a_force_load);
        cia.phi2_pulse();
        assert_eq!(cia.timer_a, 0x1234);
        assert!(!cia.timer_a_force_load);

        cia.write(reg::TB_LO, 0x78);
        cia.write(reg::TB_HI, 0x56);
        cia.write(reg::CRB, CR_LOAD);
        assert_eq!(cia.read(reg::CRB) & CR_LOAD, 0);
        assert!(cia.timer_b_force_load);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b, 0x5678);
        assert!(!cia.timer_b_force_load);
    }

    #[test]
    fn timer_a_oneshot_high_byte_write_autostarts_and_stops_on_underflow() {
        let mut cia = Cia8520::new();

        cia.write(reg::CRA, CR_RUNMODE);
        assert!(!cia.timer_a_running());

        cia.write(reg::TA_LO, 0x02);
        cia.write(reg::TA_HI, 0x00);

        assert!(cia.timer_a_running());
        assert_ne!(cia.read(reg::CRA) & CR_START, 0);
        assert_eq!(cia.timer_a(), 0x0002);

        cia.phi2_pulse(); // 2 → 1
        cia.phi2_pulse(); // 1 → 0
        cia.phi2_pulse(); // underflow, reload, stop (one-shot)

        assert_eq!(cia.timer_a(), 0x0002);
        assert!(!cia.timer_a_running());
        assert_eq!(cia.read(reg::CRA) & CR_START, 0);
        assert_ne!(cia.icr_status() & ICR_TA, 0);
    }

    #[test]
    fn timer_b_chained_mode_counts_only_timer_a_underflows() {
        let mut cia = Cia8520::new();

        cia.timer_a = 0x0001;
        cia.timer_a_latch = 0x0001;
        cia.timer_a_running = true;
        cia.cra = CR_START;

        cia.timer_b = 0x0002;
        cia.timer_b_latch = 0x0002;
        cia.timer_b_running = true;
        cia.crb = CRB_INMODE_TA | CR_START;

        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0002);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0001);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0001);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0000);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0000);
        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0002);
        assert_ne!(cia.icr_status() & (ICR_TA | ICR_TB), 0);
    }

    #[test]
    fn icr_read_sets_master_bit_only_when_masked_and_clears_status() {
        let mut cia = Cia8520::new();

        cia.receive_serial_byte(0xA5);
        assert_eq!(cia.icr_status() & ICR_SP, ICR_SP);
        assert!(!cia.irq_active());

        let masked_off = cia.read(reg::ICR);
        assert_eq!(masked_off & ICR_SP, ICR_SP);
        assert_eq!(masked_off & ICR_IR, 0);
        assert_eq!(cia.icr_status(), 0);

        cia.write(reg::ICR, ICR_IR | ICR_SP); // SET mask bit 3
        cia.receive_serial_byte(0x5A);
        assert!(cia.irq_active());

        let masked_on = cia.read(reg::ICR);
        assert_eq!(masked_on & ICR_SP, ICR_SP);
        assert_eq!(masked_on & ICR_IR, ICR_IR);
        assert_eq!(cia.icr_status(), 0);
        assert!(!cia.irq_active());
    }

    #[test]
    fn peek_icr_does_not_clear_status() {
        let mut cia = Cia8520::new();
        cia.receive_serial_byte(0);
        assert_eq!(cia.peek(reg::ICR) & ICR_SP, ICR_SP);
        assert_eq!(cia.peek(reg::ICR) & ICR_SP, ICR_SP);
        let _ = cia.read(reg::ICR);
        assert_eq!(cia.peek(reg::ICR) & ICR_SP, 0);
    }

    #[test]
    fn flag_falling_edge_sets_flag_status_and_irq_when_masked() {
        let mut cia = Cia8520::new();
        cia.flag_falling_edge();
        assert_eq!(cia.icr_status() & ICR_FLAG, ICR_FLAG);
        assert!(!cia.irq_active());

        let _ = cia.read(reg::ICR);
        cia.write(reg::ICR, ICR_IR | ICR_FLAG);
        cia.flag_falling_edge();

        assert_eq!(cia.icr_status() & ICR_FLAG, ICR_FLAG);
        assert!(cia.irq_active());
        assert_eq!(cia.read(reg::ICR) & (ICR_FLAG | ICR_IR), ICR_FLAG | ICR_IR);
    }

    #[test]
    fn timer_a_cnt_mode_counts_on_cnt_pulses_not_phi2() {
        let mut cia = Cia8520::new();
        cia.timer_a = 0x0002;
        cia.timer_a_latch = 0x0002;
        cia.timer_a_running = true;
        cia.cra = CR_START | CRA_INMODE;

        cia.phi2_pulse();
        assert_eq!(cia.timer_a(), 0x0002);

        cia.cnt_pulse();
        assert_eq!(cia.timer_a(), 0x0001);
        cia.cnt_pulse();
        assert_eq!(cia.timer_a(), 0x0000);
        cia.cnt_pulse();
        assert_eq!(cia.timer_a(), 0x0002);
        assert_ne!(cia.icr_status() & ICR_TA, 0);
    }

    #[test]
    fn timer_b_cnt_mode_counts_on_cnt_pulses_not_phi2() {
        let mut cia = Cia8520::new();
        cia.timer_b = 0x0001;
        cia.timer_b_latch = 0x0001;
        cia.timer_b_running = true;
        cia.crb = CRB_INMODE_CNT | CR_START;

        cia.phi2_pulse();
        assert_eq!(cia.timer_b(), 0x0001);
        cia.cnt_pulse();
        assert_eq!(cia.timer_b(), 0x0000);
        cia.cnt_pulse();
        assert_eq!(cia.timer_b(), 0x0001);
        assert_ne!(cia.icr_status() & ICR_TB, 0);
    }

    #[test]
    fn tod_writes_target_alarm_when_alarm_bit_set() {
        let mut cia = Cia8520::new();
        cia.tod_counter = 0x00AA55;
        cia.write(reg::CRB, CRB_ALARM_SELECT);
        cia.write(reg::TOD_HI, 0x12);
        assert!(!cia.tod_halted());
        cia.write(reg::TOD_MID, 0x34);
        cia.write(reg::TOD_LO, 0x56);
        assert!(!cia.tod_halted());
        assert_eq!(cia.tod_counter(), 0x00AA55);
        assert_eq!(cia.tod_alarm(), 0x123456);
    }

    #[test]
    fn tod_writes_target_clock_when_alarm_bit_clear() {
        let mut cia = Cia8520::new();
        cia.write(reg::CRB, 0);
        cia.write(reg::TOD_HI, 0x12);
        assert!(cia.tod_halted());
        cia.write(reg::TOD_MID, 0x34);
        cia.write(reg::TOD_LO, 0x56);
        assert!(!cia.tod_halted());
        assert_eq!(cia.tod_counter(), 0x123456);
        assert_eq!(cia.tod_alarm(), 0x000000);
    }

    #[test]
    fn tod_clock_write_preserves_alarm_value_with_normal_halt_protocol() {
        let mut cia = Cia8520::new();
        cia.tod_alarm = 0x00ABCD;
        cia.write(reg::CRB, 0);
        cia.write(reg::TOD_HI, 0x12);
        assert!(cia.tod_halted());
        cia.write(reg::TOD_MID, 0x34);
        cia.write(reg::TOD_LO, 0x56);
        assert!(!cia.tod_halted());
        assert_eq!(cia.tod_counter(), 0x123456);
        assert_eq!(cia.tod_alarm(), 0x00ABCD);
    }

    #[test]
    fn tod_alarm_write_does_not_restart_a_halted_clock() {
        let mut cia = Cia8520::new();
        cia.write(reg::CRB, 0);
        cia.write(reg::TOD_HI, 0x12);
        assert!(cia.tod_halted());

        cia.write(reg::CRB, CRB_ALARM_SELECT);
        cia.write(reg::TOD_HI, 0xAB);
        cia.write(reg::TOD_MID, 0xCD);
        cia.write(reg::TOD_LO, 0xEF);

        assert!(cia.tod_halted());
        assert_eq!(cia.tod_counter(), 0x120000);
        assert_eq!(cia.tod_alarm(), 0xABCDEF);
    }

    #[test]
    fn tod_mid_write_alone_halts_counter_on_8520() {
        // 8520-specific: writing MID ($9) halts even without writing MSB
        // first. The 6526 only halts on MSB write.
        let mut cia = Cia8520::new();
        cia.write(reg::CRB, 0);
        cia.write(reg::TOD_MID, 0x34);
        assert!(cia.tod_halted());
    }

    #[test]
    fn tod_lsb_write_alone_commits_and_leaves_counter_running() {
        let mut cia = Cia8520::new();
        cia.write(reg::CRB, 0);
        cia.write(reg::TOD_LO, 0x01);
        assert!(!cia.tod_halted());
        assert_eq!(cia.tod_counter() & 0xFF, 0x01);
    }

    #[test]
    fn external_pin_drive_visible_through_port_read() {
        let mut cia = Cia8520::new();
        cia.set_external_a(0xEB);
        // DDRA defaults to all-input, so the read returns the external
        // value byte-for-byte (floating-high bits are carried by the
        // caller's chosen external value).
        assert_eq!(cia.read(reg::PRA), 0xEB);
        assert_eq!(cia.external_a(), 0xEB);
    }

    // ── PBON / OUTMODE timer output on PB6 / PB7 (HRM §F, #451) ──────

    #[test]
    fn timer_a_toggle_drives_pb6_square_wave() {
        let mut cia = Cia8520::new();
        // Latch = 1 → Timer A underflows every 2 PHI2 pulses.
        cia.write(reg::TA_LO, 1);
        cia.write(reg::TA_HI, 0);
        // PBON + toggle (OUTMODE) + continuous + start.
        cia.write(reg::CRA, CR_PBON | CR_OUTMODE | CR_START);

        // Toggle output is initialised high when the timer starts.
        assert_eq!(cia.read(reg::PRB) & 0x40, 0x40, "PB6 high after start");

        cia.phi2_pulse(); // 1 -> 0
        cia.phi2_pulse(); // underflow #1 -> toggle low
        assert_eq!(cia.read(reg::PRB) & 0x40, 0x00, "PB6 low after underflow 1");

        cia.phi2_pulse(); // 1 -> 0
        cia.phi2_pulse(); // underflow #2 -> toggle high
        assert_eq!(
            cia.read(reg::PRB) & 0x40,
            0x40,
            "PB6 high after underflow 2"
        );
    }

    #[test]
    fn timer_b_pulse_drives_pb7_for_one_cycle() {
        let mut cia = Cia8520::new();
        cia.write(reg::TB_LO, 1);
        cia.write(reg::TB_HI, 0);
        // PBON + pulse (OUTMODE clear) + continuous + start, PHI2-sourced.
        cia.write(reg::CRB, CR_PBON | CR_START);

        // Pulse output idles low.
        assert_eq!(cia.read(reg::PRB) & 0x80, 0x00, "PB7 idles low");
        cia.phi2_pulse(); // 1 -> 0, still no underflow
        assert_eq!(cia.read(reg::PRB) & 0x80, 0x00, "PB7 low pre-underflow");
        cia.phi2_pulse(); // underflow -> high for this cycle
        assert_eq!(
            cia.read(reg::PRB) & 0x80,
            0x80,
            "PB7 high on the underflow cycle"
        );
        cia.phi2_pulse(); // next cycle -> back low
        assert_eq!(cia.read(reg::PRB) & 0x80, 0x00, "PB7 low one cycle later");
    }

    #[test]
    fn without_pbon_the_timer_leaves_pb6_pb7_alone() {
        let mut cia = Cia8520::new();
        // PB6/PB7 driven as outputs holding 1s; no PBON.
        cia.write(reg::DDRB, 0xC0);
        cia.write(reg::PRB, 0xC0);
        cia.write(reg::TA_LO, 1);
        cia.write(reg::TA_HI, 0);
        // Toggle + start but PBON clear — the timer must not reach the pins.
        cia.write(reg::CRA, CR_OUTMODE | CR_START);

        cia.phi2_pulse();
        cia.phi2_pulse(); // underflow — would toggle if PBON were set
        assert_eq!(
            cia.read(reg::PRB) & 0xC0,
            0xC0,
            "PB6/PB7 keep their port value when PBON is clear"
        );
    }
}
