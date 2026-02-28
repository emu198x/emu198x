//! Spectrum bus: memory and I/O routing.
//!
//! The bus connects the Z80 CPU to memory, video, keyboard, and beeper.
//! I/O routing is model-aware: v1 implements port $FE only. Future models
//! will add $7FFD (128K banking), $FFFD/$BFFD (AY audio), $FF (Timex SCLD),
//! etc.
//!
//! # Contention
//!
//! Memory contention is delegated to the ULA via `ula.contention()`.
//! The bus adds the returned wait states to the `ReadResult`. I/O contention
//! is similarly delegated via `ula.io_contention()`.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};
use gi_ay_3_8910::Ay3_8910;
use nec_upd765::Upd765;
use sinclair_ula::Ula;

use crate::beeper::BeeperState;
use crate::keyboard::KeyboardState;
use crate::memory::SpectrumMemory;

/// The Spectrum bus, implementing `emu_core::Bus`.
///
/// Owns the memory, ULA, keyboard, and beeper subsystems. The CPU
/// accesses all of these through the `Bus` trait.
pub struct SpectrumBus {
    pub memory: Box<dyn SpectrumMemory>,
    pub ula: Ula,
    pub keyboard: KeyboardState,
    pub beeper: BeeperState,
    /// Last value written to port $FE (for EAR bit and border).
    pub last_fe_write: u8,
    /// Kempston joystick state: bits 0-4 = right, left, down, up, fire (active-high).
    pub kempston: u8,
    /// AY-3-8910 sound chip (present on 128K/+2/+3 models).
    pub ay: Option<Ay3_8910>,
    /// NEC uPD765 floppy disk controller (present on +3 only).
    pub fdc: Option<Upd765>,
    /// Tape EAR override: `Some(level)` when TZX signal is active, `None`
    /// falls back to MIC loopback (bit 3 of last $FE write).
    pub tape_ear: Option<bool>,
}

impl SpectrumBus {
    #[must_use]
    pub fn new(
        memory: Box<dyn SpectrumMemory>,
        ula: Ula,
        beeper: BeeperState,
    ) -> Self {
        Self {
            memory,
            ula,
            keyboard: KeyboardState::new(),
            beeper,
            last_fe_write: 0,
            kempston: 0,
            ay: None,
            fdc: None,
            tape_ear: None,
        }
    }

    /// Enable the AY sound chip (for 128K/+2/+3 models).
    pub fn enable_ay(&mut self, cpu_frequency: u32, sample_rate: u32) {
        // AY clock is CPU clock / 2 on the Spectrum 128
        self.ay = Some(Ay3_8910::new(cpu_frequency / 2, sample_rate));
    }
}

