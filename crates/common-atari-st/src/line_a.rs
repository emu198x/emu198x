//! Asking TOS where its line-A variables are.
//!
//! The line-A opcode `$A000` returns the base of the line-A variables, the
//! VDI's own state, in D0 and A0 (*The Atari Compendium*, Scott Sanders,
//! 1992, chapter 8, "Line-A"). The base is not at a fixed address: each TOS
//! build puts the variables where its linker did.
//!
//! [`AtariSt::line_a_base`] asks the running TOS by executing `$A000` on a
//! scratch 68000 that shares this machine's memory and nothing else. The
//! scratch CPU's writes go to an overlay, so the machine is not touched; it
//! reaches no I/O, so a handler that tries to is treated as having failed.

use std::collections::HashMap;

use motorola_68000::Cpu68000;
use motorola_68000::bus::BusStatus;
use motorola_68000::cpu::State;

use crate::AtariSt;

/// The line-A exception vector.
const LINE_A_VECTOR: u32 = 0x28;
/// The opcode that returns the variables' base.
const LINE_A_INIT: u16 = 0xA000;
/// How far below the supervisor stack the scratch opcode goes, clear of
/// the frame the exception pushes.
const CODE_BELOW_STACK: u32 = 0x400;
/// CPU cycles the handler may take before the call is given up. TOS's and
/// EmuTOS's take a few hundred.
const MAX_CYCLES: u32 = 20_000;

impl AtariSt {
    /// The base of TOS's line-A variables, as `$A000` returns it, or `None`
    /// when TOS has not yet installed its line-A handler or the call did not
    /// come back.
    ///
    /// The call runs on a scratch CPU against an overlay of this machine's
    /// memory, so nothing here changes. It costs a few hundred CPU cycles of
    /// host work.
    #[must_use]
    pub fn line_a_base(&self) -> Option<u32> {
        let vector = self.peek_long(LINE_A_VECTOR);
        if !self.is_rom_address(vector) && !self.is_ram_address(vector) {
            return None;
        }
        let ssp = self.cpu.regs.ssp & !1;
        let code = ssp.checked_sub(CODE_BELOW_STACK)?;
        if code < 0x800 || !self.is_ram_address(ssp.wrapping_sub(2)) {
            return None;
        }
        let mut scratch = Scratch {
            machine: self,
            overlay: HashMap::new(),
        };
        scratch.write_word(code, LINE_A_INIT);
        let mut cpu = Cpu68000::new();
        cpu.reset_to(ssp, code);
        let resume = code + 2;
        let mut starts = cpu.instruction_starts;
        for _ in 0..MAX_CYCLES {
            scratch.service(&mut cpu)?;
            cpu.tick();
            if cpu.is_halted() {
                return None;
            }
            if cpu.instruction_starts != starts {
                starts = cpu.instruction_starts;
                if cpu.instr_start_pc == resume {
                    let (d0, a0) = (cpu.regs.d[0], cpu.regs.a[0]);
                    return (d0 == a0 && self.is_ram_address(a0)).then_some(a0);
                }
            }
        }
        None
    }

    /// A big-endian long as the CPU would read it in supervisor mode.
    fn peek_long(&self, address: u32) -> u32 {
        (0..4).fold(0, |long, i| long << 8 | u32::from(self.peek(address + i)))
    }

    fn is_ram_address(&self, address: u32) -> bool {
        let address = address & 0x00FF_FFFF;
        (8..0x40_0000).contains(&address) && self.glue.translate(address).is_some()
    }

    fn is_rom_address(&self, address: u32) -> bool {
        let address = address & 0x00FF_FFFF;
        address
            .checked_sub(self.rom_base)
            .is_some_and(|offset| (offset as usize) < self.rom.len())
    }
}

/// The machine's memory as the scratch CPU sees it: RAM and ROM read
/// through, writes kept aside.
struct Scratch<'a> {
    machine: &'a AtariSt,
    overlay: HashMap<u32, u8>,
}

impl Scratch<'_> {
    fn read_byte(&self, address: u32) -> Option<u8> {
        if let Some(&byte) = self.overlay.get(&address) {
            return Some(byte);
        }
        (address < 8
            || self.machine.is_ram_address(address)
            || self.machine.is_rom_address(address))
        .then(|| self.machine.peek(address))
    }

    fn write_byte(&mut self, address: u32, value: u8) -> Option<()> {
        self.machine.is_ram_address(address).then(|| {
            self.overlay.insert(address, value);
        })
    }

    fn write_word(&mut self, address: u32, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.overlay.insert(address, hi);
        self.overlay.insert(address + 1, lo);
    }

    /// Answer the scratch CPU's bus cycle, if it is in one. `None` when it
    /// reaches outside RAM and ROM.
    fn service(&mut self, cpu: &mut Cpu68000) -> Option<()> {
        let State::BusCycle {
            addr,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = cpu.state.clone()
        else {
            cpu.bus_status = BusStatus::Wait;
            return Some(());
        };
        if cycle_count < 3 {
            cpu.bus_status = BusStatus::Wait;
            return Some(());
        }
        let address = addr & 0x00FF_FFFF;
        cpu.bus_status = if is_read {
            let word = if is_word {
                let even = address & !1;
                u16::from(self.read_byte(even)?) << 8 | u16::from(self.read_byte(even | 1)?)
            } else {
                u16::from(self.read_byte(address)?)
            };
            BusStatus::Ready(word)
        } else {
            let value = data.unwrap_or(0);
            if is_word {
                let even = address & !1;
                self.write_byte(even, (value >> 8) as u8)?;
                self.write_byte(even | 1, value as u8)?;
            } else {
                self.write_byte(address, value as u8)?;
            }
            BusStatus::Ready(0)
        };
        Some(())
    }
}
