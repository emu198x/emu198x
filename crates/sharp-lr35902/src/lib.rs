//! Sharp LR35902 (SM83) CPU — the Game Boy's system-on-chip core.
//!
//! M-cycle granular per
//! [`wiki/decisions/sm83-abstraction-level.md`](../../../wiki/decisions/sm83-abstraction-level.md):
//! one [`Sm83::tick`] advances one machine cycle (4 T-cycles). Pin-level
//! per [`wiki/decisions/cpu-bus-interface.md`](../../../wiki/decisions/cpu-bus-interface.md):
//! the CPU exposes its bus state as public fields and the machine
//! performs the read or write between ticks.
//!
//! Ported from the Zig reference at `~/Projects/Emu198x-Zig/src/sm83.zig`,
//! restructured to the pipelined pin-level convention.

mod alu;
mod cb;
mod flags;
mod opcodes;
mod reg;

pub use flags::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

use serde::{Deserialize, Serialize};

/// The Sharp LR35902 (SM83) CPU.
///
/// Registers, flags, internal instruction state, and the pin interface
/// are all public so the machine layer can inspect everything between
/// ticks without going through accessors.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sm83 {
    // -- Registers ----------------------------------------------------
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,

    // -- Internal instruction state ----------------------------------
    /// Latched opcode for the in-progress instruction. `m_cycle == 0`
    /// means "ready to fetch a new opcode on the next tick".
    pub opcode: u8,
    /// Sub-cycle within the current instruction. 0 = opcode-fetch
    /// boundary; 1..=N = subsequent m-cycles of that opcode.
    pub m_cycle: u8,
    /// Internal scratch low byte — the SM83 analogue of the Z80 MEMPTR
    /// pair. Holds the low half of an immediate / address / popped word.
    pub z: u8,
    /// Internal scratch high byte; pairs with `z` to form the 16-bit
    /// `WZ` value used by absolute addressing, CALL, and POP-style ops.
    pub w: u8,

    /// Interrupt master enable. Gates dispatch.
    pub ime: bool,
    /// `EI` sets this; on the next opcode boundary (m_cycle 0), this
    /// promotes to `ime`. That's the documented one-instruction delay
    /// between `EI` and interrupts becoming visible.
    pub ime_pending: bool,

    /// `HALT` suspends the CPU until an interrupt is pending. The CPU
    /// keeps ticking but performs no bus operations and the PC stays
    /// put.
    pub halt_mode: bool,
    /// `STOP` halts the entire SoC (CPU + LCD) until a button-press
    /// wakes it. Modelled as a sticky flag for now; the machine layer
    /// resets it when the joypad pin transitions.
    pub stopped: bool,
    /// Set when the CPU executes an opcode we haven't implemented yet
    /// (originally a Zig-side diagnostic). Real hardware has no
    /// unimplemented opcodes — this is a porting safety net we'll
    /// remove once the table is complete.
    pub diag_unimplemented: bool,

    /// HALT-bug latch: when `HALT` is executed with `IME=0` and an
    /// interrupt is already pending, the next opcode is fetched twice
    /// (the PC fails to increment for the duplicate). The machine
    /// observes this as a single redundant byte read, which Blargg's
    /// `cpu_instrs` exercises.
    pub halt_bug: bool,

    /// True while the CPU is in the middle of a 5-m-cycle interrupt
    /// dispatch sequence. Mutually exclusive with normal opcode
    /// execution; entered at an instruction boundary when
    /// `ime && irq_pending != 0`, exited after the vector is loaded
    /// into PC.
    pub dispatching: bool,

    // -- Output pins (CPU → machine) ---------------------------------
    /// Address bus driven for the current m-cycle's bus operation.
    pub addr: u16,
    /// Data bus driven on writes (`wr == true`). Indeterminate on reads.
    pub data: u8,
    /// Read strobe — high if this m-cycle is a memory read.
    pub rd: bool,
    /// Write strobe — high if this m-cycle is a memory write.
    pub wr: bool,
    /// Memory request — high for any externally visible bus m-cycle
    /// (any read or write). Low for "internal" m-cycles where the bus
    /// is idle (e.g. ADD SP, r8's compute step or PUSH's SP-decrement
    /// step).
    pub mreq: bool,

    /// Asserted on the m-cycle when an interrupt is being serviced.
    /// `int_ack_bit` carries the IF/IE bit number (0..4) that was
    /// dispatched. The machine clears that bit from `$FF0F` when it
    /// sees the strobe.
    pub int_ack: bool,
    pub int_ack_bit: u8,

    // -- Input pins (machine → CPU) ----------------------------------
    /// Data bus value latched by the machine after the previous tick's
    /// scheduled read. Consumed by the next tick.
    pub data_in: u8,
    /// Pending interrupt mask: `IF & IE & 0x1F`. The machine refreshes
    /// this between ticks. The CPU dispatches the lowest set bit when
    /// `IME` is true and `irq_pending != 0` at an opcode boundary.
    pub irq_pending: u8,
    /// Interrupt mask latched during the dispatch sequence, after the
    /// PC-high push has been externally serviced. Later IE changes
    /// during the PC-low push are too late to affect the vector.
    #[serde(default)]
    pub irq_dispatch_mask: u8,
}

