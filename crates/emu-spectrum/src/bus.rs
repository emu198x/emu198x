//! Spectrum bus: memory and I/O routing.
//!
//! The bus connects the Z80 CPU to memory, video, keyboard, and beeper.
//! I/O routing is model-aware: v1 implements port $FE only. Future models
//! will add $7FFD (128K banking), $FFFD/$BFFD (AY audio), $FF (Timex SCLD),
//! etc.
//!
//! # Contention
//!
//! Memory contention is delegated to the video chip via `video.contention()`.
//! The bus adds the returned wait states to the `ReadResult`. I/O contention
//! is similarly delegated via `video.io_contention()`.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};

use crate::beeper::BeeperState;
use crate::keyboard::KeyboardState;
use crate::memory::SpectrumMemory;
use crate::video::SpectrumVideo;

/// The Spectrum bus, implementing `emu_core::Bus`.
///
/// Owns the memory, video, keyboard, and beeper subsystems. The CPU
/// accesses all of these through the `Bus` trait.
pub struct SpectrumBus {
    pub memory: Box<dyn SpectrumMemory>,
    pub video: Box<dyn SpectrumVideo>,
    pub keyboard: KeyboardState,
    pub beeper: BeeperState,
    /// Last value written to port $FE (for EAR bit and border).
    pub last_fe_write: u8,
}

impl SpectrumBus {
    #[must_use]
    pub fn new(
        memory: Box<dyn SpectrumMemory>,
        video: Box<dyn SpectrumVideo>,
        beeper: BeeperState,
    ) -> Self {
        Self {
            memory,
            video,
            keyboard: KeyboardState::new(),
            beeper,
            last_fe_write: 0,
        }
    }
}

impl Bus for SpectrumBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr16 = addr as u16;
        let data = self.memory.read(addr16);
        let wait = self.video.contention(addr16, &*self.memory);
        ReadResult::with_wait(data, wait)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr16 = addr as u16;
        let wait = self.video.contention(addr16, &*self.memory);
        self.memory.write(addr16, value);
        wait
    }

    fn io_read(&mut self, addr: u32) -> ReadResult {
        let port = addr as u16;
        let wait = self.video.io_contention(port, &*self.memory);

        // Port $FE (active when bit 0 is clear)
        let data = if port & 0x01 == 0 {
            let addr_high = (port >> 8) as u8;
            let keyboard = self.keyboard.read(addr_high);
            // Bits 0-4: keyboard, bit 5: unused (1), bit 6: EAR input (tape),
            // bit 7: unused (1). For now, EAR returns 1 (no tape).
            keyboard | 0xC0
        } else {
            // Non-ULA ports: floating bus leaks ULA data bus
            self.video.floating_bus(&*self.memory)
        };

        ReadResult::with_wait(data, wait)
    }

    fn io_write(&mut self, addr: u32, value: u8) -> u8 {
        let port = addr as u16;
        let wait = self.video.io_contention(port, &*self.memory);

        // Port $FE (active when bit 0 is clear)
        if port & 0x01 == 0 {
            self.last_fe_write = value;
            // Bit 0-2: border colour
            self.video.set_border_colour(value & 0x07);
            // Bit 3: MIC output (tape) — ignored
            // Bit 4: beeper
            self.beeper.set_level((value >> 4) & 1);
        }

        // Other ports silently ignored in v1

        wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory48K;
    use crate::ula::Ula;

    fn make_bus() -> SpectrumBus {
        let rom = vec![0u8; 0x4000];
        let memory = Box::new(Memory48K::new(&rom));
        let video = Box::new(Ula::new());
        let beeper = BeeperState::new(3_500_000, 48_000);
        SpectrumBus::new(memory, video, beeper)
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
        // No keys pressed — all bits high
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
        assert_eq!(bus.video.border_colour(), 2);
        assert_eq!(bus.beeper.level(), 1);
    }

    #[test]
    fn unimplemented_port_returns_ff() {
        let mut bus = make_bus();
        let result = bus.io_read(0x00FF); // Odd port, not $FE
        assert_eq!(result.data, 0xFF);
    }
}