impl Bus for SpectrumBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr16 = addr as u16;
        let data = self.memory.read(addr16);
        let wait = self.ula.contention(self.memory.contended_page(addr16));

        // Snow: CPU read from display memory during ULA fetch → corrupts ULA's bitmap
        if addr16 >= 0x4000 && addr16 <= 0x5AFF && self.ula.is_screen_fetch_phase() {
            self.ula.set_snow_byte(data);
        }

        ReadResult::with_wait(data, wait)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr16 = addr as u16;
        let wait = self.ula.contention(self.memory.contended_page(addr16));
        self.memory.write(addr16, value);
        wait
    }

    fn io_read(&mut self, addr: u32) -> ReadResult {
        let port = addr as u16;
        let ula_port = port & 0x01 == 0;
        let contended_high = self.memory.contended_page(port);
        let wait = self.ula.io_contention(ula_port, contended_high);

        // Kempston joystick (port $1F, active when low byte = $1F)
        if port & 0xFF == 0x1F {
            return ReadResult::with_wait(self.kempston, wait);
        }

        // Port $2FFD: FDC main status register (+3 only)
        if port & 0xF002 == 0x2000 {
            if let Some(ref fdc) = self.fdc {
                return ReadResult::with_wait(fdc.read_msr(), wait);
            }
        }

        // Port $3FFD: FDC data register read (+3 only)
        if port & 0xF002 == 0x3000 {
            if let Some(ref mut fdc) = self.fdc {
                return ReadResult::with_wait(fdc.read_data(), wait);
            }
        }

        // Port $FE (active when bit 0 is clear)
        let data = if ula_port {
            let addr_high = (port >> 8) as u8;
            let keyboard = self.keyboard.read(addr_high) & 0x1F;
            // Bits 0-4: keyboard, bit 5: always 1, bit 6: EAR input,
            // bit 7: always 1. When a TZX signal is active, EAR comes from
            // the tape; otherwise it reflects MIC output (bit 3 of $FE write).
            let ear = if let Some(level) = self.tape_ear {
                if level { 0x40 } else { 0x00 }
            } else {
                (self.last_fe_write & 0x08) << 3
            };
            keyboard | 0xA0 | ear
        } else if port & 0xC002 == 0xC000 {
            // Port $FFFD: AY register read
            if let Some(ay) = &self.ay {
                ay.read_data()
            } else {
                0xFF
            }
        } else {
            // Non-ULA ports: floating bus leaks ULA data bus
            let mem = &*self.memory;
            self.ula.floating_bus(|a| mem.peek(a))
        };

        ReadResult::with_wait(data, wait)
    }

    fn io_write(&mut self, addr: u32, value: u8) -> u8 {
        let port = addr as u16;
        let ula_port = port & 0x01 == 0;
        let contended_high = self.memory.contended_page(port);
        let wait = self.ula.io_contention(ula_port, contended_high);

        // Port $FE (active when bit 0 is clear)
        if ula_port {
            self.last_fe_write = value;
            // Bit 0-2: border colour
            self.ula.set_border_colour(value & 0x07);
            // Bit 3: MIC output (tape) -- ignored
            // Bit 4: beeper
            self.beeper.set_level((value >> 4) & 1);
        }

        // Port $7FFD: 128K bank switching (bit 1 set, bit 15 clear)
        if port & 0x8002 == 0x0000 && !ula_port {
            self.memory.write_bank_register(value);
        }

        // Port $1FFD: +3 memory/disk banking (bit 12 set, bit 1 clear, not ULA)
        if port & 0xF002 == 0x1000 && !ula_port {
            self.memory.write_plus3_register(value);
        }

        // Port $3FFD: FDC data register write (+3 only)
        if port & 0xF002 == 0x3000 {
            if let Some(ref mut fdc) = self.fdc {
                fdc.write_data(value);
            }
        }

        // Port $FFFD: AY register select
        if port & 0xC002 == 0xC000 && let Some(ay) = &mut self.ay {
            ay.select_register(value);
        }

        // Port $BFFD: AY data write
        if port & 0xC002 == 0x8000 && let Some(ay) = &mut self.ay {
            ay.write_data(value);
        }

        wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory48K;

    fn make_bus() -> SpectrumBus {
        let rom = vec![0u8; 0x4000];
        let memory = Box::new(Memory48K::new(&rom));
        let ula = Ula::new();
        let beeper = BeeperState::new(3_500_000, 48_000);
        SpectrumBus::new(memory, ula, beeper)
    }

    #[test]
    fn memory_read_write() {
        let mut bus = make_bus();
        bus.write(0x8000, 0xAB);
        assert_eq!(bus.read(0x8000).data, 0xAB);
    }

    #[test]
    fn rom_write_ignored() {
        let mut bus = make_bus();
        bus.write(0x0000, 0xFF);
        assert_eq!(bus.read(0x0000).data, 0x00); // ROM was all zeros
    }

    #[test]
    fn keyboard_read_via_io() {
        let mut bus = make_bus();
        // No keys pressed -- all bits high
        let result = bus.io_read(0xFEFE); // Port $FE, scan row 0
        assert_eq!(result.data & 0x1F, 0x1F);

        // Press SHIFT (row 0, bit 0)
        bus.keyboard.set_key(0, 0, true);
        let result = bus.io_read(0xFEFE);
        assert_eq!(result.data & 0x01, 0x00); // Active low
    }

    #[test]
    fn border_and_beeper_via_io() {
        let mut bus = make_bus();
        // Write port $FE: border=2 (red), beeper=1
        bus.io_write(0x00FE, 0x12); // 0b0001_0010: beeper=1, border=010
        assert_eq!(bus.ula.border_colour(), 2);
        assert_eq!(bus.beeper.level(), 1);
    }

    #[test]
    fn unimplemented_port_returns_ff() {
        let mut bus = make_bus();
        let result = bus.io_read(0x00FF); // Odd port, not $FE
        assert_eq!(result.data, 0xFF);
    }

    #[test]
    fn kempston_port_returns_joystick_state() {
        let mut bus = make_bus();
        // No buttons pressed
        let result = bus.io_read(0x001F);
        assert_eq!(result.data, 0x00);

        // Press right (bit 0) and fire (bit 4)
        bus.kempston = 0b0001_0001;
        let result = bus.io_read(0x001F);
        assert_eq!(result.data, 0x11);
    }

    #[test]
    fn tape_ear_overrides_mic_loopback() {
        let mut bus = make_bus();

        // Set MIC bit high — without tape_ear, EAR should reflect MIC
        bus.io_write(0x00FE, 0x08);
        assert_eq!(bus.io_read(0xFEFE).data & 0x40, 0x40, "MIC loopback");

        // Override with tape_ear = Some(false) — EAR should be 0
        bus.tape_ear = Some(false);
        assert_eq!(bus.io_read(0xFEFE).data & 0x40, 0x00, "tape_ear=false overrides MIC");

        // Override with tape_ear = Some(true) — EAR should be 1
        bus.tape_ear = Some(true);
        assert_eq!(bus.io_read(0xFEFE).data & 0x40, 0x40, "tape_ear=true");

        // Remove override — MIC loopback resumes
        bus.tape_ear = None;
        assert_eq!(bus.io_read(0xFEFE).data & 0x40, 0x40, "MIC loopback restored");

        // Clear MIC bit — EAR should now be 0 again
        bus.io_write(0x00FE, 0x00);
        assert_eq!(bus.io_read(0xFEFE).data & 0x40, 0x00, "MIC cleared, no tape_ear");
    }

    #[test]
    fn snow_triggered_by_display_read_during_fetch() {
        let mut bus = make_bus();

        // Write a known value into display memory
        bus.write(0x4000, 0xAB);

        // Position ULA at a screen fetch phase (line 64, T-state 0)
        bus.ula.set_position(64, 0);
        assert!(bus.ula.is_screen_fetch_phase());

        // Read from display memory — should trigger snow
        let result = bus.read(0x4000);
        assert_eq!(result.data, 0xAB);
        assert!(bus.ula.has_snow_byte(), "snow_byte should be set after display read during fetch");
    }

    #[test]
    fn no_snow_outside_fetch_phase() {
        let mut bus = make_bus();
        bus.write(0x4000, 0xAB);

        // Position ULA at idle phase (line 64, T-state 4)
        bus.ula.set_position(64, 4);
        assert!(!bus.ula.is_screen_fetch_phase());

        bus.read(0x4000);
        assert!(!bus.ula.has_snow_byte(), "no snow during idle phase");
    }

    #[test]
    fn no_snow_outside_display_memory() {
        let mut bus = make_bus();

        // Position ULA at fetch phase
        bus.ula.set_position(64, 0);
        assert!(bus.ula.is_screen_fetch_phase());

        // Read from outside display memory ($5B00 = above attribute area)
        bus.read(0x5B00);
        assert!(!bus.ula.has_snow_byte(), "no snow outside $4000-$5AFF");

        // Read from RAM above screen area
        bus.read(0x8000);
        assert!(!bus.ula.has_snow_byte(), "no snow in upper RAM");
    }

    #[test]
    fn ear_reflects_mic_output() {
        let mut bus = make_bus();

        // No write to $FE yet — MIC bit 3 = 0, so EAR bit 6 = 0
        let result = bus.io_read(0xFEFE);
        assert_eq!(result.data & 0x40, 0x00, "EAR should be 0 when MIC is 0");

        // Write to $FE with MIC bit (bit 3) set
        bus.io_write(0x00FE, 0x08);
        let result = bus.io_read(0xFEFE);
        assert_eq!(result.data & 0x40, 0x40, "EAR should be 1 when MIC is 1");

        // Write to $FE with MIC bit clear
        bus.io_write(0x00FE, 0x00);
        let result = bus.io_read(0xFEFE);
        assert_eq!(result.data & 0x40, 0x00, "EAR should be 0 when MIC is 0");
    }
}