/// CPU register state produced by a Game Boy boot ROM before it
/// jumps to cartridge entry at `$0100`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PostBootCpuState {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl PostBootCpuState {
    /// DMG boot ROM v1 and later, matching Pan Docs' common skipped
    /// boot values.
    pub const DMG_ABC: Self = Self {
        a: 0x01,
        f: 0xB0,
        b: 0x00,
        c: 0x13,
        d: 0x00,
        e: 0xD8,
        h: 0x01,
        l: 0x4D,
        sp: 0xFFFE,
        pc: 0x0100,
    };

    /// Original DMG0 boot ROM exit state.
    pub const DMG0: Self = Self {
        a: 0x01,
        f: 0x00,
        b: 0xFF,
        c: 0x13,
        d: 0x00,
        e: 0xC1,
        h: 0x84,
        l: 0x03,
        sp: 0xFFFE,
        pc: 0x0100,
    };

    /// Game Boy Pocket boot ROM exit state.
    pub const MGB: Self = Self {
        a: 0xFF,
        f: 0xB0,
        b: 0x00,
        c: 0x13,
        d: 0x00,
        e: 0xD8,
        h: 0x01,
        l: 0x4D,
        sp: 0xFFFE,
        pc: 0x0100,
    };

    /// Super Game Boy boot ROM exit state.
    pub const SGB: Self = Self {
        a: 0x01,
        f: 0x00,
        b: 0x00,
        c: 0x14,
        d: 0x00,
        e: 0x00,
        h: 0xC0,
        l: 0x60,
        sp: 0xFFFE,
        pc: 0x0100,
    };

    /// Super Game Boy 2 boot ROM exit state.
    pub const SGB2: Self = Self {
        a: 0xFF,
        f: 0x00,
        b: 0x00,
        c: 0x14,
        d: 0x00,
        e: 0x00,
        h: 0xC0,
        l: 0x60,
        sp: 0xFFFE,
        pc: 0x0100,
    };
}

impl Sm83 {
    /// Creates a CPU in the post-power-on default state — every
    /// register zero, pins idle, no scheduled bus operation. Call
    /// [`reset_post_bootrom`](Sm83::reset_post_bootrom) for the
    /// documented post-boot-ROM register state if you're skipping the
    /// boot ROM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0,

            opcode: 0,
            m_cycle: 0,
            z: 0,
            w: 0,
            ime: false,
            ime_pending: false,
            halt_mode: false,
            stopped: false,
            diag_unimplemented: false,
            halt_bug: false,
            dispatching: false,

            addr: 0,
            data: 0,
            rd: false,
            wr: false,
            mreq: false,
            int_ack: false,
            int_ack_bit: 0,

