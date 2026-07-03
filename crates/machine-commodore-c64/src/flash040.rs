//! AM29F040B flash chip (the EasyFlash pair).
//!
//! Ported from VICE's `flash040core.c` (AM29F040B variant only): the JEDEC
//! command state machine driven by writes — unlock at `$555`/`$2AA` (11-bit
//! address decode), then autoselect (`$90`), byte program (`$A0`), erase
//! (`$80` + `$10` chip / `$30` sector) — plus the status reads software
//! polls while an erase runs. Programming can only clear bits (`old & new`);
//! erasing sets a 64 KiB sector (or the chip) to `$FF`.
//!
//! VICE schedules erase completion on alarms; here the same delays are
//! counted down by [`Flash040::tick`], which the board calls every phi2
//! cycle. Real parts take seconds; software (the EasyFlash EAPI) polls the
//! DQ7/DQ6 status protocol until completion, so the exact figure only needs
//! to be long enough to be observable — VICE's cycle counts are kept.

use std::cell::Cell;

use serde::{Deserialize, Serialize};

/// 512 KiB per chip.
pub(crate) const FLASH_SIZE: usize = 0x8_0000;

const MANUFACTURER_ID: u8 = 0x01;
const DEVICE_ID: u8 = 0xA4;
/// 64 KiB erase sectors (8 per chip).
const SECTOR_SIZE: usize = 0x1_0000;
const SECTOR_SHIFT: u32 = 16;
/// AM29F040B unlock addresses, decoded on the low 11 address lines.
const MAGIC_1_ADDR: u32 = 0x555;
const MAGIC_2_ADDR: u32 = 0x2AA;
const MAGIC_MASK: u32 = 0x7FF;
/// DQ6 toggles on erase-status reads.
const STATUS_TOGGLE_BITS: u8 = 0x40;
/// VICE's AM29F040B erase timings, in phi2 cycles.
const ERASE_SECTOR_TIMEOUT_CYCLES: u32 = 50;
const ERASE_SECTOR_CYCLES: u32 = 1_000_000;
const ERASE_CHIP_CYCLES: u32 = 8_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum State {
    Read,
    Magic1,
    Magic2,
    Autoselect,
    ByteProgram,
    ByteProgramError,
    EraseMagic1,
    EraseMagic2,
    EraseSelect,
    SectorEraseTimeout,
    SectorErase,
    SectorEraseSuspend,
    ChipErase,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Flash040 {
    data: Vec<u8>,
    state: State,
    base_state: State,
    /// Last programmed byte / erase-status shift register. `Cell` because
    /// erase-status *reads* toggle DQ6 in it (VICE does the same through a
    /// mutable context), and the board's read path is `&self`.
    program_byte: Cell<u8>,
    /// One bit per 64 KiB sector queued for erase.
    erase_mask: u8,
    /// Cycles until the pending erase step completes.
    erase_countdown: u32,
}

impl Flash040 {
    /// Wraps a 512 KiB image (shorter input is padded with `$FF` — erased).
    pub(crate) fn new(mut data: Vec<u8>) -> Self {
        data.resize(FLASH_SIZE, 0xFF);
        Self {
            data,
            state: State::Read,
            base_state: State::Read,
            program_byte: Cell::new(0),
            erase_mask: 0,
            erase_countdown: 0,
        }
    }

    #[must_use]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// True while the chip is out of plain read-array mode (a command or
    /// erase is in flight), i.e. reads return status, not data.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn busy(&self) -> bool {
        self.state != State::Read
    }

    fn magic_1(addr: u32) -> bool {
        addr & MAGIC_MASK == MAGIC_1_ADDR
    }

    fn magic_2(addr: u32) -> bool {
        addr & MAGIC_MASK == MAGIC_2_ADDR
    }

    fn erase_sector(&mut self, sector: usize) {
        let start = sector * SECTOR_SIZE;
        self.data[start..start + SECTOR_SIZE].fill(0xFF);
    }

    /// DQ7 = complement of the programmed bit 7, DQ5 = timeout flag. (VICE
    /// also toggles DQ6 from the CPU clock; the polling protocol only needs
    /// DQ7 here.)
    fn write_operation_status(&self) -> u8 {
        ((self.program_byte.get() ^ 0x80) & 0x80) | (1 << 5)
    }

    /// Erase status: DQ6 toggles per read, DQ3 set once the sector-erase
    /// timeout window has closed.
    fn erase_operation_status(&self) -> u8 {
        let value = self.program_byte.get();
        self.program_byte.set(value ^ STATUS_TOGGLE_BITS);
        if self.state == State::SectorEraseTimeout {
            value
        } else {
            value | 0x08
        }
    }

    /// One phi2 cycle: advance any pending erase.
    pub(crate) fn tick(&mut self) {
        if self.erase_countdown == 0 {
            return;
        }
        self.erase_countdown -= 1;
        if self.erase_countdown > 0 {
            return;
        }
        match self.state {
            State::SectorEraseTimeout => {
                self.state = State::SectorErase;
                self.erase_countdown = ERASE_SECTOR_CYCLES;
            }
            State::SectorErase => {
                // Erase one queued sector per period, like VICE's alarm.
                for sector in 0..8 {
                    if self.erase_mask & (1 << sector) != 0 {
                        self.erase_sector(sector);
                        self.erase_mask &= !(1 << sector);
                        break;
                    }
                }
                if self.erase_mask != 0 {
                    self.erase_countdown = ERASE_SECTOR_CYCLES;
                } else {
                    self.state = self.base_state;
                }
            }
            State::ChipErase => {
                self.data.fill(0xFF);
                self.state = self.base_state;
            }
            _ => {}
        }
    }

    pub(crate) fn read(&self, addr: u32) -> u8 {
        let addr = addr as usize & (FLASH_SIZE - 1);
        match self.state {
            State::Autoselect => match addr & 0xFF {
                0x00 => MANUFACTURER_ID,
                0x01 => DEVICE_ID,
                0x02 => 0,
                _ => self.data[addr],
            },
            State::ByteProgramError => self.write_operation_status(),
            State::SectorEraseSuspend
            | State::ChipErase
            | State::SectorErase
            | State::SectorEraseTimeout => self.erase_operation_status(),
            // A read during a command sequence does not reset the state.
            _ => self.data[addr],
        }
    }

    pub(crate) fn store(&mut self, addr: u32, byte: u8) {
        let addr = addr & (FLASH_SIZE as u32 - 1);
        match self.state {
            State::Read => {
                if Self::magic_1(addr) && byte == 0xAA {
                    self.state = State::Magic1;
                }
            }
            State::Magic1 => {
                if Self::magic_2(addr) && byte == 0x55 {
                    self.state = State::Magic2;
                } else {
                    self.state = self.base_state;
                }
            }
            State::Magic2 => {
                if Self::magic_1(addr) {
                    match byte {
                        0x90 => {
                            self.state = State::Autoselect;
                            self.base_state = State::Autoselect;
                        }
                        0xF0 => {
                            self.state = State::Read;
                            self.base_state = State::Read;
                        }
                        0xA0 => self.state = State::ByteProgram,
                        0x80 => self.state = State::EraseMagic1,
                        _ => self.state = self.base_state,
                    }
                } else {
                    self.state = self.base_state;
                }
            }
            State::ByteProgram => {
                // Programming can only clear bits.
                let index = addr as usize;
                let new = self.data[index] & byte;
                self.data[index] = new;
                self.program_byte.set(byte);
                self.state = if new == byte {
                    self.base_state
                } else {
                    State::ByteProgramError
                };
            }
            State::EraseMagic1 => {
                if Self::magic_1(addr) && byte == 0xAA {
                    self.state = State::EraseMagic2;
                } else {
                    self.state = self.base_state;
                }
            }
            State::EraseMagic2 => {
                if Self::magic_2(addr) && byte == 0x55 {
                    self.state = State::EraseSelect;
                } else {
                    self.state = self.base_state;
                }
            }
            State::EraseSelect => {
                if Self::magic_1(addr) && byte == 0x10 {
                    self.state = State::ChipErase;
                    self.program_byte.set(0);
                    self.erase_countdown = ERASE_CHIP_CYCLES;
                } else if byte == 0x30 {
                    self.erase_mask |= 1 << (addr >> SECTOR_SHIFT);
                    self.program_byte.set(0);
                    self.state = State::SectorEraseTimeout;
                    self.erase_countdown = ERASE_SECTOR_TIMEOUT_CYCLES;
                } else {
                    self.state = self.base_state;
                }
            }
            State::SectorEraseTimeout => {
                if byte == 0x30 {
                    self.erase_mask |= 1 << (addr >> SECTOR_SHIFT);
                } else {
                    self.state = self.base_state;
                    self.erase_mask = 0;
                    self.erase_countdown = 0;
                }
            }
            State::SectorErase => {
                if byte == 0xB0 {
                    self.state = State::SectorEraseSuspend;
                    self.erase_countdown = 0;
                }
            }
            State::SectorEraseSuspend => {
                if byte == 0x30 {
                    self.state = State::SectorErase;
                    self.erase_countdown = ERASE_SECTOR_CYCLES;
                }
            }
            State::ByteProgramError | State::Autoselect => {
                if Self::magic_1(addr) && byte == 0xAA {
                    self.state = State::Magic1;
                }
                if byte == 0xF0 {
                    self.state = State::Read;
                    self.base_state = State::Read;
                }
            }
            State::ChipErase => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erased() -> Flash040 {
        Flash040::new(Vec::new())
    }

    fn unlock(flash: &mut Flash040) {
        flash.store(0x555, 0xAA);
        flash.store(0x2AA, 0x55);
    }

    #[test]
    fn autoselect_reports_amd_29f040_ids() {
        let mut flash = erased();
        unlock(&mut flash);
        flash.store(0x555, 0x90);
        assert_eq!(flash.read(0x00), 0x01);
        assert_eq!(flash.read(0x01), 0xA4);
        // $F0 returns to read-array mode.
        flash.store(0x000, 0xF0);
        assert_eq!(flash.read(0x00), 0xFF);
    }

    #[test]
    fn program_clears_bits_only() {
        let mut flash = erased();
        unlock(&mut flash);
        flash.store(0x555, 0xA0);
        flash.store(0x1234, 0x5A);
        assert_eq!(flash.read(0x1234), 0x5A);
        // A second program can only clear further bits; asking it to set
        // one ($F0 over $5A) leaves the AND result and flags a program
        // error until a $F0 reset returns the chip to read-array mode.
        unlock(&mut flash);
        flash.store(0x555, 0xA0);
        flash.store(0x1234, 0xF0);
        assert!(flash.busy());
        flash.store(0x1234, 0xF0); // reset command
        assert_eq!(flash.read(0x1234), 0x50);
    }

    #[test]
    fn sector_erase_completes_after_delay_and_reports_status() {
        let mut flash = erased();
        unlock(&mut flash);
        flash.store(0x555, 0xA0);
        flash.store(0x2_0000, 0x00); // program a byte in sector 2
        assert_eq!(flash.read(0x2_0000), 0x00);

        unlock(&mut flash);
        flash.store(0x555, 0x80);
        unlock(&mut flash);
        flash.store(0x2_0000, 0x30); // erase sector 2
        assert!(flash.busy());

        // DQ6 toggles while the erase runs.
        let a = flash.read(0x2_0000);
        let b = flash.read(0x2_0000);
        assert_ne!(a & 0x40, b & 0x40);

        for _ in 0..(ERASE_SECTOR_TIMEOUT_CYCLES + ERASE_SECTOR_CYCLES) {
            flash.tick();
        }
        assert!(!flash.busy());
        assert_eq!(flash.read(0x2_0000), 0xFF);
    }

    #[test]
    fn chip_erase_blanks_everything() {
        let mut flash = Flash040::new(vec![0x00; FLASH_SIZE]);
        unlock(&mut flash);
        flash.store(0x555, 0x80);
        unlock(&mut flash);
        flash.store(0x555, 0x10);
        for _ in 0..ERASE_CHIP_CYCLES {
            flash.tick();
        }
        assert_eq!(flash.read(0x7_FFFF), 0xFF);
        assert!(!flash.busy());
    }
}