            data_in: 0,
            irq_pending: 0,
            irq_dispatch_mask: 0,
        }
    }

    /// Primes the CPU for booting from `$0000` with all registers zero
    /// (the actual boot-ROM entry state). Schedules the first opcode
    /// fetch at PC=$0000 so the next tick consumes it.
    pub fn reset(&mut self) {
        *self = Self::new();
        self.schedule_opcode_fetch(self.pc);
    }

    /// Primes the CPU for `$0100` entry with the documented
    /// post-boot-ROM register state for the DMG. Use this when the
    /// machine is skipping the boot ROM (e.g. for cartridge tests where
    /// no boot ROM is present).
    ///
    /// Values per Pan Docs §15.7: `A=$01`, `F=$B0`, `B=$00`, `C=$13`,
    /// `D=$00`, `E=$D8`, `H=$01`, `L=$4D`, `SP=$FFFE`, `PC=$0100`.
    pub fn reset_post_bootrom(&mut self) {
        self.reset_post_bootrom_with_state(PostBootCpuState::DMG_ABC);
    }

    /// Primes the CPU for `$0100` entry with a caller-supplied boot
    /// ROM exit state.
    pub fn reset_post_bootrom_with_state(&mut self, state: PostBootCpuState) {
        *self = Self::new();
        self.a = state.a;
        self.f = state.f;
        self.b = state.b;
        self.c = state.c;
        self.d = state.d;
        self.e = state.e;
        self.h = state.h;
        self.l = state.l;
        self.sp = state.sp;
        self.pc = state.pc;
        self.schedule_opcode_fetch(self.pc);
    }

    /// True at instruction boundaries — the next tick will fetch a new
    /// opcode rather than continue an in-progress one.
    #[must_use]
    pub const fn instruction_complete(&self) -> bool {
        self.m_cycle == 0
    }

    // -- Pin scheduling helpers --------------------------------------

    /// Drive an opcode-fetch m-cycle on the bus: `mreq=rd=true`,
    /// `addr=pc`. The machine populates `data_in` before the next
    /// tick.
    ///
    /// `int_ack` is a one-shot pulse owned by the dispatch path and
    /// cleared at the start of each [`tick`](Self::tick); the
    /// schedule helpers deliberately don't touch it so an
    /// interrupt-acknowledge can ride along with whatever bus op the
    /// dispatch chooses for the same m-cycle.
    #[inline]
    pub(crate) fn schedule_opcode_fetch(&mut self, pc: u16) {
        self.addr = pc;
        self.rd = true;
        self.wr = false;
        self.mreq = true;
    }

    /// Drive a memory read m-cycle on the bus.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn schedule_read(&mut self, addr: u16) {
        self.addr = addr;
        self.rd = true;
        self.wr = false;
        self.mreq = true;
    }

    /// Drive a memory write m-cycle on the bus.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn schedule_write(&mut self, addr: u16, data: u8) {
        self.addr = addr;
        self.data = data;
        self.rd = false;
        self.wr = true;
        self.mreq = true;
    }

    /// Drive an internal (bus-idle) m-cycle. Pins go quiet; the next
    /// tick will set them up for whatever the instruction needs.
    #[inline]
    pub(crate) fn schedule_internal(&mut self) {
        self.rd = false;
        self.wr = false;
        self.mreq = false;
    }
}

impl Default for Sm83 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_state_is_all_zero_and_pins_idle() {
        let cpu = Sm83::new();
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.sp, 0);
        assert!(!cpu.rd);
        assert!(!cpu.wr);
        assert!(!cpu.mreq);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn reset_primes_opcode_fetch_at_zero() {
        let mut cpu = Sm83::new();
        cpu.reset();
        assert_eq!(cpu.addr, 0x0000);
        assert!(cpu.rd);
        assert!(!cpu.wr);
        assert!(cpu.mreq);
        assert_eq!(cpu.pc, 0x0000);
        assert!(cpu.instruction_complete());
    }

    #[test]
    fn reset_post_bootrom_sets_dmg_state_and_primes_pc_0100() {
        let mut cpu = Sm83::new();
        cpu.reset_post_bootrom();
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.f, 0xB0);
        assert_eq!(cpu.bc(), 0x0013);
        assert_eq!(cpu.de(), 0x00D8);
        assert_eq!(cpu.hl(), 0x014D);
        assert_eq!(cpu.sp, 0xFFFE);
        assert_eq!(cpu.pc, 0x0100);
        assert_eq!(cpu.addr, 0x0100);
        assert!(cpu.rd);
        assert!(cpu.mreq);
    }
}
